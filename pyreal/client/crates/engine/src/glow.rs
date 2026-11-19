//! Creatures' light: a wisp's trail, a rift's swirl and sparkles, an energy cluster's haze.
//!
//! Retail draws these as pictures of light, a soft coloured ball or a lens star, added to what is
//! behind. A creature's particle whose picture is one of these is drawn as light instead
//! ([`crate::particles::Look::Glow`]): a hot core whitening at its heart in a soft halo of the
//! picture's colour, a star with thin rays, bright enough to bloom. Each creature carries one lamp
//! of its light's colour where its light is, so the ground under a wisp lights as it flies.
//!
//! Which pictures are light is read from the pictures themselves: brightest at the middle and
//! falling away, either round (a ball) or rayed (a star). Fire's puffs, clouds, lightning and
//! tendrils are ragged or dark at the middle, and keep their own looks.

use glam::Vec3;
use pyreal_assets::{GfxObj, Surface};
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::num::math;
use pyreal_primitives::DataId;

use crate::asset::texture::TextureSource;
use crate::lamps::decode;

/// A picture of light's shape: a soft ball, or a lens star.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlowKind {
    Ball = 0,
    Star = 1,
}

/// A picture of light: its shape, its colour (linear, brightest channel 1), how bright its heart
/// is (its brightest channel there, 0 to 1), so a faint haze stays faint, and how far out from its
/// middle half its light lies (over its half width), so a glow is as soft as its picture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glow {
    pub kind: GlowKind,
    pub colour: Vec3,
    pub strength: f32,
    pub spread: f32,
}

impl Glow {
    /// Its look in a word: red, green and blue (4 bits each, gamma 2.2) at bits 20-31, its spread
    /// at bits 16-19 and its strength at bits 8-15; the particle's alpha takes bits 0-7.
    #[must_use]
    pub fn word(&self, alpha: f32) -> u32 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 to the bits' most
        let q = |v: f32, most: f32| (v.clamp(0.0, 1.0) * most).round() as u32;
        let g = |v: f32| math::powf(v.max(0.0), 1.0 / 2.2);
        q(alpha, 255.0)
            | q(self.strength, 255.0) << 8
            | q(self.spread, 15.0) << 16
            | q(g(self.colour.z), 15.0) << 20
            | q(g(self.colour.y), 15.0) << 24
            | q(g(self.colour.x), 15.0) << 28
    }
}

/// The lamp a creature's light carries: a magic light's most, reaching a few metres.
pub const LAMP_INTENSITY: f32 = crate::lamps::MAGIC_INTENSITY;
pub const LAMP_FALLOFF: f32 = 5.0;
/// A held thing's light (a staff's crystal): a quarter of a creature's, a glow and not a torch.
pub const HELD_SHARE: f32 = 0.25;
pub const HELD_INTENSITY: f32 = LAMP_INTENSITY * HELD_SHARE;

/// The lamp a body's light carries, of `colour`: a creature's, or a held thing's (`held`).
#[must_use]
pub fn carried_source(colour: Vec3, held: bool) -> crate::lamps::LampSource {
    crate::lamps::LampSource {
        kind: crate::lamps::LampKind::Glow,
        offset: Vec3::ZERO,
        colour: colour / colour.max_element().max(1e-6),
        intensity: if held { HELD_INTENSITY } else { LAMP_INTENSITY },
        falloff: LAMP_FALLOFF,
        flicker: crate::lamps::Flicker::Steady,
        shadows: false,
    }
}

/// `lamp` as a held thing's light: its sprite's own `colour`, unpaled, so a purple crystal lights
/// purple.
#[must_use]
pub fn held_lamp(mut lamp: crate::lamps::Lamp, colour: Vec3) -> crate::lamps::Lamp {
    lamp.held = true;
    lamp.colour = colour;
    lamp
}

/// Rings from the middle a picture is measured in, and sectors round its middle band.
const RINGS: usize = 8;
const SECTORS: usize = 16;

/// The most a ball's brightness varies round its middle band (over its mean), and the least of
/// its brightness its rings explain; the most a star's rings may leave unexplained, and how much
/// its brightness varies round (its rays) or how much brighter its heart is than the ring past it.
const BALL_ROUND: f32 = 0.1;
const BALL_SYMMETRY: f32 = 0.8;
const STAR_SYMMETRY: f32 = 0.5;
const STAR_RAYS: f32 = 0.5;
const STAR_POINT: f32 = 3.0;

