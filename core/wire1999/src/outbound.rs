//! [`Outbound`]: what the client sends once a character is in the world.
//!
//! Every game action goes up in the ordered envelope (`0xF7B1`, order number, action) on the 1999
//! weenie queue, numbered by this module's own counter: an action refused or dropped takes no
//! number. Each action Pyreal sends is passed with its body unchanged, rebuilt in its 1999 form,
//! refused with a chat line, or dropped without one. The few bare requests the 1999 client sends
//! go up on its control queue.
//!
//! The server reads two kinds of string. Most fields are read as an aligned string: after an
//! empty one the server still moves to the next four-byte boundary. Speech, squelch names and
//! book pages are read as the 1999 string, whose empty form is its two length bytes alone.

use pyreal_primitives::text::cp1252;
use pyreal_primitives::NetQueue;
use pyreal_protocol::actions::unpack_action;
use pyreal_protocol::comms::{
    CommunicationChannelBroadcast, CommunicationModifyCharacterSquelch, CommunicationTalkDirect,
    CommunicationTalkDirectByName, CommunicationTextboxString,
};
use pyreal_protocol::social::{FellowshipCreate, SocialAddFriend, SocialSendFriendsCommand};
use pyreal_protocol::trade::{VendorBuy, WritingBookModifyPage, WritingSetInscription};
use pyreal_protocol::{Message, MessageError, Opcode, Reader as LaterReader};

use crate::strings::{ReadError, Writer};
use crate::translate::{Context, Translate};

/// The 1999 queue every ordered game action goes on.
pub const WEENIE_QUEUE: u16 = 5;
/// The 1999 control queue: appearance, server version and friends requests.
pub const CONTROL_QUEUE: u16 = 1;
/// The ordered envelope's opcode.
pub const GAME_ACTION: u32 = 0xF7B1;
/// The 1999 friends command, a bare message of its own.
pub const FRIENDS_COMMAND: u32 = 0xF7CD;
/// Friends commands: 0 lists, 1 adds, 2 removes one by name, 3 removes every friend.
pub const FRIENDS_ADD: u32 = 1;
/// See [`FRIENDS_ADD`].
pub const FRIENDS_REMOVE_ALL: u32 = 3;

/// Most bytes in a name: a person, a fellowship, a squelch or friend target.
pub const NAME_MAX: usize = 255;
/// Most bytes in other text, the most a u16 count with its terminator allows.
pub const TEXT_MAX: usize = 65_534;

/// The chat type of the lines this module tells the player.
pub const REFUSAL_TEXT_TYPE: u32 = 0x1A;
/// Told when an action has no 1999 form.
pub const NOT_IN_THIS_WORLD: &str = "That action is not available in this world.";
/// Told when text has a character the 1999 wire cannot carry.
pub const TEXT_NOT_SENDABLE: &str =
    "That text has a character this world cannot take, so it was not sent.";
/// Told when text is longer than the 1999 wire allows.
pub const TEXT_TOO_LONG: &str = "That text is too long for this world, so it was not sent.";

/// The 1999 options word's bits, by Pyreal's `PLAYER_OPTIONS` ordinal. Other options have none.
pub const OPTION_BITS: [(u32, u32); 12] = [
    (0, 0x0002),
    (1, 0x0004),
    (2, 0x0008),
    (6, 0x0040),
    (7, 0x0080),
    (8, 0x0100),
    (9, 0x0200),
    (10, 0x0400),
    (11, 0x0800),
    (12, 0x1000),
    (13, 0x2000),
    (14, 0x8000),
];
/// Every bit of [`OPTION_BITS`]. The rest of the word is kept as the server gave it.
pub const OPTION_MASK: u32 = 0xBFCE;

