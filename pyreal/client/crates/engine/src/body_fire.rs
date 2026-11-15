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

use glam::{Affine3A, Vec3, Vec4};
use pyreal_animation::data::{ParticleEmitterInfo, ParticleType};
use pyreal_animation::MotionDriver;
use pyreal_assets::{GfxObj, Surface};
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::DataId;

use crate::asset::texture::{PixelLayout, TextureSource};
use crate::flames::{Flame, FlameKind};
use crate::lamps::{decode, Flicker, Lamp, LampKind, LampSource};

/// Retail's flame puff (the Flamma's), whose shape every flame picture shares.
const FLAME_PUFF: u32 = 0x0500_171A;
/// How closely a picture's light must follow the puff's, and how bright and coloured it must be
/// (sRGB, its mean's brightest channel; saturation), to be fire.
const LIKENESS: f32 = 0.9;
const BRIGHTEST: u8 = 35;
const SATURATION: f32 = 0.3;

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
    let gfx: GfxObj = decode(store, DbType::GfxObj, gid)?;
    let src = TextureSource::new(store);
    let (puff, _) = shape_of(&src, DataId(FLAME_PUFF))?;
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
fn packed(c: Vec3) -> u32 {
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
        let mut body = record(centre, r, FlameKind::Body, f.size);
        body.count = u32::try_from(f.points.len()).unwrap_or(0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // quarter radians, 0 to 63
        let spin = (f.spin * 4.0).round().clamp(0.0, 63.0) as u32;
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
        if lamps.len() < MAX_LAMPS {
            // A blue fire lights deeper blue than it burns, so its light on grass is not green.
            let colour = if f.colour.z > f.colour.x {
                let c = f.colour.powf(5.0);
                c / c.max_element().max(1e-6)
            } else {
                f.colour
            };
            let source = LampSource {
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
