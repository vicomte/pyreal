//! [`Link1999`]: the session's [`Transport`] over a 1999 [`Connection`].
//!
//! Inbound, each finished 1999 message is offered to the translating modules ([`modules`]) in
//! turn, and the first that takes it hands the session the later messages Pyreal already
//! dispatches. Outbound, each later message the session sends is offered to them the same way. A
//! message no module takes is dropped and logged.

use std::collections::{BTreeMap, VecDeque};

use pyreal_primitives::{IncomingMessage, NetQueue, Transport};
use pyreal_protocol::Opcode;

use crate::chargen::Characters;
use crate::conn::Connection;
use crate::dats::DatPush;
use crate::entry::Entry;
use crate::events::EventState;
use crate::login_area::LoginArea;
use crate::outbound::Outbound;
use crate::translate::{Context, Shared, Translate, Wrapped};
use crate::world::World;

/// `0xF7B0`, which orders one whole message against one object.
pub const WRAPPER: u32 = 0xF7B0;

pub use crate::login_area::{LOGIN_QUEUE, MIN_SLOTS};

/// The translating modules, in the order a message is offered to them. Each area of the protocol
/// adds its own here.
#[must_use]
pub fn modules() -> Vec<Box<dyn Translate>> {
    vec![
        // Ahead of the login area: it watches the roster and entry messages that area answers.
        Box::new(Entry::default()),
        Box::new(LoginArea::default()),
        Box::new(DatPush::default()),
        Box::new(Characters),
        Box::new(EventState::default()),
        Box::new(World::default()),
        Box::new(Outbound::default()),
    ]
}

/// The session's transport over a 1999 connection.
#[derive(Debug)]
pub struct Link1999 {
    conn: Connection,
    inbox: VecDeque<IncomingMessage>,
    shared: Shared,
    modules: Vec<Box<dyn Translate>>,
    /// The last `0xF7B0` stamp taken per object, cleared as a character enters the world.
    stamps: BTreeMap<u32, u32>,
    /// 1999 messages dropped, for a log line.
    dropped: u32,
}

impl Link1999 {
    /// A link over `conn` to the world called `world_name`.
    #[must_use]
    pub fn new(conn: Connection, world_name: &str) -> Self {
        let shared = Shared {
            account: conn.credential().account().to_owned(),
            world_name: world_name.to_owned(),
            ..Shared::default()
        };
        Self {
            conn,
            inbox: VecDeque::new(),
            shared,
            modules: modules(),
            stamps: BTreeMap::new(),
            dropped: 0,
        }
    }

    #[must_use]
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// What the modules share: the roster's account name, the world's name.
    #[must_use]
    pub fn shared(&self) -> &Shared {
        &self.shared
    }

    /// 1999 messages this layer dropped.
    #[must_use]
    pub fn dropped(&self) -> u32 {
        self.dropped
    }

    /// Turn the connection's finished messages into later ones.
    fn pump(&mut self) {
        while let Some((queue, message)) = self.conn.next_message() {
            self.translate(queue, &message);
        }
    }

    fn translate(&mut self, queue: u16, message: &[u8]) {
        let Some(opcode) = message
            .get(..4)
            .map(|o| u32::from_le_bytes([o[0], o[1], o[2], o[3]]))
        else {
            return;
        };
        let (opcode, message, wrapped) = if opcode == WRAPPER {
            match self.unwrap(message) {
                Some((inner, w)) => (
                    u32::from_le_bytes([inner[0], inner[1], inner[2], inner[3]]),
                    inner,
                    Some(w),
                ),
                None => return,
            }
        } else {
            (opcode, message, None)
        };
        let Self {
            conn,
            inbox,
            shared,
            modules,
            dropped,
            ..
        } = self;
        let mut cx = Context {
            inbox,
            conn,
            shared,
            wrapped,
        };
        for m in modules.iter_mut() {
            match m.down(opcode, queue, message, &mut cx) {
                Ok(false) => {}
                Ok(true) => return,
                Err(e) => {
                    tracing::warn!("1999: {opcode:#06X} did not read: {e:?}");
                    *dropped += 1;
                    return;
                }
            }
        }
        tracing::debug!("1999: {opcode:#06X} on queue {queue} has no translation yet");
        *dropped += 1;
    }

    /// `0xF7B0`: object id, stamp, then a whole 1999 message. `None` for one cut short, one
    /// wrapping another, or a stamp not newer than the last taken for its object.
    fn unwrap<'m>(&mut self, message: &'m [u8]) -> Option<(&'m [u8], Wrapped)> {
        let word = |at: usize| {
            message
                .get(at..at + 4)
                .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
        };
        let (Some(object), Some(stamp), Some(inner)) = (word(4), word(8), word(12)) else {
            tracing::warn!("1999: 0xF7B0 cut short");
            self.dropped += 1;
            return None;
        };
        if inner == WRAPPER {
            tracing::warn!("1999: 0xF7B0 inside 0xF7B0");
            self.dropped += 1;
            return None;
        }
        if let Some(&last) = self.stamps.get(&object) {
            if i32::from_le_bytes(stamp.wrapping_sub(last).to_le_bytes()) <= 0 {
                tracing::debug!("1999: stale 0xF7B0 for {object:#010X}: {stamp} after {last}");
                self.dropped += 1;
                return None;
            }
        }
        self.stamps.insert(object, stamp);
        Some((&message[12..], Wrapped { object, stamp }))
    }
}

