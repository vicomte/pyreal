//! Dereth's own messages, which only this client and Empyrean read. Neither side sends one the
//! other has not asked for: the client names the extensions it knows in [`DerethExtensions`]
//! once its account is in, and a server that knows the message answers with those of them it
//! has. A retail server drops the message unread, and its client then uses none. See
//! `docs/networking/messages/12-admin-and-misc.md`.
//!
//! - [`AI_FELLOWS`]: the server sends [`AiFellows`], the AI Players in the player's fellowship
//!   (or following them) and each one's mode, whenever that changes; the player sets a mode with
//!   the `@aifellow` command, which any client may type.
//! - [`ADVOCATE_OFFERS`]: an Advocate may offer a waypoint or a journal note; the player confirms
//!   in the game's Yes/No dialog, and on yes the server sends [`Waypoint`] or [`JournalEntry`]
//!   (a client without the extension gets the same as a chat line).
//! - [`QUEST_TIMERS`]: the server sends [`QuestTimers`], the character's quests and when each
//!   may be solved again, whole once in the world and then each change.

use crate::archive::{Reader, Writer};
use crate::error::MessageError;
use crate::opcodes::Opcode;
use crate::Message;

/// The AI fellows extension.
pub const AI_FELLOWS: &str = "ai_fellows";
/// The Advocate offers extension.
pub const ADVOCATE_OFFERS: &str = "advocate_offers";
/// The quest timers extension.
pub const QUEST_TIMERS: &str = "quest_timers";
/// The extensions this client knows, in the order it names them.
pub const KNOWN: [&str; 3] = [AI_FELLOWS, ADVOCATE_OFFERS, QUEST_TIMERS];

/// `0xF7EF Dereth_Extensions` (both directions): extensions by name. From the client, those it
/// knows; from the server, those of them it has.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DerethExtensions {
    pub names: Vec<String>,
}

impl Message for DerethExtensions {
    const OPCODE: Opcode = Opcode::DERETH_EXTENSIONS;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            names: r.packed_list(Reader::pstring)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.packed_list(&self.names, |w, n| w.pstring(n))
    }
}

/// One AI Player in [`AiFellows`]: its id and name, its mode (`aggressive`, `defensive`,
/// `assist`, `heal` or `passive`), and whether it is in the player's fellowship (else it only
/// follows them).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AiFellow {
    pub guid: u32,
    pub name: String,
    pub mode: String,
    pub in_fellowship: bool,
}

/// `0xF7F0 Dereth_AiFellows` (server to client): the AI Players in the player's company and their
/// modes, all of them each time; an empty list when none is left.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AiFellows {
    pub fellows: Vec<AiFellow>,
}

impl Message for AiFellows {
    const OPCODE: Opcode = Opcode::DERETH_AI_FELLOWS;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            fellows: r.packed_list(|r| {
                Ok(AiFellow {
                    guid: r.u32()?,
                    name: r.pstring()?,
                    mode: r.pstring()?,
                    in_fellowship: r.u32()? != 0,
                })
            })?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.packed_list(&self.fellows, |w, f| {
            w.u32(f.guid);
            w.pstring(&f.name)?;
            w.pstring(&f.mode)?;
            w.u32(u32::from(f.in_fellowship));
            Ok(())
        })
    }
}

/// `0xF7F1 Dereth_Waypoint` (server to client): a waypoint an Advocate offered and the player
/// accepted: who offered it, a label, and the spot (a cell and the place in its landblock).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Waypoint {
    pub from: u32,
    pub from_name: String,
    pub label: String,
    pub cell: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Message for Waypoint {
    const OPCODE: Opcode = Opcode::DERETH_WAYPOINT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            from: r.u32()?,
            from_name: r.pstring()?,
            label: r.pstring()?,
            cell: r.u32()?,
            x: r.f32()?,
            y: r.f32()?,
            z: r.f32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.from);
        w.pstring(&self.from_name)?;
        w.pstring(&self.label)?;
        w.u32(self.cell);
        w.f32(self.x);
        w.f32(self.y);
        w.f32(self.z);
        Ok(())
    }
}

/// `0xF7F2 Dereth_JournalEntry` (server to client): a note an Advocate offered and the player
/// accepted, for the client's journal: who offered it, a title and the text, and perhaps a place.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JournalEntry {
    pub from: u32,
    pub from_name: String,
    pub title: String,
    pub text: String,
    /// The page's location as the journal stores it (north, east in map coordinates): a trailing
    /// `u32` flag and two `f32`s, left off when there is none and read only if bytes remain.
    pub location: Option<(f32, f32)>,
}

impl Message for JournalEntry {
    const OPCODE: Opcode = Opcode::DERETH_JOURNAL_ENTRY;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            from: r.u32()?,
            from_name: r.pstring()?,
            title: r.pstring()?,
            text: r.pstring()?,
            // An older sender ends at the text (with under four bytes of padding).
            location: if r.remaining() >= 4 && r.u32()? != 0 {
                Some((r.f32()?, r.f32()?))
            } else {
                None
            },
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.from);
        w.pstring(&self.from_name)?;
        w.pstring(&self.title)?;
        w.pstring(&self.text)?;
        if let Some((ns, ew)) = self.location {
            w.u32(1);
            w.f32(ns);
            w.f32(ew);
        }
        Ok(())
    }
}

