//! Creatures made of fire drawn as fire: the Flamma and its kin, fire wisps, pillars of fire.
//!
//! A body's fire is its emitters that draw a flame picture ([`fire_picture`], their role
//! [`crate::roles::Role::Fire`]): retail's flame puff, in any colour (Hellfire's and Firestorm's
//! are cyan, Fiery Remains' blue). Each frame they become the points its fire stands on
//! ([`gather`]), riding its parts: a cone of flame licking up from each, as wide as its particles
//! spread and as high as they rise. Their particles are left out; its other emitters (a spell's,
//! an acid cloud's round a burning heart) are drawn as their roles say.
//!
//! `body_fire.wgsl` draws a body as one volume: the cones merged, torn by noise rising through
//! them, white-hot at the heart to the picture's colour at the edges, bright enough to bloom.
//! It lights what stands round it with one flickering lamp of its colour; when many gather, the
//! nearest only ([`frame`]).
//!
//! A wreath's fire ([`wreath`]), fire puffs swarming round an object, is placed once with it: a
//! ring of flames round it, whirling as its puffs go round.
//!
//! A hearth's fire ([`hearth`]) is drawn the same way, placed once with its object: two rows of
//! flames across its coals, sparks thrown up from them, flickering with its lamp. So is a
//! forge's ([`forge`]), its rows shaped to its firebox, and a fire pit's ([`pit`]).

use glam::{Affine3A, Vec3, Vec4};
use pyreal_animation::data::{ParticleEmitterInfo, ParticleType};
use pyreal_animation::MotionDriver;
use pyreal_assets::{GfxObj, Surface};
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::DataId;

use crate::asset::texture::{PixelLayout, TextureSource};
use crate::flames::{Flame, FlameKind};
use crate::lamps::{decode, Flicker, Lamp, LampKind, LampSource};
use pyreal_primitives::num::math;

/// Retail's flame puff (the Flamma's), whose shape every flame picture shares.
const FLAME_PUFF: u32 = 0x0500_171A;
/// Retail's torch flame, whose shape its torches' flames of other colours share.
const TORCH_FLAME: u32 = 0x0500_16D5;
/// How closely a picture's light must follow the puff's, and how bright and coloured it must be
/// (sRGB, its mean's brightest channel; saturation), to be fire.
const LIKENESS: f32 = 0.9;
const BRIGHTEST: u8 = 35;
const SATURATION: f32 = 0.3;

/// Default scripts whose fire is a molten body's heat, drawn by its glowing cracks instead: the
/// Magma Golem's (0x020007CB).
const MOLTEN_CORES: [u32; 1] = [0x3300_0866];

/// Whether `driver`'s fire is left out for its molten body's glow ([`MOLTEN_CORES`]).
#[must_use]
pub fn molten_core(driver: &MotionDriver) -> bool {
    driver
        .part_array
        .setup
        .as_ref()
        .and_then(|s| s.default_script)
        .is_some_and(|s| MOLTEN_CORES.contains(&s.0))
}

/// The most bodies of fire drawn, the most points one stands on, and the most that light.
pub const MAX_BODIES: usize = 24;
pub const MAX_POINTS: usize = 40;
pub const MAX_LAMPS: usize = 8;
/// How far from the eye a body of fire is drawn, metres.
pub const DISTANCE: f32 = 200.0;

/// One point a body's fire stands on, render space: a cone of flame `height` high licking up
/// from `at`, `radius` wide at its foot and `tip` at its top, swinging `swirl` metres round its
/// axis as it rises; `part` the body part it rides. A trail's ([`TRAIL`]) is a length of flame
/// `radius` wide from `at` along `run`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FirePoint {
    pub at: Vec3,
    pub radius: f32,
    pub height: f32,
    pub tip: f32,
    pub swirl: f32,
    pub part: u32,
    pub run: Vec3,
}

/// The part of a trail's point: it merges with nothing.
pub const TRAIL: u32 = u32::MAX - 1;
/// About how many of an emitter's world-fixed particles a trail keeps, and the fewest it needs.
const TRAIL_KEPT: i32 = 10;
const TRAIL_STREAM: i32 = 8;

/// A body's fire this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct BodyFire {
    pub points: Vec<FirePoint>,
    /// Linear, brightest channel one.
    pub colour: Vec3,
    pub cell: u32,
    pub seed: u32,
    /// Its flames' mean width at their feet, from its emitters, metres: the noise's scale.
    pub size: f32,
    /// How fast it whirls, radians a second (a pillar's swarm), nought for none.
    pub spin: f32,
    /// The object it belongs to; nought for a creature's, which is gathered each frame.
    pub object: u32,
    /// A hearth's ([`hearth`]): the reach of the lamp it flickers with, which stands for its
    /// light, so it brings none of its own.
    pub hearth: Option<f32>,
}

