//! How far out objects keep their detail, Object Detail: a menu among the effects' rows
//! ([`super::fidelity::MENUS`]), Low, Medium, High or Ultra, stored as a share of the distances
//! the dats and the art pack switch each object to a coarser level (and a tree to its card) at;
//! the quality presets set it. It is registered with them ([`super::fidelity::register`]); the
//! profile keeps a level the player chose, and leaves out one still at its default
//! ([`super::fidelity::left_out_of_save`]).

use crate::options::names::fidelity::OBJECT_DETAIL;
use crate::view::PrefValue;

/// The menu's caption.
pub const CAPTION: &str = "Object Detail";

/// What the menu does, for the row's tooltip.
pub const NOTE: &str =
    "Sets how far out trees, buildings, plants and ground cover keep their full \
                        shape. Beyond that they switch to simpler shapes, and trees switch to \
                        pictures. Each level blends smoothly into the next. Higher levels cost \
                        more, most of all in forests. Changes apply at once.";

/// The levels the menu offers, each with its share of the game's distances in percent, least
/// first.
pub const LEVELS: [(&str, u32); 4] = [("Low", 75), ("Medium", 150), ("High", 200), ("Ultra", 300)];

/// The share until the player changes it, as the Medium preset has it.
pub const DEFAULT: i32 = 150;

/// The least share.
pub const LEAST: u32 = 50;

/// The greatest share.
pub const MOST: u32 = 400;

/// Whether `name` is the menu.
#[must_use]
pub fn is_object_detail(name: &str) -> bool {
    name.eq_ignore_ascii_case(OBJECT_DETAIL)
}

/// The distance scale a stored share asks for, 1 the game's own: the default where nothing is
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

/// The share a word stands for: a level's name, an older profile's multiple (`1.5x`) or a percent
/// with or without its sign, between [`LEAST`] and [`MOST`]. `None` for another name or another
/// word, which leaves the option as it was.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_object_detail(name) {
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

/// The words for a value: its level's name. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !is_object_detail(name) {
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
    if !is_object_detail(name) {
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
    fn the_menu_offers_four_levels_by_name_with_medium_its_default() {
        let rows = choice_rows(OBJECT_DETAIL).expect("the menu");
        let shares: Vec<i32> = rows.iter().map(|c| c.value).collect();
        assert_eq!(shares, [75, 150, 200, 300]);
        let names: Vec<&str> = rows.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(names, ["Low", "Medium", "High", "Ultra"]);
        assert!(choice_rows("Fidelity.GodRays").is_none());
        assert!((scale(0) - 1.5).abs() < 1e-6);
        assert!((scale(300) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn a_level_is_read_by_name_or_an_older_share_and_refused_outside_the_range() {
        let read = |raw| parse_value(OBJECT_DETAIL, raw);
        assert_eq!(read("ultra"), Some(300));
        assert_eq!(read(" 200 % "), Some(200));
        assert_eq!(read("2.5x"), Some(250));
        assert_eq!(read("1.5x (Default)"), Some(150));
        assert_eq!(read("5x"), None);
        assert_eq!(read("49"), None);
        assert_eq!(read("far"), None);
        assert_eq!(parse_value("Fidelity.GodRays", "100"), None);
        let words = |share| convert_to_string(OBJECT_DETAIL, &PrefValue::Int(share));
        assert_eq!(words(75).as_deref(), Some("Low"));
        assert_eq!(words(250).as_deref(), Some("High"));
    }
}
