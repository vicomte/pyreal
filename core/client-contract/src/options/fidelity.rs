//! The words and rules of the high-fidelity presentation's options: fourteen check boxes, the
//! Depth Mapping menu ([`super::depth_mapping`]), the God Rays menu ([`super::god_rays`]), the
//! Cloud Detail menu ([`super::cloud_detail`]), the Water Detail menu ([`super::water_detail`]),
//! the Landscape Smoothing menu
//! ([`super::landscape_smoothing`]), the Ground Cover menu ([`super::draw_distance`]), the Grass
//! Height menu ([`super::grass_height`]), the Light
//! Draw Distance menu ([`super::light_distance`]) and the Shadow Distance menu
//! ([`super::shadow_distance`]) on the Horizon interface's Display page, among its topics'
//! headings.
//!
//! The presentation always draws the better lighting. The
//! lamps also need a graphics device that traces rays. [`blocked`] says which of these holds for a
//! box, and the presentation follows the same rules, so a box the page leaves out or greys draws
//! nothing. (The bounced light darkens the creases itself, so the ambient occlusion stands aside in
//! the outdoor frames it is drawn into; indoors and underground, where it draws nothing, the
//! occlusion is drawn.) The water is drawn with reflections, and the art pack's ground always has
//! that water. The better lighting smooths the world's edges with temporal anti-aliasing, which a
//! box turns off; the art pack's ground lays scree and rock on steep slopes, which a box turns off;
//! the wind moves the pack's trees and plants, which a box stops; and two boxes choose how its
//! frames reach the display. The Depth Mapping menu gives the world's own pictures the relief the
//! pack has for them, and a box keeps the damp and slime it lays on a room's surfaces. God Rays
//! shows the sunlight in the air, Cloud Detail sets how finely the clouds are drawn, Water Detail
//! how much the water's surface rises and falls, and Landscape Smoothing how rounded the land is
//! drawn. The Ground
//! Cover menu chooses how far the pack's plants are drawn, or leaves them out, and Grass Height
//! how tall they grow. The flames' light
//! flickers, which a box holds steady, Light Draw Distance chooses how far the lamps light the
//! world, and Shadow Distance how far the sun's shadows reach. A box draws the sharper pictures
//! of `client_highres.dat` where it is installed, from the next start.
//!
//! Two menus follow the switches: the Render Scale, the share of the frame's pixels the better
//! lighting draws the world at, or Dynamic, and the frame rate a Dynamic render scale holds
//! ([`super::dynamic_resolution`]). A picture drawn at fewer pixels is rebuilt at the frame's size
//! from the frames before it: by MetalFX on macOS, by the better lighting's own anti-aliasing
//! elsewhere.
//!
//! The effects start on. The quality presets set several of them at once, and the terrain's draw
//! distance, the portal style and Detailed Faces with them ([`PRESET_VALUES`]); the defaults of the boxes and the
//! menus are the Medium preset's. The Display tab's other options are the player's own
//! ([`LEFT_TO_THE_PLAYER`]).
//!
//! The options are registered with the value store ([`register`]), so the page reads and writes
//! them like any other option and the profile keeps them in its `[Fidelity]` section.

use super::names::fidelity;
use crate::PrefValue::{self, Int};

/// What a box needs before it draws anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    /// Nothing: it draws on its own.
    Nothing,
    /// A graphics device that traces rays.
    Rays,
    /// Depth mapping on: the relief it is drawn with.
    Relief,
}

/// One box as the page shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FidelityOption {
    /// The preference name, `Fidelity.<Key>`.
    pub name: &'static str,
    /// The caption on the page.
    pub caption: &'static str,
    /// What the effect does, for the row's tooltip.
    pub note: &'static str,
    /// What it needs.
    pub needs: Needs,
    /// Whether the box is ticked while the preference is off: it names what stays on, and the
    /// preference what is turned off.
    pub inverted: bool,
    /// The preference's value until the player changes it.
    pub on_by_default: bool,
}

/// The eleven boxes.
pub const OPTIONS: [FidelityOption; 11] = [
    FidelityOption {
        name: fidelity::SHADOWS,
        caption: "Sun Shadows",
        note: "Casts soft shadows from the sun across the land, buildings, trees and scenery.",
        needs: Needs::Nothing,
        inverted: false,
        on_by_default: true,
    },
    FidelityOption {
        name: fidelity::GLOBAL_ILLUMINATION,
        caption: "Global Illumination",
        note: "Bounces sunlight off nearby surfaces to fill outdoor shadows with soft, coloured \
               light.",
        needs: Needs::Nothing,
        inverted: false,
        on_by_default: true,
    },
    FidelityOption {
        name: fidelity::AMBIENT_OCCLUSION,
        caption: "Ambient Occlusion",
        note: "Darkens creases, corners and the ground under objects, where less light reaches.",
        needs: Needs::Nothing,
        inverted: false,
        on_by_default: true,
    },
    FidelityOption {
        name: fidelity::WEATHER,
        caption: "Weather Effects",
        note: "Draws rain or snow on the game's rainy days, with wet ground, puddles and \
               lightning.",
        needs: Needs::Nothing,
        inverted: false,
        on_by_default: true,
    },
    FidelityOption {
        name: fidelity::NO_ANTI_ALIASING,
        caption: "Anti-Aliasing",
        note: "Smooths jagged, shimmering edges by blending recent frames. It stays on whenever \
               Render Scale is below 100%.",
        needs: Needs::Nothing,
        inverted: true,
        on_by_default: false,
    },
    FidelityOption {
        name: fidelity::VSYNC,
        caption: "VSync",
        note: "Shows each frame in step with the display's refresh. This prevents tearing and \
               keeps the pace even, with a little more input delay. It also caps the frame rate, \
               which lowers the load.",
        needs: Needs::Nothing,
        inverted: false,
        on_by_default: true,
    },
    FidelityOption {
        name: fidelity::HDR,
        caption: "HDR Output",
        note: "Lets lamps, portals and sun glints shine brighter than white on an HDR display.",
        needs: Needs::Nothing,
        inverted: false,
        on_by_default: false,
    },
    FidelityOption {
        name: fidelity::NO_WIND,
        caption: "Foliage Wind",
        note: "Trees and plants sway in the wind.",
        needs: Needs::Nothing,
        inverted: true,
        on_by_default: false,
    },
    FidelityOption {
        name: fidelity::NO_FLAME_FLICKER,
        caption: "Flame Flicker",
        note: "Makes torch, brazier and candle light flicker and their shadows sway. Turn it off \
               if flickering light bothers you.",
        needs: Needs::Nothing,
        inverted: true,
        on_by_default: false,
    },
    FidelityOption {
        name: fidelity::NO_SILK_COBWEBS,
        caption: "Silk Cobwebs",
        note: "Draws dungeon cobwebs as fine, drifting silk.",
        needs: Needs::Nothing,
        inverted: true,
        on_by_default: false,
    },
    FidelityOption {
        name: fidelity::NO_HIGH_RES_TEXTURES,
        caption: "High-Res Textures",
        note: "Draws the sharper textures from client_highres.dat when it is installed. Takes \
               effect at the next start.",
        needs: Needs::Nothing,
        inverted: true,
        on_by_default: false,
    },
];

