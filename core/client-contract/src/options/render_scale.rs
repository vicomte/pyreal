//! The render scale: the share of the screen's pixels the world is drawn at
//! (`Fidelity.RenderScale`, [`super::fidelity`]), or Dynamic ([`DYNAMIC`]). Fewer pixels draw
//! faster, and the picture, enlarged to fill the screen, looks softer.
//!
//! The window's own share (`Display.RenderScale`) has no row: the window always draws all of its
//! screen's pixels. A host whose window goes by points still registers it ([`register`]), so an
//! older profile's value is read and kept, and nothing draws by it.

use crate::options::names::fidelity::RENDER_SCALE as FIDELITY_RENDER_SCALE;
use crate::view::PrefValue;

/// `[Display] RenderScale`: the window's share of its screen's pixels, in percent, kept from an
/// older profile and ignored.
pub const RENDER_SCALE: &str = crate::options::names::DISPLAY_RENDER_SCALE;

/// The shares the menu offers, in percent, the whole screen first.
pub const CHOICES: [u32; 5] = [100, 77, 67, 58, 50];

/// All of the screen's pixels.
pub const ALL: u32 = 100;

/// The enhanced graphics' choice that moves the render scale to hold a frame rate (see
/// [`super::dynamic_resolution`]): no share, so no share is mistaken for it.
pub const DYNAMIC: u32 = 1;

/// Its words.
const DYNAMIC_WORD: &str = "Dynamic";

/// The least share drawn: half the screen's pixels each way.
pub const LEAST: u32 = 50;

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

/// `share`'s words in the profile, marked where it is the enhanced graphics' default.
fn label(share: u32) -> String {
    if share == super::fidelity::SCALE_DEFAULT {
        format!("{} (Default)", times(share))
    } else {
        times(share)
    }
}

/// The share in percent a word stands for: a multiple with or without its mark, or a percent with
/// or without its sign.
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

/// Whether `name` is the enhanced graphics' render scale.
fn is_fidelity(name: &str) -> bool {
    name.eq_ignore_ascii_case(FIDELITY_RENDER_SCALE)
}

/// Whether `name` is one of the two preferences that hold a share.
#[must_use]
pub fn is_a_scale(name: &str) -> bool {
    name.eq_ignore_ascii_case(RENDER_SCALE) || is_fidelity(name)
}

/// `share`, kept between [`LEAST`] and [`ALL`].
const fn kept(share: u32) -> u32 {
    if share < LEAST {
        LEAST
    } else if share > ALL {
        ALL
    } else {
        share
    }
}

/// Register the option in the option value store at `start` percent (kept between [`LEAST`] and
/// [`ALL`]): what a host that offers it does as the client starts. A store that already has it
/// keeps what it holds.
pub fn register(start: u32) -> usize {
    usize::from(super::store::register_preference(
        RENDER_SCALE,
        PrefValue::Int(i32::try_from(kept(start)).unwrap_or(100)),
        super::store::DataType::UInt,
    ))
}

/// Whether the host registered the window's share.
#[must_use]
pub fn offered() -> bool {
    super::store::is_registered(RENDER_SCALE)
}

/// The value a word stands for: a share in percent, with or without its sign, or as a multiple,
/// between [`LEAST`] and [`ALL`], or the enhanced graphics' Dynamic. `None` for another name or
/// another word, which leaves the option as it was.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_a_scale(name) {
        return None;
    }
    if is_fidelity(name)
        && raw
            .trim()
            .get(..DYNAMIC_WORD.len())
            .is_some_and(|w| w.eq_ignore_ascii_case(DYNAMIC_WORD))
    {
        return i32::try_from(DYNAMIC).ok();
    }
    let share = parse_share(raw)?;
    (LEAST..=ALL)
        .contains(&share)
        .then(|| i32::try_from(share).ok())
        .flatten()
}

/// The words for a value: the share in percent, or for the enhanced graphics as a multiple, or
/// Dynamic. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !is_a_scale(name) {
        return None;
    }
    match v {
        PrefValue::Int(i) if is_fidelity(name) => u32::try_from(*i).ok().map(|s| {
            if s == DYNAMIC {
                DYNAMIC_WORD.to_owned()
            } else {
                label(s)
            }
        }),
        PrefValue::Int(i) => Some(format!("{i}%")),
        _ => None,
    }
}

