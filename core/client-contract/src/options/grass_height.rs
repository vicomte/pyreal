//! How tall the grass grows, Grass Height: a menu among the effects' rows
//! ([`super::fidelity::MENUS`]), Short, Medium, Tall or Very Tall, stored as a share of the
//! height the art pack's ground cover and the grass under it grow to. A visual preference the
//! quality presets leave alone. It is registered with them ([`super::fidelity::register`]); the
//! profile keeps a level the player chose, and leaves out one still at its default
//! ([`super::fidelity::left_out_of_save`]).

use crate::options::names::fidelity::GRASS_HEIGHT;
use crate::view::PrefValue;

/// The menu's caption.
pub const CAPTION: &str = "Grass Height";

/// What the menu does, for the row's tooltip.
pub const NOTE: &str = "Sets how tall the grass and small plants grow. Medium is the art pack's \
                        own height. Taller grass covers more of the ground and can hide items \
                        lying on it.";

/// The levels the menu offers, each with its share of the grass's height in percent, shortest
/// first.
pub const LEVELS: [(&str, u32); 4] = [
    ("Short", 65),
    ("Medium", 100),
    ("Tall", 135),
    ("Very Tall", 170),
];

/// The share until the player changes it: Medium, the pack's own height.
pub const DEFAULT: i32 = 100;

/// The least share.
pub const LEAST: u32 = 50;

/// The greatest share.
pub const MOST: u32 = 200;

/// Whether `name` is the menu.
#[must_use]
pub fn is_grass_height(name: &str) -> bool {
    name.eq_ignore_ascii_case(GRASS_HEIGHT)
}

/// The height scale a stored share asks for, 1 the pack's own: the default where nothing is
/// stored (0).
#[must_use]
pub fn scale(stored: u32) -> f32 {
    let share = if stored == 0 {
        u32::try_from(DEFAULT).unwrap_or(100)
    } else {
        stored.clamp(LEAST, MOST)
    };
    #[allow(clippy::cast_precision_loss)]
    let s = share as f32 / 100.0;
    s
}

/// `share`'s name: the level nearest it.
fn label(share: u32) -> String {
    LEVELS
        .iter()
        .min_by_key(|(_, s)| s.abs_diff(share))
        .map_or_else(String::new, |(name, _)| (*name).to_owned())
}

/// The share a word stands for: a level's name or a percent with or without its sign, between
/// [`LEAST`] and [`MOST`]. `None` for another name or another word, which leaves the option as it
/// was.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_grass_height(name) {
        return None;
    }
    let word = raw.trim();
    let word = word.split(" (").next().unwrap_or(word).trim_end();
    if let Some((_, s)) = LEVELS.iter().find(|(l, _)| l.eq_ignore_ascii_case(word)) {
        return i32::try_from(*s).ok();
    }
    let share: u32 = word.trim_end_matches('%').trim_end().parse().ok()?;
    i32::try_from(share)
        .ok()
        .filter(|_| (LEAST..=MOST).contains(&share))
}

/// The words for a value: its level's name. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !is_grass_height(name) {
        return None;
    }
    match v {
        PrefValue::Int(i) => u32::try_from(*i).ok().map(label),
        _ => None,
    }
}

/// The levels the menu offers ([`LEVELS`]), each as its name; `None` for any other name.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !is_grass_height(name) {
        return None;
    }
    Some(
        LEVELS
            .iter()
            .map(|&(level, share)| super::store::Choice {
                label: level.to_owned(),
                value: i32::try_from(share).unwrap_or(DEFAULT),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_offers_four_heights_by_name_with_medium_the_packs_own() {
        let rows = choice_rows(GRASS_HEIGHT).expect("the menu");
        let shares: Vec<i32> = rows.iter().map(|c| c.value).collect();
        assert_eq!(shares, [65, 100, 135, 170]);
        let names: Vec<&str> = rows.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(names, ["Short", "Medium", "Tall", "Very Tall"]);
        assert!(choice_rows("Fidelity.GodRays").is_none());
        assert!((scale(0) - 1.0).abs() < 1e-6);
        assert!((scale(170) - 1.7).abs() < 1e-6);
    }

    #[test]
    fn a_height_is_read_by_name_or_percent_and_refused_outside_the_range() {
        let read = |raw| parse_value(GRASS_HEIGHT, raw);
        assert_eq!(read("very tall"), Some(170));
        assert_eq!(read("Short (Default)"), Some(65));
        assert_eq!(read(" 120 % "), Some(120));
        assert_eq!(read("49"), None);
        assert_eq!(read("high"), None);
        assert_eq!(parse_value("Fidelity.GodRays", "100"), None);
        let words = |share| convert_to_string(GRASS_HEIGHT, &PrefValue::Int(share));
        assert_eq!(words(100).as_deref(), Some("Medium"));
        assert_eq!(words(150).as_deref(), Some("Tall"));
    }
}