/// The menus among the effects' rows, each with what it needs. A menu's own module registers it
/// and gives its choices; a row here gives it the boxes' rules.
pub const MENUS: &[(&str, Needs)] = &[
    (fidelity::RENDER_SCALE, Needs::Nothing),
    (fidelity::DYNAMIC_RESOLUTION, Needs::Nothing),
    (fidelity::DEPTH_MAPPING, Needs::Nothing),
    (fidelity::GOD_RAYS, Needs::Nothing),
    (fidelity::CLOUD_DETAIL, Needs::Nothing),
    (fidelity::WATER_DETAIL, Needs::Nothing),
    (fidelity::LANDSCAPE_SMOOTHING, Needs::Nothing),
    (fidelity::LIGHT_DRAW_DISTANCE, Needs::Nothing),
    (fidelity::SHADOW_DISTANCE, Needs::Nothing),
    (fidelity::GROUND_COVER_DRAW_DISTANCE, Needs::Nothing),
    (fidelity::GRASS_HEIGHT, Needs::Nothing),
    (fidelity::FOG_DENSITY, Needs::Nothing),
    (fidelity::OBJECT_DETAIL, Needs::Nothing),
];

/// The render scale until the player changes it: Dynamic, as the Medium preset has it.
pub const SCALE_DEFAULT: u32 = super::render_scale::DYNAMIC;

/// The render scale menu's caption.
pub const SCALE_CAPTION: &str = "Render Scale";

/// What the render scale does, for the row's tooltip.
#[cfg(target_os = "macos")]
pub const SCALE_NOTE: &str =
    "Draws the world at fewer pixels, and MetalFX enlarges it to fill the \
                              screen. Lower values run faster and look a little softer. Dynamic \
                              lowers it only when needed to hold the target frame rate.";

/// What the render scale does, for the row's tooltip.
#[cfg(not(target_os = "macos"))]
pub const SCALE_NOTE: &str =
    "Draws the world at fewer pixels and rebuilds the picture from recent \
                              frames to fill the screen. Lower values run faster and look a little \
                              softer. Dynamic lowers it only when needed to hold the target frame \
                              rate.";

/// The render scale value of a Dynamic render scale.
#[allow(clippy::cast_possible_wrap)] // a small constant
const DYNAMIC: i32 = super::render_scale::DYNAMIC as i32;

/// The heading over the quality presets, first on the Display tab.
pub const QUALITY_HEADING: &str = "Quality";

/// The caption of the quality presets.
pub const PRESET_CAPTION: &str = "Quality Preset";

/// What the presets do, for the row's tooltip.
pub const PRESET_NOTE: &str = "Sets the graphics options below together, from Low to Ultra. \
                               Changing any of them afterwards shows Custom.";

/// The quality presets' names, least first.
pub const PRESET_NAMES: [&str; 4] = ["Low", "Medium", "High", "Ultra"];

/// The name for settings that match no preset.
pub const CUSTOM_PRESET: &str = "Custom";

/// The terrain draw distance until the player changes it: Very High, as the Medium preset has it.
pub const TERRAIN_DEFAULT: i32 = 15;

const ON: PrefValue = PrefValue::Bool(true);
const OFF: PrefValue = PrefValue::Bool(false);

