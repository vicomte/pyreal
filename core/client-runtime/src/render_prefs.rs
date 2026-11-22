//! The render preferences the scene reads, and the high-fidelity presentation's `[Fidelity]`
//! section. No preference-file writes.

use crate::config::Preferences;

/// `Render.LandscapeDrawDistance` controls the landscape's mid radius.
/// Updating render preferences hands it to the smart box's mid-radius setter,
/// which resets the cell manager and sets the landscape radius. Render startup registers
/// it as an unsigned 32-bit preference with the choice list
/// `[VeryLow, Low, Medium, High, VeryHigh, Extreme, Distant, Far, Horizon]` and the value array
/// `[3, 5, 8, 11, 15, 25, 48, 80, 127]`
/// in the client; the registered default is **15** (`VeryHigh`, the Medium quality preset's).
pub const LANDSCAPE_DRAW_DISTANCE: &str =
    pyreal_client_contract::options::names::LANDSCAPE_DRAW_DISTANCE;

/// `Render.LandscapeDrawDistance`'s registered default, i.e. on a fresh install: the Medium
/// quality preset's, applied before the first landscape is generated.
#[allow(clippy::cast_sign_loss)] // a small positive constant
pub const LANDSCAPE_DRAW_DISTANCE_DEFAULT: u32 =
    pyreal_client_contract::options::fidelity::TERRAIN_DEFAULT as u32;

/// Decode this preference's registered choice list with
/// `pyreal_client_contract::options::store::EnumChoices::resolve` — the same function the
/// options page loads the profile through — rather than a second copy of the label table here:
/// a label match wins, an unmatched string is `strtol`'d as an **index** into the value array
/// (`LandscapeDrawDistance=3` is `High` = 11), and a negative or out-of-range index selects 0.
///
/// `None` when the profile does not name it, so the caller keeps its own default.
#[must_use]
pub fn landscape_draw_distance(prefs: &Preferences) -> Option<u32> {
    let raw = prefs.get(LANDSCAPE_DRAW_DISTANCE)?;
    let choices = pyreal_client_contract::options::store::enum_choices(LANDSCAPE_DRAW_DISTANCE)?;
    u32::try_from(choices.resolve(raw)).ok()
}

/// The 48-byte render-preference record plus its three adjacent static preferences,
/// initialized to each name's registered default.
///
/// Graphics-device preparation applies almost all of these preferences **every frame**,
/// both in the main client tick and while keeping the UI alive. It is a poll, not a
/// callback: each field is compared against a shadow copy and only a *change* does any work.
/// The shadows are one static word per preference: the five `Display.*` ones, the two
/// texture-detail levels, and one each for LandscapeDrawDistance, FieldOfView, AspectRatio,
/// ScreenBrightness and the environment-detail textures setting.
/// The render-preference-change callback, supplied for every registration, does **only**
/// recompute the cached overall graphics quality -- it applies nothing.
/// That is why a preference changed on the options page reaches the picture on the next frame
/// rather than on the Apply click, and why this build applies them the same way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderPreferences {
    /// **No consumer in the retail client beyond the quality-level round trip**: the value is
    /// written and read back only by preference handling.
    pub scenery_draw_distance: u32,
    /// The smart box passes the mid radius to the landscape.
    pub landscape_draw_distance: u32,
    /// Display-aspect setup maps 1 to `0x3FAAAAAB` (4/3),
    /// 2 -> `0x3FE38E39` (16/9), anything else -> display width / display height.
    pub aspect_ratio: u32,
    /// Brightness sets the 256-entry gamma ramp.
    pub screen_brightness: f32,
    /// Default-FOV setup computes the game FOV as `pref * 0.017453292`.
    pub field_of_view: f32,
    /// The offset subtracted from every degrade distance.
    pub degrade_distance: f32,
    /// `[Fidelity]`: the high-fidelity presentation. Not a retail preference; the options page,
    /// the file and `--set-at` set it. The scene applies a change live.
    pub fidelity: FidelityPreferences,
}

impl Default for RenderPreferences {
    fn default() -> Self {
        Self {
            scenery_draw_distance: 1,
            landscape_draw_distance: LANDSCAPE_DRAW_DISTANCE_DEFAULT,
            aspect_ratio: ASPECT_RATIO_DEFAULT,
            screen_brightness: 0.0,
            field_of_view: pyreal_client_contract::camera::DEFAULT_FOV_DEGREES,
            degrade_distance: pyreal_animation::parts::S_R_DEGRADE_DISTANCE,
            fidelity: FidelityPreferences::default(),
        }
    }
}

/// `Render.AspectRatio`'s registered default, `Normal` -- 4:3.
pub const ASPECT_RATIO_DEFAULT: u32 = 1;

/// `Render.SceneryDrawDistance`.
pub const SCENERY_DRAW_DISTANCE: &str =
    pyreal_client_contract::options::names::SCENERY_DRAW_DISTANCE;
/// `Render.AspectRatio`. One of the nine with no options-page row.
pub const ASPECT_RATIO: &str = pyreal_client_contract::options::names::ASPECT_RATIO;
/// `Render.FieldOfView`.
pub const FIELD_OF_VIEW: &str = pyreal_client_contract::options::names::FIELD_OF_VIEW;
/// `Render.ScreenBrightness`.
pub const SCREEN_BRIGHTNESS: &str = pyreal_client_contract::options::names::SCREEN_BRIGHTNESS;
/// `Render.DegradeDistance`.
pub const DEGRADE_DISTANCE: &str = pyreal_client_contract::options::names::DEGRADE_DISTANCE;