/// Graphics object `gfx`'s picture of light, when every surface adds its light and its picture is
/// a ball or a star.
#[must_use]
pub fn glow_of(store: &RetailDatStore, gfx: DataId) -> Option<Glow> {
    let obj: GfxObj = decode(store, DbType::GfxObj, gfx)?;
    let surfs: Vec<Surface> = obj
        .surfaces
        .iter()
        .filter_map(|s| decode::<Surface>(store, DbType::Surface, *s))
        .collect();
    if surfs.is_empty() || surfs.iter().any(|s| s.surface_type & 0x1_0000 == 0) {
        return None;
    }
    let (w, h, px) = TextureSource::new(store)
        .bgra(surfs[0].orig_texture_id?)
        .ok()?;
    measure(w, h, &px)
}

/// A picture's [`Glow`], from its texels (blue, green, red, alpha; rows from the top).
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // a picture's texel counts and places, and the ring or sector one falls in
pub fn measure(w: u32, h: u32, px: &[u8]) -> Option<Glow> {
    if w == 0 || h == 0 || px.len() < (w * h * 4) as usize {
        return None;
    }
    let lin = |c: u8| math::powf(f32::from(c) / 255.0, 2.2);
    let (wf, hf) = (w as f32, h as f32);
    let mut texels = Vec::with_capacity((w * h) as usize);
    let (mut colour, mut heart, mut heart_n) = (Vec3::ZERO, Vec3::ZERO, 0.0);
    for y in 0..h {
        for x in 0..w {
            let p = &px[((y * w + x) * 4) as usize..][..4];
            let a = f32::from(p[3]) / 255.0;
            let c = Vec3::new(lin(p[2]), lin(p[1]), lin(p[0])) * a;
            let l = c.dot(Vec3::new(0.2126, 0.7152, 0.0722));
            colour += c * l;
            let d = glam::Vec2::new((x as f32 + 0.5) / wf, (y as f32 + 0.5) / hf) * 2.0 - 1.0;
            if d.length() < 1.0 / RINGS as f32 {
                heart += c;
                heart_n += 1.0;
            }
            texels.push((d, l));
        }
    }
    let ring_of = |d: glam::Vec2| ((d.length() * RINGS as f32) as usize).min(RINGS - 1);
    let (mut ring, mut ring_n) = ([0f32; RINGS], [0f32; RINGS]);
    let (mut sector, mut sector_n) = ([0f32; SECTORS], [0f32; SECTORS]);
    for &(d, l) in &texels {
        ring[ring_of(d)] += l;
        ring_n[ring_of(d)] += 1.0;
        let r = d.length();
        if r > 0.3 && r < 0.7 {
            let turn = math::atan2f(d.y, d.x) / std::f32::consts::TAU + 0.5;
            let k = ((turn * SECTORS as f32) as usize).min(SECTORS - 1);
            sector[k] += l;
            sector_n[k] += 1.0;
        }
    }
    for (r, n) in ring.iter_mut().zip(ring_n) {
        *r /= n.max(1.0);
    }
    for (s, n) in sector.iter_mut().zip(sector_n) {
        *s /= n.max(1.0);
    }
    let mean = texels.iter().map(|t| t.1).sum::<f32>() / texels.len() as f32;
    let (mut spread, mut left) = (0.0, 0.0);
    for &(d, l) in &texels {
        spread += (l - mean).powi(2);
        left += (l - ring[ring_of(d)]).powi(2);
    }
    let symmetry = 1.0 - left / f32::max(spread, 1e-9);
    let around = sector.iter().sum::<f32>() / SECTORS as f32;
    let rays = (sector.iter().map(|s| (s - around).powi(2)).sum::<f32>() / SECTORS as f32).sqrt()
        / around.max(1e-6);
    // Brightest at the middle.
    if ring[0] <= 0.0 || ring.iter().any(|&r| r > ring[0]) || colour.max_element() <= 0.0 {
        return None;
    }
    let kind = if rays <= BALL_ROUND && symmetry >= BALL_SYMMETRY {
        GlowKind::Ball
    } else if symmetry >= STAR_SYMMETRY && (rays >= STAR_RAYS || ring[0] >= STAR_POINT * ring[1]) {
        GlowKind::Star
    } else {
        return None;
    };
    // Half its light within: the rings' light summed outward, to where it passes half.
    let light: Vec<f32> = ring.iter().zip(ring_n).map(|(r, n)| r * n).collect();
    let total = light.iter().sum::<f32>().max(1e-9);
    let mut within = 0.0;
    let mut spread = 1.0;
    for (k, l) in light.iter().enumerate() {
        if within + l >= total * 0.5 {
            spread = (k as f32 + (total * 0.5 - within) / l.max(1e-9)) / RINGS as f32;
            break;
        }
        within += l;
    }
    Some(Glow {
        kind,
        colour: colour / colour.max_element(),
        strength: (heart / f32::max(heart_n, 1.0)).max_element().min(1.0),
        spread: spread.clamp(1.0 / 15.0, 1.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A held thing's light (Ethangar's Staff's crystal) lights a quarter as brightly as a
    /// creature's, over the same reach, in its own colour, and says so to the outdoor shading.
    #[test]
    fn a_held_things_light_is_a_quarter_of_a_creatures() {
        let at = glam::Affine3A::IDENTITY;
        let purple = Vec3::new(0.4, 0.09, 1.0);
        let creature = crate::lamps::place(&carried_source(purple, false), &at, 0, u32::MAX);
        let source = carried_source(purple, true);
        let held = held_lamp(
            crate::lamps::place(&source, &at, 0, u32::MAX),
            source.colour,
        );
        assert!((held.candela - creature.candela * HELD_SHARE).abs() < 1e-6 * creature.candela);
        assert!((held.range - creature.range).abs() < 1e-6);
        // Its own purple, where a creature's is paled toward grey.
        assert_eq!(held.colour, purple / purple.max_element());
        let spread = |c: Vec3| (c.max_element() - c.min_element()) / c.max_element();
        assert!(spread(held.colour) > spread(creature.colour) + 0.2);
        let flags = |l: &crate::lamps::Lamp| {
            let b = crate::lamps::lamp_bytes(l, 0, 0);
            u32::from_le_bytes([b[28], b[29], b[30], b[31]])
        };
        assert_eq!(flags(&held) & !(1 << 16), flags(&creature) & !(1 << 16));
        assert_ne!(flags(&held) & (1 << 16), 0);
        let wgsl = include_str!("shaders/lamp_types.wgsl");
        assert!(wgsl.contains(&format!("const HELD_SHARE: f32 = {HELD_SHARE:?};")));
    }

    /// A 32-texel square picture of `f` over its card (-1 to 1 across), blue-green.
    fn picture(f: impl Fn(f32, f32) -> f32) -> Vec<u8> {
        let mut px = Vec::new();
        for y in 0..32u8 {
            for x in 0..32u8 {
                let d = |i: u8| (f32::from(i) + 0.5) / 16.0 - 1.0;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 to 255
                let v = (f(d(x), d(y)).clamp(0.0, 1.0) * 255.0) as u8;
                px.extend_from_slice(&[v, v / 2, 0, 255]);
            }
        }
        px
    }

    #[test]
    fn a_soft_ball_a_rayed_star_and_a_ring_are_told_apart() {
        let ball = picture(|x, y| 1.0 - (x * x + y * y).sqrt());
        let star = picture(|x, y| {
            let ray = |a: f32, b: f32| (1.0 - a.abs()) * (1.0 - b.abs() * 12.0).max(0.0);
            (ray(x, y) + ray(y, x)).max(1.0 - (x * x + y * y).sqrt() * 6.0)
        });
        let ring = picture(|x, y| 1.0 - ((x * x + y * y).sqrt() - 0.6).abs() * 4.0);
        assert_eq!(measure(32, 32, &ball).map(|g| g.kind), Some(GlowKind::Ball));
        assert_eq!(measure(32, 32, &star).map(|g| g.kind), Some(GlowKind::Star));
        assert_eq!(measure(32, 32, &ring), None);
        let g = measure(32, 32, &ball).expect("a ball");
        assert!(
            g.colour.z > g.colour.y && g.colour.x < 0.01,
            "{:?}",
            g.colour
        );
        assert!(g.strength > 0.8, "{}", g.strength);
        let s = measure(32, 32, &star).expect("a star");
        assert!(g.spread > s.spread, "{} {}", g.spread, s.spread);
    }
}