/// What the quality presets set: each row a preference and its value in each preset, in
/// [`PRESET_NAMES`] order. A box that names what stays on is off to keep it.
pub const PRESET_VALUES: &[(&str, [PrefValue; PRESET_NAMES.len()])] = &[
    (fidelity::SHADOWS, [OFF, ON, ON, ON]),
    (
        super::names::LANDSCAPE_DRAW_DISTANCE,
        [Int(11), Int(TERRAIN_DEFAULT), Int(48), Int(127)],
    ),
    (fidelity::GLOBAL_ILLUMINATION, [OFF, ON, ON, ON]),
    (fidelity::AMBIENT_OCCLUSION, [OFF, ON, ON, ON]),
    (fidelity::WEATHER, [OFF, ON, ON, ON]),
    (fidelity::NO_ANTI_ALIASING, [ON, OFF, OFF, OFF]),
    (
        fidelity::DEPTH_MAPPING,
        [
            Int(super::depth_mapping::OFF),
            Int(super::depth_mapping::MEDIUM),
            Int(super::depth_mapping::MEDIUM),
            Int(super::depth_mapping::MEDIUM),
        ],
    ),
    (fidelity::GOD_RAYS, [Int(0), Int(1), Int(1), Int(1)]),
    (
        fidelity::CLOUD_DETAIL,
        [
            Int(super::cloud_detail::LOW),
            Int(super::cloud_detail::MEDIUM),
            Int(super::cloud_detail::HIGH),
            Int(super::cloud_detail::HIGH),
        ],
    ),
    (
        fidelity::WATER_DETAIL,
        [
            Int(super::water_detail::LOW),
            Int(super::water_detail::MEDIUM),
            Int(super::water_detail::HIGH),
            Int(super::water_detail::HIGH),
        ],
    ),
    (fidelity::NO_WIND, [ON, OFF, OFF, OFF]),
    (fidelity::NO_SILK_COBWEBS, [ON, OFF, OFF, OFF]),
    (fidelity::NO_HIGH_RES_TEXTURES, [ON, OFF, OFF, OFF]),
    (
        fidelity::LANDSCAPE_SMOOTHING,
        [
            Int(super::landscape_smoothing::OFF),
            Int(super::landscape_smoothing::MEDIUM),
            Int(super::landscape_smoothing::MEDIUM),
            Int(super::landscape_smoothing::MEDIUM),
        ],
    ),
    (
        super::names::PORTAL_STYLE,
        [
            Int(super::portal_style::ORIGINAL),
            Int(super::portal_style::ENHANCED),
            Int(super::portal_style::ENHANCED),
            Int(super::portal_style::ENHANCED),
        ],
    ),
    (super::names::DETAILED_FACES, [OFF, ON, ON, ON]),
    (
        fidelity::GROUND_COVER_DRAW_DISTANCE,
        [Int(super::draw_distance::OFF), Int(100), Int(300), Int(500)],
    ),
    (
        fidelity::LIGHT_DRAW_DISTANCE,
        [Int(100), Int(150), Int(300), Int(500)],
    ),
    (
        fidelity::SHADOW_DISTANCE,
        [Int(120), Int(200), Int(320), Int(500)],
    ),
    (
        fidelity::OBJECT_DETAIL,
        [Int(75), Int(150), Int(200), Int(300)],
    ),
    (
        fidelity::RENDER_SCALE,
        [Int(DYNAMIC), Int(DYNAMIC), Int(DYNAMIC), Int(100)],
    ),
    (
        fidelity::DYNAMIC_RESOLUTION,
        [Int(60), Int(60), Int(60), Int(60)],
    ),
];

/// The Display tab's options no preset sets: looks, comfort and the display's own settings.
pub const LEFT_TO_THE_PLAYER: &[&str] = &[
    fidelity::VSYNC,
    fidelity::HDR,
    fidelity::NO_FLAME_FLICKER,
    fidelity::FOG_DENSITY,
    fidelity::GRASS_HEIGHT,
    super::names::GROUND_COLORING,
    super::names::SCREEN_BRIGHTNESS,
    super::names::FIELD_OF_VIEW,
    super::names::DISPLAY_FULL_SCREEN,
    super::names::UI_BLUR,
];

/// One quality preset, by its place in [`PRESET_NAMES`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset(pub usize);

/// The presets, least first.
pub const PRESETS: [Preset; PRESET_NAMES.len()] = [Preset(0), Preset(1), Preset(2), Preset(3)];

impl Preset {
    /// The preset's name.
    #[must_use]
    pub fn name(self) -> &'static str {
        PRESET_NAMES[self.0]
    }

    /// What the preset sets, as preferences.
    #[must_use]
    pub fn values(self) -> Vec<(&'static str, PrefValue)> {
        PRESET_VALUES
            .iter()
            .map(|(name, values)| (*name, values[self.0].clone()))
            .collect()
    }

    /// Whether `read` holds every value the preset sets.
    #[must_use]
    pub fn in_use(self, read: impl Fn(&str) -> Option<PrefValue>) -> bool {
        PRESET_VALUES
            .iter()
            .all(|(name, values)| read(name).as_ref() == Some(&values[self.0]))
    }
}

/// The first preset `read` matches; `None` for settings of the player's own.
#[must_use]
pub fn preset_in_use(read: impl Fn(&str) -> Option<PrefValue>) -> Option<Preset> {
    PRESETS.into_iter().find(|p| p.in_use(&read))
}

/// Why a ticked box draws nothing, as the page says it under the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    /// The graphics device does not trace rays.
    NeedsRays,
    /// Depth mapping is off.
    NeedsDepthMapping,
}

impl Blocked {
    /// The reason line.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NeedsRays => {
                "Needs ray tracing, which this graphics card, driver or backend lacks"
            }
            Self::NeedsDepthMapping => "Needs Depth Mapping",
        }
    }

    /// Whether the page leaves the row off altogether: another box's choice makes it mean
    /// nothing. A device without ray tracing stays, greyed, with its reason: no box changes that.
    #[must_use]
    pub const fn hides_the_row(self) -> bool {
        !matches!(self, Self::NeedsRays)
    }
}

/// The box named `name`.
#[must_use]
pub fn find(name: &str) -> Option<&'static FidelityOption> {
    OPTIONS.iter().find(|o| o.name.eq_ignore_ascii_case(name))
}

/// Whether `name` is one of the effects' menus.
#[must_use]
pub fn is_a_menu(name: &str) -> bool {
    MENUS.iter().any(|(m, _)| m.eq_ignore_ascii_case(name))
}