/// One unsigned 32-bit preference with a registered choice list: a case-insensitive label match
/// wins, an unmatched string is `strtol`'d as an **index** into the value array, and a negative or
/// out-of-range index selects choice 0.
fn choice(prefs: &Preferences, name: &str) -> Option<u32> {
    let raw = prefs.get(name)?;
    let choices = pyreal_client_contract::options::store::enum_choices(name)?;
    u32::try_from(choices.resolve(raw)).ok()
}

/// `Render.AspectRatio`'s three labels, as registered during render startup
/// (`Auto`, `Normal`, `Wide`, with no value array, so the index is the value). It is one of the
/// nine names UI preference initialization never attaches, so
/// `pyreal_client_contract::options::store::ENUM_CHOICES` has no row for it and [`choice`] cannot serve
/// it; the registry's own list still decodes the file.
fn aspect_choice(raw: Option<&str>) -> Option<u32> {
    let raw = raw?.trim();
    const LABELS: [&str; 3] = ["Auto", "Normal", "Wide"];
    if let Some(i) = LABELS.iter().position(|l| l.eq_ignore_ascii_case(raw)) {
        return u32::try_from(i).ok();
    }
    // The string conversion's numeric arm: an index into the choice list, 0 when out of range.
    Some(raw.parse::<u32>().ok().filter(|n| *n < 3).unwrap_or(0))
}

impl RenderPreferences {
    /// User preferences load before render startup registers:
    /// every name the file does not carry keeps the registered default.
    #[must_use]
    pub fn from_preferences(prefs: &Preferences) -> Self {
        let mut r = Self::default();
        if let Some(v) = choice(prefs, SCENERY_DRAW_DISTANCE) {
            r.scenery_draw_distance = v;
        }
        if let Some(v) = landscape_draw_distance(prefs) {
            r.landscape_draw_distance = v;
        }
        if let Some(v) = aspect_choice(prefs.get(ASPECT_RATIO)) {
            r.aspect_ratio = v;
        }
        if let Some(v) = prefs.f32(FIELD_OF_VIEW) {
            r.field_of_view = v;
        }
        if let Some(v) = prefs.f32(SCREEN_BRIGHTNESS) {
            r.screen_brightness = v;
        }
        if let Some(v) = prefs.f32(DEGRADE_DISTANCE) {
            r.degrade_distance = v;
        }
        // The high-fidelity presentation's section.
        r.fidelity = FidelityPreferences::from_preferences(prefs);
        r
    }

    /// Default-FOV setup stores the game FOV in radians.
    #[must_use]
    pub fn game_fov_rad(&self) -> f32 {
        self.field_of_view * pyreal_client_contract::camera::DEG_TO_RAD
    }

    /// The three display-aspect setup arms.
    #[must_use]
    pub fn aspect(&self) -> pyreal_client_contract::camera::AspectPreference {
        match self.aspect_ratio {
            1 => pyreal_client_contract::camera::AspectPreference::Normal,
            2 => pyreal_client_contract::camera::AspectPreference::Wide,
            _ => pyreal_client_contract::camera::AspectPreference::Auto,
        }
    }

    /// The `UiRequest::SetPreference` names this struct owns, applied to the struct itself.
    /// Returns whether the value landed, so the caller can hand an unowned request on.
    ///
    /// **What "landed" means, per name.** `Render.FieldOfView`, `Render.AspectRatio` and
    /// `Render.DegradeDistance` are
    /// read out of this struct on the next frame and change the picture then, which is exactly
    /// what the render-preference poll does with them.
    ///
    /// The rest change the world live too, through the scene's per-frame poll of this struct: the
    /// landscape draw distance opens a new landblock window. `Render.SceneryDrawDistance` is
    /// recorded and read by nothing; see the struct's field docs for what reads each in retail.
    pub fn set_named(&mut self, name: &str, value: &pyreal_client_contract::PrefValue) -> bool {
        use pyreal_client_contract::PrefValue;
        let int = |v: &PrefValue| match v {
            PrefValue::Int(i) => Some(u32::from_ne_bytes(i.to_ne_bytes())),
            _ => None,
        };
        let float = |v: &PrefValue| match v {
            PrefValue::Float(f) => Some(*f),
            _ => None,
        };
        macro_rules! take {
            ($field:ident, $get:ident) => {{
                let Some(v) = $get(value) else { return false };
                self.$field = v;
                return true;
            }};
        }
        if name.eq_ignore_ascii_case(FIELD_OF_VIEW) {
            take!(field_of_view, float);
        }
        if name.eq_ignore_ascii_case(ASPECT_RATIO) {
            take!(aspect_ratio, int);
        }
        if name.eq_ignore_ascii_case(SCREEN_BRIGHTNESS) {
            take!(screen_brightness, float);
        }
        if name.eq_ignore_ascii_case(DEGRADE_DISTANCE) {
            take!(degrade_distance, float);
        }
        if name.eq_ignore_ascii_case(SCENERY_DRAW_DISTANCE) {
            take!(scenery_draw_distance, int);
        }
        // **Without this arm the draw distance needs a restart.** The options page's write
        // would fall through every owner and end on `App`'s *"UI request with no owner yet"*
        // line; nothing reads it after start-up, so the preference could only take effect
        // through `Config::apply_preferences` on the next launch. The live path is
        // the presentation's preference poll, which reads the field this writes.
        if name.eq_ignore_ascii_case(LANDSCAPE_DRAW_DISTANCE) {
            take!(landscape_draw_distance, int);
        }
        // The high-fidelity presentation, which the scene's poll applies live.
        self.fidelity.set_named(name, value)
    }
}