/// The actions whose later body is the 1999 body: they go up re-stamped and unchanged. `0x0063`
/// kills the character, as the 1999 `@lifestone` did; Pyreal asks first on a world without the
/// lifestone recall. The last eight are 1999-only numbers Pyreal has no later meaning for.
pub const PASSED: [u32; 62] = [
    0x0008, 0x000A, 0x01B7, 0x01BF, 0xF61B, 0xF61C, 0xF61E, 0xF649, 0xF661, 0xF752, 0xF753, 0xF7C9,
    0x0044, 0x0045, 0x0046, 0x0047, 0x0048, 0x004A, 0x004B, 0x01A8, 0x0019, 0x001A, 0x001B, 0x0035,
    0x0036, 0x0054, 0x0055, 0x0056, 0x019B, 0x0195, 0x00CD, 0x00C8, 0x0060, 0x0145, 0x0146, 0x0148,
    0x0149, 0x00A4, 0x00A5, 0x00A6, 0x001D, 0x001E, 0x001F, 0x00AA, 0x00AC, 0x00AD, 0x00AE, 0x00A1,
    0x0063, 0x019C, 0x019D, 0x01C2, 0x01C4, 0x01E9, 0x005A, 0x005C, 0x01A2, 0x00CA, 0x00D3, 0x00D5,
    0x01E6, 0x01EB,
];

/// Later actions Pyreal sends without the player asking: dropped with nothing said.
pub const DROPPED: [u32; 10] = [
    0x000F, 0x0010, 0x021E, 0x0224, 0x0263, 0x0286, 0x02AF, 0x02B2, 0x0311, 0x0316,
];

/// 1999 actions made here.
mod action {
    pub const TALK: u32 = 0x0015;
    pub const TALK_DIRECT: u32 = 0x0032;
    pub const CHANGE_COMBAT_MODE: u32 = 0x0053;
    pub const SQUELCH_NAME: u32 = 0x0058;
    pub const UNSQUELCH_NAME: u32 = 0x0059;
    pub const SQUELCH_ID: u32 = 0x005A;
    pub const UNSQUELCH_ID: u32 = 0x005B;
    pub const SQUELCH_LIST: u32 = 0x005C;
    pub const TALK_BY_NAME: u32 = 0x005D;
    pub const VENDOR_BUY: u32 = 0x005F;
    pub const LOGIN_COMPLETE: u32 = 0x00A1;
    pub const FELLOWSHIP_CREATE: u32 = 0x00A2;
    pub const FELLOWSHIP_QUIT: u32 = 0x00A3;
    pub const BOOK_MODIFY_PAGE: u32 = 0x00AB;
    pub const INSCRIPTION: u32 = 0x00BF;
    pub const ADVOCATE_TELEPORT: u32 = 0x00D6;
    pub const ABUSE_REPORT: u32 = 0x0140;
    pub const CHANNEL_BROADCAST: u32 = 0x0147;
    pub const CHARACTER_OPTIONS: u32 = 0x01A1;
    pub const SAVE_SANCTUARY: u32 = 0x01A2;
    pub const EMOTE: u32 = 0x01DF;
    pub const SOUL_EMOTE: u32 = 0x01E1;
    pub const ADD_SPELL_FAVORITE: u32 = 0x01E3;
    pub const REMOVE_SPELL_FAVORITE: u32 = 0x01E4;
}

/// Bare requests the 1999 client sends on its control queue with the later body.
const BARE_PASSED: [Opcode; 2] = [
    Opcode::OBJECT_SEND_FORCE_OBJDESC,
    Opcode::ADMIN_SEND_ADMIN_GET_SERVER_VERSION,
];

/// One 1999 message to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Up {
    /// A whole game action, envelope included, its order number still 0.
    Action(Vec<u8>),
    /// A bare message on a 1999 queue.
    Bare { queue: u16, message: Vec<u8> },
}

/// What became of one later message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Sent as these, in order.
    Send(Vec<Up>),
    /// Nothing sent; the player is told this line.
    Refuse(&'static str),
    /// Nothing sent and nothing said.
    Drop,
}

/// Why text did not go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextError {
    NoForm,
    TooLong,
}

impl From<TextError> for Outcome {
    fn from(e: TextError) -> Self {
        match e {
            TextError::NoForm => Self::Refuse(TEXT_NOT_SENDABLE),
            TextError::TooLong => Self::Refuse(TEXT_TOO_LONG),
        }
    }
}

