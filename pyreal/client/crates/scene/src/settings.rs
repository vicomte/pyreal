//! The player's graphics options as the engine's switches.
//!
//! The quality presets (Low, Medium, High, Ultra) set the `[Fidelity]` boxes, so they reach the
//! engine through the boxes. What each box does now:
//!
//! | Option | The engine |
//! |---|---|
//! | Sun Shadows | the sun's cascades and contact shadows |
//! | Global Illumination | the bounce light's probes |
//! | Ambient Occlusion | GTAO |
//! | Anti-Aliasing (off) | the temporal resolve and the upscalers' history |
//! | Depth Mapping | a pack material's relief on a retail picture: Low its bend, Medium and High parallax and self-shadows, deeper and farther ([`pyreal_engine::renderer::RELIEF_LEVELS`]) |
//! | Foliage Wind (off) | particles of the ground cover stand still: cover drawn without sway |
//! | Ground Cover | the cover's reach, a share of the pack's (Off: none) |
//! | Render Scale | the share of the output's pixels the world is drawn at, or Dynamic |
//! | Dynamic Resolution | the frame rate a Dynamic render scale holds (its GPU budget) |
//! | VSync | how frames reach the window |
//! | Weather Effects | rain and snow on the rainy days, wet ground and puddles, lightning ([`pyreal_engine::weather`]) |
//! | God Rays | the air's froxels: volumetric fog and the sun's shafts, at the level asked ([`pyreal_engine::air`]) |
//! | Cloud Detail | the cloud deck's detail: Off the retail deck, Low to High more octaves of it |
//! | Flame Flicker (off) | the lamps' light held steady |
//! | Silk Cobwebs (off) | the cobwebs drawn as the dats' cards, not silk ([`pyreal_engine::webs`]) |
//! | Light Draw Distance | how far from the eye lamps light the world, a share of 250 m |
//! | Shadow Distance | how far along the view the sun's cascades reach, metres, fading over the last quarter |
//! | Fog Density | the haze over the land and the air's blue of distance: a share of the world fog's own (Off: none) |
//! | Object Detail | the degrade distances (coarser levels, the trees' cards, shadow casters) and the cover's levels, a share of the dats' and the pack's |
//! | HDR Output | the window's extended-range output where the display has it |
//! | High-Res Textures (off) | the original level of every picture; on, a two-level picture's `client_highres.dat` level where the file is installed, from the next dat set ([`pyreal_engine::asset::texture::set_high_res_textures`]) |
//! | Landscape Smoothing | the ground drawn at the drawn offset, the scenery, bodies and camera on it ([`pyreal_engine::drawn`]); the land is loaded again |
//! | Ground Coloring | end of retail's ground pictures tinted toward the 1999 ground's colours (Classic) or as they are, the cover's colour after them ([`pyreal_engine::ground_tint`]); the land is loaded again |
//!
//! `Render.LandscapeDrawDistance` is how far the far terrain reaches, to
//! [`MOST_FAR`](crate::settings::MOST_FAR) blocks (the far trees and the haze follow it:
//! [`pyreal_engine::world::far_tree_rings`],
//! [`pyreal_engine::atmosphere::fog_end_for`]). The lamps, the physical sky, the water, the slope
//! materials and the damp are always drawn, as the engine starts. The Standard look's texture
//! detail, filtering and multi-pass alpha are gone.

use pyreal_client_runtime::render_prefs::{
    FidelityFeature as F, FidelityPreferences, RenderPreferences,
};
use pyreal_engine::Engine;

use crate::bridge::Reach;

/// The farthest the far terrain reaches, blocks: the Horizon draw distance's, about 24 km, the
/// most a 2048-texel field holds.
pub const MOST_FAR: i32 = 127;

