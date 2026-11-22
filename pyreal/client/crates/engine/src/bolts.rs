//! Bolts: lightning in a creature's cloud (a Pillar of Lightning's), drawn as electrical discharge.
//!
//! A body whose particles make a volume ([`crate::particles::Look::Puff`]) and whose other
//! emitters throw short-lived, slender, added pictures ([`is_bolt`]) has lightning. Those
//! particles are not drawn: each only says when and where a strike happens. At its first sight a
//! strike grows a channel from there ([`Strike::grow`]): a jagged path, split again and again
//! with sideways kicks, forking as it goes, held fixed to its body for the strike's life. Each
//! frame its brightness follows retail's life of the particle through a strike's strokes
//! ([`flash`]), and it is handed to the GPU three ways:
//! - its segments, drawn by `bolts.wgsl` as thin hot cores with a glow, dimmed by the cloud
//!   between them and the eye ([`Particles::bolts`](crate::particles::Particles::bolts));
//! - its chord, a line of light at the tail of its body's volume run, which the volume's march
//!   adds as light scattered from within (`bolt_light` in `bolts.wgsl`);
//! - a lamp at its middle for a few frames, lighting the ground and what stands near
//!   ([`Particles::flashes`](crate::particles::Particles::flashes)).
//!
//! A volume kind uses it by calling `bolt_light(x, first, count)` in its march and
//! `puff_count(first, count)` for its balls (strike lines trail a run, flag 1).

use glam::Vec3;
use pyreal_primitives::num::math::{cosf, sinf};

use crate::particles::Sprite;

/// The longest-lived particle (seconds) taken for a strike.
const BOLT_LONGEST: f64 = 1.0;
/// The least a strike lasts, seconds, whatever its particle's life (a golem's last a frame), and
/// how far apart strikes born together are spread.
pub const STRIKE_LEAST: f32 = 0.45;
pub const STRIKE_SPREAD: f32 = 0.35;
/// The most strikes alive in one body, and in all: those past them are not struck.
pub const BODY_MOST: usize = 6;
pub const MOST: usize = 48;

/// How long after its particle's birth a strike seeded `seed` comes, seconds: strikes born
/// together (a golem's, one each third of a metre it walks) crackle one after another.
#[must_use]
pub fn delay(seed: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // eight bits
    let h = ((seed >> 5) & 0xFF) as f32 / 255.0;
    h * STRIKE_SPREAD
}

/// Whether an emitter throws lightning, on a body with a volume: short-lived, long and thin,
/// added pictures.
#[must_use]
pub fn is_bolt(sprite: &Sprite, e: &pyreal_animation::data::ParticleEmitterInfo) -> bool {
    sprite.additive() && sprite.slender && e.lifespan <= BOLT_LONGEST
}

/// A strike's strokes: when each comes (a share of its life, and how far a seed moves it) and how
/// bright it is; how short a stroke's flash is (of its life); and the glow left after.
const STROKES: [(f32, f32, f32); 3] = [(0.0, 0.0, 1.0), (0.22, 0.1, 0.75), (0.45, 0.15, 0.55)];
const STROKE: f32 = 0.05;
const AFTERGLOW: f32 = 0.1;

/// How bright a strike is at `age` (0 to 1 of its life): a first stroke and later ones, each a
/// flash dying away over a frame or two, over a dimming afterglow; `seed` moves the later ones.
#[must_use]
pub fn flash(age: f32, seed: u32) -> f32 {
    let mut f = AFTERGLOW * (1.0 - age).max(0.0);
    for (k, (at, shift, strength)) in STROKES.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)] // four bits
        let h = ((seed >> (4 * k)) & 15) as f32 / 15.0;
        let x = age - (at + shift * h);
        if x >= 0.0 {
            f += strength * pyreal_primitives::num::math::expf(-x / STROKE);
        }
    }
    f
}

/// How long a strike's channel is over its particle's picture (its length at its first scale);
/// how many times the main channel is halved, and a fork's; how far a halving kicks its middle
/// sideways, over the piece's length; how likely a joint is to fork, and how far a fork turns.
const REACH: f32 = 1.4;
const SPLITS: u32 = 5;
const FORK_SPLITS: u32 = 4;
/// A piece's width marking it as the wide glow's only.
pub const GLOW_ONLY: f32 = -1.0;
/// How many pieces a channel's wide glow is drawn along.
const GLOW_PIECES: usize = 4;

