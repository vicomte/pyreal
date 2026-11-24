//! Quality updates: the flat 1999 `object, key, value` messages become the later private or public
//! updates, with sequence bytes from the shared counters ([`Stamps::next_property`]).
//!
//! The 1999 numbers clash with unrelated later ones (`0x0029` is a vital here, a title table later),
//! so nothing in this module is looked up in the later opcode table.

use pyreal_primitives::ObjectId;
use pyreal_protocol::items::ItemUpdateStackSize;
use pyreal_protocol::qualities::{
    self as q, stat_type, AlignedString, PrivateUpdate, PublicUpdate,
};
use pyreal_protocol::types::qualities::{Attribute, SecondaryAttribute, Skill};
use pyreal_protocol::{Message, MessageError, Reader};

use crate::events::codec;
use crate::events::Later;
use crate::stamps::Stamps;

/// `0x000E` Int.
pub const UPDATE_INT: u32 = 0x000E;
/// `0x000F` String.
pub const UPDATE_STRING: u32 = 0x000F;
/// `0x0010` skill level.
pub const UPDATE_SKILL_LEVEL: u32 = 0x0010;
/// `0x0011` DataID.
pub const UPDATE_DATA_ID: u32 = 0x0011;
/// `0x0026` attribute.
pub const UPDATE_ATTRIBUTE: u32 = 0x0026;
/// `0x0028` skill.
pub const UPDATE_SKILL: u32 = 0x0028;
/// `0x0029` vital.
pub const UPDATE_VITAL: u32 = 0x0029;
/// `0x002C` attribute level.
pub const UPDATE_ATTRIBUTE_LEVEL: u32 = 0x002C;
/// `0x002E` Float.
pub const UPDATE_FLOAT: u32 = 0x002E;
/// `0x002F` skill advancement class.
pub const UPDATE_SKILL_AC: u32 = 0x002F;
/// `0x0043` Bool.
pub const UPDATE_BOOL: u32 = 0x0043;
/// `0x0057` InstanceID.
pub const UPDATE_INSTANCE_ID: u32 = 0x0057;
/// `0x0197` stack size, three words with no sequence.
pub const STACK_SIZE: u32 = 0x0197;

/// Every 1999 quality update opcode.
pub const UPDATES: [u32; 12] = [
    UPDATE_INT,
    UPDATE_STRING,
    UPDATE_SKILL_LEVEL,
    UPDATE_DATA_ID,
    UPDATE_ATTRIBUTE,
    UPDATE_SKILL,
    UPDATE_VITAL,
    UPDATE_ATTRIBUTE_LEVEL,
    UPDATE_FLOAT,
    UPDATE_SKILL_AC,
    UPDATE_BOOL,
    UPDATE_INSTANCE_ID,
];

/// Int keys the layer renumbers.
pub mod keys {
    /// `StackSize`, whose sequence the stack-size message shares.
    pub const STACK_SIZE: u32 = 12;
    /// Total experience: Int in 1999.
    pub const TOTAL_XP: u32 = 0x15;
    /// Unassigned experience: Int in 1999.
    pub const AVAILABLE_XP: u32 = 0x16;
    /// Total experience as the later Int64 key.
    pub const TOTAL_XP_64: u32 = 1;
    /// Unassigned experience as the later Int64 key.
    pub const AVAILABLE_XP_64: u32 = 2;
    /// `CombatMode`.
    pub const COMBAT_MODE: u32 = 0x28;
    /// `HeritageGroup`.
    pub const HERITAGE: u32 = 0xBC;
}

/// The skill format word the layer writes: 0, the legacy fold of `init_level` into `pp`, which is
/// what the 1999 client does. AC1999 sends a player's `init_level` as 0, so both forms agree.
pub const SKILL_FORMAT: u16 = 0;