/// A wreath's points round its ring, which hold its fire and throw its sparks; how far under its
/// emitter its band of fire stands, how wide it is and how high it licks, against its puffs'
/// width; and how fast it is seen to whirl against its puffs.
const WREATH_POINTS: usize = 8;
const WREATH_FOOT: f32 = 0.45;
const WREATH_WIDTH: f32 = 0.55;
const WREATH_HEIGHT: f32 = 1.1;
const WREATH_SPIN: f32 = 0.5;

/// A wreath's fire ([`FlameKind::Wreath`]): its ring `s` placed at `at` in cell slot `cell` by
/// `object`, one band of flame round the ring (`body_fire.wgsl` draws it from the points standing
/// on it) that whirls round its object as its puffs do, flickering on `seed`.
#[must_use]
pub fn wreath(
    s: &crate::flames::FlameSource,
    at: &Affine3A,
    cell: u32,
    object: u32,
    seed: u32,
) -> BodyFire {
    let (scale, _, _) = at.to_scale_rotation_translation();
    let (ring, puff, rate) = (s.bed.x, s.bed.y, s.bed.z);
    let radius = puff * WREATH_WIDTH;
    let foot = s.offset - Vec3::Z * puff * WREATH_FOOT;
    let points = (0..WREATH_POINTS)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)] // a handful of points
            let a = std::f32::consts::TAU * k as f32 / WREATH_POINTS as f32;
            FirePoint {
                at: at.transform_point3(foot + Vec3::new(math::cosf(a), math::sinf(a), 0.0) * ring),
                radius: radius * scale.x,
                height: puff * WREATH_HEIGHT * scale.z,
                tip: radius * 0.15 * scale.x,
                swirl: 0.0,
                part: 0,
                run: Vec3::ZERO,
            }
        })
        .collect();
    BodyFire {
        points,
        colour: s.tint / s.tint.max_element().max(1e-6),
        cell,
        seed,
        size: radius * scale.x,
        spin: rate * WREATH_SPIN,
        object,
        hearth: None,
    }
}

/// A hearth's flames across the front of its bed and the back, and how much lower its end ones
/// lick than its middle ones.
const HEARTH_FRONT: usize = 5;
const HEARTH_BACK: usize = 4;
const HEARTH_CROWN: f32 = 0.6;

/// A hearth's fire ([`FlameKind::Hearth`]): its bed `s` placed at `at` in cell slot `cell` by
/// `object`, two rows of flames across its coals licking up into one body of fire, the back
/// row taller, flickering with its lamp (`seed` and `reach`).
#[must_use]
pub fn hearth(
    s: &crate::flames::FlameSource,
    at: &Affine3A,
    cell: u32,
    object: u32,
    seed: u32,
    reach: f32,
) -> BodyFire {
    let (scale, _, _) = at.to_scale_rotation_translation();
    let (spread, drop, height) = (s.bed.x, s.bed.y, s.bed.z);
    let radius = spread * 0.3;
    // Its feet sunk in the coals, which hide their rounded ends.
    let foot = s.offset - Vec3::Z * (drop + radius * 0.25);
    let mut points = Vec::with_capacity(HEARTH_FRONT + HEARTH_BACK);
    let mut jitter = seed;
    for (row, n, y, tall) in [(0, HEARTH_FRONT, 0.0, 0.95), (1, HEARTH_BACK, 0.3, 1.15)] {
        #[allow(clippy::cast_precision_loss)] // a handful of flames
        let gap = 2.0 * spread * 0.8 / (n - 1) as f32;
        for k in 0..n {
            #[allow(clippy::cast_precision_loss)]
            let x = -spread * 0.8 + gap * k as f32;
            let middle = 1.0 - (x / spread).abs();
            jitter = jitter.wrapping_mul(0x2C1B_3C6D).wrapping_add(0x297A_2D39);
            #[allow(clippy::cast_precision_loss)] // eight bits
            let j = ((jitter >> 16) & 255) as f32 / 255.0 - 0.5;
            points.push(FirePoint {
                at: at.transform_point3(foot + Vec3::new(x, y * spread, 0.0)),
                radius: radius * scale.x * if row == 0 { 1.0 } else { 0.8 },
                height: height * tall * (1.0 - HEARTH_CROWN * (1.0 - middle) + 0.25 * j) * scale.z,
                tip: radius * 0.15 * scale.x,
                swirl: 0.0,
                part: 0,
                run: Vec3::ZERO,
            });
        }
    }
    BodyFire {
        points,
        colour: s.tint / s.tint.max_element().max(1e-6),
        cell,
        seed,
        size: radius * scale.x,
        spin: 0.0,
        object,
        hearth: Some(reach),
    }
}