/// How the server reads a string field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    /// The 1999 string: empty is two bytes.
    Plain,
    /// Aligned after it even when empty.
    Aligned,
}

/// Write `text` as a 1999 string of at most `max` bytes.
fn put_text(w: &mut Writer, text: &str, max: usize, form: Form) -> Result<(), TextError> {
    let bytes = cp1252::encode(text).ok_or(TextError::NoForm)?;
    if bytes.contains(&0) {
        return Err(TextError::NoForm);
    }
    if bytes.len() > max {
        return Err(TextError::TooLong);
    }
    if bytes.is_empty() {
        w.u16(0);
        if form == Form::Aligned {
            w.align();
        }
        return Ok(());
    }
    // `max` is at most TEXT_MAX, so the count with its terminator fits.
    w.u16(u16::try_from(bytes.len() + 1).unwrap_or(u16::MAX));
    w.bytes(&bytes).bytes(&[0]).align();
    Ok(())
}

/// A game action's writer, past its envelope.
fn action_writer(action: u32) -> Writer {
    let mut w = Writer::new(GAME_ACTION);
    w.u32(0).u32(action);
    w
}

/// The later combat mode as the 1999 wire counts it.
#[must_use]
pub fn combat_mode_1999(later: u32) -> u32 {
    match later {
        4 => 3,
        8 => 4,
        other => other,
    }
}

/// Whether a line of speech is the `@save` command, which the 1999 client sends as `0x01A2`.
#[must_use]
pub fn is_save_command(text: &str) -> bool {
    let t = text.trim();
    t.eq_ignore_ascii_case("@save") || t.eq_ignore_ascii_case("/save")
}

/// The outbound state: the order counter, and whether this world entry's squelch list went up.
/// The kept option word, squelch list and confirmations owed are in
/// [`crate::translate::Shared`], where the inbound modules write them.
#[derive(Debug, Default)]
pub struct Outbound {
    /// The last order number sent; the first action of a connection is 1.
    order: u32,
    /// The squelch list went up after this world entry's first login complete. Pyreal sends
    /// login complete again after every teleport; the 1999 client uploaded once per entry.
    squelch_uploaded: bool,
}

impl Outbound {
    /// The last order number used.
    #[must_use]
    pub fn order(&self) -> u32 {
        self.order
    }

    /// Stamp and send each message, numbering actions as they go. An action the connection does
    /// not take gives its number back.
    fn send(&mut self, ups: Vec<Up>, cx: &mut Context<'_>) -> bool {
        let mut sent = false;
        for up in ups {
            match up {
                Up::Action(mut m) => {
                    let n = self.order.wrapping_add(1);
                    m[4..8].copy_from_slice(&n.to_le_bytes());
                    if cx.send(WEENIE_QUEUE, &m) {
                        self.order = n;
                        sent = true;
                    }
                }
                Up::Bare { queue, message } => sent |= cx.send(queue, &message),
            }
        }
        sent
    }
}

