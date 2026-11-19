//! Puffs: a body's billowing particles drawn as one volume in place of their cards.
//!
//! An emitter whose particles make up a body of vapour (an acid elemental's churning green cloud)
//! is classified by its sprite and how its particles behave ([`kind_of`]); its particles are
//! drawn as [`crate::particles::Look::Puff`]. Each live particle is a soft ball of the volume at
//! its place, as wide as its picture and as thick as its strength, so the volume follows the
//! body's parts as the emitters do. `shaders/puffs.wgsl` draws a card per ball and marches the
//! body's whole volume once per pixel, by the first of its balls the view crosses: churned and
//! carved by noise, lit by the sun, the sky and the lamps, glowing of itself, blended over what
//! is behind it. A glowing kind lights its surroundings through the creature's glow lamp
//! ([`crate::lamps::glows`]).

use glam::Vec3;
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::DataId;

use crate::asset::texture::TextureSource;
use crate::lamps::{decode, Tone};
use crate::particles::{is_light_sprite, Sprite};

/// Acid: a glowing green cloud, denser and brighter at its heart, that bubbles and drips.
pub const ACID: u8 = 0;
/// Steam: white vapour that rises and rolls, lit by what is round it.
pub const STEAM: u8 = 1;
/// Frost mist: cold air that sinks, faintly blue (or its picture's colour, a pillar's cyan).
pub const FROST: u8 = 2;
/// A creature's smoke (dark, grey or tinted, as its picture) and dust (sandy, grainy, whirling):
/// `shaders/smoke.wgsl` marches these.
pub const SMOKE: u8 = 3;
pub const DUST: u8 = 4;
/// Smoke drawn into a whirl (Dark Vapor's, a Shadow Vortex's): a disc about its emitter's axis,
/// its arms spiralling in to a dark eye.
pub const VORTEX: u8 = 5;
/// Snow and ice (a frost elemental's): a cold core, rime at its edges and ice-blue deep, streaked
/// by the snow whirling round it, with flakes and glints of its own ([`crate::rime`]).
pub const SNOW: u8 = 6;
/// A portal in the world ([`crate::gates`]): a glowing disc of its colour about its axis, its arms
/// spiralling in to its heart. `shaders/gates.wgsl` draws it.
pub const GATE: u8 = 7;
/// A dungeon room's haze ([`haze_of`]): a soft glow of its picture's colour that fills the room,
/// drifting and churning slowly, adding its light and hiding little.
pub const HAZE: u8 = 8;
/// A vent's gas ([`gas_of`]): a cloud of its picture's colour blown out and swelling, laid over
/// what is behind it, lit by the room's lamps.
pub const GAS: u8 = 9;
/// A ball's reach over its transform's scale (`puffs.wgsl`'s `PUFF_SIZE`).
pub const PUFF_SIZE: f32 = 0.85;
/// How much wider a snow ball is than its particle: the few small puffs of a frost elemental's
/// emitters make one body of snow, as retail's added pictures do.
pub const SNOW_REACH: f32 = 1.35;

/// An acid body's glow lamp: its intensity (the client's units) and how far it reaches, metres.
pub const ACID_GLOW_INTENSITY: f32 = 8.0;
pub const ACID_GLOW_FALLOFF: f32 = 6.0;

/// Whether an emitter makes an acid cloud: a strong green additive sprite (`green`, `additive`)
/// riding its body (`local`), born endlessly, a few at a time, each living a second or more and
/// swelling as it goes.
#[must_use]
#[allow(clippy::fn_params_excessive_bools)] // an emitter's facts, as each caller reads them
pub fn is_acid(
    green: bool,
    additive: bool,
    endless: bool,
    local: bool,
    scales: (f32, f32),
    lifespan: f64,
    max_particles: i32,
) -> bool {
    green
        && additive
        && endless
        && local
        && max_particles >= 3
        && lifespan >= 1.0
        && scales.1 >= scales.0 * 1.4
}

/// The kind of puff an emitter of `sprite` makes, if its particles are drawn as a volume.
#[must_use]
pub fn kind_of(sprite: &Sprite, e: &pyreal_animation::data::ParticleEmitterInfo) -> Option<u8> {
    is_acid(
        sprite.tone.green,
        sprite.additive(),
        e.total_particles <= 0 && e.total_seconds <= 0.0,
        e.is_parent_local != 0,
        (e.start_scale, e.final_scale),
        e.lifespan,
        e.max_particles,
    )
    .then_some(ACID)
    .or_else(|| steam_of(sprite.tone, sprite.additive(), &Stream::of(e)))
    .or_else(|| haze_of(sprite.tone, sprite.additive(), sprite.half, e))
    .or_else(|| gas_of(sprite.tone, sprite.additive(), e))
}

