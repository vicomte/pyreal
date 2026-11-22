//! Waterfalls drawn as sheets of falling water in place of retail's cards.
//!
//! Retail builds a waterfall of endless emitters along its lip, each throwing large blue cards
//! off the edge under a hard pull down ([`is_sheet`]: setups 0x02000859, 0x0200085A, 0x020003E4
//! and 0x02000B4E place them), and of emitters of growing white puffs at its foot ([`is_mist`]:
//! 0x020003C5, the lighthouses' wisps too). With falls on
//! ([`crate::particles::Particles::falls`]) their particles are not drawn. Each frame the lips the
//! eye sees are joined into runs along the edge, and each lip's water follows its emitter's path
//! down until it meets the land, then runs on down the slope until it meets the water; where the
//! pool above stops short of the edge, the water runs to the lip from it ([`frame_falls`]). `shaders/falls.wgsl` draws a run as a sheet of streaming water, a quad a
//! row, glassy at the lip and whitening as it falls; foam spreading over the pool where it lands;
//! a low bank of mist all along its foot, over the line the water meets the pool; and soft puffs
//! of mist rolling up from there, with a faint rainbow in them when the sun shines
//! on a clear day ([`bow_of`]).

use glam::{Vec2, Vec3, Vec4};
use pyreal_animation::data::{ParticleEmitterInfo, ParticleType};
use pyreal_animation::MotionDriver;

use crate::particles::Particles;

/// A piece's bytes on the GPU (`falls.wgsl`'s `Fall`).
const PIECE: usize = 112;
pub const FALL_SIZE: u64 = PIECE as u64;
/// The most pieces drawn a frame.
pub const MAX_FRAME_FALLS: usize = 4096;
/// The rows of a sheet down its water's path.
const ROWS: usize = 20;
/// How far from the eye a waterfall is drawn, metres; its mist, nearer.
const FALL_REACH: f32 = 800.0;
const MIST_REACH: f32 = 300.0;
/// Lips joined into one run: at most this far apart, metres, facing within this cosine, and this
/// far apart in height.
const JOIN_GAP: f32 = 15.0;
const JOIN_FACING: f32 = 0.8;
const JOIN_RISE: f32 = 4.0;
/// How far past its end lips a run reaches, a share of a card's half width.
const RUN_END: f32 = 0.4;
/// The fall's path is walked this many seconds a step until it meets the ground, at most this
/// long; it must first clear the ground within `EMERGE` seconds.
const WALK_STEP: f32 = 0.04;
const WALK_MOST: f32 = 6.0;
const EMERGE: f32 = 0.5;
/// Water that lands on the slope runs on down it this many metres a step, this far over it, at
/// most this far, and no slower than this, metres a second.
const SLIDE_STEP: f32 = 0.75;
const SLIDE_LIFT: f32 = 0.35;
const SLIDE_MOST: f32 = 40.0;
const SLIDE_SPEED: f32 = 6.0;
/// How far the foam spreads from where the water lands, a share of a card's half width.
const FOAM_REACH: f32 = 1.4;
/// One spot of mist each this many metres along where a run lands; a puff's radius, a share of a
/// card's half width.
const MIST_SPACING: f32 = 6.0;
const MIST_SIZE: f32 = 1.3;
/// What sets a spot's second puff half a life after its first (`falls.wgsl` turns a seed into its
/// life by 0.618 of it).
const PUFF_APART: f32 = 0.809;
/// The bank of mist along where a run lands: how far out over the pool it lies and how tall it
/// stands, shares of a card's half width.
const BANK_OUT: f32 = 1.5;
const BANK_HEIGHT: f32 = 0.9;
/// A lone mist emitter (no water landing within this many metres) still makes its puffs, this
/// wide.
const MIST_ALONE: f32 = 20.0;
const MIST_ALONE_SIZE: f32 = 4.0;
/// The clock the shader's water runs by wraps at this many seconds.
const CLOCK_WRAP: f64 = 600.0;

/// What a piece draws (`falls.wgsl`'s kinds), in the order they are drawn.
const FOAM: u32 = 0;
const SHEET: u32 = 1;
const MIST: u32 = 2;
const BANK: u32 = 3;
/// A piece's flags: drawn in a room; its water starts under the pool above the lip.
const INDOOR: u32 = 1;
const FROM_POOL: u32 = 2;
/// Where the pool above stops short of the lip, the water runs to the lip from the pool, found
/// within this many metres back from it, starting this far under the pool's surface.
const HEAD_MOST: f32 = 25.0;
const HEAD_SINK: f32 = 0.15;
/// Water less than this many metres under a lip is the pool it leaves, not one it falls into;
/// nor is water more than `LYING` metres over its ground whose level changes more than
/// `STILL_RISE` metres across `STILL_STEP` either side.
const LEFT: f32 = 0.25;
const LYING: f32 = 2.0;
const STILL_STEP: f32 = 0.5;
const STILL_RISE: f32 = 0.25;
/// Where two lips' water lands further apart in height than this, metres, the foot between them
/// runs down the hillside: no foam or mist is laid along it.
const LAND_STEP: f32 = 3.0;

