//! Speech, emotes, channels, system text, errors, questions and the squelch list.

use pyreal_primitives::ObjectId;
use pyreal_protocol::archive::PackedHash;
use pyreal_protocol::comms::{
    CommunicationChannelBroadcastRecv, CommunicationHearDirectSpeech, CommunicationHearEmote,
    CommunicationHearRangedSpeech, CommunicationHearSoulEmote, CommunicationHearSpeech,
    CommunicationSetSquelchDb, CommunicationTextboxString, CommunicationTransientString,
    CommunicationWeenieError, SquelchDb, SquelchInfo, VLong,
};
use pyreal_protocol::{MessageError, Reader};

use super::codec::read_string;
use super::magic::read;
use super::{Handled, Later, Squelches};
use crate::translate::Shared;

/// `0x0037` speech heard.
pub const SPEECH: u32 = 0x0037;
/// `0x0038` tell heard.
pub const TELL: u32 = 0x0038;
/// `0x01B5` ranged speech, name first.
pub const RANGED: u32 = 0x01B5;
/// `0x01E0` emote.
pub const EMOTE: u32 = 0x01E0;
/// `0x01E2` soul emote.
pub const SOUL_EMOTE: u32 = 0x01E2;
/// `0x014A` channel line.
pub const CHANNEL: u32 = 0x014A;
/// `0xF62C` system or magic text.
pub const SYSTEM_TEXT: u32 = 0xF62C;
/// `0x0016` status text, in the later string form.
pub const STATUS: u32 = 0x0016;
/// `0x001C` error code.
pub const ERROR: u32 = 0x001C;
/// `0x005A` one character squelched.
pub const SQUELCHED: u32 = 0x005A;
/// `0x005B` one character unsquelched.
pub const UNSQUELCHED: u32 = 0x005B;
/// `0x0274` a question, in the later form.
pub const QUESTION: u32 = 0x0274;

/// The chat type the later client gives local speech.
const SPEECH_TYPE: u32 = 2;
/// The chat type of a tell.
const TELL_TYPE: u32 = 3;
/// The chat type a status line is also written to chat with.
const STATUS_CHAT_TYPE: u32 = 0;
/// The table size hint of the squelch database the layer builds.
const SQUELCH_TABLE_SIZE: u32 = 16;
/// Later, already in the later form: error with text, transient string.
const PASSED: [u32; 2] = [0x028B, 0x02EB];

pub(super) fn translate(
    squelch: &mut Squelches,
    shared: &mut Shared,
    opcode: u32,
    message: &[u8],
) -> Option<Result<Handled, MessageError>> {
    let out = match opcode {
        SPEECH => read(message, |r| {
            Later::of(&CommunicationHearSpeech {
                message: read_string(r)?,
                sender_name: read_string(r)?,
                sender_id: ObjectId(r.u32()?),
                text_type: SPEECH_TYPE,
            })
        }),
        TELL => read(message, |r| {
            Later::of(&CommunicationHearDirectSpeech {
                message: read_string(r)?,
                sender_name: read_string(r)?,
                sender_id: ObjectId(r.u32()?),
                target_id: ObjectId(r.u32()?),
                text_type: TELL_TYPE,
                secret_flags: 0,
            })
        }),
        RANGED => read(message, |r| {
            let sender_name = read_string(r)?;
            Later::of(&CommunicationHearRangedSpeech {
                message: read_string(r)?,
                sender_name,
                sender_id: ObjectId(r.u32()?),
                range: r.f32()?,
                text_type: SPEECH_TYPE,
            })
        }),
        EMOTE => read(message, |r| {
            let (sender, sender_name, text) = emote(r)?;
            Later::of(&CommunicationHearEmote {
                sender,
                sender_name,
                text,
            })
        }),
        SOUL_EMOTE => read(message, |r| {
            let (sender, sender_name, text) = emote(r)?;
            Later::of(&CommunicationHearSoulEmote {
                sender,
                sender_name,
                text,
            })
        }),
        CHANNEL => read(message, |r| {
            let channel = r.u32()?;
            let sender_name = read_string(r)?;
            let text = read_string(r)?;
            // A line with no name carries a word nothing reads.
            if sender_name.is_empty() && r.remaining() >= 4 {
                r.u32()?;
            }
            Later::of(&CommunicationChannelBroadcastRecv {
                channel,
                sender_name,
                message: text,
            })
        }),
        SYSTEM_TEXT => read(message, |r| {
            Later::of(&CommunicationTextboxString {
                text: read_string(r)?,
                text_type: r.u32()?,
            })
        }),
        STATUS => {
            return Some(read(message, |r| r.pstring()).and_then(|text| {
                let chat = Later::of(&CommunicationTextboxString {
                    text: text.clone(),
                    text_type: STATUS_CHAT_TYPE,
                })?;
                let strip = Later::of(&CommunicationTransientString { text })?;
                Ok(Handled::Out(vec![strip, chat]))
            }))
        }
        ERROR => read(message, |r| r.u32())
            .and_then(|error_type| Later::of(&CommunicationWeenieError { error_type })),
        SQUELCHED | UNSQUELCHED => read(message, |r| {
            let name = read_string(r)?;
            let id = r.u32()?;
            shared.note_squelch(opcode == SQUELCHED, id);
            if opcode == SQUELCHED {
                squelch.insert(id, name);
            } else {
                squelch.remove(&id);
            }
            squelch_db(squelch)
        }),
        op if PASSED.contains(&op) => Ok(Later::same(op, message)),
        _ => return None,
    };
    Some(out.map(Handled::from))
}

fn emote(r: &mut Reader<'_>) -> Result<(ObjectId, String, String), MessageError> {
    Ok((ObjectId(r.u32()?), read_string(r)?, read_string(r)?))
}

/// The later database: every squelched character with all message types, no account or global
/// entries, which 1999 lacks.
pub(super) fn squelch_db(squelch: &Squelches) -> Result<Later, MessageError> {
    let character_hash = PackedHash {
        table_size: SQUELCH_TABLE_SIZE,
        entries: squelch
            .iter()
            .map(|(id, name)| {
                let info = SquelchInfo {
                    squelch_msgs: VLong(vec![u32::MAX; 4]),
                    name: name.clone(),
                    is_zone_squelch: 0,
                };
                (*id, info)
            })
            .collect(),
    };
    Later::of(&CommunicationSetSquelchDb(SquelchDb {
        account_hash: PackedHash {
            table_size: SQUELCH_TABLE_SIZE,
            entries: Vec::new(),
        },
        character_hash,
        global_squelch_info: SquelchInfo::default(),
    }))
}
