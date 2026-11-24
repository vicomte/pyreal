//! Combat notifications: later field orders and widths, 1999 strings.

use pyreal_primitives::ObjectId;
use pyreal_protocol::combat::{
    AttackerNotification, CombatHandlePlayerDeathEvent, DefenderNotification,
    EvasionAttackerNotification, EvasionDefenderNotification, VictimNotificationOther,
    VictimNotificationSelf,
};
use pyreal_protocol::MessageError;

use super::codec::read_string;
use super::magic::read;
use super::{Handled, Later};

/// `0x01B1` you hit.
pub const YOU_HIT: u32 = 0x01B1;
/// `0x01B2` you were hit.
pub const YOU_WERE_HIT: u32 = 0x01B2;
/// `0x01B3` the defender evaded your attack.
pub const THEY_EVADED: u32 = 0x01B3;
/// `0x01B4` you evaded.
pub const YOU_EVADED: u32 = 0x01B4;
/// `0x01AC` your death line.
pub const DEATH_SELF: u32 = 0x01AC;
/// `0x01AD` another's death line.
pub const DEATH_OTHER: u32 = 0x01AD;
/// `0x019E` player death broadcast.
pub const PLAYER_DEATH: u32 = 0x019E;
/// Attack done, commence attack, health query answer: bodies with no strings.
const SAME: [u32; 3] = [0x01A7, 0x01B8, 0x01C0];

pub(super) fn translate(opcode: u32, message: &[u8]) -> Option<Result<Handled, MessageError>> {
    let out = match opcode {
        YOU_HIT => read(message, |r| {
            Later::of(&AttackerNotification {
                defender_name: read_string(r)?,
                damage_type: r.u32()?,
                percent: r.f64()?,
                damage: r.u32()?,
                critical: r.u32()?,
                attack_conditions: r.u32()?,
                attack_conditions_high: r.u32()?,
            })
        }),
        YOU_WERE_HIT => read(message, |r| {
            Later::of(&DefenderNotification {
                attacker_name: read_string(r)?,
                damage_type: r.u32()?,
                percent: r.f64()?,
                damage: r.u32()?,
                damage_location: r.u32()?,
                critical: r.u32()?,
                attack_conditions: r.u32()?,
                attack_conditions_high: r.u32()?,
            })
        }),
        THEY_EVADED => read(message, read_string)
            .and_then(|defender_name| Later::of(&EvasionAttackerNotification { defender_name })),
        YOU_EVADED => read(message, read_string)
            .and_then(|attacker_name| Later::of(&EvasionDefenderNotification { attacker_name })),
        DEATH_SELF => read(message, read_string)
            .and_then(|message| Later::of(&VictimNotificationSelf { message })),
        DEATH_OTHER => read(message, read_string)
            .and_then(|message| Later::of(&VictimNotificationOther { message })),
        PLAYER_DEATH => read(message, |r| {
            Later::of(&CombatHandlePlayerDeathEvent {
                message: read_string(r)?,
                killed: ObjectId(r.u32()?),
                killer: ObjectId(r.u32()?),
            })
        }),
        op if SAME.contains(&op) => Ok(Later::same(op, message)),
        _ => return None,
    };
    Some(out.map(Handled::from))
}
