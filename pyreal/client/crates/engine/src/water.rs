//! The water: its own surface and pass, laid where retail lays its water
//! ([`crate::land`]), drawn over the lit world before the translucent surfaces.
//!
//! - **Shape**: the surface lies about retail's level; a lake's chop and the sea's swell
//!   ([`wave_constants`], render-hifi's, their phases worked out here in double precision so they
//!   run at deep water's speed in real seconds), the swell turned to run in to the shore, and three
//!   drifting octaves of ripples, each left out (and kept as roughness) once a pixel is too coarse
//!   for it; near the eye the longer waves give the surface its height ([`height_at`]). Near the
//!   shore the sea's long swell and a share of a wide lake's chop run in as shore waves
//!   ([`ShoreWave`], [`shore_table`]), closing up over the made-up bed; all shrink before the
//!   shore. Each shore wave breaks in foam on its front, and the sea's runs up a flat shore as
//!   swash and slides back, its edge moving and leaving the ground it uncovers wet.
//! - **Disturbance**: near the eye the water is simulated ([`Sim`], render-hifi's iWave ported):
//!   bodies wading push it aside and leave a bow wave and rings; rain drops are plumbed in.
//! - **Optics**: the bed (drawn by the ground at the level, its water picture swapped for silt)
//!   is read a made-up depth down, falling away from the edge to the body's deepest, the
//!   refracted ray bending where its picture is read; anything standing in the water is read at
//!   its own depth. Both fade into the water's colour by what each metre absorbs. The mirror is
//!   the frame along the reflected ray (a march through the depth buffer), else the probes' light
//!   and the sky with its cloud deck, by Fresnel. The sun glints by GGX, shadowed. Foam gathers
//!   along the edge where the waves break, round what stands in the water and where the
//!   simulation laps a body's legs. The ground darkens and shines in a band just over the level.
//! - **Looks**: each body takes a [`WaterLook`] from a small table: a clear lake, a river, a
//!   swamp, a shallow sea and the deep sea by default, picked by its water's terrain types and its
//!   shore's ([`WaterTable::pick`]). The pack holds none yet; the format it would add them in is
//!   [`WaterLook`]'s, as a `[[water]]` table:
//!
//!   ```toml
//!   [[water]]
//!   name = "peat bog"
//!   grounds = [16, 17]           # the water's terrain types it describes
//!   shores = [4, 29]             # only bodies whose shore is mostly these (any when absent)
//!   absorb = [1.4, 1.6, 2.4]     # what a metre takes of red, green and blue
//!   colour = [0.05, 0.04, 0.015] # the deep water's own colour, linear
//!   open = [0.05, 0.04, 0.015]   # the open water's own colour, far from the shore
//!   depth = 1.2                  # the bed's deepest, metres
//!   shelf = 10.0                 # metres in from the edge to the deepest
//!   waves = 0.1                  # the waves' size, 1 a lake's
//!   ripples = 0.5                # the ripples' size
//!   flow = 0.0                   # metres a second it drifts with the wind
//!   foam = 0.1                   # how much foam gathers, 0 to 1
//!   roughness = 0.08
//!   far_roughness = 0.14         # how rough it reads far off
//!   swash = 0.1                  # metres its swash runs up a flat shore
//!   calm = 1.0                   # how much its chop is a pond's, short and slow, 0 to 1
//!   shore = 0.0                  # how much of its chop turns to run in to the shore, 0 to 1
//!   ```
//!
//! Only pixels change: the ground, physics and what can be picked keep retail's surface. Under
//! the retail calibration the pass does not run and retail's picture is drawn as it always was.

use glam::{Vec2, Vec3, Vec4, Vec4Swizzles};
use pyreal_primitives::num::math::{cos, exp};

use crate::gpu::Gpu;
use crate::land::LandField;

/// One body's look.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterLook {
    pub name: &'static str,
    /// The water terrain types (16 to 20) it describes.
    pub grounds: &'static [u32],
    /// The shore's terrain types that make a body this one, when most of its shore is them;
    /// empty for any shore.
    pub shores: &'static [u32],
    /// What a metre of it takes of red, green and blue.
    pub absorb: [f32; 3],
    /// The deep water's own colour, linear.
    pub colour: [f32; 3],
    /// The open water's own colour, linear, taking over from `colour` two to four shelves out.
    pub open: [f32; 3],
    /// How rough it reads far off, past what a pixel holds of its waves.
    pub far_roughness: f32,
    /// The bed's deepest, metres, and how far in from the edge it is reached.
    pub depth: f32,
    pub shelf: f32,
    pub waves: f32,
    pub ripples: f32,
    pub flow: f32,
    pub foam: f32,
    pub roughness: f32,
    /// How much of the sea's swell it carries, 0 to 1 (it turns to run in to the shore).
    pub swell: f32,
    /// How much surf rolls in to its shore, 0 to 1.
    pub surf: f32,
    /// How far its swash runs up a flat shore past the water's edge, metres, on a wide body in a
    /// stiff breeze.
    pub swash: f32,
    /// How much of its chop is a pond's (short, slow waves) rather than a lake's, 0 to 1; a body
    /// reaching far enough for waves takes the lake's back.
    pub calm: f32,
    /// How much of its chop turns to run in to the shore as shore waves, 0 to 1 (a lake's; the
    /// sea's swell turns of itself).
    pub shore: f32,
}

/// The ground types a swampy shore is made of: marsh, mud, sea slime, moss and dark moss.
const SWAMP_SHORES: &[u32] = &[4, 5, 23, 28, 29];

/// The looks the engine knows without a pack.
#[must_use]
pub fn default_looks() -> Vec<WaterLook> {
    vec![
        WaterLook {
            name: "clear lake",
            grounds: &[17],
            shores: &[],
            absorb: [0.42, 0.075, 0.055],
            colour: [0.010, 0.040, 0.050],
            open: [0.010, 0.040, 0.050],
            depth: 3.5,
            shelf: 24.0,
            waves: 1.0,
            ripples: 1.0,
            flow: 0.0,
            foam: 0.45,
            roughness: 0.035,
            far_roughness: 0.14,
            swell: 0.0,
            surf: 0.3,
            swash: 0.5,
            calm: 1.0,
            shore: 0.35,
        },
        WaterLook {
            name: "river",
            grounds: &[16],
            shores: &[],
            absorb: [0.5, 0.11, 0.08],
            colour: [0.015, 0.045, 0.045],
            open: [0.015, 0.045, 0.045],
            depth: 1.8,
            shelf: 12.0,
            waves: 0.3,
            ripples: 1.2,
            flow: 0.5,
            foam: 0.6,
            roughness: 0.05,
            far_roughness: 0.14,
            swell: 0.0,
            surf: 0.0,
            swash: 0.35,
            calm: 1.0,
            shore: 0.0,
        },
        WaterLook {
            name: "swamp",
            grounds: &[17, 19],
            shores: SWAMP_SHORES,
            absorb: [0.95, 0.38, 0.5],
            colour: [0.012, 0.034, 0.026],
            open: [0.012, 0.034, 0.026],
            depth: 1.2,
            shelf: 10.0,
            waves: 0.2,
            ripples: 1.0,
            flow: 0.0,
            foam: 0.12,
            roughness: 0.09,
            far_roughness: 0.14,
            swell: 0.0,
            surf: 0.0,
            swash: 0.04,
            calm: 1.0,
            shore: 0.0,
        },
        WaterLook {
            name: "shallow sea",
            grounds: &[18, 19],
            shores: &[],
            absorb: [0.38, 0.06, 0.05],
            colour: [0.006, 0.050, 0.060],
            open: [0.003, 0.010, 0.040],
            depth: 4.0,
            shelf: 40.0,
            waves: 1.0,
            ripples: 1.0,
            flow: 0.0,
            foam: 1.0,
            roughness: 0.04,
            far_roughness: 0.3,
            swell: 1.0,
            surf: 1.0,
            swash: 2.2,
            calm: 0.0,
            shore: 0.0,
        },
        WaterLook {
            name: "deep sea",
            grounds: &[20],
            shores: &[],
            absorb: [0.45, 0.08, 0.05],
            colour: [0.004, 0.030, 0.060],
            open: [0.003, 0.010, 0.040],
            depth: 12.0,
            shelf: 60.0,
            waves: 1.3,
            ripples: 1.0,
            flow: 0.0,
            foam: 1.0,
            roughness: 0.045,
            far_roughness: 0.3,
            swell: 1.0,
            surf: 1.0,
            swash: 2.4,
            calm: 0.0,
            shore: 0.0,
        },
    ]
}