/// Decide what a later game action becomes. `payload` is the whole later message, envelope
/// included; `shared` is updated for what goes up (options word, confirmations owed).
///
/// # Errors
/// [`MessageError`] for a later action that does not read.
pub fn action_up(
    payload: &[u8],
    shared: &mut crate::translate::Shared,
) -> Result<Outcome, MessageError> {
    let unpacked = unpack_action(payload)?;
    let sub = unpacked.sub_type.0;
    let mut body = unpacked.body;
    let raw = payload.get(12..).unwrap_or_default();
    if PASSED.contains(&sub) {
        let mut w = action_writer(sub);
        w.bytes(raw);
        let mut ups = vec![Up::Action(w.into_inner())];
        if sub == action::LOGIN_COMPLETE {
            ups.push(Up::Action(squelch_list(&shared.squelched)));
        }
        return Ok(Outcome::Send(ups));
    }
    if DROPPED.contains(&sub) {
        return Ok(Outcome::Drop);
    }
    let one = |w: Writer| Ok(Outcome::Send(vec![Up::Action(w.into_inner())]));
    let text = |action: u32, s: &str, max: usize, form: Form| {
        let mut w = action_writer(action);
        match put_text(&mut w, s, max, form) {
            Ok(()) => one(w),
            Err(e) => Ok(e.into()),
        }
    };
    match Opcode(sub) {
        Opcode::COMMUNICATION_TALK => {
            let s = body.pstring()?;
            if is_save_command(&s) {
                return one(action_writer(action::SAVE_SANCTUARY));
            }
            text(action::TALK, &s, TEXT_MAX, Form::Plain)
        }
        Opcode::COMMUNICATION_EMOTE => text(action::EMOTE, &body.pstring()?, TEXT_MAX, Form::Plain),
        Opcode::COMMUNICATION_SOUL_EMOTE => {
            text(action::SOUL_EMOTE, &body.pstring()?, TEXT_MAX, Form::Plain)
        }
        Opcode::COMMUNICATION_TALK_DIRECT => {
            let m = CommunicationTalkDirect::read(&mut body)?;
            let mut w = action_writer(action::TALK_DIRECT);
            if let Err(e) = put_text(&mut w, &m.message, TEXT_MAX, Form::Aligned) {
                return Ok(e.into());
            }
            w.u32(m.target.0);
            one(w)
        }
        Opcode::COMMUNICATION_TALK_DIRECT_BY_NAME => {
            let m = CommunicationTalkDirectByName::read(&mut body)?;
            let mut w = action_writer(action::TALK_BY_NAME);
            if let Err(e) = put_text(&mut w, &m.message, TEXT_MAX, Form::Aligned)
                .and_then(|()| put_text(&mut w, &m.target_name, NAME_MAX, Form::Aligned))
            {
                return Ok(e.into());
            }
            one(w)
        }
        Opcode::COMMUNICATION_CHANNEL_BROADCAST => {
            let m = CommunicationChannelBroadcast::read(&mut body)?;
            let mut w = action_writer(action::CHANNEL_BROADCAST);
            w.u32(m.channel);
            match put_text(&mut w, &m.message, TEXT_MAX, Form::Aligned) {
                Ok(()) => one(w),
                Err(e) => Ok(e.into()),
            }
        }
        Opcode::COMMUNICATION_MODIFY_CHARACTER_SQUELCH => {
            let m = CommunicationModifyCharacterSquelch::read(&mut body)?;
            let add = m.add != 0;
            if !m.character_name.is_empty() {
                let a = if add {
                    action::SQUELCH_NAME
                } else {
                    action::UNSQUELCH_NAME
                };
                return text(a, &m.character_name, NAME_MAX, Form::Plain);
            }
            if m.character_id.0 == 0 {
                return Ok(Outcome::Drop);
            }
            let mut w = action_writer(if add {
                action::SQUELCH_ID
            } else {
                action::UNSQUELCH_ID
            });
            w.u32(m.character_id.0);
            one(w)
        }
        Opcode::COMBAT_CHANGE_COMBAT_MODE => {
            let mut w = action_writer(action::CHANGE_COMBAT_MODE);
            w.u32(combat_mode_1999(body.u32()?));
            one(w)
        }
        Opcode::CHARACTER_ADD_SPELL_FAVORITE => {
            let (spell, index) = (body.u32()?, body.i32()?);
            let mut w = action_writer(action::ADD_SPELL_FAVORITE);
            w.u32(spell).i32(index);
            one(w)
        }
        Opcode::CHARACTER_REMOVE_SPELL_FAVORITE => {
            let mut w = action_writer(action::REMOVE_SPELL_FAVORITE);
            w.u32(body.u32()?);
            one(w)
        }
        Opcode::VENDOR_BUY => {
            let m = VendorBuy::read(&mut body)?;
            // The 1999 body is the later one without the alternate currency after the list.
            let end = raw.len().saturating_sub(body.remaining() + 4);
            let mut w = action_writer(action::VENDOR_BUY);
            w.bytes(&raw[..end]);
            debug_assert_eq!(m.alternate_currency_id.to_le_bytes(), raw[end..end + 4]);
            one(w)
        }
        Opcode::FELLOWSHIP_CREATE => {
            let m = FellowshipCreate::read(&mut body)?;
            text(action::FELLOWSHIP_CREATE, &m.name, NAME_MAX, Form::Aligned)
        }
        Opcode::FELLOWSHIP_QUIT => one(action_writer(action::FELLOWSHIP_QUIT)),
        Opcode::WRITING_BOOK_MODIFY_PAGE => {
            let m = WritingBookModifyPage::read(&mut body)?;
            let mut w = action_writer(action::BOOK_MODIFY_PAGE);
            w.u32(m.book_id.0).u32(m.page);
            match put_text(&mut w, &m.text, TEXT_MAX, Form::Plain) {
                Ok(()) => one(w),
                Err(e) => Ok(e.into()),
            }
        }
        Opcode::WRITING_SET_INSCRIPTION => {
            let m = WritingSetInscription::read(&mut body)?;
            let mut w = action_writer(action::INSCRIPTION);
            w.u32(m.object_id.0);
            match put_text(&mut w, &m.text, TEXT_MAX, Form::Aligned) {
                Ok(()) => one(w),
                Err(e) => Ok(e.into()),
            }
        }
        Opcode::CHARACTER_ABUSE_LOG_REQUEST => {
            let (target, status, complaint) = (body.pstring()?, body.u32()?, body.pstring()?);
            let mut w = action_writer(action::ABUSE_REPORT);
            let written = put_text(&mut w, &target, NAME_MAX, Form::Aligned).and_then(|()| {
                w.u32(status);
                put_text(&mut w, &complaint, TEXT_MAX, Form::Aligned)
            });
            match written {
                Ok(()) => one(w),
                Err(e) => Ok(e.into()),
            }
        }
        Opcode::ADVOCATE_TELEPORT => {
            let name = body.pstring()?;
            let position = body.rest();
            if position.len() != 32 {
                return Err(MessageError::Unencodable {
                    field: "advocate teleport",
                    reason: "the position is 32 bytes",
                });
            }
            let mut w = action_writer(action::ADVOCATE_TELEPORT);
            if let Err(e) = put_text(&mut w, &name, NAME_MAX, Form::Aligned) {
                return Ok(e.into());
            }
            w.bytes(position);
            one(w)
        }
        Opcode::CHARACTER_PLAYER_OPTION_CHANGED_EVENT => {
            let (option, value) = (body.u32()?, body.u32()?);
            let Some(&(_, bit)) = OPTION_BITS.iter().find(|(o, _)| *o == option) else {
                return Ok(Outcome::Drop);
            };
            let word = shared.options_word.unwrap_or(0);
            let word = if value != 0 { word | bit } else { word & !bit };
            Ok(options(word, shared))
        }
        Opcode::CHARACTER_CHARACTER_OPTIONS_EVENT => {
            let (_flags, later) = (body.u32()?, body.u32()?);
            let word = (shared.options_word.unwrap_or(0) & !OPTION_MASK) | (later & OPTION_MASK);
            Ok(options(word, shared))
        }
        Opcode::CHARACTER_CONFIRMATION_RESPONSE if shared.questions_owed > 0 => {
            shared.questions_owed -= 1;
            let mut w = action_writer(sub);
            w.bytes(raw);
            one(w)
        }
        Opcode::SOCIAL_ADD_FRIEND => {
            let m = SocialAddFriend::read(&mut body)?;
            Ok(friends(FRIENDS_ADD, &m.name))
        }
        Opcode::SOCIAL_CLEAR_FRIENDS => Ok(friends(FRIENDS_REMOVE_ALL, "")),
        // Everything else, among them `0x0059` and `0x005B`, which mean unsquelch on the 1999
        // wire, and `0x0017`, which names a friend by an id the 1999 server has no use for.
        _ => Ok(Outcome::Refuse(NOT_IN_THIS_WORLD)),
    }
}