/// What the box or menu `name` needs.
#[must_use]
pub fn needs(name: &str) -> Option<Needs> {
    MENUS
        .iter()
        .find(|(m, _)| m.eq_ignore_ascii_case(name))
        .map(|(_, needs)| *needs)
        .or_else(|| find(name).map(|o| o.needs))
}

/// Why the box or menu `name` draws nothing however it is set, with `ticked` saying which boxes
/// are ticked (and which menus are off their first choice) and `rays` whether the graphics device
/// traces rays (`None` while that is not known yet, which blocks nothing). `None` when it draws
/// when ticked.
#[must_use]
pub fn blocked(name: &str, ticked: impl Fn(&str) -> bool, rays: Option<bool>) -> Option<Blocked> {
    match needs(name)? {
        Needs::Rays if rays == Some(false) => Some(Blocked::NeedsRays),
        Needs::Relief if !ticked(fidelity::DEPTH_MAPPING) => Some(Blocked::NeedsDepthMapping),
        _ => None,
    }
}

/// What the menu `name` holds until the player changes it.
fn menu_default(name: &str) -> Option<PrefValue> {
    if name.eq_ignore_ascii_case(fidelity::RENDER_SCALE) {
        i32::try_from(SCALE_DEFAULT).ok().map(Int)
    } else if super::dynamic_resolution::is_dynamic_resolution(name) {
        Some(Int(super::dynamic_resolution::DEFAULT))
    } else {
        None
    }
}

/// Register every box with the value store, each at its default, and the Render Scale, Dynamic
/// Resolution, Depth Mapping, God Rays, Cloud Detail, draw distance, Light Draw Distance and
/// Shadow Distance menus at their own.
/// The store's load then reads the `[Fidelity]` section of the profile into them, and the page
/// reads and writes them.
pub fn register() -> usize {
    let boxes: usize = OPTIONS
        .iter()
        .map(|o| {
            usize::from(super::store::register_preference(
                o.name,
                PrefValue::Bool(o.on_by_default),
                super::store::DataType::Bool,
            ))
        })
        .sum();
    let depth = super::store::register_preference(
        fidelity::DEPTH_MAPPING,
        PrefValue::Int(super::depth_mapping::DEFAULT),
        super::store::DataType::UInt,
    );
    let rays = super::store::register_preference(
        fidelity::GOD_RAYS,
        PrefValue::Int(super::god_rays::DEFAULT),
        super::store::DataType::UInt,
    );
    let clouds = super::store::register_preference(
        fidelity::CLOUD_DETAIL,
        PrefValue::Int(super::cloud_detail::DEFAULT),
        super::store::DataType::UInt,
    );
    let water = super::store::register_preference(
        fidelity::WATER_DETAIL,
        PrefValue::Int(super::water_detail::DEFAULT),
        super::store::DataType::UInt,
    );
    let smoothing = super::store::register_preference(
        fidelity::LANDSCAPE_SMOOTHING,
        PrefValue::Int(super::landscape_smoothing::DEFAULT),
        super::store::DataType::UInt,
    );
    let distances = [fidelity::GROUND_COVER_DRAW_DISTANCE]
        .into_iter()
        .filter(|name| {
            super::store::register_preference(
                name,
                PrefValue::Int(super::draw_distance::default_of(name)),
                super::store::DataType::UInt,
            )
        })
        .count();
    let lights = super::store::register_preference(
        fidelity::LIGHT_DRAW_DISTANCE,
        PrefValue::Int(super::light_distance::DEFAULT),
        super::store::DataType::UInt,
    );
    let shadows = super::store::register_preference(
        fidelity::SHADOW_DISTANCE,
        PrefValue::Int(super::shadow_distance::DEFAULT),
        super::store::DataType::UInt,
    );
    let fog = super::store::register_preference(
        fidelity::FOG_DENSITY,
        PrefValue::Int(super::fog_density::DEFAULT),
        super::store::DataType::UInt,
    );
    let detail = super::store::register_preference(
        fidelity::OBJECT_DETAIL,
        PrefValue::Int(super::object_detail::DEFAULT),
        super::store::DataType::UInt,
    );
    let grass = super::store::register_preference(
        fidelity::GRASS_HEIGHT,
        PrefValue::Int(super::grass_height::DEFAULT),
        super::store::DataType::UInt,
    );
    let menus = [fidelity::RENDER_SCALE, fidelity::DYNAMIC_RESOLUTION]
        .into_iter()
        .filter_map(|name| Some((name, menu_default(name)?)))
        .filter(|(name, value)| {
            super::store::register_preference(name, value.clone(), super::store::DataType::UInt)
        })
        .count();
    boxes
        + usize::from(depth)
        + usize::from(rays)
        + usize::from(clouds)
        + usize::from(water)
        + usize::from(smoothing)
        + distances
        + usize::from(lights)
        + usize::from(shadows)
        + usize::from(fog)
        + usize::from(detail)
        + usize::from(grass)
        + menus
}