/// The looks bodies are given: the pack's (none yet) before the defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterTable {
    pub looks: Vec<WaterLook>,
}

impl Default for WaterTable {
    fn default() -> Self {
        Self {
            looks: default_looks(),
        }
    }
}

/// The share of a body's shore that must be a look's shore types for the look to take it.
const SHORE_SHARE: f32 = 0.45;

impl WaterTable {
    /// The look of a body whose water vertices have terrain types counted in `water` and whose
    /// shore's in `shore`: the first look with shore types that most of the shore is and whose
    /// grounds hold the body's commonest water type, else the first look of that type, else the
    /// first.
    #[must_use]
    pub fn pick(&self, water: &[u32; 32], shore: &[u32; 32]) -> usize {
        let commonest = (0..32u32).max_by_key(|&t| water[t as usize]).unwrap_or(17);
        let shore_total: u32 = shore.iter().sum();
        #[allow(clippy::cast_precision_loss)]
        let share = |types: &[u32]| {
            let n: u32 = types.iter().map(|&t| shore[t as usize]).sum();
            n as f32 / shore_total.max(1) as f32
        };
        self.looks
            .iter()
            .position(|l| {
                !l.shores.is_empty()
                    && l.grounds.contains(&commonest)
                    && share(l.shores) >= SHORE_SHARE
            })
            .or_else(|| {
                self.looks
                    .iter()
                    .position(|l| l.shores.is_empty() && l.grounds.contains(&commonest))
            })
            .unwrap_or(0)
    }

    /// The looks as the shader reads them: six vectors each.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for l in &self.looks {
            let (kind, wave) = shore_wave(l);
            let (k, a) = wave.map_or((0.0, 0.0), |w| (w.wavenumber(), w.amplitude));
            for v in [
                l.absorb[0],
                l.absorb[1],
                l.absorb[2],
                l.depth,
                l.colour[0],
                l.colour[1],
                l.colour[2],
                l.shelf,
                l.waves,
                l.ripples,
                l.flow,
                l.foam,
                l.roughness,
                l.swell,
                l.surf,
                l.calm,
                l.open[0],
                l.open[1],
                l.open[2],
                l.far_roughness,
                l.shore,
                kind,
                k,
                a,
            ] {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out
    }

    /// Each look's shore wave's phase gained running in from the made-up bed's distances
    /// ([`shore_table`]).
    #[must_use]
    pub fn shore_table(&self) -> Vec<f32> {
        shore_table(&self.looks)
    }
}

/// A shore wave: crests that follow the shore's outline, closing up and
/// slowing as the made-up bed rises, in place of the open water's waves of the same period.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShoreWave {
    /// Its length in deep water, metres, and its height for a `waves` of one (the waves it
    /// replaces, of equal power).
    pub length: f32,
    pub amplitude: f32,
}

impl ShoreWave {
    /// Its wavenumber in deep water, radians a metre.
    #[must_use]
    pub fn wavenumber(&self) -> f32 {
        std::f32::consts::TAU / self.length
    }

    /// Radians a second.
    #[must_use]
    pub fn omega(&self) -> f64 {
        (GRAVITY_F64 * std::f64::consts::TAU / f64::from(self.length)).sqrt()
    }
}

const GRAVITY_F64: f64 = 9.81;

/// The equal-power height of `waves` for a `waves` of one.
fn equal_power(waves: &[Wave]) -> f32 {
    waves
        .iter()
        .map(|w| {
            let a = w.steepness * w.length / std::f32::consts::TAU;
            a * a
        })
        .sum::<f32>()
        .sqrt()
}

/// The sea's shore wave: its long swell, at the longest's period.
#[must_use]
pub fn sea_shore_wave() -> ShoreWave {
    ShoreWave {
        length: LONG_SWELL[0].length,
        amplitude: equal_power(&LONG_SWELL),
    }
}

/// A lake's shore wave: its chop, at the longest's period.
#[must_use]
pub fn lake_shore_wave() -> ShoreWave {
    ShoreWave {
        length: WAVES[0].length,
        amplitude: equal_power(&WAVES[..CHOP]),
    }
}

/// Which shore wave a look has (1 the sea's, 2 a lake's, 0 none) and it.
#[must_use]
pub fn shore_wave(look: &WaterLook) -> (f32, Option<ShoreWave>) {
    if look.swell > 0.0 {
        (1.0, Some(sea_shore_wave()))
    } else if look.shore > 0.0 {
        (2.0, Some(lake_shore_wave()))
    } else {
        (0.0, None)
    }
}

/// The shore waves' table: metres between its entries, and entries a look (then the wavenumber
/// past the last).
pub const SHORE_STEP: f32 = 0.5;
pub const SHORE_STEPS: usize = 257;
pub const SHORE_ROW: usize = SHORE_STEPS + 1;

/// The wavenumber of a wave of `omega` radians a second over water `depth` metres deep
/// (`ω² = g k tanh(k h)`, by Newton's method).
#[must_use]
pub fn wavenumber(omega: f64, depth: f64) -> f64 {
    let h = depth.max(1e-4);
    let deep = omega * omega / GRAVITY_F64;
    let mut k = deep.max(omega / (GRAVITY_F64 * h).sqrt());
    for _ in 0..30 {
        let t = 1.0 - 2.0 / (exp(2.0 * k * h) + 1.0);
        let f = GRAVITY_F64 * k * t - omega * omega;
        let df = GRAVITY_F64 * (t + k * h * (1.0 - t * t));
        let next = (k - f / df).max(deep);
        if (next - k).abs() < 1e-10 * k {
            return next;
        }
        k = next;
    }
    k
}

/// The made-up bed's depth `inside` metres in from the water's edge (`land.wgsl` `bed_depth`).
#[must_use]
pub fn bed_depth(inside: f32, depth: f32, shelf: f32) -> f32 {
    let t = 1.0 - (inside / shelf.max(1e-3)).clamp(0.0, 1.0);
    depth * (1.0 - t * t)
}

/// For each look, the phase its shore wave gains running in to the edge from each [`SHORE_STEP`]
/// out over its made-up bed, `K(d) = ∫₀ᵈ k(h(s)) ds` (radians), then its wavenumber past the
/// last entry; nought for a look with none.
#[must_use]
pub fn shore_table(looks: &[WaterLook]) -> Vec<f32> {
    let mut out = Vec::with_capacity(looks.len() * SHORE_ROW);
    for l in looks {
        let Some(wave) = shore_wave(l).1 else {
            out.extend(std::iter::repeat_n(0.0, SHORE_ROW));
            continue;
        };
        let omega = wave.omega();
        let k = |s: f64| {
            #[allow(clippy::cast_possible_truncation)]
            let h = bed_depth(s as f32, l.depth, l.shelf);
            wavenumber(omega, f64::from(h))
        };
        let sub = 32;
        let ds = f64::from(SHORE_STEP) / f64::from(sub);
        let mut total = 0.0f64;
        for i in 0..SHORE_STEPS {
            #[allow(clippy::cast_possible_truncation)]
            out.push(total as f32);
            #[allow(clippy::cast_precision_loss)]
            let from = i as f64 * f64::from(SHORE_STEP);
            for j in 0..sub {
                total += k(from + (f64::from(j) + 0.5) * ds) * ds;
            }
        }
        #[allow(clippy::cast_possible_truncation)]
        out.push(k(1.0e4) as f32);
    }
    out
}

