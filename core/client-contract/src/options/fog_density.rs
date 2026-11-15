//! How thick the haze over the world is, Fog Density: a menu among the effects' rows
//! ([`super::fidelity::MENUS`]), a share of the haze the world's own fog asks for (Original), drawn
//! inside the better lighting; `/fog` sets it too. It is registered with them
//! ([`super::fidelity::register`]); the profile keeps a share the player chose, and leaves out one
//! still at its default ([`super::fidelity::left_out_of_save`]).
//!
//! The stored value is the share in percent, [`OFF`] for none.

use crate::options::names::fidelity::FOG_DENSITY;
use crate::view::PrefValue;

/// The menu's caption.
pub const CAPTION: &str = "Fog Density";

/// What the menu does, for the row's tooltip.
pub const NOTE: &str = "Sets how thick the distance haze is. Original matches the original game. \
                        Off draws no haze.";

/// No haze at all, stored as 1 as the ground cover's Off is.
pub const OFF: i32 = 1;

/// The levels the menu offers, thinnest first, each with the share it is stored as.
pub const LEVELS: [(&str, i32); 5] = [
    ("Off", OFF),
    ("Faint", 20),
    ("Thin", 40),
    ("Original", 100),
    ("Heavy", 200),
];

/// The levels' older names, still read from a profile.
const OLD_NAMES: [(&str, i32); 3] = [("Light", 20), ("Clear", 40), ("Retail", 100)];

/// The share until the player changes it: Thin.
pub const DEFAULT: i32 = 40;

/// The greatest share.
pub const MOST: u32 = 300;

/// Whether `name` is the menu.
#[must_use]
pub fn is_fog_density(name: &str) -> bool {
    name.eq_ignore_ascii_case(FOG_DENSITY)
}

/// The haze's thickness a stored value asks for, 1 the world fog's own: none at [`OFF`], the
/// default where nothing is stored (0).
#[must_use]
pub fn density(stored: u32) -> f32 {
    let share = match i32::try_from(stored) {
        Ok(0) | Err(_) => DEFAULT,
        Ok(OFF) => 0,
        Ok(v) => v,
    };
    #[allow(clippy::cast_precision_loss)]
    let d = share as f32 / 100.0;
    d
}

/// The stored value a density asks for (1 the world fog's own), within the menu's range.
#[must_use]
pub fn stored(density: f32) -> i32 {
    if !density.is_finite() || density <= 0.005 {
        return OFF;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    // LINT-OK: a share in percent, rounded and held to the menu's range.
    let v = (density * 100.0).round().clamp(2.0, MOST as f32) as i32;
    v
}

/// `share`'s words: a level's name, else the percent.
fn label(share: i32) -> String {
    LEVELS
        .iter()
        .find(|(_, v)| *v == share)
        .map_or_else(|| format!("{share}%"), |(l, _)| (*l).to_owned())
}

/// The value a word stands for: a level's name (or its older name) with or without its mark, or a
/// percent with or without its sign, up to [`MOST`]. `None` for another name or another word, which leaves the
/// option as it was.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_fog_density(name) {
        return None;
    }
    let word = raw.trim();
    let word = word.split(" (").next().unwrap_or(word).trim_end();
    if let Some((_, v)) = LEVELS
        .iter()
        .chain(&OLD_NAMES)
        .find(|(l, _)| l.eq_ignore_ascii_case(word))
    {
        return Some(*v);
    }
    let share: u32 = word.trim_end_matches('%').trim_end().parse().ok()?;
    i32::try_from(share)
        .ok()
        .filter(|_| (1..=MOST).contains(&share))
}

/// The words for a value. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !is_fog_density(name) {
        return None;
    }
    match v {
        PrefValue::Int(i) => Some(label(*i)),
        _ => None,
    }
}

/// The levels the menu offers, each as its words; `None` for any other name.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !is_fog_density(name) {
        return None;
    }
    Some(
        LEVELS
            .iter()
            .map(|&(_, value)| super::store::Choice {
                label: label(value),
                value,
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_levels_read_by_name_or_percent_and_off_draws_no_haze() {
        assert_eq!(parse_value(FOG_DENSITY, "Retail"), Some(100));
        assert_eq!(parse_value(FOG_DENSITY, "light"), Some(20));
        assert_eq!(parse_value(FOG_DENSITY, "Thin"), Some(DEFAULT));
        assert_eq!(parse_value(FOG_DENSITY, "clear (Default)"), Some(DEFAULT));
        assert_eq!(parse_value(FOG_DENSITY, "off"), Some(OFF));
        assert_eq!(parse_value(FOG_DENSITY, "75%"), Some(75));
        assert_eq!(parse_value(FOG_DENSITY, "900"), None);
        assert_eq!(parse_value("Fidelity.Other", "off"), None);
        assert!(density(1).abs() < 1e-6);
        assert!((density(0) - 0.4).abs() < 1e-6);
        assert!((density(100) - 1.0).abs() < 1e-6);
        assert_eq!(stored(0.0), OFF);
        assert_eq!(stored(1.0), 100);
        assert_eq!(stored(0.4), DEFAULT);
        let rows = choice_rows(FOG_DENSITY).expect("the menu's rows");
        let labels: Vec<&str> = rows.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Off", "Faint", "Thin", "Original", "Heavy"]);
    }
}
