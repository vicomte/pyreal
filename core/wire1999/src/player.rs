//! The 1999 player description (`0x0013`): the later layout with 1999 strings in the property
//! table and a short player module. It becomes Pyreal's `LoginPlayerDescription` with experience on
//! the Int64 keys, the combat mode as a bit, enchantments on layer 1 and the later module's defaults.

use pyreal_primitives::ObjectId;
use pyreal_protocol::archive::PackedHash;
use pyreal_protocol::login::{LoginPlayerDescription, PlayerModule, ShortCutData};
use pyreal_protocol::types::qualities::{
    base_flags, quality_flags, AcBaseQualities, AcQualities, AttributeCache, Enchantment,
    EnchantmentRegistry, PropertyTables, Skill, SpellBookPage,
};
use pyreal_protocol::types::{ContentProfile, InventoryPlacement, PositionWire};
use pyreal_protocol::{MessageError, Reader};

use crate::events::codec;
use crate::qualities::{later_int, later_skill};

/// `0x0013`.
pub const PLAYER_DESCRIPTION: u32 = 0x0013;

/// The table size hint the layer gives the Int64 table it adds.
const INT64_TABLE_SIZE: u32 = 8;

/// The blocks a player's description may carry; any other is refused.
const PLAYER_BLOCKS: u32 = quality_flags::ATTRIBUTE_CACHE
    | quality_flags::SKILLS
    | quality_flags::SPELL_BOOK
    | quality_flags::ENCHANTMENT_REGISTRY;

/// The 1999 player module: shortcuts and exactly one spell bar.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Module1999 {
    /// Only bit `0x0001` (shortcuts) has meaning.
    pub selector: u32,
    /// The `CharacterOptions1` word, whose bits match the later ones.
    pub options: u32,
    pub shortcuts: Option<Vec<ShortCutData>>,
    pub spell_bar: Vec<u32>,
}

/// A decoded 1999 player description, before normalisation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerDescription {
    pub qualities: AcQualities,
    pub module: Module1999,
    pub content_profiles: Vec<ContentProfile>,
    pub inventory_placements: Vec<InventoryPlacement>,
}

impl PlayerDescription {
    /// # Errors
    /// [`MessageError`] for a short body, a block a player never has, or anything left over.
    pub fn read(message: &[u8]) -> Result<Self, MessageError> {
        let mut r = codec::reader(message)?;
        let base = read_base(&mut r)?;
        let flags = r.u32()?;
        let has_health = r.u32()?;
        if flags & !PLAYER_BLOCKS != 0 {
            return Err(MessageError::Unsupported {
                field: "1999 player description block",
                note: "a player has only attributes, skills, spells and enchantments",
            });
        }
        let has = |b: u32| flags & b != 0;
        let mut qualities = AcQualities {
            base,
            flags,
            has_health,
            ..AcQualities::default()
        };
        if has(quality_flags::ATTRIBUTE_CACHE) {
            qualities.attribute_cache = Some(AttributeCache::read(&mut r)?);
        }
        if has(quality_flags::SKILLS) {
            qualities.skills = Some(r.packed_hash(|r| Ok((r.u32()?, Skill::read(r)?)))?);
        }
        if has(quality_flags::SPELL_BOOK) {
            qualities.spell_book =
                Some(r.packed_hash(|r| Ok((r.u32()?, SpellBookPage::read(r)?)))?);
        }
        if has(quality_flags::ENCHANTMENT_REGISTRY) {
            qualities.enchantments = Some(EnchantmentRegistry::read(&mut r)?);
        }
        let selector = r.u32()?;
        let options = r.u32()?;
        let shortcuts = if selector & 1 != 0 {
            Some(r.packed_list(ShortCutData::read)?)
        } else {
            None
        };
        let spell_bar = r.packed_list(Reader::u32)?;
        let content_profiles = r.packed_list(ContentProfile::read)?;
        let inventory_placements = r.packed_list(InventoryPlacement::read)?;
        codec::finish(&mut r)?;
        Ok(Self {
            qualities,
            module: Module1999 {
                selector,
                options,
                shortcuts,
                spell_bar,
            },
            content_profiles,
            inventory_placements,
        })
    }

