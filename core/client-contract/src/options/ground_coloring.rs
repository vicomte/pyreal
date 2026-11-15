//! Ground Coloring: end of retail's ground drawn in colours toward the 1999 ground's (Classic), or
//! as Throne of Destiny painted it. This client's own menu, an integer in the option value store.

use crate::view::PrefValue;

/// `[Display] GroundColoring`: how end of retail's ground is coloured.
pub const GROUND_COLORING: &str = crate::options::names::GROUND_COLORING;

/// The menu's caption.
pub const CAPTION: &str = "Ground Coloring";

/// What the menu does, for the row's tooltip.
pub const NOTE: &str = "The colors of grass, dirt, rock and roads. Classic shades them like the \
                        game at its launch in 1999. Throne of Destiny shows them as that \
                        expansion repainted them.";

/// Toward the 1999 ground's colours.
pub const CLASSIC: i32 = 0;

/// As Throne of Destiny painted it.
pub const THRONE_OF_DESTINY: i32 = 1;

/// The coloring until the player changes it.
pub const DEFAULT: i32 = CLASSIC;

/// The colorings the menu offers.
pub const STYLES: [&str; 2] = ["Classic", "Throne of Destiny"];

/// Whether `name` is the menu.
#[must_use]
pub fn is_ground_coloring(name: &str) -> bool {
    name.eq_ignore_ascii_case(GROUND_COLORING)
}

/// The coloring a word stands for: its name or its number. `None` for another name or another word.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_ground_coloring(name) {
        return None;
    }
    let word = raw.trim();
    if let Some(i) = STYLES.iter().position(|s| s.eq_ignore_ascii_case(word)) {
        return i32::try_from(i).ok();
    }
    word.parse()
        .ok()
        .filter(|v| (CLASSIC..=THRONE_OF_DESTINY).contains(v))
}

/// The words for a value: the coloring's name. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !is_ground_coloring(name) {
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

/// The colorings the menu offers, each as its words; `None` for any other name.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !is_ground_coloring(name) {
        return None;
    }
    Some(
        (CLASSIC..=THRONE_OF_DESTINY)
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
        GROUND_COLORING,
        PrefValue::Int(DEFAULT),
        super::store::DataType::UInt,
    ))
}

/// Whether the store says the ground is Classic. Classic until the store says otherwise.
#[must_use]
pub fn classic() -> bool {
    !matches!(
        super::store::inq_value(GROUND_COLORING),
        Some(PrefValue::Int(THRONE_OF_DESTINY))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_colorings_read_by_name_or_number_and_classic_is_the_default() {
        assert_eq!(DEFAULT, CLASSIC);
        assert_eq!(parse_value(GROUND_COLORING, "classic"), Some(CLASSIC));
        assert_eq!(
            parse_value(GROUND_COLORING, "Throne of Destiny"),
            Some(THRONE_OF_DESTINY)
        );
        assert_eq!(parse_value(GROUND_COLORING, "1"), Some(THRONE_OF_DESTINY));
        assert_eq!(parse_value(GROUND_COLORING, "2"), None);
        assert_eq!(parse_value("Display.Other", "classic"), None);
        assert_eq!(
            convert_to_string(GROUND_COLORING, &PrefValue::Int(THRONE_OF_DESTINY)).as_deref(),
            Some("Throne of Destiny")
        );
        assert_eq!(choice_rows(GROUND_COLORING).map(|c| c.len()), Some(2));
    }
}
