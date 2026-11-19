//! Fountains and basins drawn as water in place of retail's blue pictures and bubbles.
//!
//! Retail fills a fountain's basin with a level face of a translucent blue water picture
//! ([`crate::asset::material::POOL_PICTURES`]) and throws its water as endless emitters of small
//! bubble cards ([`is_jet`]): straight up and back from a spout (0x32000227 on 0x0200027B, the
//! academies' fountain, and its kin), or off a tier's lip down to the basin below (0x3200060E and
//! 0x3200060F on 0x02000AA3). With water drawn as water ([`crate::particles::Particles::fountains`])
//! the basin's faces draw as still water over what lies under them (`fountains.wgsl`'s `fs_pool`:
//! the room mirrored, the floor seen through it, ripples), the jets' bubbles are not drawn, and
//! each jet is drawn as streams of clear water along its emitter's path ([`frame_jets`]): a
//! column up to where it stops rising, then a crown of streams falling from there into the basin,
//! or one stream off a lip; each ringing the water where it lands.

use glam::{Affine3A, Vec2, Vec3, Vec4};
use pyreal_animation::data::{ParticleEmitterInfo, ParticleType};
use pyreal_assets::{Decode, GfxObj, Setup, Surface};
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::DataId;

use crate::particles::Particles;

/// A piece's bytes on the GPU (`fountains.wgsl`'s `Jet`), and the header before them (how many,
/// the clock).
const PIECE: usize = 64;
pub const JET_SIZE: u64 = PIECE as u64;
pub const JET_HEAD: u64 = HEAD as u64;
const HEAD: usize = 16;
/// The most pieces drawn a frame.
pub const MAX_FRAME_JETS: usize = 512;
/// A piece's strip: this many rows along its path (`fountains.wgsl`'s `JET_ROWS`), two
/// triangles a row.
pub const JET_ROWS: u32 = 24;
pub const JET_VERTICES: u32 = JET_ROWS * 6;
/// The pictures of retail's fountain water: a jet's bubbles, a spout's stream.
pub const JET_SPRITES: [u32; 2] = [0x0100_16C0, 0x0100_108E];
/// The blue glints a jet's spray throws round its spout (0x3200000B).
pub const SPRAY_SPRITES: [u32; 1] = [0x0100_173E];
/// How far from the eye a jet is drawn, metres.
const JET_REACH: f32 = 150.0;
/// Its path is walked this many seconds a step, at most this long.
const WALK_STEP: f32 = 0.01;
const WALK_MOST: f32 = 4.0;
/// A jet whose throw across is under this share of its throw up rises and falls back: a column
/// and a crown.
const UPRIGHT: f32 = 0.15;
/// The column's radius at the spout and where it stops rising, metres; a stream's.
const COLUMN_RADIUS: f32 = 0.035;
const COLUMN_TOP: f32 = 0.06;
const STREAM_RADIUS: f32 = 0.028;
/// The crown's streams, and how far out across the basin they land, a share of the way from the
/// column to the basin's nearest edge.
const CROWN: usize = 14;
const CROWN_REACH: f32 = 0.5;
/// How far a crown with no basin under it throws its water, metres a second.
const CROWN_LOOSE: f32 = 0.25;
/// The clock the shader's water runs by wraps at this many seconds.
const CLOCK_WRAP: f64 = 600.0;

/// What a piece draws (`fountains.wgsl`'s kinds).
const COLUMN: u32 = 0;
const CROWN_STREAM: u32 = 1;
const STREAM: u32 = 2;
/// Nothing drawn: only rings where a model's stream meets its basin ([`Spill`]).
const RING: u32 = 3;
/// How strongly a model's stream rings its basin, and how far off its water's height its foot may
/// be, metres.
const SPILL_RING: f32 = 6.0;
const SPILL_NEAR: f32 = 0.02;
/// Feet nearer than this, metres, are one stream's.
const SPILL_JOIN: f32 = 0.3;
/// A piece's flag: drawn in a room.
const INDOOR: u32 = 1 << 8;