/// One quest in [`QuestTimers`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QuestTimer {
    /// The quest's registry name.
    pub name: String,
    /// The quest table's message; empty when none.
    pub message: String,
    pub solves: u32,
    /// `-1` for no limit.
    pub max_solves: i32,
    /// Seconds between solves.
    pub min_delta: u32,
    /// Unix seconds of the last solve.
    pub last_solved: u32,
    /// Unix seconds when it may be solved again: 0 when it may be now, `u32::MAX` when never.
    pub ready_at: u32,
}

/// `0xF7F3 Dereth_QuestTimers` (server to client): the character's quest timers, the whole list
/// or the changes, with the server's Unix seconds when sent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QuestTimers {
    pub now: u32,
    /// The whole list, in place of the last; else only changes.
    pub whole: bool,
    pub timers: Vec<QuestTimer>,
    /// The quests erased, by name.
    pub removed: Vec<String>,
}

impl Message for QuestTimers {
    const OPCODE: Opcode = Opcode::DERETH_QUEST_TIMERS;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            now: r.u32()?,
            whole: r.u32()? != 0,
            timers: r.packed_list(|r| {
                Ok(QuestTimer {
                    name: r.pstring()?,
                    message: r.pstring()?,
                    solves: r.u32()?,
                    max_solves: r.i32()?,
                    min_delta: r.u32()?,
                    last_solved: r.u32()?,
                    ready_at: r.u32()?,
                })
            })?,
            removed: r.packed_list(Reader::pstring)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.now);
        w.u32(u32::from(self.whole));
        w.packed_list(&self.timers, |w, t| {
            w.pstring(&t.name)?;
            w.pstring(&t.message)?;
            w.u32(t.solves);
            w.i32(t.max_solves);
            w.u32(t.min_delta);
            w.u32(t.last_solved);
            w.u32(t.ready_at);
            Ok(())
        })?;
        w.packed_list(&self.removed, |w, n| w.pstring(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{read_body, write_blob};

    fn round_trip<M: Message + PartialEq + std::fmt::Debug>(m: &M, opcode: u32) {
        let blob = write_blob(m).unwrap();
        assert_eq!(&blob[0..4], &opcode.to_le_bytes());
        assert_eq!(&read_body::<M>(&blob[4..]).unwrap(), m);
    }

    #[test]
    fn each_message_reads_back_as_written() {
        round_trip(
            &DerethExtensions {
                names: vec![AI_FELLOWS.to_owned(), ADVOCATE_OFFERS.to_owned()],
            },
            0xF7EF,
        );
        round_trip(
            &AiFellows {
                fellows: vec![AiFellow {
                    guid: 0x5000_0002,
                    name: "Aerin".to_owned(),
                    mode: "heal".to_owned(),
                    in_fellowship: true,
                }],
            },
            0xF7F0,
        );
        round_trip(
            &Waypoint {
                from: 0x5000_0003,
                from_name: "Advocate Tamsin".to_owned(),
                label: "Holtburg square".to_owned(),
                cell: 0xA9B4_0031,
                x: 84.0,
                y: 7.5,
                z: 94.0,
            },
            0xF7F1,
        );
        round_trip(
            &JournalEntry {
                from: 0x5000_0003,
                from_name: "Advocate Tamsin".to_owned(),
                title: "Drudges".to_owned(),
                text: "South of town.".to_owned(),
                location: None,
            },
            0xF7F2,
        );
        round_trip(
            &QuestTimers {
                now: 1_700_000_000,
                whole: true,
                timers: vec![QuestTimer {
                    name: "aerbaxdefeat".to_owned(),
                    message: "Aerbax's Defeat".to_owned(),
                    solves: 3,
                    max_solves: -1,
                    min_delta: 72_000,
                    last_solved: 1_699_990_000,
                    ready_at: 1_700_062_000,
                }],
                removed: Vec::new(),
            },
            0xF7F3,
        );
        round_trip(
            &QuestTimers {
                now: 1_700_000_100,
                whole: false,
                timers: Vec::new(),
                removed: vec!["aerbaxdefeat".to_owned()],
            },
            0xF7F3,
        );
    }

    #[test]
    fn a_journal_entry_s_location_reads_back_and_older_ends_tolerate_it() {
        let plain = JournalEntry {
            from: 0x5000_0003,
            from_name: "Advocate Tamsin".to_owned(),
            title: "To Fort Tethana (1/2)".to_owned(),
            text: "Step to the Portal to Town Network.".to_owned(),
            location: None,
        };
        let placed = JournalEntry {
            location: Some((42.3, -33.3)),
            ..plain.clone()
        };
        round_trip(&placed, 0xF7F2);
        // Without a location it is the old message; with one, the old message and then more.
        let (old, new) = (write_blob(&plain).unwrap(), write_blob(&placed).unwrap());
        assert_eq!(&new[..old.len()], &old[..]);
        assert_eq!(new.len(), old.len() + 12);
        // An older sender's entry, padded, has no location.
        let mut padded = old[4..].to_vec();
        padded.extend([0; 3]);
        assert_eq!(
            crate::read_body_padded::<JournalEntry>(&padded).unwrap(),
            plain
        );
    }
}