/// Whether the store's save leaves `name` out of the profile: a box or a menu still at its default
/// that the profile does not name, so a profile that never changed one has no `[Fidelity]`
/// section.
#[must_use]
pub fn left_out_of_save(
    name: &str,
    value: &PrefValue,
    ini: &crate::persist::preferences::UserPreferences,
) -> bool {
    let unchanged = match find(name) {
        Some(o) => *value == PrefValue::Bool(o.on_by_default),
        None if menu_default(name).is_some() => menu_default(name).as_ref() == Some(value),
        None if super::god_rays::is_god_rays(name) => {
            *value == PrefValue::Int(super::god_rays::DEFAULT)
        }
        None if super::cloud_detail::is_cloud_detail(name) => {
            *value == PrefValue::Int(super::cloud_detail::DEFAULT)
        }
        None if super::water_detail::is_water_detail(name) => {
            *value == PrefValue::Int(super::water_detail::DEFAULT)
        }
        None if super::landscape_smoothing::is_landscape_smoothing(name) => {
            *value == PrefValue::Int(super::landscape_smoothing::DEFAULT)
        }
        None if super::light_distance::is_light_distance(name) => {
            *value == PrefValue::Int(super::light_distance::DEFAULT)
        }
        None if super::shadow_distance::is_shadow_distance(name) => {
            *value == PrefValue::Int(super::shadow_distance::DEFAULT)
        }
        None if super::fog_density::is_fog_density(name) => {
            *value == PrefValue::Int(super::fog_density::DEFAULT)
        }
        None if super::object_detail::is_object_detail(name) => {
            *value == PrefValue::Int(super::object_detail::DEFAULT)
        }
        None if super::grass_height::is_grass_height(name) => {
            *value == PrefValue::Int(super::grass_height::DEFAULT)
        }
        None if super::draw_distance::is_a_distance(name) => {
            *value == PrefValue::Int(super::draw_distance::default_of(name))
        }
        None => is_a_menu(name) && *value == PrefValue::Int(super::depth_mapping::DEFAULT),
    };
    unchanged
        && !ini
            .entries
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case(name))
}

/// Where the effects stand on the device the client is drawing with, for the options page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Availability {
    /// The client was built with the effects.
    pub built: bool,
    /// The device was asked at start-up for what they draw with.
    pub widened: bool,
    /// The device has what they draw with.
    pub supported: bool,
    /// Whether the effects trace rays here, which the lamps need; `None` where that is not known.
    pub rays: Option<bool>,
    /// They drew the last frame.
    pub active: bool,
    /// Why they stopped, when they failed on this device and were taken off; they stay off until
    /// a box changes.
    pub failed: Option<String>,
}

/// The longest a failure's reason is shown on the status line, in characters; the log has it
/// whole.
const REASON_SHOWN: usize = 48;

impl Availability {
    /// Whether the boxes can be ticked here: the client has the effects. Elsewhere the page shows
    /// them greyed.
    #[must_use]
    pub fn offered(&self) -> bool {
        self.built
    }

