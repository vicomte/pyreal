//! The login-area messages: the roster (`0xF658`), the news (`0xF65A`), the character error
//! (`0xF659`) and select-character (`0xF657`).

pub use crate::strings::{ReadError, Reader, Writer, ACCOUNT_FIELD_SIZE, ACCOUNT_MARKER};

/// `0xF658`.
pub const ROSTER: u32 = 0xF658;
/// `0xF659`.
pub const CHARACTER_ERROR: u32 = 0xF659;
/// `0xF65A`.
pub const NEWS: u32 = 0xF65A;
/// `0xF657`.
pub const SELECT_CHARACTER: u32 = 0xF657;
/// `0xF7C7`, the 1999 entry-ready.
pub const ENTRY_READY: u32 = 0xF7C7;

/// The most rows a roster may have.
pub const MAX_ROSTER_ROWS: usize = 64;

/// One roster row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterRow {
    pub id: u32,
    pub name: String,
    /// Seconds until deletion; 0 = not being deleted.
    pub deletion: u32,
}

/// `0xF658`: the characters, rows being deleted included in place, and the account name the
/// server knows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Roster {
    pub rows: Vec<RosterRow>,
    pub account: String,
}

impl Roster {
    /// # Errors
    /// [`ReadError`] for a short message, rows out of order, a zero or repeated id, a non-zero
    /// reserved word, more than [`MAX_ROSTER_ROWS`] rows or anything left over.
    pub fn read(message: &[u8]) -> Result<Self, ReadError> {
        let mut r = Reader::after_opcode(message);
        let count = usize::try_from(r.u32()?).map_err(|_| ReadError::Bad("count"))?;
        if count > MAX_ROSTER_ROWS {
            return Err(ReadError::Bad("too many rows"));
        }
        if r.u32()? != 0 {
            return Err(ReadError::Bad("reserved word"));
        }
        let mut rows: Vec<RosterRow> = Vec::with_capacity(count);
        for slot in 0..count {
            if usize::try_from(r.u32()?).ok() != Some(slot) {
                return Err(ReadError::Bad("slot order"));
            }
            let id = r.u32()?;
            if id == 0 || rows.iter().any(|row| row.id == id) {
                return Err(ReadError::Bad("character id"));
            }
            let name = r.string()?;
            let deletion = r.u32()?;
            rows.push(RosterRow { id, name, deletion });
        }
        let account = r.account()?;
        r.finish()?;
        Ok(Self { rows, account })
    }

    #[must_use]
    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::new(ROSTER);
        w.u32(u32::try_from(self.rows.len()).unwrap_or(0)).u32(0);
        for (slot, row) in self.rows.iter().enumerate() {
            w.u32(u32::try_from(slot).unwrap_or(0))
                .u32(row.id)
                .string(&row.name)
                .u32(row.deletion);
        }
        w.account(0, &self.account);
        w.into_inner()
    }

    /// The later character set: every row in the main list in wire order, countdowns kept,
    /// `slots` allowed, no deleted list and no flags.
    #[must_use]
    pub fn to_later(&self, slots: i32) -> pyreal_protocol::login::LoginCharacterSet {
        pyreal_protocol::login::LoginCharacterSet {
            status: 0,
            characters: self
                .rows
                .iter()
                .map(|row| pyreal_protocol::login::CharacterIdentity {
                    gid: pyreal_primitives::ObjectId(row.id),
                    name: row.name.clone(),
                    seconds_greyed_out: row.deletion,
                })
                .collect(),
            deleted: Vec::new(),
            num_allowed_characters: slots,
            account: self.account.clone(),
            use_turbine_chat: 0,
            has_throne_of_destiny: 0,
        }
    }
}

/// `0xF65A`: two strings, of which the 1999 client shows the second.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct News {
    pub first: String,
    pub shown: String,
}

impl News {
    /// # Errors
    /// [`ReadError`].
    pub fn read(message: &[u8]) -> Result<Self, ReadError> {
        let mut r = Reader::after_opcode(message);
        let first = r.string()?;
        let shown = r.string()?;
        r.finish()?;
        Ok(Self { first, shown })
    }

    #[must_use]
    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::new(NEWS);
        w.string(&self.first).string(&self.shown);
        w.into_inner()
    }
}

/// `0xF659`: one error code.
///
/// # Errors
/// [`ReadError`].
pub fn read_character_error(message: &[u8]) -> Result<u32, ReadError> {
    let mut r = Reader::after_opcode(message);
    let code = r.u32()?;
    r.finish()?;
    Ok(code)
}

/// `0xF657`: the character id and the roster's account name.
#[must_use]
pub fn select_character(id: u32, account: &str) -> Vec<u8> {
    let mut w = Writer::new(SELECT_CHARACTER);
    w.u32(id).account(ACCOUNT_MARKER, account);
    w.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster() -> Roster {
        Roster {
            rows: vec![
                RosterRow {
                    id: 0x5000_0001,
                    name: "Aluvian Ann".to_owned(),
                    deletion: 0,
                },
                RosterRow {
                    id: 0x5000_0002,
                    name: String::new(),
                    deletion: 3600,
                },
            ],
            account: "demo".to_owned(),
        }
    }

    #[test]
    fn a_roster_round_trips_and_becomes_the_later_set() {
        let bytes = roster().write();
        assert_eq!(&bytes[..12], &[0x58, 0xF6, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(Roster::read(&bytes), Ok(roster()));
        let later = roster().to_later(5);
        assert_eq!(later.characters.len(), 2);
        assert_eq!(later.characters[1].seconds_greyed_out, 3600);
        assert!(later.deleted.is_empty());
        assert_eq!(later.num_allowed_characters, 5);
        assert_eq!(later.account, "demo");
        let blob = pyreal_protocol::write_blob(&later).expect("encodes");
        let back: pyreal_protocol::login::LoginCharacterSet =
            pyreal_protocol::read_body(&blob[4..]).expect("Pyreal reads it");
        assert_eq!(back, later);
    }

    #[test]
    fn a_bad_roster_is_refused() {
        let mut out_of_order = roster();
        out_of_order.rows[1].id = out_of_order.rows[0].id;
        assert!(Roster::read(&out_of_order.write()).is_err(), "repeated id");
        let mut bytes = roster().write();
        bytes[12] = 1;
        assert!(Roster::read(&bytes).is_err(), "slot order");
        let mut bytes = roster().write();
        bytes.extend([1, 2, 3, 4]);
        assert!(Roster::read(&bytes).is_err(), "trailing bytes");
        let mut bytes = roster().write();
        bytes[8] = 1;
        assert!(Roster::read(&bytes).is_err(), "reserved word");
    }

    #[test]
    fn news_shows_its_second_string_and_may_be_long() {
        let long = "Welcome to Dereth. ".repeat(1200);
        let news = News {
            first: "AC1999".to_owned(),
            shown: long.clone(),
        };
        let back = News::read(&news.write()).expect("reads");
        assert_eq!(back.shown, long);
        assert!(long.len() > 16 * 1024);
    }

    #[test]
    fn select_carries_the_roster_account() {
        let m = select_character(0x5000_0001, "demo");
        assert_eq!(m.len(), 4 + 4 + ACCOUNT_FIELD_SIZE);
        assert_eq!(&m[..8], &[0x57, 0xF6, 0, 0, 1, 0, 0, 0x50]);
        assert_eq!(&m[12..16], b"demo");
        assert_eq!(read_character_error(&[0x59, 0xF6, 0, 0, 5, 0, 0, 0]), Ok(5));
    }
}