/// The shore waves' phase at the water's edge at `clock` seconds, before their wander along it:
/// the sea's (x) and a lake's (y), radians.
#[must_use]
pub fn shore_phases(clock: f64) -> [f32; 2] {
    let tau = std::f64::consts::TAU;
    #[allow(clippy::cast_possible_truncation)]
    let at = |omega: f64, seed: f64| (seed - omega * clock).rem_euclid(tau) as f32;
    [
        at(tau / SURF_PERIOD, 0.7),
        at(lake_shore_wave().omega(), 2.1),
    ]
}

/// Where on a shore wave its breaking foam's front lies, radians past its crest.
pub const SHORE_FRONT: f64 = std::f64::consts::FRAC_PI_2 + 0.3;

/// One wave: metres from crest to crest, its turn from its heading (radians), and the most slope
/// it gives the surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wave {
    pub length: f32,
    pub turn: f32,
    pub steepness: f32,
}

/// How many headings the swell is laid toward, evenly round from east.
pub const HEADINGS: usize = 8;
/// How many waves of chop, of swell a heading, and how many there are: the chop, the swell toward
/// each heading, then a pond's chop.
pub const CHOP: usize = 4;
pub const PER_HEADING: usize = 6;
pub const CALM: usize = 4;
pub const WAVE_COUNT: usize = CHOP + PER_HEADING * HEADINGS + CALM;
/// The share of deep water's speed a pond's chop runs at.
pub const CALM_PACE: f64 = 0.3;
/// The heading the chop and the ripples run along, east and north.
pub const WIND: (f32, f32) = (0.82, 0.57);
/// How long between the sea's shore waves reaching the shore, seconds: the long swell's period
/// (64 m in deep water).
pub const SURF_PERIOD: f64 = 6.402_55;

/// A lake's chop (turned from the wind), then the sea's swell (turned from its heading).
pub const WAVES: [Wave; 8] = [
    Wave {
        length: 8.5,
        turn: 0.0,
        steepness: 0.10,
    },
    Wave {
        length: 5.3,
        turn: 0.55,
        steepness: 0.09,
    },
    Wave {
        length: 3.4,
        turn: -0.7,
        steepness: 0.08,
    },
    Wave {
        length: 2.2,
        turn: 1.2,
        steepness: 0.06,
    },
    Wave {
        length: 26.0,
        turn: 0.1,
        steepness: 0.07,
    },
    Wave {
        length: 17.0,
        turn: -0.35,
        steepness: 0.07,
    },
    Wave {
        length: 11.5,
        turn: 0.45,
        steepness: 0.06,
    },
    Wave {
        length: 7.8,
        turn: -0.9,
        steepness: 0.05,
    },
];

/// The swell's long waves (turned from its heading), which give the sea metres of height at gentle
/// slopes.
pub const LONG_SWELL: [Wave; 2] = [
    Wave {
        length: 64.0,
        turn: 0.05,
        steepness: 0.055,
    },
    Wave {
        length: 41.0,
        turn: -0.2,
        steepness: 0.06,
    },
];

/// A pond's chop (turned from the wind): short and low.
pub const CALM_WAVES: [Wave; CALM] = [
    Wave {
        length: 2.6,
        turn: 0.2,
        steepness: 0.05,
    },
    Wave {
        length: 1.7,
        turn: -0.5,
        steepness: 0.045,
    },
    Wave {
        length: 1.1,
        turn: 0.9,
        steepness: 0.04,
    },
    Wave {
        length: 0.7,
        turn: -1.1,
        steepness: 0.03,
    },
];

/// The ripples' octaves: lattice (metres) and how fast each drifts along the wind (metres a
/// second).
pub const RIPPLES: [(f32, f64); 3] = [(1.1, 0.16), (0.42, 0.11), (0.17, 0.07)];
/// The foam's drift, metres a second.
const FOAM_DRIFT: f64 = 0.05;
/// The share of the ripples' drift they keep on a pond.
const CALM_DRIFT: f64 = 0.4;

/// The waves at `clock` seconds, render space's origin lying at `global` in the world: each
/// wave's wave vector (radians a metre east and north), steepness and phase at the origin (0..2π),
/// worked out in double precision so the shader's stays exact and each runs at the speed deep
/// water carries a wave of its length (a pond's at [`CALM_PACE`] of it). The chop first, then
/// [`PER_HEADING`] swell waves toward each heading (the long two last), then a pond's chop.
#[must_use]
pub fn wave_constants(clock: f64, global: glam::Vec2) -> [[f32; 4]; WAVE_COUNT] {
    use pyreal_primitives::num::math::{atan2, sin};
    let tau = std::f64::consts::TAU;
    let wind = atan2(f64::from(WIND.1), f64::from(WIND.0));
    let laid = WAVES[..CHOP]
        .iter()
        .map(|w| (wind, w))
        .chain((0..HEADINGS).flat_map(|h| {
            #[allow(clippy::cast_precision_loss)]
            let toward = tau * h as f64 / HEADINGS as f64;
            WAVES[CHOP..]
                .iter()
                .chain(&LONG_SWELL)
                .map(move |w| (toward, w))
        }))
        .chain(CALM_WAVES.iter().map(|w| (wind, w)));
    let mut out = [[0.0; 4]; WAVE_COUNT];
    for (i, ((base, w), o)) in laid.zip(&mut out).enumerate() {
        let heading = base + f64::from(w.turn);
        let k = tau / f64::from(w.length);
        let (kx, ky) = (k * cos(heading), k * sin(heading));
        let pace = if i >= CHOP + PER_HEADING * HEADINGS {
            CALM_PACE
        } else {
            1.0
        };
        let omega = (9.81 * k).sqrt() * pace;
        #[allow(clippy::cast_precision_loss)]
        let seed = i as f64 * 2.39;
        let phase = (kx * f64::from(global.x) + ky * f64::from(global.y) - omega * clock + seed)
            .rem_euclid(tau);
        #[allow(clippy::cast_possible_truncation)]
        {
            *o = [kx as f32, ky as f32, w.steepness, phase as f32];
        }
    }
    out
}

/// How far a pattern drifting `speed` metres a second along the wind has gone at `clock`, on a
/// lattice `cell` metres wide: the rest within a cell (metres), then whole cells east and north
/// (as `i32` bits).
#[must_use]
pub fn drift(clock: f64, speed: f64, cell: f32) -> [f32; 4] {
    let along = clock * speed / f64::from(cell);
    let (dx, dy) = (along * f64::from(WIND.0), along * f64::from(WIND.1));
    let (wx, wy) = (dx.floor(), dy.floor());
    #[allow(clippy::cast_possible_truncation)]
    [
        ((dx - wx) * f64::from(cell)) as f32,
        ((dy - wy) * f64::from(cell)) as f32,
        f32::from_bits((wx as i64 as i32).cast_unsigned()), // LINT-OK: whole cells, wrapping on purpose
        f32::from_bits((wy as i64 as i32).cast_unsigned()), // LINT-OK: whole cells, wrapping on purpose
    ]
}

/// What the waves' heights take besides the clock: how far from the eye the waves have height (0:
/// none), and the bodies near the eye (their feet), round which the troughs are held.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Heights {
    pub reach: f32,
    pub bodies: Vec<Vec3>,
}

/// The water's constants for the frame at `clock` seconds, render space's origin at `global`, with
/// no heights.
#[must_use]
pub fn frame_constants(clock: f64, global: glam::Vec2) -> Vec<u8> {
    frame_bytes(clock, global, &Heights::default())
}