/// The 1999 combat mode (1 peace, 2 melee, 3 missile, 4 magic) as the later bit; others unchanged.
#[must_use]
pub fn combat_mode_bit(mode: i32) -> i32 {
    match mode {
        3 => 4,
        4 => 8,
        m => m,
    }
}

/// One Int as the later client keeps it: experience moves to Int64 (`Err`), the combat mode
/// becomes a bit and the heritage counts from 1.
///
/// # Errors
/// `Err((int64 key, value))` for the two experience keys.
pub fn later_int(key: u32, value: i32) -> Result<(u32, i32), (u32, i64)> {
    match key {
        // The 1999 word is unsigned experience.
        keys::TOTAL_XP => Err((keys::TOTAL_XP_64, i64::from(value as u32))),
        keys::AVAILABLE_XP => Err((keys::AVAILABLE_XP_64, i64::from(value as u32))),
        keys::COMBAT_MODE => Ok((key, combat_mode_bit(value))),
        keys::HERITAGE => Ok((key, value.saturating_add(1))),
        _ => Ok((key, value)),
    }
}

/// A 1999 skill with the format word the later client should see.
#[must_use]
pub fn later_skill(mut s: Skill) -> Skill {
    s.format_version = SKILL_FORMAT;
    s
}

/// A 1999 quality update's value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i32),
    String(String),
    SkillLevel(u32),
    DataId(u32),
    Attribute(Attribute),
    Skill(Skill),
    /// The whole vital; the key says whether the maximum (odd) or current (even) changed.
    Vital(SecondaryAttribute),
    AttributeLevel(u32),
    Float(f64),
    SkillAc(u32),
    Bool(i32),
    InstanceId(u32),
}

/// One 1999 quality update.
#[derive(Debug, Clone, PartialEq)]
pub struct QualityUpdate {
    pub object: u32,
    pub key: u32,
    pub value: Value,
}

impl QualityUpdate {
    /// # Errors
    /// [`MessageError`] for an opcode that is not a quality update or a body that does not read.
    pub fn read(message: &[u8]) -> Result<Self, MessageError> {
        let mut r = codec::reader(message)?;
        let opcode = u32::from_le_bytes([message[0], message[1], message[2], message[3]]);
        let object = r.u32()?;
        let key = r.u32()?;
        let value = match opcode {
            UPDATE_INT => Value::Int(r.i32()?),
            UPDATE_STRING => Value::String(codec::read_string(&mut r)?),
            UPDATE_SKILL_LEVEL => Value::SkillLevel(r.u32()?),
            UPDATE_DATA_ID => Value::DataId(r.u32()?),
            UPDATE_ATTRIBUTE => Value::Attribute(Attribute::read(&mut r)?),
            UPDATE_SKILL => Value::Skill(Skill::read(&mut r)?),
            UPDATE_VITAL => Value::Vital(SecondaryAttribute::read(&mut r)?),
            UPDATE_ATTRIBUTE_LEVEL => Value::AttributeLevel(r.u32()?),
            UPDATE_FLOAT => Value::Float(r.f64()?),
            UPDATE_SKILL_AC => Value::SkillAc(r.u32()?),
            UPDATE_BOOL => Value::Bool(r.i32()?),
            UPDATE_INSTANCE_ID => Value::InstanceId(r.u32()?),
            other => {
                return Err(MessageError::OpcodeMismatch {
                    expected: UPDATE_INT,
                    found: other,
                })
            }
        };
        codec::finish(&mut r)?;
        Ok(Self { object, key, value })
    }

