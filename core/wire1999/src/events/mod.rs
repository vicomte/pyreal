//! Server-to-client game events: the 1999 messages of the player area become the later messages
//! Pyreal already dispatches, with the state the translation needs ([`EventState`]).
//!
//! Hand [`EventState::translate`] one 1999 message, opcode first, after the link has unwrapped any
//! `0xF7B0` and dropped its stale stamps. [`Later::deliver`] then gives each later game event a
//! fresh gap-free stamp against the player, so no 1999 stamp reaches Pyreal's stamp window.

pub mod codec;

mod chat;
mod combat;
pub mod items;
mod magic;
mod social;

use std::collections::BTreeMap;

use pyreal_primitives::NetQueue;
use pyreal_protocol::{events::is_game_event, write_body, Message, MessageError, Opcode};

use crate::link::WRAPPER;
use crate::player::{PlayerDescription, PLAYER_DESCRIPTION};
use crate::qualities::{self, QualityUpdate};
use crate::strings::ReadError;
use crate::translate::{Context, Shared, Translate};

/// One later message: its opcode and the body after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Later {
    pub opcode: u32,
    pub body: Vec<u8>,
}

impl Later {
    /// # Errors
    /// [`MessageError`] when `m` does not encode.
    pub fn of<M: Message>(m: &M) -> Result<Self, MessageError> {
        Ok(Self {
            opcode: M::OPCODE.0,
            body: write_body(m)?,
        })
    }

    /// A message whose body is the 1999 body unchanged, under the later opcode.
    #[must_use]
    pub fn same(opcode: u32, message: &[u8]) -> Self {
        Self {
            opcode,
            body: message[4..].to_vec(),
        }
    }

    /// The queue Pyreal drains this message from.
    #[must_use]
    pub fn queue(&self) -> NetQueue {
        Opcode(self.opcode)
            .info()
            .and_then(|i| i.recv_queue)
            .unwrap_or(NetQueue::UiQueue)
    }

    /// Whether the later client takes it as a game event.
    #[must_use]
    pub fn is_game_event(&self) -> bool {
        is_game_event(Opcode(self.opcode))
    }

    /// Hands it to the session as [`crate::stamps::Stamps::deliver`] does: a game event inside
    /// `0xF7B0` against the player with the next gap-free stamp, anything else bare on its queue.
    pub fn deliver(self, cx: &mut Context<'_>) {
        if !self.is_game_event() {
            let queue = self.queue();
            cx.push_raw(Opcode(self.opcode), queue, self.body);
            return;
        }
        let stamps = &mut cx.shared.stamps;
        let iid = stamps.player();
        let stamp = stamps.next_event(iid);
        let mut body = Vec::with_capacity(12 + self.body.len());
        for w in [iid, stamp, self.opcode] {
            body.extend_from_slice(&w.to_le_bytes());
        }
        body.extend_from_slice(&self.body);
        cx.push_raw(Opcode(WRAPPER), NetQueue::UiQueue, body);
    }
}

/// What a handler made of one message.
#[derive(Debug)]
enum Handled {
    Out(Vec<Later>),
    /// Dropped on purpose: a later-only system a Launch world lacks.
    Drop,
}

impl From<Later> for Handled {
    fn from(l: Later) -> Self {
        Self::Out(vec![l])
    }
}

/// What became of the messages of one opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    pub taken: u32,
    pub dropped: u32,
    pub malformed: u32,
}

/// Per-opcode counts, so a real session can show what is missing.
#[derive(Debug, Clone, Default)]
pub struct Tally(BTreeMap<u32, Counts>);

impl Tally {
    #[must_use]
    pub fn get(&self, opcode: u32) -> Counts {
        self.0.get(&opcode).copied().unwrap_or_default()
    }

    pub fn iter(&self) -> impl Iterator<Item = (u32, Counts)> + '_ {
        self.0.iter().map(|(k, v)| (*k, *v))
    }
}

/// The 1999 per-character squelch: object id to name.
type Squelches = BTreeMap<u32, String>;

/// This area's own state, owned by the link. The player id, stamps and property bytes live in
/// [`Shared::stamps`]; the option word, squelched ids and open questions in [`Shared`] for the
/// outbound module.
#[derive(Debug, Clone)]
pub struct EventState {
    /// The squelched characters' names, which the later squelch list carries.
    squelch: Squelches,
    tally: Tally,
    /// The 1999 public description reader vendor lists use.
    description: items::DescriptionReader,
}

impl Default for EventState {
    fn default() -> Self {
        Self {
            squelch: Squelches::new(),
            tally: Tally::default(),
            description: crate::objects::read_description,
        }
    }
}

