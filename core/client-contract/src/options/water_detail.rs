//! How finely the water's surface is drawn, Water Detail: a menu under Surfaces & Detail, Low,
//! Medium or High, drawn inside the better lighting. It is registered with the effects
//! ([`super::fidelity::register`]); the profile keeps a level the player chose, and leaves out one
//! still [`DEFAULT`] ([`super::fidelity::left_out_of_save`]).
//!
//! The stored value is 0 at Low, the water as it was before the menu, so a capture that never
//! names it draws it so. The menu starts at Medium, as the Medium preset has it.

use crate::options::names::fidelity::WATER_DETAIL;
use crate::view::PrefValue;

/// The menu's caption.
pub const CAPTION: &str = "Water Detail";

/// What the menu does, for the row's tooltip.
pub const NOTE: &str = "Sets how much the water's surface rises and falls with its waves. Low \
                        keeps the surface flat and shows the waves in its light. Medium gives the \
                        waves near you their height. High reaches farther and adds finer waves.";

/// The water as it was before the menu, stored as 0.
pub const LOW: i32 = 0;

/// The waves near the eye given their height.
pub const MEDIUM: i32 = 1;

/// Farther, and finer.
pub const HIGH: i32 = 2;

/// The level until the player changes it: Medium, as the Medium preset has it.
pub const DEFAULT: i32 = MEDIUM;

/// The greatest stored value.
pub const MOST: i32 = HIGH;

/// The levels the menu offers, least first, each with the value it is stored as.
pub const LEVELS: [(&str, i32); 3] = [("Low", LOW), ("Medium", MEDIUM), ("High", HIGH)];

/// Whether `name` is the menu.
#[must_use]
pub fn is_water_detail(name: &str) -> bool {
    name.eq_ignore_ascii_case(WATER_DETAIL)
}

/// The stored value a word stands for: a level's name, or a stored value's number. `None` for
/// another name or another word, which leaves the option as it was.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_water_detail(name) {
        return None;
    }
    let word = raw.trim();
    if let Some((_, v)) = LEVELS.iter().find(|(l, _)| l.eq_ignore_ascii_case(word)) {
        return Some(*v);
    }
    word.parse().ok().filter(|v| (0..=MOST).contains(v))
}

/// The words for a value: the level's name. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !is_water_detail(name) {
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
    if !is_water_detail(name) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_levels_read_by_name_or_stored_number_and_low_is_stored_as_zero() {
        assert_eq!(parse_value(WATER_DETAIL, "high"), Some(HIGH));
        assert_eq!(parse_value(WATER_DETAIL, "Medium"), Some(MEDIUM));
        assert_eq!(parse_value(WATER_DETAIL, "0"), Some(LOW));
        assert_eq!(parse_value(WATER_DETAIL, "3"), None);
        assert_eq!(parse_value("Fidelity.Other", "high"), None);
        assert_eq!(LOW, 0);
        assert_eq!(
            convert_to_string(WATER_DETAIL, &PrefValue::Int(HIGH)).as_deref(),
            Some("High")
        );
        let rows = choice_rows(WATER_DETAIL).expect("the menu's rows");
        let labels: Vec<&str> = rows.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Low", "Medium", "High"]);
    }
}