    /// The later update: private when `object` is the player, public otherwise.
    ///
    /// # Errors
    /// [`MessageError`] when the later message does not encode.
    pub fn to_later(&self, player: Option<u32>, seq: &mut Stamps) -> Result<Later, MessageError> {
        let private = player == Some(self.object);
        let (o, k) = (self.object, self.key);
        let mut emit = Emit {
            private,
            object: o,
            seq,
        };
        use q::{
            QualitiesPrivateUpdateAttribute as PAt, QualitiesPrivateUpdateAttribute2nd as PV,
            QualitiesPrivateUpdateAttribute2ndLevel as PVl,
            QualitiesPrivateUpdateAttributeLevel as PAl, QualitiesPrivateUpdateBool as PB,
            QualitiesPrivateUpdateDataId as PD, QualitiesPrivateUpdateFloat as PF,
            QualitiesPrivateUpdateInstanceId as PI, QualitiesPrivateUpdateInt as PInt,
            QualitiesPrivateUpdateInt64 as PI64, QualitiesPrivateUpdateSkill as PS,
            QualitiesPrivateUpdateSkillAc as PSac, QualitiesPrivateUpdateSkillLevel as PSl,
            QualitiesPrivateUpdateString as PStr, QualitiesUpdateAttribute as UAt,
            QualitiesUpdateAttribute2nd as UV, QualitiesUpdateAttribute2ndLevel as UVl,
            QualitiesUpdateAttributeLevel as UAl, QualitiesUpdateBool as UB,
            QualitiesUpdateDataId as UD, QualitiesUpdateFloat as UF,
            QualitiesUpdateInstanceId as UI, QualitiesUpdateInt as UInt,
            QualitiesUpdateInt64 as UI64, QualitiesUpdateSkill as US,
            QualitiesUpdateSkillAc as USac, QualitiesUpdateSkillLevel as USl,
            QualitiesUpdateString as UStr,
        };
        use stat_type as s;
        match &self.value {
            Value::Int(v) => match later_int(k, *v) {
                Ok((k, v)) => emit.one(s::INT, k, v, PInt, UInt),
                Err((k, v)) => emit.one(s::INT64, k, v, PI64, UI64),
            },
            Value::String(v) => emit.one(s::STRING, k, AlignedString(v.clone()), PStr, UStr),
            Value::SkillLevel(v) => emit.one(s::SKILL, k, *v, PSl, USl),
            Value::DataId(v) => emit.one(s::DID, k, *v, PD, UD),
            Value::Attribute(v) => emit.one(s::ATTRIBUTE, k, *v, PAt, UAt),
            Value::Skill(v) => emit.one(s::SKILL, k, later_skill(*v), PS, US),
            Value::Vital(v) if k % 2 == 1 => emit.one(s::ATTRIBUTE_2ND, k, *v, PV, UV),
            Value::Vital(v) => emit.one(s::ATTRIBUTE_2ND, k, v.current_level, PVl, UVl),
            Value::AttributeLevel(v) => emit.one(s::ATTRIBUTE, k, *v, PAl, UAl),
            Value::Float(v) => emit.one(s::FLOAT, k, *v, PF, UF),
            Value::SkillAc(v) => emit.one(s::SKILL, k, *v, PSac, USac),
            Value::Bool(v) => emit.one(s::BOOL, k, *v, PB, UB),
            Value::InstanceId(v) => emit.one(s::IID, k, ObjectId(*v), PI, UI),
        }
    }
}

/// Builds the private or public form of one update with its sequence.
struct Emit<'a> {
    private: bool,
    object: u32,
    seq: &'a mut Stamps,
}

impl Emit<'_> {
    fn one<P: Message, U: Message, V>(
        &mut self,
        stat: u32,
        key: u32,
        value: V,
        private: fn(PrivateUpdate<V>) -> P,
        public: fn(PublicUpdate<V>) -> U,
    ) -> Result<Later, MessageError> {
        let sequence = self
            .seq
            .next_property(self.object, stat_type::key(stat, key));
        if self.private {
            Later::of(&private(PrivateUpdate {
                sequence,
                property_id: key,
                value,
            }))
        } else {
            Later::of(&public(PublicUpdate {
                sequence,
                object: ObjectId(self.object),
                property_id: key,
                value,
            }))
        }
    }
}