/// The engine's switches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub shadows: bool,
    pub bounce: bool,
    pub ambient_occlusion: bool,
    pub anti_aliasing: bool,
    /// Depth mapping's level, 0 (Off) to 3 (High).
    pub relief: u8,
    /// The cover's share of the pack's reach; zero for none.
    pub cover: f32,
    /// The world's share of the output's pixels; `None` for dynamic.
    pub render_scale: Option<f32>,
    /// The frame rate a dynamic scale holds.
    pub frame_rate: u32,
    pub vsync: bool,
    pub reach: Reach,
    pub weather: bool,
    /// The God Rays level, 0 (off) to 3.
    pub god_rays: u32,
    /// The cloud deck's detail, 0 (the retail deck) to 3.
    pub cloud_detail: u32,
    pub flicker: bool,
    /// Silk Cobwebs: the cobweb cards drawn as silk.
    pub webs: bool,
    /// Foliage Wind: trees and cover sway.
    pub wind: bool,
    /// How far lamps light the world, a share of [`pyreal_engine::lamps::LAMP_DISTANCE`].
    pub light_reach: f32,
    /// How far along the view the sun's shadows reach, metres.
    pub shadow_distance: f32,
    /// The haze's thickness, 1 the world fog's own; 0 none.
    pub fog: f32,
    /// How far out objects keep their detail, 1 the dats' and the pack's distances.
    pub detail: f32,
    /// How tall the grass grows, 1 the pack's own height.
    pub grass_height: f32,
    /// Water Detail's stored level: 0 Low, 1 Medium, 2 High.
    pub water_detail: u32,
    pub hdr: bool,
    /// High-Res Textures: the high-res level of a two-level picture where the file is installed.
    pub high_res: bool,
    /// Landscape Smoothing's strength: 0 retail's triangles, 1 Low, 2 Medium, 3 High.
    pub smoothing: u8,
}

impl Settings {
    /// The switches the preferences ask for.
    #[must_use]
    pub fn from_preferences(prefs: &RenderPreferences) -> Self {
        let f = &prefs.fidelity;
        let on = |x: F| f.effective(x) != 0;
        let scale = f.effective(F::RenderScale);
        let render_scale = if scale == pyreal_client_contract::options::render_scale::DYNAMIC {
            None
        } else if scale == 0 {
            Some(1.0)
        } else {
            #[allow(clippy::cast_precision_loss)]
            Some((scale as f32 / 100.0).clamp(0.5, 1.0))
        };
        let cover = match f.effective(F::GroundCoverDrawDistance) {
            0 => 1.0,
            v if i32::try_from(v).ok()
                == Some(pyreal_client_contract::options::draw_distance::OFF) =>
            {
                0.0
            }
            #[allow(clippy::cast_precision_loss)]
            v => v as f32 / 100.0,
        };
        let frame_rate = match f.effective(F::DynamicResolution) {
            0 => 60,
            v => v.clamp(30, 240),
        };
        let distance = i32::try_from(prefs.landscape_draw_distance).unwrap_or(0);
        let radius = 4;
        let far = if distance <= 0 {
            12
        } else {
            distance.clamp(radius, MOST_FAR)
        };
        // Cloud Detail stores Low as 0 and Off past High.
        let cloud_detail = match f.effective(F::CloudDetail) {
            0 => 1,
            1 => 2,
            2 => 3,
            _ => 0,
        };
        let light_reach = match f.effective(F::LightDrawDistance) {
            0 => 1.5,
            #[allow(clippy::cast_precision_loss)]
            v => (v as f32 / 100.0).clamp(0.5, 5.0),
        };
        #[allow(clippy::cast_precision_loss)] // metres, under a thousand
        let shadow_distance = match f.effective(F::ShadowDistance) {
            0 => pyreal_client_contract::options::shadow_distance::DEFAULT as f32,
            v => v as f32,
        };
        let fog = pyreal_client_contract::options::fog_density::density(f.effective(F::FogDensity));
        let detail =
            pyreal_client_contract::options::object_detail::scale(f.effective(F::ObjectDetail));
        let grass_height =
            pyreal_client_contract::options::grass_height::scale(f.effective(F::GrassHeight));
        Self {
            fog,
            detail,
            grass_height,
            water_detail: f.effective(F::WaterDetail).min(2),
            weather: on(F::Weather),
            god_rays: f.effective(F::GodRays).min(3),
            cloud_detail,
            flicker: !on(F::NoFlameFlicker),
            wind: !on(F::NoWind),
            webs: !on(F::NoSilkCobwebs),
            light_reach,
            shadow_distance,
            hdr: on(F::Hdr),
            high_res: !on(F::NoHighResTextures),
            smoothing: pyreal_client_contract::options::landscape_smoothing::strength(
                f.effective(F::LandscapeSmoothing),
            ),
            shadows: on(F::Shadows),
            bounce: on(F::GlobalIllumination),
            ambient_occlusion: on(F::AmbientOcclusion),
            anti_aliasing: !on(F::NoAntiAliasing),
            relief: u8::try_from(f.effective(F::DepthMapping).min(3)).unwrap_or(0),
            cover,
            render_scale,
            frame_rate,
            vsync: on(F::VSync),
            reach: Reach { radius, far },
        }
    }

