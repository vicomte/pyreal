//! How far along the view the sun's shadows reach, Shadow Distance: a menu among the effects' rows
//! ([`super::fidelity::MENUS`]), Short, Medium, Long or Very Long, stored as metres and drawn inside
//! the better lighting. It is registered with them ([`super::fidelity::register`]); the profile
//! keeps a distance the player chose, and leaves out one still at its default
//! ([`super::fidelity::left_out_of_save`]).

use crate::options::names::fidelity::SHADOW_DISTANCE;
use crate::view::PrefValue;

/// The menu's caption.
pub const CAPTION: &str = "Shadow Distance";

/// What the menu does, for the row's tooltip.
pub const NOTE: &str =
    "Sets how far from you the sun's shadows reach before they fade out. Longer \
                        distances cost more, most of all in forests.";

/// The distances the menu offers, metres, each with its name, nearest first.
pub const LEVELS: [(&str, u32); 4] = [
    ("Short", 120),
    ("Medium", 200),
    ("Long", 320),
    ("Very Long", 500),
];

/// The distance until the player changes it, as the Medium preset has it.
pub const DEFAULT: i32 = 200;

/// The least distance.
pub const LEAST: u32 = 100;

/// The greatest distance.
pub const MOST: u32 = 500;

/// Whether `name` is the menu.
#[must_use]
pub fn is_shadow_distance(name: &str) -> bool {
    name.eq_ignore_ascii_case(SHADOW_DISTANCE)
}

/// `metres`' words: its name where the menu offers it, else the distance.
fn label(metres: u32) -> String {
    match LEVELS.iter().find(|(_, m)| *m == metres) {
        Some((name, _)) => (*name).to_owned(),
        None => format!("{metres} m"),
    }
}

/// The distance a word stands for: a level's name (with or without its distance), or metres with
/// or without their mark, between [`LEAST`] and [`MOST`]. `None` for another name or another
/// word, which leaves the option as it was.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_shadow_distance(name) {
        return None;
    }
    let word = raw.trim();
    let word = word.split(" (").next().unwrap_or(word).trim_end();
    if let Some((_, m)) = LEVELS.iter().find(|(l, _)| l.eq_ignore_ascii_case(word)) {
        return i32::try_from(*m).ok();
    }
    let metres: u32 = word.trim_end_matches('m').trim_end().parse().ok()?;
    i32::try_from(metres)
        .ok()
        .filter(|_| (LEAST..=MOST).contains(&metres))
}

/// The words for a value. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !is_shadow_distance(name) {
        return None;
    }
    match v {
        PrefValue::Int(i) => u32::try_from(*i).ok().map(label),
        _ => None,
    }
}

/// The distances the menu offers ([`LEVELS`]), each as its words; `None` for any other name.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !is_shadow_distance(name) {
        return None;
    }
    Some(
        LEVELS
            .iter()
            .map(|&(_, metres)| super::store::Choice {
                label: label(metres),
                value: i32::try_from(metres).unwrap_or(DEFAULT),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_offers_four_distances_by_name_with_medium_its_default() {
        let rows = choice_rows(SHADOW_DISTANCE).expect("the menu");
        let metres: Vec<i32> = rows.iter().map(|c| c.value).collect();
        assert_eq!(metres, [120, 200, 320, 500]);
        assert_eq!(rows[1].label, "Medium");
        assert_eq!(DEFAULT, 200);
        assert!(choice_rows("Fidelity.GodRays").is_none());
    }

    #[test]
    fn a_distance_is_read_by_name_or_metres_and_refused_outside_the_menu_range() {
        let read = |raw| parse_value(SHADOW_DISTANCE, raw);
        assert_eq!(read("long"), Some(320));
        assert_eq!(read("Very Long (500 m)"), Some(500));
        assert_eq!(read(" 250 m "), Some(250));
        assert_eq!(read("99"), None);
        assert_eq!(read("501"), None);
        assert_eq!(read("far"), None);
        assert_eq!(parse_value("Fidelity.GodRays", "200"), None);
        assert_eq!(
            convert_to_string(SHADOW_DISTANCE, &PrefValue::Int(120)).as_deref(),
            Some("Short")
        );
    }
}