/// `0x01A1` with the whole options word, which is kept.
fn options(word: u32, shared: &mut crate::translate::Shared) -> Outcome {
    shared.options_word = Some(word);
    let mut w = action_writer(action::CHARACTER_OPTIONS);
    w.u32(word);
    Outcome::Send(vec![Up::Action(w.into_inner())])
}

/// `0x005C`: the whole squelch list, which the 1999 client sends at every world entry.
fn squelch_list(ids: &[u32]) -> Vec<u8> {
    let mut w = action_writer(action::SQUELCH_LIST);
    w.u32(u32::try_from(ids.len()).unwrap_or(u32::MAX));
    for &id in ids {
        w.u32(id);
    }
    w.into_inner()
}

/// `0xF7CD`: a friends command and its name, on the control queue.
fn friends(command: u32, name: &str) -> Outcome {
    let mut w = Writer::new(FRIENDS_COMMAND);
    w.u32(command);
    match put_text(&mut w, name, NAME_MAX, Form::Aligned) {
        Ok(()) => Outcome::Send(vec![Up::Bare {
            queue: CONTROL_QUEUE,
            message: w.into_inner(),
        }]),
        Err(e) => e.into(),
    }
}

/// Decide what a later bare request becomes; `None` when it is not this module's.
///
/// # Errors
/// [`MessageError`] for a later message that does not read.
pub fn bare_up(opcode: Opcode, payload: &[u8]) -> Result<Option<Outcome>, MessageError> {
    if BARE_PASSED.contains(&opcode) {
        return Ok(Some(Outcome::Send(vec![Up::Bare {
            queue: CONTROL_QUEUE,
            message: payload.to_vec(),
        }])));
    }
    if opcode == Opcode::SOCIAL_SEND_FRIENDS_COMMAND {
        let mut r = LaterReader::with_origin(payload.get(4..).unwrap_or_default(), 4);
        let m = SocialSendFriendsCommand::read(&mut r)?;
        return Ok(Some(friends(m.cmd, &m.player)));
    }
    Ok(None)
}