impl EventState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the 1999 public description reader that vendor lists use.
    pub fn set_description_reader(&mut self, read: items::DescriptionReader) {
        self.description = read;
    }

    #[must_use]
    pub fn tally(&self) -> &Tally {
        &self.tally
    }

    /// [`Self::translate`] for a message that came inside `0xF7B0`. A wrapped `0x0029` is the
    /// title table, never a vital, and is dropped.
    pub fn translate_wrapped(&mut self, message: &[u8], shared: &mut Shared) -> Option<Vec<Later>> {
        if message.get(..4) == Some(&qualities::UPDATE_VITAL.to_le_bytes()[..]) {
            self.tally
                .0
                .entry(qualities::UPDATE_VITAL)
                .or_default()
                .dropped += 1;
            return Some(Vec::new());
        }
        self.translate(message, shared)
    }

    /// The later messages for one 1999 message, or `None` when it is not this area's.
    ///
    /// A message that does not read is counted and logged, and gives nothing.
    pub fn translate(&mut self, message: &[u8], shared: &mut Shared) -> Option<Vec<Later>> {
        let opcode = u32::from_le_bytes(message.get(..4)?.try_into().ok()?);
        let handled = self.handle(opcode, message, shared)?;
        let counts = self.tally.0.entry(opcode).or_default();
        Some(match handled {
            Ok(Handled::Out(out)) => {
                counts.taken += 1;
                out
            }
            Ok(Handled::Drop) => {
                counts.dropped += 1;
                Vec::new()
            }
            Err(e) => {
                counts.malformed += 1;
                tracing::warn!(
                    "1999: {opcode:#06X} of {} bytes did not read: {e}",
                    message.len()
                );
                Vec::new()
            }
        })
    }

    fn handle(
        &mut self,
        opcode: u32,
        message: &[u8],
        shared: &mut Shared,
    ) -> Option<Result<Handled, MessageError>> {
        let player = Some(shared.stamps.player()).filter(|&p| p != 0);
        let stamps = &mut shared.stamps;
        if opcode == PLAYER_DESCRIPTION {
            return Some(PlayerDescription::read(message).and_then(|d| {
                shared.options_word = Some(d.module.options);
                let mut out = vec![Later::of(&d.to_later())?];
                // The squelch list follows the description once the client is in the world.
                if !self.squelch.is_empty() {
                    out.push(chat::squelch_db(&self.squelch)?);
                }
                Ok(Handled::Out(out))
            }));
        }
        if qualities::UPDATES.contains(&opcode) {
            return Some(
                QualityUpdate::read(message)
                    .and_then(|u| u.to_later(player, stamps))
                    .map(Handled::from),
            );
        }
        if opcode == qualities::STACK_SIZE {
            return Some(qualities::stack_size(message, stamps).map(Handled::from));
        }
        if qualities::later_form(opcode).is_some() {
            return qualities::pass_later_form(message, player, stamps)
                .map(|l| l.map(Handled::from))
                .transpose();
        }
        if opcode == chat::QUESTION {
            shared.questions_owed += 1;
            return Some(Ok(Later::same(opcode, message).into()));
        }
        if DROPPED.contains(&opcode) || (0x01FD..=0x0207).contains(&opcode) {
            return Some(Ok(Handled::Drop));
        }
        magic::translate(opcode, message)
            .or_else(|| chat::translate(&mut self.squelch, shared, opcode, message))
            .or_else(|| combat::translate(opcode, message))
            .or_else(|| items::translate(opcode, message, self.description))
            .or_else(|| social::translate(opcode, message))
    }
}

impl Translate for EventState {
    fn down(
        &mut self,
        _opcode: u32,
        _queue: u16,
        message: &[u8],
        cx: &mut Context<'_>,
    ) -> Result<bool, ReadError> {
        let out = if cx.wrapped.is_some() {
            self.translate_wrapped(message, cx.shared)
        } else {
            self.translate(message, cx.shared)
        };
        let Some(out) = out else {
            return Ok(false);
        };
        for l in out {
            l.deliver(cx);
        }
        Ok(true)
    }

    fn up(
        &mut self,
        _opcode: Opcode,
        _queue: NetQueue,
        _ordered: bool,
        _payload: &[u8],
        _cx: &mut Context<'_>,
    ) -> bool {
        false
    }
}

/// Later-only systems a Launch world lacks, which the server still sends: titles, barber, the page
/// change answer, houses, mana query, allegiance notices, salvage, Turbine chat, visual description.
/// Trade (`0x01FD` to `0x0207`) is dropped beside them.
const DROPPED: [u32; 15] = [
    0x002B, 0x0051, 0x0075, 0x00B5, 0x01C8, 0x021D, 0x0225, 0x0226, 0x0264, 0x0271, 0x027A, 0x027C,
    0x02B4, 0xF7DE, 0xF630,
];

#[cfg(test)]
mod tests;