/// Whether an emitter throws a waterfall's water: endless, its large cards thrown along its frame
/// and pulled hard down.
#[must_use]
pub fn is_sheet(e: &ParticleEmitterInfo) -> bool {
    let throw = Vec2::new(e.a.x, e.a.y).length() * e.max_a;
    e.total_particles <= 0
        && e.total_seconds <= 0.0
        && e.particle_type == ParticleType::ParabolicLvga
        && throw >= 5.0
        && e.b.z * e.max_b <= -10.0
        && e.start_scale.min(e.final_scale) >= 3.0
}

/// Whether an emitter makes a waterfall's mist: endless, slowly sinking puffs that swell to
/// several metres as they fade.
#[must_use]
pub fn is_mist(e: &ParticleEmitterInfo) -> bool {
    let sink = e.a.z * e.max_a + e.b.z * e.max_b;
    e.total_particles <= 0
        && e.total_seconds <= 0.0
        && e.particle_type == ParticleType::LocalVelocity
        && e.final_scale >= 8.0
        && e.final_scale >= e.start_scale * 2.0
        && e.final_trans > e.start_trans
        && (-1.5..0.0).contains(&sink)
}

/// Whether a host's particles are a waterfall's, which falls draw in their place.
#[must_use]
pub fn is_fall(driver: &MotionDriver) -> bool {
    driver
        .particles
        .iter()
        .any(|e| is_sheet(&e.info) || is_mist(&e.info))
}

/// One emitter's lip: where its water leaves the edge (render space), which way along the ground,
/// how fast across and up, its pull down (m/s², positive), its cards' life and half width, metres.
#[derive(Debug, Clone, Copy)]
struct Lip {
    at: Vec3,
    dir: Vec2,
    speed: f32,
    rise: f32,
    pull: f32,
    life: f32,
    half: f32,
    cell: u32,
}

impl Lip {
    fn at_time(&self, t: f32) -> Vec3 {
        self.at + (self.dir * self.speed * t).extend(self.rise * t - 0.5 * self.pull * t * t)
    }
}

fn mid(lo: f32, hi: f32) -> f32 {
    0.5 * (lo + hi)
}

/// What the land is at a place, where known: the ground's height, and the water's level where it
/// is water.
type Ground<'a> = &'a dyn Fn(Vec2) -> Option<(f32, Option<f32>)>;

/// The top of the land at `p` for water falling at `under` metres, and whether it is water. The
/// pool above, carried out over the cliff metres over its slope, is not landed in; water lying on
/// its slope (retail's painted falls) is.
fn top(ground: Ground, p: Vec2, under: f32) -> Option<(f32, bool)> {
    let (g, water) = ground(p)?;
    let level = |q: Vec2| ground(q).and_then(|l| l.1);
    let still = |w: f32| {
        w - g <= LYING
            || [Vec2::X, Vec2::Y].iter().all(|&d| {
                match (level(p + d * STILL_STEP), level(p - d * STILL_STEP)) {
                    (Some(a), Some(b)) => (a - b).abs() <= STILL_RISE,
                    (Some(a), None) | (None, Some(a)) => (a - w).abs() * 2.0 <= STILL_RISE,
                    (None, None) => true,
                }
            })
    };
    Some(match water.filter(|w| *w < under && still(*w)) {
        Some(w) => (w.max(g), true),
        None => (g, false),
    })
}

/// Where a fall's water ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Landing {
    Water,
    Ground,
    Unknown,
}

/// Seconds the water from `lip` falls before it meets `ground`, and what it meets; retail's
/// cards' life where the ground is not known.
fn landing(lip: &Lip, ground: Ground) -> (f32, Landing) {
    let under = lip.at.z - LEFT;
    let mut clear = false;
    let mut t = 0.0;
    let mut last = 0.0;
    while t < WALK_MOST {
        t += WALK_STEP;
        let p = lip.at_time(t);
        let Some((g, _)) = top(ground, p.truncate(), under) else {
            return (lip.life, Landing::Unknown);
        };
        if p.z > g + 0.2 {
            clear = true;
        } else if clear || t > EMERGE {
            // Halve the step down to where it meets.
            let (mut lo, mut hi) = (last, t);
            for _ in 0..6 {
                let m = 0.5 * (lo + hi);
                let q = lip.at_time(m);
                if top(ground, q.truncate(), under).is_some_and(|(g, _)| q.z > g) {
                    lo = m;
                } else {
                    hi = m;
                }
            }
            let wet = top(ground, lip.at_time(hi).truncate(), under).is_some_and(|(_, w)| w);
            return (hi, if wet { Landing::Water } else { Landing::Ground });
        }
        last = t;
    }
    (lip.life, Landing::Unknown)
}

