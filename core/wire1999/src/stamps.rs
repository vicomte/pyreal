//! The numbers the 1999 wire lacks: game-event stamps and property sequence bytes. The modules
//! reach them through [`crate::translate::Shared::stamps`].
//!
//! Pyreal's UI queue holds back a game event whose stamp leaves a gap for its object, and the layer
//! drops, splits and invents events, so every game event it hands up gets a fresh gap-free stamp
//! against the player. The server's own `0xF7B0` stamps are checked by the link and never passed
//! on.

use std::collections::BTreeMap;

use pyreal_primitives::{IncomingMessage, NetBlobId, NetQueue, ObjectId, RecipientId};
use pyreal_protocol::events::{is_game_event, pack_event};
use pyreal_protocol::{write_blob, Message, MessageError};

/// The layer's numbering for one 1999 connection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stamps {
    /// The `0xF746` player id; 0 before it.
    player: u32,
    /// The last stamp the layer gave a game event, per object.
    events: BTreeMap<u32, u32>,
    /// The last property sequence byte, per (object, property key).
    properties: BTreeMap<(u32, u32), u8>,
}

impl Stamps {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the player id from `0xF746`.
    pub fn set_player(&mut self, id: u32) {
        self.player = id;
    }

    /// The player id, or 0 before `0xF746`.
    #[must_use]
    pub fn player(&self) -> u32 {
        self.player
    }

    /// The next game-event stamp for `iid`: 1, 2, 3 and so on, never 0.
    pub fn next_event(&mut self, iid: u32) -> u32 {
        let s = self.events.entry(iid).or_default();
        *s = s.wrapping_add(1).max(1);
        *s
    }

    /// The next later property sequence byte for (`object`, `key`), starting at 1. `key` is
    /// Pyreal's gate key, `property_id | (stat_type << 16)`.
    pub fn next_property(&mut self, object: u32, key: u32) -> u8 {
        let s = self.properties.entry((object, key)).or_default();
        *s = s.wrapping_add(1);
        *s
    }

    /// Pyreal dropped this object's windows and gates (a delete, or a re-create with a newer
    /// instance): its counters start again.
    pub fn forget_object(&mut self, id: u32) {
        self.events.remove(&id);
        self.properties.retain(|&(object, _), _| object != id);
    }

    /// The enter-world request went out: the server makes every object again and Pyreal tears its
    /// world view down.
    pub fn enter_world(&mut self) {
        self.player = 0;
        self.events.clear();
        self.properties.clear();
    }

    /// A later message ready for the session's inbox. A game event is wrapped in `0xF7B0` against
    /// the player with the next stamp; anything else goes bare on the queue Pyreal's opcode table
    /// gives it.
    ///
    /// # Errors
    /// [`MessageError`] when the message does not encode.
    pub fn deliver<M: Message>(&mut self, m: &M) -> Result<IncomingMessage, MessageError> {
        if is_game_event(M::OPCODE) {
            let iid = self.player;
            let stamp = self.next_event(iid);
            let blob = pack_event(ObjectId(iid), stamp, m)?;
            return Ok(incoming(NetQueue::UiQueue, &blob));
        }
        let queue = M::OPCODE
            .info()
            .and_then(|i| i.recv_queue)
            .unwrap_or(NetQueue::UiQueue);
        Ok(incoming(queue, &write_blob(m)?))
    }
}

/// A whole blob as the session receives it: the opcode lifted off.
fn incoming(queue: NetQueue, blob: &[u8]) -> IncomingMessage {
    IncomingMessage {
        opcode: u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]),
        queue,
        sender: RecipientId(0),
        blob_id: NetBlobId(0),
        body: blob[4..].to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyreal_protocol::events::split_ui_blob;
    use pyreal_protocol::objects::ItemDeleteObject;
    use pyreal_protocol::qualities::{PrivateUpdate, QualitiesPrivateUpdateInt};

    #[test]
    fn event_stamps_run_one_two_three_per_object_and_restart_when_forgotten() {
        let mut s = Stamps::new();
        assert_eq!(
            [s.next_event(7), s.next_event(7), s.next_event(9)],
            [1, 2, 1]
        );
        assert_eq!(s.next_event(7), 3);
        s.forget_object(7);
        assert_eq!(s.next_event(7), 1);
        s.enter_world();
        assert_eq!(s.next_event(9), 1);
    }

    #[test]
    fn property_bytes_count_per_key_and_wrap_and_start_again_at_enter_world() {
        let mut s = Stamps::new();
        assert_eq!(s.next_property(1, 2), 1);
        assert_eq!(s.next_property(1, 2), 2);
        assert_eq!(s.next_property(1, 3), 1);
        for _ in 0..254 {
            s.next_property(1, 2);
        }
        assert_eq!(s.next_property(1, 2), 1, "256 then wraps past 0");
        s.enter_world();
        assert_eq!(s.next_property(1, 3), 1);
    }

    #[test]
    fn a_game_event_is_wrapped_against_the_player_and_others_go_bare_on_their_queue() {
        let mut s = Stamps::new();
        let text = QualitiesPrivateUpdateInt(PrivateUpdate {
            sequence: 1,
            property_id: 25,
            value: 126,
        });
        let early = s.deliver(&text).unwrap();
        assert_eq!((early.opcode, early.queue), (0xF7B0, NetQueue::UiQueue));
        s.set_player(0x5000_0001);
        let a = s.deliver(&text).unwrap();
        let b = s.deliver(&text).unwrap();
        for (m, want_iid, want_stamp) in
            [(&early, 0, 1), (&a, 0x5000_0001, 1), (&b, 0x5000_0001, 2)]
        {
            let mut blob = m.opcode.to_le_bytes().to_vec();
            blob.extend_from_slice(&m.body);
            let ui = split_ui_blob(&blob).unwrap();
            let order = ui.order.unwrap();
            assert_eq!((order.iid.0, order.stamp), (want_iid, want_stamp));
            assert_eq!(ui.sub_type, QualitiesPrivateUpdateInt::OPCODE);
        }

        let gone = s
            .deliver(&ItemDeleteObject {
                id: ObjectId(5),
                instance_sequence: 1,
            })
            .unwrap();
        assert_eq!((gone.opcode, gone.queue), (0xF747, NetQueue::WorldObjects));
    }
}
