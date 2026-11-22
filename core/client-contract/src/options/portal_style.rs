//! Portal Style: portals and portal space drawn as the original swirl, or enhanced as a smoke
//! vortex in the world and a tunnel flown through to the destination. This client's own menu, an
//! integer in the option value store.

use crate::view::PrefValue;

/// `[Display] PortalStyle`: how portals and portal space are drawn.
pub const PORTAL_STYLE: &str = crate::options::names::PORTAL_STYLE;

/// The menu's caption.
pub const CAPTION: &str = "Portal Style";

/// What the menu does, for the row's tooltip.
pub const NOTE: &str = "How portals look, in the world and while you travel. Original shows the \
                        swirl from the original game. Enhanced draws each portal as a whirl of \
                        smoke in its own colour and flies you through a tunnel of light.";

/// The original swirl.
pub const ORIGINAL: i32 = 0;

/// The smoke vortex and the tunnel flown through.
pub const ENHANCED: i32 = 1;

/// The style until the player changes it.
pub const DEFAULT: i32 = ENHANCED;

/// The styles the menu offers.
pub const STYLES: [&str; 2] = ["Original", "Enhanced"];

/// Whether `name` is the menu.
#[must_use]
pub fn is_portal_style(name: &str) -> bool {
    name.eq_ignore_ascii_case(PORTAL_STYLE)
}

/// The style a word stands for: its name or its number. `None` for another name or another word.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_portal_style(name) {
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
    if !is_portal_style(name) {
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
    if !is_portal_style(name) {
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
        PORTAL_STYLE,
        PrefValue::Int(DEFAULT),
        super::store::DataType::UInt,
    ))
}

/// Whether the store says portal space is enhanced. Enhanced until the store says otherwise.
#[must_use]
pub fn enhanced() -> bool {
    !matches!(
        super::store::inq_value(PORTAL_STYLE),
        Some(PrefValue::Int(ORIGINAL))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_styles_read_by_name_or_number() {
        assert_eq!(parse_value(PORTAL_STYLE, "original"), Some(ORIGINAL));
        assert_eq!(parse_value(PORTAL_STYLE, "Enhanced"), Some(ENHANCED));
        assert_eq!(parse_value(PORTAL_STYLE, "0"), Some(ORIGINAL));
        assert_eq!(parse_value(PORTAL_STYLE, "2"), None);
        assert_eq!(parse_value("Display.Other", "original"), None);
        assert_eq!(
            convert_to_string(PORTAL_STYLE, &PrefValue::Int(ORIGINAL)).as_deref(),
            Some("Original")
        );
        assert_eq!(choice_rows(PORTAL_STYLE).map(|c| c.len()), Some(2));
    }
}