    /// The status line under the heading: whether the effects are drawn, or what stops them,
    /// with `ticked` saying whether any box is ticked. `None` when there is nothing to say.
    #[must_use]
    pub fn status(&self, ticked: bool) -> Option<String> {
        if !self.built || !ticked {
            return None;
        }
        if let Some(why) = &self.failed {
            let mut why: String = why.chars().take(REASON_SHOWN).collect();
            if why.len() < self.failed.as_ref().map_or(0, String::len) {
                why.push_str("...");
            }
            return Some(format!(
                "Stopped after an error ({why}): untick and tick a box to retry"
            ));
        }
        if self.active {
            return Some("Active".to_owned());
        }
        if !self.widened {
            return Some("Restart to apply".to_owned());
        }
        if !self.supported {
            return Some("This graphics backend can't run these effects".to_owned());
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_box_has_its_name_and_the_names_list_holds_every_box() {
        for o in &OPTIONS {
            assert!(fidelity::NAMES.contains(&o.name), "{}", o.name);
            assert_eq!(find(&o.name.to_ascii_lowercase()), Some(o));
            assert_eq!(needs(o.name), Some(o.needs));
        }
        for (name, _) in MENUS {
            assert!(fidelity::NAMES.contains(name), "{name}");
            assert!(find(name).is_none(), "{name} is a menu and a box");
        }
    }

    #[test]
    fn the_effects_start_on_and_a_box_at_its_default_stays_out_of_a_profile() {
        let ini = crate::persist::preferences::UserPreferences::parse("").unwrap();
        for o in &OPTIONS {
            let draws = !matches!(
                o.name,
                fidelity::NO_ANTI_ALIASING
                    | fidelity::NO_WIND
                    | fidelity::HDR
                    | fidelity::NO_FLAME_FLICKER
                    | fidelity::NO_SILK_COBWEBS
                    | fidelity::NO_HIGH_RES_TEXTURES
            );
            assert_eq!(o.on_by_default, draws, "{}", o.name);
            assert!(
                left_out_of_save(o.name, &PrefValue::Bool(draws), &ini),
                "{}",
                o.name
            );
            assert!(
                !left_out_of_save(o.name, &PrefValue::Bool(!draws), &ini),
                "{}",
                o.name
            );
        }
    }

    #[test]
    fn the_presets_go_from_a_light_low_to_a_full_ultra_and_the_boxes_start_at_medium() {
        for (name, _) in PRESET_VALUES {
            assert!(
                fidelity::NAMES.contains(name)
                    || *name == super::super::names::LANDSCAPE_DRAW_DISTANCE
                    || *name == super::super::names::PORTAL_STYLE
                    || *name == super::super::names::DETAILED_FACES,
                "{name}"
            );
        }
        let names: Vec<&str> = PRESETS.iter().map(|p| p.name()).collect();
        assert_eq!(names, PRESET_NAMES);
        // Low turns the costly effects off and the anti-aliasing with them; Ultra has them all.
        let low = PRESETS[0].values();
        assert!(low.contains(&(fidelity::GLOBAL_ILLUMINATION, PrefValue::Bool(false))));
        assert!(low.contains(&(fidelity::SHADOWS, PrefValue::Bool(false))));
        assert!(low.contains(&(fidelity::NO_ANTI_ALIASING, PrefValue::Bool(true))));
        assert!(low.contains(&(
            fidelity::DEPTH_MAPPING,
            Int(super::super::depth_mapping::OFF)
        )));
        assert!(low.contains(&(fidelity::GOD_RAYS, Int(super::super::god_rays::OFF))));
        assert!(low.contains(&(fidelity::CLOUD_DETAIL, Int(super::super::cloud_detail::LOW))));
        assert!(low.contains(&(fidelity::WATER_DETAIL, Int(super::super::water_detail::LOW))));
        assert!(low.contains(&(fidelity::NO_WIND, PrefValue::Bool(true))));
        assert!(low.contains(&(
            fidelity::GROUND_COVER_DRAW_DISTANCE,
            Int(super::super::draw_distance::OFF)
        )));
        assert!(low.contains(&(fidelity::LIGHT_DRAW_DISTANCE, Int(100))));
        assert!(low.contains(&(fidelity::RENDER_SCALE, Int(DYNAMIC))));
        assert!(low.contains(&(fidelity::NO_HIGH_RES_TEXTURES, PrefValue::Bool(true))));
        assert!(low.contains(&(
            super::super::names::PORTAL_STYLE,
            Int(super::super::portal_style::ORIGINAL)
        )));
        assert!(low.contains(&(super::super::names::DETAILED_FACES, PrefValue::Bool(false))));
        let ultra = PRESETS[3].values();
        assert!(ultra.iter().all(|(name, v)| match find(name) {
            Some(o) => *v == PrefValue::Bool(!o.inverted),
            None if *name == fidelity::GOD_RAYS => *v == Int(super::super::god_rays::LOW),
            None if *name == fidelity::CLOUD_DETAIL => *v == Int(super::super::cloud_detail::HIGH),
            None if *name == fidelity::WATER_DETAIL => *v == Int(super::super::water_detail::HIGH),
            None if *name == fidelity::RENDER_SCALE => *v == Int(100),
            None if *name == fidelity::DYNAMIC_RESOLUTION => *v == Int(60),
            None if *name == fidelity::LIGHT_DRAW_DISTANCE => *v == Int(500),
            None if *name == fidelity::SHADOW_DISTANCE => *v == Int(500),
            None if *name == fidelity::OBJECT_DETAIL => *v == Int(300),
            None if *name == super::super::names::LANDSCAPE_DRAW_DISTANCE => *v == Int(127),
            None if *name == super::super::names::PORTAL_STYLE => {
                *v == Int(super::super::portal_style::ENHANCED)
            }
            None if *name == super::super::names::DETAILED_FACES => *v == PrefValue::Bool(true),
            None if *name == fidelity::LANDSCAPE_SMOOTHING => {
                *v == Int(super::super::landscape_smoothing::MEDIUM)
            }
            None if super::super::draw_distance::is_a_distance(name) => *v == Int(500),
            None => *v == Int(super::super::depth_mapping::MEDIUM),
        }));
        // The defaults of the boxes and the menus are the Medium preset's.
        let defaults = |name: &str| match find(name) {
            Some(o) => Some(PrefValue::Bool(o.on_by_default)),
            None if name == super::super::names::LANDSCAPE_DRAW_DISTANCE => {
                Some(Int(TERRAIN_DEFAULT))
            }
            None if name == super::super::names::PORTAL_STYLE => {
                Some(Int(super::super::portal_style::DEFAULT))
            }
            None if name == super::super::names::DETAILED_FACES => Some(PrefValue::Bool(true)),
            None if name == fidelity::LANDSCAPE_SMOOTHING => {
                Some(Int(super::super::landscape_smoothing::DEFAULT))
            }
            None if menu_default(name).is_some() => menu_default(name),
            None if name == fidelity::GOD_RAYS => Some(Int(super::super::god_rays::DEFAULT)),
            None if name == fidelity::CLOUD_DETAIL => {
                Some(Int(super::super::cloud_detail::DEFAULT))
            }
            None if name == fidelity::WATER_DETAIL => {
                Some(Int(super::super::water_detail::DEFAULT))
            }
            None if name == fidelity::LIGHT_DRAW_DISTANCE => {
                Some(Int(super::super::light_distance::DEFAULT))
            }
            None if name == fidelity::SHADOW_DISTANCE => {
                Some(Int(super::super::shadow_distance::DEFAULT))
            }
            None if name == fidelity::OBJECT_DETAIL => {
                Some(Int(super::super::object_detail::DEFAULT))
            }
            None if name == fidelity::GRASS_HEIGHT => {
                Some(Int(super::super::grass_height::DEFAULT))
            }
            None if super::super::draw_distance::is_a_distance(name) => {
                Some(Int(super::super::draw_distance::default_of(name)))
            }
            None => is_a_menu(name).then_some(Int(super::super::depth_mapping::DEFAULT)),
        };
        assert_eq!(preset_in_use(defaults), Some(PRESETS[1]));
        // Each preset is in use once set, and a value of the player's own is none.
        for p in PRESETS {
            let set = p.values();
            let read = |name: &str| set.iter().find(|(n, _)| *n == name).map(|(_, v)| v.clone());
            assert!(p.in_use(read), "{}", p.name());
        }
        let own = |name: &str| {
            if name == fidelity::WEATHER {
                Some(PrefValue::Bool(false))
            } else {
                defaults(name)
            }
        };
        assert_eq!(preset_in_use(own), None);
    }

    /// The Depth Mapping menu starts at Medium, as the Medium preset has it, and the save leaves it
    /// out until it is changed.
    #[test]
    fn the_depth_mapping_menu_starts_at_medium_and_is_left_out_of_the_profile_until_chosen() {
        use crate::persist::preferences::UserPreferences;
        let empty = UserPreferences::parse("").expect("an empty profile");
        let medium = PrefValue::Int(super::super::depth_mapping::MEDIUM);
        assert_eq!(
            super::super::depth_mapping::DEFAULT,
            super::super::depth_mapping::MEDIUM
        );
        assert!(left_out_of_save(fidelity::DEPTH_MAPPING, &medium, &empty));
        assert!(!left_out_of_save(
            fidelity::DEPTH_MAPPING,
            &PrefValue::Int(super::super::depth_mapping::OFF),
            &empty
        ));
        assert!(!left_out_of_save(
            fidelity::DEPTH_MAPPING,
            &PrefValue::Int(super::super::depth_mapping::MOST),
            &empty
        ));
        let named = UserPreferences::parse("[Fidelity]\r\nDepthMapping=Medium\r\n").expect("reads");
        assert!(!left_out_of_save(fidelity::DEPTH_MAPPING, &medium, &named));
    }

    /// Every option on the Display tab's graphics headings is set by every preset or left to the
    /// player, never both, and each preset value is one of its menu's choices.
    #[test]
    fn every_display_option_is_in_the_presets_or_left_to_the_player() {
        use super::super::sheet::{self, PageId};
        super::super::store::init();
        let graphics = [
            sheet::SCREEN_HEADING,
            sheet::LIGHTING_HEADING,
            sheet::SKY_HEADING,
            sheet::SURFACES_HEADING,
        ];
        let shown: Vec<&str> = sheet::page(PageId::Client)
            .headings
            .iter()
            .filter(|h| graphics.contains(&h.title))
            .flat_map(|h| h.rows.iter())
            .filter_map(sheet::Row::preference)
            .collect();
        let preset = |name: &str| PRESET_VALUES.iter().any(|(n, _)| *n == name);
        for name in &shown {
            assert!(
                preset(name) != LEFT_TO_THE_PLAYER.contains(name),
                "{name} must be in exactly one of PRESET_VALUES and LEFT_TO_THE_PLAYER"
            );
        }
        for name in PRESET_VALUES
            .iter()
            .map(|(n, _)| *n)
            .chain(LEFT_TO_THE_PLAYER.iter().copied())
        {
            assert!(shown.contains(&name), "{name} is on no graphics heading");
        }
        for (name, values) in PRESET_VALUES {
            let Some(choices) = super::super::store::choice_rows(name) else {
                assert!(
                    values.iter().all(|v| matches!(v, PrefValue::Bool(_))),
                    "{name}"
                );
                continue;
            };
            for v in values {
                assert!(
                    choices.iter().any(|c| PrefValue::Int(c.value) == *v),
                    "{name} has no choice {v:?}"
                );
            }
        }
    }

    /// The God Rays menu starts at Low, as the Medium preset has it, and the save leaves it out
    /// until it is changed.
    #[test]
    fn the_god_rays_menu_starts_low_and_is_left_out_of_the_profile_until_chosen() {
        use super::super::god_rays::{DEFAULT, LOW, OFF};
        use crate::persist::preferences::UserPreferences;
        let empty = UserPreferences::parse("").expect("an empty profile");
        assert_eq!(DEFAULT, LOW);
        let low = PrefValue::Int(LOW);
        assert!(left_out_of_save(fidelity::GOD_RAYS, &low, &empty));
        assert!(!left_out_of_save(
            fidelity::GOD_RAYS,
            &PrefValue::Int(OFF),
            &empty
        ));
        let named = UserPreferences::parse("[Fidelity]\r\nGodRays=Low\r\n").expect("reads");
        assert!(!left_out_of_save(fidelity::GOD_RAYS, &low, &named));
    }

    /// The ground cover's draw distance starts as the Medium preset has it, and the save leaves it
    /// out until it is changed.
    #[test]
    fn the_ground_cover_starts_at_medium_and_is_left_out_of_the_profile_until_chosen() {
        use super::super::draw_distance::{default_of, OFF};
        use crate::persist::preferences::UserPreferences;
        let empty = UserPreferences::parse("").expect("an empty profile");
        let name = fidelity::GROUND_COVER_DRAW_DISTANCE;
        let medium = PrefValue::Int(default_of(name));
        assert!(left_out_of_save(name, &medium, &empty), "{name}");
        assert!(!left_out_of_save(name, &PrefValue::Int(300), &empty));
        assert!(!left_out_of_save(name, &PrefValue::Int(OFF), &empty));
        assert_eq!(default_of(name), 100);
        let named =
            UserPreferences::parse("[Fidelity]\r\nGroundCoverDrawDistance=1x\r\n").expect("reads");
        assert!(!left_out_of_save(name, &PrefValue::Int(100), &named));
    }

    /// The Light Draw Distance starts at 1.5x, as the Medium preset has it, and the save leaves it
    /// out until it is changed.
    #[test]
    fn the_light_draw_distance_starts_at_the_medium_presets_and_is_left_out_until_chosen() {
        use crate::persist::preferences::UserPreferences;
        let empty = UserPreferences::parse("").expect("an empty profile");
        let whole = PrefValue::Int(super::super::light_distance::DEFAULT);
        assert_eq!(whole, PrefValue::Int(150));
        let name = fidelity::LIGHT_DRAW_DISTANCE;
        assert!(left_out_of_save(name, &whole, &empty));
        assert!(!left_out_of_save(name, &PrefValue::Int(300), &empty));
        let named =
            UserPreferences::parse("[Fidelity]\r\nLightDrawDistance=1x\r\n").expect("reads");
        assert!(!left_out_of_save(name, &whole, &named));
    }

    #[test]
    fn the_render_scale_needs_the_better_lighting_and_stays_out_of_a_profile_until_changed() {
        let with = |on: &'static [&'static str]| move |n: &str| on.contains(&n);
        for name in [fidelity::RENDER_SCALE, fidelity::DYNAMIC_RESOLUTION] {
            assert_eq!(blocked(name, with(&[]), None), None);
            assert!(find(name).is_none(), "{name} is no box");
        }
        let ini = crate::persist::preferences::UserPreferences::parse("").unwrap();
        assert_eq!(SCALE_DEFAULT, super::super::render_scale::DYNAMIC);
        assert!(left_out_of_save(
            fidelity::RENDER_SCALE,
            &Int(DYNAMIC),
            &ini
        ));
        assert!(!left_out_of_save(fidelity::RENDER_SCALE, &Int(100), &ini));
        assert!(left_out_of_save(
            fidelity::DYNAMIC_RESOLUTION,
            &Int(super::super::dynamic_resolution::DEFAULT),
            &ini
        ));
        assert!(!left_out_of_save(
            fidelity::DYNAMIC_RESOLUTION,
            &Int(90),
            &ini
        ));
        let named = crate::persist::preferences::UserPreferences::parse(
            "[Fidelity]\r\nRenderScale=Dynamic\r\n",
        )
        .unwrap();
        assert!(!left_out_of_save(
            fidelity::RENDER_SCALE,
            &Int(DYNAMIC),
            &named
        ));
    }

    /// The Cloud Detail menu is registered at Medium, and the save leaves it out until it is
    /// changed.
    #[test]
    fn the_cloud_detail_menu_is_medium_and_left_out_of_the_profile_until_chosen() {
        use super::super::cloud_detail::{DEFAULT, HIGH, MEDIUM};
        use crate::persist::preferences::UserPreferences;
        let empty = UserPreferences::parse("").expect("an empty profile");
        assert_eq!(DEFAULT, MEDIUM);
        let medium = PrefValue::Int(MEDIUM);
        assert!(left_out_of_save(fidelity::CLOUD_DETAIL, &medium, &empty));
        assert!(!left_out_of_save(
            fidelity::CLOUD_DETAIL,
            &PrefValue::Int(HIGH),
            &empty
        ));
        let named = UserPreferences::parse("[Fidelity]\r\nCloudDetail=Medium\r\n").expect("reads");
        assert!(!left_out_of_save(fidelity::CLOUD_DETAIL, &medium, &named));
    }

    #[test]
    fn every_box_draws_with_nothing_else_ticked() {
        use fidelity::{
            AMBIENT_OCCLUSION, CLOUD_DETAIL, DEPTH_MAPPING, GLOBAL_ILLUMINATION, GOD_RAYS, NO_WIND,
            SHADOWS, WEATHER,
        };
        let with = |on: &'static [&'static str]| move |n: &str| on.contains(&n);
        for name in [
            SHADOWS,
            GLOBAL_ILLUMINATION,
            AMBIENT_OCCLUSION,
            WEATHER,
            NO_WIND,
            DEPTH_MAPPING,
            GOD_RAYS,
            CLOUD_DETAIL,
        ] {
            assert_eq!(blocked(name, with(&[]), Some(false)), None, "{name}");
        }
        // The retired boxes are no boxes: an older profile's are read as nothing.
        for name in [
            "Fidelity.Lighting",
            "Fidelity.Debug",
            "Fidelity.Lamps",
            "Fidelity.Sky",
            "Fidelity.Water",
            "Fidelity.NoDamp",
        ] {
            assert!(
                find(name).is_none() && fidelity::RETIRED.contains(&name),
                "{name}"
            );
        }
    }