/// `0x0197`: item, amount, value; the later form leads with the `StackSize` Int sequence.
///
/// # Errors
/// [`MessageError`] for a body that does not read.
pub fn stack_size(message: &[u8], seq: &mut Stamps) -> Result<Later, MessageError> {
    let mut r = codec::reader(message)?;
    let item = r.u32()?;
    let amount = r.u32()?;
    let new_value = r.u32()?;
    codec::finish(&mut r)?;
    Later::of(&ItemUpdateStackSize {
        sequence: seq.next_property(item, stat_type::key(stat_type::INT, keys::STACK_SIZE)),
        item: ObjectId(item),
        amount,
        new_value,
    })
}

/// The stat type and whether it is the private form, for the later-form updates the server still
/// writes with its own sequence byte.
#[must_use]
pub fn later_form(opcode: u32) -> Option<(u32, bool)> {
    use stat_type as s;
    Some(match opcode {
        0x01D1 => (s::INT, true),
        0x01D2 => (s::INT, false),
        0x01D3 => (s::BOOL, true),
        0x01D4 => (s::BOOL, false),
        0x01D5 => (s::FLOAT, true),
        0x01D6 => (s::FLOAT, false),
        0x01D7 => (s::STRING, true),
        0x01D8 => (s::STRING, false),
        0x01D9 => (s::DID, true),
        0x01DA => (s::DID, false),
        0x01DB => (s::IID, true),
        0x01DC => (s::IID, false),
        0x01DD => (s::POSITION, true),
        0x01DE => (s::POSITION, false),
        0x02B8 => (s::INT64, true),
        0x02B9 => (s::INT64, false),
        0x02CF => (s::INT64, true),
        0x02D0 => (s::INT64, false),
        0x02DB => (s::POSITION, true),
        0x02DC => (s::POSITION, false),
        0x02E1 => (s::SKILL, true),
        _ => return None,
    })
}