/// The `[Fidelity]` names: the high-fidelity presentation. See
/// [`pyreal_client_contract::options::names::fidelity`].
pub use pyreal_client_contract::options::names::fidelity;

/// One option of the high-fidelity presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FidelityFeature {
    /// `Fidelity.Shadows`.
    Shadows,
    /// `Fidelity.GlobalIllumination`.
    GlobalIllumination,
    /// `Fidelity.AmbientOcclusion`.
    AmbientOcclusion,
    /// `Fidelity.Weather`.
    Weather,
    /// `Fidelity.NoAntiAliasing`.
    NoAntiAliasing,
    /// `Fidelity.NoWind`.
    NoWind,
    /// `Fidelity.VSync`.
    VSync,
    /// `Fidelity.Hdr`.
    Hdr,
    /// `Fidelity.DepthMapping`.
    DepthMapping,
    /// `Fidelity.GodRays`.
    GodRays,
    /// `Fidelity.CloudDetail`.
    CloudDetail,
    /// `Fidelity.GroundCoverDrawDistance`.
    GroundCoverDrawDistance,
    /// `Fidelity.NoFlameFlicker`.
    NoFlameFlicker,
    /// `Fidelity.NoSilkCobwebs`.
    NoSilkCobwebs,
    /// `Fidelity.NoHighResTextures`.
    NoHighResTextures,
    /// `Fidelity.LightDrawDistance`.
    LightDrawDistance,
    /// `Fidelity.ShadowDistance`.
    ShadowDistance,
    /// `Fidelity.FogDensity`.
    FogDensity,
    /// `Fidelity.ObjectDetail`.
    ObjectDetail,
    /// `Fidelity.GrassHeight`.
    GrassHeight,
    /// `Fidelity.WaterDetail`.
    WaterDetail,
    /// `Fidelity.LandscapeSmoothing`.
    LandscapeSmoothing,
    /// `Fidelity.DynamicResolution`.
    DynamicResolution,
    /// `Fidelity.RenderScale`.
    RenderScale,
}

impl FidelityFeature {
    /// Every option, in the order the names list them.
    pub const ALL: [Self; 24] = [
        Self::Shadows,
        Self::GlobalIllumination,
        Self::AmbientOcclusion,
        Self::Weather,
        Self::NoAntiAliasing,
        Self::NoWind,
        Self::VSync,
        Self::Hdr,
        Self::DepthMapping,
        Self::GodRays,
        Self::CloudDetail,
        Self::GroundCoverDrawDistance,
        Self::NoFlameFlicker,
        Self::NoSilkCobwebs,
        Self::NoHighResTextures,
        Self::LightDrawDistance,
        Self::ShadowDistance,
        Self::FogDensity,
        Self::ObjectDetail,
        Self::GrassHeight,
        Self::WaterDetail,
        Self::LandscapeSmoothing,
        Self::DynamicResolution,
        Self::RenderScale,
    ];