/// The water's constants for the frame at `clock` seconds, render space's origin at `global`: the
/// waves, then the surf's share of its period and the clock wrapped to ten minutes, then each
/// ripple octave's drift, the foam's, and each octave's on a pond; then the heights' reach and the
/// bodies (`heights`); then the shore waves' phases ([`shore_phases`]).
#[must_use]
pub fn frame_bytes(clock: f64, global: glam::Vec2, heights: &Heights) -> Vec<u8> {
    let mut out = Vec::with_capacity(usize::try_from(WAVES_BYTES).unwrap_or(0));
    for w in wave_constants(clock, global) {
        for v in w {
            out.extend(v.to_le_bytes());
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    for v in [surf_phase(clock), clock.rem_euclid(600.0) as f32, 0.0, 0.0] {
        out.extend(v.to_le_bytes());
    }
    for d in RIPPLES
        .iter()
        .map(|&(cell, speed)| drift(clock, speed, cell))
        .chain([drift(clock, FOAM_DRIFT, 0.5)])
        .chain(
            RIPPLES
                .iter()
                .map(|&(cell, speed)| drift(clock, speed * CALM_DRIFT, cell)),
        )
    {
        for v in d {
            out.extend(v.to_le_bytes());
        }
    }
    for v in [heights.reach, 0.0, 0.0, 0.0] {
        out.extend(v.to_le_bytes());
    }
    for i in 0..SIM_BODIES {
        let b = heights
            .bodies
            .get(i)
            .map_or([0.0; 4], |b| [b.x, b.y, b.z, 1.0]);
        for v in b {
            out.extend(v.to_le_bytes());
        }
    }
    let [sea, lake] = shore_phases(clock);
    for v in [sea, lake, 0.0, 0.0] {
        out.extend(v.to_le_bytes());
    }
    out
}

/// How far the sea's shore waves have come in at `clock` seconds, a share of their period: a
/// wave's breaking foam reaches the still edge, and its swash starts up the shore, at nought.
#[must_use]
pub fn surf_phase(clock: f64) -> f32 {
    let tau = std::f64::consts::TAU;
    #[allow(clippy::cast_possible_truncation)]
    let s = ((SHORE_FRONT - f64::from(shore_phases(clock)[0])) / tau).rem_euclid(1.0) as f32;
    s
}

/// The size of [`frame_constants`].
pub const WAVES_BYTES: u64 = (WAVE_COUNT as u64 + 1 + 7 + 1 + SIM_BODIES as u64 + 1) * 16;

/// The mean of `exp(sin θ − 1)` over a period, which each wave's height is taken about.
pub const WAVE_MEAN: f32 = 0.465_759_6;

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What [`height_at`] reads besides the land field: the frame's waves ([`wave_constants`]) and
/// shore waves' phases ([`shore_phases`]), the looks and their shore waves' table
/// ([`WaterTable::shore_table`]), the gust (0 to 1), the eye, how far from it the waves have
/// height and the metres between the patches' vertices there, and render space's origin in the
/// world.
#[derive(Debug, Clone, Copy)]
pub struct HeightInputs<'a> {
    pub waves: &'a [[f32; 4]; WAVE_COUNT],
    pub shore: [f32; 2],
    pub table: &'a WaterTable,
    pub shore_table: &'a [f32],
    pub gust: f32,
    pub eye: Vec3,
    pub reach: f32,
    pub band: f32,
    pub global: Vec2,
}

/// A lattice point's hash, 0..1 (`land.wgsl` `land_hash`).
fn land_hash(x: i32, y: i32) -> f32 {
    let h0 =
        x.cast_unsigned().wrapping_mul(0x8DA6_B343) ^ y.cast_unsigned().wrapping_mul(0xD816_3841);
    let h1 = (h0 ^ (h0 >> 13)).wrapping_mul(0x2C1B_3C6D);
    let h2 = h1 ^ (h1 >> 15);
    #[allow(clippy::cast_precision_loss)]
    let v = (h2 & 0xFFFF) as f32 / 65535.0;
    v
}

/// Smooth value noise over a lattice `cell` metres wide at render-space `p`, render space's origin
/// at `global` (`land.wgsl` `land_noise`).
#[must_use]
pub fn land_noise(p: Vec2, cell: f32, global: Vec2) -> f32 {
    use pyreal_primitives::num::{floor_to_i32, to_i32};
    #[allow(clippy::cast_precision_loss)]
    let o = Vec2::new(
        to_i32(global.x / 192.0) as f32,
        to_i32(global.y / 192.0) as f32,
    ) * (192.0 / cell);
    let whole = o.floor();
    let q = p / cell + (o - whole);
    let (ix, iy) = (
        floor_to_i32(q.x) + floor_to_i32(whole.x),
        floor_to_i32(q.y) + floor_to_i32(whole.y),
    );
    let f = q - q.floor();
    let s = f * f * (Vec2::splat(3.0) - 2.0 * f);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    lerp(
        lerp(land_hash(ix, iy), land_hash(ix + 1, iy), s.x),
        lerp(land_hash(ix, iy + 1), land_hash(ix + 1, iy + 1), s.x),
        s.y,
    )
}

/// The mean of `exp(κ (sin θ − 1))` over a period: the modified Bessel function of order nought
/// at κ, times `e^−κ`.
#[must_use]
pub fn crest_mean(kappa: f32) -> f32 {
    let q = kappa * kappa * 0.25;
    let (mut term, mut sum) = (1.0f32, 1.0f32);
    for m in 1..=6u8 {
        term *= q / f32::from(m * m);
        sum += term;
    }
    sum * pyreal_primitives::num::math::expf(-kappa)
}

/// The phase a shore wave gains running in to the edge from `x` metres out, from `row` of the
/// shore table.
fn shore_phase_in(row: &[f32], x: f32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let last = (SHORE_STEPS - 1) as f32;
    let t = x / SHORE_STEP;
    if t >= last {
        return row[SHORE_STEPS - 1] + row[SHORE_STEPS] * (x - last * SHORE_STEP);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let j = t as usize;
    #[allow(clippy::cast_precision_loss)]
    let f = t - j as f32;
    row[j] + (row[j + 1] - row[j]) * f
}

/// The water's surface at render-space `p` as its patches draw it there (`water.wgsl`
/// `water_surface`, without the edge's wander or the troughs held round bodies): the level and the
/// waves' height, when the field holds water there.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn height_at(field: &LandField, at: &HeightInputs<'_>, p: Vec2) -> Option<f32> {
    use crate::land::bits;
    use pyreal_primitives::num::math::{atan2f, cosf, expf, powf, sinf};
    let (level, inside) = field.water_at(p)?;
    // Inside the drawn edge, which against a bank lies where the still water stops showing.
    let (rise, shift) = field.bank_at(p);
    let inside = inside - shift;
    if at.reach <= 0.0 || inside <= 0.0 {
        return Some(level);
    }
    let fade = 1.0 - smoothstep(at.reach * 0.75, at.reach, (p - at.eye.truncate()).length());
    if fade <= 0.0 {
        return Some(level);
    }
    // The look of the cell's most watery corner.
    let q = (p - field.origin) / pyreal_terrain::consts::CELL_SIZE;
    let (cx, cy) = (
        pyreal_primitives::num::floor_to_i32(q.x),
        pyreal_primitives::num::floor_to_i32(q.y),
    );
    let corner = [(0, 0), (1, 0), (0, 1), (1, 1)]
        .iter()
        .filter_map(|&(dx, dy)| field.index(field.first.0 + cx + dx, field.first.1 + cy + dy))
        .max_by(|&a, &b| field.inside[a].total_cmp(&field.inside[b]))?;
    let index = (((field.word[corner] >> bits::LOOK_SHIFT) & 15) as usize)
        .min(at.table.looks.len().saturating_sub(1));
    let look = at.table.looks.get(index)?;
    let shore = field
        .shore
        .as_ref()
        .map_or((crate::shore::MOST, Vec2::ZERO, 0.0), |s| {
            let t = ((p - field.origin) / crate::shore::TEXEL).clamp(
                Vec2::ZERO,
                Vec2::new((s.side.0 - 1) as f32, (s.side.1 - 1) as f32),
            );
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (x0, y0) = (t.x.floor() as usize, t.y.floor() as usize);
            let (x1, y1) = ((x0 + 1).min(s.side.0 - 1), (y0 + 1).min(s.side.1 - 1));
            #[allow(clippy::cast_precision_loss)]
            let f = t - Vec2::new(x0 as f32, y0 as f32);
            let at = |x: usize, y: usize| {
                let i = y * s.side.0 + x;
                (s.distance[i], s.toward[i], s.width[i])
            };
            let mix = |a: (f32, Vec2, f32), b: (f32, Vec2, f32), k: f32| {
                (
                    a.0 + (b.0 - a.0) * k,
                    a.1.lerp(b.1, k),
                    a.2 + (b.2 - a.2) * k,
                )
            };
            mix(
                mix(at(x0, y0), at(x1, y0), f.x),
                mix(at(x0, y1), at(x1, y1), f.x),
                f.y,
            )
        });
    let fetch = smoothstep(8.0, 120.0, inside)
        .max(smoothstep(30.0, 150.0, shore.2) * smoothstep(0.0, 6.0, inside));
    let bank = (rise * look.shelf / (2.0 * look.depth).max(1e-3)).clamp(1.0, 10.0);
    let near = inside.max(0.0) * bank;
    let pond = look.calm * (1.0 - fetch);
    let here = look.waves * at.gust * smoothstep(0.3, 4.0, near);
    let mut chop = here * (0.3 + 0.7 * fetch).max(look.swell) * (1.0 - pond);
    let mut sea = look.waves * look.swell * at.gust;
    let wind = Vec2::new(at.waves[0][0], at.waves[0][1]).normalize_or_zero();
    let mut toward = wind;
    if sea > 0.0 {
        let (s_dist, s_way) = (shore.0, shore.1);
        let open = smoothstep(200.0, 480.0, s_dist);
        toward = (s_way * (1.0 - open) + wind * (0.15 + open)).normalize_or_zero();
        let agree = smoothstep(0.2, 0.7, s_way.length()).max(open);
        sea *= 0.6 + 0.4 * agree;
    }
    let deep = bed_depth(near, look.depth, look.shelf);
    // The shore wave, and the share of the long swell left running straight.
    let (kind, shore_run) = shore_wave(look);
    let (mut a0, mut d, mut long) = (0.0f32, 0.0f32, 1.0f32);
    if let Some(sw) = shore_run {
        d = (shore.0 - shift).max(0.0);
        let zone = (look.shelf + 1.5 * sw.length).min(120.0);
        let t = ((d - 0.5 * zone) / (0.5 * zone)).clamp(0.0, 1.0);
        let share = cosf(t * std::f32::consts::FRAC_PI_2) * smoothstep(0.3, 0.7, shore.1.length());
        if kind < 1.5 {
            a0 = sea * sw.amplitude * share;
            long = (1.0 - share * share).max(0.0).sqrt();
        } else {
            let turned = look.shore * share;
            a0 = look.waves * at.gust * smoothstep(30.0, 150.0, shore.2) * sw.amplitude * turned;
            chop *= (1.0 - turned * turned).max(0.0).sqrt();
        }
        a0 = a0.min(0.05 * shore.2);
    }
    let sea_here = sea * smoothstep(1.0, 10.0, near);
    let (mut height, mut most) = (0.0f32, 0.0f32);
    let mut wave = |i: usize, amount: f32| {
        let w = at.waves[i];
        let k = Vec2::new(w[0], w[1]);
        let kk = k.length().max(1e-4);
        let held = smoothstep(3.0 * at.band, 6.0 * at.band, std::f32::consts::TAU / kk);
        if held <= 0.0 || amount <= 0.0 {
            return;
        }
        let e = expf(sinf(k.dot(p) + w[3]) - 1.0);
        let a = w[2] / kk * amount * held;
        height += a * (e - WAVE_MEAN);
        most += a;
    };
    for i in 0..CHOP {
        wave(i, chop);
    }
    let calm_first = CHOP + PER_HEADING * HEADINGS;
    for i in 0..CALM {
        wave(calm_first + i, here * pond);
    }
    if sea_here > 0.0 {
        #[allow(clippy::cast_precision_loss)]
        let turn = atan2f(toward.y, toward.x) / (std::f32::consts::TAU / HEADINGS as f32);
        let f = turn - turn.floor();
        let first = pyreal_primitives::num::floor_to_i32(turn).rem_euclid(8);
        for side in 0..2 {
            let share = if side == 0 {
                cosf(f * std::f32::consts::FRAC_PI_2)
            } else {
                sinf(f * std::f32::consts::FRAC_PI_2)
            };
            if share < 0.01 {
                continue;
            }
            let h = usize::try_from((first + side) % 8).unwrap_or(0);
            for j in 0..PER_HEADING {
                let amount = sea_here * share * if j >= 4 { long } else { 1.0 };
                wave(CHOP + h * PER_HEADING + j, amount);
            }
        }
    }
    if let (true, Some(sw)) = (a0 > 0.0, shore_run) {
        let row = &at.shore_table[index * SHORE_ROW..(index + 1) * SHORE_ROW];
        let k0 = sw.wavenumber();
        let x = powf(k0 * bed_depth(d, look.depth, look.shelf).max(0.05), 0.75);
        let th0 = 1.0 - 2.0 / (expf(2.0 * x) + 1.0);
        let k = k0 / powf(th0, 2.0 / 3.0);
        let n = land_noise(p + Vec2::new(5.3, 17.1), 40.0, at.global) - 0.5;
        let clock = if kind < 1.5 { at.shore[0] } else { at.shore[1] };
        let th = -shore_phase_in(row, d) + clock + std::f32::consts::TAU * 0.6 * n;
        let ratio = a0 / deep.max(0.05);
        let kappa = 1.0 + 0.5 * smoothstep(0.1, 0.35, ratio);
        let held = smoothstep(3.0 * at.band, 6.0 * at.band, std::f32::consts::TAU / k);
        let a = a0 * held;
        height += a * (expf(kappa * (sinf(th) - 1.0)) - crest_mean(kappa));
        most += a;
    }
    let shrink = smoothstep(0.3, 3.0, deep).min(0.39 * deep / most.max(1e-3));
    Some(level + height * shrink * fade)
}

/// The simulation's square: its side in cells and a cell's side in metres.
pub const SIM_SIDE: u32 = 256;
pub const SIM_CELL: f32 = 0.08;
/// A step, seconds, and the most a frame runs.
pub const SIM_STEP: f64 = 1.0 / 60.0;
pub const SIM_MOST_STEPS: usize = 4;
/// The most bodies that disturb it.
pub const SIM_BODIES: usize = 8;
const GRAVITY: f32 = 9.81;
const KERNEL_CENTER: f32 = 0.443_113_5;
const PACE: f32 = KERNEL_CENTER * 0.75;
const DAMPING: f32 = 0.4;
const KERNEL_REACH: usize = 5;
const CENTER_WITHIN: f32 = 30.0;
const FOLLOW: f32 = 1.5;
const DROPS: f32 = 6.0;
const DROP_DEPTH: f32 = 0.008;
const RAIN_PATCH: f32 = 0.4;
/// One step's constants and the stride they are laid at.
pub const STEP_BYTES: usize = 64 + SIM_BODIES * 4 + 2 * SIM_BODIES * 16;
pub const STEP_STRIDE: u64 = 512;

/// The simulation's state between frames: where its square lies, the clock it has stepped to,
/// and where each body stood.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sim {
    origin: Option<[i64; 2]>,
    clock: Option<f64>,
    owed: f64,
    count: u32,
    bodies: Vec<Vec3>,
}

