//! The quality preset chosen for this machine's graphics card: a table over the card's name,
//! maker and kind ([`Adapter`]) picks Low, Medium, High or Ultra ([`suggest`]).
//!
//! A profile that names none of the presets' options gets the suggestion at the start
//! ([`choose_at_start`]), and the store keeps which preset was chosen and for which card
//! ([`CHOSEN_PRESET`], [`CHOSEN_FOR`]). A new card gets a new choice only while the settings are
//! still the preset chosen for the old one, so a player's own choices are never overwritten.
//!
//! The graphics API gives no memory size, core count or clock, so the table goes by name: Apple's
//! chips by generation and Pro, Max or Ultra; NVIDIA, AMD and Intel cards by series and number;
//! anything else by whether it is a card of its own (Medium) or built in (Low).

use super::fidelity::{Preset, PRESET_VALUES};
use crate::view::PrefValue;

/// `[Quality] ChosenPreset`: the preset chosen for this machine, 1 Low to 4 Ultra; 0 none yet.
pub const CHOSEN_PRESET: &str = "Quality.ChosenPreset";

/// `[Quality] ChosenFor`: the graphics card [`CHOSEN_PRESET`] was chosen for ([`Adapter::id`]).
pub const CHOSEN_FOR: &str = "Quality.ChosenFor";

/// What kind of graphics device it is, as the graphics API says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Built into the processor, sharing its memory. Apple's chips say this too.
    Integrated,
    /// A card of its own.
    Discrete,
    /// A virtual machine's.
    Virtual,
    /// Drawn by the processor in software.
    Software,
    /// Not said.
    Other,
}

/// The graphics device the client draws with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    /// Its name, as the driver gives it.
    pub name: String,
    /// Its maker's PCI vendor number; 0 where the API gives none.
    pub vendor: u32,
    /// What kind it is.
    pub kind: Kind,
}

const NVIDIA: u32 = 0x10DE;
const AMD: u32 = 0x1002;
const INTEL: u32 = 0x8086;

impl Adapter {
    /// A number standing for the card, kept with the choice: a hash of its name, below 2^31.
    #[must_use]
    pub fn id(&self) -> i32 {
        let mut h: u32 = 0x811C_9DC5;
        for b in self.name.trim().bytes() {
            h = (h ^ u32::from(b.to_ascii_lowercase())).wrapping_mul(0x0100_0193);
        }
        i32::try_from(h >> 1).unwrap_or(0)
    }

    /// Its name for the page, without the makers' marks.
    #[must_use]
    pub fn short_name(&self) -> String {
        let mut s = self.name.replace("(R)", "").replace("(TM)", "");
        for word in ["NVIDIA ", " GPU"] {
            s = s.replace(word, "");
        }
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn is(&self, vendor: u32, words: &[&str]) -> bool {
        self.vendor == vendor || words.iter().any(|w| self.lower().contains(w))
    }

    fn lower(&self) -> String {
        self.name.to_ascii_lowercase()
    }
}

const LOW: Preset = Preset(0);
const MEDIUM: Preset = Preset(1);
const HIGH: Preset = Preset(2);
const ULTRA: Preset = Preset(3);

/// The preset that fits `adapter`.
#[must_use]
pub fn suggest(adapter: &Adapter) -> Preset {
    let name = adapter.lower();
    let tier = apple(&name)
        .or_else(|| {
            adapter
                .is(NVIDIA, &["nvidia", "geforce"])
                .then(|| nvidia(&name))
                .flatten()
        })
        .or_else(|| adapter.is(AMD, &["radeon"]).then(|| amd(&name)).flatten())
        .or_else(|| {
            adapter
                .is(INTEL, &["intel"])
                .then(|| intel(&name))
                .flatten()
        });
    tier.unwrap_or(match adapter.kind {
        Kind::Discrete | Kind::Other => MEDIUM,
        Kind::Integrated | Kind::Virtual | Kind::Software => LOW,
    })
}

/// The first number in `s` after `word`, and the letters straight after it.
fn number_after<'a>(s: &'a str, word: &str) -> Option<(u32, &'a str)> {
    let at = s.find(word)? + word.len();
    let rest = s[at..].trim_start();
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let n = rest[..digits].parse().ok()?;
    Some((n, &rest[digits..]))
}

