//! Flames drawn in code where an object burns: torches, sconces, candles; braziers' and pyres'
//! beds of fire drawn as fire puffs.
//!
//! An object id is looked at once ([`examine`]): its default script's fire-coloured particle
//! emitters say where it burns and what kind of fire it is, by how the emitter's particles
//! behave ([`kind_of`]), with a short table of ids ([`KINDS`]) where that is not enough. Each
//! becomes a [`FlameSource`] at the emitter, close emitters merged into one. Placed, a flame takes
//! its object's nearest lamp's seed and reach, so the flame and its light flicker as one.
//!
//! Each frame the flames that can be seen are chosen ([`frame_flames`]) and drawn by
//! `flames.wgsl`: a view-facing card per flame, its fire made of scrolling noise under a
//! temperature ramp, drawn additively and bright enough to bloom. The emitters a flame stands for
//! are left out of the particles while flames are on ([`crate::particles::Particles::flamed`]).
//! A bed of fire ([`FlameKind::Brazier`]) has no card: retail builds it of large billowing fire
//! puffs, which keep their emitters and are drawn as fire
//! ([`crate::particles::Particles::puffed`]).

use std::collections::HashMap;

use glam::{Affine3A, Vec3, Vec4};
use pyreal_assets::Setup;
use pyreal_dat::{divine_type, DbType, RetailDatStore};
use pyreal_primitives::DataId;

use crate::lamps::{burns_on, decode, flame_colour, parts, script_emitters, Flicker, Lamp};

/// What burns, which sets a flame's shape and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlameKind {
    /// A small teardrop.
    Candle,
    /// A taller licking flame.
    Torch,
    /// A bed of fire, retail's billowing fire puffs: drawn as its particles' fire, not a card.
    Brazier,
    /// A creature of fire, its points following ([`crate::body_fire`]).
    Body,
    /// One point of a creature's fire: a cone of flame licking up from it.
    Point,
    /// A length of a creature's trail of fire, from its base to its base and run.
    Trail,
}

impl FlameKind {
    fn word(self) -> u32 {
        match self {
            Self::Candle => 1,
            Self::Torch => 2,
            Self::Brazier => 3,
            Self::Body => 4,
            Self::Point => 5,
            Self::Trail => 6,
        }
    }

    /// The flame's height, metres, at an object's own scale.
    fn height(self) -> f32 {
        match self {
            Self::Candle => 0.09,
            Self::Torch => 0.32,
            Self::Brazier => 0.75,
            Self::Body | Self::Point | Self::Trail => 0.0,
        }
    }

    /// Whether it is drawn as a flame card; a bed of fire is drawn as its puffs.
    #[must_use]
    pub fn carded(self) -> bool {
        self != Self::Brazier
    }
}

/// Objects whose fire their emitters do not tell: a chandelier's ring of candles.
pub const KINDS: &[(u32, FlameKind)] = &[(0x0200_0081, FlameKind::Candle)];

/// One flame of an object, in the object's own space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlameSource {
    pub kind: FlameKind,
    /// Where its base stands.
    pub offset: Vec3,
    /// The emitter picture it stands for, left out of the particles.
    pub sprite: u32,
}

/// A placed flame, render space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flame {
    pub base: Vec3,
    pub kind: FlameKind,
    /// Its height, metres.
    pub height: f32,
    /// The flicker it shares with its lamp: the lamp's seed and reach.
    pub seed: u32,
    pub reach: f32,
    /// The cell slot it stands in; zero outdoors.
    pub cell: u32,
    pub object: u32,
    /// A body's colour, linear bytes, count of points and whirl (quarter radians a second, to
    /// 63); a point's tip width (its bits), its seed its swing (its bits).
    pub colour: u32,
    pub count: u32,
    pub spin: u32,
}

/// The most flames a frame draws.
pub const MAX_FRAME_FLAMES: usize = 1024;
/// How far from the eye a flame is drawn, metres.
pub const FLAME_DISTANCE: f32 = 150.0;
/// A flame's bytes on the GPU.
pub const FLAME_SIZE: u64 = 32;

/// A stable seed for a lamp or flame at `p` (render space), 24 bits.
#[must_use]
pub fn seed_of(p: Vec3) -> u32 {
    #[allow(clippy::cast_possible_truncation)] // a hash of the metre's tenth
    let q = (p * 10.0).round().as_ivec3();
    let mut h = q.x.cast_unsigned().wrapping_mul(0x9E37_79B1)
        ^ q.y.cast_unsigned().wrapping_mul(0x85EB_CA77)
        ^ q.z.cast_unsigned().wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x27D4_EB2F);
    (h ^ (h >> 13)) & 0xFF_FFFF
}

/// The kind of fire an emitter of a fire-coloured sprite draws, by how its particles behave, or
/// `None` for a lamp's glow sprite (one particle that holds its size); `many` when its object
/// burns in several places.
#[must_use]
pub fn kind_of(e: &pyreal_assets::world::ParticleEmitterInfo, many: bool) -> Option<FlameKind> {
    if e.max_particles <= 2 && (e.start_scale - e.final_scale).abs() < 0.5 {
        return None;
    }
    if many {
        return Some(FlameKind::Candle);
    }
    // A bed of fire: many particles rising, or shrinking as they burn out.
    let rising = e.a.z > 0.05 && e.final_scale > e.start_scale && e.max_particles >= 18;
    let bed = e.start_scale > e.final_scale && e.max_particles >= 18;
    Some(if rising || bed {
        FlameKind::Brazier
    } else {
        FlameKind::Torch
    })
}

