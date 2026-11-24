//! World entry on the 1999 wire: `0xF7C8` asks, `0xF7C7` says the server is ready, `0xF657` picks
//! the character and the server starts the world; `0xF653` from the server brings the player back
//! to the roster.
//!
//! The server creates every object again on each entry, so [`Entry::entries`] tells the other
//! modules when what they hold per object is stale.

use pyreal_primitives::NetQueue;
use pyreal_protocol::login::CharacterError;
use pyreal_protocol::Opcode;

use crate::messages::{self, Roster, RosterRow};
use crate::strings::ReadError;
use crate::translate::{Context, Translate};

/// `0xF7C8`, the enter-world request: empty, as on the later wire.
pub const ENTER_REQUEST: u32 = 0xF7C8;
/// `0xF653`, log-off: empty from the server.
pub const LOG_OFF: u32 = 0xF653;

/// The later character error for a character not in the roster.
pub const NOT_OWNED: u32 = 15;
/// The later character error for a character counting down to deletion.
pub const LOCKED: u32 = 23;

/// Where the entry exchange stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    /// On the character screen.
    #[default]
    Roster,
    /// `0xF7C8` sent.
    Requested,
    /// `0xF7C7` received: the character may be picked.
    Ready,
    /// `0xF657` sent for this character: the server is starting the world.
    InWorld(u32),
    /// `0xF653` sent; the server's answer returns to the roster.
    LoggingOff,
}

/// One connection's world entry. No sockets and no clock.
#[derive(Debug, Default)]
pub struct Entry {
    phase: Phase,
    rows: Vec<RosterRow>,
    entries: u32,
}

impl Entry {
    #[must_use]
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// How many times a character has been sent into the world on this connection.
    #[must_use]
    pub fn entries(&self) -> u32 {
        self.entries
    }

    /// The character in the world, if any.
    #[must_use]
    pub fn character(&self) -> Option<u32> {
        match self.phase {
            Phase::InWorld(id) => Some(id),
            _ => None,
        }
    }

    /// A roster arrived: it replaces the list a character is checked against.
    pub fn on_roster(&mut self, roster: &Roster) {
        self.rows.clone_from(&roster.rows);
    }

    /// `0xF7C8` is going out.
    pub fn request(&mut self) {
        self.phase = Phase::Requested;
    }

    /// `0xF7C7` arrived.
    pub fn on_ready(&mut self) {
        if self.phase == Phase::Requested {
            self.phase = Phase::Ready;
        }
    }

    /// `0xF657` for `id` is about to go out: the character must be a row of the latest roster and
    /// not counting down to deletion.
    ///
    /// # Errors
    /// The later character error to show instead: [`NOT_OWNED`] or [`LOCKED`].
    pub fn select(&mut self, id: u32) -> Result<(), u32> {
        match self.rows.iter().find(|row| row.id == id) {
            None => Err(NOT_OWNED),
            Some(row) if row.deletion != 0 => Err(LOCKED),
            Some(_) => {
                self.phase = Phase::InWorld(id);
                self.entries += 1;
                Ok(())
            }
        }
    }

    /// `0xF653` is going out.
    pub fn log_off(&mut self) {
        if matches!(self.phase, Phase::InWorld(_)) {
            self.phase = Phase::LoggingOff;
        }
    }

    /// `0xF653` arrived from the server: back to the roster, which follows.
    pub fn on_logged_off(&mut self) {
        self.phase = Phase::Roster;
    }
}