/// A forge's flames: rows across its coals, each this far from its mouth to its back and of this
/// many; how far in from its sides they stand, how wide against the gap between them (at most
/// [`FORGE_FATTEST`] metres), and how high against its roof (which hides their tips).
const FORGE_ROWS: [(f32, usize); 3] = [(0.3, 5), (0.55, 4), (0.8, 3)];
const FORGE_SIDES: f32 = 0.8;
const FORGE_WIDTH: f32 = 0.75;
const FORGE_FATTEST: f32 = 0.24;
const FORGE_ROOF: f32 = 1.6;

/// A forge's fire ([`crate::flames::forge_of`]): bed of fire `s` burning in forge `f` placed at
/// `at`, in cell slot `cell` by `object`. Rows of flames across its firebox's coals, as wide as it
/// is there and licking up nearly to its roof, so tallest at its mouth and in its middle, drawn
/// as a hearth's and flickering with its lamp (`seed` and `reach`).
#[must_use]
pub fn forge(
    s: &crate::flames::FlameSource,
    f: &crate::flames::Forge,
    at: &Affine3A,
    cell: u32,
    object: u32,
    seed: u32,
    reach: f32,
) -> BodyFire {
    let (scale, _, _) = at.to_scale_rotation_translation();
    let aside = (f.mouth - f.back)
        .truncate()
        .perp()
        .normalize_or_zero()
        .extend(0.0);
    let mut points = Vec::new();
    let mut jitter = seed;
    let mut sizes = 0.0;
    for (t, n) in FORGE_ROWS {
        let middle = f.mouth.lerp(f.back, t);
        let half = (f.mouth_half + (f.back_half - f.mouth_half) * t) * FORGE_SIDES;
        let roof = f.mouth_roof + (f.back_roof - f.mouth_roof) * t;
        #[allow(clippy::cast_precision_loss)] // a handful of flames
        let gap = 2.0 * half / (n - 1) as f32;
        let radius = (gap * FORGE_WIDTH).min(FORGE_FATTEST);
        for k in 0..n {
            #[allow(clippy::cast_precision_loss)]
            let x = -half + gap * k as f32;
            let edge = (x / half).abs();
            jitter = jitter.wrapping_mul(0x2C1B_3C6D).wrapping_add(0x297A_2D39);
            #[allow(clippy::cast_precision_loss)] // eight bits
            let j = ((jitter >> 16) & 255) as f32 / 255.0 - 0.5;
            // Its feet sunk in the coals, which hide their rounded ends.
            let foot = middle + aside * x - Vec3::Z * radius * 0.25;
            points.push(FirePoint {
                at: at.transform_point3(foot),
                radius: radius * scale.x,
                height: roof * FORGE_ROOF * (1.0 - 0.45 * edge * edge) * (1.0 + 0.2 * j) * scale.z,
                tip: radius * 0.15 * scale.x,
                swirl: 0.0,
                part: 0,
                run: Vec3::ZERO,
            });
            sizes += radius * scale.x;
        }
    }
    #[allow(clippy::cast_precision_loss)] // a handful of flames
    let size = sizes / points.len().max(1) as f32;
    BodyFire {
        points,
        colour: s.tint / s.tint.max_element().max(1e-6),
        cell,
        seed,
        size,
        spin: 0.0,
        object,
        hearth: Some(reach),
    }
}

/// A fire pit's flames: one in the middle of its coals and a ring round it, this far out against
/// its coals' spread; how wide each is against that spread, and how high the middle one and the
/// ring lick, metres.
const PIT_RING: usize = 6;
const PIT_OUT: f32 = 0.75;
const PIT_MIDDLE_WIDTH: f32 = 0.55;
const PIT_RING_WIDTH: f32 = 0.45;
const PIT_MIDDLE_HEIGHT: f32 = 0.8;
const PIT_RING_HEIGHT: f32 = 0.55;