/// What a frame asks of the simulation: each step's constants, and the square it holds after them
/// (its south-west corner and side, render space).
#[derive(Debug, Clone, PartialEq)]
pub struct SimPlan {
    pub steps: Vec<Vec<u8>>,
    pub area: [f32; 3],
}

fn cell_of(m: f32) -> i64 {
    #[allow(clippy::cast_possible_truncation)]
    let c = (f64::from(m) / f64::from(SIM_CELL)).floor() as i64;
    c
}

impl Sim {
    /// Plans the frame at `clock` seconds: the eye at `eye`, the bodies at `bodies` (their feet;
    /// the nearest the eye stir it) with the waves lifting the water `lifts` over its level where
    /// each stands, `rain` from 0 to 1. A fixed rate of steps by the clock, a few at most; a held
    /// clock holds the water still, and a clock run back starts it calm.
    #[must_use]
    pub fn plan(
        &mut self,
        clock: f64,
        eye: Vec3,
        bodies: &[Vec3],
        lifts: &[f32],
        rain: f32,
    ) -> SimPlan {
        if self.clock.is_some_and(|c| clock < c) {
            *self = Self::default();
        }
        let elapsed = self.clock.map_or(0.0, |c| clock - c);
        self.clock = Some(clock);
        self.owed += elapsed;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = ((self.owed / SIM_STEP).floor() as usize).min(SIM_MOST_STEPS);
        #[allow(clippy::cast_precision_loss)]
        {
            self.owed -= steps as f64 * SIM_STEP;
        }
        if self.owed > SIM_STEP {
            self.owed = 0.0;
        }
        let e = eye.truncate();
        let mut lifted: Vec<(Vec3, f32)> = bodies
            .iter()
            .enumerate()
            .map(|(i, b)| (*b, lifts.get(i).copied().unwrap_or(0.0)))
            .collect();
        lifted.sort_by(|a, b| {
            (a.0.truncate() - e)
                .length()
                .total_cmp(&(b.0.truncate() - e).length())
        });
        lifted.truncate(SIM_BODIES);
        let world: Vec<Vec3> = lifted.iter().map(|b| b.0).collect();
        let center = world
            .iter()
            .map(|b| b.truncate())
            .filter(|b| (*b - e).length() < CENTER_WITHIN)
            .min_by(|a, b| (*a - e).length().total_cmp(&(*b - e).length()))
            .unwrap_or(e);
        let half = i64::from(SIM_SIDE / 2);
        let first = [cell_of(center.x) - half, cell_of(center.y) - half];
        let mut taken = vec![false; self.bodies.len()];
        let came: Vec<Vec3> = world
            .iter()
            .map(|b| {
                let nearest = self
                    .bodies
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !taken[*i])
                    .map(|(i, p)| (i, (*p - *b).length()))
                    .filter(|(_, l)| *l < FOLLOW)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                nearest.map_or(*b, |(i, _)| {
                    taken[i] = true;
                    self.bodies[i]
                })
            })
            .collect();
        let rain = rain.clamp(0.0, 1.0);
        #[allow(clippy::cast_possible_truncation)]
        let chance = (DROPS * rain * RAIN_PATCH * RAIN_PATCH * SIM_STEP as f32).min(1.0);
        #[allow(clippy::cast_possible_truncation)]
        let step = SIM_STEP as f32;
        let mut out = Vec::with_capacity(steps);
        for s in 0..steps {
            let held = if s == 0 {
                self.origin.unwrap_or([first[0] + 4 * half, first[1]])
            } else {
                first
            };
            #[allow(clippy::cast_precision_loss)]
            let (a, b) = (s as f32 / steps as f32, (s + 1) as f32 / steps as f32);
            let mut c = Vec::with_capacity(STEP_BYTES);
            for v in [first[0], first[1], held[0], held[1]] {
                #[allow(clippy::cast_possible_truncation)]
                c.extend((v as i32).to_le_bytes());
            }
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            let corner = [
                (first[0] as f64 * f64::from(SIM_CELL)) as f32,
                (first[1] as f64 * f64::from(SIM_CELL)) as f32,
            ];
            for v in [
                corner[0],
                corner[1],
                0.0,
                0.0,
                GRAVITY * PACE / SIM_CELL * step * step,
                DAMPING * step,
                chance,
                DROP_DEPTH * rain,
            ] {
                c.extend(v.to_le_bytes());
            }
            #[allow(clippy::cast_possible_truncation)]
            for v in [self.count, world.len() as u32, 0, 0] {
                c.extend(v.to_le_bytes());
            }
            for i in 0..SIM_BODIES {
                c.extend(lifted.get(i).map_or(0.0, |b| b.1).to_le_bytes());
            }
            for t in [a, b] {
                for i in 0..SIM_BODIES {
                    let p = match (came.get(i), world.get(i)) {
                        (Some(f), Some(n)) => {
                            let at = f.lerp(*n, t);
                            [at.x, at.y, at.z, 1.0]
                        }
                        _ => [0.0; 4],
                    };
                    for v in p {
                        c.extend(v.to_le_bytes());
                    }
                }
            }
            out.push(c);
            self.count = self.count.wrapping_add(1);
        }
        // Where each stood at the last step, so a step between frames sees all its move.
        if steps > 0 {
            self.origin = Some(first);
            self.bodies = world;
        }
        let held = self.origin.unwrap_or(first);
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let area = [
            (held[0] as f64 * f64::from(SIM_CELL)) as f32,
            (held[1] as f64 * f64::from(SIM_CELL)) as f32,
            SIM_SIDE as f32 * SIM_CELL,
        ];
        SimPlan { steps: out, area }
    }
}

