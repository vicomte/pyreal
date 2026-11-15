//! How far an art pack's ground cover is drawn: an experimental menu after the effects' boxes, a
//! share of the distance it is drawn to with nothing changed, or Off. It is registered with the
//! boxes ([`super::fidelity::register`]); the profile keeps a share the player chose, and leaves
//! out one still at its default ([`super::fidelity::left_out_of_save`]).

use crate::options::names::fidelity::GROUND_COVER_DRAW_DISTANCE;
use crate::view::PrefValue;

/// The ground cover's menu's caption.
pub const GROUND_COVER_CAPTION: &str = "Ground Cover";

/// What the ground cover's menu does, for the row's tooltip.
pub const GROUND_COVER_NOTE: &str =
    "Draws the art pack's grass and small plants and sets how far out they appear. Nothing \
     collides with them.";

/// The levels the menu offers after Off, each with its share in percent, nearest first. A share
/// saved off them still draws, and the menu shows it as Custom.
pub const LEVELS: [(&str, u32); 4] = [
    ("Short", 50),
    ("Medium", 100),
    ("Long", 300),
    ("Very Long", 500),
];

/// The distance the pack draws to with nothing changed.
pub const WHOLE: u32 = 100;

/// The ground cover's share until the player changes it: the whole distance.
pub const GROUND_COVER_DEFAULT: i32 = 100;

/// The share `name`'s menu holds until the player changes it.
#[must_use]
pub fn default_of(_name: &str) -> i32 {
    GROUND_COVER_DEFAULT
}

/// The ground cover's choice that leaves it out: no share, so no share is mistaken for it.
pub const OFF: i32 = 1;

/// Its words.
const OFF_WORD: &str = "Off";

/// The least share.
pub const LEAST: u32 = 50;

/// The greatest share.
pub const MOST: u32 = 500;

/// Whether `name` is the menu.
#[must_use]
pub fn is_a_distance(name: &str) -> bool {
    name.eq_ignore_ascii_case(GROUND_COVER_DRAW_DISTANCE)
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

/// The share in percent a menu's words stand for: a multiple with or without its mark, or a
/// percent with or without its sign.
fn parse_share(raw: &str) -> Option<u32> {
    let word = raw.trim();
    let word = word.split(" (").next().unwrap_or(word).trim_end();
    if let Some(many) = word.strip_suffix(['x', 'X']) {
        let many: f32 = many.trim_end().parse().ok()?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a menu's multiple, a small positive number, brought to percent and rounded.
        return (many.is_finite() && many > 0.0).then_some((many * 100.0 + 0.5) as u32);
    }
    word.trim_end_matches('%').trim_end().parse().ok()
}

/// The value a word stands for: a level's name, a share between [`LEAST`] and [`MOST`], or the
/// ground cover's Off. `None` for another name or another word, which leaves the option as it was.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_a_distance(name) {
        return None;
    }
    if name.eq_ignore_ascii_case(GROUND_COVER_DRAW_DISTANCE)
        && raw.trim().eq_ignore_ascii_case(OFF_WORD)
    {
        return Some(OFF);
    }
    let word = raw.trim();
    let word = word.split(" (").next().unwrap_or(word).trim_end();
    if let Some((_, s)) = LEVELS.iter().find(|(l, _)| l.eq_ignore_ascii_case(word)) {
        return i32::try_from(*s).ok();
    }
    let share = parse_share(raw)?;
    i32::try_from(share)
        .ok()
        .filter(|_| (LEAST..=MOST).contains(&share))
}

/// The words for a value: its level's name, Off, or else the share as a multiple. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !is_a_distance(name) {
        return None;
    }
    match v {
        PrefValue::Int(OFF) if name.eq_ignore_ascii_case(GROUND_COVER_DRAW_DISTANCE) => {
            Some(OFF_WORD.to_owned())
        }
        PrefValue::Int(i) => u32::try_from(*i).ok().map(label),
        _ => None,
    }
}

/// The levels a menu offers ([`LEVELS`]), each as its name, after the ground cover's Off; `None`
/// for any other name.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !is_a_distance(name) {
        return None;
    }
    let off = name
        .eq_ignore_ascii_case(GROUND_COVER_DRAW_DISTANCE)
        .then(|| super::store::Choice {
            label: OFF_WORD.to_owned(),
            value: OFF,
        });
    Some(
        off.into_iter()
            .chain(LEVELS.iter().map(|&(level, share)| super::store::Choice {
                label: level.to_owned(),
                value: i32::try_from(share).unwrap_or(GROUND_COVER_DEFAULT),
            }))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_offers_off_then_four_named_distances_with_medium_the_whole() {
        let rows = choice_rows(GROUND_COVER_DRAW_DISTANCE).expect("a menu");
        let shares: Vec<i32> = rows.iter().map(|c| c.value).collect();
        assert_eq!((rows[0].label.as_str(), rows[0].value), ("Off", OFF));
        assert_eq!(shares[1..], [50, 100, 300, 500]);
        assert_eq!(
            (rows[2].label.as_str(), rows[2].value),
            ("Medium", GROUND_COVER_DEFAULT)
        );
        assert!(choice_rows("Fidelity.GodRays").is_none());
        assert!(
            choice_rows("Fidelity.TreeDrawDistance").is_none(),
            "retired"
        );
    }

    #[test]
    fn a_share_is_read_with_or_without_its_sign_and_refused_outside_the_menu_range() {
        let name = GROUND_COVER_DRAW_DISTANCE;
        assert_eq!(parse_value(name, " 300 % "), Some(300));
        assert_eq!(parse_value(name, "1.5x"), Some(150));
        assert_eq!(parse_value(name, "1x (Default)"), Some(100));
        assert_eq!(parse_value(name, "Long"), Some(300));
        assert_eq!(parse_value(name, "6x"), None);
        assert_eq!(parse_value(name, "50"), Some(50));
        assert_eq!(parse_value(name, "off"), Some(OFF));
        assert_eq!(parse_value(name, "49"), None);
        assert_eq!(parse_value(name, "501"), None);
        assert_eq!(parse_value(name, "far"), None);
        assert_eq!(parse_value("Fidelity.GodRays", "100"), None);
        assert_eq!(
            convert_to_string(name, &PrefValue::Int(100)).as_deref(),
            Some("Medium")
        );
        assert_eq!(
            convert_to_string(name, &PrefValue::Int(150)).as_deref(),
            Some("1.5x")
        );
        assert_eq!(
            convert_to_string(name, &PrefValue::Int(OFF)).as_deref(),
            Some("Off")
        );
    }
}