    /// The option's preference name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Shadows => fidelity::SHADOWS,
            Self::GlobalIllumination => fidelity::GLOBAL_ILLUMINATION,
            Self::AmbientOcclusion => fidelity::AMBIENT_OCCLUSION,
            Self::Weather => fidelity::WEATHER,
            Self::NoAntiAliasing => fidelity::NO_ANTI_ALIASING,
            Self::NoWind => fidelity::NO_WIND,
            Self::VSync => fidelity::VSYNC,
            Self::Hdr => fidelity::HDR,
            Self::DepthMapping => fidelity::DEPTH_MAPPING,
            Self::GodRays => fidelity::GOD_RAYS,
            Self::CloudDetail => fidelity::CLOUD_DETAIL,
            Self::GroundCoverDrawDistance => fidelity::GROUND_COVER_DRAW_DISTANCE,
            Self::NoFlameFlicker => fidelity::NO_FLAME_FLICKER,
            Self::NoSilkCobwebs => fidelity::NO_SILK_COBWEBS,
            Self::NoHighResTextures => fidelity::NO_HIGH_RES_TEXTURES,
            Self::LightDrawDistance => fidelity::LIGHT_DRAW_DISTANCE,
            Self::ShadowDistance => fidelity::SHADOW_DISTANCE,
            Self::FogDensity => fidelity::FOG_DENSITY,
            Self::ObjectDetail => fidelity::OBJECT_DETAIL,
            Self::GrassHeight => fidelity::GRASS_HEIGHT,
            Self::WaterDetail => fidelity::WATER_DETAIL,
            Self::LandscapeSmoothing => fidelity::LANDSCAPE_SMOOTHING,
            Self::DynamicResolution => fidelity::DYNAMIC_RESOLUTION,
            Self::RenderScale => fidelity::RENDER_SCALE,
        }
    }

    /// The highest value the option takes: 4 for an effect (Ultra), 1 for the switches that
    /// turn something off (the anti-aliasing's, the slope materials', the
    /// damp's, the wind's, the flicker's) and the presentation choices, 3 for depth mapping's,
    /// God Rays' and Cloud Detail's levels, 2 for Water Detail's, the greatest share for the draw
    /// distances, the highest frame rate for Dynamic Resolution, and under a hundred for the render
    /// scale.
    #[must_use]
    pub const fn max(self) -> u32 {
        match self {
            Self::DepthMapping | Self::GodRays | Self::CloudDetail | Self::LandscapeSmoothing => 3,
            Self::GroundCoverDrawDistance => pyreal_client_contract::options::draw_distance::MOST,
            Self::RenderScale => 99,
            Self::DynamicResolution => 120,
            Self::NoAntiAliasing
            | Self::NoWind
            | Self::VSync
            | Self::Hdr
            | Self::NoFlameFlicker
            | Self::NoSilkCobwebs
            | Self::NoHighResTextures => 1,
            Self::LightDrawDistance => pyreal_client_contract::options::light_distance::MOST,
            Self::ShadowDistance => pyreal_client_contract::options::shadow_distance::MOST,
            Self::FogDensity => pyreal_client_contract::options::fog_density::MOST,
            Self::ObjectDetail => pyreal_client_contract::options::object_detail::MOST,
            Self::GrassHeight => pyreal_client_contract::options::grass_height::MOST,
            Self::WaterDetail => 2,
            _ => 4,
        }
    }

    /// What the option holds where the profile names nothing, when it is not 0: a box's default,
    /// or the Depth Mapping, God Rays, Cloud Detail, Tree Draw Distance, Light Draw Distance, Shadow Distance, Render Scale and
    /// Dynamic Resolution menus'.
    #[must_use]
    pub fn default_value(self) -> Option<u32> {
        use pyreal_client_contract::options as o;
        match self {
            Self::DepthMapping => u32::try_from(o::depth_mapping::DEFAULT).ok(),
            Self::GodRays => u32::try_from(o::god_rays::DEFAULT).ok(),
            Self::CloudDetail => u32::try_from(o::cloud_detail::DEFAULT).ok(),
            Self::LandscapeSmoothing => u32::try_from(o::landscape_smoothing::DEFAULT).ok(),
            Self::LightDrawDistance => u32::try_from(o::light_distance::DEFAULT).ok(),
            Self::ShadowDistance => u32::try_from(o::shadow_distance::DEFAULT).ok(),
            Self::FogDensity => u32::try_from(o::fog_density::DEFAULT).ok(),
            Self::ObjectDetail => u32::try_from(o::object_detail::DEFAULT).ok(),
            Self::GrassHeight => u32::try_from(o::grass_height::DEFAULT).ok(),
            Self::WaterDetail => u32::try_from(o::water_detail::DEFAULT).ok(),
            Self::RenderScale => Some(o::fidelity::SCALE_DEFAULT),
            Self::DynamicResolution => u32::try_from(o::dynamic_resolution::DEFAULT).ok(),
            _ => o::fidelity::find(self.name())
                .filter(|b| b.on_by_default)
                .map(|_| 1),
        }
    }

    /// `raw` as this option stores it: an effect past Ultra is Ultra; a draw distance outside its menu is the whole distance, 0, but for the ground
    /// cover's Off; a shadow distance outside its menu's range is 0; a render scale that is
    /// neither Dynamic nor a share under the whole frame is all of its pixels, 0.
    #[must_use]
    pub const fn clamp(self, raw: u32) -> u32 {
        if matches!(self, Self::RenderScale) {
            use pyreal_client_contract::options::render_scale::{DYNAMIC, LEAST};
            if raw == DYNAMIC || (raw >= LEAST && raw <= self.max()) {
                raw
            } else {
                0
            }
        } else if matches!(self, Self::GrassHeight) {
            use pyreal_client_contract::options::grass_height::LEAST;
            if raw >= LEAST && raw <= self.max() {
                raw
            } else {
                0
            }
        } else if matches!(self, Self::ShadowDistance) {
            use pyreal_client_contract::options::shadow_distance::LEAST;
            if raw >= LEAST && raw <= self.max() {
                raw
            } else {
                0
            }
        } else if matches!(
            self,
            Self::GroundCoverDrawDistance | Self::LightDrawDistance | Self::ObjectDetail
        ) {
            use pyreal_client_contract::options::draw_distance::{LEAST, OFF};
            #[allow(clippy::cast_sign_loss)] // a small positive constant
            let off = matches!(self, Self::GroundCoverDrawDistance) && raw == OFF as u32;
            if raw >= LEAST && raw <= self.max() || off {
                raw
            } else {
                0
            }
        } else if raw <= self.max() {
            raw
        } else {
            self.max()
        }
    }
}

/// The `[Fidelity]` section: what the player asked of the high-fidelity presentation.
/// Plain data, all off
/// until read from a profile, which starts each box at its default; the scene maps it onto the
/// presentation's own settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FidelityPreferences {
    /// Each option's stored value, in [`FidelityFeature::ALL`] order. 0 is off for every one; a
    /// ticked box is 1.
    pub values: [u32; FidelityFeature::ALL.len()],
}

