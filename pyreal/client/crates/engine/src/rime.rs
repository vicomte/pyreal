//! Rime: a frost elemental's driven snow and glinting ice, round its cold core.
//!
//! A frost elemental's emitters ([`crate::puffs::SNOW`]) are drawn as one cold volume, rime at
//! its edges and ice-blue where it is deep (`shaders/puffs.wgsl`). Each of their particles also
//! carries a few flakes whirling round it and sinking as it lives ([`flakes`]), and now and then a
//! crystal that catches the light ([`crate::particles::Look::Flake`]): small lights drawn as the
//! creatures' light is ([`crate::glow`]), laid over the volume.

use glam::Vec3;
use pyreal_primitives::num::math::{cosf, powf, sinf};

use crate::glow::{Glow, GlowKind};

/// How many flakes a particle carries; and how many of them, in a hundred, are glints.
pub const FLAKES: u32 = 10;
const GLINTS: u32 = 12;
/// How wide a flake is and a glint, metres; and the least either is on screen, metres a metre
/// away, so a far one does not shimmer out.
const FLAKE: f32 = 0.022;
const GLINT: f32 = 0.14;
const LEAST: f32 = 0.0025;
/// How far out from its particle's middle a flake whirls (shares of its ball's reach), how fast
/// (radians a second), and how fast it sinks, metres a second.
const ORBIT: (f32, f32) = (0.3, 1.8);
const WHIRL: (f32, f32) = (1.2, 3.2);
const SINK: f32 = 0.35;
/// How often a glint flares, radians a second, and how short its flare is (higher, shorter).
const TWINKLE: f32 = 7.0;
const FLARE: f32 = 6.0;

/// A flake's light: white with a cold edge, small and hard.
pub const FLAKE_LIGHT: Glow = Glow {
    kind: GlowKind::Ball,
    colour: Vec3::new(0.85, 0.93, 1.0),
    strength: 0.6,
    spread: 0.28,
};
/// A glint's: a star of ice-white light.
pub const GLINT_LIGHT: Glow = Glow {
    kind: GlowKind::Star,
    colour: Vec3::new(0.78, 0.9, 1.0),
    strength: 0.9,
    spread: 0.18,
};

/// One flake: where it is from its particle, how wide, how much shows, and whether it glints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flake {
    pub offset: Vec3,
    pub radius: f32,
    pub alpha: f32,
    pub glint: bool,
}

fn hash(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

fn unit(h: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // 16 bits
    let v = (h & 0xFFFF) as f32 / 65_535.0;
    v
}

/// The flakes of a particle seeded `seed`, `seconds` into its life (`age` of it), its ball
/// `reach` metres, seen from `far` metres: each whirling round it at its own pace and sinking,
/// swelling in and out with its particle.
pub fn flakes(
    seed: u32,
    seconds: f32,
    age: f32,
    reach: f32,
    far: f32,
) -> impl Iterator<Item = Flake> {
    let swell = crate::puffs::swell(age);
    (0..FLAKES).map(move |j| {
        let h = hash(seed, j);
        let (u, v, w) = (unit(h), unit(h >> 8), unit(hash(h, 7)));
        let glint = hash(h, 11) % 100 < GLINTS;
        let r = reach * (ORBIT.0 + (ORBIT.1 - ORBIT.0) * u);
        let spin = (WHIRL.0 + (WHIRL.1 - WHIRL.0) * v) * if h & 1 == 0 { 1.0 } else { -1.0 };
        let turn = std::f32::consts::TAU * w + spin * seconds;
        let rise = reach * (v - 0.5) * 1.2 - SINK * seconds;
        let offset = Vec3::new(cosf(turn) * r, sinf(turn) * r, rise);
        let (size, alpha) = if glint {
            // Flares as the crystal turns to the light, then darkens.
            let s = sinf(seconds * TWINKLE + std::f32::consts::TAU * u).max(0.0);
            (GLINT, powf(s, FLARE))
        } else {
            (FLAKE * (0.6 + 0.8 * w), 1.0)
        };
        Flake {
            offset,
            radius: size.max(far * LEAST),
            alpha: alpha * swell,
            glint,
        }
    })
}