/// The least a haze's picture grows to across, metres.
const HAZE_WIDEST: f32 = 8.0;
/// Which of a haze's puffs its volume keeps: one in this many ([`stride_reach`] grows them).
pub const HAZE_STRIDE: u32 = 3;

/// A room's haze: an endless stream of flat additive pictures, coloured, that neither rise nor
/// fall and swell from a speck to many metres across over seconds (the dungeons' purple, blue and
/// warm hazes, 0x0100192C to 0x01001931). `half` is its picture's half width a unit of scale.
#[must_use]
pub fn haze_of(
    tone: Tone,
    additive: bool,
    half: f32,
    e: &pyreal_animation::data::ParticleEmitterInfo,
) -> Option<u8> {
    use pyreal_animation::data::ParticleType;
    let still = matches!(
        e.particle_type,
        ParticleType::Still | ParticleType::LocalVelocity | ParticleType::GlobalVelocity
    );
    let rise = Stream::of(e).rise;
    (additive
        && tone.flat
        && !tone.grey
        && still
        && rise.abs() <= 0.05
        && e.total_particles <= 0
        && e.total_seconds <= 0.0
        && e.max_particles >= 10
        && e.lifespan >= 4.0
        && e.start_scale <= 0.5
        && e.final_scale * half * 2.0 >= HAZE_WIDEST)
        .then_some(HAZE)
}

/// A vent's gas: an endless stream of laid-over (not added) puffs of a green, rounded picture that
/// swell over a second and a half or more and do not fall (the dungeons' gas vents 0x02000AB3,
/// 0x02000AB4, 0x0200028D and their kin, all of picture 0x01000FF9).
#[must_use]
pub fn gas_of(
    tone: Tone,
    additive: bool,
    e: &pyreal_animation::data::ParticleEmitterInfo,
) -> Option<u8> {
    let s = Stream::of(e);
    let swells = s.final_scale >= s.start_scale * 1.4 && s.start_scale.max(s.final_scale) >= 0.8;
    (!additive
        && tone.green
        && !tone.flat
        && s.endless
        && !s.light
        && swells
        && s.lifespan >= 1.5
        && s.rise > -1.0
        && e.max_particles >= 4)
        .then_some(GAS)
}

/// The most of one gas emitter's puffs its volume keeps whole: a larger one keeps one in two.
const GAS_MOST: i32 = 12;

/// Which of a gas emitter's puffs its volume keeps: one in this many ([`stride_reach`] grows them).
#[must_use]
pub fn gas_stride(e: &pyreal_animation::data::ParticleEmitterInfo) -> u32 {
    if e.max_particles > GAS_MOST {
        2
    } else {
        1
    }
}

/// How fast (metres a second) a gas emitter's puffs must fall to pour down and pool.
const GAS_POOLS_UNDER: f32 = -0.05;

/// Whether a gas emitter's puffs are heavy gas that pours down and spreads low as it ages (a
/// vent's falling jet), not its rising haze or a pit's cloud.
#[must_use]
pub fn gas_pools(e: &pyreal_animation::data::ParticleEmitterInfo) -> bool {
    Stream::of(e).rise < GAS_POOLS_UNDER
}

/// The volume a placed object's emitter of picture `gfx` makes, if any, as [`kind_of`] reads its
/// sprite: steam, frost, snow, haze or gas.
#[must_use]
pub fn placed_kind(
    store: &RetailDatStore,
    gfx: DataId,
    e: &pyreal_animation::data::ParticleEmitterInfo,
) -> Option<u8> {
    let g = decode::<pyreal_assets::GfxObj>(store, DbType::GfxObj, gfx)?;
    let additive = g.surfaces.iter().any(|s| {
        decode::<pyreal_assets::Surface>(store, DbType::Surface, *s)
            .is_some_and(|s| s.surface_type & 0x1_0000 != 0)
    });
    let tone = crate::lamps::tone(store, gfx);
    steam_of(tone, additive, &Stream::of(e))
        .or_else(|| haze_of(tone, additive, picture_half(store, gfx), e))
        .or_else(|| gas_of(tone, additive, e))
}