impl FidelityPreferences {
    /// The section from the profile: what the file does not name stands at its default.
    #[must_use]
    pub fn from_preferences(prefs: &Preferences) -> Self {
        let mut f = Self::default();
        for (i, feature) in FidelityFeature::ALL.into_iter().enumerate() {
            let raw = match feature {
                // The share the page chose, or Dynamic, in its words.
                FidelityFeature::RenderScale => prefs
                    .get(feature.name())
                    .and_then(|v| {
                        pyreal_client_contract::options::render_scale::parse_value(
                            feature.name(),
                            v,
                        )
                    })
                    .and_then(|v| u32::try_from(v).ok()),
                // A level by its name or number, or the box it once was.
                FidelityFeature::DepthMapping => prefs
                    .get(feature.name())
                    .and_then(|v| {
                        pyreal_client_contract::options::depth_mapping::parse_value(
                            feature.name(),
                            v,
                        )
                    })
                    .and_then(|v| u32::try_from(v).ok()),
                // A frame rate in its words.
                FidelityFeature::DynamicResolution => prefs
                    .get(feature.name())
                    .and_then(|v| {
                        pyreal_client_contract::options::dynamic_resolution::parse_value(
                            feature.name(),
                            v,
                        )
                    })
                    .and_then(|v| u32::try_from(v).ok()),
                // A level by its name or number.
                FidelityFeature::GodRays => prefs
                    .get(feature.name())
                    .and_then(|v| {
                        pyreal_client_contract::options::god_rays::parse_value(feature.name(), v)
                    })
                    .and_then(|v| u32::try_from(v).ok()),
                // A share by its words, or the ground cover's Off.
                FidelityFeature::GroundCoverDrawDistance => prefs
                    .get(feature.name())
                    .and_then(|v| {
                        pyreal_client_contract::options::draw_distance::parse_value(
                            feature.name(),
                            v,
                        )
                    })
                    .and_then(|v| u32::try_from(v).ok()),
                // A level by its name or stored number.
                FidelityFeature::CloudDetail => prefs
                    .get(feature.name())
                    .and_then(|v| {
                        pyreal_client_contract::options::cloud_detail::parse_value(
                            feature.name(),
                            v,
                        )
                    })
                    .and_then(|v| u32::try_from(v).ok()),
                // A level by its name or stored number.
                FidelityFeature::WaterDetail => prefs
                    .get(feature.name())
                    .and_then(|v| {
                        pyreal_client_contract::options::water_detail::parse_value(
                            feature.name(),
                            v,
                        )
                    })
                    .and_then(|v| u32::try_from(v).ok()),
                // A level by its name or stored number, or the box it once was.
                FidelityFeature::LandscapeSmoothing => prefs
                    .get(feature.name())
                    .and_then(|v| {
                        pyreal_client_contract::options::landscape_smoothing::parse_value(
                            feature.name(),
                            v,
                        )
                    })
                    .and_then(|v| u32::try_from(v).ok()),
                // A share by its words, or Off.
                FidelityFeature::FogDensity => prefs
                    .get(feature.name())
                    .and_then(|v| {
                        pyreal_client_contract::options::fog_density::parse_value(feature.name(), v)
                    })
                    .and_then(|v| u32::try_from(v).ok()),
                // A share by its words.
                FidelityFeature::ObjectDetail => prefs
                    .get(feature.name())
                    .and_then(|v| {
                        pyreal_client_contract::options::object_detail::parse_value(
                            feature.name(),
                            v,
                        )
                    })
                    .and_then(|v| u32::try_from(v).ok()),
                // A height by its name or share.
                FidelityFeature::GrassHeight => prefs
                    .get(feature.name())
                    .and_then(|v| {
                        pyreal_client_contract::options::grass_height::parse_value(
                            feature.name(),
                            v,
                        )
                    })
                    .and_then(|v| u32::try_from(v).ok()),
                // A share by its words.
                FidelityFeature::LightDrawDistance => prefs
                    .get(feature.name())
                    .and_then(|v| {
                        pyreal_client_contract::options::light_distance::parse_value(
                            feature.name(),
                            v,
                        )
                    })
                    .and_then(|v| u32::try_from(v).ok()),
                // A distance by its name or metres.
                FidelityFeature::ShadowDistance => prefs
                    .get(feature.name())
                    .and_then(|v| {
                        pyreal_client_contract::options::shadow_distance::parse_value(
                            feature.name(),
                            v,
                        )
                    })
                    .and_then(|v| u32::try_from(v).ok()),
                // A box the page ticked; a number the file holds otherwise (from an older
                // build) is not read, as the page does not read it.
                _ => prefs.bool(feature.name()).map(u32::from),
            };
            if let Some(raw) = raw.or_else(|| feature.default_value()) {
                f.values[i] = feature.clamp(raw);
            }
        }
        f
    }

    /// The stored value of `feature`.
    #[must_use]
    pub fn value(&self, feature: FidelityFeature) -> u32 {
        FidelityFeature::ALL
            .iter()
            .position(|f| *f == feature)
            .map_or(0, |i| self.values[i])
    }

