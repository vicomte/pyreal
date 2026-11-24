//! Inventory, appraisal and vendors.
//!
//! The 1999 appraisal is fixed sections behind one flag word; the later one is property tables
//! keyed by the same property numbers, so each section lands in a table.

use pyreal_primitives::ObjectId;
use pyreal_protocol::archive::PackedHash;
use pyreal_protocol::objects::ItemSetAppraiseInfo;
use pyreal_protocol::trade::{ItemProfile, VendorInfo, VendorProfile};
use pyreal_protocol::types::appraisal::{
    flags as later, AppraisalProfile, ArmorProfile, CreatureAppraisalProfile, WeaponProfile,
};
use pyreal_protocol::types::PublicWeenieDesc;
use pyreal_protocol::{MessageError, Reader};

use super::codec::read_string;
use super::magic::read;
use super::{Handled, Later};

/// `0x00C9` appraisal.
pub const APPRAISAL: u32 = 0x00C9;
/// `0x0062` vendor list.
pub const VENDOR: u32 = 0x0062;
/// Contain, wear, remove, stop viewing, view contents, move, use done, attempt failed: bodies with
/// no strings.
const SAME: [u32; 8] = [
    0x0022, 0x0023, 0x0024, 0x0052, 0x0196, 0x019A, 0x01C7, 0x00A0,
];

/// A word of all ones is an absent value.
const ABSENT: u32 = u32::MAX;
/// The table size hint of the appraisal tables.
const TABLE_SIZE: u32 = 16;

