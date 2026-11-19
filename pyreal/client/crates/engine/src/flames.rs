//! Flames drawn in code where an object burns: torches, sconces, candles; braziers' and pyres'
//! beds of fire drawn as fire puffs; a hearth's as a body of fire on its coals.
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
//! ([`crate::particles::Particles::puffed`]). A bed of fire in a hearth ([`FlameKind::Hearth`],
//! told by its setup's stonework round it) is drawn as a body of fire on its coals
//! ([`crate::body_fire::hearth`], [`Flames::hearths`]) while bodies of fire are, its puffs left out.
//! Fire puffs swarming round an object in a ring ([`FlameKind::Wreath`], Redspire's spire) are
//! drawn as a body of fire whirling round it ([`crate::body_fire::wreath`]) the same way.

use std::collections::HashMap;

use glam::{Affine3A, Vec3, Vec4};
use pyreal_assets::Setup;
use pyreal_dat::{divine_type, DbType, RetailDatStore};
use pyreal_primitives::DataId;

use crate::lamps::{
    broad_glow, burns_on, decode, flame_colour, parts, script_emitters, Flicker, Lamp,
};

/// What burns, which sets a flame's shape and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlameKind {
    /// A small teardrop.
    Candle,
    /// A taller licking flame.
    Torch,
    /// A bed of fire, retail's billowing fire puffs: drawn as its particles' fire, not a card.
    Brazier,
    /// A bed of fire in a hearth, its setup's stonework round it and over it: drawn as a body of
    /// fire on its coals ([`crate::body_fire::hearth`]).
    Hearth,
    /// Fire puffs swarming round their object in a ring, not rising: drawn as a body of fire
    /// whirling round it ([`crate::body_fire::wreath`]).
    Wreath,
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
            Self::Hearth => 7,
            Self::Wreath => 8,
        }
    }

    /// The flame's height, metres, at an object's own scale.
    fn height(self) -> f32 {
        match self {
            Self::Candle => 0.09,
            Self::Torch => 0.32,
            Self::Brazier | Self::Hearth | Self::Wreath => 0.75,
            Self::Body | Self::Point | Self::Trail => 0.0,
        }
    }

    /// Whether it is drawn as a flame card; a bed of fire is drawn as its puffs, a hearth's and a
    /// wreath as a body of fire.
    #[must_use]
    pub fn carded(self) -> bool {
        !matches!(self, Self::Brazier | Self::Hearth | Self::Wreath)
    }

    /// Whether it is drawn as a body of fire placed with its object.
    #[must_use]
    pub fn placed_body(self) -> bool {
        matches!(self, Self::Hearth | Self::Wreath)
    }
}

/// A hearth's stonework round its bed of fire, metres: rising at least this far over it, the fire
/// at least this far within its sides and at most this far over its floor.
const HEARTH_HOOD: f32 = 1.0;
const HEARTH_SIDES: f32 = 0.3;
const HEARTH_FLOOR: f32 = 1.2;

/// Whether emitter `e`, a bed of fire at `at`, burns in a hearth: its puffs shrinking as they burn
/// on the coals, and its setup's own parts standing round it from just under it to
/// [`HEARTH_HOOD`] over it, as a fireplace's firebox under its chimney does.
fn in_hearth(
    store: &RetailDatStore,
    parts: &[(DataId, Affine3A)],
    e: &pyreal_assets::world::ParticleEmitterInfo,
    at: Vec3,
) -> bool {
    let Some((lo, hi)) = crate::lamps::bounds(store, parts) else {
        return false;
    };
    let inside = |v: f32, lo: f32, hi: f32| v > lo + HEARTH_SIDES && v < hi - HEARTH_SIDES;
    e.start_scale > e.final_scale
        && inside(at.x, lo.x, hi.x)
        && inside(at.y, lo.y, hi.y)
        && (lo.z..=lo.z + HEARTH_FLOOR).contains(&at.z)
        && hi.z >= at.z + HEARTH_HOOD
}