/// A later-form update passed through with its sequence byte drawn from the layer's counter.
///
/// # Errors
/// [`MessageError`] for a body too short to hold the sequence, object and key.
pub fn pass_later_form(
    message: &[u8],
    player: Option<u32>,
    seq: &mut Stamps,
) -> Result<Option<Later>, MessageError> {
    let opcode = u32::from_le_bytes([message[0], message[1], message[2], message[3]]);
    let Some((stat, private)) = later_form(opcode) else {
        return Ok(None);
    };
    let mut r = Reader::new(&message[4..]);
    r.u8()?;
    let object = if private {
        player.unwrap_or(0)
    } else {
        r.u32()?
    };
    let key = r.u32()?;
    let mut body = message[4..].to_vec();
    body[0] = seq.next_property(object, stat_type::key(stat, key));
    Ok(Some(Later { opcode, body }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyreal_protocol::{read_body, Writer};

    const ME: u32 = 0x5000_0001;

    fn msg(opcode: u32, f: impl FnOnce(&mut Writer)) -> Vec<u8> {
        let mut w = Writer::new();
        w.u32(opcode);
        f(&mut w);
        w.into_inner()
    }

    fn int(object: u32, key: u32, v: i32) -> Vec<u8> {
        msg(UPDATE_INT, |w| {
            w.u32(object);
            w.u32(key);
            w.i32(v);
        })
    }

    #[test]
    fn an_int_on_the_player_is_private_and_on_another_public_each_with_its_own_counter() {
        let mut s = Stamps::default();
        let me = QualityUpdate::read(&int(ME, 25, 7)).unwrap();
        let a = me.to_later(Some(ME), &mut s).unwrap();
        let b = me.to_later(Some(ME), &mut s).unwrap();
        assert_eq!(a.opcode, 0x02CD);
        let a: q::QualitiesPrivateUpdateInt = read_body(&a.body).unwrap();
        let b: q::QualitiesPrivateUpdateInt = read_body(&b.body).unwrap();
        assert_eq!((a.0.sequence, a.0.property_id, a.0.value), (1, 25, 7));
        assert_eq!(b.0.sequence, 2);

        let other = QualityUpdate::read(&int(0x8000_0002, 25, 3)).unwrap();
        let c = other.to_later(Some(ME), &mut s).unwrap();
        assert_eq!(c.opcode, 0x02CE);
        let c: q::QualitiesUpdateInt = read_body(&c.body).unwrap();
        assert_eq!((c.0.sequence, c.0.object.0, c.0.value), (1, 0x8000_0002, 3));
    }

    #[test]
    fn experience_becomes_int64_and_shares_the_counter_with_a_later_form_update() {
        let mut s = Stamps::default();
        let xp = QualityUpdate::read(&int(ME, keys::TOTAL_XP, -2)).unwrap();
        let l = xp.to_later(Some(ME), &mut s).unwrap();
        assert_eq!(l.opcode, 0x02CF);
        let m: q::QualitiesPrivateUpdateInt64 = read_body(&l.body).unwrap();
        assert_eq!(
            (m.0.sequence, m.0.property_id, m.0.value),
            (1, 1, 0xFFFF_FFFE)
        );
        let later = msg(0x02CF, |w| {
            w.u8(200);
            w.u32(1);
            w.i64(5);
        });
        let p = pass_later_form(&later, Some(ME), &mut s).unwrap().unwrap();
        assert_eq!(p.body[0], 2, "the server's 200 is replaced");
        let unspent = QualityUpdate::read(&int(ME, keys::AVAILABLE_XP, 9)).unwrap();
        let m: q::QualitiesPrivateUpdateInt64 =
            read_body(&unspent.to_later(Some(ME), &mut s).unwrap().body).unwrap();
        assert_eq!((m.0.sequence, m.0.property_id, m.0.value), (1, 2, 9));
    }

    #[test]
    fn the_combat_mode_becomes_a_bit_and_the_heritage_counts_from_one() {
        let cases = [(1, 1), (2, 2), (3, 4), (4, 8)];
        for (mode, bit) in cases {
            assert_eq!(
                later_int(keys::COMBAT_MODE, mode),
                Ok((keys::COMBAT_MODE, bit))
            );
        }
        assert_eq!(later_int(keys::HERITAGE, 0), Ok((keys::HERITAGE, 1)));
        assert_eq!(later_int(5, 0), Ok((5, 0)));
    }

    #[test]
    fn a_string_update_reads_the_1999_form_and_writes_the_aligned_later_one() {
        let m = msg(UPDATE_STRING, |w| {
            w.u32(ME);
            w.u32(1);
            codec::write_string(w, "Ann");
        });
        let u = QualityUpdate::read(&m).unwrap();
        assert_eq!(u.value, Value::String("Ann".to_owned()));
        let l = u.to_later(Some(ME), &mut Stamps::default()).unwrap();
        assert_eq!(l.opcode, 0x02D5);
        let s: q::QualitiesPrivateUpdateString = read_body(&l.body).unwrap();
        assert_eq!(s.0.value.0, "Ann");
    }

    fn vital(key: u32) -> Vec<u8> {
        msg(UPDATE_VITAL, |w| {
            w.u32(ME);
            w.u32(key);
            for v in [10, 20, 30, 44] {
                w.u32(v);
            }
        })
    }

    #[test]
    fn a_vital_maximum_sends_the_whole_vital_and_a_current_only_the_level() {
        let mut s = Stamps::default();
        let max = QualityUpdate::read(&vital(1)).unwrap();
        let l = max.to_later(Some(ME), &mut s).unwrap();
        assert_eq!(l.opcode, 0x02E7);
        let v: q::QualitiesPrivateUpdateAttribute2nd = read_body(&l.body).unwrap();
        assert_eq!(v.0.value.current_level, 44);
        let cur = QualityUpdate::read(&vital(2)).unwrap();
        let l = cur.to_later(Some(ME), &mut s).unwrap();
        assert_eq!(l.opcode, 0x02E9);
        let v: q::QualitiesPrivateUpdateAttribute2ndLevel = read_body(&l.body).unwrap();
        assert_eq!((v.0.property_id, v.0.value), (2, 44));
    }

    #[test]
    fn a_skill_update_carries_the_layers_format_word() {
        let m = msg(UPDATE_SKILL, |w| {
            w.u32(ME);
            w.u32(6);
            Skill {
                level_from_pp: 5,
                format_version: 7,
                sac: 2,
                pp: 100,
                init_level: 10,
                resistance_of_last_check: 0,
                last_used_time: 0.0,
            }
            .write(w);
        });
        let l = QualityUpdate::read(&m)
            .unwrap()
            .to_later(Some(ME), &mut Stamps::default())
            .unwrap();
        assert_eq!(l.opcode, 0x02DD);
        let s: q::QualitiesPrivateUpdateSkill = read_body(&l.body).unwrap();
        assert_eq!(
            (s.0.value.format_version, s.0.value.init_level),
            (SKILL_FORMAT, 10)
        );
    }

    #[test]
    fn every_other_update_maps_to_its_later_opcode() {
        let me = Some(ME);
        type Case<'a> = (u32, &'a dyn Fn(&mut Writer), u32, u32);
        let cases: [Case<'_>; 8] = [
            (UPDATE_SKILL_LEVEL, &|w| w.u32(3), 0x02DF, 0x02E0),
            (UPDATE_DATA_ID, &|w| w.u32(3), 0x02D7, 0x02D8),
            (UPDATE_BOOL, &|w| w.i32(1), 0x02D1, 0x02D2),
            (UPDATE_INSTANCE_ID, &|w| w.u32(3), 0x02D9, 0x02DA),
            (UPDATE_FLOAT, &|w| w.f64(1.5), 0x02D3, 0x02D4),
            (UPDATE_ATTRIBUTE_LEVEL, &|w| w.u32(3), 0x02E5, 0x02E6),
            (UPDATE_SKILL_AC, &|w| w.u32(3), 0x02E1, 0x02E2),
            (
                UPDATE_ATTRIBUTE,
                &|w| {
                    w.u32(1);
                    w.u32(2);
                    w.u32(3);
                },
                0x02E3,
                0x02E4,
            ),
        ];
        for (op, value, private, public) in cases {
            for (object, want) in [(ME, private), (7, public)] {
                let m = msg(op, |w| {
                    w.u32(object);
                    w.u32(4);
                    value(w);
                });
                let l = QualityUpdate::read(&m)
                    .unwrap()
                    .to_later(me, &mut Stamps::default())
                    .unwrap();
                assert_eq!(l.opcode, want, "{op:#06X}");
                assert_eq!(l.body[0], 1);
            }
        }
    }

    #[test]
    fn stack_size_gains_the_stack_size_int_sequence() {
        let mut s = Stamps::default();
        let item = 0x8000_0010;
        let k = QualityUpdate::read(&int(item, keys::STACK_SIZE, 4))
            .unwrap()
            .to_later(Some(ME), &mut s)
            .unwrap();
        assert_eq!(k.body[0], 1);
        let m = msg(STACK_SIZE, |w| {
            w.u32(item);
            w.u32(5);
            w.u32(50);
        });
        let l = stack_size(&m, &mut s).unwrap();
        let u: ItemUpdateStackSize = read_body(&l.body).unwrap();
        assert_eq!(
            (u.sequence, u.item.0, u.amount, u.new_value),
            (2, item, 5, 50)
        );
    }

    #[test]
    fn a_short_or_overlong_update_is_refused() {
        let mut m = int(ME, 1, 1);
        m.truncate(14);
        assert!(QualityUpdate::read(&m).is_err());
        let mut m = int(ME, 1, 1);
        m.extend_from_slice(&[1, 0, 0, 0]);
        assert!(QualityUpdate::read(&m).is_err());
    }
}
