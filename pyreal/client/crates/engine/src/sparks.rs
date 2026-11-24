//! The crystal creatures' sparkles: retail swarms a lens star about each shard and lays a haze
//! round it, magenta or red whatever the shard's colour. With the enhanced looks they are drawn as
//! light in the crystal's own colour, which its palette gives: the stars as small motes of light
//! that twinkle, the haze and the light the shard casts tinted to match
//! ([`crate::particles::Look::Spark`]).

use glam::Vec3;
use pyreal_animation::MotionDriver;
use pyreal_primitives::num::math;
use pyreal_primitives::DataId;

use crate::glow::{Glow, GlowKind};

/// The shards' default scripts (setups 0x02000700 to 0x02000713, 0x020008F9 to 0x020008FC and the
/// Recording Device's 0x02001AC7), which their sparkles come from. Spells swarm the same stars.
pub const SCRIPTS: [u32; 9] = [
    0x3300_07C3,
    0x3300_07D8,
    0x3300_07D9,
    0x3300_07DA,
    0x3300_094E,
    0x3300_094F,
    0x3300_0950,
    0x3300_0951,
    0x3300_12DE,
];
/// Their lens stars, magenta, red and white, and their hazes.
pub const STARS: [u32; 3] = [0x0100_10C2, 0x0100_10B2, 0x0100_10A3];
pub const HAZES: [u32; 2] = [0x0100_192C, 0x0100_192B];

/// A mote's size over its star's, how far out half its light lies, and how bright its heart is.
pub const MOTE_SIZE: f32 = 0.5;
const MOTE_SPREAD: f32 = 0.35;
const MOTE_STRENGTH: f32 = 0.6;
/// How deep a mote's twinkle dips (a share of its light) and how fast it beats, per second.
const TWINKLE_DEPTH: f32 = 0.6;
const TWINKLE_RATE: f32 = 3.0;

/// Whether `driver` is a crystal shard, whose sparkles are drawn as its own light.
#[must_use]
pub fn crystal(driver: &MotionDriver) -> bool {
    driver
        .part_array
        .setup
        .as_ref()
        .and_then(|s| s.default_script)
        .is_some_and(|s| SCRIPTS.contains(&s.0))
}

/// Whether sprite `gfx` is a shard's star (a mote), a haze, or neither.
#[must_use]
pub fn mote(gfx: u32) -> Option<bool> {
    if STARS.contains(&gfx) {
        Some(true)
    } else if HAZES.contains(&gfx) {
        Some(false)
    } else {
        None
    }
}

/// The palette `driver`'s look lays over its body, whose colour its light takes: its palette
/// template's, else its base palette.
#[must_use]
pub fn palette(driver: &MotionDriver) -> Option<DataId> {
    let looks = driver
        .part_array
        .parts
        .iter()
        .filter_map(|p| p.surface_overrides.as_ref());
    let mut base = None;
    for o in looks {
        if let Some(r) = o.subpalettes.first() {
            return Some(r.palette_set);
        }
        base = base.or(o.shift_palette);
    }
    base
}

/// The shards' picture, whose colour in a shard's palette its light takes.
pub const PICTURE: u32 = 0x0500_0E18;

/// How bright a picture is drawn (the mean of each texel's brightest channel) for its light to
/// be whole: a dark crystal's is dimmer.
const FULL_VALUE: f32 = 0.6;

/// A picture's light colour (its texels, blue, green, red and alpha): each texel weighed by how
/// bright it is, brightest channel 1, dimmed when the picture is dark. `None` for one of black.
#[must_use]
#[allow(clippy::cast_precision_loss)] // a picture's texel count
pub fn colour_of(px: &[u8]) -> Option<Vec3> {
    let lin = |c: u8| math::powf(f32::from(c) / 255.0, 2.2);
    let (mut sum, mut value) = (Vec3::ZERO, 0.0);
    for p in px.as_chunks::<4>().0 {
        let rgb = Vec3::new(lin(p[2]), lin(p[1]), lin(p[0])) * (f32::from(p[3]) / 255.0);
        sum += rgb * rgb.dot(Vec3::new(0.2126, 0.7152, 0.0722));
        value += f32::from(p[0].max(p[1]).max(p[2])) / 255.0;
    }
    let value = value / (px.len() / 4).max(1) as f32;
    (sum.max_element() > 1e-6).then(|| sum / sum.max_element() * (value / FULL_VALUE).min(1.0))
}