    /// The value `feature` takes in effect: its stored value.
    #[must_use]
    pub fn effective(&self, feature: FidelityFeature) -> u32 {
        self.value(feature)
    }

    /// The settings a capture or a test names: a comma list of `Name=value` (`+` also separates),
    /// each name with or without its `Fidelity.` prefix and each value a number or `off`, `on`,
    /// `low`, `medium`, `high` or `ultra`. `off` alone is every option off.
    ///
    /// # Errors
    /// A name no option has, or a value that is not one of those, as a sentence.
    pub fn parse_switch(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        let mut f = Self::default();
        if raw.eq_ignore_ascii_case("off") {
            return Ok(f);
        }
        for item in raw
            .split([',', '+'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let (name, value) = item
                .split_once('=')
                .ok_or_else(|| format!("a fidelity setting is Name=value, not {item:?}"))?;
            let full = if name.trim().to_ascii_lowercase().starts_with("fidelity.") {
                name.trim().to_owned()
            } else {
                format!("Fidelity.{}", name.trim())
            };
            // Cloud Detail and Water Detail store their levels in orders of their own; Fog
            // Density's Off is 1.
            let level = pyreal_client_contract::options::cloud_detail::parse_value(&full, value)
                .or_else(|| {
                    pyreal_client_contract::options::water_detail::parse_value(&full, value)
                })
                .or_else(|| pyreal_client_contract::options::fog_density::parse_value(&full, value))
                .and_then(|v| u32::try_from(v).ok());
            let value = match (level, value.trim().to_ascii_lowercase().as_str()) {
                (Some(level), _) => level,
                (None, "off") => 0,
                (None, "on" | "low") => 1,
                (None, "medium") => 2,
                (None, "high") => 3,
                (None, "ultra") => 4,
                (None, v) => v
                    .parse::<u32>()
                    .map_err(|_| format!("{value:?} is not a value for {name}"))?,
            };
            // A name no option takes any more is read as nothing.
            if fidelity::RETIRED
                .iter()
                .any(|r| r.eq_ignore_ascii_case(&full))
            {
                continue;
            }
            if !f.set_named(
                &full,
                &pyreal_client_contract::PrefValue::Int(i32::try_from(value).unwrap_or(i32::MAX)),
            ) {
                return Err(format!("there is no fidelity option {name:?}"));
            }
        }
        Ok(f)
    }

    /// Apply one `[Fidelity]` preference. Returns whether `name` is one
    /// of them and `value` landed: an integer or a boolean for every one.
    pub fn set_named(&mut self, name: &str, value: &pyreal_client_contract::PrefValue) -> bool {
        use pyreal_client_contract::PrefValue;
        let raw = match value {
            PrefValue::Bool(b) => u32::from(*b),
            PrefValue::Int(i) => u32::try_from(*i).unwrap_or(0),
            _ => return false,
        };
        for (i, feature) in FidelityFeature::ALL.into_iter().enumerate() {
            if name.eq_ignore_ascii_case(feature.name()) {
                self.values[i] = feature.clamp(raw);
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_naming_nothing_has_the_effects_on() {
        assert!(FidelityFeature::ALL
            .into_iter()
            .all(|f| FidelityPreferences::default().value(f) == 0));
        let d = FidelityPreferences::from_preferences(&Preferences::parse(""));
        for f in FidelityFeature::ALL {
            let on = matches!(
                f,
                FidelityFeature::Shadows
                    | FidelityFeature::GlobalIllumination
                    | FidelityFeature::AmbientOcclusion
                    | FidelityFeature::Weather
                    | FidelityFeature::VSync
                    | FidelityFeature::LandscapeSmoothing
            );
            let at = if f == FidelityFeature::DepthMapping {
                2
            } else if f == FidelityFeature::GodRays
                || f == FidelityFeature::CloudDetail
                || f == FidelityFeature::WaterDetail
            {
                1
            } else if f == FidelityFeature::LightDrawDistance {
                150
            } else if f == FidelityFeature::ShadowDistance {
                200
            } else if f == FidelityFeature::FogDensity {
                40
            } else if f == FidelityFeature::ObjectDetail {
                150
            } else if f == FidelityFeature::GrassHeight {
                100
            } else if f == FidelityFeature::RenderScale {
                pyreal_client_contract::options::render_scale::DYNAMIC
            } else if f == FidelityFeature::DynamicResolution {
                60
            } else {
                u32::from(on)
            };
            assert_eq!(d.value(f), at, "{f:?}");
        }
        // A box the profile turned off stays off.
        let off = FidelityPreferences::from_preferences(&Preferences::parse(
            "[Fidelity]\nShadows=False\nLighting=False\nRetailLook=True\n",
        ));
        assert_eq!(off.value(FidelityFeature::Shadows), 0);
        // The ticked boxes are in effect, whatever interface an older profile names.
        let horizon = FidelityPreferences::from_preferences(&Preferences::parse(
            "[UI]\nInterface=Modern\n[Fidelity]\nAmbientOcclusion=True\n",
        ));
        assert_eq!(horizon.effective(FidelityFeature::AmbientOcclusion), 1);
        // An older build's levels, and the names of options this build does not have, are not
        // read, which leaves each at its default; neither is an error.
        let old = FidelityPreferences::from_preferences(&Preferences::parse(
            "[UI]\nInterface=2\n[Fidelity]\nEnabled=1\nPreset=2\nWater=3\nLighting=3\n",
        ));
        assert_eq!(old, d);
        // Every option has its name, and every name its option.
        assert_eq!(fidelity::NAMES.len(), FidelityFeature::ALL.len());
        for f in FidelityFeature::ALL {
            assert!(fidelity::NAMES.contains(&f.name()), "{f:?}");
        }
    }

    #[test]
    fn the_god_rays_level_is_read_by_name_or_number_and_draws_nothing_alone() {
        let read = |v: &str| {
            FidelityPreferences::from_preferences(&Preferences::parse(&format!(
                "[UI]\nInterface=Horizon\n[Fidelity]\nGodRays={v}\n"
            )))
        };
        assert_eq!(read("Medium").effective(FidelityFeature::GodRays), 2);
        assert_eq!(read("3").effective(FidelityFeature::GodRays), 3);
        assert_eq!(
            read("9").effective(FidelityFeature::GodRays),
            1,
            "a word it does not know leaves it at its default"
        );
        let switched = |s: &str| FidelityPreferences::parse_switch(s).unwrap();
        assert_eq!(
            switched("Lighting=1,GodRays=2").effective(FidelityFeature::GodRays),
            2
        );
    }

    #[test]
    fn the_light_draw_distance_is_read_by_its_words_and_draws_nothing_alone() {
        let read = |line: &str| {
            FidelityPreferences::from_preferences(&Preferences::parse(&format!(
                "[UI]\nInterface=Horizon\n[Fidelity]\n{line}\n"
            )))
        };
        let lights = FidelityFeature::LightDrawDistance;
        assert_eq!(read("LightDrawDistance=3x").effective(lights), 300);
        assert_eq!(read("LightDrawDistance=150").effective(lights), 150);
        assert_eq!(
            read("LightDrawDistance=9x").effective(lights),
            lights.default_value().unwrap_or(0),
            "a word it does not know leaves it at its default"
        );
        let switched = FidelityPreferences::parse_switch("Lighting=1,LightDrawDistance=200")
            .expect("a switch");
        assert_eq!(switched.effective(lights), 200);
        assert_eq!(lights.clamp(20), 0);
        assert_eq!(FidelityFeature::NoFlameFlicker.clamp(3), 1);
    }

    #[test]
    fn the_cloud_detail_level_is_read_by_name_starts_medium_and_draws_nothing_alone() {
        use pyreal_client_contract::options::cloud_detail::{HIGH, LOW, MEDIUM, OFF};
        let read = |v: &str| {
            FidelityPreferences::from_preferences(&Preferences::parse(&format!(
                "[UI]\nInterface=Horizon\n[Fidelity]\nCloudDetail={v}\n"
            )))
        };
        let level =
            |p: FidelityPreferences| i32::try_from(p.effective(FidelityFeature::CloudDetail));
        assert_eq!(level(read("Medium")), Ok(MEDIUM));
        assert_eq!(level(read("off")), Ok(OFF));
        assert_eq!(
            level(read("9")),
            Ok(MEDIUM),
            "a word it does not know leaves it at its default"
        );
        assert_eq!(level(FidelityPreferences::default()), Ok(LOW));
        let switched = FidelityPreferences::parse_switch("Lighting=1,CloudDetail=high").unwrap();
        assert_eq!(level(switched), Ok(HIGH));
        let numbered = FidelityPreferences::parse_switch("Lighting=1,Shadows=high").unwrap();
        assert_eq!(
            numbered.effective(FidelityFeature::Shadows),
            3,
            "other words are as before"
        );
    }

    #[test]
    fn fidelity_values_read_and_set_by_name_and_out_of_range_values_clamp() {
        use pyreal_client_contract::PrefValue;
        let p = FidelityPreferences::from_preferences(&Preferences::parse(
            "[UI]\nInterface=Horizon\n[Fidelity]\nShadows=1\n",
        ));
        assert_eq!(p.effective(FidelityFeature::Shadows), 1);
        let mut q = FidelityPreferences::default();
        assert!(!q.set_named("fidelity.interface", &PrefValue::Bool(true)));
        assert!(q.set_named(fidelity::SHADOWS, &PrefValue::Int(7)));
        assert!(!q.set_named(fidelity::SHADOWS, &PrefValue::Float(2.0)));
        assert!(!q.set_named("Render.FieldOfView", &PrefValue::Int(2)));
        assert!(!q.set_named("Fidelity.Puddles", &PrefValue::Int(2)));
        assert_eq!(
            q.effective(FidelityFeature::Shadows),
            4,
            "a level saturates at Ultra"
        );
    }

    #[test]
    fn the_depth_mapping_level_is_read_by_name_or_number_and_a_ticked_box_is_high() {
        let read = |v: &str| {
            FidelityPreferences::from_preferences(&Preferences::parse(&format!(
                "[UI]\nInterface=Horizon\n[Fidelity]\nDepthMapping={v}\nNoWind=True\n"
            )))
        };
        assert_eq!(read("Medium").effective(FidelityFeature::DepthMapping), 2);
        assert_eq!(read("1").effective(FidelityFeature::DepthMapping), 1);
        assert_eq!(read("True").effective(FidelityFeature::DepthMapping), 3);
        assert_eq!(
            read("9").effective(FidelityFeature::DepthMapping),
            2,
            "a word it does not know leaves it at its default"
        );
        assert_eq!(read("Off").effective(FidelityFeature::NoWind), 1);
        let switched = |s: &str| FidelityPreferences::parse_switch(s).unwrap();
        assert_eq!(
            switched("Lighting=1,DepthMapping=1").effective(FidelityFeature::DepthMapping),
            1
        );
    }

    #[test]
    fn a_draw_distance_is_read_by_its_words_and_draws_nothing_alone() {
        let read = |line: &str| {
            FidelityPreferences::from_preferences(&Preferences::parse(&format!(
                "[UI]\nInterface=Horizon\n[Fidelity]\n{line}\n"
            )))
        };
        let cover = FidelityFeature::GroundCoverDrawDistance;
        assert_eq!(read("GroundCoverDrawDistance=3x").effective(cover), 300);
        assert_eq!(read("GroundCoverDrawDistance=150").effective(cover), 150);
        assert_eq!(
            read("GroundCoverDrawDistance=9x").effective(cover),
            0,
            "a word it does not know leaves it at the whole distance"
        );
        assert_eq!(read("GroundCoverDrawDistance=Off").effective(cover), 1);
        // The trees' old draw distance is read as nothing, from a profile or a capture's line.
        assert_eq!(read("TreeDrawDistance=5x"), read(""));
        assert_eq!(
            FidelityPreferences::parse_switch("TreeDrawDistance=500,NoWind=1"),
            FidelityPreferences::parse_switch("NoWind=1")
        );
        let switched = FidelityPreferences::parse_switch("Lighting=1,GroundCoverDrawDistance=1")
            .expect("a switch");
        assert_eq!(switched.effective(cover), 1);
        assert_eq!(FidelityFeature::GroundCoverDrawDistance.clamp(20), 0);
        assert_eq!(FidelityFeature::GroundCoverDrawDistance.clamp(1), 1);
        assert_eq!(FidelityFeature::NoWind.clamp(3), 1);
    }

    #[test]
    fn the_render_scale_holds_dynamic_or_a_share_under_the_whole_frame_and_draws_nothing_itself() {
        use pyreal_client_contract::PrefValue;
        let mut q = FidelityPreferences::default();
        assert!(q.set_named(fidelity::RENDER_SCALE, &PrefValue::Int(67)));
        assert_eq!(q.effective(FidelityFeature::RenderScale), 67);
        for (raw, stored) in [(100, 0), (49, 0), (2, 0), (1, 1), (50, 50), (99, 99)] {
            assert!(q.set_named(fidelity::RENDER_SCALE, &PrefValue::Int(raw)));
            assert_eq!(q.value(FidelityFeature::RenderScale), stored, "{raw}");
        }
        let q = FidelityPreferences::parse_switch("Lighting=1,RenderScale=75").unwrap();
        assert_eq!(q.effective(FidelityFeature::RenderScale), 75);
        let read = FidelityPreferences::from_preferences(&Preferences::parse(
            "[UI]\nInterface=Horizon\n[Fidelity]\nRenderScale=0.5x\nDynamicResolution=90 FPS\n",
        ));
        assert_eq!(read.effective(FidelityFeature::RenderScale), 50);
        assert_eq!(read.effective(FidelityFeature::DynamicResolution), 90);
    }

    #[test]
    fn the_render_preferences_carry_the_fidelity_section_from_the_file_and_by_name() {
        use pyreal_client_contract::PrefValue;
        assert_eq!(
            RenderPreferences::default().fidelity,
            FidelityPreferences::default()
        );
        let r = RenderPreferences::from_preferences(&Preferences::parse(
            "[UI]\nInterface=Horizon\n[Fidelity]\nShadows=False\n",
        ));
        assert_eq!(r.fidelity.effective(FidelityFeature::Shadows), 0);
        // The live path: a name the render preferences do not own themselves lands in the
        // section.
        let mut live = RenderPreferences::default();
        assert!(live.set_named(fidelity::AMBIENT_OCCLUSION, &PrefValue::Bool(true)));
        assert_eq!(
            live.fidelity.effective(FidelityFeature::AmbientOcclusion),
            1
        );
        assert!(!live.set_named("Fidelity.NoSuchOption", &PrefValue::Int(1)));
        // A capture's settings, as Horizon draws them.
        let off = FidelityPreferences::parse_switch("off").expect("off");
        assert_eq!(off, FidelityPreferences::default());
        let list = FidelityPreferences::parse_switch("Debug=1, Fidelity.Lighting=high+Shadows=on")
            .expect("list");
        assert_eq!(list.effective(FidelityFeature::Shadows), 1);
        assert_eq!(
            list,
            FidelityPreferences::parse_switch("Shadows=on").expect("one")
        );
        assert!(FidelityPreferences::parse_switch("Lighting=1,Interface=0").is_ok());
        assert!(FidelityPreferences::parse_switch("Lighting").is_err());
        assert!(FidelityPreferences::parse_switch("Puddles=1").is_err());
        assert!(FidelityPreferences::parse_switch("Shadows=lots").is_err());
    }
}