    #[test]
    fn the_boxes_that_turn_something_off_are_inverted() {
        let inverted: Vec<&str> = OPTIONS
            .iter()
            .filter(|o| o.inverted)
            .map(|o| o.caption)
            .collect();
        assert_eq!(
            inverted,
            [
                "Anti-Aliasing",
                "Foliage Wind",
                "Flame Flicker",
                "Silk Cobwebs",
                "High-Res Textures"
            ]
        );
    }

    #[test]
    fn a_refused_box_is_told_what_is_still_needed() {
        let started_without = Availability {
            built: true,
            widened: false,
            supported: true,
            rays: Some(true),
            active: false,
            failed: None,
        };
        assert_eq!(
            started_without.status(true).as_deref(),
            Some("Restart to apply")
        );
        assert_eq!(started_without.status(false), None);
        let short = Availability {
            widened: true,
            supported: false,
            ..started_without.clone()
        };
        assert_eq!(
            short.status(true).as_deref(),
            Some("This graphics backend can't run these effects")
        );
    }

    #[test]
    fn the_status_line_says_what_stops_the_effects_and_nothing_with_no_box_ticked() {
        let ready = Availability {
            built: true,
            widened: true,
            supported: true,
            rays: Some(true),
            active: false,
            failed: None,
        };
        assert!(ready.offered());
        for ticked in [false, true] {
            assert_eq!(ready.status(ticked), None, "ticked={ticked}");
        }
        let active = Availability {
            active: true,
            ..ready.clone()
        };
        assert_eq!(active.status(true).as_deref(), Some("Active"));
        assert_eq!(active.status(false), None);
        let failed = Availability {
            failed: Some("the lighting pass's shader did not build".to_owned()),
            ..ready
        };
        assert_eq!(
            failed.status(true).as_deref(),
            Some(
                "Stopped after an error (the lighting pass's shader did not build): untick and \
                 tick a box to retry"
            )
        );
        let long = Availability {
            failed: Some("x".repeat(200)),
            ..failed.clone()
        };
        assert!(
            long.status(true).unwrap().len() < 120,
            "a long reason is cut short"
        );
        assert_eq!(
            failed.status(false),
            None,
            "nothing ticked, nothing stopped"
        );
        assert_eq!(Availability::default().status(true), None, "not built");
        assert!(!Availability::default().offered());
    }
}