/// A fire pit's fire ([`crate::flames::firebox_of`]): bed of fire `s` burning on pit `p`'s coals,
/// placed at `at` in cell slot `cell` by `object`. A flame in the middle of its coals and a ring
/// round it, the middle tallest, licking up round what hangs over it (a cauldron hides their
/// tips), drawn as a hearth's and flickering with its lamp (`seed` and `reach`).
#[must_use]
pub fn pit(
    s: &crate::flames::FlameSource,
    p: &crate::flames::Pit,
    at: &Affine3A,
    cell: u32,
    object: u32,
    seed: u32,
    reach: f32,
) -> BodyFire {
    let (scale, _, _) = at.to_scale_rotation_translation();
    let mut points = Vec::with_capacity(PIT_RING + 1);
    let mut jitter = seed;
    for k in 0..=PIT_RING {
        jitter = jitter.wrapping_mul(0x2C1B_3C6D).wrapping_add(0x297A_2D39);
        #[allow(clippy::cast_precision_loss)] // eight bits
        let j = ((jitter >> 16) & 255) as f32 / 255.0 - 0.5;
        let (out, width, tall) = if k == 0 {
            (Vec3::ZERO, PIT_MIDDLE_WIDTH, PIT_MIDDLE_HEIGHT)
        } else {
            #[allow(clippy::cast_precision_loss)] // a handful of flames
            let a = (k as f32 + 0.3 * j) * std::f32::consts::TAU / PIT_RING as f32;
            let (sin, cos) = pyreal_primitives::num::math::sin_cosf(a);
            (
                Vec3::new(cos, sin, 0.0) * p.radius * PIT_OUT,
                PIT_RING_WIDTH,
                PIT_RING_HEIGHT,
            )
        };
        let radius = p.radius * width;
        // Its feet sunk in the coals, which hide their rounded ends.
        let foot = p.coals + out - Vec3::Z * radius * 0.25;
        points.push(FirePoint {
            at: at.transform_point3(foot),
            radius: radius * scale.x,
            height: tall * (1.0 + 0.25 * j) * scale.z,
            tip: radius * 0.15 * scale.x,
            swirl: 0.0,
            part: 0,
            run: Vec3::ZERO,
        });
    }
    BodyFire {
        points,
        colour: s.tint / s.tint.max_element().max(1e-6),
        cell,
        seed,
        size: p.radius * (PIT_MIDDLE_WIDTH + PIT_RING_WIDTH * 2.0) / 3.0 * scale.x,
        spin: 0.0,
        object,
        hearth: Some(reach),
    }
}

/// A hearth's own light on its coals: how bright against its lamp's, how far over its coals,
/// how far it reaches (metres) and how warm (kelvin).
const HEARTH_GLOW: f32 = 0.3;
const HEARTH_GLOW_HEIGHT: f32 = 0.3;
const HEARTH_GLOW_FALLOFF: f32 = 3.0;
const HEARTH_GLOW_KELVIN: f32 = 1800.0;

/// The light a hearth's fire throws from its coals (`s`, placed at `at` in cell slot `cell` by
/// `object`), as warm as firelight, onto its firebox, its hearthstones and the floor before it: a
/// share of its lamp's (`seed`, `intensity`), which lights the room, reaching a few metres and
/// flickering on the lamp's seed, quicker than it as a smaller fire's light does.
#[must_use]
pub fn hearth_lamp(
    s: &crate::flames::FlameSource,
    at: &Affine3A,
    cell: u32,
    object: u32,
    (seed, _, intensity): (u32, f32, f32),
) -> Lamp {
    coal_glow(
        s.offset + Vec3::Z * (HEARTH_GLOW_HEIGHT - s.bed.y),
        intensity * HEARTH_GLOW,
        at,
        (cell, object, seed),
    )
}

/// How bright a forge's or a pit's coals' light is (as the academy hearth's), and where over a
/// forge's it stands:
/// this far from its mouth to its back and this high, deep in its firebox, so the brick round its
/// mouth faces away from it (retail's sum of a room's lights wraps round a face and casts no
/// shadow).
const FORGE_GLOW: f32 = 30.0;
const FORGE_GLOW_DEPTH: f32 = 0.8;
const FORGE_GLOW_HEIGHT: f32 = 0.15;

/// The light a forge's fire throws from its coals, forge `f` placed at `at` in cell slot `cell`
/// by `object`: on its firebox and out of its mouth onto the floor before it, flickering with its
/// fire's lamp (`seed`).
#[must_use]
pub fn forge_lamp(
    f: &crate::flames::Forge,
    at: &Affine3A,
    cell: u32,
    object: u32,
    seed: u32,
) -> Lamp {
    coal_glow(
        f.mouth.lerp(f.back, FORGE_GLOW_DEPTH) + Vec3::Z * FORGE_GLOW_HEIGHT,
        FORGE_GLOW,
        at,
        (cell, object, seed),
    )
}

/// The light a fire pit's fire throws from its coals, pit `p` placed at `at` in cell slot `cell`
/// by `object`: on its stones and the floor round it, flickering with its fire's lamp (`seed`).
#[must_use]
pub fn pit_lamp(p: &crate::flames::Pit, at: &Affine3A, cell: u32, object: u32, seed: u32) -> Lamp {
    coal_glow(
        p.coals + Vec3::Z * HEARTH_GLOW_HEIGHT,
        FORGE_GLOW,
        at,
        (cell, object, seed),
    )
}