/// The wide glow's pieces along `path`, `share` of the strike's brightness: a few of its joints
/// joined, finer than the glow can show.
fn glow_along(path: &[Vec3], share: f32, out: &mut Vec<Piece>) {
    let last = path.len().saturating_sub(1);
    let mut from = 0;
    for k in 1..=GLOW_PIECES {
        let to = last * k / GLOW_PIECES;
        if to > from {
            out.push(Piece {
                a: path[from],
                b: path[to],
                width: GLOW_ONLY,
                share,
            });
            from = to;
        }
    }
}

/// The shortest piece a channel is halved to, metres: a short strike is halved fewer times.
const PIECE_LEAST: f32 = 0.15;

/// How many times a channel `len` metres long is halved, at most `most`.
fn splits(len: f32, most: u32) -> u32 {
    let mut n = 1;
    while n < most && len / f32::from(1u16 << n) > PIECE_LEAST {
        n += 1;
    }
    n
}
const KICK: f32 = 0.32;
const FORKING: f32 = 0.18;
const FORK_TURN: (f32, f32) = (0.45, 1.0);
/// A channel's core width, metres, and a fork's share of its parent's width and brightness.
const CORE: f32 = 0.045;
const FORK_SHARE: f32 = 0.6;

/// One piece of a channel, relative to its body: its ends, its width and its brightness's share.
/// A piece of no width ([`GLOW_ONLY`]) carries only the channel's wide glow, along a coarse run
/// of its joints, so the fine pieces' cards stay narrow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    pub a: Vec3,
    pub b: Vec3,
    pub width: f32,
    pub share: f32,
}

/// A strike: its channel's pieces and its chord (start and end), relative to its body, and its
/// colour (its picture's, whitened).
#[derive(Debug, Clone, PartialEq)]
pub struct Strike {
    pub pieces: Vec<Piece>,
    pub chord: (Vec3, Vec3),
    pub colour: Vec3,
    /// When it strikes (the clock's seconds), how long it lasts, the body it is in, and how
    /// strong it is (its particle's opacity at birth).
    pub born: f64,
    pub life: f32,
    pub body: usize,
    pub strength: f32,
}

/// A small generator of its own, so a strike grows the same way from its seed.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        #[allow(clippy::cast_precision_loss)] // 24 bits
        let v = (self.0 >> 8) as f32 / 16_777_216.0;
        v
    }

    fn signed(&mut self) -> f32 {
        self.next() * 2.0 - 1.0
    }
}

/// A unit at right angles to `d`, turned `t` round it.
fn across(d: Vec3, t: f32) -> Vec3 {
    let u = d.any_orthonormal_vector();
    let v = d.cross(u);
    u * cosf(t) + v * sinf(t)
}

/// A path from `a` to `b` split `splits` times, each middle kicked sideways.
fn channel(rng: &mut Rng, a: Vec3, b: Vec3, splits: u32) -> Vec<Vec3> {
    let mut points = vec![a, b];
    for _ in 0..splits {
        let mut next = Vec::with_capacity(points.len() * 2);
        for w in points.windows(2) {
            let (p, q) = (w[0], w[1]);
            let d = q - p;
            let len = d.length().max(1e-4);
            let kick =
                across(d / len, rng.next() * std::f32::consts::TAU) * len * KICK * rng.signed();
            next.push(p);
            next.push((p + q) * 0.5 + kick);
        }
        next.push(*points.last().unwrap_or(&b));
        points = next;
    }
    points
}