/// The steam picture's half width (0x01000FC3's), metres a unit of scale, which the shader's
/// balls are sized for: other pictures' balls are as much wider or narrower as they are.
pub const STEAM_HALF: f32 = 0.72;

/// Picture `gfx`'s half width, metres a unit of scale ([`STEAM_HALF`] when it will not read).
#[must_use]
pub fn picture_half(store: &pyreal_dat::RetailDatStore, gfx: pyreal_primitives::DataId) -> f32 {
    let Some(g) =
        crate::lamps::decode::<pyreal_assets::GfxObj>(store, pyreal_dat::DbType::GfxObj, gfx)
    else {
        return STEAM_HALF;
    };
    let (lo, hi) = g
        .vertex_array
        .vertices
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), v| {
            let p = v.position;
            (lo.min(p.x.min(p.y).min(p.z)), hi.max(p.x.max(p.y).max(p.z)))
        });
    if hi > lo {
        (hi - lo) * 0.5
    } else {
        STEAM_HALF
    }
}

/// The share of a steam puff's life it takes to swell into being, and to fade before it dies.
const STEAM_SWELL: [f32; 2] = [0.15, 0.12];

/// How much of a steam puff its volume holds at `age` (0 to 1 of its life): none at its birth or
/// its death, so a puff born whole (a golem's, a pillar's) does not appear or vanish in a frame.
#[must_use]
pub fn swell(age: f32) -> f32 {
    let s = |x: f32| {
        let x = x.clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    s(age / STEAM_SWELL[0]) * s((1.0 - age) / STEAM_SWELL[1])
}

/// What steam is told by of an emitter (the dats' and the animation's emitters alike).
#[derive(Debug, Clone, Copy)]
pub struct Stream {
    /// It never stops.
    pub endless: bool,
    pub lifespan: f64,
    pub start_scale: f32,
    pub final_scale: f32,
    /// One steady sprite standing for a light ([`is_light_sprite`]).
    pub light: bool,
    /// How fast its particles rise (falling below 0).
    pub rise: f32,
}

impl Stream {
    #[must_use]
    pub fn of(e: &pyreal_animation::data::ParticleEmitterInfo) -> Self {
        Self {
            endless: e.total_particles <= 0 && e.total_seconds <= 0.0,
            lifespan: e.lifespan,
            start_scale: e.start_scale,
            final_scale: e.final_scale,
            light: is_light_sprite(e),
            rise: rise(
                e.particle_type,
                [e.a.z * e.max_a, e.b.z * e.max_b, e.c.z * e.max_c],
            ),
        }
    }
}

/// How fast an emitter's particles rise from its `a`, `b` and `c` (each times its most): a
/// swarm's `b` and `c` only swing it about its path, and a still one never moves.
#[must_use]
pub fn rise(ty: pyreal_animation::data::ParticleType, [a, b, c]: [f32; 3]) -> f32 {
    use pyreal_animation::data::ParticleType;
    match ty {
        ParticleType::Unknown | ParticleType::Still => 0.0,
        ParticleType::Swarm => a,
        _ => a + b + c,
    }
}

/// Steam, frost mist and snow: an endless stream of soft additive puffs, white or cold, that swell
/// over a second or more. A white puff that sinks is a frost elemental's snow; one that rises,
/// vapour. A cold one only from a big puff (a pillar of frost's mist), since small cold ones are a
/// spirit's flames.
#[must_use]
pub fn steam_of(tone: Tone, additive: bool, e: &Stream) -> Option<u8> {
    let swells = e.final_scale >= e.start_scale * 1.4 && e.start_scale.max(e.final_scale) >= 0.8;
    let cloud = tone.grey || (tone.cool && e.start_scale >= 2.0);
    if !additive || tone.flat || !e.endless || e.lifespan < 1.5 || !swells || !cloud || e.light {
        return None;
    }
    Some(if tone.cool {
        FROST
    } else if e.rise < 0.0 {
        SNOW
    } else {
        STEAM
    })
}

/// What a placed vent's steam puffs' alpha is multiplied by: retail's opacity at their most. A
/// vent's puffs born three quarters clear make a veil half as thick (a ball's strength is the root
/// of its alpha), a creature's born whole a body.
#[must_use]
pub fn vent_strength(e: &pyreal_animation::data::ParticleEmitterInfo) -> f32 {
    (1.0 - e.start_trans.min(e.final_trans)).clamp(0.0, 1.0)
}

/// How fast a placed steam emitter's puffs rise at the least to be a jet (a geyser's), metres a
/// second.
pub const JET_RISE: f32 = 4.0;
/// How thick a jet's puff is beside a vent's of the same strength once it has spread: a veil the
/// light passes, its churn and shape a vent's (`puffs.wgsl` reads it from a ball's empty entry).
pub const JET_SHARE: f32 = 0.2;
/// The share of a jet puff's life over which it thins to [`JET_SHARE`]: whole at the vent, as
/// retail's are born.
const JET_THINS: f32 = 0.4;
/// The share of a jet puff's life it takes to swell into being: born small at the vent, it shows
/// at once, as retail's do.
const JET_SWELL: f32 = 0.02;

/// Whether a placed steam emitter is a jet: its puffs thrown up fast from its foot (retail's
/// explode type), a geyser's.
#[must_use]
pub fn is_jet(e: &pyreal_animation::data::ParticleEmitterInfo) -> bool {
    e.particle_type == pyreal_animation::data::ParticleType::Explode
        && Stream::of(e).rise >= JET_RISE
}

/// How thick a jet puff is beside its strength at `age` (0 to 1 of its life): whole at the vent,
/// a veil as it spreads.
#[must_use]
pub fn jet_share(age: f32) -> f32 {
    JET_SHARE + (1.0 - JET_SHARE) * (1.0 - ease(age / JET_THINS))
}

/// Whether a jet's host's emitter is its spray: an endless stream of the grey additive puffs
/// round its foot, which rise slowly or not at all (retail's flat pictures), drawn in its volume.
#[must_use]
pub fn is_spray(
    tone: Tone,
    additive: bool,
    e: &pyreal_animation::data::ParticleEmitterInfo,
) -> bool {
    tone.grey
        && additive
        && !tone.flat
        && e.total_particles <= 0
        && e.total_seconds <= 0.0
        && e.max_particles >= 4
        && e.lifespan >= 1.5
        && !is_jet(e)
}

/// How much stronger a jet's spray is than its pictures' opacity: retail's overlap, added, in a
/// bank round its foot.
pub const SPRAY_STRENGTH: f32 = 1.2;
/// How thick a jet's spray is beside its strength: shaped as a vent's, a veil.
pub const SPRAY_SHARE: f32 = 0.3;
/// How much wider a jet's ball is than a vent's of its scale: as wide as retail's picture shows.
pub const JET_REACH: f32 = 1.3;

/// A placed steam puff's alpha as the volume takes it from retail's opacity `alpha`, `age`
/// through its life: a vent's is [`vent_strength`] and [`swell`] as any steam's; a jet's spray
/// its own opacity, swelled; a jet's shows as soon as it is born and falls as the square of
/// retail's, so its stem is whole and its spreading top frays.
#[must_use]
pub fn placed_alpha(
    e: &pyreal_animation::data::ParticleEmitterInfo,
    alpha: f32,
    age: f32,
    spray: bool,
) -> f32 {
    if spray {
        return (alpha * SPRAY_STRENGTH).min(1.0) * swell(age);
    }
    if !is_jet(e) {
        return alpha * vent_strength(e) * swell(age);
    }
    alpha * alpha * ease(age / JET_SWELL) * ease((1.0 - age) / STEAM_SWELL[1])
}

/// Smoothstep over 0 to 1.
fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The least reach (metres) of a creature's puff drawn as smoke or dust, at its largest.
const SMOKE_LEAST: f32 = 0.5;
/// The brightest a smoke's picture is (its visible texels' linear mean): brighter is steam's.
const SMOKE_BRIGHTEST: f32 = 0.35;

/// The smoke or dust a creature's emitter makes, if any: a standing stream of four or more large
/// puffs, laid over (not added), in a dark or grey colour (smoke) or a dusty one (dust). Asked of
/// bodies only, so a chimney's smoke stays its own.
#[must_use]
pub fn smoke_of(sprite: &Sprite, e: &pyreal_animation::data::ParticleEmitterInfo) -> Option<u8> {
    let big = e.start_scale.max(e.final_scale) * sprite.size;
    if sprite.additive()
        || e.total_particles > 0
        || e.total_seconds > 0.0
        || e.max_particles < 4
        || big < SMOKE_LEAST
    {
        return None;
    }
    let c = sprite.colour;
    let (max, min) = (c.max_element(), c.min_element());
    let sat = if max > 0.0 { (max - min) / max } else { 0.0 };
    let warm = c.x >= c.y && c.y >= c.z;
    let green = c.y > c.x && c.y > c.z;
    if warm && sat >= 0.3 && max > 0.1 {
        Some(DUST)
    } else if max <= SMOKE_BRIGHTEST && ((sat < 0.7 && !green) || max < 0.05) {
        Some(if is_vortex(e) { VORTEX } else { SMOKE })
    } else {
        None
    }
}

/// Whether a smoke emitter's puffs are drawn into a whirl: each turning about its emitter
/// (retail's turning type) while it shrinks to a fraction of its birth size and darkens.
#[must_use]
pub fn is_vortex(e: &pyreal_animation::data::ParticleEmitterInfo) -> bool {
    e.particle_type == pyreal_animation::data::ParticleType::ParabolicLvlalr
        && e.final_scale < e.start_scale * 0.3
        && e.final_trans < e.start_trans
}

/// The most of one smoke emitter's puffs drawn: a larger emitter's are thinned to a share kept for
/// each puff's whole life, chosen by its own seed, each grown to hold their volume.
const SMOKE_EMITTER_MOST: i32 = 20;

/// Which of an emitter's puffs smoke keeps: one in this many.
#[must_use]
pub fn stride(e: &pyreal_animation::data::ParticleEmitterInfo) -> u32 {
    u32::try_from((e.max_particles + SMOKE_EMITTER_MOST - 1) / SMOKE_EMITTER_MOST)
        .unwrap_or(1)
        .max(1)
}

/// How much larger a kept puff is drawn, so the thinned ones' volume is held.
#[must_use]
pub fn stride_reach(stride: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // a few
    pyreal_primitives::num::math::powf(stride as f32, 1.0 / 3.0)
}

/// The share of a smoke puff's life it takes to swell into being, and to fade before it dies:
/// slower than steam's, as a creature's smoke is born large and dark.
const SMOKE_SWELL: [f32; 2] = [0.35, 0.15];

/// How much of a smoke puff is drawn at `age` (0 to 1 of its life): none at its birth or its
/// death, so a puff born whole does not appear in a frame.
#[must_use]
pub fn smoke_swell(age: f32) -> f32 {
    let s = |x: f32| {
        let x = x.clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    s(age / SMOKE_SWELL[0]) * s((1.0 - age) / SMOKE_SWELL[1])
}

/// A picture's half width (metres a unit of scale), the linear mean colour of its texels that
/// show, and whether it is slender (three times as long as it is wide: a bolt's).
#[must_use]
pub fn picture(store: &RetailDatStore, gid: DataId) -> (f32, Vec3, bool) {
    let Some(gfx) = decode::<pyreal_assets::GfxObj>(store, DbType::GfxObj, gid) else {
        return (0.5, Vec3::ZERO, false);
    };
    let (wide, long) = gfx
        .vertex_array
        .vertices
        .iter()
        .fold((0.0f32, 0.0f32), |(w, l), v| {
            (w.max(v.position.x.abs()), l.max(v.position.z.abs()))
        });
    let src = TextureSource::new(store);
    let (mut sum, mut n) = (Vec3::ZERO, 0.0);
    for s in &gfx.surfaces {
        let Some(surf) = decode::<pyreal_assets::Surface>(store, DbType::Surface, *s) else {
            continue;
        };
        let c = surf.orig_texture_id.and_then(|t| {
            src.average_dressed(t, true, None)
                .or_else(|_| src.average_linear(t))
                .ok()
        });
        if let Some(c) = c {
            sum += Vec3::from(c);
            n += 1.0;
        }
    }
    (
        wide.max(long).max(0.05),
        sum / f32::max(n, 1.0),
        long >= 3.0 * wide && long > 0.0,
    )
}

/// A smoke puff's colour as the GPU reads it: its picture's mean, gamma 2.2, 5:6:5.
#[must_use]
pub fn tint_bits(c: Vec3) -> u32 {
    let f = |v: f32, most: f32| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 to `most`
        let q = (pyreal_primitives::num::math::powf(v.clamp(0.0, 1.0), 1.0 / 2.2) * most).round()
            as u32;
        q
    };
    f(c.x, 31.0) << 11 | f(c.y, 63.0) << 5 | f(c.z, 31.0)
}