/// One step down, for a laptop's part of the same name.
const fn down(p: Preset) -> Preset {
    Preset(p.0.saturating_sub(1))
}

/// Apple's M chips: the generation and whether Pro, Max or Ultra.
fn apple(name: &str) -> Option<Preset> {
    let (generation, rest) = number_after(name, "apple m")?;
    let newer = generation >= 3;
    let rest = rest.trim_start();
    Some(if rest.starts_with("max") || rest.starts_with("ultra") {
        if newer {
            ULTRA
        } else {
            HIGH
        }
    } else if rest.starts_with("pro") {
        if newer {
            HIGH
        } else {
            MEDIUM
        }
    } else if newer {
        MEDIUM
    } else {
        LOW
    })
}

/// NVIDIA's GeForce RTX and GTX: the series and the class, 50 to 90.
fn nvidia(name: &str) -> Option<Preset> {
    let laptop = name.contains("laptop") || name.contains("mobile") || name.contains("max-q");
    let p = if let Some((n, _)) = number_after(name, "rtx").filter(|(n, _)| *n >= 2000) {
        let (series, class) = (n / 1000, n % 1000);
        match class {
            80.. if series >= 3 => ULTRA,
            80.. => HIGH,
            70 if series >= 4 => ULTRA,
            70 => HIGH,
            60 if series >= 3 => HIGH,
            60 => MEDIUM,
            _ if series >= 3 => MEDIUM,
            _ => LOW,
        }
    } else if let Some((n, _)) = number_after(name, "gtx") {
        if n % 100 >= 70 {
            MEDIUM
        } else {
            LOW
        }
    } else {
        return name.contains(" mx").then_some(LOW);
    };
    Some(if laptop { down(p) } else { p })
}

/// AMD's Radeon RX by series and class, and the larger built-in Radeons by name.
fn amd(name: &str) -> Option<Preset> {
    if ["8060s", "8050s"].iter().any(|w| name.contains(w)) {
        return Some(HIGH);
    }
    if ["780m", "880m", "890m"].iter().any(|w| name.contains(w)) {
        return Some(MEDIUM);
    }
    let (n, after) = number_after(name, "rx")?;
    if n < 1000 {
        // RX 400 to 500 and Vega.
        return Some(LOW);
    }
    let series = n / 1000;
    let class = if series >= 9 {
        n % 100 / 10
    } else {
        n % 1000 / 100
    };
    let p = match (series, class) {
        (7.., 8..) | (9.., 7) => ULTRA,
        (7.., 7) | (9.., 6) | (6, 8..) | (6, 7) => HIGH,
        (6.., 6) | (5, 7..) => MEDIUM,
        _ => LOW,
    };
    let laptop = after.starts_with('m') || after.starts_with('s');
    Some(if laptop { down(p) } else { p })
}

/// Intel's Arc cards by series; the built-in ones fall to Low.
fn intel(name: &str) -> Option<Preset> {
    let at = name.find("arc")?;
    let words: Vec<&str> = name[at + 3..].split_whitespace().collect();
    let model = words.iter().find(|w| {
        let mut c = w.chars();
        matches!(c.next(), Some('a' | 'b')) && c.next().is_some_and(|d| d.is_ascii_digit())
    })?;
    let (letter, digit) = (model.as_bytes()[0], model.as_bytes()[1]);
    Some(match (letter, digit) {
        (b'b', b'5'..) => HIGH,
        (_, b'5'..) => MEDIUM,
        _ => LOW,
    })
}