/// The water running to `lip` from the pool above it where the pool stops short of it (where,
/// seconds before the lip, the pool's end first), from under the pool's surface over the ground
/// between; none where no pool lies close behind it.
fn head(lip: &Lip, ground: Ground) -> Vec<(Vec3, f32)> {
    let speed = lip.speed.max(SLIDE_SPEED);
    let mut back = Vec::new();
    let mut d = 0.0;
    let pool = loop {
        d += SLIDE_STEP;
        if d > HEAD_MOST {
            return Vec::new();
        }
        let at = lip.at.truncate() - lip.dir * d;
        let Some((g, water)) = ground(at) else {
            return Vec::new();
        };
        if let Some(w) = water {
            break at.extend(w.max(g) - HEAD_SINK);
        }
        back.push((at, g));
    };
    let mut points = vec![(pool, -d / speed)];
    for (at, g) in back.into_iter().rev() {
        let behind = lip.at.truncate().distance(at);
        points.push((at.extend(g + SLIDE_LIFT), -behind / speed));
    }
    points
}

/// The path the water from `lip` takes: where it is and how many seconds past the lip it is,
/// from the pool behind it where the pool stops short ([`head`]), through the air from the lip,
/// then on down the slope where it lands on dry ground; and where it ends.
fn path(lip: &Lip, ground: Ground) -> Walked {
    let mut points = head(lip, ground);
    let from_pool = !points.is_empty();
    // Water from the pool leaves the edge at the bank's height, not over it.
    let lip = &match points.last() {
        Some(&(bank, _)) if bank.z < lip.at.z => Lip {
            at: lip.at.truncate().extend(bank.z),
            ..*lip
        },
        _ => *lip,
    };
    let (t, mut end) = landing(lip, ground);
    points.extend((0..=12).map(|k| {
        #[allow(clippy::cast_precision_loss)] // a dozen steps
        let s = t * k as f32 / 12.0;
        (lip.at_time(s), s)
    }));
    if end != Landing::Ground {
        return (points, end, from_pool);
    }
    let (mut at, mut time) = points[points.len() - 1];
    let speed = lip.speed.max(SLIDE_SPEED);
    let mut run = 0.0;
    while run < SLIDE_MOST {
        let next = at.truncate() + lip.dir * SLIDE_STEP;
        let Some((g, wet)) = top(ground, next, at.z) else {
            break;
        };
        // It stops where the land levels off or climbs.
        if g + SLIDE_LIFT > at.z - 0.05 && !wet {
            break;
        }
        time += SLIDE_STEP / speed;
        run += SLIDE_STEP;
        if wet {
            points.push((next.extend(g), time));
            end = Landing::Water;
            break;
        }
        at = next.extend(g + SLIDE_LIFT);
        points.push((at, time));
    }
    (points, end, from_pool)
}

/// A lip's path (where, seconds past the lip), where it ends, and whether it starts in the pool
/// above ([`path`]).
type Walked = (Vec<(Vec3, f32)>, Landing, bool);

/// Each lip's path down, by where it stands and which way it throws, kept while the land under it
/// is known: the lips stand still, so a path is walked once.
#[derive(Debug, Default)]
pub struct Paths {
    // ORDER-OK: looked up by lip, never iterated.
    known: std::collections::HashMap<[u32; 4], Walked>,
}

/// The most paths kept before they are all walked again.
const PATHS_KEPT: usize = 8192;

impl Paths {
    fn of(&mut self, lip: &Lip, ground: Ground) -> Walked {
        let key = [
            lip.at.x.to_bits(),
            lip.at.y.to_bits(),
            lip.at.z.to_bits(),
            lip.dir.x.to_bits(),
        ];
        if let Some(p) = self.known.get(&key) {
            return p.clone();
        }
        let p = path(lip, ground);
        if p.1 != Landing::Unknown {
            if self.known.len() >= PATHS_KEPT {
                self.known.clear();
            }
            self.known.insert(key, p.clone());
        }
        p
    }
}

/// `points` (where, seconds) resampled to `ROWS + 1` at even steps along its length, each with
/// its share of the fall (its seconds over the last's: before the lip, below nought).
fn rows_of(points: &[(Vec3, f32)]) -> Vec<(Vec3, f32, f32)> {
    let last = points[points.len() - 1].1.max(1e-4);
    let mut along = vec![0.0f32];
    for w in points.windows(2) {
        along.push(along[along.len() - 1] + w[0].0.distance(w[1].0));
    }
    let length = along[along.len() - 1].max(1e-4);
    let mut k = 0;
    (0..=ROWS)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)] // a few rows
            let at = length * i as f32 / ROWS as f32;
            while k + 2 < along.len() && along[k + 1] < at {
                k += 1;
            }
            if points.len() < 2 {
                return (points[0].0, points[0].1, 0.0);
            }
            let s = ((at - along[k]) / (along[k + 1] - along[k]).max(1e-6)).clamp(0.0, 1.0);
            let t = points[k].1 + (points[k + 1].1 - points[k].1) * s;
            (points[k].0.lerp(points[k + 1].0, s), t, t / last)
        })
        .collect()
}

/// The frame's word for every piece: the wrapped clock, and how strongly the sun makes a bow in
/// the spray ([`bow_of`]).
#[derive(Debug, Clone, Copy)]
struct Now {
    clock: f32,
    bow: f32,
}