/// Whether an emitter throws a fountain's water: endless, its cards one of retail's water
/// pictures, pulled down.
#[must_use]
pub fn is_jet(e: &ParticleEmitterInfo) -> bool {
    e.total_particles <= 0
        && e.total_seconds <= 0.0
        && matches!(
            e.particle_type,
            ParticleType::ParabolicLvga
                | ParticleType::ParabolicLvgagr
                | ParticleType::ParabolicLvla
                | ParticleType::ParabolicLvlalr
                | ParticleType::ParabolicGvga
                | ParticleType::ParabolicGvgagr
        )
        && e.b.z * mid(e.min_b, e.max_b) < 0.0
        && JET_SPRITES.contains(&crate::particles::sprite_of(e).0)
}

/// Whether an emitter is a jet's spray: endless, its cards retail's blue glints.
#[must_use]
pub fn is_spray(e: &ParticleEmitterInfo) -> bool {
    e.total_particles <= 0
        && e.total_seconds <= 0.0
        && SPRAY_SPRITES.contains(&crate::particles::sprite_of(e).0)
}

/// Whether a host's emitter is drawn by the fountains in its place: a jet, or the spray round one.
#[must_use]
pub fn drawn_for(driver: &pyreal_animation::MotionDriver, e: &ParticleEmitterInfo) -> bool {
    is_jet(e) || is_spray(e) && driver.particles.iter().any(|j| is_jet(&j.info))
}

fn mid(lo: f32, hi: f32) -> f32 {
    0.5 * (lo + hi)
}

/// One level face of a basin's water: its part, its rectangle and height on the part's model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoolFace {
    pub part: usize,
    pub min: Vec2,
    pub max: Vec2,
    pub z: f32,
}

fn read<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

/// The level faces of basin water on setup `setup`'s parts ([`crate::asset::material::is_pool`]),
/// those at one height on one part joined into one rectangle.
#[must_use]
pub fn pool_faces(store: &RetailDatStore, setup: DataId) -> Vec<PoolFace> {
    let mut out: Vec<PoolFace> = Vec::new();
    if pyreal_dat::divine_type(setup) != Some(DbType::Setup) {
        return out;
    }
    let Some(s) = read::<Setup>(store, DbType::Setup, setup) else {
        return out;
    };
    for (part, id) in s.parts.iter().enumerate() {
        let Some(g) = read::<GfxObj>(store, DbType::GfxObj, *id) else {
            continue;
        };
        let pools: Vec<bool> = g
            .surfaces
            .iter()
            .map(|s| {
                read::<Surface>(store, DbType::Surface, *s)
                    .is_some_and(|s| crate::asset::material::is_pool(&s))
            })
            .collect();
        for p in &g.polygons {
            if !pools
                .get(usize::from(p.pos_surface))
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            let at: Vec<Vec3> = p
                .vertex_ids
                .iter()
                .filter_map(|&v| g.vertex_array.vertices.get(usize::from(v)))
                .map(|v| Vec3::new(v.position.x, v.position.y, v.position.z))
                .collect();
            let Some(&first) = at.first() else {
                continue;
            };
            if at.iter().any(|q| (q.z - first.z).abs() > 0.01) {
                continue;
            }
            let (lo, hi) = at
                .iter()
                .fold((first.truncate(), first.truncate()), |(lo, hi), q| {
                    (lo.min(q.truncate()), hi.max(q.truncate()))
                });
            if let Some(f) = out
                .iter_mut()
                .find(|f| f.part == part && (f.z - first.z).abs() <= 0.01)
            {
                f.min = f.min.min(lo);
                f.max = f.max.max(hi);
            } else {
                out.push(PoolFace {
                    part,
                    min: lo,
                    max: hi,
                    z: first.z,
                });
            }
        }
    }
    out
}

/// Where a model's own stream (an upright water face, a wall fountain's) meets a basin's water on
/// its part: its foot's middle on the part's model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spill {
    pub part: usize,
    pub at: Vec3,
}

