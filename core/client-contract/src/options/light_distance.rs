//! How far the lamps light the world and show their glow, Light Draw Distance: an experimental
//! menu among the effects' rows ([`super::fidelity::MENUS`]), a share of the distance they reach
//! at 1x, drawn inside the better lighting. It is registered with them
//! ([`super::fidelity::register`]); the profile keeps a share the player chose, and leaves out one
//! still at its default ([`super::fidelity::left_out_of_save`]).

use crate::options::names::fidelity::LIGHT_DRAW_DISTANCE;
use crate::view::PrefValue;

/// The menu's caption.
pub const CAPTION: &str = "Light Draw Distance";

/// What the menu does, for the row's tooltip.
pub const NOTE: &str = "Sets how far away torches, lamps and other lights light the world. Lights \
                        fade out gently at the edge.";

/// The levels the menu offers, each with its share in percent, nearest first. A share saved off
/// them still draws, and the menu shows it as Custom.
pub const LEVELS: [(&str, u32); 5] = [
    ("Very Short", 50),
    ("Short", 100),
    ("Medium", 150),
    ("Long", 300),
    ("Very Long", 500),
];

/// The share until the player changes it, as the Medium preset has it.
pub const DEFAULT: i32 = 150;

/// The least share.
pub const LEAST: u32 = 50;

/// The greatest share.
pub const MOST: u32 = 500;

/// Whether `name` is the menu.
#[must_use]
pub fn is_light_distance(name: &str) -> bool {
    name.eq_ignore_ascii_case(LIGHT_DRAW_DISTANCE)
}

/// A share in percent as a multiple: `0.5x`, `1x`, `1.5x`.
fn times(share: u32) -> String {
    let whole = share / 100;
    let part = share % 100;
    if part == 0 {
        format!("{whole}x")
    } else if part.is_multiple_of(10) {
        format!("{whole}.{}x", part / 10)
    } else {
        format!("{whole}.{part:02}x")
    }
}

/// `share`'s words: its level's name, else the share as a multiple.
fn label(share: u32) -> String {
    LEVELS
        .iter()
        .find(|(_, s)| *s == share)
        .map_or_else(|| times(share), |(name, _)| (*name).to_owned())
}

/// The value a word stands for: a level's name, a multiple with or without its mark, or a percent
/// with or without its sign, between [`LEAST`] and [`MOST`]. `None` for another name or another
/// word, which leaves the option as it was.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_light_distance(name) {
        return None;
    }
    let word = raw.trim();
    let word = word.split(" (").next().unwrap_or(word).trim_end();
    if let Some((_, s)) = LEVELS.iter().find(|(l, _)| l.eq_ignore_ascii_case(word)) {
        return i32::try_from(*s).ok();
    }
    let share = if let Some(many) = word.strip_suffix(['x', 'X']) {
        let many: f32 = many.trim_end().parse().ok()?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a menu's multiple, a small positive number, brought to percent and rounded.
        let share = (many.is_finite() && many > 0.0).then_some((many * 100.0 + 0.5) as u32)?;
        share
    } else {
        word.trim_end_matches('%').trim_end().parse().ok()?
    };
    i32::try_from(share)
        .ok()
        .filter(|_| (LEAST..=MOST).contains(&share))
}

/// The words for a value: its level's name, else the share as a multiple. `None` for any other
/// name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !is_light_distance(name) {
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
    if !is_light_distance(name) {
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
    fn the_menu_offers_five_named_distances_with_medium_its_default() {
        let rows = choice_rows(LIGHT_DRAW_DISTANCE).expect("the menu");
        let shares: Vec<i32> = rows.iter().map(|c| c.value).collect();
        assert_eq!(shares, [50, 100, 150, 300, 500]);
        assert_eq!(rows[2].label, "Medium");
        assert_eq!(rows[2].value, DEFAULT);
        assert!(choice_rows("Fidelity.GodRays").is_none());
    }

    #[test]
    fn a_share_is_read_with_or_without_its_sign_and_refused_outside_the_menu_range() {
        let read = |raw| parse_value(LIGHT_DRAW_DISTANCE, raw);
        assert_eq!(read(" 300 % "), Some(300));
        assert_eq!(read("1.5x"), Some(150));
        assert_eq!(read("1.5x (Default)"), Some(150));
        assert_eq!(read("very long"), Some(500));
        assert_eq!(read("6x"), None);
        assert_eq!(read("49"), None);
        assert_eq!(read("501"), None);
        assert_eq!(read("far"), None);
        assert_eq!(parse_value("Fidelity.GodRays", "100"), None);
        assert_eq!(
            convert_to_string(LIGHT_DRAW_DISTANCE, &PrefValue::Int(200)).as_deref(),
            Some("2x")
        );
        assert_eq!(
            convert_to_string(LIGHT_DRAW_DISTANCE, &PrefValue::Int(150)).as_deref(),
            Some("Medium")
        );
    }
}