/// The lowest and highest corners of setup `id`'s parts, its own space.
#[must_use]
pub fn setup_bounds(store: &RetailDatStore, id: DataId) -> Option<(Vec3, Vec3)> {
    if divine_type(id) != Some(DbType::Setup) {
        return None;
    }
    let setup = decode::<Setup>(store, DbType::Setup, id)?;
    crate::lamps::bounds(store, &parts(&setup))
}

/// A forge's firebox, in its own space: the middle of its coals at its mouth and at its back,
/// how far they spread to each side there, and how high its roof stands over them there, metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Forge {
    pub setup: u32,
    pub mouth: Vec3,
    pub back: Vec3,
    pub mouth_half: f32,
    pub back_half: f32,
    pub mouth_roof: f32,
    pub back_roof: f32,
}

/// The forges whose fire is a bed of fire placed in their mouths apart from them: the smithy's
/// brick forge, its firebox as its mesh 0x01000E41 has it.
pub const FORGES: &[Forge] = &[Forge {
    setup: 0x0200_033C,
    mouth: Vec3::new(0.0, -1.92, 0.96),
    back: Vec3::new(0.0, -1.0, 0.96),
    mouth_half: 0.8,
    back_half: 0.5,
    mouth_roof: 0.95,
    back_roof: 0.3,
}];

/// How far out of a forge's mouth, and under its coals or over its roof, a bed of fire may stand
/// and still burn in it, metres.
const FORGE_REACH: f32 = 0.4;

/// The forge of `forges` (setups placed, each where) in whose mouth bed of fire `s`, its object
/// placed at `at`, burns, and where that forge is: a bed on coals ([`FlameSource::bed`]) only.
#[must_use]
pub fn forge_of(
    s: &FlameSource,
    at: &Affine3A,
    forges: &[(u32, Affine3A)],
) -> Option<(&'static Forge, Affine3A)> {
    if s.kind != FlameKind::Brazier || s.bed == Vec3::ZERO {
        return None;
    }
    let fire = at.transform_point3(s.offset);
    forges.iter().find_map(|(id, place)| {
        let f = FORGES.iter().find(|f| f.setup == *id)?;
        let p = place.inverse().transform_point3(fire) - f.back;
        let run = (f.mouth - f.back).truncate();
        let ahead = p.truncate().dot(run.normalize_or_zero());
        let aside = p.truncate().perp_dot(run.normalize_or_zero()).abs();
        (aside <= f.mouth_half
            && (0.0..=run.length() + FORGE_REACH).contains(&ahead)
            && (-FORGE_REACH..=f.mouth_roof + FORGE_REACH).contains(&p.z))
        .then_some((f, *place))
    })
}

/// A fire pit, in its own space: the middle of its coals, and how far they spread round it, metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pit {
    pub setup: u32,
    pub coals: Vec3,
    pub radius: f32,
}

/// The fire pits whose fire is a bed of fire placed on their coals apart from them: the ring of
/// stones 0x0200_0354 (a kitchen's, under a cauldron's tripod), its coals as its mesh 0x01000E5D
/// has them.
pub const PITS: &[Pit] = &[Pit {
    setup: 0x0200_0354,
    coals: Vec3::new(0.0, 0.0, 0.25),
    radius: 0.38,
}];

/// How far over a pit's coals, and out past them, a bed of fire may stand and still burn on them,
/// metres (some stand at a pit's edge).
const PIT_REACH: f32 = 0.6;
const PIT_AROUND: f32 = 0.3;