/// The feet of setup `setup`'s upright water faces that stand in its basins' water
/// ([`pool_faces`]), one a stream.
#[must_use]
pub fn spills(store: &RetailDatStore, setup: DataId, faces: &[PoolFace]) -> Vec<Spill> {
    let mut out = Vec::new();
    if faces.is_empty() {
        return out;
    }
    let Some(s) = read::<Setup>(store, DbType::Setup, setup) else {
        return out;
    };
    for (part, id) in s.parts.iter().enumerate() {
        if !faces.iter().any(|f| f.part == part) {
            continue;
        }
        let Some(g) = read::<GfxObj>(store, DbType::GfxObj, *id) else {
            continue;
        };
        let pools: Vec<bool> = g
            .surfaces
            .iter()
            .map(|s| {
                read::<Surface>(store, DbType::Surface, *s)
                    .is_some_and(|s| crate::asset::material::is_pool(&s))
            })
            .collect();
        for p in &g.polygons {
            if !pools
                .get(usize::from(p.pos_surface))
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            let at: Vec<Vec3> = p
                .vertex_ids
                .iter()
                .filter_map(|&v| g.vertex_array.vertices.get(usize::from(v)))
                .map(|v| Vec3::new(v.position.x, v.position.y, v.position.z))
                .collect();
            let [a, b, c, ..] = at[..] else {
                continue;
            };
            if (b - a).cross(c - a).normalize_or_zero().z.abs() > 0.7 {
                continue;
            }
            let low = at.iter().map(|q| q.z).fold(f32::MAX, f32::min);
            let foot: Vec<Vec3> = at.iter().copied().filter(|q| q.z - low < 0.01).collect();
            #[allow(clippy::cast_precision_loss)] // a face's few corners
            let mid = foot.iter().copied().sum::<Vec3>() / foot.len() as f32;
            let Some(f) = faces.iter().find(|f| {
                f.part == part
                    && (f.z - low).abs() <= SPILL_NEAR
                    && mid.truncate().cmpge(f.min - SPILL_NEAR).all()
                    && mid.truncate().cmple(f.max + SPILL_NEAR).all()
            }) else {
                continue;
            };
            let at = mid.truncate().clamp(f.min, f.max).extend(f.z);
            // One stream's faces ring as one.
            if let Some(s) = out
                .iter_mut()
                .find(|s| s.part == part && s.at.distance(at) < SPILL_JOIN)
            {
                s.at = (s.at + at) * 0.5;
            } else {
                out.push(Spill { part, at });
            }
        }
    }
    out
}

/// A basin's water face where it stands: the way back to its part's model, its rectangle there
/// and its height in the world.
#[derive(Debug, Clone, Copy)]
struct Basin {
    to_model: Affine3A,
    min: Vec2,
    max: Vec2,
    level: f32,
}

impl Basin {
    fn new(place: Affine3A, f: &PoolFace) -> Self {
        let centre = (f.min + f.max) * 0.5;
        Self {
            to_model: place.inverse(),
            min: f.min,
            max: f.max,
            level: place.transform_point3(centre.extend(f.z)).z,
        }
    }

    fn holds(&self, p: Vec3) -> bool {
        let q = self.to_model.transform_point3(p).truncate();
        q.cmpge(self.min).all() && q.cmple(self.max).all()
    }

    /// How far from `p` (over the face) to the face's nearest edge, metres on the model.
    fn room(&self, p: Vec3) -> f32 {
        let q = self.to_model.transform_point3(p).truncate();
        (q - self.min).min(self.max - q).min_element().max(0.0)
    }
}

/// A path thrown from `at` at `v`, pulled by `pull` (m/s², up positive).
#[derive(Debug, Clone, Copy)]
struct Path {
    at: Vec3,
    v: Vec3,
    pull: f32,
}

impl Path {
    fn at_time(&self, t: f32) -> Vec3 {
        self.at + self.v * t + Vec3::Z * (0.5 * self.pull * t * t)
    }

    /// When and where the path, falling, first meets a basin's water under it, if it does before
    /// `most` seconds.
    fn landing(&self, basins: &[Basin], most: f32) -> Option<(f32, Vec3)> {
        let mut prev = self.at;
        let mut t = 0.0;
        while t < most {
            t += WALK_STEP;
            let p = self.at_time(t);
            let falling = self.v.z + self.pull * t < 0.0;
            if falling {
                for b in basins {
                    if prev.z >= b.level && p.z < b.level && b.holds(p) {
                        let k = (prev.z - b.level) / (prev.z - p.z).max(1e-6);
                        let tt = t - WALK_STEP * (1.0 - k);
                        return Some((tt, self.at_time(tt)));
                    }
                }
            }
            prev = p;
        }
        None
    }
}

