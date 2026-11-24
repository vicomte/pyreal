//! The login screens' translation: the roster, the news, character errors, entry-ready, and the
//! requests that pick a character.

use pyreal_primitives::NetQueue;
use pyreal_protocol::login::{CharacterError, LoginCharacterScreenMessage, LoginWorldInfo};
use pyreal_protocol::Opcode;

use crate::messages::{self, News, Roster};
use crate::strings::ReadError;
use crate::translate::{Context, Translate};

/// The 1999 queue for login, character and world-entry requests.
pub const LOGIN_QUEUE: u16 = 6;
/// The character slots offered when the roster has fewer rows.
pub const MIN_SLOTS: i32 = 5;

/// The login screens.
#[derive(Debug, Default)]
pub struct LoginArea {
    world_info_sent: bool,
}

fn log_encode(e: &pyreal_protocol::MessageError) {
    tracing::warn!("1999: a later login message did not encode: {e}");
}

impl Translate for LoginArea {
    fn down(
        &mut self,
        opcode: u32,
        _queue: u16,
        message: &[u8],
        cx: &mut Context<'_>,
    ) -> Result<bool, ReadError> {
        match opcode {
            messages::ROSTER => {
                let roster = Roster::read(message)?;
                roster.account.clone_into(&mut cx.shared.account);
                let slots = i32::try_from(roster.rows.len())
                    .unwrap_or(i32::MAX)
                    .max(MIN_SLOTS);
                if let Err(e) = cx.push(NetQueue::UiQueue, &roster.to_later(slots)) {
                    log_encode(&e);
                }
                if !self.world_info_sent {
                    self.world_info_sent = true;
                    let info = LoginWorldInfo {
                        connections: 0,
                        max_connections: 0,
                        world_name: cx.shared.world_name.clone(),
                    };
                    if let Err(e) = cx.push(NetQueue::UiQueue, &info) {
                        log_encode(&e);
                    }
                }
            }
            messages::NEWS => {
                let news = News::read(message)?;
                let m = LoginCharacterScreenMessage {
                    text: news.shown,
                    more: String::new(),
                };
                if let Err(e) = cx.push(NetQueue::UiQueue, &m) {
                    log_encode(&e);
                }
            }
            messages::CHARACTER_ERROR => {
                let char_error = messages::read_character_error(message)?;
                if let Err(e) = cx.push(NetQueue::UiQueue, &CharacterError { char_error }) {
                    log_encode(&e);
                }
            }
            messages::ENTRY_READY => {
                cx.push_raw(
                    Opcode::LOGIN_ENTER_GAME_SERVER_READY,
                    NetQueue::UiQueue,
                    Vec::new(),
                );
            }
            _ => return Ok(false),
        }
        Ok(true)
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
            Opcode::LOGIN_SEND_ENTER_WORLD_REQUEST | Opcode::LOGIN_EXECUTE_LOG_OFF => {
                cx.send(LOGIN_QUEUE, payload)
            }
            Opcode::LOGIN_SEND_ENTER_WORLD => {
                let id = payload
                    .get(4..8)
                    .map_or(0, |w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]));
                let m = messages::select_character(id, &cx.shared.account);
                cx.send(LOGIN_QUEUE, &m)
            }
            _ => false,
        }
    }
}