/// The water's vertical derivative as the simulation takes it: a cell pulled by each neighbour
/// within five cells, `KERNEL_REACH + 1` a row, levelled to the centre's. As WGSL.
#[must_use]
pub fn kernel_wgsl() -> String {
    let n = KERNEL_REACH + 1;
    let at = |r: f64| -> f64 {
        let dq = 0.001;
        (1..=10_000)
            .map(|i| {
                let q = f64::from(i) * dq;
                q * q * exp(-q * q) * bessel_j0(q * r) * dq
            })
            .sum()
    };
    let center = at(0.0);
    let k: Vec<String> = (0..n * n)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = ((i % n) as f64, (i / n) as f64);
            format!("{:.7}", at((x * x + y * y).sqrt()) / center)
        })
        .collect();
    format!("const KERNEL = array<f32, {}>({});", n * n, k.join(", "))
}

/// The Bessel function of the first kind and order nought (Abramowitz and Stegun 9.4.1, 9.4.3).
fn bessel_j0(x: f64) -> f64 {
    let x = x.abs();
    if x <= 3.0 {
        let y = (x / 3.0) * (x / 3.0);
        1.0 + y
            * (-2.249_999_7
                + y * (1.265_620_8
                    + y * (-0.316_386_6
                        + y * (0.044_447_9 + y * (-0.003_944_4 + y * 0.000_210_0)))))
    } else {
        let y = 3.0 / x;
        let f = 0.797_884_56
            + y * (-0.000_000_77
                + y * (-0.005_527_40
                    + y * (-0.000_095_12
                        + y * (0.001_372_37 + y * (-0.000_728_05 + y * 0.000_144_76)))));
        let t = x - std::f64::consts::FRAC_PI_4
            + y * (-0.041_663_97
                + y * (-0.000_039_54
                    + y * (0.002_625_73
                        + y * (-0.000_541_25 + y * (-0.000_293_33 + y * 0.000_135_58)))));
        f * cos(t) / x.sqrt()
    }
}