/// It watches the roster and entry-ready, which the login area answers, so it reports them as not
/// its own; it answers the server's log-off, and refuses a pick the roster does not allow.
impl Translate for Entry {
    fn down(
        &mut self,
        opcode: u32,
        _queue: u16,
        message: &[u8],
        cx: &mut Context<'_>,
    ) -> Result<bool, ReadError> {
        match opcode {
            messages::ROSTER => {
                if let Ok(roster) = Roster::read(message) {
                    self.on_roster(&roster);
                }
                Ok(false)
            }
            messages::ENTRY_READY => {
                self.on_ready();
                Ok(false)
            }
            LOG_OFF => {
                self.on_logged_off();
                cx.push_raw(Opcode::LOGIN_EXECUTE_LOG_OFF, NetQueue::UiQueue, Vec::new());
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn up(
        &mut self,
        opcode: Opcode,
        _queue: NetQueue,
        _ordered: bool,
        payload: &[u8],
        cx: &mut Context<'_>,
    ) -> bool {
        match opcode {
            Opcode::LOGIN_SEND_ENTER_WORLD_REQUEST => self.request(),
            Opcode::LOGIN_EXECUTE_LOG_OFF => self.log_off(),
            Opcode::LOGIN_SEND_ENTER_WORLD => {
                let id = payload
                    .get(4..8)
                    .map_or(0, |w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]));
                if let Err(char_error) = self.select(id) {
                    tracing::warn!("1999: {id:#010X} cannot enter the world: error {char_error}");
                    if let Err(e) = cx.push(NetQueue::UiQueue, &CharacterError { char_error }) {
                        tracing::warn!("1999: the character error did not encode: {e}");
                    }
                    return true;
                }
            }
            _ => {}
        }
        // The login area sends them.
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster() -> Roster {
        Roster {
            rows: vec![
                RosterRow {
                    id: 0x5000_0001,
                    name: "Ann".to_owned(),
                    deletion: 0,
                },
                RosterRow {
                    id: 0x5000_0002,
                    name: "Bo".to_owned(),
                    deletion: 3600,
                },
            ],
            account: "demo".to_owned(),
        }
    }

    #[test]
    fn entry_runs_request_ready_select_and_back() {
        let mut e = Entry::default();
        e.on_roster(&roster());
        e.on_ready();
        assert_eq!(e.phase(), Phase::Roster, "ready without a request");
        e.request();
        e.on_ready();
        assert_eq!(e.phase(), Phase::Ready);
        assert_eq!(e.select(0x5000_0001), Ok(()));
        assert_eq!(e.character(), Some(0x5000_0001));
        assert_eq!(e.entries(), 1);
        e.log_off();
        assert_eq!(e.phase(), Phase::LoggingOff);
        e.on_logged_off();
        assert_eq!(e.phase(), Phase::Roster);
        e.request();
        e.on_ready();
        assert_eq!(e.select(0x5000_0001), Ok(()));
        assert_eq!(e.entries(), 2, "each entry starts the world again");
    }

    #[test]
    fn only_a_listed_character_not_being_deleted_is_sent_in() {
        let mut e = Entry::default();
        assert_eq!(e.select(0x5000_0001), Err(NOT_OWNED), "no roster yet");
        e.on_roster(&roster());
        assert_eq!(e.select(0x5000_0009), Err(NOT_OWNED));
        assert_eq!(e.select(0x5000_0002), Err(LOCKED));
        assert_eq!(e.entries(), 0);
        let mut fresh = roster();
        fresh.rows.remove(0);
        e.on_roster(&fresh);
        assert_eq!(
            e.select(0x5000_0001),
            Err(NOT_OWNED),
            "the latest roster rules"
        );
    }

    fn parts() -> (
        std::collections::VecDeque<pyreal_primitives::IncomingMessage>,
        crate::conn::Connection,
        crate::translate::Shared,
    ) {
        let cred = crate::login::Credential::new("Demo", "secret").unwrap();
        let shared = crate::translate::Shared {
            account: "demo".to_owned(),
            world_name: "AC1999".to_owned(),
            ..Default::default()
        };
        (
            Default::default(),
            crate::conn::Connection::new(cred, (102, 67)),
            shared,
        )
    }

    #[test]
    fn through_the_link_a_refused_pick_is_answered_and_log_off_returns() {
        let (mut inbox, mut conn, mut shared) = parts();
        let mut cx = Context {
            inbox: &mut inbox,
            conn: &mut conn,
            shared: &mut shared,
            wrapped: None,
        };
        let mut e = Entry::default();
        assert_eq!(
            e.down(messages::ROSTER, 4, &roster().write(), &mut cx),
            Ok(false)
        );
        assert!(!e.up(
            Opcode::LOGIN_SEND_ENTER_WORLD_REQUEST,
            NetQueue::Logon,
            false,
            &[0xC8, 0xF7, 0, 0],
            &mut cx
        ));
        assert_eq!(
            e.down(messages::ENTRY_READY, 4, &[0xC7, 0xF7, 0, 0], &mut cx),
            Ok(false)
        );
        assert_eq!(e.phase(), Phase::Ready);
        let pick = |id: u32| {
            let mut m = vec![0x57, 0xF6, 0, 0];
            m.extend_from_slice(&id.to_le_bytes());
            m
        };
        assert!(e.up(
            Opcode::LOGIN_SEND_ENTER_WORLD,
            NetQueue::Logon,
            false,
            &pick(0x5000_0002),
            &mut cx
        ));
        assert!(!e.up(
            Opcode::LOGIN_SEND_ENTER_WORLD,
            NetQueue::Logon,
            false,
            &pick(0x5000_0001),
            &mut cx
        ));
        assert_eq!(e.down(LOG_OFF, 4, &[0x53, 0xF6, 0, 0], &mut cx), Ok(true));
        assert_eq!(e.phase(), Phase::Roster);
        let got: Vec<_> = inbox.iter().map(|m| (m.opcode, m.body.clone())).collect();
        assert_eq!(
            got,
            [
                (0xF659, LOCKED.to_le_bytes().to_vec()),
                (0xF653, Vec::new())
            ]
        );
    }
}