/// One piece as `falls.wgsl` reads it.
fn piece(rows: [[f32; 4]; 6], kind: u32, flags: u32, now: Now) -> [u8; PIECE] {
    let mut out = [0u8; PIECE];
    let words = rows.iter().flatten().map(|v| v.to_bits()).chain([
        kind,
        flags,
        now.clock.to_bits(),
        now.bow.to_bits(),
    ]);
    for (k, w) in words.enumerate() {
        out[k * 4..k * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

/// Where a bank of mist stands over water from `lip` landing at `landed`, and how tall.
fn bank_end(landed: Vec3, lip: &Lip) -> (Vec3, f32) {
    (
        landed + (lip.dir * lip.half * BANK_OUT).extend(0.0),
        lip.half * BANK_HEIGHT,
    )
}

fn sees(planes: &[Vec4], at: Vec3, r: f32) -> bool {
    planes.iter().all(|q| q.truncate().dot(at) + q.w >= -r)
}

/// The lips of the waterfalls' emitters the eye can see, and the waterfalls' mist emitters.
fn lips_of(
    particles: &Particles,
    eye: Vec3,
    seen: impl Fn(u32) -> bool,
) -> (Vec<Lip>, Vec<(Vec3, u32)>) {
    let mut lips = Vec::new();
    let mut mists = Vec::new();
    for h in &particles.hosts {
        if !h.fall || !seen(h.cell) {
            continue;
        }
        let frame = crate::asset::object::frame_affine(&h.driver.particle_parent_frame());
        if Vec3::from(frame.translation).distance(eye) > FALL_REACH {
            continue;
        }
        for e in h.driver.particles.iter() {
            let info = &e.info;
            let offset = frame.transform_vector3(
                Vec3::new(info.offset_dir.x, info.offset_dir.y, info.offset_dir.z)
                    * mid(info.min_offset, info.max_offset),
            );
            let at = Vec3::from(frame.translation) + offset;
            if is_mist(info) {
                mists.push((at, h.cell));
                continue;
            }
            if !is_sheet(info) {
                continue;
            }
            let v = frame.transform_vector3(Vec3::new(info.a.x, info.a.y, info.a.z))
                * mid(info.min_a, info.max_a);
            let across = v.truncate();
            let half = particles
                .sprites
                .get(&crate::particles::sprite_of(info).0)
                .and_then(Option::as_ref)
                .map_or(crate::puffs::STEAM_HALF, |s| s.half)
                * mid(info.start_scale, info.final_scale);
            #[allow(clippy::cast_possible_truncation)] // seconds of a card's life
            lips.push(Lip {
                at,
                dir: across.normalize_or(Vec2::Y),
                speed: across.length(),
                rise: v.z,
                pull: -info.b.z * mid(info.min_b, info.max_b),
                life: info.lifespan as f32,
                half,
                cell: h.cell,
            });
        }
    }
    (lips, mists)
}

/// `lips` joined into runs along their edge, each in order along it, its ends reaching past its
/// end lips.
fn runs_of(mut left: Vec<Lip>) -> Vec<Vec<Lip>> {
    let mut runs = Vec::new();
    while let Some(first) = left.pop() {
        let mut run = vec![first];
        let mut k = 0;
        while k < run.len() {
            let a = run[k];
            let mut i = 0;
            while i < left.len() {
                let b = left[i];
                if a.dir.dot(b.dir) >= JOIN_FACING
                    && (a.at.z - b.at.z).abs() <= JOIN_RISE
                    && a.at.truncate().distance(b.at.truncate()) <= JOIN_GAP
                {
                    run.push(left.swap_remove(i));
                } else {
                    i += 1;
                }
            }
            k += 1;
        }
        let dir = run
            .iter()
            .fold(Vec2::ZERO, |s, l| s + l.dir)
            .normalize_or(first.dir);
        let axis = Vec2::new(-dir.y, dir.x);
        run.sort_by(|a, b| {
            a.at.truncate()
                .dot(axis)
                .total_cmp(&b.at.truncate().dot(axis))
        });
        let (head, tail) = (run[0], run[run.len() - 1]);
        run.insert(
            0,
            Lip {
                at: head.at - (axis * head.half * RUN_END).extend(0.0),
                ..head
            },
        );
        run.push(Lip {
            at: tail.at + (axis * tail.half * RUN_END).extend(0.0),
            ..tail
        });
        runs.push(run);
    }
    runs
}

/// How strongly the sun makes a rainbow in the falls' spray: only on a clear or sunny day with
/// nothing falling (`wet`, the rain's or the snow's share), fading out as the sun sets and by night.
#[must_use]
pub fn bow_of(light: &crate::sky::SkyLight, wet: f32) -> f32 {
    let clear = matches!(
        light.weather,
        crate::sky::Weather::Clear | crate::sky::Weather::Sunny
    );
    let s = |lo: f32, hi: f32, x: f32| {
        let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    f32::from(u8::from(clear))
        * s(0.5, 0.9, light.daylight)
        * s(0.02, 0.12, light.sun_dir.z)
        * (1.0 - wet.clamp(0.0, 1.0))
}

/// A frame's pieces, far to near: as `falls.wgsl` reads them, and each one's distance from the
/// eye, by which the renderer draws them among the other translucent draws.
#[derive(Debug, Default)]
pub struct Framed {
    pub bytes: Vec<u8>,
    pub distances: Vec<f32>,
}

/// The waterfalls' pieces seen from `eye` inside `planes` (their cells' rectangles not empty, as
/// `seen` says): the foam, the sheets' rows and the mist, far to near. `ground` is the land's
/// height at a place (the water's where it is water) and whether it is water, where known;
/// `paths` keeps the paths walked; `bow` how strongly the sun makes a rainbow in the spray
/// ([`bow_of`]).
#[must_use]
#[allow(clippy::too_many_lines)] // one pass from lips to pieces
pub fn frame_falls(
    paths: &mut Paths,
    particles: &Particles,
    eye: Vec3,
    planes: &[Vec4],
    seen: impl Fn(u32) -> bool,
    ground: Ground,
    bow: f32,
) -> Framed {
    if !particles.falls {
        return Framed::default();
    }
    #[allow(clippy::cast_possible_truncation)] // a wrapped clock
    let now = Now {
        clock: (particles.clock % CLOCK_WRAP) as f32,
        bow,
    };
    let (lips, mists) = lips_of(particles, eye, seen);
    let (mut sheets, mut foam, mut mist) = (Vec::new(), Vec::new(), Vec::new());
    let mut bank_pieces = Vec::new();
    let mut landed: Vec<Vec3> = Vec::new();
    for run in runs_of(lips) {
        let paths: Vec<Walked> = run.iter().map(|l| paths.of(l, ground)).collect();
        let rows: Vec<Vec<(Vec3, f32, f32)>> = paths.iter().map(|p| rows_of(&p.0)).collect();
        let mut u = vec![0.0f32];
        for w in run.windows(2) {
            u.push(u[u.len() - 1] + w[0].at.distance(w[1].at));
        }
        let length = u[u.len() - 1];
        let head = run[0];
        let seed = (head.at.x * 0.37 + head.at.y * 0.61).rem_euclid(97.0);
        let flags = if head.cell != 0 { INDOOR } else { 0 };
        for k in 0..run.len() - 1 {
            let (a, b) = (run[k], run[k + 1]);
            let (ra, rb) = (&rows[k], &rows[k + 1]);
            let half = mid(a.half, b.half);
            let pooled = if paths[k].2 && paths[k + 1].2 {
                flags | FROM_POOL
            } else {
                flags
            };
            for r in 0..ROWS {
                let centre = (ra[r].0 + rb[r].0 + ra[r + 1].0 + rb[r + 1].0) * 0.25;
                let reach = ra[r]
                    .0
                    .distance(rb[r + 1].0)
                    .max(rb[r].0.distance(ra[r + 1].0));
                if centre.distance(eye) > FALL_REACH || !sees(planes, centre, reach) {
                    continue;
                }
                sheets.push((
                    centre.distance(eye),
                    piece(
                        [
                            ra[r].0.extend(u[k]).to_array(),
                            rb[r].0.extend(u[k + 1]).to_array(),
                            ra[r + 1].0.extend(ra[r + 1].1).to_array(),
                            rb[r + 1].0.extend(rb[r + 1].1).to_array(),
                            [
                                ra[r].1,
                                rb[r].1,
                                mid(ra[r].2, rb[r].2),
                                mid(ra[r + 1].2, rb[r + 1].2),
                            ],
                            [length, half, seed, 0.0],
                        ],
                        SHEET,
                        pooled,
                        now,
                    ),
                ));
            }
            let (ea, eb) = (paths[k].1, paths[k + 1].1);
            if ea == Landing::Unknown || eb == Landing::Unknown {
                continue;
            }
            let (pa, pb) = (ra[ROWS].0, rb[ROWS].0);
            landed.push(pa);
            if (pa.z - pb.z).abs() > LAND_STEP {
                continue;
            }
            let reach = half * FOAM_REACH;
            let middle = (pa + pb) * 0.5;
            if ea == Landing::Water
                && eb == Landing::Water
                && sees(planes, middle, reach + pa.distance(pb))
            {
                foam.push((
                    middle.distance(eye),
                    piece(
                        [
                            pa.extend(u[k]).to_array(),
                            pb.extend(u[k + 1]).to_array(),
                            [a.dir.x, a.dir.y, 0.0, 0.0],
                            [b.dir.x, b.dir.y, 0.0, 0.0],
                            [0.0; 4],
                            [length, reach, seed, 0.0],
                        ],
                        FOAM,
                        flags,
                        now,
                    ),
                ));
            }
            // A low bank of mist all along where it lands, over the line the water meets.
            // Each end by its own lip, so the banks of a run meet.
            let ((ba, ha), (bb, hb)) = (bank_end(pa, &a), bank_end(pb, &b));
            let bank = (ba + bb) * 0.5;
            if bank.distance(eye) <= MIST_REACH
                && sees(planes, bank, ba.distance(bb) + ha.max(hb) * 2.0)
            {
                bank_pieces.push((
                    bank.distance(eye),
                    piece(
                        [
                            ba.extend(u[k]).to_array(),
                            bb.extend(u[k + 1]).to_array(),
                            [a.dir.x, a.dir.y, ha, 0.0],
                            [b.dir.x, b.dir.y, hb, 0.0],
                            [0.0; 4],
                            [length, half, seed, 0.0],
                        ],
                        BANK,
                        flags,
                        now,
                    ),
                ));
            }
            // Its mist: a spot each `MIST_SPACING` along where it lands, two puffs at each, half a
            // life apart, so it never thins.
            let span = pa.distance(pb);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few spots
            let n = (span / MIST_SPACING).ceil().max(1.0) as u32;
            let radius = half * MIST_SIZE;
            for j in 0..n {
                #[allow(clippy::cast_precision_loss)] // a few spots
                let s = (j as f32 + 0.5) / n as f32;
                let at = pa.lerp(pb, s);
                if at.distance(eye) > MIST_REACH || !sees(planes, at, radius * 3.0) {
                    continue;
                }
                let dir = a.dir.lerp(b.dir, s).normalize_or(a.dir);
                for apart in [0.0, PUFF_APART] {
                    mist.push((
                        at.distance(eye),
                        piece(
                            [
                                at.extend(radius).to_array(),
                                [0.0; 4],
                                [dir.x, dir.y, 0.0, 0.0],
                                [0.0; 4],
                                [0.0; 4],
                                [0.0, radius, seed + u[k] + s * span + apart, 0.0],
                            ],
                            MIST,
                            flags,
                            now,
                        ),
                    ));
                }
            }
        }
    }
    // A mist emitter with no water landing near it keeps its puffs.
    for (at, cell) in mists {
        if landed.iter().any(|p| p.distance(at) < MIST_ALONE)
            || at.distance(eye) > MIST_REACH
            || !sees(planes, at, MIST_ALONE_SIZE * 3.0)
        {
            continue;
        }
        let seed = (at.x * 0.37 + at.y * 0.61).rem_euclid(97.0);
        for apart in [0.0, PUFF_APART] {
            mist.push((
                at.distance(eye),
                piece(
                    [
                        at.extend(MIST_ALONE_SIZE).to_array(),
                        [0.0; 4],
                        [0.0, 1.0, 0.0, 0.0],
                        [0.0; 4],
                        [0.0; 4],
                        [0.0, MIST_ALONE_SIZE, seed + apart, 0.0],
                    ],
                    MIST,
                    if cell != 0 { INDOOR } else { 0 },
                    now,
                ),
            ));
        }
    }
    mist.extend(bank_pieces);
    far_to_near(foam, sheets, mist)
}

/// The foam, sheets and mist as one list far to near, so the pieces sort in among the other
/// translucent draws; at one distance foam goes under its sheet and mist over it.
fn far_to_near(
    foam: Vec<(f32, [u8; PIECE])>,
    sheets: Vec<(f32, [u8; PIECE])>,
    mist: Vec<(f32, [u8; PIECE])>,
) -> Framed {
    let mut all: Vec<(f32, [u8; PIECE])> = foam.into_iter().chain(sheets).chain(mist).collect();
    all.sort_by(|a, b| b.0.total_cmp(&a.0));
    all.truncate(MAX_FRAME_FALLS);
    Framed {
        distances: all.iter().map(|(d, _)| *d).collect(),
        bytes: all.into_iter().flat_map(|(_, p)| p).collect(),
    }
}

/// Where the pieces from `from` on (`distances` far to near) that are no nearer than a
/// translucent draw `at` metres away end: those drawn before it.
#[must_use]
pub fn drawn_before(distances: &[f32], from: usize, at: f32) -> usize {
    from + distances
        .get(from..)
        .unwrap_or(&[])
        .iter()
        .take_while(|d| **d >= at)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> ParticleEmitterInfo {
        // 0x320004A4, a lip of 0x0200085A.
        ParticleEmitterInfo {
            emitter_type: 1,
            particle_type: ParticleType::ParabolicLvga,
            max_particles: 20,
            lifespan: 1.8,
            a: pyreal_primitives::Vec3::new(0.0, 22.5, 0.5),
            min_a: 0.5,
            max_a: 0.5,
            b: pyreal_primitives::Vec3::new(0.0, 0.0, -32.0),
            min_b: 0.5,
            max_b: 0.5,
            start_scale: 6.0,
            final_scale: 9.0,
            ..ParticleEmitterInfo::default()
        }
    }

    fn mist() -> ParticleEmitterInfo {
        // 0x3200000C, 0x020003C5's.
        ParticleEmitterInfo {
            emitter_type: 1,
            particle_type: ParticleType::LocalVelocity,
            max_particles: 24,
            lifespan: 2.75,
            a: pyreal_primitives::Vec3::new(0.0, 0.0, -0.3),
            min_a: 0.5,
            max_a: 1.5,
            b: pyreal_primitives::Vec3::new(0.0, 0.0, -0.3),
            min_b: 0.5,
            max_b: 1.5,
            start_scale: 3.0,
            final_scale: 10.0,
            start_trans: 0.0,
            final_trans: 1.0,
            ..ParticleEmitterInfo::default()
        }
    }

    /// A waterfall's lip and its mist are told by how their particles behave; a fountain's spray
    /// and rising steam are not.
    /// The foam, sheets and mist of every fall make one list far to near, and a translucent draw
    /// between them comes after the pieces beyond it and before those nearer: a lamp's glow near
    /// the eye is not drawn under a waterfall far off.
    #[test]
    fn a_falls_pieces_sort_in_among_the_other_translucent_draws() {
        let p = |d: f32, kind: u32| {
            let now = Now {
                clock: 0.0,
                bow: 0.0,
            };
            (d, piece([[0.0; 4]; 6], kind, 0, now))
        };
        let framed = far_to_near(
            vec![p(90.0, FOAM), p(30.0, FOAM)],
            vec![p(100.0, SHEET), p(30.0, SHEET), p(12.0, SHEET)],
            vec![p(95.0, MIST), p(30.0, MIST)],
        );
        assert_eq!(
            framed.distances,
            [100.0, 95.0, 90.0, 30.0, 30.0, 30.0, 12.0]
        );
        assert_eq!(framed.bytes.len(), framed.distances.len() * PIECE);
        let kind = |i: usize| framed.bytes[i * PIECE + 96];
        // At one distance: foam, then the sheet, then the mist.
        assert_eq!([kind(3), kind(4), kind(5)], [0, 1, 2]);
        let d = &framed.distances;
        // A glow 5 m away comes after every piece; one 50 m away after the far fall only; one past
        // them all before any.
        assert_eq!(drawn_before(d, 0, 5.0), 7);
        assert_eq!(drawn_before(d, 0, 50.0), 3);
        assert_eq!(drawn_before(d, 0, 200.0), 0);
        assert_eq!(drawn_before(d, 3, 20.0), 6);
        assert_eq!(drawn_before(d, 7, 1.0), 7);
    }

    /// The bank of mist at a fall's foot lies out over the pool in front of where the water
    /// lands, so from in front it hides the line the sheet meets the water at, and stands about as
    /// tall as the water is wide; each end is its own lip's, so a run's banks meet.
    #[test]
    fn the_bank_of_mist_lies_out_over_the_pool_in_front_of_the_fall() {
        let lip = Lip {
            at: Vec3::new(0.0, 0.0, 30.0),
            dir: Vec2::new(0.0, 1.0),
            speed: 11.0,
            rise: 0.0,
            pull: 16.0,
            life: 1.8,
            half: 3.5,
            cell: 0,
        };
        let landed = Vec3::new(0.0, 6.0, 0.0);
        let (at, tall) = bank_end(landed, &lip);
        assert!(at.y > landed.y + 3.0 && at.y < landed.y + 8.0, "{at}");
        assert!((at.x, at.z) == (landed.x, landed.z), "{at}");
        assert!(tall > 2.0 && tall < 5.0, "{tall}");
        let wider = Lip { half: 4.5, ..lip };
        assert_ne!(bank_end(landed, &wider), (at, tall));
    }

    #[test]
    fn a_waterfall_is_told_by_its_emitters() {
        assert!(is_sheet(&sheet()));
        assert!(!is_mist(&sheet()));
        assert!(is_mist(&mist()));
        assert!(!is_sheet(&mist()));
        let spray = ParticleEmitterInfo {
            start_scale: 0.1,
            final_scale: 0.5,
            ..sheet()
        };
        assert!(!is_sheet(&spray));
        let steam = ParticleEmitterInfo {
            a: pyreal_primitives::Vec3::new(0.0, 0.0, 0.5),
            b: pyreal_primitives::Vec3::new(0.0, 0.0, 0.5),
            ..mist()
        };
        assert!(!is_mist(&steam));
        let once = ParticleEmitterInfo {
            total_particles: 30,
            ..sheet()
        };
        assert!(!is_sheet(&once));
    }

    /// Water thrown off a lip falls until it meets the ground under it, after clearing the bank
    /// it leaves from.
    #[test]
    fn the_water_lands_where_its_path_meets_the_ground() {
        let lip = Lip {
            at: Vec3::new(0.0, 0.0, 30.0),
            dir: Vec2::Y,
            speed: 11.25,
            rise: 0.25,
            pull: 16.0,
            life: 1.8,
            half: 5.0,
            cell: 0,
        };
        // A bank as high as the lip behind it, a pool at nought beyond.
        let ground = |p: Vec2| {
            Some(if p.y < 0.5 {
                (30.5, None)
            } else {
                (0.0, Some(0.0))
            })
        };
        let (t, landed) = landing(&lip, &ground);
        assert_eq!(landed, Landing::Water);
        assert!(
            (lip.at_time(t).z).abs() < 0.2,
            "lands at {:?}",
            lip.at_time(t)
        );
        let unknown = |_: Vec2| None;
        assert_eq!(landing(&lip, &unknown), (1.8, Landing::Unknown));
    }

    /// The land's water carries the pool above out over the brink of a fall: water from a lip
    /// falls through it, down the cliff to the pool below, and lands on no water higher than the
    /// lip.
    #[test]
    fn water_falls_through_the_pool_it_leaves_to_the_one_below() {
        let lip = Lip {
            at: Vec3::new(0.0, 0.0, 29.85),
            dir: Vec2::Y,
            speed: 11.25,
            rise: 0.25,
            pull: 20.0,
            life: 3.0,
            half: 5.0,
            cell: 0,
        };
        // The pool at 30 m reaches 12 m out over a cliff that falls to a pool at nought, at its own
        // level or sloping down towards the pool below.
        for slope in [0.0, 0.8] {
            let ground = move |p: Vec2| {
                let h = (30.0 - p.y.max(0.0) * 2.5).max(0.0);
                let band = 30.0 - p.y.max(0.0) * slope;
                Some((
                    h,
                    if p.y < 12.0 {
                        Some(band)
                    } else {
                        (h <= 0.0).then_some(0.0)
                    },
                ))
            };
            let (t, landed) = landing(&lip, &ground);
            assert_eq!(landed, Landing::Water);
            assert!(
                lip.at_time(t).z.abs() < 0.3,
                "{slope}: lands at {:?}",
                lip.at_time(t)
            );
            let (points, end, _) = path(&lip, &ground);
            assert_eq!(end, Landing::Water);
            assert!(points[points.len() - 1].0.z.abs() < 0.3);
        }
        // A slope painted as water lying on it is landed on where the water meets it.
        let painted = |p: Vec2| {
            let h = (30.0 - p.y.max(0.0) * 0.8).max(0.0);
            Some((h, Some(h)))
        };
        let (t, landed) = landing(&lip, &painted);
        assert_eq!(landed, Landing::Water);
        let at = lip.at_time(t);
        assert!(
            at.z > 1.0 && (at.z - painted(at.truncate()).unwrap().0).abs() < 0.3,
            "{at}"
        );
    }

    /// Water that lands on the slope runs on down it to the pool.
    #[test]
    fn water_landing_on_the_slope_runs_down_it_to_the_pool() {
        let lip = Lip {
            at: Vec3::new(0.0, 0.0, 30.0),
            dir: Vec2::Y,
            speed: 3.0,
            rise: 0.0,
            pull: 16.0,
            life: 1.8,
            half: 5.0,
            cell: 0,
        };
        // A slope falling a metre a metre from the lip to a pool at nought, 30 m out.
        let ground = |p: Vec2| {
            let h = 30.0 - p.y.max(0.0) - 0.5;
            Some((h, (h <= 0.0).then_some(0.0)))
        };
        let (points, end, from_pool) = path(&lip, &ground);
        assert!(!from_pool);
        assert_eq!(end, Landing::Water);
        let last = points[points.len() - 1].0;
        assert!(last.z.abs() < 0.01 && last.y > 25.0, "ends at {last:?}");
        assert!(points.windows(2).all(|w| w[1].1 >= w[0].1), "time runs on");
        let rows = rows_of(&points);
        assert_eq!(rows.len(), ROWS + 1);
        assert!(rows[0].0.distance(lip.at) < 1e-4 && rows[ROWS].0.distance(last) < 1e-3);
    }
    /// Where the pool above stops short of the lip, the water runs to the lip from under the
    /// pool's surface; where the lip stands in dry land far from any pool, it starts at the lip.
    #[test]
    fn water_runs_to_the_lip_from_a_pool_that_stops_short() {
        let lip = Lip {
            at: Vec3::new(0.0, 0.0, 31.0),
            dir: Vec2::Y,
            speed: 11.0,
            rise: 0.0,
            pull: 16.0,
            life: 1.8,
            half: 5.0,
            cell: 0,
        };
        // A pool at 30 m behind y = -8, a dry beach at 30.2 m up to the lip, a drop to nought.
        let ground = |p: Vec2| {
            Some(if p.y < -8.0 {
                (29.0, Some(30.0))
            } else if p.y < 0.5 {
                (30.2, None)
            } else {
                (0.0, Some(0.0))
            })
        };
        let (points, end, from_pool) = path(&lip, &ground);
        assert!(from_pool);
        assert_eq!(end, Landing::Water);
        let (start, t0) = points[0];
        assert!(start.y < -8.0 && start.z < 30.0, "starts at {start:?}");
        assert!(t0 < 0.0);
        let to_lip: Vec<_> = points.iter().take_while(|p| p.1 < 0.0).collect();
        assert!(to_lip
            .windows(2)
            .all(|w| w[1].1 > w[0].1 && w[1].0.z >= w[0].0.z - 1e-4));
        assert!(to_lip.iter().all(|p| p.0.z >= 30.0 - 0.2));
        let far = |_: Vec2| Some((30.2, None));
        assert!(head(&lip, &far).is_empty());
    }
}