impl Strike {
    /// The strike a particle seeded `seed` makes from `start` (relative to its body), its picture
    /// `len` metres long and coloured `colour`: a channel mostly downward or across, forking.
    #[must_use]
    pub fn grow(seed: u32, start: Vec3, len: f32, colour: Vec3) -> Self {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9) | 1);
        let len = len * REACH * (0.7 + 0.6 * rng.next());
        // Down through the cloud more often than across it.
        let down = rng.next() < 0.6;
        let dir = Vec3::new(
            rng.signed(),
            rng.signed(),
            if down {
                -1.2 - rng.next()
            } else {
                rng.signed() * 0.4
            },
        )
        .normalize_or(Vec3::NEG_Z);
        let end = start + dir * len;
        let mut pieces = Vec::new();
        let main = channel(&mut rng, start, end, splits(len, SPLITS));
        for w in main.windows(2) {
            pieces.push(Piece {
                a: w[0],
                b: w[1],
                width: CORE,
                share: 1.0,
            });
        }
        glow_along(&main, 1.0, &mut pieces);
        // Forks from its joints, each a shorter channel turned off the main one, with forks of
        // their own now and then.
        let mut forks = vec![(main, CORE, 1.0f32, 0u32)];
        while let Some((path, width, share, depth)) = forks.pop() {
            if depth >= 2 {
                continue;
            }
            let n = path.len();
            for k in 1..n.saturating_sub(2) {
                if rng.next() > FORKING / (1.0 + depth as f32) {
                    continue;
                }
                let along = (path[k + 1] - path[k]).normalize_or(dir);
                let turn = FORK_TURN.0 + (FORK_TURN.1 - FORK_TURN.0) * rng.next();
                let side = across(along, rng.next() * std::f32::consts::TAU);
                let fdir = (along * cosf(turn) + side * sinf(turn)).normalize_or(along);
                #[allow(clippy::cast_precision_loss)] // a few dozen joints
                let left = 1.0 - k as f32 / n as f32;
                let flen = len * left * (0.25 + 0.3 * rng.next());
                let fpath = channel(
                    &mut rng,
                    path[k],
                    path[k] + fdir * flen,
                    splits(flen, FORK_SPLITS - depth),
                );
                let (fw, fs) = (width * FORK_SHARE, share * FORK_SHARE);
                for w in fpath.windows(2) {
                    pieces.push(Piece {
                        a: w[0],
                        b: w[1],
                        width: fw,
                        share: fs,
                    });
                }
                glow_along(&fpath, fs, &mut pieces);
                forks.push((fpath, fw, fs, depth + 1));
            }
        }
        Self {
            pieces,
            chord: (start, end),
            colour: (colour / colour.max_element().max(1e-4)).lerp(Vec3::ONE, 0.25),
            born: 0.0,
            life: STRIKE_LEAST,
            body: 0,
            strength: 1.0,
        }
    }
}

/// How a strike's lamp swells and dies over its life (shares of it): smoothly, so the cloud and
/// the ground glow with it rather than strobing with each stroke.
const LAMP_SWELL: f32 = 0.06;
const LAMP_FADE: f32 = 0.3;

/// How much of a full flash a strike's lamp is at its brightest: a glow in the cloud.
const LAMP_SHARE: f32 = 0.3;

/// How bright a strike's lamp is at `age` (0 to 1 of its life), of a full flash.
#[must_use]
pub fn glow(age: f32) -> f32 {
    let e = pyreal_primitives::num::math::expf;
    LAMP_SHARE * (1.0 - e(-age / LAMP_SWELL)) * e(-age / LAMP_FADE) * (1.0 - age).clamp(0.0, 1.0)
}

/// A strike's lamp: how bright at full flash (the client's intensity) and how far it reaches,
/// metres.
const LAMP_INTENSITY: f32 = 40.0;
const LAMP_FALLOFF: f32 = 10.0;

/// The lamp a strike lights its surroundings with at `at`, `flash` of its full brightness.
#[must_use]
pub fn lamp(at: Vec3, colour: Vec3, flash: f32, cell: u32) -> crate::lamps::Lamp {
    let room = crate::renderer::ROOM_LUX / crate::sky::SCENE_LUX;
    let range = LAMP_FALLOFF * crate::lamps::RANGE_OF_FALLOFF;
    crate::lamps::Lamp {
        position: at,
        colour,
        candela: LAMP_INTENSITY * flash * room,
        range,
        retail: (colour * LAMP_INTENSITY * flash).extend(range),
        intensity: LAMP_INTENSITY * flash,
        client: None,
        flicker: crate::lamps::Flicker::Steady,
        shadows: false,
        cell,
        kind: crate::lamps::LampKind::Authored,
        object: u32::MAX,
        flash: true,
        held: false,
        seed: 0,
    }
}