/// A fire's body and its coals' light in firebox `b` (placed at `at`): [`forge`] and
/// [`forge_lamp`], or [`pit`] and [`pit_lamp`].
#[must_use]
pub fn in_firebox(
    s: &crate::flames::FlameSource,
    b: crate::flames::Firebox,
    at: &Affine3A,
    cell: u32,
    object: u32,
    (seed, reach): (u32, f32),
) -> (BodyFire, Lamp) {
    match b {
        crate::flames::Firebox::Forge(f) => (
            forge(s, f, at, cell, object, seed, reach),
            forge_lamp(f, at, cell, object, seed),
        ),
        crate::flames::Firebox::Pit(p) => (
            pit(s, p, at, cell, object, seed, reach),
            pit_lamp(p, at, cell, object, seed),
        ),
    }
}

/// A fire's light on its coals at `offset` (its object's space, placed at `at`), `intensity`
/// strong, as warm as firelight and reaching a few metres, flickering on `seed`.
fn coal_glow(
    offset: Vec3,
    intensity: f32,
    at: &Affine3A,
    (cell, object, seed): (u32, u32, u32),
) -> Lamp {
    let source = LampSource {
        client_offset: None,
        kind: LampKind::Flame,
        offset,
        colour: crate::lamps::kelvin(HEARTH_GLOW_KELVIN),
        intensity,
        falloff: HEARTH_GLOW_FALLOFF,
        flicker: Flicker::Flame,
        shadows: true,
    };
    let mut lamp = crate::lamps::place(&source, at, cell, object);
    lamp.seed = seed;
    lamp
}

/// A picture's texels' light, less its mean, of length one: the shape of its brightness.
fn shape_of(src: &TextureSource<'_>, id: DataId) -> Option<(Vec<f32>, [u8; 3])> {
    let px = src.decode(id, false, None).ok()?;
    if px.layout != PixelLayout::Bgra8 {
        return None;
    }
    let level = px.levels.first()?;
    let (mut sum, mut light) = ([0u64; 3], Vec::with_capacity(level.len() / 4));
    for c in level.as_chunks::<4>().0 {
        sum[0] += u64::from(c[2]);
        sum[1] += u64::from(c[1]);
        sum[2] += u64::from(c[0]);
        light.push(f32::from(c[2]) * 0.21 + f32::from(c[1]) * 0.72 + f32::from(c[0]) * 0.07);
    }
    let n = light.len().max(1);
    #[allow(clippy::cast_precision_loss)] // a picture's texel count
    let mean = light.iter().sum::<f32>() / n as f32;
    let norm = light
        .iter()
        .map(|l| (l - mean) * (l - mean))
        .sum::<f32>()
        .sqrt()
        .max(1e-6);
    let shape = light.iter().map(|l| (l - mean) / norm).collect();
    let mean = sum.map(|s| u8::try_from(s / n as u64).unwrap_or(u8::MAX));
    Some((shape, mean))
}

/// The colour of picture `gid` if it is a flame: additive, and retail's flame puff recoloured,
/// bright and of a colour.
#[must_use]
pub fn fire_picture(store: &RetailDatStore, gid: DataId) -> Option<Vec3> {
    recoloured(store, gid, FLAME_PUFF)
}

/// The colour of picture `gid` if it is a torch's flame: additive, and retail's torch flame
/// recoloured, bright and of a colour.
#[must_use]
pub fn torch_picture(store: &RetailDatStore, gid: DataId) -> Option<Vec3> {
    recoloured(store, gid, TORCH_FLAME)
}

/// The colour of picture `gid` if it is picture `model` recoloured: additive, bright and of a
/// colour.
fn recoloured(store: &RetailDatStore, gid: DataId, model: u32) -> Option<Vec3> {
    let gfx: GfxObj = decode(store, DbType::GfxObj, gid)?;
    let src = TextureSource::new(store);
    let (puff, _) = shape_of(&src, DataId(model))?;
    let mut colour = Vec3::ZERO;
    for s in &gfx.surfaces {
        let surf: Surface = decode(store, DbType::Surface, *s)?;
        if surf.surface_type & 0x1_0000 == 0 {
            return None;
        }
        let (shape, mean) = shape_of(&src, surf.orig_texture_id?)?;
        let like: f32 = shape.iter().zip(&puff).map(|(a, b)| a * b).sum();
        let hi = mean.iter().copied().max().unwrap_or(0);
        let lo = mean.iter().copied().min().unwrap_or(0);
        if shape.len() != puff.len()
            || like < LIKENESS
            || hi < BRIGHTEST
            || f32::from(hi - lo) < SATURATION * f32::from(hi)
        {
            return None;
        }
        let f = crate::asset::material::srgb_to_linear;
        colour += Vec3::new(f(mean[0]), f(mean[1]), f(mean[2]));
    }
    (colour.max_element() > 0.0).then(|| colour / colour.max_element())
}