    /// Sets the engine's switches.
    pub fn apply(&self, engine: &mut Engine) {
        let r = &mut engine.renderer;
        r.shadows = self.shadows;
        r.shadow_distance = self.shadow_distance;
        r.bounce = self.bounce;
        r.ambient_occlusion = self.ambient_occlusion;
        r.weather.on = self.weather;
        r.air.level = self.god_rays;
        r.atmosphere.cloud_detail = self.cloud_detail;
        r.atmosphere.fog_density = self.fog;
        r.lod_bias = self.detail;
        r.flicker = self.flicker;
        r.foliage_wind = if self.wind { 1.0 } else { 0.0 };
        r.webs.on = self.webs;
        r.lamp_reach = pyreal_engine::lamps::LAMP_DISTANCE * self.light_reach;
        r.anti_aliasing = self.anti_aliasing;
        r.surfaces.relief = self.relief;
        r.cover_on = self.cover > 0.0;
        r.cover_reach = self.cover.max(0.05);
        r.cover_height = self.grass_height;
        r.water.finest = pyreal_engine::water::finest_level(self.water_detail);
        match self.render_scale {
            Some(s) => {
                r.render_scale = s;
                r.dynamic = None;
            }
            None => {
                r.render_scale = 1.0;
                let budget = 900.0 / f64::from(self.frame_rate);
                let same = r
                    .dynamic
                    .is_some_and(|d| (d.budget_ms - budget).abs() < 1e-6);
                if !same {
                    r.dynamic = Some(pyreal_engine::temporal::DynamicResolution::new(budget));
                }
            }
        }
    }
}

/// The boxes in effect as a report writes them, `Name=value` a comma between.
#[must_use]
pub fn effects_text(f: &FidelityPreferences) -> String {
    F::ALL
        .into_iter()
        .filter(|x| f.effective(*x) != 0)
        .map(|x| {
            let name = x.name();
            let name = name.strip_prefix("Fidelity.").unwrap_or(name);
            format!("{name}={}", f.effective(x))
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyreal_client_contract::PrefValue;

    /// Every option the graphics page sets reaches the engine's switches through the live
    /// preference road (a page's write is `RenderPreferences::set_named`, polled each frame): each
    /// changes [`Settings`] from its default, but those that draw nothing here.
    #[test]
    fn every_graphics_option_changes_the_engines_switches() {
        let base = RenderPreferences::default();
        let before = Settings::from_preferences(&base);
        let mut inert = Vec::new();
        for f in F::ALL {
            let other = match f {
                F::FogDensity => 100,
                F::GrassHeight => 170,
                F::LightDrawDistance
                | F::GroundCoverDrawDistance
                | F::ObjectDetail
                | F::ShadowDistance => 300,
                F::DynamicResolution => 120,
                F::RenderScale => 60,
                _ if base.fidelity.effective(f) == 0 => f.max().clamp(1, 2),
                _ => 0,
            };
            let mut p = base;
            let value = PrefValue::Int(i32::try_from(other).unwrap_or(0));
            assert!(p.set_named(f.name(), &value), "{} is taken live", f.name());
            if Settings::from_preferences(&p) == before {
                inert.push(f.name());
            }
        }
        assert!(inert.is_empty(), "options that reach nothing: {inert:?}");
    }

    /// High-Res Textures starts on, and its box off draws a picture's original level.
    #[test]
    fn high_res_textures_start_on_and_the_box_turns_them_off() {
        let mut p = RenderPreferences::default();
        assert!(Settings::from_preferences(&p).high_res);
        assert!(p.set_named(F::NoHighResTextures.name(), &PrefValue::Bool(true)));
        assert!(!Settings::from_preferences(&p).high_res);
    }

    /// Each landscape draw distance the menu offers is the far terrain's reach, Horizon's the
    /// farthest.
    #[test]
    fn the_landscape_draw_distances_reach_the_far_terrain_up_to_the_horizon() {
        let choices = pyreal_client_contract::options::store::enum_choices(
            pyreal_client_contract::options::names::LANDSCAPE_DRAW_DISTANCE,
        )
        .expect("the menu");
        let mut p = RenderPreferences::default();
        let reach = |p: &RenderPreferences| Settings::from_preferences(p).reach.far;
        for &v in choices.values {
            p.landscape_draw_distance = u32::try_from(v).unwrap_or(0);
            assert_eq!(reach(&p), v.max(4), "{v}");
        }
        assert_eq!(choices.resolve("Horizon"), MOST_FAR);
        p.landscape_draw_distance = 400;
        assert_eq!(reach(&p), MOST_FAR);
    }
}
