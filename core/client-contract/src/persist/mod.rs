//! Persistence: what survives a session and a re-install.
//!
//! | Store | Lives in | Survives | Here |
//! |---|---|---|---|
//! | `UserPreferences.ini` | `<install dir>` | everything | [`preferences`] |
//! | `PlayerModule` | the **server** | account-wide | not in this crate |
//!
//! The file I/O is the application's: what is here is the shape of the data, the text format, and
//! the parse/serialise pair either side of it.

pub mod overlay_blocklist;
pub mod persistent_data;
pub mod preferences;

pub use persistent_data::{CharacterIdentity, CharacterSet};
pub use preferences::{UserPreferences, FILE_NAME};

/// Everything the persistence parsers here can refuse to do. `Display` reads `"persistence: {0}"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistError(pub String);

impl std::fmt::Display for PersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "persistence: {}", self.0)
    }
}

impl std::error::Error for PersistError {}
