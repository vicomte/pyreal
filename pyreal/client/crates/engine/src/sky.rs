//! The sun and the sky's light at a time of day, from the region's day group (its sky times).
//! The sun and the ambient stay in the dats' own colour space, where the client sums them; the
//! fog is linear.
//!
//! **Physical units.** The HDR target holds light in scene units: 1 is the light of 10,000 lux
//! falling on a white surface ([`SCENE_LUX`]). The dats give the colours and how they change
//! through the day; [`SkyLight::sun_light`] and [`SkyLight::sky_light`] scale them to lux: full
//! sun 100,000 lux, the moon a few lux, a starlit sky under one. Daylight is read from the world
//! fog's brightness, which the dats darken at night (their "sun" never sets; at night it is the
//! moon's light).

use glam::Vec3;
use pyreal_assets::Region;
use pyreal_primitives::num::math::{expf, logf, powf};
use pyreal_world_render::sky::{get_lighting, get_world_fog};

use crate::asset::material::srgb_to_linear;

/// The light of one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyLight {
    /// Unit vector toward the sun.
    pub sun_dir: Vec3,
    /// The sun's colour times its brightness (the length of the client's sun vector).
    pub sun: Vec3,
    /// The sky's ambient colour times its level.
    pub ambient: Vec3,
    /// The sun's brightness alone.
    pub sun_strength: f32,
    /// The world fog's colour, linear, and the distances it starts and is whole at.
    pub fog: Vec3,
    pub fog_start: f32,
    pub fog_end: f32,
    /// How much of a day it is, 0 night to 1 full day, from the fog's brightness.
    pub daylight: f32,
    /// The directional light (the sun by day, the moon by night) in scene units, linear.
    pub sun_light: Vec3,
    /// The sky's light on an open, level surface in scene units, linear.
    pub sky_light: Vec3,
    /// The day group and the time of day (0 to 1) it is the light of.
    pub group: usize,
    pub time: f32,
    /// The day group's weather, from its name.
    pub weather: Weather,
}

/// The kinds of weather the day groups are named for (render-hifi's reading).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Weather {
    Clear,
    #[default]
    Sunny,
    Cloudy,
    Rainy,
}

impl Weather {
    /// The weather of a day group by its name; an unknown name is sunny.
    #[must_use]
    pub fn of(day_group: &str) -> Self {
        let name = day_group.trim().to_ascii_lowercase();
        if name.starts_with("rain") || name.starts_with("storm") || name.starts_with("snow") {
            Self::Rainy
        } else if name.starts_with("cloud") || name.starts_with("overcast") {
            Self::Cloudy
        } else if name.starts_with("clear") {
            Self::Clear
        } else {
            Self::Sunny
        }
    }

    /// How much more haze (Mie scattering) than the Earth's clear air, and how grey the sky is.
    #[must_use]
    pub fn haze(self) -> (f32, f32) {
        match self {
            Self::Clear => (0.6, 0.0),
            Self::Sunny => (1.0, 0.0),
            Self::Cloudy => (5.0, 0.45),
            Self::Rainy => (9.0, 0.7),
        }
    }
}

/// Lux per scene unit: an HDR value of 1 is a white surface under 10,000 lux.
pub const SCENE_LUX: f32 = 10_000.0;
/// The sun's illuminance at the dats' strength 1, lux.
pub const SUN_LUX: f32 = 100_000.0;
/// The moon's at the dats' night strength 1, lux (a bright moonlit night is a few tenths).
pub const MOON_LUX: f32 = 0.6;
/// The sky's light per unit of the dats' linear ambient by day, lux: noon's ambient gives about
/// 13,000 lux, a seventh of the sun, as a clear sky does.
pub const DAY_SKY_LUX: f32 = 100_000.0;
/// The night sky's per unit of the dats' linear ambient, lux.
pub const NIGHT_SKY_LUX: f32 = 18.0;
/// The night dome's own, darker than the light it gives: retail's night layer leaves the sky near
/// black between its stars and clouds.
pub const NIGHT_DOME_LUX: f32 = 3.0;
/// The night's grey: a cool blue-grey with a hint of the dats' purple.
pub const NIGHT_GREY: Vec3 = Vec3::new(0.9, 0.9, 1.3);
/// How much of the dats' night colour the sky's light and the fog lose at full night.
pub const NIGHT_SKY_GREY: f32 = 0.8;
/// The fog's linear brightness at full night and at full day: the daylight ramp's ends.
pub const NIGHT_FOG: f32 = 0.012;
pub const DAY_FOG: f32 = 0.3;