/// Register the two values the choice is kept in, both none yet.
pub fn register() -> usize {
    [CHOSEN_PRESET, CHOSEN_FOR]
        .into_iter()
        .filter(|name| {
            super::store::register_preference(name, PrefValue::Int(0), super::store::DataType::UInt)
        })
        .count()
}

/// Whether a profile holding the keys `keys` sets any option a preset sets: one the player, or an
/// earlier run of the client, has kept.
#[must_use]
pub fn names_a_preset_option<'a>(mut keys: impl Iterator<Item = &'a str>) -> bool {
    keys.any(|key| {
        PRESET_VALUES
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(key.trim()))
    })
}

/// The preset kept as chosen for this machine, and the card it was chosen for.
#[must_use]
pub fn chosen(read: impl Fn(&str) -> Option<PrefValue>) -> Option<(Preset, i32)> {
    let int = |name| match read(name) {
        Some(PrefValue::Int(i)) => Some(i),
        _ => None,
    };
    let preset = usize::try_from(int(CHOSEN_PRESET)?).ok()?.checked_sub(1)?;
    (preset < super::fidelity::PRESET_NAMES.len())
        .then(|| (Preset(preset), int(CHOSEN_FOR).unwrap_or(0)))
}

/// The preset to set at the start for `adapter`, when one is due: for a profile that names no
/// preset option and has no choice kept, or for a new card while the settings are still the
/// preset chosen for the old one. `None` leaves the settings as they are.
#[must_use]
pub fn choose_at_start(
    adapter: &Adapter,
    profile_names_a_preset_option: bool,
    read: impl Fn(&str) -> Option<PrefValue>,
) -> Option<Preset> {
    match chosen(&read) {
        None if profile_names_a_preset_option => None,
        None => Some(suggest(adapter)),
        Some((_, card)) if card == adapter.id() => None,
        Some((preset, _)) => preset.in_use(&read).then(|| suggest(adapter)),
    }
}

/// What choosing `preset` for `adapter` sets: the preset's values and the kept choice.
#[must_use]
pub fn values_choosing(preset: Preset, adapter: &Adapter) -> Vec<(&'static str, PrefValue)> {
    let number = i32::try_from(preset.0 + 1).unwrap_or(0);
    let mut values = preset.values();
    values.push((CHOSEN_PRESET, PrefValue::Int(number)));
    values.push((CHOSEN_FOR, PrefValue::Int(adapter.id())));
    values
}

/// The caption of the row under Quality Preset.
pub const SUGGESTION_CAPTION: &str = "Suggested Preset";

/// What the row does, for its tooltip.
pub const SUGGESTION_NOTE: &str = "The quality preset that fits this graphics card. A new \
                                   player starts with it, and a new graphics card brings it while \
                                   the settings are still the ones chosen for the old card. Your \
                                   own changes are kept.";

