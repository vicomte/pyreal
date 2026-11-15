//! How strongly the landscape is drawn rounded, Landscape Smoothing: a menu among the effects'
//! rows, Off, Low, Medium or High. It is registered with them ([`super::fidelity::register`]); the
//! profile keeps a level the player chose, and leaves out one still [`DEFAULT`]
//! ([`super::fidelity::left_out_of_save`]).
//!
//! It was a box: its ticked value, 1, is stored for Medium and its empty one, 0, for Off, so a
//! profile or a capture from then reads as it drew; Low and High are stored after them.

use crate::options::names::fidelity::LANDSCAPE_SMOOTHING;
use crate::view::PrefValue;

/// The menu's caption.
pub const CAPTION: &str = "Landscape Smoothing";

/// What the menu does, for the row's tooltip.
pub const NOTE: &str = "Rounds off the landscape's hard edges and flat facets. Movement still \
                        follows the original land. Changing it reloads the land.";

/// Retail's exact triangles.
pub const OFF: i32 = 0;

/// The level the box stood for.
pub const MEDIUM: i32 = 1;

/// Lightly rounded.
pub const LOW: i32 = 2;

/// The roundest.
pub const HIGH: i32 = 3;

/// The level until the player changes it.
pub const DEFAULT: i32 = MEDIUM;

/// The greatest stored value.
pub const MOST: i32 = HIGH;

/// The strength [`DEFAULT`] draws at ([`strength`]).
pub const DEFAULT_STRENGTH: u8 = strength(DEFAULT.unsigned_abs());

/// The levels the menu offers, least first, each with the value it is stored as.
pub const LEVELS: [(&str, i32); 4] = [
    ("Off", OFF),
    ("Low", LOW),
    ("Medium", MEDIUM),
    ("High", HIGH),
];

/// Whether `name` is the menu.
#[must_use]
pub fn is_landscape_smoothing(name: &str) -> bool {
    name.eq_ignore_ascii_case(LANDSCAPE_SMOOTHING)
}

/// The stored value a word stands for: a level's name, a stored value's number, or the box it
/// once was. `None` for another name or another word, which leaves the option as it was.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_landscape_smoothing(name) {
        return None;
    }
    let word = raw.trim();
    if let Some((_, v)) = LEVELS.iter().find(|(l, _)| l.eq_ignore_ascii_case(word)) {
        return Some(*v);
    }
    if word.eq_ignore_ascii_case("true") {
        return Some(MEDIUM);
    }
    if word.eq_ignore_ascii_case("false") {
        return Some(OFF);
    }
    word.parse().ok().filter(|v| (0..=MOST).contains(v))
}

/// The words for a value: the level's name. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !is_landscape_smoothing(name) {
        return None;
    }
    match v {
        PrefValue::Int(i) => LEVELS
            .iter()
            .find(|(_, v)| v == i)
            .map(|(l, _)| (*l).to_owned()),
        _ => None,
    }
}

/// The levels the menu offers, each as its words; `None` for any other name.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !is_landscape_smoothing(name) {
        return None;
    }
    Some(
        LEVELS
            .iter()
            .map(|(label, value)| super::store::Choice {
                label: (*label).to_owned(),
                value: *value,
            })
            .collect(),
    )
}

/// The strength the stored `value` draws at: 0 off, 1 Low, 2 Medium, 3 High.
#[must_use]
pub const fn strength(value: u32) -> u8 {
    match value {
        0 => 0,
        2 => 1,
        3 => 3,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_levels_read_by_name_or_number_and_the_old_box_reads_as_medium_or_off() {
        assert_eq!(parse_value(LANDSCAPE_SMOOTHING, "high"), Some(HIGH));
        assert_eq!(parse_value(LANDSCAPE_SMOOTHING, "true"), Some(MEDIUM));
        assert_eq!(parse_value(LANDSCAPE_SMOOTHING, "1"), Some(MEDIUM));
        assert_eq!(parse_value(LANDSCAPE_SMOOTHING, "false"), Some(OFF));
        assert_eq!(parse_value(LANDSCAPE_SMOOTHING, "9"), None);
        assert_eq!(parse_value("Fidelity.Other", "high"), None);
        let rows = choice_rows(LANDSCAPE_SMOOTHING).expect("the menu's rows");
        let labels: Vec<&str> = rows.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Off", "Low", "Medium", "High"]);
        let at = |v: i32| strength(u32::try_from(v).unwrap());
        assert_eq!([at(OFF), at(LOW), at(MEDIUM), at(HIGH)], [0, 1, 2, 3]);
    }
}