/// How much of a day a fog of linear colour `fog` says it is: 0 night to 1 day, along the
/// logarithm of its brightness.
#[must_use]
pub fn daylight(fog: Vec3) -> f32 {
    let l = fog.dot(Vec3::new(0.2126, 0.7152, 0.0722)).max(1e-6);
    let t = ((logf(l) - logf(NIGHT_FOG)) / (logf(DAY_FOG) - logf(NIGHT_FOG))).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A gamma colour times a level, made linear.
fn linear_scaled(c: Vec3) -> Vec3 {
    let f = |v: f32| powf(v.max(0.0), 2.2);
    Vec3::new(f(c.x), f(c.y), f(c.z))
}

/// The power the sun's colour is made linear with for its hue (its brightness keeps 2.2): retail
/// sums its sun and ambient before the display's gamma, which pales the sun; 2.2 alone saturates
/// it, and sunlit sand reads orange.
pub const SUN_HUE_GAMMA: f32 = 1.6;

/// The sun's gamma colour times its level made linear, its hue at [`SUN_HUE_GAMMA`].
fn sun_linear(c: Vec3) -> Vec3 {
    let luma = |c: Vec3| c.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    let f = |v: f32| powf(v.max(0.0), SUN_HUE_GAMMA);
    let hue = Vec3::new(f(c.x), f(c.y), f(c.z));
    hue * (luma(linear_scaled(c)) / luma(hue).max(1e-9))
}

/// The sky's light per unit of the dats' linear ambient at daylight `day`, lux: on a log scale,
/// so dusk dims as the eye sees it.
#[must_use]
pub fn sky_lux(day: f32) -> f32 {
    expf(logf(NIGHT_SKY_LUX) + (logf(DAY_SKY_LUX) - logf(NIGHT_SKY_LUX)) * day)
}

/// How much of the noon sun shines at daylight `day`: as the sky's light dims, on its log scale.
#[must_use]
pub fn sun_share(day: f32) -> f32 {
    (sky_lux(day) - NIGHT_SKY_LUX) / (DAY_SKY_LUX - NIGHT_SKY_LUX)
}

/// The physical lights of a moment: the directional light and the sky's, in scene units.
#[must_use]
pub fn physical(sun: Vec3, ambient: Vec3, day: f32) -> (Vec3, Vec3) {
    // Daylight on a log scale, so dusk dims as the eye sees it, not as a straight line; the sun
    // dims with the sky, so a low sun does not glare over a land the dusk sky lights.
    let sky_lux = sky_lux(day);
    let sun_lux = MOON_LUX + (SUN_LUX - MOON_LUX) * sun_share(day);
    (
        sun_linear(sun) * sun_lux / SCENE_LUX,
        linear_scaled(ambient) * sky_lux / SCENE_LUX,
    )
}

fn unit(c: [u8; 3]) -> Vec3 {
    Vec3::new(f32::from(c[0]), f32::from(c[1]), f32::from(c[2])) / 255.0
}

fn linear(c: [u8; 3]) -> Vec3 {
    Vec3::new(
        srgb_to_linear(c[0]),
        srgb_to_linear(c[1]),
        srgb_to_linear(c[2]),
    )
}

/// The time of day (0 to 1) and the day group the game clock `clock` (seconds) falls on, as the
/// client's calendar reckons them.
#[must_use]
pub fn calendar(region: &Region, clock: f64) -> (f32, usize) {
    let g = &region.game_time;
    let day = f64::from(g.day_length.max(1.0));
    let year_len = day * f64::from(g.days_per_year.max(1));
    let y = clock + g.zero_time_of_year;
    let years = (y / year_len).floor();
    let rem = y - years * year_len;
    let d = (rem / day).floor();
    #[allow(clippy::cast_possible_truncation)]
    let time = ((rem - d * day) / day) as f32;
    let year = u32::try_from(pyreal_primitives::num::to_i32_f64(years))
        .unwrap_or(0)
        .wrapping_add(g.zero_year);
    let day_index = u32::try_from(pyreal_primitives::num::to_i32_f64(d)).unwrap_or(0);
    let groups = region.sky_info.as_ref().map_or(0, |s| s.day_groups.len());
    let group =
        pyreal_assets::region::calc_present_day_group(year, g.days_per_year, day_index, groups);
    (time, group)
}

impl SkyLight {
    /// The sky's radiance (times pi) per unit of the fog's colour, scene units: the horizon of a
    /// clear noon about as bright as a white wall in the sky's light.
    #[must_use]
    pub fn sky_scale(&self) -> f32 {
        let day = self.daylight;
        expf(logf(NIGHT_DOME_LUX) + (logf(DAY_SKY_LUX) - logf(NIGHT_DOME_LUX)) * day) / SCENE_LUX
            * 0.5
    }

    /// The light of `region`'s day group `group` at `time` (0 midnight, 0.5 noon).
    #[must_use]
    pub fn at(region: &Region, group: usize, time: f32) -> Self {
        let Some(g) = region
            .sky_info
            .as_ref()
            .and_then(|s| s.day_groups.get(group).or_else(|| s.day_groups.first()))
        else {
            return Self::default();
        };
        let t = time.rem_euclid(1.0);
        let weather = Weather::of(&g.day_name);
        let l = get_lighting(g, t);
        let v = Vec3::new(l.sun_vec.x, l.sun_vec.y, l.sun_vec.z);
        let bright = v.length();
        let sun_dir = if bright > 0.0 { v / bright } else { Vec3::Z };
        let (fog, fog_start, fog_end) = get_world_fog(g, t)
            .map_or((linear(l.ambient_color) * 0.8, 400.0, 1200.0), |f| {
                (linear(f.color), f.min, f.max)
            });
        let sun = unit(l.sun_color) * bright;
        let ambient = unit(l.ambient_color) * l.ambient_level;
        let day = daylight(fog);
        let (sun_light, sky_light) = physical(sun, ambient, day);
        // By night the dats' purples and magentas read as a wash over everything: the sky's light
        // and the fog are drawn toward a cool grey keeping a hint of the purple, the deeper the
        // night, and the moon lights in that grey, as retail's white moon does.
        let night = 1.0 - day;
        let moonlit = |c: Vec3, k: f32| {
            let grey = Vec3::splat(c.dot(Vec3::new(0.2126, 0.7152, 0.0722))) * NIGHT_GREY;
            c.lerp(grey, night * night * k)
        };
        let (sun_light, sky_light, fog) = (
            moonlit(sun_light, 1.0),
            moonlit(sky_light, NIGHT_SKY_GREY),
            moonlit(fog, NIGHT_SKY_GREY),
        );
        Self {
            sun_dir,
            sun,
            ambient,
            sun_strength: bright,
            fog,
            fog_start,
            fog_end,
            daylight: day,
            sun_light,
            sky_light,
            group,
            time: t,
            weather,
        }
    }
}

impl Default for SkyLight {
    /// The client's fallback light: ambient 0.3, white, the sun at `(0.5, 0, 0.8)`.
    fn default() -> Self {
        let v = Vec3::new(0.5, 0.0, 0.8);
        let (sun_light, sky_light) = physical(Vec3::splat(v.length()), Vec3::splat(0.3), 1.0);
        Self {
            sun_dir: v.normalize(),
            sun: Vec3::splat(v.length()),
            ambient: Vec3::splat(0.3),
            sun_strength: v.length(),
            fog: Vec3::new(0.6, 0.7, 0.8),
            fog_start: 400.0,
            fog_end: 1200.0,
            daylight: 1.0,
            sun_light,
            sky_light,
            group: 0,
            time: 0.5,
            weather: Weather::Sunny,
        }
    }
}