/// Quads along one side of a water patch.
pub const PATCH: u32 = 16;
/// Metres between the finest patches' vertices; each level doubles it, to the land field's cells.
pub const FINEST: f32 = 0.75;
pub const LEVELS: u32 = 6;
/// How far a level is drawn from the eye, in its own patches.
pub const REACH: f32 = 5.0;
/// The share of a level's reach its vertices start morphing toward the next level's at. With
/// [`REACH`] patches, a coarser neighbour's vertices are still whole where a finer one's are
/// wholly morphed (`1/2 + √2/REACH` at least), so no edge holds a T-junction.
pub const MORPH_FROM: f32 = 0.8;
/// The most nodes a frame draws, and a node's bytes.
pub const MAX_NODES: usize = 4096;
pub const NODE_BYTES: u64 = 32;
/// How far a node's box reaches over and under its water's levels, metres.
const BOX_SLACK: f32 = 2.0;

/// How far from the eye the waves have height with `finest` the finest level drawn, metres: none
/// with the land field's cells alone (Water Detail's Low).
#[must_use]
pub const fn height_reach(finest: u32) -> f32 {
    match finest {
        0 => 384.0,
        1 => 192.0,
        _ => 0.0,
    }
}

/// The finest level of patches Water Detail draws at its stored `detail`: Low the land field's
/// cells, Medium 1.5 m, High 0.75 m.
#[must_use]
pub const fn finest_level(detail: u32) -> u32 {
    match detail {
        0 => LEVELS - 1,
        1 => 1,
        _ => 0,
    }
}

/// A patch's side, metres, at `level`.
#[must_use]
pub fn patch_side(level: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let s = PATCH as f32 * FINEST * (1u32 << level) as f32;
    s
}

/// How far from the eye `level` is drawn: [`REACH`] of its patches, the coarsest everywhere.
#[must_use]
pub fn level_reach(level: u32) -> f32 {
    if level + 1 >= LEVELS {
        f32::MAX
    } else {
        REACH * patch_side(level)
    }
}

/// One water patch drawn this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterNode {
    /// Its south-west vertex, in finest steps from the land field's first vertex.
    pub origin: [i32; 2],
    pub level: u32,
    /// Where its vertices start morphing to the next level, and where they have.
    pub morph: (f32, f32),
}

impl WaterNode {
    pub fn write(&self, out: &mut Vec<u8>) {
        for v in self.origin {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.level.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        for v in [self.morph.0, self.morph.1, 0.0, 0.0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
}

/// One level's nodes across, and each node's lowest and highest water level (none: no water).
type LevelNodes = (usize, Vec<Option<(f32, f32)>>);

/// Where a land field's water lies, for choosing its patches: per level, each node's lowest and
/// highest level of the water, or none where it holds no cell with a water corner (or beside one,
/// for the swash).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WaterLayout {
    /// Render-space position of the land field's first vertex.
    pub origin: Vec2,
    /// Per level: nodes across, then each node, east then north.
    levels: Vec<LevelNodes>,
}

impl WaterLayout {
    #[must_use]
    pub fn new(field: &LandField) -> Self {
        let (w, h) = field.side;
        let (cw, ch) = (w.saturating_sub(1), h.saturating_sub(1));
        // Each cell's levels where it or a neighbour has a water corner.
        let corner = |x: usize, y: usize| {
            [
                y * w + x,
                y * w + x + 1,
                (y + 1) * w + x,
                (y + 1) * w + x + 1,
            ]
        };
        let wet: Vec<bool> = (0..cw * ch)
            .map(|i| corner(i % cw, i / cw).iter().any(|&v| field.is_water(v)))
            .collect();
        let cells: Vec<Option<(f32, f32)>> = (0..cw * ch)
            .map(|i| {
                let (x, y) = (i % cw, i / cw);
                let near = (y.saturating_sub(1)..=(y + 1).min(ch - 1)).any(|ny| {
                    (x.saturating_sub(1)..=(x + 1).min(cw - 1)).any(|nx| wet[ny * cw + nx])
                });
                near.then(|| {
                    corner(x, y)
                        .iter()
                        .fold((f32::MAX, f32::MIN), |(lo, hi), &v| {
                            (lo.min(field.level[v]), hi.max(field.level[v]))
                        })
                })
            })
            .collect();
        let cell = pyreal_terrain::consts::CELL_SIZE;
        let levels = (0..LEVELS)
            .map(|level| {
                let side = patch_side(level);
                #[allow(
                    clippy::cast_precision_loss,
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss
                )]
                let across = ((cw.max(ch) as f32 * cell) / side).ceil() as usize;
                let nodes = (0..across * across)
                    .map(|n| {
                        let (i, j) = (n % across, n / across);
                        #[allow(
                            clippy::cast_precision_loss,
                            clippy::cast_possible_truncation,
                            clippy::cast_sign_loss
                        )]
                        let span = |k: usize, count: usize| {
                            let lo = (k as f32 * side / cell).floor() as usize;
                            let hi = (((k + 1) as f32 * side / cell).ceil() as usize).min(count);
                            lo..hi
                        };
                        let mut out: Option<(f32, f32)> = None;
                        for y in span(j, ch) {
                            for x in span(i, cw) {
                                if let Some((lo, hi)) = cells[y * cw + x] {
                                    let b = out.unwrap_or((lo, hi));
                                    out = Some((b.0.min(lo), b.1.max(hi)));
                                }
                            }
                        }
                        out
                    })
                    .collect();
                (across, nodes)
            })
            .collect();
        Self {
            origin: field.origin,
            levels,
        }
    }

    /// The nodes to draw for an eye at `eye` seeing through `planes`, none finer than `finest`:
    /// a node is drawn whole unless the next finer level's reach meets its box, when its four
    /// children are chosen instead (CDLOD); nodes with no water or outside the frustum are left out.
    #[must_use]
    pub fn select(&self, eye: Vec3, planes: &[Vec4], finest: u32) -> Vec<WaterNode> {
        let Some(top) = u32::try_from(self.levels.len())
            .ok()
            .and_then(|n| n.checked_sub(1))
        else {
            return Vec::new();
        };
        let finest = finest.min(top);
        let mut out = Vec::new();
        let mut stack = Vec::new();
        let roots = self.levels[top as usize].0;
        for j in 0..roots {
            for i in 0..roots {
                stack.push((top, i, j));
            }
        }
        while let Some((level, i, j)) = stack.pop() {
            let (across, nodes) = &self.levels[level as usize];
            if i >= *across || j >= *across {
                continue;
            }
            let Some((lo, hi)) = nodes[j * across + i] else {
                continue;
            };
            let side = patch_side(level);
            #[allow(clippy::cast_precision_loss)]
            let min = self.origin + Vec2::new(i as f32, j as f32) * side;
            let (bmin, bmax) = (
                min.extend(lo - BOX_SLACK),
                (min + Vec2::splat(side)).extend(hi + BOX_SLACK),
            );
            let outside = planes.iter().any(|p| {
                let far = Vec3::select(p.xyz().cmpge(Vec3::ZERO), bmax, bmin);
                p.xyz().dot(far) + p.w < 0.0
            });
            if outside || out.len() >= MAX_NODES {
                continue;
            }
            let d = (eye.clamp(bmin, bmax) - eye).length();
            if level > finest && d < level_reach(level - 1) {
                for (di, dj) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    stack.push((level - 1, i * 2 + di, j * 2 + dj));
                }
                continue;
            }
            let end = level_reach(level);
            let steps = i32::try_from(PATCH << level).unwrap_or(0);
            out.push(WaterNode {
                origin: [
                    i32::try_from(i).unwrap_or(0) * steps,
                    i32::try_from(j).unwrap_or(0) * steps,
                ],
                level,
                morph: if end == f32::MAX {
                    (1.0e30, 2.0e30)
                } else {
                    (end * MORPH_FROM, end)
                },
            });
        }
        out
    }
}