impl Transport for Link1999 {
    fn send(&mut self, queue: NetQueue, ordered: bool, payload: &[u8]) {
        let Some(opcode) = payload
            .get(..4)
            .map(|o| Opcode(u32::from_le_bytes([o[0], o[1], o[2], o[3]])))
        else {
            return;
        };
        let Self {
            conn,
            inbox,
            shared,
            modules,
            stamps,
            ..
        } = self;
        let mut cx = Context {
            inbox,
            conn,
            shared,
            wrapped: None,
        };
        let sent = modules
            .iter_mut()
            .any(|m| m.up(opcode, queue, ordered, payload, &mut cx));
        if !sent {
            tracing::debug!("1999: {:#06X} not sent", opcode.0);
        } else if opcode == Opcode::LOGIN_SEND_ENTER_WORLD {
            stamps.clear();
            cx.shared.stamps.enter_world();
            // The character picked is the player: AC1999 can send its first updates before
            // `0xF746`, which then only confirms it.
            if let Some(id) = payload
                .get(4..8)
                .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
                .filter(|&id| id != 0)
            {
                cx.shared.stamps.set_player(id);
            }
            for m in modules.iter_mut() {
                m.entering_world();
            }
        }
    }

    fn poll(&mut self) -> Option<IncomingMessage> {
        if self.inbox.is_empty() {
            self.pump();
        }
        self.inbox.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::login::Credential;
    use crate::messages::{News, Roster, RosterRow};
    use pyreal_protocol::login::{LoginCharacterScreenMessage, LoginCharacterSet, LoginWorldInfo};
    use pyreal_protocol::read_body;

    fn link() -> Link1999 {
        let conn = Connection::new(Credential::new("Demo", "secret").unwrap(), (102, 67));
        Link1999::new(conn, "AC1999")
    }

    #[test]
    fn the_roster_becomes_the_character_set_then_the_world_info() {
        let mut l = link();
        let roster = Roster {
            rows: vec![RosterRow {
                id: 0x5000_0001,
                name: "Ann".to_owned(),
                deletion: 0,
            }],
            account: "demo".to_owned(),
        };
        l.translate(4, &roster.write());
        l.translate(4, &roster.write());
        let first = l.inbox.pop_front().unwrap();
        assert_eq!(first.opcode, 0xF658);
        assert_eq!(first.queue, NetQueue::UiQueue);
        let set: LoginCharacterSet = read_body(&first.body).unwrap();
        assert_eq!(set.characters[0].name, "Ann");
        assert_eq!(set.num_allowed_characters, 5);
        let info = l.inbox.pop_front().unwrap();
        assert_eq!(info.opcode, 0xF7E1);
        let info: LoginWorldInfo = read_body(&info.body).unwrap();
        assert_eq!((info.connections, info.max_connections), (0, 0));
        assert_eq!(info.world_name, "AC1999");
        assert_eq!(l.inbox.pop_front().map(|m| m.opcode), Some(0xF658), "once");
        assert!(l.inbox.is_empty());
    }

    #[test]
    fn news_shows_the_second_string_and_errors_and_entry_pass_on() {
        let mut l = link();
        let news = News {
            first: "AC1999".to_owned(),
            shown: "Welcome".to_owned(),
        };
        l.translate(4, &news.write());
        let m = l.inbox.pop_front().unwrap();
        assert_eq!(m.opcode, 0xF65A);
        let m: LoginCharacterScreenMessage = read_body(&m.body).unwrap();
        assert_eq!(m.shown(), "Welcome");

        l.translate(4, &[0x59, 0xF6, 0, 0, 3, 0, 0, 0]);
        let e = l.inbox.pop_front().unwrap();
        assert_eq!((e.opcode, e.body.as_slice()), (0xF659, &[3, 0, 0, 0][..]));
        l.translate(4, &[0xC7, 0xF7, 0, 0]);
        assert_eq!(l.inbox.pop_front().map(|m| m.opcode), Some(0xF7DF));
        l.translate(7, &[0xB8, 0xF7, 0, 0]);
        assert!(l.inbox.is_empty());
        assert_eq!(l.dropped(), 1, "a begin cut short");
    }

    fn begin(portal: u32, cell: u32) -> Vec<u8> {
        [0xF7B8, 102, portal, 67, cell, 0, 0]
            .iter()
            .flat_map(|w: &u32| w.to_le_bytes())
            .collect()
    }

    #[test]
    fn an_empty_push_ends_at_the_second_begin_and_a_full_one_at_its_last_record() {
        let mut l = link();
        for _ in 0..3 {
            l.translate(7, &begin(0, 0));
        }
        let ends: Vec<_> = l.inbox.drain(..).collect();
        assert_eq!(ends.len(), 1, "once, at the second begin");
        assert_eq!(
            (ends[0].opcode, ends[0].queue),
            (0xF7EA, NetQueue::ClientCache)
        );

        let mut l = link();
        l.translate(7, &begin(1, 1));
        l.translate(7, &begin(1, 1));
        assert_eq!(
            l.inbox.pop_front().map(|m| m.opcode),
            Some(0xF7E5),
            "the patch opens"
        );
        l.translate(7, &[0xB7, 0xF7, 0, 0]);
        assert!(l.inbox.is_empty(), "one of two");
        l.translate(7, &[0xBB, 0xF7, 0, 0]);
        assert_eq!(l.inbox.pop_front().map(|m| m.opcode), Some(0xF7EA));

        let mut l = link();
        l.translate(
            7,
            &[
                0xB8, 0xF7, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0,
            ],
        );
        l.translate(7, &begin(0, 0));
        assert!(
            l.inbox.is_empty(),
            "a begin with no iterations arms nothing"
        );
    }

    /// Each message taken: its opcode and its envelope.
    type SeenLog = std::rc::Rc<std::cell::RefCell<Vec<(u32, Option<Wrapped>)>>>;

    /// Takes anything, noting whether it came wrapped.
    #[derive(Debug, Default)]
    struct Seen(SeenLog);

    impl Translate for Seen {
        fn down(
            &mut self,
            opcode: u32,
            _queue: u16,
            _message: &[u8],
            cx: &mut Context<'_>,
        ) -> Result<bool, crate::strings::ReadError> {
            self.0.borrow_mut().push((opcode, cx.wrapped));
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

    #[test]
    fn a_wrapped_message_is_offered_again_and_stale_stamps_are_dropped() {
        let mut l = link();
        let seen = Seen::default();
        let log = std::rc::Rc::clone(&seen.0);
        l.modules = vec![Box::new(seen)];
        let wrap = |object: u32, stamp: u32, inner: u32| -> Vec<u8> {
            [WRAPPER, object, stamp, inner, 7]
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .collect()
        };
        l.translate(3, &wrap(0x5000_0001, 5, 0x0029));
        l.translate(3, &wrap(0x5000_0001, 5, 0x0029));
        l.translate(3, &wrap(0x5000_0001, 4, 0x0029));
        l.translate(3, &wrap(0x5000_0002, 1, 0x0013));
        l.translate(3, &[0x29, 0, 0, 0, 7, 0, 0, 0]);
        l.translate(3, &wrap(0x5000_0001, 6, WRAPPER));
        let w = |object, stamp| Some(Wrapped { object, stamp });
        assert_eq!(
            *log.borrow(),
            [
                (0x0029, w(0x5000_0001, 5)),
                (0x0013, w(0x5000_0002, 1)),
                (0x0029, None),
            ]
        );
        assert_eq!(l.dropped(), 3, "two stale, one nested");
        l.stamps.clear();
        l.translate(3, &wrap(0x5000_0001, 1, 0x0029));
        assert_eq!(log.borrow().len(), 4, "a new entry starts the stamps again");
    }

    #[test]
    fn a_wrapped_delete_reaches_the_world_once() {
        let mut l = link();
        let wrapped = |stamp: u32| -> Vec<u8> {
            [WRAPPER, 0x5000_0001, stamp, 0xF747, 0x7000_0001, 2]
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .collect()
        };
        l.translate(4, &wrapped(9));
        l.translate(4, &wrapped(9));
        let m = l.inbox.pop_front().unwrap();
        assert_eq!((m.opcode, m.queue), (0xF747, NetQueue::WorldObjects));
        assert!(l.inbox.is_empty(), "the repeat is stale");
    }

    #[test]
    fn the_identity_reaches_the_shared_stamps() {
        let mut l = link();
        l.translate(3, &[0x46, 0xF7, 0, 0, 1, 0, 0, 0x50]);
        assert_eq!(l.inbox.pop_front().map(|m| m.opcode), Some(0xF746));
        assert_eq!(l.shared.stamps.player(), 0x5000_0001);
    }

    #[test]
    fn the_character_picked_is_the_player_before_its_identity_comes() {
        let mut c = Connection::new(Credential::new("demo", "secret").unwrap(), (102, 67));
        c.tick(0.0);
        let _ = c.take_outgoing();
        c.feed(&crate::login::tests::grant_datagram(b"4.35752", 0, 0), 0.1)
            .expect("granted");
        let mut l = Link1999::new(c, "AC1999");
        l.shared.stamps.set_player(0x5000_0009);
        let pick = pyreal_protocol::write_blob(&pyreal_protocol::login::LoginSendEnterWorld {
            character: pyreal_primitives::ObjectId(0x5000_0006),
            account: "demo".to_owned(),
        })
        .unwrap();
        l.send(NetQueue::Logon, false, &pick);
        assert_eq!(l.shared.stamps.player(), 0x5000_0006);
    }

    #[test]
    fn nothing_is_sent_before_the_grant() {
        let mut l = link();
        l.send(NetQueue::Logon, false, &[0xC8, 0xF7, 0, 0]);
        assert!(l.connection_mut().take_outgoing().is_empty());
    }
}