fn v3(v: pyreal_primitives::Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

/// A swarm emitter's whirl: how fast its particles swing round, radians a second, and how far.
fn whirl(e: &ParticleEmitterInfo) -> (f32, f32) {
    if e.particle_type != ParticleType::Swarm {
        return (0.0, 0.0);
    }
    let mean = |lo: f32, hi: f32| (lo + hi) * 0.5;
    let b = v3(e.b) * mean(e.min_b, e.max_b);
    let c = v3(e.c) * mean(e.min_c, e.max_c);
    (b.x.abs().max(b.y.abs()), c.x.abs().max(c.y.abs()))
}

/// How wide an emitter's fire is at its foot and how high it licks, metres: as wide as its
/// particles and their spread halfway through their lives, as high as they rise; and how wide at
/// its top, as much as its particles still show at their end (a pillar's do).
fn cone(e: &ParticleEmitterInfo) -> (f32, f32, f32) {
    #[allow(clippy::cast_possible_truncation)] // seconds of a particle's life
    let life = e.lifespan as f32;
    let mean = |lo: f32, hi: f32| (lo + hi) * 0.5;
    let a = v3(e.a) * mean(e.min_a, e.max_a);
    let b = v3(e.b) * mean(e.min_b, e.max_b);
    let c = v3(e.c) * mean(e.min_c, e.max_c);
    // How far a particle has spread and risen at a share of its life.
    let moved = |s: f32| {
        let t = life * s;
        match e.particle_type {
            ParticleType::Unknown | ParticleType::Still => (0.0, 0.0),
            ParticleType::LocalVelocity | ParticleType::GlobalVelocity => (0.0, (a * t).length()),
            ParticleType::Swarm => (c.truncate().length(), a.z.abs() * t),
            ParticleType::Explode => (c.length() * a.x * t, (b.z * t + a.z) * t),
            ParticleType::Implode => (c.length(), 0.0),
            _ => (0.0, (a * t + b * 0.5 * t * t).length()),
        }
    };
    let (spread, _) = moved(0.5);
    let (_, rise) = moved(0.8);
    // Its particles' size, nearer the larger end of their lives.
    let size =
        (0.3 * e.start_scale.min(e.final_scale) + 0.7 * e.start_scale.max(e.final_scale)).max(0.05);
    let radius = 0.3 * size + 0.5 * spread + 0.5 * e.max_offset;
    let height = (rise.abs() + 0.4 * e.start_scale.max(e.final_scale)).max(radius * 1.6);
    let shows = (1.0 - e.final_trans) * (e.final_scale / e.start_scale.max(1e-3)).min(1.0);
    (radius, height, radius * shows.clamp(0.12, 0.9))
}

/// A stable seed for a body: where its driver lives, which holds while it does.
fn seed_of(driver: &MotionDriver) -> u32 {
    let mut h = std::ptr::from_ref(driver) as usize as u64;
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 29;
    u32::try_from(h & 0xFF_FFFF).unwrap_or(1) | 1
}

/// The fire of `driver`'s emitters that burn (`burning`, each with its flame's colour), moved by
/// `offset` into render space and seen through cell slot `cell`; none when none burns.
#[must_use]
pub fn gather<'a>(
    driver: &MotionDriver,
    burning: impl Iterator<Item = (&'a pyreal_animation::particles::ParticleEmitter, Vec3)>,
    offset: Vec3,
    cell: u32,
) -> Option<BodyFire> {
    let mut colour = Vec3::ZERO;
    let mut points: Vec<FirePoint> = Vec::new();
    let mut trail: Vec<FirePoint> = Vec::new();
    let (mut size, mut spin, mut cones) = (0.0, 0.0f32, 0.0);
    for (e, c) in burning {
        colour += c;
        let (radius, height, tip) = cone(&e.info);
        let (rate, swing) = whirl(&e.info);
        size += radius;
        cones += 1.0;
        spin = spin.max(rate);
        if e.degraded_out || (e.stopped && e.live().next().is_none()) {
            continue;
        }
        // Its foot rests where the emitter is, rather than half sunk below it.
        let from = v3(e.object_origin) + offset;
        let point = FirePoint {
            at: from + Vec3::Z * radius * 0.5,
            radius,
            height,
            tip,
            swirl: swing * 1.5,
            part: e.part_index,
            run: Vec3::ZERO,
        };
        if let Some(p) = points.iter_mut().find(|p| p.joins(&point)) {
            p.merge(&point);
        } else if points.len() < MAX_POINTS {
            points.push(point);
        }
        // A stream of world-fixed particles draws where the body has been.
        if e.info.is_parent_local == 0 && e.info.max_particles >= TRAIL_STREAM {
            trail_of(e, from, radius, &mut trail);
        }
    }
    let room = MAX_POINTS.saturating_sub(points.len());
    points.extend(trail.into_iter().take(room));
    (!points.is_empty()).then(|| BodyFire {
        points,
        colour: colour / colour.max_element().max(1e-6),
        cell,
        seed: seed_of(driver),
        size: (size / f32::max(cones, 1.0)).max(0.02),
        // Seen whirling at a third of its particles' swing.
        spin: spin * 0.35,
        object: 0,
        hearth: None,
    })
}

/// The trail a moving body leaves of emitter `e`'s world-fixed particles: a fixed share of them,
/// chosen at birth, joined youngest to oldest by lengths of flame from the emitter (at `from`,
/// its flame `radius` wide), each as wide as its older end's particle, so the trail tapers as they
/// shrink, swelling in and dying away; none while the trail is still within the emitter's flame
/// seen from above.
fn trail_of(
    e: &pyreal_animation::particles::ParticleEmitter,
    from: Vec3,
    radius: f32,
    out: &mut Vec<FirePoint>,
) {
    let stride = (e.info.max_particles / TRAIL_KEPT).max(1);
    let biggest = e.info.start_scale.max(e.info.final_scale);
    let shift = from - v3(e.object_origin);
    let mut kept: Vec<(f64, Vec3, f32)> = e
        .live()
        .filter(|p| crate::particles::seed_of(p).is_multiple_of(stride.unsigned_abs()))
        .map(|p| {
            #[allow(clippy::cast_possible_truncation)] // a share of a particle's life
            let age = (p.lifetime / p.lifespan.max(1e-3)).clamp(0.0, 1.0) as f32;
            let swell = smooth(0.0, 0.15, age) * (1.0 - smooth(0.88, 1.0, age));
            let o = p.frame.origin;
            // Held to a fifth of its largest, so the tail's tip stays a flame.
            let r = 0.25 * p.scale.max(0.2 * biggest) * swell;
            (p.lifetime, Vec3::new(o.x, o.y, o.z) + shift, r)
        })
        .collect();
    kept.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut last = from + Vec3::Z * radius * 0.5;
    for (_, at, r) in kept {
        // Left behind across the ground, not risen over the emitter in place.
        let r = r * smooth(0.5 * radius, 2.0 * radius, (at - from).truncate().length());
        if r > 0.02 {
            out.push(FirePoint {
                at: last,
                radius: r,
                height: 0.0,
                tip: 0.0,
                swirl: 0.0,
                part: TRAIL,
                run: at - last,
            });
        }
        last = at;
    }
}

fn smooth(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl FirePoint {
    /// Whether `o` burns in the same column: on the same part (so they join or not as the body
    /// moves), as wide, over or under it, their flames meeting.
    fn joins(&self, o: &Self) -> bool {
        let (thin, wide) = (self.radius.min(o.radius), self.radius.max(o.radius));
        self.part == o.part
            && self.part != TRAIL
            && wide < 1.5 * thin
            && (self.at - o.at).truncate().length() < 0.6 * thin
            && o.at.z < self.at.z + self.height
            && self.at.z < o.at.z + o.height
    }

    /// One column of both: from the lower foot to the higher tip, the wider.
    fn merge(&mut self, o: &Self) {
        let top = (self.at.z + self.height).max(o.at.z + o.height);
        if o.at.z < self.at.z {
            self.at = o.at;
        }
        self.height = top - self.at.z;
        self.radius = self.radius.max(o.radius);
        self.tip = self.tip.max(o.tip);
        self.swirl = self.swirl.max(o.swirl);
    }
}

impl BodyFire {
    /// The ball holding its fire and the flames' noise round it: centre and radius.
    #[must_use]
    pub fn bounds(&self) -> (Vec3, f32) {
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let mut wide = 0.0;
        for p in &self.points {
            let swing = Vec3::new(p.swirl, p.swirl, 0.0);
            let end = p.at + p.run;
            lo = lo.min(p.at.min(end) - Vec3::new(p.radius, p.radius, p.radius * 0.6) - swing);
            hi = hi.max(
                p.at.max(end) + Vec3::new(p.radius, p.radius, p.height + p.radius * 0.6) + swing,
            );
            wide += p.radius;
        }
        // The noise tears its flames out by up to twice their mean width.
        #[allow(clippy::cast_precision_loss)] // a few dozen points
        let tear = 2.0 * wide / self.points.len().max(1) as f32;
        ((lo + hi) * 0.5, (hi - lo).length() * 0.5 + tear)
    }

    /// Its light's place: where its fire burns hottest (the points' mean, by their size, a third
    /// of the way up their flames), held low enough over its lowest flame to light the ground.
    fn heart(&self, r: f32) -> Vec3 {
        let (mut at, mut w, mut foot) = (Vec3::ZERO, 0.0, f32::MAX);
        for p in self.points.iter().filter(|p| p.part != TRAIL) {
            let k = p.radius * p.radius * p.height;
            at += (p.at + Vec3::Z * p.height * 0.33) * k;
            w += k;
            foot = foot.min(p.at.z);
        }
        let mut at = at / w.max(1e-6);
        at.z = at.z.min(foot + (0.25 * r).max(0.7));
        at
    }
}

/// Linear `c` (brightest channel one) as bytes, red lowest.
pub(crate) fn packed(c: Vec3) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 to 255
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    b(c.x) | b(c.y) << 8 | b(c.z) << 16 | 1 << 24
}

/// The bodies of fire seen inside `planes` (their cells seen, as `seen` says) within
/// [`DISTANCE`] of `eye`, nearest first, as flame records taking at most `room` slots (each body
/// followed by its points); and the lamps of the [`MAX_LAMPS`] nearest.
#[must_use]
pub fn frame(
    fires: &[BodyFire],
    planes: &[Vec4],
    eye: Vec3,
    seen: impl Fn(u32) -> bool,
    room: usize,
) -> (Vec<Flame>, Vec<Lamp>) {
    let mut near: Vec<(f32, &BodyFire, Vec3, f32)> = fires
        .iter()
        .filter_map(|f| {
            let (c, r) = f.bounds();
            let d = c.distance(eye);
            (d < DISTANCE + r
                && seen(f.cell)
                && planes.iter().all(|p| p.truncate().dot(c) + p.w >= -r))
            .then_some((d, f, c, r))
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (mut flames, mut lamps) = (Vec::new(), Vec::new());
    for &(_, f, centre, r) in near.iter().take(MAX_BODIES) {
        if flames.len() + 1 + f.points.len() > room {
            break;
        }
        let record = |base: Vec3, height: f32, kind: FlameKind, reach: f32| Flame {
            base,
            kind,
            height,
            seed: f.seed,
            reach,
            cell: f.cell,
            object: 0,
            colour: packed(f.colour),
            count: 0,
            spin: 0,
        };
        // A creature's is gathered each frame; a wreath's is placed with its object.
        let kind = if f.hearth.is_some() {
            FlameKind::Hearth
        } else if f.object != 0 {
            FlameKind::Wreath
        } else {
            FlameKind::Body
        };
        let mut body = record(centre, r, kind, f.size);
        body.count = u32::try_from(f.points.len()).unwrap_or(0);
        // Quarter radians, 0 to 63; a hearth's, which never whirls, its lamp's reach in metres.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let spin = (f.hearth.unwrap_or(f.spin * 4.0)).round().clamp(0.0, 63.0) as u32;
        body.spin = spin;
        flames.push(body);
        flames.extend(f.points.iter().map(|p| {
            if p.part == TRAIL {
                // A trail's run in its height, colour and seed.
                let mut length = record(p.at, p.run.x, FlameKind::Trail, p.radius);
                length.colour = p.run.y.to_bits();
                length.seed = p.run.z.to_bits();
                return length;
            }
            let mut point = record(p.at, p.height, FlameKind::Point, p.radius);
            point.colour = p.tip.to_bits();
            point.seed = p.swirl.to_bits();
            point
        }));
        if lamps.len() < MAX_LAMPS && f.hearth.is_none() {
            // A blue fire lights deeper blue than it burns, so its light on grass is not green.
            let colour = if f.colour.z > f.colour.x {
                let c = f.colour.powf(5.0);
                c / c.max_element().max(1e-6)
            } else {
                f.colour
            };
            let source = LampSource {
                client_offset: None,
                kind: LampKind::Flame,
                offset: f.heart(r),
                colour,
                intensity: (3.0 + 3.0 * r).clamp(4.0, 12.0),
                falloff: (4.0 + 3.0 * r).clamp(4.0, 30.0),
                flicker: Flicker::Lively,
                shadows: false,
            };
            let mut lamp = crate::lamps::place(&source, &Affine3A::IDENTITY, f.cell, 0);
            lamp.seed = f.seed;
            lamps.push(lamp);
        }
    }
    (flames, lamps)
}
