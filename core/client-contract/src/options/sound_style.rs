//! Sound Style: the game's sounds played by the original engine or by this client's enhanced one.
//! This client's own menu, an integer in the option value store; `pyreal_audio::SoundStyle` is
//! what it chooses.

use crate::view::PrefValue;

/// `[Sound] SoundStyle`: which engine plays the sounds.
pub const SOUND_STYLE: &str = crate::options::names::SOUND_STYLE;

/// The menu's caption.
pub const CAPTION: &str = "Sound Style";

/// What the menu does, for the row's tooltip.
pub const NOTE: &str = "How the game's sounds are played. Original plays them as the original \
                        game did, sixteen at a time. Enhanced plays many more at once, places \
                        them around you as they move, echoes them indoors and loops the \
                        background sounds.";

/// The original engine.
pub const ORIGINAL: i32 = 0;

/// This client's own engine.
pub const ENHANCED: i32 = 1;

/// The style until the player changes it.
pub const DEFAULT: i32 = ENHANCED;

/// The styles the menu offers.
pub const STYLES: [&str; 2] = ["Original", "Enhanced"];

/// Whether `name` is the menu.
#[must_use]
pub fn is_sound_style(name: &str) -> bool {
    name.eq_ignore_ascii_case(SOUND_STYLE)
}

/// The style a word stands for: its name or its number. `None` for another name or another word.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_sound_style(name) {
        return None;
    }
    let word = raw.trim();
    if let Some(i) = STYLES.iter().position(|s| s.eq_ignore_ascii_case(word)) {
        return i32::try_from(i).ok();
    }
    word.parse()
        .ok()
        .filter(|v| (ORIGINAL..=ENHANCED).contains(v))
}

/// The words for a value: the style's name. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !is_sound_style(name) {
        return None;
    }
    match v {
        PrefValue::Int(i) => usize::try_from(*i)
            .ok()
            .and_then(|i| STYLES.get(i))
            .map(|s| (*s).to_owned()),
        _ => None,
    }
}

/// The styles the menu offers, each as its words; `None` for any other name.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !is_sound_style(name) {
        return None;
    }
    Some(
        (ORIGINAL..=ENHANCED)
            .zip(STYLES)
            .map(|(value, label)| super::store::Choice {
                label: label.to_owned(),
                value,
            })
            .collect(),
    )
}

/// Register the menu in the option value store, at [`DEFAULT`].
pub fn register() -> usize {
    usize::from(super::store::register_preference(
        SOUND_STYLE,
        PrefValue::Int(DEFAULT),
        super::store::DataType::UInt,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::sheet::{self, PageId};

    #[test]
    fn the_styles_read_by_name_or_number() {
        assert_eq!(parse_value(SOUND_STYLE, "original"), Some(ORIGINAL));
        assert_eq!(parse_value(SOUND_STYLE, "Enhanced"), Some(ENHANCED));
        assert_eq!(parse_value(SOUND_STYLE, "0"), Some(ORIGINAL));
        assert_eq!(parse_value(SOUND_STYLE, "2"), None);
        assert_eq!(parse_value("Sound.Other", "original"), None);
        assert_eq!(
            convert_to_string(SOUND_STYLE, &PrefValue::Int(ORIGINAL)).as_deref(),
            Some("Original")
        );
        assert_eq!(choice_rows(SOUND_STYLE).map(|c| c.len()), Some(2));
    }

    /// The menu leads the Sound heading, enhanced by default, and is saved under `[Sound]`.
    #[test]
    fn the_menu_leads_the_sound_heading_and_saves_under_sound() {
        super::super::store::init();
        let row = sheet::row_of_preference(PageId::Client, SOUND_STYLE).expect("its row");
        assert_eq!(row.caption, CAPTION);
        let first = sheet::page(PageId::Client).headings[0].rows[0].caption;
        assert_eq!(first, CAPTION);
        assert_eq!(
            super::super::store::inq_value(SOUND_STYLE),
            Some(PrefValue::Int(ENHANCED))
        );
        assert!(super::super::store::set_value(
            SOUND_STYLE,
            PrefValue::Int(ORIGINAL)
        ));
        let text = super::super::store::save().to_text();
        assert!(text.contains("SoundStyle=Original"), "{text}");
        super::super::store::clear();
    }
}