/// The water on the GPU: its surface, its looks and the simulation's pictures and steps.
#[derive(Debug)]
pub struct WaterGpu {
    /// Where the land field's water lies, which this frame's patches are chosen from.
    pub layout: Option<WaterLayout>,
    /// The finest level of patches drawn (Water Detail, [`finest_level`]).
    pub finest: u32,
    /// The gust the waves were drawn in.
    pub gust: f32,
    /// One patch's triangles, and this frame's nodes.
    pub indices: wgpu::Buffer,
    pub nodes: wgpu::Buffer,
    pub node_count: u32,
    pub looks: wgpu::Buffer,
    /// The looks' shore waves' table ([`shore_table`]), and the same on the GPU.
    pub shore_table: Vec<f32>,
    pub shore_gain: wgpu::Buffer,
    /// The frame's waves, surf and drifts ([`frame_constants`]).
    pub waves: wgpu::Buffer,
    pub sim_textures: [wgpu::Texture; 2],
    pub sim_views: [wgpu::TextureView; 2],
    pub steps: wgpu::Buffer,
    /// Per (picture read, step slot): the step's bind group.
    pub sim_groups: Vec<wgpu::BindGroup>,
    pub sim_pipeline: wgpu::ComputePipeline,
    /// Which picture holds the water as it stands.
    pub current: usize,
    pub sim: Sim,
    /// The square the simulation holds, when it has run.
    pub area: Option<[f32; 3]>,
}

impl WaterGpu {
    /// The simulation's pictures and pipeline (`frame_layout` group 0) and room for `looks`.
    #[must_use]
    pub fn new(gpu: &Gpu, frame_layout: &wgpu::BindGroupLayout, table: &WaterTable) -> Self {
        let device = &gpu.device;
        let make = |label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: SIM_SIDE,
                    height: SIM_SIDE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let sim_textures = [make("ripples a"), make("ripples b")];
        let sim_views = [
            sim_textures[0].create_view(&Default::default()),
            sim_textures[1].create_view(&Default::default()),
        ];
        let steps = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ripple steps"),
            size: STEP_STRIDE * SIM_MOST_STEPS as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut idx: Vec<u8> = Vec::new();
        let row = PATCH + 1;
        for y in 0..PATCH {
            for x in 0..PATCH {
                let a = y * row + x;
                for i in [a, a + 1, a + row + 1, a, a + row + 1, a + row] {
                    idx.extend_from_slice(&i.to_le_bytes());
                }
            }
        }
        let indices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water patch"),
            size: idx.len() as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue.write_buffer(&indices, 0, &idx);
        let nodes = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water nodes"),
            size: MAX_NODES as u64 * NODE_BYTES,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let looks_bytes = table.bytes();
        let looks = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water looks"),
            size: (looks_bytes.len() as u64).max(96),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue.write_buffer(&looks, 0, &looks_bytes);
        let shore_table = table.shore_table();
        let gain_bytes: Vec<u8> = shore_table.iter().flat_map(|v| v.to_le_bytes()).collect();
        let shore_gain = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water shore waves"),
            size: (gain_bytes.len() as u64).max(SHORE_ROW as u64 * 4),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue.write_buffer(&shore_gain, 0, &gain_bytes);
        let waves = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water waves"),
            size: WAVES_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        use wgpu::ShaderStages as S;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ripples"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: S::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: S::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: S::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
        let mut sim_groups = Vec::new();
        for read in 0..2 {
            for slot in 0..SIM_MOST_STEPS as u64 {
                sim_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("ripple step"),
                    layout: &layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &steps,
                                offset: slot * STEP_STRIDE,
                                size: wgpu::BufferSize::new(STEP_BYTES as u64),
                            }),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&sim_views[read]),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&sim_views[1 - read]),
                        },
                    ],
                }));
            }
        }
        let module = gpu.shader("ripples", &crate::renderer::source::water_sim());
        let sim_pipeline = gpu.compute_pipeline(wgpu::ComputePipelineDescriptor {
            label: Some("ripples"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("ripples"),
                    bind_group_layouts: &[Some(frame_layout), Some(&layout)],
                    immediate_size: 0,
                }),
            ),
            module: &module,
            entry_point: Some("step"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            layout: None,
            finest: finest_level(1),
            gust: 1.0,
            indices,
            nodes,
            node_count: 0,
            looks,
            shore_table,
            shore_gain,
            waves,
            sim_textures,
            sim_views,
            steps,
            sim_groups,
            sim_pipeline,
            current: 0,
            sim: Sim::default(),
            area: None,
        }
    }

    /// Lays out the water of `field` for its patches; none when it holds no water.
    pub fn set_field(&mut self, field: &LandField) {
        self.layout = (field.side.0 > 1 && (0..field.word.len()).any(|i| field.is_water(i)))
            .then(|| WaterLayout::new(field));
        self.node_count = 0;
    }

    /// Chooses this frame's patches for an eye at `eye` seeing through `planes`, and writes them.
    pub fn update(&mut self, gpu: &Gpu, eye: Vec3, planes: &[Vec4]) {
        let nodes = self
            .layout
            .as_ref()
            .map(|l| l.select(eye, planes, self.finest))
            .unwrap_or_default();
        let mut bytes = Vec::with_capacity(nodes.len() * 32);
        for n in &nodes {
            n.write(&mut bytes);
        }
        if !bytes.is_empty() {
            gpu.queue.write_buffer(&self.nodes, 0, &bytes);
        }
        self.node_count = u32::try_from(nodes.len()).unwrap_or(0);
    }

    /// Patches drawn this frame, and their triangles.
    #[must_use]
    pub fn drawn(&self) -> (u32, u32) {
        (self.node_count, self.node_count * PATCH * PATCH * 2)
    }

    /// Draws this frame's patches, the pass's pipeline and groups set.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.node_count == 0 {
            return;
        }
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..PATCH * PATCH * 6, 0, 0..self.node_count);
    }

    /// Writes the frame's waves and, when `simulate` (near water is drawn), plans this frame's
    /// steps and writes their constants (`rain` from 0 to 1 stirs the water); the number of steps.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        clock: f64,
        global: glam::Vec2,
        eye: Vec3,
        bodies: &[Vec3],
        lifts: &[f32],
        rain: f32,
        simulate: bool,
    ) -> usize {
        let mut near: Vec<Vec3> = bodies.to_vec();
        near.sort_by(|a, b| a.distance(eye).total_cmp(&b.distance(eye)));
        near.truncate(SIM_BODIES);
        let heights = Heights {
            reach: height_reach(self.finest),
            bodies: near,
        };
        gpu.queue
            .write_buffer(&self.waves, 0, &frame_bytes(clock, global, &heights));
        if !simulate {
            return 0;
        }
        let plan = self.sim.plan(clock, eye, bodies, lifts, rain);
        for (k, s) in plan.steps.iter().enumerate() {
            gpu.queue
                .write_buffer(&self.steps, k as u64 * STEP_STRIDE, s);
        }
        if !plan.steps.is_empty() || self.area.is_some() {
            self.area = Some(plan.area);
        }
        plan.steps.len()
    }

    /// Encodes `steps` steps, the pictures swapped after each.
    pub fn encode(
        &mut self,
        pass: &mut wgpu::ComputePass<'_>,
        frame: &wgpu::BindGroup,
        steps: usize,
    ) {
        pass.set_pipeline(&self.sim_pipeline);
        pass.set_bind_group(0, frame, &[]);
        for k in 0..steps {
            pass.set_bind_group(1, &self.sim_groups[self.current * SIM_MOST_STEPS + k], &[]);
            pass.dispatch_workgroups(SIM_SIDE / 16, SIM_SIDE / 16, 1);
            self.current = 1 - self.current;
        }
    }
}