impl Translate for Outbound {
    fn down(
        &mut self,
        _opcode: u32,
        _queue: u16,
        _message: &[u8],
        _cx: &mut Context<'_>,
    ) -> Result<bool, ReadError> {
        Ok(false)
    }

    fn entering_world(&mut self) {
        self.squelch_uploaded = false;
    }

    fn up(
        &mut self,
        opcode: Opcode,
        _queue: NetQueue,
        _ordered: bool,
        payload: &[u8],
        cx: &mut Context<'_>,
    ) -> bool {
        let decided = if opcode.0 == GAME_ACTION {
            action_up(payload, cx.shared).map(Some)
        } else {
            bare_up(opcode, payload)
        };
        match decided {
            Ok(Some(Outcome::Send(mut ups))) => {
                let sub = payload
                    .get(8..12)
                    .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]));
                if opcode.0 == GAME_ACTION && sub == Some(action::LOGIN_COMPLETE) {
                    if self.squelch_uploaded {
                        ups.truncate(1);
                    }
                    self.squelch_uploaded = true;
                }
                self.send(ups, cx)
            }
            Ok(Some(Outcome::Refuse(line))) => {
                let sub = payload
                    .get(8..12)
                    .map_or(0, |w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]));
                tracing::info!("1999: refused {:#06X} {sub:#06X}: {line}", opcode.0);
                let told = CommunicationTextboxString {
                    text: line.to_owned(),
                    text_type: REFUSAL_TEXT_TYPE,
                };
                if let Err(e) = cx.push(NetQueue::UiQueue, &told) {
                    tracing::warn!("1999: the refusal line did not encode: {e}");
                }
                true
            }
            Ok(Some(Outcome::Drop)) => true,
            Ok(None) => false,
            Err(e) => {
                tracing::warn!("1999: an outgoing {:#06X} did not read: {e}", opcode.0);
                true
            }
        }
    }
}

#[cfg(test)]
#[path = "outbound/tests.rs"]
mod tests;