/// Where object `id` burns, from the dats.
#[must_use]
pub fn examine(store: &RetailDatStore, id: DataId) -> Vec<FlameSource> {
    if divine_type(id) != Some(DbType::Setup) {
        return Vec::new();
    }
    let Some(setup) = decode::<Setup>(store, DbType::Setup, id) else {
        return Vec::new();
    };
    let fiery: Vec<_> = script_emitters(store, &setup, &parts(&setup))
        .into_iter()
        .filter(|(e, _)| flame_colour(store, e).is_some())
        .filter(|(e, _)| kind_of(e, false).is_some())
        .collect();
    // A ring of candles counts its bursts too.
    let many = fiery.len() > 2;
    let emitters: Vec<_> = fiery.into_iter().filter(|(e, _)| burns_on(e)).collect();
    let table = KINDS.iter().find(|(k, _)| *k == id.0).map(|(_, k)| *k);
    let mut out: Vec<FlameSource> = Vec::new();
    for (e, at) in &emitters {
        let Some(kind) = table.or_else(|| kind_of(e, many)) else {
            continue;
        };
        let sprite = if e.hw_gfxobj_id.0 != 0 {
            e.hw_gfxobj_id.0
        } else {
            e.gfxobj_id.0
        };
        // Two emitters of one fire (its flames and its sparks) are one flame, the larger kind.
        if let Some(f) = out.iter_mut().find(|f| f.offset.distance(*at) < 0.4) {
            if kind.height() > f.kind.height() {
                f.kind = kind;
            }
            continue;
        }
        out.push(FlameSource {
            kind,
            offset: *at,
            sprite,
        });
    }
    out
}

/// `s` placed at `at` in cell slot `cell`, a part of object `object` whose lamps are `lamps`: it
/// flickers with the nearest of them that flickers.
#[must_use]
pub fn place(s: &FlameSource, at: &Affine3A, cell: u32, object: u32, lamps: &[Lamp]) -> Flame {
    let (scale, _, _) = at.to_scale_rotation_translation();
    let base = at.transform_point3(s.offset);
    let lamp = lamps
        .iter()
        .filter(|l| l.flicker == Flicker::Flame)
        .min_by(|a, b| {
            a.position
                .distance_squared(base)
                .total_cmp(&b.position.distance_squared(base))
        });
    Flame {
        base,
        kind: s.kind,
        height: s.kind.height() * scale.z,
        seed: lamp.map_or_else(|| seed_of(base), |l| seed_of(l.position)),
        reach: lamp.map_or(6.0, |l| l.range),
        cell,
        object,
        colour: 0,
        count: 0,
        spin: 0,
    }
}

/// The flames seen inside `planes` (their cells seen, as `seen` says), within `reach` of `eye`,
/// nearest first, at most [`MAX_FRAME_FLAMES`].
#[must_use]
pub fn frame_flames(
    flames: &[Flame],
    planes: &[Vec4],
    eye: Vec3,
    seen: impl Fn(u32) -> bool,
    reach: f32,
) -> Vec<Flame> {
    let mut near: Vec<(f32, Flame)> = flames
        .iter()
        .filter(|f| {
            let r = f.height * 1.5;
            f.base.distance(eye) < reach
                && planes
                    .iter()
                    .all(|p| p.truncate().dot(f.base + Vec3::Z * f.height * 0.5) + p.w >= -r)
                && seen(f.cell)
        })
        .map(|f| (f.base.distance_squared(eye), *f))
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    near.truncate(MAX_FRAME_FLAMES);
    near.into_iter().map(|(_, f)| f).collect()
}

/// A flame's bytes: its base and height; its kind, seed, reach and whether it flickers; a body's
/// count of points, whirl and colour; a point's tip width and swing (its reach is its foot's).
#[must_use]
pub fn flame_bytes(f: &Flame, flickers: bool) -> [u8; 32] {
    let words: [u32; 8] = [
        f.base.x.to_bits(),
        f.base.y.to_bits(),
        f.base.z.to_bits(),
        f.height.to_bits(),
        f.kind.word()
            | (u32::from(f.cell != 0) << 8)
            | (u32::from(flickers) << 9)
            | (f.spin.min(63) << 10)
            | (f.count.min(0xFFFF) << 16),
        f.seed,
        f.reach.to_bits(),
        f.colour,
    ];
    let mut b = [0u8; 32];
    for (k, w) in words.iter().enumerate() {
        b[k * 4..k * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    b
}

/// The placed flames, and each examined object id's.
#[derive(Debug)]
pub struct Flames {
    pub placed: Vec<Flame>,
    /// Whether flames are drawn (and the emitters they stand for left out).
    pub on: bool,
    // ORDER-OK: looked up by object id, never iterated.
    sources: HashMap<u32, Vec<FlameSource>>,
}

impl Default for Flames {
    fn default() -> Self {
        Self {
            placed: Vec::new(),
            on: true,
            sources: HashMap::new(),
        }
    }
}

impl Flames {
    /// The flames of object `id`, examined the first time.
    pub fn sources(&mut self, store: &RetailDatStore, id: DataId) -> &[FlameSource] {
        self.sources
            .entry(id.0)
            .or_insert_with(|| examine(store, id))
    }
}
