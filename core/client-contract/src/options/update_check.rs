//! Check for Updates: whether the client asks GitHub for a newer Pyreal when it opens.
//!
//! This is this client's own option, not a retail one. [`CHECK_FOR_UPDATES`] is a boolean in the
//! option value store ([`crate::options::store::init`]), on by default, saved in the profile's
//! `[Misc]` section, and a row of the Client page's Information heading.

use crate::view::PrefValue;

/// `[Misc] CheckForUpdates`: whether the client checks for a newer release. `True` is on.
pub const CHECK_FOR_UPDATES: &str = crate::options::names::CHECK_FOR_UPDATES;

/// The row's caption, literal text: no string table has one.
pub const CAPTION: &str = "Check for Updates";

/// What the row does, for its tooltip.
pub const NOTE: &str = "Checks GitHub for a newer Pyreal when the app opens.";

/// Register the option in the option value store, on.
pub fn register() -> usize {
    usize::from(super::store::register_preference(
        CHECK_FOR_UPDATES,
        PrefValue::Bool(true),
        super::store::DataType::Bool,
    ))
}

/// Whether the store says to check. On where the option is not registered.
#[must_use]
pub fn enabled() -> bool {
    !matches!(
        super::store::inq_value(CHECK_FOR_UPDATES),
        Some(PrefValue::Bool(false))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::sheet::{self, PageId};
    use crate::persist::preferences::UserPreferences;

    /// The row is on the Client page under Information, on by default.
    #[test]
    fn the_row_is_under_information_and_on() {
        super::super::store::init();
        let row = sheet::row_of_preference(PageId::Client, CHECK_FOR_UPDATES).expect("its row");
        assert_eq!(row.caption, CAPTION);
        assert_eq!(
            row.default,
            Some(crate::options::config::PrefValueConst::Bool(true))
        );
        let info = sheet::page(PageId::Client)
            .headings
            .iter()
            .find(|h| h.title == sheet::INFORMATION_HEADING)
            .unwrap();
        assert!(info.rows.iter().any(|r| r.caption == CAPTION));
        assert!(enabled());
    }

    /// Turned off, it is saved under `[Misc]` and a saved profile brings it back.
    #[test]
    fn the_setting_is_saved_and_read_back() {
        super::super::store::clear();
        assert_eq!(register(), 1);
        assert!(enabled());
        assert!(super::super::store::set_value(
            CHECK_FOR_UPDATES,
            PrefValue::Bool(false)
        ));
        assert!(!enabled());
        let text = super::super::store::save().to_text();
        let misc = text
            .split("[Misc]\r\n")
            .nth(1)
            .and_then(|s| s.split("\r\n[").next())
            .unwrap_or_else(|| panic!("no [Misc] section: {text}"));
        assert!(misc.contains("CheckForUpdates=False"), "{misc}");

        super::super::store::clear();
        register();
        let ini = UserPreferences::parse(&text).expect("the saved profile parses");
        super::super::store::load(&ini);
        assert!(!enabled());
        super::super::store::clear();
    }
}