/// The shares a menu offers ([`CHOICES`]) as their percent, then the render scale's Dynamic;
/// `None` for any other name.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !is_a_scale(name) {
        return None;
    }
    let dynamic = is_fidelity(name).then(|| super::store::Choice {
        label: DYNAMIC_WORD.to_owned(),
        value: i32::try_from(DYNAMIC).unwrap_or(1),
    });
    Some(
        CHOICES
            .iter()
            .map(|&share| super::store::Choice {
                label: format!("{share}%"),
                value: i32::try_from(share).unwrap_or(100),
            })
            .chain(dynamic)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{sheet, store};

    /// The Client Options page, as the preferences of its rows in order.
    fn client_page() -> Vec<&'static str> {
        sheet::rows_for(sheet::PageId::Client)
            .filter_map(sheet::Row::preference)
            .collect()
    }

    /// The window's share has no row on any page, and the Defaults leave it alone.
    #[test]
    fn the_window_s_share_has_no_row() {
        store::init();
        assert_eq!(register(75), 1);
        assert!(offered());
        assert!(!client_page().contains(&RENDER_SCALE));
        assert!(sheet::defaults(sheet::PageId::Client)
            .iter()
            .all(|(name, _)| *name != RENDER_SCALE));
        store::clear();
    }

    /// A registration keeps what the store holds.
    #[test]
    fn a_second_registration_keeps_the_store_s_share() {
        store::init();
        assert_eq!(register(30), 1);
        assert_eq!(
            store::inq_value(RENDER_SCALE),
            Some(PrefValue::Int(LEAST as i32))
        );
        assert!(store::set_value(RENDER_SCALE, PrefValue::Int(85)));
        assert_eq!(register(100), 0);
        assert_eq!(store::inq_value(RENDER_SCALE), Some(PrefValue::Int(85)));
        store::clear();
    }

    /// The profile keeps an older profile's share.
    #[test]
    fn the_profile_keeps_the_render_scale() {
        store::init();
        register(ALL);
        let ini =
            crate::persist::preferences::UserPreferences::parse("[Display]\r\nRenderScale=50\r\n")
                .unwrap();
        store::load(&ini);
        assert_eq!(store::inq_value(RENDER_SCALE), Some(PrefValue::Int(50)));
        let text = store::save().to_text();
        assert!(text.contains("RenderScale=50"), "{text}");
        store::clear();
    }

    /// The words the option reads and writes are its share in percent.
    #[test]
    fn a_share_reads_and_writes_as_its_percent() {
        assert_eq!(parse_value(RENDER_SCALE, "75"), Some(75));
        assert_eq!(parse_value(RENDER_SCALE, " 67 % "), Some(67));
        assert_eq!(parse_value(RENDER_SCALE, "40"), None);
        assert_eq!(parse_value(RENDER_SCALE, "big"), None);
        assert_eq!(parse_value("Display.FullScreen", "75"), None);
        assert_eq!(
            convert_to_string(RENDER_SCALE, &PrefValue::Int(85)).as_deref(),
            Some("85%")
        );
        assert_eq!(convert_to_string("Other.Name", &PrefValue::Int(85)), None);
    }

    /// The render scale offers the shares then Dynamic, reads a multiple or a percent, and writes a
    /// multiple, or Dynamic, which reads back.
    #[test]
    fn the_enhanced_render_scale_reads_a_multiple_or_a_percent_and_offers_dynamic() {
        let fidelity = FIDELITY_RENDER_SCALE;
        assert!(is_a_scale(fidelity) && is_a_scale(RENDER_SCALE));
        assert_eq!(parse_value(fidelity, "0.5x"), Some(50));
        assert_eq!(parse_value(fidelity, "75%"), Some(75));
        assert_eq!(parse_value(RENDER_SCALE, "0.75x"), Some(75));
        assert_eq!(
            parse_value(RENDER_SCALE, "Dynamic"),
            None,
            "the display's has none"
        );
        assert_eq!(
            convert_to_string(fidelity, &PrefValue::Int(50)).as_deref(),
            Some("0.5x")
        );
        assert_eq!(
            convert_to_string(fidelity, &PrefValue::Int(100)).as_deref(),
            Some("1x")
        );
        let dynamic = convert_to_string(fidelity, &PrefValue::Int(1)).unwrap();
        assert_eq!(dynamic, "Dynamic");
        assert_eq!(parse_value(fidelity, &dynamic), Some(1));
        assert_eq!(parse_value(fidelity, "Dynamic (Default)"), Some(1));
        let rows: Vec<(String, i32)> = choice_rows(fidelity)
            .unwrap()
            .into_iter()
            .map(|c| (c.label, c.value))
            .collect();
        assert_eq!(
            rows,
            [
                ("100%".to_owned(), 100),
                ("77%".to_owned(), 77),
                ("67%".to_owned(), 67),
                ("58%".to_owned(), 58),
                ("50%".to_owned(), 50),
                ("Dynamic".to_owned(), 1)
            ]
        );
    }
}
