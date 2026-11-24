//! What each translating module implements: [`Translate`], its `down` and `up` over a
//! [`Context`].
//!
//! Each area of the protocol (the login screens, the dat push, objects, the player, outbound
//! actions) is a module with its own state struct implementing [`Translate`]. [`crate::link`]
//! keeps one of each in [`crate::link::modules`] and offers every message to them in turn.

use std::collections::VecDeque;

use pyreal_primitives::{IncomingMessage, NetBlobId, NetQueue, RecipientId};
use pyreal_protocol::{write_blob, Message, MessageError, Opcode};

use crate::conn::Connection;
use crate::stamps::Stamps;
use crate::strings::ReadError;

/// What every module may read: facts the login area learns and others need.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shared {
    /// The account name the roster gave, which every account field carries.
    pub account: String,
    /// The world's name on the character screen, which the 1999 wire never sends.
    pub world_name: String,
    /// Event stamps and property bytes for the later messages the modules hand up.
    pub stamps: Stamps,
    /// The character's 1999 options word: the player description's, then as `0x01A1` sends it.
    pub options_word: Option<u32>,
    /// The characters squelched this session, in the order squelched, uploaded at world entry.
    pub squelched: Vec<u32>,
    /// Confirmations the server asked (`0x0274`) that the player has not answered.
    pub questions_owed: u32,
}

impl Shared {
    /// The server squelched (`added`) or unsquelched the character `id`.
    pub fn note_squelch(&mut self, added: bool, id: u32) {
        self.squelched.retain(|&s| s != id);
        if added {
            self.squelched.push(id);
        }
    }
}

/// The `0xF7B0` envelope a message arrived in: the object it is ordered against and the server's
/// stamp. The link has already dropped it if the stamp was stale by the 1999 client's rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wrapped {
    pub object: u32,
    pub stamp: u32,
}

/// A module's view of the link while it translates one message: the session's inbox of later
/// messages, the connection to the server, the shared facts, and the envelope the message came in.
#[derive(Debug)]
pub struct Context<'a> {
    pub(crate) inbox: &'a mut VecDeque<IncomingMessage>,
    pub conn: &'a mut Connection,
    pub shared: &'a mut Shared,
    /// `Some` when the message came inside `0xF7B0`; `None` when it came alone.
    pub wrapped: Option<Wrapped>,
}

impl Context<'_> {
    /// Hand the session a later message on `queue`.
    ///
    /// # Errors
    /// The message's own [`MessageError`] when it does not encode.
    pub fn push<M: Message>(&mut self, queue: NetQueue, m: &M) -> Result<(), MessageError> {
        let blob = write_blob(m)?;
        self.push_raw(M::OPCODE, queue, blob[4..].to_vec());
        Ok(())
    }

    /// Hand the session a later message already encoded: its opcode and the body after it.
    pub fn push_raw(&mut self, opcode: Opcode, queue: NetQueue, body: Vec<u8>) {
        self.inbox.push_back(IncomingMessage {
            opcode: opcode.0,
            queue,
            sender: RecipientId(0),
            blob_id: NetBlobId(0),
            body,
        });
    }

    /// Send a whole 1999 message, opcode included, on a 1999 queue. `false` when the connection
    /// cannot send it.
    pub fn send(&mut self, queue: u16, message: &[u8]) -> bool {
        self.conn.send(queue, message)
    }
}

/// One area of the translation, with its own state.
///
/// Both methods answer whether the message was this module's: `Ok(false)` passes it to the next
/// module. A message no module takes is dropped and logged by the link.
pub trait Translate: std::fmt::Debug {
    /// A whole 1999 message from the server, opcode included, which arrived on 1999 `queue`.
    ///
    /// # Errors
    /// [`ReadError`] for a message of this module's that did not read; the link logs it.
    fn down(
        &mut self,
        opcode: u32,
        queue: u16,
        message: &[u8],
        cx: &mut Context<'_>,
    ) -> Result<bool, ReadError>;

    /// A later message the session sends, opcode included, on the later `queue`.
    fn up(
        &mut self,
        opcode: Opcode,
        queue: NetQueue,
        ordered: bool,
        payload: &[u8],
        cx: &mut Context<'_>,
    ) -> bool;

    /// A character is about to enter the world (`0xF657` is going out): the server creates every
    /// object again and restarts their event stamps, so per-object state goes.
    fn entering_world(&mut self) {}
}