/// A colour in 12 bits, red, green and blue at 4 bits each (gamma 2.2), and back.
#[must_use]
pub fn pack(c: Vec3) -> u16 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 to 15
    let q = |v: f32| (math::powf(v.clamp(0.0, 1.0), 1.0 / 2.2) * 15.0).round() as u16;
    q(c.x) << 8 | q(c.y) << 4 | q(c.z)
}

#[must_use]
pub fn unpack(bits: u16) -> Vec3 {
    let q = |s: u16| math::powf(f32::from((bits >> s) & 15) / 15.0, 2.2);
    Vec3::new(q(8), q(4), q(0))
}

/// The light a spark is drawn as: a mote's small hot ball, or its haze's own glow, in `tint`.
#[must_use]
pub fn light(tint: u16, mote: bool, own: Option<Glow>) -> Glow {
    let colour = unpack(tint);
    match own {
        Some(g) if !mote => Glow {
            kind: GlowKind::Ball,
            colour,
            ..g
        },
        _ => Glow {
            kind: GlowKind::Ball,
            colour,
            strength: MOTE_STRENGTH,
            spread: MOTE_SPREAD,
        },
    }
}

/// How much of a mote's light shows at `seconds`: each its own beat, from its `seed`.
#[must_use]
pub fn twinkle(seed: u32, seconds: f64) -> f32 {
    #[allow(clippy::cast_precision_loss)] // a seed's phase
    let phase = (seed % 997) as f32 / 997.0 * std::f32::consts::TAU;
    let rate = TWINKLE_RATE * (0.75 + 0.5 * ((seed >> 10) % 101) as f32 / 100.0);
    #[allow(clippy::cast_possible_truncation)] // seconds into a beat
    let t = (seconds % 3600.0) as f32;
    let s = 0.5 + 0.5 * math::sinf(t * rate + phase);
    1.0 - TWINKLE_DEPTH * s * s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bright green picture's light is green, a dark grey one's a dim white, a black one's none,
    /// and a colour keeps its hue in 12 bits.
    #[test]
    fn a_pictures_light_is_its_bright_texels_hue() {
        let green: Vec<u8> = (0..=255u8)
            .flat_map(|i| [i / 3, i.max(200), i / 4, 255])
            .collect();
        let c = colour_of(&green).expect("a colour");
        assert!((c.y - 1.0).abs() < 1e-6 && c.x < 0.2 && c.z < 0.2, "{c:?}");
        let dark = colour_of(&[40, 40, 40, 255].repeat(16)).expect("a colour");
        assert!(
            dark.max_element() < 0.4 && dark.min_element() == dark.max_element(),
            "{dark:?}"
        );
        assert_eq!(colour_of(&[0, 0, 0, 255].repeat(16)), None);
        let back = unpack(pack(c));
        assert!((back - c).abs().max_element() < 0.05, "{back:?} {c:?}");
    }

    /// Motes twinkle: their light dips and comes back, each on its own beat.
    #[test]
    fn motes_twinkle_on_beats_of_their_own() {
        let seen: Vec<f32> = (0..40).map(|k| twinkle(7, f64::from(k) * 0.1)).collect();
        let (lo, hi) = seen
            .iter()
            .fold((1.0f32, 0.0f32), |(a, b), &v| (a.min(v), b.max(v)));
        assert!(lo < 0.6 && hi > 0.95, "{lo} {hi}");
        assert!((twinkle(7, 1.0) - twinkle(500_000, 1.0)).abs() > 1e-3);
        assert_eq!(mote(0x0100_10C2), Some(true));
        assert_eq!(mote(0x0100_192C), Some(false));
        assert_eq!(mote(0x0100_10F9), None);
    }
}