/// Where a bed of fire burns apart from its own object: in a forge's mouth or on a pit's coals.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Firebox {
    Forge(&'static Forge),
    Pit(&'static Pit),
}

/// Whether setup `id` holds a bed of fire placed apart from it: a forge or a fire pit.
#[must_use]
pub fn holds_fire(id: u32) -> bool {
    FORGES.iter().any(|f| f.setup == id) || PITS.iter().any(|p| p.setup == id)
}

/// The firebox of `holders` (setups placed, each where) that bed of fire `s`, its object placed at
/// `at`, burns in ([`forge_of`], or on a pit's coals), and where it is.
#[must_use]
pub fn firebox_of(
    s: &FlameSource,
    at: &Affine3A,
    holders: &[(u32, Affine3A)],
) -> Option<(Firebox, Affine3A)> {
    if let Some((f, place)) = forge_of(s, at, holders) {
        return Some((Firebox::Forge(f), place));
    }
    if s.kind != FlameKind::Brazier || s.bed == Vec3::ZERO {
        return None;
    }
    let fire = at.transform_point3(s.offset);
    holders.iter().find_map(|(id, place)| {
        let p = PITS.iter().find(|p| p.setup == *id)?;
        let q = place.inverse().transform_point3(fire) - p.coals;
        (q.truncate().length() <= p.radius + PIT_AROUND && (-PIT_REACH..=PIT_REACH).contains(&q.z))
            .then_some((Firebox::Pit(p), *place))
    })
}

/// The narrowest ring a wreath's puffs swarm round in, metres, and how much they may rise.
const WREATH_RING: f32 = 0.5;
const WREATH_RISE: f32 = 0.05;

/// Whether emitter `e`, a bed of fire, swarms round its object in a ring: how wide a ring and how
/// fast its puffs go round, radians a second.
fn wreath_of(e: &pyreal_assets::world::ParticleEmitterInfo) -> Option<(f32, f32)> {
    let mean = |lo: f32, hi: f32| (lo + hi) * 0.5;
    let ring = e.c.x.abs().max(e.c.y.abs()) * mean(e.min_c, e.max_c);
    let rate = e.b.x.abs().max(e.b.y.abs()) * mean(e.min_b, e.max_b);
    let rise = e.a.z.abs() * mean(e.min_a, e.max_a);
    // LINT-OK: an enum's discriminant, not a float.
    (e.particle_type == pyreal_animation::data::ParticleType::Swarm as i32
        && ring >= WREATH_RING
        && rate > 0.0
        && rise <= WREATH_RISE)
        .then_some((ring, rate))
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
    /// A bed on coals (a hearth's, or one that may burn in a forge, [`forge_of`]): how far its
    /// fire spreads from its middle, how far under the emitter its coals lie and how high its
    /// flames lick, metres; a wreath's ring, its puffs' mean width and how fast they go round
    /// (radians a second). Nought for other fires.
    pub bed: Vec3,
    /// A hearth's picture's colour, or a torch's flame's of another colour than fire's (a blue
    /// or green torch); nought for fire's own.
    pub tint: Vec3,
    /// How much taller than its kind's height it stands: a torch whose flame grows larger than
    /// retail's usual torch flame burns taller; one for any other.
    pub grow: f32,
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
    /// 63); a point's tip width (its bits), its seed its swing (its bits); a coloured torch's
    /// colour, linear bytes (nought for fire's own).
    pub colour: u32,
    pub count: u32,
    pub spin: u32,
}

/// The most flames a frame draws.
pub const MAX_FRAME_FLAMES: usize = 1024;
/// How far from the eye a flame is drawn, metres.
pub const FLAME_DISTANCE: f32 = 150.0;
/// How large retail's usual torch flame grows, which [`FlameKind::Torch`]'s height stands for.
const TORCH_GROWS: f32 = 3.0;

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
    let parts = parts(&setup);
    let fiery: Vec<_> = script_emitters(store, &setup, &parts)
        .into_iter()
        .filter(|(e, _)| flame_colour(store, e).is_some() && !broad_glow(store, e))
        .filter(|(e, _)| kind_of(e, false).is_some())
        .collect();
    // A ring of candles counts its bursts too.
    let many = fiery.len() > 2;
    let emitters: Vec<_> = fiery.into_iter().filter(|(e, _)| burns_on(e)).collect();
    let table = KINDS.iter().find(|(k, _)| *k == id.0).map(|(_, k)| *k);
    let mut out: Vec<FlameSource> = Vec::new();
    for (e, at) in &emitters {
        let Some(mut kind) = table.or_else(|| kind_of(e, many)) else {
            continue;
        };
        if kind == FlameKind::Brazier && in_hearth(store, &parts, e, *at) {
            kind = FlameKind::Hearth;
        }
        let wreath = wreath_of(e).filter(|_| kind == FlameKind::Brazier);
        if wreath.is_some() {
            kind = FlameKind::Wreath;
        }
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
        let coals = kind == FlameKind::Hearth
            || kind == FlameKind::Brazier && e.start_scale > e.final_scale;
        let (bed, tint) = if coals {
            // From its coals, as high as its puffs stand and rise over most of their lives.
            let puff = e.start_scale.max(e.final_scale);
            #[allow(clippy::cast_possible_truncation)] // seconds of a puff's life
            let rise = Vec3::new(e.a.x, e.a.y, e.a.z).length()
                * (e.min_a + e.max_a)
                * 0.5
                * (e.lifespan as f32)
                * 0.8;
            let tint = crate::body_fire::fire_picture(store, DataId(sprite))
                .unwrap_or(Vec3::new(1.0, 0.22, 0.06));
            (Vec3::new(e.max_offset, 0.4 * puff, rise + puff), tint)
        } else if let Some((ring, rate)) = wreath {
            let tint = crate::body_fire::fire_picture(store, DataId(sprite))
                .unwrap_or(Vec3::new(1.0, 0.22, 0.06));
            (
                Vec3::new(ring, 0.5 * (e.start_scale + e.final_scale), rate),
                tint,
            )
        } else {
            let tint = crate::lamps::coloured_flame(store, e).unwrap_or(Vec3::ZERO);
            (Vec3::ZERO, tint)
        };
        let grow = if kind == FlameKind::Torch
            && crate::body_fire::torch_picture(store, DataId(sprite)).is_some()
        {
            // Its puffs fade as they grow, so less than in proportion.
            (e.final_scale / TORCH_GROWS).sqrt().clamp(0.5, 2.0)
        } else {
            1.0
        };
        out.push(FlameSource {
            kind,
            offset: *at,
            sprite,
            bed,
            tint,
            grow,
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
    let (seed, reach) = flicker_of(base, lamps);
    Flame {
        base,
        kind: s.kind,
        height: s.kind.height() * s.grow * scale.z,
        seed,
        reach,
        cell,
        object,
        colour: if s.tint == Vec3::ZERO {
            0
        } else {
            crate::body_fire::packed(s.tint)
        },
        count: 0,
        spin: 0,
    }
}

/// The flicker a flame at `base` shares with the nearest of `lamps` that flickers: its seed and
/// reach.
#[must_use]
pub fn flicker_of(base: Vec3, lamps: &[Lamp]) -> (u32, f32) {
    let lamp = lamps
        .iter()
        .filter(|l| l.flicker == Flicker::Flame)
        .min_by(|a, b| {
            a.position
                .distance_squared(base)
                .total_cmp(&b.position.distance_squared(base))
        });
    lamp.map_or_else(|| (seed_of(base), 6.0), |l| (seed_of(l.position), l.range))
}

/// The nearest of `lamps` that flickers to `base`: its seed, reach and intensity (as its source
/// gave it, recovered from the client's light).
#[must_use]
pub fn hearth_lamp_of(base: Vec3, lamps: &[Lamp]) -> (u32, f32, f32) {
    let (seed, reach) = flicker_of(base, lamps);
    let intensity = lamps
        .iter()
        .filter(|l| l.flicker == Flicker::Flame && seed_of(l.position) == seed)
        .map(|l| l.retail.truncate().max_element())
        .next()
        .unwrap_or(1.0);
    (seed, reach, intensity)
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
    /// The hearths' and wreaths' fires placed, drawn as bodies of fire
    /// ([`crate::body_fire::hearth`], [`crate::body_fire::wreath`]).
    pub hearths: Vec<crate::body_fire::BodyFire>,
    /// Whether flames are drawn (and the emitters they stand for left out).
    pub on: bool,
    // ORDER-OK: looked up by object id, never iterated.
    sources: HashMap<u32, Vec<FlameSource>>,
}

impl Default for Flames {
    fn default() -> Self {
        Self {
            placed: Vec::new(),
            hearths: Vec::new(),
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