    /// The later description the session dispatches.
    #[must_use]
    pub fn to_later(&self) -> LoginPlayerDescription {
        let mut qualities = self.qualities.clone();
        normalise_base(&mut qualities.base);
        if let Some(skills) = &mut qualities.skills {
            for (_, s) in &mut skills.entries {
                *s = later_skill(*s);
            }
        }
        if let Some(e) = &mut qualities.enchantments {
            layer_registry(e);
        }
        let m = &self.module;
        LoginPlayerDescription {
            qualities,
            player_module: PlayerModule {
                option_flags: m.selector & 1,
                options: m.options,
                shortcuts: m.shortcuts.clone(),
                spell_bars: vec![m.spell_bar.clone()],
                ..PlayerModule::default()
            },
            content_profiles: self.content_profiles.clone(),
            inventory_placements: self.inventory_placements.clone(),
        }
    }
}

/// The base qualities: the later layout with the 1999 string in the string table.
fn read_base(r: &mut Reader<'_>) -> Result<AcBaseQualities, MessageError> {
    use base_flags as f;
    let flags = r.u32()?;
    let weenie_type = r.u32()?;
    let has = |b: u32| flags & b != 0;
    let mut t = PropertyTables::default();
    if has(f::INT) {
        t.ints = Some(r.packed_hash(|r| Ok((r.u32()?, r.i32()?)))?);
    }
    if has(f::INT64) {
        t.int64s = Some(r.packed_hash(|r| Ok((r.u32()?, r.i64()?)))?);
    }
    if has(f::BOOL) {
        t.bools = Some(r.packed_hash(|r| Ok((r.u32()?, r.i32()?)))?);
    }
    if has(f::FLOAT) {
        t.floats = Some(r.packed_hash(|r| Ok((r.u32()?, r.f64()?)))?);
    }
    if has(f::STRING) {
        t.strings = Some(r.packed_hash(|r| Ok((r.u32()?, codec::read_string(r)?)))?);
    }
    if has(f::DID) {
        t.dids = Some(r.packed_hash(|r| Ok((r.u32()?, r.u32()?)))?);
    }
    if has(f::IID) {
        t.iids = Some(r.packed_hash(|r| Ok((r.u32()?, ObjectId(r.u32()?))))?);
    }
    if has(f::POSITION) {
        t.positions = Some(r.packed_hash(|r| Ok((r.u32()?, PositionWire::read(r)?)))?);
    }
    Ok(AcBaseQualities {
        flags,
        weenie_type,
        tables: t,
    })
}

/// Experience to the Int64 table, the combat mode to a bit, the heritage from 1.
fn normalise_base(base: &mut AcBaseQualities) {
    let Some(ints) = &mut base.tables.ints else {
        return;
    };
    let mut moved = Vec::new();
    ints.entries.retain_mut(|(k, v)| match later_int(*k, *v) {
        Ok((_, later)) => {
            *v = later;
            true
        }
        Err(wide) => {
            moved.push(wide);
            false
        }
    });
    if moved.is_empty() {
        return;
    }
    let int64s = base.tables.int64s.get_or_insert_with(|| PackedHash {
        table_size: INT64_TABLE_SIZE,
        entries: Vec::new(),
    });
    for (k, v) in moved {
        int64s.entries.retain(|(old, _)| *old != k);
        int64s.entries.push((k, v));
    }
    base.flags |= base_flags::INT64;
}

/// An enchantment id with no layer gets layer 1; one that has a layer is left alone.
#[must_use]
pub fn layered(id: u32) -> u32 {
    if id >> 16 == 0 {
        id | 1 << 16
    } else {
        id
    }
}

fn layer_registry(e: &mut EnchantmentRegistry) {
    let lists = [&mut e.multiplicative, &mut e.additive, &mut e.cooldowns];
    for list in lists.into_iter().flatten() {
        for x in list.iter_mut() {
            layer(x);
        }
    }
    if let Some(v) = &mut e.vitae {
        layer(v);
    }
}