/// Reads a 1999 public object description; [`crate::objects::read_description`] by default.
pub type DescriptionReader = fn(&mut Reader<'_>) -> Result<PublicWeenieDesc, MessageError>;

pub(super) fn translate(
    opcode: u32,
    message: &[u8],
    description: DescriptionReader,
) -> Option<Result<Handled, MessageError>> {
    let out = match opcode {
        APPRAISAL => read(message, appraisal).and_then(|a| Later::of(&a)),
        VENDOR => read(message, |r| vendor(r, description)).and_then(|v| Later::of(&v)),
        op if SAME.contains(&op) => Ok(Later::same(op, message)),
        _ => return None,
    };
    Some(out.map(Handled::from))
}

/// The 1999 appraisal sections, in their fixed order, as later tables and profiles.
#[allow(clippy::too_many_lines)] // one block per section, in wire order
fn appraisal(r: &mut Reader<'_>) -> Result<ItemSetAppraiseInfo, MessageError> {
    let object = ObjectId(r.u32()?);
    let flags = r.u32()?;
    let success_flag = r.u32()?;
    let has = |b: u32| flags & b != 0;
    let mut t = Tables::default();
    let mut p = AppraisalProfile {
        success_flag,
        ..AppraisalProfile::default()
    };

    if has(0x1) {
        let sub = r.u32()?;
        t.int(25, r.u32()?);
        let health = r.u32()?;
        let max_health = r.u32()?;
        let mut c = CreatureAppraisalProfile {
            flags: sub
                & (CreatureAppraisalProfile::HAS_ATTRIBUTES
                    | CreatureAppraisalProfile::HAS_ENCHANTMENTS),
            health,
            max_health,
            ..CreatureAppraisalProfile::default()
        };
        if sub & 0x8 != 0 {
            let mut a = [0u32; 10];
            for v in &mut a {
                *v = r.u32()?;
            }
            c.attributes = Some(a);
        }
        if sub & 0x4 != 0 {
            t.int(2, r.u32()?);
        }
        if sub & 0x2 != 0 {
            t.int(30, r.u32()?);
            t.int(35, r.u32()?);
            for _ in 0..3 {
                r.u32()?;
            }
            for key in [3, 4, 5] {
                t.string(key, read_string(r)?);
            }
            // The fellowship and monarch names, then the face, have no later place.
            read_string(r)?;
            read_string(r)?;
            for _ in 0..6 {
                r.u32()?;
            }
        }
        if sub & 0x1 != 0 {
            c.enchantment_bitfield = Some(r.u32()?);
        }
        p.creature_profile = Some(c);
    }
    if has(0x2) {
        let damage_type = r.u32()?;
        let weapon_time = r.i32()?;
        let weapon_skill = r.u32()?;
        let weapon_damage = r.i32()?;
        let damage_variance = r.f64()?;
        let weapon_length = r.f64()?;
        let max_velocity = r.f64()?;
        // The later profile has no defence modifier; it is the Float the later panel reads.
        t.float(29, r.f64()?);
        p.weapon_profile = Some(WeaponProfile {
            damage_type,
            weapon_time,
            weapon_skill,
            weapon_damage,
            damage_variance,
            damage_mod: 1.0,
            weapon_length,
            max_velocity,
            weapon_offense: r.f64()?,
            max_velocity_estimated: 0,
        });
    }
    if has(0x4) {
        let mut m = [0f32; 7];
        for v in &mut m {
            *v = r.f32()?;
        }
        p.armor_profile = Some(ArmorProfile {
            mod_vs_slash: m[0],
            mod_vs_pierce: m[1],
            mod_vs_bludgeon: m[2],
            mod_vs_cold: m[3],
            mod_vs_fire: m[4],
            mod_vs_acid: m[5],
            mod_vs_nether: 0.0,
            mod_vs_electric: m[6],
        });
    }
    if has(0x8) {
        for key in [8, 19, 5] {
            t.int(key, r.u32()?);
        }
    }
    for (bit, key) in [
        (0x200, 173),
        (0x800, 0),
        (0x2000, 89),
        (0x1000, 90),
        (0x8000, 28),
        (0x40, 38),
    ] {
        if has(bit) {
            let v = r.u32()?;
            if bit == 0x800 {
                t.bool(3, v);
            } else {
                t.int(key, v);
            }
        }
    }
    if has(0x0400_0000) {
        t.string(25, read_string(r)?);
    }
    if has(0x10) {
        for key in [7, 8, 23] {
            t.string(key, read_string(r)?);
        }
    }
    for (bit, key) in [(0x400, 14), (0x2_0000, 16), (0x4_0000, 15)] {
        if has(bit) {
            t.string(key, read_string(r)?);
        }
    }
    if has(0x1_0000) {
        t.int(86, r.u32()?);
        t.int(87, r.u32()?);
        let destination = read_string(r)?;
        if destination != " " {
            t.string(38, destination);
        }
    }
    if has(0x8_0000) {
        t.int(92, r.u32()?);
        t.int(91, r.u32()?);
    }
    if has(0x10_0000) {
        p.armor_enchantment = Some(r.u32()?);
    }
    if has(0x20_0000) {
        p.weapon_enchantment = Some(r.u32()?);
    }
    if has(0x40_0000) {
        p.resist_enchantment = Some(r.u32()?);
    }
    if has(0x80_0000) {
        for key in [109, 106, 110, 107, 108] {
            t.int(key, r.u32()?);
        }
        r.u32()?;
        r.u32()?;
        t.string(19, read_string(r)?);
        t.int(115, r.u32()?);
        let skill = r.u32()?;
        if skill != 0 {
            t.int(176, skill);
        }
        // The spells are named as one line, not ids: it goes under the long description.
        let spells = read_string(r)?;
        t.append_line(16, spells);
        read_string(r)?;
        r.f64()?;
    }

    t.fill(&mut p);
    Ok(ItemSetAppraiseInfo { object, profile: p })
}

/// The later appraisal's property tables, built section by section.
#[derive(Debug, Default)]
struct Tables {
    ints: Vec<(u32, i32)>,
    bools: Vec<(u32, i32)>,
    floats: Vec<(u32, f64)>,
    strings: Vec<(u32, String)>,
}

impl Tables {
    fn int(&mut self, key: u32, v: u32) {
        if v != ABSENT {
            self.ints.push((key, v as i32));
        }
    }

    fn bool(&mut self, key: u32, v: u32) {
        if v != ABSENT {
            self.bools.push((key, v as i32));
        }
    }

    fn float(&mut self, key: u32, v: f64) {
        self.floats.push((key, v));
    }

    fn string(&mut self, key: u32, v: String) {
        if !v.is_empty() {
            self.strings.push((key, v));
        }
    }

    fn append_line(&mut self, key: u32, line: String) {
        if line.is_empty() {
            return;
        }
        match self.strings.iter_mut().find(|(k, _)| *k == key) {
            Some((_, s)) => {
                s.push('\n');
                s.push_str(&line);
            }
            None => self.strings.push((key, line)),
        }
    }

    fn fill(self, p: &mut AppraisalProfile) {
        fn table<V>(v: Vec<(u32, V)>) -> Option<PackedHash<u32, V>> {
            (!v.is_empty()).then_some(PackedHash {
                table_size: TABLE_SIZE,
                entries: v,
            })
        }
        p.tables.ints = table(self.ints);
        p.tables.bools = table(self.bools);
        p.tables.floats = table(self.floats);
        p.tables.strings = table(self.strings);
        let bits = [
            (p.tables.ints.is_some(), later::INT),
            (p.tables.bools.is_some(), later::BOOL),
            (p.tables.floats.is_some(), later::FLOAT),
            (p.tables.strings.is_some(), later::STRING),
            (p.weapon_profile.is_some(), later::WEAPON_PROFILE),
            (p.armor_profile.is_some(), later::ARMOR_PROFILE),
            (p.creature_profile.is_some(), later::CREATURE_PROFILE),
            (p.armor_enchantment.is_some(), later::ARMOR_ENCHANTMENT),
            (p.weapon_enchantment.is_some(), later::WEAPON_ENCHANTMENT),
            (p.resist_enchantment.is_some(), later::RESIST_ENCHANTMENT),
        ];
        p.flags = bits.iter().filter(|(on, _)| *on).fold(0, |f, (_, b)| f | b);
    }
}

/// The 1999 vendor list: the later profile without the alternate currency, then the items.
fn vendor(r: &mut Reader<'_>, description: DescriptionReader) -> Result<VendorInfo, MessageError> {
    let merchant_id = ObjectId(r.u32()?);
    let profile = VendorProfile {
        item_types: r.u32()?,
        min_value: r.i32()?,
        max_value: r.i32()?,
        magic: r.i32()?,
        buy_price: r.f32()?,
        sell_price: r.f32()?,
        trade_id: 0,
        trade_num: 0,
        trade_name: String::new(),
    };
    let items = r.packed_list(|r| {
        let v = r.u32()?;
        let amount = if v & 0x0080_0000 != 0 {
            (v | 0xFF00_0000) as i32
        } else {
            (v & 0x00FF_FFFF) as i32
        };
        let iid = ObjectId(r.u32()?);
        let pwd = if v >> 24 == 0xFF {
            Some(description(r)?)
        } else {
            None
        };
        Ok(ItemProfile { amount, iid, pwd })
    })?;
    Ok(VendorInfo {
        merchant_id,
        profile,
        items,
    })
}