/// The button that sets the suggested preset.
#[must_use]
pub fn use_label(adapter: &Adapter) -> String {
    format!("Use {}", suggest(adapter).name())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, vendor: u32, kind: Kind) -> Adapter {
        Adapter {
            name: name.to_owned(),
            vendor,
            kind,
        }
    }

    fn pick(name: &str, vendor: u32, kind: Kind) -> &'static str {
        suggest(&card(name, vendor, kind)).name()
    }

    #[test]
    fn apple_chips_go_by_generation_and_size() {
        let mac = |name| pick(name, 0, Kind::Integrated);
        assert_eq!(mac("Apple M1"), "Low");
        assert_eq!(mac("Apple M2"), "Low");
        assert_eq!(mac("Apple M1 Pro"), "Medium");
        assert_eq!(mac("Apple M1 Max"), "High");
        assert_eq!(mac("Apple M2 Ultra"), "High");
        assert_eq!(mac("Apple M3"), "Medium");
        assert_eq!(mac("Apple M4"), "Medium");
        assert_eq!(mac("Apple M5 Pro"), "High");
        assert_eq!(mac("Apple M4 Max"), "Ultra");
        assert_eq!(mac("Apple M3 Ultra"), "Ultra");
        assert_eq!(mac("Apple M9"), "Medium");
    }

    #[test]
    fn nvidia_cards_go_by_series_and_class() {
        let pc = |name| pick(name, NVIDIA, Kind::Discrete);
        assert_eq!(pc("NVIDIA GeForce RTX 4090"), "Ultra");
        assert_eq!(pc("NVIDIA GeForce RTX 3080"), "Ultra");
        assert_eq!(pc("NVIDIA GeForce RTX 2080 Ti"), "High");
        assert_eq!(pc("NVIDIA GeForce RTX 5070"), "Ultra");
        assert_eq!(pc("NVIDIA GeForce RTX 3070"), "High");
        assert_eq!(pc("NVIDIA GeForce RTX 4060 Ti"), "High");
        assert_eq!(pc("NVIDIA GeForce RTX 2060 SUPER"), "Medium");
        assert_eq!(pc("NVIDIA GeForce RTX 3050"), "Medium");
        assert_eq!(pc("NVIDIA GeForce GTX 1080"), "Medium");
        assert_eq!(pc("NVIDIA GeForce GTX 1660 SUPER"), "Low");
        assert_eq!(pc("NVIDIA GeForce MX450"), "Low");
        assert_eq!(pc("NVIDIA GeForce RTX 4070 Laptop GPU"), "High");
        assert_eq!(pc("NVIDIA GeForce RTX 3060 Laptop GPU"), "Medium");
    }

    #[test]
    fn amd_cards_go_by_series_and_class() {
        let pc = |name| pick(name, AMD, Kind::Discrete);
        assert_eq!(pc("AMD Radeon RX 7900 XTX"), "Ultra");
        assert_eq!(pc("AMD Radeon RX 9070 XT"), "Ultra");
        assert_eq!(pc("AMD Radeon RX 9060 XT"), "High");
        assert_eq!(pc("AMD Radeon RX 7700 XT"), "High");
        assert_eq!(pc("AMD Radeon RX 6800 XT"), "High");
        assert_eq!(pc("AMD Radeon RX 6600"), "Medium");
        assert_eq!(pc("AMD Radeon RX 5700 XT"), "Medium");
        assert_eq!(pc("AMD Radeon RX 6500 XT"), "Low");
        assert_eq!(pc("Radeon RX 580 Series"), "Low");
        assert_eq!(pc("AMD Radeon RX 7600M XT"), "Low");
        let built_in = |name| pick(name, AMD, Kind::Integrated);
        assert_eq!(built_in("AMD Radeon 780M"), "Medium");
        assert_eq!(built_in("AMD Radeon 8060S Graphics"), "High");
        assert_eq!(built_in("AMD Radeon(TM) Graphics"), "Low");
    }

    #[test]
    fn intel_arc_cards_go_by_series_and_built_in_graphics_are_low() {
        assert_eq!(
            pick("Intel(R) Arc(TM) B580 Graphics", INTEL, Kind::Discrete),
            "High"
        );
        assert_eq!(
            pick("Intel(R) Arc(TM) A770 Graphics", INTEL, Kind::Discrete),
            "Medium"
        );
        assert_eq!(
            pick("Intel(R) Arc(TM) A380 Graphics", INTEL, Kind::Discrete),
            "Low"
        );
        assert_eq!(
            pick("Intel(R) UHD Graphics 770", INTEL, Kind::Integrated),
            "Low"
        );
        assert_eq!(
            pick("Intel(R) Arc(TM) Graphics", INTEL, Kind::Integrated),
            "Low"
        );
        assert_eq!(
            pick("Intel(R) Iris(R) Plus Graphics", INTEL, Kind::Integrated),
            "Low"
        );
    }

    #[test]
    fn an_unknown_card_goes_by_its_kind() {
        assert_eq!(pick("Some New Card 9000", 0x1234, Kind::Discrete), "Medium");
        assert_eq!(
            pick("Snapdragon X Elite Adreno", 0x5143, Kind::Integrated),
            "Low"
        );
        assert_eq!(
            pick("Microsoft Basic Render Driver", 0x1414, Kind::Software),
            "Low"
        );
        assert_eq!(
            pick("llvmpipe (LLVM 17.0.6, 256 bits)", 0x10005, Kind::Software),
            "Low"
        );
        assert_eq!(pick("Virtual GPU", 0, Kind::Virtual), "Low");
        assert_eq!(pick("Mystery", 0, Kind::Other), "Medium");
        assert_eq!(
            pick("NVIDIA Quadro P2000", NVIDIA, Kind::Discrete),
            "Medium"
        );
    }

    #[test]
    fn the_short_name_leaves_out_the_makers_marks() {
        let named = |name: &str| card(name, 0, Kind::Discrete).short_name();
        assert_eq!(
            named("NVIDIA GeForce RTX 4070 Laptop GPU"),
            "GeForce RTX 4070 Laptop"
        );
        assert_eq!(
            named("Intel(R) Arc(TM) B580 Graphics"),
            "Intel Arc B580 Graphics"
        );
        assert_eq!(named("Apple M5 Pro"), "Apple M5 Pro");
    }

    fn profile(values: Vec<(&'static str, PrefValue)>) -> impl Fn(&str) -> Option<PrefValue> {
        let medium = MEDIUM.values();
        move |name: &str| {
            values
                .iter()
                .chain(medium.iter())
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v.clone())
                .or_else(|| {
                    [CHOSEN_PRESET, CHOSEN_FOR]
                        .contains(&name)
                        .then_some(PrefValue::Int(0))
                })
        }
    }

    #[test]
    fn a_new_profile_gets_the_suggestion_and_a_kept_one_is_left_alone() {
        let m5 = card("Apple M5 Pro", 0, Kind::Integrated);
        assert_eq!(choose_at_start(&m5, false, profile(vec![])), Some(HIGH));
        assert_eq!(choose_at_start(&m5, true, profile(vec![])), None);
    }

    #[test]
    fn the_same_card_is_not_chosen_for_again() {
        let m5 = card("Apple M5 Pro", 0, Kind::Integrated);
        let kept = profile(values_choosing(HIGH, &m5));
        assert_eq!(choose_at_start(&m5, true, kept), None);
    }

    #[test]
    fn a_new_card_is_chosen_for_only_while_the_old_choice_stands() {
        let old = card("Apple M1", 0, Kind::Integrated);
        let new = card("NVIDIA GeForce RTX 4090", NVIDIA, Kind::Discrete);
        let kept = profile(values_choosing(LOW, &old));
        assert_eq!(choose_at_start(&new, true, kept), Some(ULTRA));
        let mut changed = values_choosing(LOW, &old);
        changed.insert(
            0,
            (
                super::super::names::fidelity::SHADOWS,
                PrefValue::Bool(true),
            ),
        );
        assert_eq!(choose_at_start(&new, true, profile(changed)), None);
    }

    #[test]
    fn a_profile_naming_a_preset_option_is_a_kept_one() {
        let keys = ["Net.Server", "Render.LandscapeDrawDistance"];
        assert!(names_a_preset_option(keys.into_iter()));
        assert!(!names_a_preset_option(
            ["Net.Server", "Sound.Volume"].into_iter()
        ));
    }

    #[test]
    fn the_card_number_is_steady_and_kept_as_a_positive_number() {
        let a = card("Apple M5 Pro", 0, Kind::Integrated);
        let b = card(" apple m5 pro ", 0, Kind::Integrated);
        assert_eq!(a.id(), b.id());
        assert!(a.id() >= 0);
        assert_ne!(a.id(), card("Apple M5 Max", 0, Kind::Integrated).id());
    }
}