/// Sets layer 1 on one enchantment.
pub fn layer(e: &mut Enchantment) {
    e.id = layered(e.id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyreal_protocol::types::qualities::{Attribute, SecondaryAttribute, StatMod};
    use pyreal_protocol::{read_body, write_body, Writer};

    fn hash<V>(entries: Vec<(u32, V)>) -> PackedHash<u32, V> {
        PackedHash {
            table_size: 16,
            entries,
        }
    }

    fn enchantment(id: u32) -> Enchantment {
        Enchantment {
            id,
            category_word: 0x0010,
            power_level: 50,
            start_time: -3.0,
            duration: 1800.0,
            caster: ObjectId(0x5000_0001),
            degrade_modifier: 1.0,
            degrade_limit: -666.0,
            last_time_degraded: 0.0,
            smod: StatMod {
                kind: 0x0000_8011,
                key: 1,
                value: 10.0,
            },
            spell_set_id: None,
        }
    }

    /// A 1999 description as AC1999 lays it out, written field by field.
    fn sample() -> Vec<u8> {
        let mut w = Writer::new();
        w.u32(PLAYER_DESCRIPTION);
        w.u32(base_flags::INT | base_flags::STRING | base_flags::DID);
        w.u32(0x0A);
        let ints = hash(vec![
            (0x15, 150_000),
            (0x16, 40),
            (0x19, 7),
            (0x28, 3),
            (0xBC, 0),
        ]);
        w.packed_hash(&ints, |w, k, v| {
            w.u32(*k);
            w.i32(*v);
            Ok(())
        })
        .unwrap();
        let strings = hash(vec![(1, "Ann".to_owned()), (5, String::new())]);
        w.packed_hash(&strings, |w, k, v| {
            w.u32(*k);
            codec::write_string(w, v);
            Ok(())
        })
        .unwrap();
        w.packed_hash(&hash(vec![(1, 0x0200_0001u32)]), |w, k, v| {
            w.u32(*k);
            w.u32(*v);
            Ok(())
        })
        .unwrap();
        w.u32(PLAYER_BLOCKS);
        w.u32(1);
        let mut cache = AttributeCache {
            flags: 0x41,
            strength: Some(Attribute {
                level_from_cp: 0,
                init_level: 60,
                cp_spent: 0,
            }),
            ..AttributeCache::default()
        };
        cache.health = Some(SecondaryAttribute {
            attribute: Attribute::default(),
            current_level: 30,
        });
        cache.write(&mut w);
        let skill = Skill {
            level_from_pp: 2,
            format_version: 0,
            sac: 2,
            pp: 400,
            init_level: 10,
            resistance_of_last_check: 0,
            last_used_time: 5.0,
        };
        w.packed_hash(&hash(vec![(6, skill)]), |w, k, v| {
            w.u32(*k);
            v.write(w);
            Ok(())
        })
        .unwrap();
        let page = SpellBookPage {
            casting_likelihood: 0.5,
            legacy: Some((1.0, 0)),
        };
        w.packed_hash(&hash(vec![(0x2B, page)]), |w, k, v| {
            w.u32(*k);
            v.write(w);
            Ok(())
        })
        .unwrap();
        EnchantmentRegistry {
            flags: EnchantmentRegistry::MULTIPLICATIVE | EnchantmentRegistry::VITAE,
            multiplicative: Some(vec![enchantment(0x2B), enchantment(0x0002_002C)]),
            vitae: Some(enchantment(666)),
            ..EnchantmentRegistry::default()
        }
        .write(&mut w)
        .unwrap();
        w.u32(1);
        w.u32(0x0000_0402);
        w.packed_list(
            &[ShortCutData {
                index: 0,
                object_id: ObjectId(0x8000_0001),
                spell_id: 0,
            }],
            |w, s| {
                s.write(w);
                Ok(())
            },
        )
        .unwrap();
        w.packed_list(&[0x2Bu32, 0x2C], |w, v| {
            w.u32(*v);
            Ok(())
        })
        .unwrap();
        w.packed_list(
            &[ContentProfile {
                iid: ObjectId(0x8000_0002),
                container_properties: 1,
            }],
            |w, c| {
                c.write(w);
                Ok(())
            },
        )
        .unwrap();
        w.packed_list(
            &[InventoryPlacement {
                iid: ObjectId(0x8000_0003),
                location: 0x10,
                priority: 1,
            }],
            |w, p| {
                p.write(w);
                Ok(())
            },
        )
        .unwrap();
        w.into_inner()
    }

    #[test]
    fn a_1999_description_reads_whole_with_its_strings_and_short_module() {
        let d = PlayerDescription::read(&sample()).unwrap();
        let strings = d.qualities.base.tables.strings.as_ref().unwrap();
        assert_eq!(strings.entries[0], (1, "Ann".to_owned()));
        assert_eq!(strings.entries[1], (5, String::new()));
        assert_eq!(d.module.options, 0x0402);
        assert_eq!(d.module.spell_bar, vec![0x2B, 0x2C]);
        assert_eq!(d.module.shortcuts.as_ref().unwrap().len(), 1);
        assert_eq!(d.inventory_placements[0].location, 0x10);
    }

    #[test]
    fn the_later_description_has_int64_experience_mode_bits_layers_and_defaults() {
        let later = PlayerDescription::read(&sample()).unwrap().to_later();
        let base = &later.qualities.base;
        assert_ne!(base.flags & base_flags::INT64, 0);
        let ints = &base.tables.ints.as_ref().unwrap().entries;
        assert!(!ints.iter().any(|(k, _)| *k == 0x15 || *k == 0x16));
        assert!(ints.contains(&(0x28, 4)), "missile is bit 4");
        assert!(ints.contains(&(0xBC, 1)), "heritage from 1");
        assert!(ints.contains(&(0x19, 7)));
        let int64s = &base.tables.int64s.as_ref().unwrap().entries;
        assert_eq!(int64s, &vec![(1, 150_000), (2, 40)]);

        let skill = later.qualities.skills.as_ref().unwrap().entries[0].1;
        assert_eq!(skill.format_version, crate::qualities::SKILL_FORMAT);
        let e = later.qualities.enchantments.as_ref().unwrap();
        let ids: Vec<u32> = e
            .multiplicative
            .as_ref()
            .unwrap()
            .iter()
            .map(|x| x.id)
            .collect();
        assert_eq!(
            ids,
            vec![0x0001_002B, 0x0002_002C],
            "a layer already set is kept"
        );
        assert_eq!(e.vitae.unwrap().id, 0x0001_029A);

        let m = &later.player_module;
        assert_eq!((m.option_flags, m.options), (1, 0x0402));
        assert_eq!(m.spell_bars, vec![vec![0x2B, 0x2C]]);
        assert_eq!(m.spell_filters, PlayerModule::DEFAULT_SPELL_FILTERS);
        assert_eq!(m.options2, PlayerModule::DEFAULT_OPTIONS2);
        assert!(m.desired_comps.is_none() && m.gameplay_options.is_none());
    }

    #[test]
    fn the_later_description_encodes_and_reads_back_in_pyreal() {
        let later = PlayerDescription::read(&sample()).unwrap().to_later();
        let body = write_body(&later).unwrap();
        let back: LoginPlayerDescription = read_body(&body).unwrap();
        assert_eq!(back, later);
    }

    #[test]
    fn a_description_with_a_block_a_player_lacks_or_extra_bytes_is_refused() {
        let mut m = sample();
        m.extend_from_slice(&[9, 0, 0, 0]);
        assert!(PlayerDescription::read(&m).is_err());
        let mut w = Writer::new();
        w.u32(PLAYER_DESCRIPTION);
        w.u32(0);
        w.u32(0x0A);
        w.u32(quality_flags::BODY);
        w.u32(1);
        assert!(matches!(
            PlayerDescription::read(&w.into_inner()),
            Err(MessageError::Unsupported { .. })
        ));
    }

    #[test]
    fn layering_leaves_an_id_with_a_layer_alone() {
        assert_eq!(layered(0x2B), 0x0001_002B);
        assert_eq!(layered(0x0003_002B), 0x0003_002B);
    }
}