/// One piece's bytes: its path, its radius and how fast it swells, its seed and kind, and where
/// it lands (w how strongly it rings the water there, 0 for nowhere).
fn piece(
    path: &Path,
    end: f32,
    radius: f32,
    swell: f32,
    seed: f32,
    kind: u32,
    land: Vec4,
) -> [u8; PIECE] {
    let mut out = [0u8; PIECE];
    #[allow(clippy::cast_precision_loss)] // a small whole number
    let kind_word = kind as f32;
    let words: [f32; 16] = [
        path.at.x, path.at.y, path.at.z, radius, path.v.x, path.v.y, path.v.z, end, path.pull,
        swell, seed, kind_word, land.x, land.y, land.z, land.w,
    ];
    for (k, w) in words.iter().enumerate() {
        out[k * 4..k * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

fn hash(n: u32) -> f32 {
    let mut h = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    #[allow(clippy::cast_precision_loss)] // 24 bits
    let f = (h & 0xFF_FFFF) as f32 / 16_777_215.0;
    f
}

/// The pieces of one jet thrown from `at` at `v` under `pull` (`life` seconds retail's bubbles
/// last), landing in `basins`.
fn jet_pieces(
    at: Vec3,
    v: Vec3,
    pull: f32,
    life: f32,
    basins: &[Basin],
    seed: u32,
    flags: u32,
) -> Vec<[u8; PIECE]> {
    let mut out = Vec::new();
    let across = v.truncate().length();
    if v.z > 0.0 && across < UPRIGHT * v.z && pull < 0.0 {
        // Up to where it stops rising, then a crown falling from there.
        let rise = Path { at, v, pull };
        let top_t = -v.z / pull;
        out.push(piece(
            &rise,
            top_t,
            COLUMN_RADIUS,
            (COLUMN_TOP - COLUMN_RADIUS) / top_t.max(0.05),
            hash(seed),
            COLUMN | flags,
            Vec4::ZERO,
        ));
        let top = rise.at_time(top_t);
        let under = basins
            .iter()
            .filter(|b| b.level < top.z && b.holds(top))
            .max_by(|a, b| a.level.total_cmp(&b.level));
        let (speed, most) = match under {
            Some(b) => {
                let fall = ((2.0 * (top.z - b.level)) / -pull).sqrt();
                (b.room(top) * CROWN_REACH / fall.max(0.1), fall * 1.5 + 0.2)
            }
            None => (CROWN_LOOSE, life),
        };
        for k in 0..CROWN {
            let s = seed
                .wrapping_mul(31)
                .wrapping_add(u32::try_from(k).unwrap_or(0));
            #[allow(clippy::cast_precision_loss)] // a handful of streams
            let turn = (k as f32 + 0.6 * hash(s)) / CROWN as f32 * std::f32::consts::TAU;
            let out_v = Vec2::from_angle(turn) * speed * (0.75 + 0.5 * hash(s ^ 0x55));
            let path = Path {
                at: top,
                v: (out_v + v.truncate()).extend(0.0),
                pull,
            };
            let (end, land) = path
                .landing(basins, most)
                .map_or((most.min(life), Vec4::ZERO), |(t, p)| (t, p.extend(1.0)));
            out.push(piece(
                &path,
                end,
                STREAM_RADIUS,
                0.0,
                hash(s ^ 0xA3),
                CROWN_STREAM | flags,
                land,
            ));
        }
    } else {
        // Off a lip or a spout, down to the basin.
        let path = Path { at, v, pull };
        let (end, land) = path
            .landing(basins, WALK_MOST)
            .map_or((life, Vec4::ZERO), |(t, p)| (t, p.extend(1.0)));
        out.push(piece(
            &path,
            end,
            STREAM_RADIUS * 1.2,
            0.02,
            hash(seed),
            STREAM | flags,
            land,
        ));
    }
    out
}

fn sees(planes: &[Vec4], at: Vec3, r: f32) -> bool {
    planes.iter().all(|q| q.truncate().dot(at) + q.w >= -r)
}

/// The fountains' jets seen from `eye` inside `planes` (their cells' rectangles not empty, as
/// `seen` says), as `fountains.wgsl` reads them: the header (how many, the clock) and the pieces.
/// Nothing when jets are drawn as retail's bubbles.
#[must_use]
pub fn frame_jets(
    particles: &Particles,
    eye: Vec3,
    planes: &[Vec4],
    seen: impl Fn(u32) -> bool,
    clock: f64,
) -> Vec<u8> {
    let mut pieces: Vec<[u8; PIECE]> = Vec::new();
    if particles.fountains {
        for (n, h) in particles.hosts.iter().enumerate() {
            if h.spills.is_empty() && !h.driver.particles.iter().any(|e| is_jet(&e.info)) {
                continue;
            }
            if !seen(h.cell) {
                continue;
            }
            let host = crate::asset::object::frame_affine(&h.driver.particle_parent_frame());
            let origin = Vec3::from(host.translation);
            if origin.distance(eye) > JET_REACH || !sees(planes, origin, 4.0) {
                continue;
            }
            let basins: Vec<Basin> = h
                .pools
                .iter()
                .filter_map(|f| {
                    let p = h.driver.part_array.parts.get(f.part)?;
                    Some(Basin::new(
                        crate::body::part_affine(&p.pos, p.gfxobj_scale),
                        f,
                    ))
                })
                .collect();
            let flags = if h.cell != 0 { INDOOR } else { 0 };
            for (k, s) in h.spills.iter().enumerate() {
                let Some(p) = h.driver.part_array.parts.get(s.part) else {
                    continue;
                };
                let at = crate::body::part_affine(&p.pos, p.gfxobj_scale).transform_point3(s.at);
                let path = Path {
                    at,
                    v: Vec3::ZERO,
                    pull: 0.0,
                };
                let seed = u32::try_from(n * 16 + k).unwrap_or(0) ^ 0x00C0_FFEE;
                pieces.push(piece(
                    &path,
                    0.0,
                    0.0,
                    0.0,
                    hash(seed),
                    RING | flags,
                    at.extend(SPILL_RING),
                ));
            }
            for (k, e) in h.driver.particles.iter().enumerate() {
                let info = &e.info;
                if !is_jet(info) {
                    continue;
                }
                let start = match h.driver.part_array.parts.get(e.part_index as usize) {
                    Some(p) if e.part_index != pyreal_animation::particles::NO_PART => {
                        crate::asset::object::frame_affine(&p.pos)
                    }
                    _ => host,
                };
                let local = matches!(
                    info.particle_type,
                    ParticleType::ParabolicLvga
                        | ParticleType::ParabolicLvgagr
                        | ParticleType::ParabolicLvla
                        | ParticleType::ParabolicLvlalr
                );
                let a = Vec3::new(info.a.x, info.a.y, info.a.z) * mid(info.min_a, info.max_a);
                let v = if local { start.transform_vector3(a) } else { a };
                let b = Vec3::new(info.b.x, info.b.y, info.b.z) * mid(info.min_b, info.max_b);
                let b = if matches!(
                    info.particle_type,
                    ParticleType::ParabolicLvla | ParticleType::ParabolicLvlalr
                ) {
                    start.transform_vector3(b)
                } else {
                    b
                };
                let o = e.object_origin;
                let at = Vec3::new(o.x, o.y, o.z)
                    + start.transform_vector3(
                        Vec3::new(info.offset_dir.x, info.offset_dir.y, info.offset_dir.z)
                            * mid(info.min_offset, info.max_offset),
                    );
                let seed = u32::try_from(n * 16 + k).unwrap_or(0);
                #[allow(clippy::cast_possible_truncation)] // seconds of a bubble's life
                let life = info.lifespan as f32;
                pieces.extend(jet_pieces(at, v, b.z, life, &basins, seed, flags));
            }
        }
    }
    pieces.truncate(MAX_FRAME_JETS);
    let mut out = Vec::with_capacity(HEAD + pieces.len() * PIECE);
    let count = u32::try_from(pieces.len()).unwrap_or(0);
    #[allow(clippy::cast_possible_truncation)] // wrapped to ten minutes
    let wrapped = clock.rem_euclid(CLOCK_WRAP) as f32;
    for w in [count, wrapped.to_bits(), 0, 0] {
        out.extend_from_slice(&w.to_le_bytes());
    }
    for p in &pieces {
        out.extend_from_slice(p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bubbles() -> ParticleEmitterInfo {
        ParticleEmitterInfo {
            particle_type: ParticleType::ParabolicLvgagr,
            hw_gfxobj_id: DataId(0x0100_16C0),
            a: pyreal_primitives::Vec3::new(0.0, 0.0, 3.0),
            min_a: 1.0,
            max_a: 1.1,
            b: pyreal_primitives::Vec3::new(0.0, 0.0, -3.0),
            min_b: 1.0,
            max_b: 1.3,
            lifespan: 1.4,
            ..ParticleEmitterInfo::default()
        }
    }

    fn word(p: &[u8], k: usize) -> f32 {
        f32::from_le_bytes([p[k * 4], p[k * 4 + 1], p[k * 4 + 2], p[k * 4 + 3]])
    }

    #[test]
    fn a_fountain_is_told_by_its_emitters() {
        assert!(is_jet(&bubbles()));
        let mut spark = bubbles();
        spark.hw_gfxobj_id = DataId(0x0100_173E);
        assert!(
            !is_jet(&spark) && is_spray(&spark),
            "the spray's glints are no jet"
        );
        let mut once = bubbles();
        once.total_particles = 20;
        assert!(!is_jet(&once), "a burst is no fountain");
        let mut rising = bubbles();
        rising.b.z = 1.0;
        assert!(!is_jet(&rising), "water falls");
    }

    #[test]
    fn an_upright_jet_rises_to_its_top_and_falls_back_into_its_basin_as_a_crown() {
        let basin = Basin::new(
            Affine3A::from_translation(Vec3::new(10.0, 20.0, 0.0)),
            &PoolFace {
                part: 0,
                min: Vec2::splat(-0.6),
                max: Vec2::splat(0.6),
                z: 0.4,
            },
        );
        let at = Vec3::new(10.0, 20.0, 1.2);
        let pieces = jet_pieces(at, Vec3::new(0.0, 0.0, 3.15), -3.45, 1.4, &[basin], 7, 0);
        assert_eq!(pieces.len(), 1 + CROWN);
        // The column stops where the water stops rising, 3.15² / (2 × 3.45) over the spout.
        let column = &pieces[0];
        let top_t = word(column, 7);
        assert!((top_t - 3.15 / 3.45).abs() < 1e-4);
        for crown in &pieces[1..] {
            assert!((word(crown, 2) - (1.2 + 3.15 * 3.15 / 6.9)).abs() < 1e-3);
            // Each lands on the water, inside the basin, away from the column.
            let land = Vec3::new(word(crown, 12), word(crown, 13), word(crown, 14));
            assert!((word(crown, 15) - 1.0).abs() < 1e-6, "it rings the water");
            assert!((land.z - 0.4).abs() < 0.05, "{land}");
            let out = land.truncate().distance(Vec2::new(10.0, 20.0));
            assert!((0.1..0.6).contains(&out), "{out}");
        }
    }

    #[test]
    fn water_off_a_lip_falls_to_the_basin_below() {
        let basin = Basin::new(
            Affine3A::IDENTITY,
            &PoolFace {
                part: 0,
                min: Vec2::splat(-1.4),
                max: Vec2::splat(1.4),
                z: 0.61,
            },
        );
        let at = Vec3::new(0.0, 1.0, 1.3);
        let pieces = jet_pieces(at, Vec3::new(0.0, 0.2, 0.0), -3.0, 0.7, &[basin], 1, INDOOR);
        assert_eq!(pieces.len(), 1);
        let t = word(&pieces[0], 7);
        // 0.69 m down at 3 m/s²: about 0.68 s.
        assert!((t - (2.0f32 * 0.69 / 3.0).sqrt()).abs() < 0.02, "{t}");
        assert!((word(&pieces[0], 14) - 0.61).abs() < 0.02);
        #[allow(clippy::cast_precision_loss)]
        let kind = (STREAM | INDOOR) as f32;
        assert!((word(&pieces[0], 11) - kind).abs() < 1e-6);
    }

    #[test]
    fn jets_are_not_drawn_while_retail_draws_them() {
        let particles = Particles::default();
        assert!(
            !particles.fountains,
            "drawn as retail's until the frame says"
        );
        let bytes = frame_jets(&particles, Vec3::ZERO, &[], |_| true, 3.0);
        assert_eq!(bytes.len() as u64, JET_HEAD);
        assert_eq!(&bytes[..4], &0u32.to_le_bytes());
    }
}
