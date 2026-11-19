//! Particles: the client's own emitters on the CPU, drawn as instanced cards turned to the eye.
//!
//! Nothing here simulates a particle. A placed object whose setup names a default script is a
//! host, a `pyreal-animation` [`MotionDriver`] set up from that setup, as the client's scene holds
//! a scripted static (`EmitterHost`): each frame its parts are placed, its scripts run (their
//! hooks start the emitters), and its [`pyreal_animation::particles::ParticleManager`] steps every
//! emitter by the retail rules (births from the shared `ran2` stream, closed-form paths, the
//! draw-distance gate). A spell's or a portal's emitter is the same: a script played on a host.
//!
//! What is drawn: each live particle is its emitter's graphics object (usually one quad) at the
//! particle's frame and uniform scale, turned to the eye as its degrade mode turns it (the
//! client's `calc_draw_frame`), with `1 - translucency` as its alpha, cut at its degrade level's
//! end. The particles are sorted far to near and handed to the GPU as one instance list; runs of
//! one picture become one instanced draw, merged by distance with the translucent objects.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Affine3A, Vec3, Vec4};
use pyreal_animation::data::AnimAssets;
use pyreal_animation::hooks::HookKind;
use pyreal_animation::MotionDriver;
use pyreal_dat::RetailDatStore;
use pyreal_primitives::num::math::{cosf, hypotf, sinf};
use pyreal_primitives::{DataId, Frame, ServerTime};

use crate::scene::{Draw, MeshId, PipelineKind};

/// The most particles drawn a frame.
pub const MAX_PARTICLES: usize = 16 << 10;
/// One particle's bytes on the GPU: its transform's rows, its material word (the turn's mode in
/// the top byte), its alpha (a glow's colour word), its cell and its flags (1: a lamp's light sprite,
/// its seed 1 when warm).
pub const PARTICLE_SIZE: u64 = 64;
/// How far from the eye a host is stepped with its particles drawn, metres (the client's
/// particle degrade distance when its picture names none).
pub const DRAW_DISTANCE: f32 = pyreal_world_render::particles::DEFAULT_PARTICLE_DEGRADE_DISTANCE;

/// One emitter picture: its mesh in the pools, how it turns to the eye, and how far (over its
/// scale) it is drawn.
#[derive(Debug, Clone)]
pub struct Sprite {
    pub mesh: MeshId,
    pub draws: Vec<Draw>,
    pub mode: u32,
    /// Its near level's end over the particle's scale, metres; past it nothing is drawn.
    pub reach: f32,
    /// Its colour and shape, which say whether it draws smoke or mist ([`Look`]).
    pub tone: crate::lamps::Tone,
    /// Its half width, the mean colour of what shows of it, and whether it is long and thin: which
    /// say whether a creature's puffs are smoke or dust ([`crate::puffs::smoke_of`]) and its
    /// pictures lightning ([`crate::bolts`]).
    pub size: f32,
    pub colour: Vec3,
    pub slender: bool,
    /// Its picture's half width over the particle's scale, metres.
    pub half: f32,
    /// The colour of the flame it draws, if it is one ([`crate::body_fire`]).
    pub fire: Option<Vec3>,
    /// It pictures light, a ball or a star, which a creature's particle is drawn as ([`Look::Glow`]).
    pub glow: Option<crate::glow::Glow>,
}

impl Sprite {
    /// Whether any of its draws adds its light.
    #[must_use]
    pub fn additive(&self) -> bool {
        self.draws.iter().any(additive)
    }
}

/// A scripted or animated static: its driver, where it stands (render space), its cell slot and
/// object; an animated one's parts' instances (each its part's index), the clock its animation
/// last stepped at, and the turn its animation gives it a step (a gull wheeling round its spot).
pub struct Host {
    pub driver: MotionDriver,
    pub frame: Frame,
    pub cell: u32,
    pub object: u32,
    pub animated: bool,
    pub instances: Vec<(usize, u32)>,
    pub stepped: Option<f64>,
    pub omega: pyreal_primitives::Vec3,
    /// Its fire is drawn as flames ([`crate::flames`]).
    pub flamed: bool,
    /// Its fire is a bed of retail's fire puffs, drawn as fire ([`Look::Fire`]).
    pub puffed: bool,
    /// Its bed of fire is a hearth's or a wreath's, drawn as a body of fire
    /// ([`crate::body_fire::hearth`], [`crate::body_fire::wreath`]) while flames are.
    pub hearth: bool,
    /// It holds nothing but retail's snowfall ([`is_snowfall`]): no longer stepped.
    pub quiet: bool,
    /// Its emitters are a waterfall's ([`crate::falls::is_fall`]): not stepped or drawn while falls
    /// are drawn.
    pub fall: bool,
    /// It is a shadow's ([`crate::body::shadow_parts`]): its eyes glow ([`is_eye`]).
    pub shadow: bool,
    /// The level faces of a basin's water on its parts ([`crate::fountains::pool_faces`]), where
    /// its jets land.
    pub pools: Vec<crate::fountains::PoolFace>,
    /// Where its own streams meet that water ([`crate::fountains::spills`]): rung there.
    pub spills: Vec<crate::fountains::Spill>,
    /// Which of a lighthouse beacon's hosts it is, if it is one ([`crate::beacons::is_beacon`]).
    pub beacon: Option<u8>,
}

/// The shortest step an animated static takes (the physics quantum's, less its tolerance) and the
/// longest gap it steps over rather than starting again.
const STEP_SHORTEST: f64 = 1.0 / 30.0 - 0.002;
const STEP_LONGEST: f64 = 2.0;

impl std::fmt::Debug for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Host")
            .field("origin", &self.frame.origin)
            .field("emitters", &self.driver.particles.len())
            .finish_non_exhaustive()
    }
}

/// How a particle is drawn: as its picture, as a billow of smoke ([`is_smoke`]) or as a bank of
/// mist ([`is_mist`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Picture,
    Smoke,
    Mist,
    /// Liquid ([`blob_of`]): water, or blood.
    Blob(u8),
    /// A body's vapour, one volume ([`crate::puffs`]): its kind.
    Puff(u8),
    /// A creature's light ([`crate::glow`]): its shape, a ball (0) or a star (1).
    Glow(u8),
    /// A frost elemental's snowflake (0) or glint of ice (1) ([`crate::rime`]), drawn as light.
    Flake(u8),
    /// A puff of a bed of fire (a brazier's, a pyre's): churning flame rising through it, hot
    /// while it is young and cooling to red as it ages.
    Fire,
    /// A speck of dust drawn into a portal ([`crate::gates`]): its look's word in its seed.
    Mote,
    /// A lighthouse beacon's part ([`crate::beacons`]): retail's pictures drawn as white light,
    /// its heart or its shaft.
    Beacon(u8),
}

/// Whether a look is laid over what is behind it whatever its picture: mist and dust.
fn laid(look: Look) -> bool {
    matches!(look, Look::Mist | Look::Mote)
}

/// The sprites drawn as liquid, and which: the water golems' (0) and the blood golems' (1).
const BLOB_SPRITES: [(u32, u8); 2] = [(0x0100_1BBE, 0), (0x0100_3BA3, 1)];
/// The most balls of one body merged as one liquid.
pub const BLOB_RUN: usize = 255;
/// A steam ball's reach over its scale (`puffs.wgsl`'s `PUFF_SIZE`).
const PUFF_REACH: f32 = 0.85;

/// A blood ball's radius a unit of its particle's scale, as `blobs.wgsl`'s `BLOOD_SIZE`.
const BLOOD_SIZE: f32 = 0.28;
/// Blood drips: each ball may start one a beat (seconds), one beat in `DRIP_ODDS`; a drop swells
/// under its ball for `DRIP_HANG` s, then falls; its radius, metres; at most this many at once.
const DRIP_BEAT: f64 = 0.8;
const DRIP_ODDS: u32 = 2;
const DRIP_HANG: f32 = 0.55;
const DRIP_RADIUS: [f32; 2] = [0.022, 0.04];
const DRIP_MOST: usize = 256;
const GRAVITY: f32 = 9.8;

/// A drop of blood dripping from a blood golem: hanging under its ball as it swells, then falling
/// until it reaches the body's feet.
#[derive(Debug, Clone, Copy)]
struct Drip {
    /// The ball it hangs from (its seed) and where under it (a share of its radius, x and y).
    ball: u32,
    under: [f32; 2],
    group: u32,
    cell: u32,
    at: Vec3,
    fall: f32,
    radius: f32,
    born: f64,
    floor: f32,
    hung: bool,
}

/// A number from 0 to 1 dealt by `h`.
fn unit(h: u32) -> f32 {
    let mut x = h.wrapping_mul(0x9E37_79B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    #[allow(clippy::cast_precision_loss)] // 16 bits
    let u = (x >> 16) as f32 / 65535.0;
    u
}

/// The liquid an emitter's sprite `id` stands for, if it is drawn as liquid.
#[must_use]
pub fn blob_of(id: u32) -> Option<u8> {
    BLOB_SPRITES.iter().find(|b| b.0 == id).map(|b| b.1)
}

/// A run of vapour balls in the instance list: the first, how many, and its balls' mean distance.
pub type PuffRun = (u32, u32, f32);

/// One drawn particle, before it goes to the GPU.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drawn {
    /// Its picture (the emitter's graphics object id).
    pub sprite: u32,
    pub transform: Affine3A,
    pub alpha: f32,
    pub cell: u32,
    /// Distance from the eye, metres.
    pub distance: f32,
    /// One additive sprite standing for a lamp's light (drawn warm and flickering).
    pub light: bool,
    pub look: Look,
    /// How far through its life, 0 to 1, and a seed of its own.
    pub age: f32,
    pub seed: u32,
    /// The body or host it came from: liquid merges only with its own.
    pub group: u32,
}

/// One instanced draw of a run of particles with one picture, sorted with the translucent
/// objects by `distance`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Batch {
    pub draw: Draw,
    pub first: u32,
    pub count: u32,
    pub distance: f32,
}

/// Every host, every picture, and this frame's particles.
pub struct Particles {
    /// The dats as the animation code reads them; `None` leaves the world without particles.
    pub assets: Option<Arc<dyn AnimAssets>>,
    pub hosts: Vec<Host>,
    /// Each emitter picture, by graphics object id (`None`: it draws nothing).
    // ORDER-OK: looked up by id, never iterated.
    pub sprites: HashMap<u32, Option<Sprite>>,
    /// Whether particles are stepped and drawn.
    pub on: bool,
    /// Leave out the sprites that only stand for a lamp's light (the lamps light per pixel).
    pub skip_light_sprites: bool,
    /// The emitter pictures a flame stands for, left out of a flamed host while flames are drawn.
    // ORDER-OK: looked up by id, never iterated.
    pub flamed: std::collections::HashSet<u32>,
    /// The emitter pictures a bed of fire burns with, drawn as fire in a puffed host while flames
    /// are drawn.
    // ORDER-OK: looked up by id, never iterated.
    pub puffed: std::collections::HashSet<u32>,
    pub skip_flamed: bool,
    /// Last frame's: the particles drawn, their batches, and the live ones in all.
    pub drawn: Vec<Drawn>,
    pub batches: Vec<Batch>,
    pub live: usize,
    /// The GPU's particle list: each batch's particles (an index into `drawn`) with its material
    /// word (the material, the turn's mode in the top byte).
    pub instances: Vec<(usize, u32)>,
    /// Last frame's CPU time stepping the hosts and gathering the particles, ms.
    pub cpu_ms: f64,
    /// Whether smoke, mist and a creature's liquid, vapour, glow and frost are drawn as such
    /// ([`Look`]); off, every particle is its picture, as retail draws it. And the wind that drifts
    /// smoke (as [`crate::wind::wind`] gives it).
    pub smoke: bool,
    pub wind: Vec3,
    /// Whether portals are drawn as a vortex with dust drawn into them ([`crate::gates`]); off,
    /// as retail draws their swirl. And the air the dust is in.
    pub portals: bool,
    /// Whether the lighthouses' beacons are drawn as a magical white light ([`crate::beacons`]);
    /// off, as retail's added pictures.
    pub beacons: bool,
    /// Whether waterfalls are drawn as falling sheets of water ([`crate::falls`]); off, as
    /// retail's cards.
    pub falls: bool,
    /// Whether fountains' jets are drawn as streams of water ([`crate::fountains`]); off, as
    /// retail's bubbles.
    pub fountains: bool,
    pub air: crate::gates::Air,
    /// Each portal's colour as last drawn, by its default script, for the tunnel it leads into.
    // ORDER-OK: looked up by script, never iterated.
    pub portal_colours: HashMap<u32, Vec3>,
    /// What the pictures' reaches are scaled by: Object Detail's share, never under the game's.
    pub detail: f32,
    /// Particles of emitters the engine does not step (the client's own objects'), gathered for
    /// the next frame by [`Self::gather`]; drawn with the hosts' and emptied by the frame.
    pub external: Vec<Drawn>,
    /// Bodies gathered since the last frame, which numbers each one's particles.
    pub gathered: u32,
    /// The liquid balls in the instance list: the first and how many ([`Look::Blob`]).
    pub blobs: (u32, u32),
    /// The vapour balls, after the liquid ([`Look::Puff`]).
    pub puffs: (u32, u32),
    /// Each run of vapour balls in `puffs` (first, how many) and how far its balls are on the
    /// whole, far to near, so it is drawn among the translucent in order.
    pub puff_runs: Vec<PuffRun>,
    /// Lightning in a body's volume ([`crate::bolts`]): each strike by its particle's seed, kept
    /// for its own life, the strikes gathered since the last frame (body, where it stands, seed,
    /// brightness, cell, its lamp's share), the records that are no particle's (a strike's
    /// segments and lines: rows, alpha, cell, flags), the segments in the list, and this frame's
    /// strikes' lamps.
    // ORDER-OK: looked up by seed; a body's are sorted by seed before they are drawn.
    pub strikes: HashMap<u32, crate::bolts::Strike>,
    pub struck: Vec<(u32, Vec3, u32, f32, u32, f32)>,
    pub records: Vec<([f32; 12], f32, u32, u32)>,
    pub bolts: (u32, u32),
    pub flashes: Vec<crate::lamps::Lamp>,
    /// Each body's volume run's material word this frame.
    // ORDER-OK: looked up by body, never iterated.
    pub run_words: HashMap<u32, u32>,
    /// Whether bodies of fire are drawn as fire, and those gathered for the next frame
    /// ([`crate::body_fire`]).
    pub fire_bodies: bool,
    pub fires: Vec<crate::body_fire::BodyFire>,
    /// The lamps the creatures gathered since the last frame carry, each where its light is
    /// ([`crate::glow`]); lit and emptied by the frame.
    pub glow_lamps: Vec<crate::lamps::Lamp>,
    /// Each body emitter's role ([`crate::roles`]).
    pub roles: crate::roles::Roles,
    /// The clock's seconds at the last step.
    pub clock: f64,
    /// The blood dripping from blood golems.
    pub blood: Blood,
}

/// The clock at the last step and at the last drip, the blood dripping, and the feet of the
/// bodies gathered since the last frame that bleed (body number, height).
#[derive(Debug, Default)]
pub struct Blood {
    now: f64,
    dripped: f64,
    drips: Vec<Drip>,
    floors: Vec<(u32, f32)>,
}

impl std::fmt::Debug for Particles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Particles")
            .field("hosts", &self.hosts.len())
            .field("sprites", &self.sprites.len())
            .field("drawn", &self.drawn.len())
            .finish_non_exhaustive()
    }
}

impl Default for Particles {
    fn default() -> Self {
        Self {
            assets: None,
            hosts: Vec::new(),
            sprites: HashMap::new(),
            on: true,
            skip_light_sprites: false,
            flamed: std::collections::HashSet::new(),
            puffed: std::collections::HashSet::new(),
            skip_flamed: false,
            drawn: Vec::new(),
            batches: Vec::new(),
            live: 0,
            instances: Vec::new(),
            cpu_ms: 0.0,
            smoke: true,
            wind: Vec3::ZERO,
            portals: true,
            beacons: true,
            falls: true,
            fountains: false,
            air: crate::gates::Air::default(),
            portal_colours: HashMap::new(),
            detail: 1.0,
            external: Vec::new(),
            gathered: 0,
            blobs: (0, 0),
            puffs: (0, 0),
            puff_runs: Vec::new(),
            strikes: HashMap::new(),
            struck: Vec::new(),
            records: Vec::new(),
            bolts: (0, 0),
            flashes: Vec::new(),
            run_words: HashMap::new(),
            fire_bodies: true,
            fires: Vec::new(),
            glow_lamps: Vec::new(),
            roles: crate::roles::Roles::new(),
            clock: 0.0,
            blood: Blood::default(),
        }
    }
}

/// The emitter pictures script `id` can start, following the scripts it calls, a few deep.
#[must_use]
pub fn script_sprites(assets: &dyn AnimAssets, id: DataId) -> Vec<DataId> {
    let mut out = Vec::new();
    let mut todo = vec![(id, 0u32)];
    while let Some((s, depth)) = todo.pop() {
        let Some(script) = assets.script(s) else {
            continue;
        };
        for step in &script.steps {
            match &step.hook.kind {
                HookKind::CreateParticle(c) | HookKind::CreateBlockingParticle(c) => {
                    if let Some(e) = assets.emitter_info(c.emitter_info_id) {
                        out.push(sprite_of(&e));
                    }
                }
                HookKind::CallPes(p) if depth < 3 => todo.push((p.pes, depth + 1)),
                _ => {}
            }
        }
    }
    out.sort_by_key(|d| d.0);
    out.dedup();
    out
}

/// Whether an emitter shows one additive sprite standing for something's light: at most two
/// particles that hold their size and strength, where fire and smoke are many that grow and fade
/// (the client's scene leaves these out when lamps light per pixel).
#[must_use]
pub fn is_light_sprite(e: &pyreal_animation::data::ParticleEmitterInfo) -> bool {
    e.max_particles <= 2
        && (e.start_scale - e.final_scale).abs() < 0.5
        && (e.start_trans - e.final_trans).abs() < 0.02
}

/// How many times its picture a shadow's eyes glow, carried in the particle's alpha.
pub const EYE_GLOW: f32 = 3.0;

/// Whether a shadow's emitter shows its eyes: a steady, small additive sprite riding one of its
/// parts.
#[must_use]
pub fn is_eye(e: &pyreal_animation::particles::ParticleEmitter, additive: bool) -> bool {
    additive
        && is_light_sprite(&e.info)
        && e.info.is_parent_local != 0
        && e.part_index != pyreal_animation::particles::NO_PART
        && e.info.start_scale <= 0.25
}

/// Whether an emitter of a grey sprite makes smoke: a standing stream of particles that rise,
/// grow and fade over a few seconds (a chimney's, a fire's), drawn as lit billows.
#[must_use]
pub fn is_smoke(e: &pyreal_animation::data::ParticleEmitterInfo) -> bool {
    let rise = e.a.z * e.max_a + e.b.z * e.max_b + e.c.z * e.max_c;
    e.total_particles <= 0
        && e.total_seconds <= 0.0
        && e.max_particles >= 4
        && e.lifespan >= 2.0
        && e.final_scale > e.start_scale * 1.2
        && e.final_trans > e.start_trans
        && rise > 0.0
}

/// Whether an emitter is retail's standing snowfall: endless, born metres over its host and
/// falling fast. The weather draws the snow when it snows ([`crate::weather`]), so these are not
/// drawn.
#[must_use]
pub fn is_snowfall(e: &pyreal_animation::data::ParticleEmitterInfo) -> bool {
    let fall = e.a.z * e.max_a + e.b.z * e.max_b + e.c.z * e.max_c;
    e.total_particles <= 0
        && e.total_seconds <= 0.0
        && e.offset_dir.z > 0.9
        && e.max_offset >= 8.0
        && fall <= -2.0
}

/// Whether an emitter of a grey sheet lying flat makes ground mist: a standing patch of particles
/// that spread wide over a few seconds, drawn as a lit bank of mist.
#[must_use]
pub fn is_mist(e: &pyreal_animation::data::ParticleEmitterInfo) -> bool {
    e.total_particles <= 0
        && e.total_seconds <= 0.0
        && e.max_particles >= 4
        && e.lifespan >= 2.0
        && e.start_scale.max(e.final_scale) >= 2.0
}

/// How an emitter's particles are drawn, smoke and mist being on (`shaped`).
#[must_use]
pub fn look_of(
    sprite: &Sprite,
    e: &pyreal_animation::data::ParticleEmitterInfo,
    shaped: bool,
) -> Look {
    let t = sprite.tone;
    if !shaped {
        Look::Picture
    } else if let Some(k) = crate::puffs::kind_of(sprite, e) {
        Look::Puff(k)
    } else if (t.grey || t.cool) && t.flat && sprite.mode < 2 && is_mist(e) {
        Look::Mist
    } else if t.grey && !sprite.additive() && is_smoke(e) {
        Look::Smoke
    } else {
        Look::Picture
    }
}

/// How far the wind carries smoke a second of its age, metres per unit of breeze; and how much
/// larger a billow is drawn than its particle at its birth and its death (its edge is soft, and
/// it spreads as it rises).
const SMOKE_DRIFT: f32 = 0.45;
const SMOKE_GROW: [f32; 2] = [1.0, 1.9];

/// A live particle's place, scale and age as smoke drifts it: carried downwind by more the older
/// it is, so the plume bends.
fn smoke_of(p: &pyreal_animation::particles::Particle, wind: Vec3) -> (Vec3, f32, f32) {
    #[allow(clippy::cast_possible_truncation)] // seconds of a particle's life
    let (age, life) = (p.lifetime as f32, p.lifespan.max(1e-3) as f32);
    let t = (age / life).clamp(0.0, 1.0);
    let drift = Vec3::new(wind.x, wind.y, 0.0) * SMOKE_DRIFT * age * (0.4 + 0.6 * t);
    (
        drift,
        p.scale * (SMOKE_GROW[0] + (SMOKE_GROW[1] - SMOKE_GROW[0]) * t),
        t,
    )
}

/// A live particle's transform and age (0 to 1) as `look` draws it: smoke drifted and spread
/// ([`smoke_of`]); mist a ball of the sheet's reach at its place, which the shader turns to the
/// eye; and its offset from where its emitter put it.
fn shape(
    look: Look,
    p: &pyreal_animation::particles::Particle,
    wind: Vec3,
) -> (Affine3A, f32, Vec3) {
    #[allow(clippy::cast_possible_truncation)] // a particle's share of its life
    let t = (p.lifetime / p.lifespan.max(1e-3)).clamp(0.0, 1.0) as f32;
    match look {
        Look::Picture => (affine(&p.frame, p.scale), 0.0, Vec3::ZERO),
        Look::Glow(_) | Look::Flake(_) | Look::Fire | Look::Beacon(_) => {
            (affine(&p.frame, p.scale), t, Vec3::ZERO)
        }
        Look::Smoke => {
            let (drift, scale, age) = smoke_of(p, wind);
            let mut a = affine(&p.frame, scale);
            a.translation += glam::Vec3A::from(drift);
            (a, age, drift)
        }
        Look::Mist | Look::Blob(_) | Look::Puff(_) | Look::Mote => {
            let o = p.frame.origin;
            let reach = if look == Look::Puff(crate::puffs::SNOW) {
                crate::puffs::SNOW_REACH
            } else {
                1.0
            };
            let a = Affine3A::from_scale_rotation_translation(
                Vec3::splat(p.scale * reach),
                glam::Quat::IDENTITY,
                Vec3::new(o.x, o.y, o.z),
            );
            (a, t, Vec3::ZERO)
        }
    }
}

/// A fire puff's alpha word: its picture's colour (linear, brightest channel one), a byte each.
fn fire_word(colour: Vec3) -> u32 {
    let c = colour.max(Vec3::ZERO) / colour.max_element().max(1e-4);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 to 255
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    byte(c.x) | byte(c.y) << 8 | byte(c.z) << 16
}

/// A stable seed for a particle, from what it was dealt at birth (an endless emitter re-bases
/// `birthtime` every update, so that cannot be used).
pub(crate) fn seed_of(p: &pyreal_animation::particles::Particle) -> u32 {
    #[allow(clippy::cast_possible_truncation)] // a hash of the birth's draws
    let life = (p.lifespan.to_bits() ^ (p.lifespan.to_bits() >> 32)) as u32;
    let mut h = 0x811C_9DC5u32;
    for w in [
        p.a.x.to_bits(),
        p.a.y.to_bits(),
        p.a.z.to_bits(),
        p.offset.x.to_bits(),
        p.offset.y.to_bits(),
        p.start_scale.to_bits(),
        p.final_scale.to_bits(),
        life,
    ] {
        h = (h ^ w).wrapping_mul(0x0100_0193);
        h ^= h >> 15;
    }
    h & 0x7_FFFF
}

/// The graphics object an emitter draws its particles with.
#[must_use]
pub fn sprite_of(e: &pyreal_animation::data::ParticleEmitterInfo) -> DataId {
    if e.hw_gfxobj_id.0 != 0 {
        e.hw_gfxobj_id
    } else {
        e.gfxobj_id
    }
}

/// Marks a puff's transform with where its body stands, `at` (its noise is laid out from there, so
/// it moves with the body and holds still while it stands): the offset from the puff, in the
/// entries a ball's transform leaves empty. `x_axis.y`, also empty, holds its emitter's largest
/// puff (its scale), which steam's noise is sized by.
fn anchor(t: &mut Affine3A, at: Vec3) {
    let o = at - Vec3::from(t.translation);
    t.matrix3.y_axis.x = o.x;
    t.matrix3.z_axis.x = o.y;
    t.matrix3.z_axis.y = o.z;
}

/// Where emitter `e` of `driver` stands and the axis its particles turn about (its frame's
/// forward): the part it hangs from, or the object itself.
pub(crate) fn whirl(
    driver: &MotionDriver,
    e: &pyreal_animation::particles::ParticleEmitter,
) -> (Vec3, Vec3) {
    let parent = driver
        .part_array
        .parts
        .get(e.part_index as usize)
        .filter(|_| e.part_index != pyreal_animation::particles::NO_PART)
        .map_or_else(|| driver.particle_parent_frame(), |p| p.pos);
    let t = crate::asset::object::frame_affine(&parent)
        * crate::asset::object::frame_affine(&e.parent_offset);
    (
        Vec3::from(t.translation),
        Vec3::from(t.matrix3.y_axis).normalize_or(Vec3::Y),
    )
}

/// A frame as a transform (its rotation and origin), scaled uniformly by `s`.
fn affine(f: &Frame, s: f32) -> Affine3A {
    crate::asset::object::frame_affine(f) * Affine3A::from_scale(Vec3::splat(s))
}

impl Particles {
    /// A host for setup `setup` at `frame` (render space) and `scale`, in cell slot `cell`, a part
    /// of object `object`: when its setup names a default script. Whether one was made.
    pub fn spawn(
        &mut self,
        setup: DataId,
        frame: Frame,
        scale: f32,
        cell: u32,
        object: u32,
    ) -> bool {
        self.host(setup, frame, scale, cell, object, false)
    }

    /// [`Self::spawn`] for a setup with no script or animation too, its parts placed once and
    /// never stepped: a wall fountain whose stream rings its basin.
    pub fn hold(
        &mut self,
        setup: DataId,
        frame: Frame,
        scale: f32,
        cell: u32,
        object: u32,
    ) -> bool {
        self.host(setup, frame, scale, cell, object, true)
    }

    fn host(
        &mut self,
        setup: DataId,
        frame: Frame,
        scale: f32,
        cell: u32,
        object: u32,
        bare: bool,
    ) -> bool {
        let Some(assets) = self.assets.clone() else {
            return false;
        };
        if pyreal_dat::divine_type(setup) != Some(pyreal_dat::DbType::Setup) {
            return false;
        }
        let Some(data) = assets.setup(setup) else {
            return false;
        };
        let animated = data.default_animation.is_some();
        let still = data.default_script.is_none() && !animated;
        if still && !bare {
            return false;
        }
        let mut driver = MotionDriver::new(assets);
        if !driver.set_setup(data) {
            return false;
        }
        if (scale - 1.0).abs() > 1e-6 {
            driver.scale = scale;
            driver
                .part_array
                .set_scale_internal(pyreal_primitives::Vec3::new(scale, scale, scale));
        }
        driver.env.in_cell = true;
        driver.env.position = pyreal_primitives::Position::new(pyreal_primitives::CellId(0), frame);
        driver.update_parts(&frame);
        self.hosts.push(Host {
            driver,
            frame,
            cell,
            object,
            animated,
            instances: Vec::new(),
            stepped: None,
            omega: pyreal_primitives::Vec3::ZERO,
            flamed: false,
            puffed: false,
            hearth: false,
            quiet: still,
            fall: false,
            shadow: false,
            pools: Vec::new(),
            spills: Vec::new(),
            beacon: None,
        });
        true
    }

    /// A host with no setup at `frame` playing script `script` (a spell's, a portal's): its
    /// emitters hang off the frame. Whether the script would play.
    pub fn play(&mut self, script: DataId, frame: Frame, cell: u32) -> bool {
        let Some(assets) = self.assets.clone() else {
            return false;
        };
        let mut driver = MotionDriver::new(assets);
        driver.env.in_cell = true;
        driver.env.position = pyreal_primitives::Position::new(pyreal_primitives::CellId(0), frame);
        driver.update_parts(&frame);
        if !driver.play_script_internal(script) {
            return false;
        }
        self.hosts.push(Host {
            driver,
            frame,
            cell,
            object: u32::MAX,
            animated: false,
            instances: Vec::new(),
            stepped: None,
            omega: pyreal_primitives::Vec3::ZERO,
            flamed: false,
            puffed: false,
            hearth: false,
            quiet: false,
            fall: false,
            shadow: false,
            pools: Vec::new(),
            spills: Vec::new(),
            beacon: None,
        });
        true
    }

    /// The pictures setup `setup`'s default script can start (its meshes are wanted before the
    /// script runs).
    #[must_use]
    pub fn setup_sprites(&self, setup: DataId) -> Vec<DataId> {
        let Some(assets) = self.assets.as_deref() else {
            return Vec::new();
        };
        assets
            .setup(setup)
            .and_then(|s| s.default_script)
            .map_or_else(Vec::new, |s| script_sprites(assets, s))
    }

    /// Steps every host to `now` (seconds on the game's clock) as the client steps a scripted
    /// static: its parts placed, its scripts run, its emitters stepped (drawn within their
    /// distance of `eye`), its ramps advanced; the events it raised are dropped.
    pub fn step(&mut self, now: f64, eye: Vec3) {
        let started = std::time::Instant::now();
        self.clock = now;
        self.blood.now = now;
        for h in &mut self.hosts {
            if h.quiet || self.falls && h.fall {
                continue;
            }
            h.driver.cur_time = ServerTime(now);
            // An animated static's default animation steps with the world's quantum, turning by
            // its spin a step (the old scene's `EmitterHost::animate`).
            let mut advanced = false;
            if h.animated {
                match h.stepped {
                    Some(last) if now - last >= STEP_SHORTEST => {
                        if now - last <= STEP_LONGEST {
                            let _ =
                                pyreal_primitives::MotionSource::advance(&mut h.driver, now - last);
                            pyreal_primitives::frame::grotate(&mut h.frame, h.omega);
                            h.driver.env.position.frame = h.frame;
                            advanced = true;
                        }
                        h.stepped = Some(now);
                    }
                    Some(_) => {}
                    None => h.stepped = Some(now),
                }
            }
            h.driver.update_parts(&h.frame);
            if advanced {
                pyreal_primitives::MotionSource::process_hooks(&mut h.driver);
            }
            h.driver.update_scripts();
            let o = h.frame.origin;
            let far = Vec3::new(o.x, o.y, o.z).distance(eye);
            let reach = reach_of(&self.sprites, &h.driver, self.detail);
            h.driver.update_particles(far <= reach);
            h.driver.update_fp_hooks();
            for e in h.driver.take_events() {
                if let pyreal_animation::AnimEvent::SetOmega(axis) = e {
                    h.omega = axis;
                }
            }
            let _ = h.driver.take_effects();
            h.quiet = !h.animated
                && !h.driver.particles.is_empty()
                && h.driver.particles.iter().all(|e| is_snowfall(&e.info));
            h.fall = !h.animated && h.beacon.is_none() && crate::falls::is_fall(&h.driver);
        }
        self.cpu_ms = started.elapsed().as_secs_f64() * 1e3;
    }

    /// Moves the animated statics' part instances to where their animations put them.
    pub fn pose(&self, scene: &mut crate::scene::Scene) {
        for h in self.hosts.iter().filter(|h| !h.instances.is_empty()) {
            for &(k, id) in &h.instances {
                if let Some(p) = h.driver.part_array.parts.get(k) {
                    scene.set_transform(id, crate::body::part_affine(&p.pos, p.gfxobj_scale));
                }
            }
        }
    }

    /// The particle half of a moving object's tick, which the client's own step leaves to the
    /// presentation: its emitters stepped, drawn within their reach of `eye`, the object standing
    /// at `at` (a portal's swirl).
    pub fn step_object(&self, driver: &mut MotionDriver, at: Vec3, eye: Vec3) {
        let draw = at.distance(eye) <= self.reach(driver);
        driver.update_particles(draw);
    }

    /// How far from the eye `driver`'s emitters are stepped and drawn: the widest of their
    /// pictures' reaches.
    #[must_use]
    pub fn reach(&self, driver: &MotionDriver) -> f32 {
        reach_of(&self.sprites, driver, self.detail)
    }

    /// Notes portal `g`'s colour, `driver`'s.
    fn note_portal(&mut self, driver: &MotionDriver, g: &crate::gates::Gate) {
        if let Some(script) = crate::gates::script_of(driver) {
            self.portal_colours.insert(script, g.colour);
        }
    }

    /// The colour of a portal of `setup` as last drawn, if one has been.
    #[must_use]
    pub fn portal_colour(&self, setup: u32) -> Option<Vec3> {
        let setup = self.assets.as_ref()?.setup(DataId(setup))?;
        self.portal_colours.get(&setup.default_script?.0).copied()
    }

    /// The pictures of `driver`'s emitters (and a portal's script's) that are not ready to draw
    /// ([`prepare_sprite`]).
    #[must_use]
    pub fn missing_sprites(&self, driver: &MotionDriver) -> Vec<DataId> {
        let mut out: Vec<DataId> = driver
            .particles
            .iter()
            .map(|e| sprite_of(&e.info))
            .chain(crate::gates::pictures(driver))
            .filter(|g| !self.sprites.contains_key(&g.0))
            .collect();
        out.sort_by_key(|g| g.0);
        out.dedup();
        out
    }

    /// Adds the live particles of `driver`'s emitters, which the client steps, to the next
    /// frame's: each moved by `offset` into render space and seen through cell slot `cell`; a
    /// `shadow`'s eyes glow. Each emitter is drawn as its role says ([`crate::roles`]): its fire
    /// with the body's fire, its lightning as strikes in the body's volume. A `held` thing's light
    /// carries a held thing's lamp ([`crate::glow::HELD_INTENSITY`]).
    pub fn gather(
        &mut self,
        driver: &MotionDriver,
        cell: u32,
        offset: Vec3,
        eye: Vec3,
        shadow: bool,
        held: bool,
    ) {
        use crate::roles::Role;
        // A portal is drawn as a vortex, its swirl's particles left out; else as retail's swirl.
        let portal = crate::gates::is_portal(driver);
        let gate = portal
            .then(|| crate::gates::gate_of(driver, &self.sprites, offset))
            .flatten();
        if let Some(g) = gate {
            self.note_portal(driver, &g);
            if self.portals {
                let shown = crate::gates::shown(g.centre.distance(eye), self.reach(driver));
                let (clock, air, group) = (self.clock, self.air, self.gathered);
                self.external
                    .extend(crate::gates::drawn(&g, cell, eye, clock, air, shown, group));
                if shown > 0.0 {
                    self.glow_lamps.push(crate::gates::lamp(&g, cell, shown));
                }
                self.gathered += 1;
                return;
            }
        }
        let roles: Vec<Option<Role>> = driver.particles.iter().map(|e| self.role(e)).collect();
        if self.fire_bodies {
            let burning = driver
                .particles
                .iter()
                .zip(&roles)
                .filter_map(|(e, r)| match r {
                    Some(Role::Fire(c)) => Some((e, *c)),
                    _ => None,
                });
            if let Some(f) = crate::body_fire::gather(driver, burning, offset, cell) {
                self.fires.push(f);
            }
        }
        let a = driver.particle_parent_frame().origin;
        let stands = Vec3::new(a.x, a.y, a.z) + offset;
        // A body with a volume has lightning in it ([`crate::bolts`]).
        let clouded = self.smoke && roles.iter().any(|r| matches!(r, Some(Role::Volume(_))));
        // A frost elemental's sparkles are glints in its snow: they light nothing round it.
        let snowy = self.smoke
            && roles
                .iter()
                .any(|r| matches!(r, Some(Role::Volume(crate::puffs::SNOW))));
        let body = std::ptr::from_ref(driver) as usize;
        // The creature's lamps, its light's and its acid's: where each is, weighed by how much
        // shows, and its colour.
        let mut lamps = [(Vec3::ZERO, Vec3::ZERO, 0.0f32); 2];
        for (e, role) in driver.particles.iter().zip(&roles) {
            let id = sprite_of(&e.info).0;
            let (Some(role), Some(Some(sprite))) = (*role, self.sprites.get(&id)) else {
                continue;
            };
            let role = match role {
                Role::Fire(_) if self.fire_bodies => continue,
                Role::Fire(_) => crate::roles::lone(sprite, &e.info),
                Role::Bolt if !clouded => crate::roles::lone(sprite, &e.info),
                Role::Volume(_) if !self.smoke => Role::Picture,
                r => r,
            };
            if role == Role::Bolt {
                let now = self.clock;
                for p in e.live() {
                    let seed = seed_of(p);
                    if self.strikes.contains_key(&seed) {
                        continue;
                    }
                    if self.strikes.len() >= crate::bolts::MOST
                        || self.strikes.values().filter(|s| s.body == body).count()
                            >= crate::bolts::BODY_MOST
                    {
                        break;
                    }
                    #[allow(clippy::cast_possible_truncation)] // seconds of a particle's life
                    let life = (p.lifespan as f32).max(crate::bolts::STRIKE_LEAST);
                    let born = now - p.lifetime + f64::from(crate::bolts::delay(seed));
                    if now - born >= f64::from(life) {
                        continue;
                    }
                    let o = p.frame.origin;
                    let at = Vec3::new(o.x, o.y, o.z) + offset - stands;
                    let len = e.info.start_scale * sprite.size * 2.0;
                    let mut s = crate::bolts::Strike::grow(seed, at, len, sprite.colour);
                    (s.born, s.life, s.body) = (born, life, body);
                    // As strong as its picture is drawn at its birth (a golem's crackle faint).
                    s.strength = (1.0 - e.info.start_trans).clamp(0.0, 1.0);
                    self.strikes.insert(seed, s);
                }
                continue;
            }
            let eyes = shadow && is_eye(e, sprite.additive());
            let (light, glow) = if eyes {
                (false, EYE_GLOW)
            } else {
                (role == Role::Light, 1.0)
            };
            if self.skip_light_sprites && light {
                continue;
            }
            let look = match role {
                _ if !self.smoke || portal => Look::Picture,
                Role::Blob(b) => Look::Blob(b),
                Role::Volume(k) => Look::Puff(k),
                Role::Glow(g) if !eyes => Look::Glow(g),
                _ => look_of(sprite, &e.info, self.smoke),
            };
            // A creature's smoke or dust ([`crate::puffs::smoke_of`]) is thinned and swelled.
            let smoke = matches!(look, Look::Puff(k) if (crate::puffs::SMOKE..crate::puffs::HAZE).contains(&k));
            let stride = crate::puffs::stride(&e.info);
            let carried = match look {
                Look::Glow(_) if !snowy => sprite.glow.map(|g| (0, g.colour)),
                Look::Puff(crate::puffs::ACID) => Some((1, sprite.colour)),
                _ => None,
            };
            if let Some((k, colour)) = carried {
                let shows: f32 = e
                    .live()
                    .map(|p| (1.0 - p.translucency).clamp(0.0, 1.0) * p.scale)
                    .sum();
                let o = e.object_origin;
                let l = &mut lamps[k];
                l.0 += Vec3::new(o.x, o.y, o.z) * shows;
                l.1 += colour * shows;
                l.2 += shows;
            }
            for p in e.live() {
                if p.translucency >= 1.0 || p.scale <= 0.0 {
                    continue;
                }
                let seed = seed_of(p);
                if smoke && !seed.is_multiple_of(stride) {
                    continue;
                }
                let o = p.frame.origin;
                let (mut transform, age, drift) = shape(look, p, self.wind);
                let at = Vec3::new(o.x, o.y, o.z) + offset + drift;
                let distance = at.distance(eye);
                if distance / p.scale > sprite.reach * self.detail {
                    continue;
                }
                transform.translation += glam::Vec3A::from(offset);
                let mut alpha = (1.0 - p.translucency).clamp(0.0, 1.0) * glow;
                if smoke {
                    let grown = sprite.size * crate::puffs::stride_reach(stride);
                    transform.matrix3 *= grown;
                    transform.matrix3.y_axis.z = e.info.start_scale.max(e.info.final_scale) * grown;
                    alpha *= crate::puffs::smoke_swell(age);
                }
                if let Look::Puff(_) = look {
                    let b = driver.particle_parent_frame().origin;
                    anchor(&mut transform, Vec3::new(b.x, b.y, b.z) + offset);
                    transform.matrix3.x_axis.y = e.info.start_scale.max(e.info.final_scale);
                }
                // A whirl turns about its emitter: laid out from it, its axis in the ball's last
                // empty entries.
                if look == Look::Puff(crate::puffs::VORTEX) {
                    let (centre, axis) = whirl(driver, e);
                    anchor(&mut transform, centre + offset);
                    transform.matrix3.y_axis.y = axis.x;
                    transform.matrix3.z_axis.z = axis.y;
                    transform.matrix3.x_axis.z = axis.z;
                }
                self.external.push(Drawn {
                    sprite: id,
                    transform,
                    alpha,
                    cell,
                    distance,
                    light,
                    look,
                    age,
                    seed,
                    group: self.gathered,
                });
                // Snow whirls round each of a frost elemental's puffs, and ice glints in it.
                if look == Look::Puff(crate::puffs::SNOW) {
                    #[allow(clippy::cast_possible_truncation)] // seconds of a particle's life
                    let seconds = p.lifetime as f32;
                    let centre = Vec3::new(o.x, o.y, o.z) + offset;
                    let reach = p.scale * sprite.half / crate::puffs::STEAM_HALF * PUFF_REACH;
                    for f in crate::rime::flakes(seed, seconds, age, reach, distance) {
                        if f.alpha <= 1e-3 {
                            continue;
                        }
                        let at = centre + f.offset;
                        let scale = f.radius / sprite.half.max(1e-3);
                        self.external.push(Drawn {
                            sprite: id,
                            transform: Affine3A::from_scale_rotation_translation(
                                Vec3::splat(scale),
                                glam::Quat::IDENTITY,
                                at,
                            ),
                            alpha: f.alpha,
                            cell,
                            distance: at.distance(eye),
                            light: false,
                            look: Look::Flake(u8::from(f.glint)),
                            age,
                            seed,
                            group: self.gathered,
                        });
                    }
                }
            }
        }
        // Its strikes, each on its own clock, lasting after the particle that started it.
        let mut mine: Vec<(u32, f32, f32)> = self
            .strikes
            .iter()
            .filter(|(_, s)| s.body == body)
            .map(|(&seed, s)| {
                #[allow(clippy::cast_possible_truncation)] // a share of a strike's life
                let age = ((self.clock - s.born) / f64::from(s.life)) as f32;
                (seed, age, s.strength)
            })
            .filter(|&(_, age, _)| (0.0..1.0).contains(&age))
            .collect();
        mine.sort_by_key(|m| m.0);
        for (seed, age, strength) in mine {
            let flash = crate::bolts::flash(age, seed) * strength;
            let glow = crate::bolts::glow(age) * strength;
            self.struck
                .push((self.gathered, stands, seed, flash, cell, glow));
        }
        // A creature's light carries a magic light's lamp, its acid a wider green glow.
        for (k, (at, colour, weight)) in lamps.into_iter().enumerate() {
            if weight <= 0.0 {
                continue;
            }
            let source = if k == 0 {
                crate::glow::carried_source(colour, held)
            } else {
                crate::lamps::LampSource {
                    intensity: crate::puffs::ACID_GLOW_INTENSITY,
                    falloff: crate::puffs::ACID_GLOW_FALLOFF,
                    ..crate::glow::carried_source(colour, false)
                }
            };
            let at = Affine3A::from_translation(at / weight + offset);
            let mut lamp = crate::lamps::place(&source, &at, cell, u32::MAX);
            if held && k == 0 {
                lamp = crate::glow::held_lamp(lamp, source.colour);
            }
            self.glow_lamps.push(lamp);
        }
        if driver
            .particles
            .iter()
            .any(|e| blob_of(sprite_of(&e.info).0) == Some(1))
        {
            let feet = driver.env.position.frame.origin.z + offset.z;
            self.blood.floors.push((self.gathered, feet));
        }
        self.gathered += 1;
    }

    /// The particles seen from `eye` inside `planes` (their cells' rectangles not empty, as
    /// `seen` says), far to near, at most [`MAX_PARTICLES`]; and the batches drawing them.
    pub fn collect(&mut self, eye: Vec3, planes: &[Vec4], seen: impl Fn(u32) -> bool) {
        let started = std::time::Instant::now();
        self.drawn.clear();
        self.live = 0;
        for (host, h) in self.hosts.iter().enumerate() {
            if !seen(h.cell) {
                continue;
            }
            let gate = crate::gates::gate_of(&h.driver, &self.sprites, Vec3::ZERO);
            if let Some(g) = &gate {
                if let Some(script) = crate::gates::script_of(&h.driver) {
                    self.portal_colours.insert(script, g.colour);
                }
            }
            if let Some(g) = gate.filter(|_| self.portals) {
                let reach = reach_of(&self.sprites, &h.driver, self.detail);
                let shown = crate::gates::shown(g.centre.distance(eye), reach);
                let group = 0x8000_0000 | u32::try_from(host).unwrap_or(0);
                if shown > 0.0 {
                    self.glow_lamps.push(crate::gates::lamp(&g, h.cell, shown));
                }
                for d in crate::gates::drawn(&g, h.cell, eye, self.clock, self.air, shown, group) {
                    let at = Vec3::from(d.transform.translation);
                    let r = d.transform.matrix3.x_axis.length() * 2.0;
                    if planes.iter().all(|q| q.truncate().dot(at) + q.w >= -r) {
                        self.drawn.push(d);
                    }
                }
                continue;
            }
            if self.falls && h.fall {
                continue;
            }
            if self.beacons && h.beacon == Some(crate::beacons::HEART) {
                let foot = Vec3::new(h.frame.origin.x, h.frame.origin.y, h.frame.origin.z);
                let sprite = h
                    .driver
                    .particles
                    .iter()
                    .map(|e| sprite_of(&e.info).0)
                    .find(|id| matches!(self.sprites.get(id), Some(Some(_))));
                if let Some(sprite) = sprite {
                    let group = 0x8000_0000 | u32::try_from(host).unwrap_or(0);
                    self.glow_lamps.push(crate::beacons::lamp(foot, h.cell));
                    self.drawn.extend(crate::beacons::drawn(
                        foot, sprite, h.cell, eye, planes, group,
                    ));
                }
            }
            // A geyser's jet: its spray round its foot is drawn in its volume.
            let jet = self.smoke
                && h.driver.particles.iter().any(|e| {
                    crate::puffs::is_jet(&e.info)
                        && matches!(self.sprites.get(&sprite_of(&e.info).0), Some(Some(s))
                            if crate::puffs::kind_of(s, &e.info) == Some(crate::puffs::STEAM))
                });
            for e in h.driver.particles.iter() {
                let id = sprite_of(&e.info).0;
                let Some(Some(sprite)) = self.sprites.get(&id) else {
                    continue;
                };
                if is_snowfall(&e.info)
                    || self.fountains && crate::fountains::drawn_for(&h.driver, &e.info)
                {
                    continue;
                }
                let eyes = h.shadow && is_eye(e, sprite.additive());
                let light = sprite.additive() && is_light_sprite(&e.info) && !eyes;
                let glow = if eyes { EYE_GLOW } else { 1.0 };
                if self.skip_light_sprites && light {
                    continue;
                }
                // A hearth's fire is drawn as a body of fire ([`crate::flames::Flames::hearths`]).
                if self.skip_flamed
                    && h.hearth
                    && sprite.fire.is_some()
                    && self.puffed.contains(&id)
                {
                    continue;
                }
                let fire = self.skip_flamed
                    && h.puffed
                    && !light
                    && sprite.fire.is_some()
                    && self.puffed.contains(&id);
                if self.skip_flamed
                    && (h.flamed || h.puffed)
                    && !fire
                    && (light || self.flamed.contains(&id))
                {
                    continue;
                }
                let look = if fire {
                    Look::Fire
                } else if h.beacon.is_some() && self.beacons {
                    Look::Beacon(crate::beacons::WISP)
                } else if h.beacon.is_some() {
                    Look::Picture
                } else {
                    blob_of(id)
                        .filter(|_| self.smoke)
                        .map_or_else(|| look_of(sprite, &e.info, self.smoke), Look::Blob)
                };
                let spray = jet
                    && look == Look::Picture
                    && crate::puffs::is_spray(sprite.tone, sprite.additive(), &e.info);
                let look = if spray {
                    Look::Puff(crate::puffs::STEAM)
                } else {
                    look
                };
                let jet_puff = crate::puffs::is_jet(&e.info);
                for p in e.live() {
                    self.live += 1;
                    if p.translucency >= 1.0 || p.scale <= 0.0 {
                        continue;
                    }
                    let o = p.frame.origin;
                    let (mut transform, age, drift) = shape(look, p, self.wind);
                    // A haze, and a big vent's gas, keep one ball in a few, each grown to hold their
                    // volume.
                    let stride = match look {
                        Look::Puff(crate::puffs::HAZE) => crate::puffs::HAZE_STRIDE,
                        Look::Puff(crate::puffs::GAS) => crate::puffs::gas_stride(&e.info),
                        _ => 1,
                    };
                    if stride > 1 {
                        if !seed_of(p).is_multiple_of(stride) {
                            continue;
                        }
                        let k = crate::puffs::stride_reach(stride);
                        transform.matrix3.x_axis.x *= k;
                        transform.matrix3.y_axis.y *= k;
                        transform.matrix3.z_axis.z *= k;
                    }
                    if let Look::Puff(_) = look {
                        let b = h.driver.particle_parent_frame().origin;
                        anchor(&mut transform, Vec3::new(b.x, b.y, b.z));
                        transform.matrix3.x_axis.y = e.info.start_scale.max(e.info.final_scale);
                    }
                    let at = Vec3::new(o.x, o.y, o.z) + drift;
                    let distance = at.distance(eye);
                    if distance / p.scale > sprite.reach * self.detail {
                        continue;
                    }
                    let r = transform.matrix3.x_axis.length() * 2.0;
                    if planes.iter().any(|q| q.truncate().dot(at) + q.w < -r) {
                        continue;
                    }
                    let mut alpha = (1.0 - p.translucency).clamp(0.0, 1.0) * glow;
                    // A placed vent's or jet's steam is as thick as retail draws its puffs.
                    if look == Look::Puff(crate::puffs::STEAM) {
                        alpha = crate::puffs::placed_alpha(&e.info, alpha, age, spray);
                        // A jet's and its spray's balls are wider, and how thick each is beside
                        // its strength rides in its last empty entry.
                        if jet_puff || spray {
                            transform.matrix3.x_axis.x *= crate::puffs::JET_REACH;
                        }
                        if jet_puff {
                            transform.matrix3.y_axis.z = crate::puffs::jet_share(age);
                        } else if spray {
                            transform.matrix3.y_axis.z = crate::puffs::SPRAY_SHARE;
                        }
                    }
                    // Heavy gas pools as it ages: its mark rides in the same entry.
                    if look == Look::Puff(crate::puffs::GAS) && crate::puffs::gas_pools(&e.info) {
                        transform.matrix3.y_axis.z = 1.0;
                    }
                    self.drawn.push(Drawn {
                        sprite: id,
                        transform,
                        alpha,
                        cell: h.cell,
                        distance,
                        light,
                        look,
                        age,
                        seed: seed_of(p),
                        group: 0x8000_0000 | u32::try_from(host).unwrap_or(0),
                    });
                }
            }
        }
        self.drip(eye);
        for d in std::mem::take(&mut self.external) {
            self.live += 1;
            let at = Vec3::from(d.transform.translation);
            let r = d.transform.matrix3.x_axis.length() * 2.0;
            if !seen(d.cell) || planes.iter().any(|q| q.truncate().dot(at) + q.w < -r) {
                continue;
            }
            self.drawn.push(d);
        }
        self.drawn.sort_by(|a, b| {
            b.distance
                .total_cmp(&a.distance)
                .then(a.sprite.cmp(&b.sprite))
                .then(laid(a.look).cmp(&laid(b.look)))
        });
        self.drawn.truncate(MAX_PARTICLES);
        self.gathered = 0;
        // Liquid and vapour are drawn as balls with the ice, each body's run together: last in the
        // list, the vapour's bodies far to near.
        let (balls, rest): (Vec<Drawn>, Vec<Drawn>) = self
            .drawn
            .drain(..)
            .partition(|d| matches!(d.look, Look::Blob(_) | Look::Puff(_)));
        self.drawn = rest;
        let (mut puffs, mut blobs): (Vec<Drawn>, Vec<Drawn>) = balls
            .into_iter()
            .partition(|d| matches!(d.look, Look::Puff(_)));
        blobs.sort_by_key(|d| d.group);
        // ORDER-OK: looked up by group, never iterated.
        let mut far: HashMap<u32, f32> = HashMap::new();
        for d in &puffs {
            let f = far.entry(d.group).or_insert(0.0);
            *f = f.max(d.distance);
        }
        let kind = |d: &Drawn| match d.look {
            Look::Puff(k) => k,
            _ => 0,
        };
        // Each body's biggest balls first: those a full gather keeps.
        puffs.sort_by(|a, b| {
            far[&b.group]
                .total_cmp(&far[&a.group])
                .then(a.group.cmp(&b.group))
                .then(kind(a).cmp(&kind(b)))
                .then(
                    b.transform
                        .matrix3
                        .x_axis
                        .length()
                        .total_cmp(&a.transform.matrix3.x_axis.length()),
                )
        });
        // Steam's, haze's and gas's seed is its picture's material (its colour); its ball is as wide as
        // its picture.
        for d in &mut puffs {
            if matches!(d.look, Look::Puff(k) if k == crate::puffs::STEAM || k == crate::puffs::FROST || k == crate::puffs::SNOW || k == crate::puffs::HAZE || k == crate::puffs::GAS)
            {
                if let Some(Some(s)) = self.sprites.get(&d.sprite) {
                    d.seed = s.draws.first().map_or(0, |m| m.material & 0x7_FFFF);
                    // Its scale and its emitter's largest puff, not the body's place.
                    let k = s.half / crate::puffs::STEAM_HALF;
                    let m = &mut d.transform.matrix3;
                    m.x_axis.x *= k;
                    m.y_axis.y *= k;
                    m.z_axis.z *= k;
                    m.x_axis.y *= k;
                    // Gas is born small and dense, retail's lumps: it swells by its size alone.
                    // A placed static's steam swelled as it was gathered.
                    if d.look != Look::Puff(crate::puffs::GAS)
                        && (d.group & 0x8000_0000 == 0 || d.look != Look::Puff(crate::puffs::STEAM))
                    {
                        d.alpha *= crate::puffs::swell(d.age);
                    }
                }
            }
        }
        let liquid = self.drawn.len();
        let vapour = liquid + blobs.len();
        self.drawn.extend(blobs);
        self.drawn.extend(puffs);
        // Runs of one picture, a batch per draw of it, each with its own copy of the run's
        // particles (most pictures are one quad of one surface). Mist is laid over, never added.
        self.batches.clear();
        self.instances.clear();
        let mut k = 0;
        while k < liquid {
            let id = self.drawn[k].sprite;
            let mist = laid(self.drawn[k].look);
            // Dust draws no picture: a run of motes is one draw, whatever their pictures.
            let dust = self.drawn[k].look == Look::Mote;
            let mut end = k + 1;
            while end < liquid
                && (self.drawn[end].sprite == id || dust && self.drawn[end].look == Look::Mote)
                && laid(self.drawn[end].look) == mist
            {
                end += 1;
            }
            if let Some(Some(s)) = self.sprites.get(&id) {
                for d in s.draws.iter().take(if dust { 1 } else { usize::MAX }) {
                    if self.instances.len() + (end - k) > MAX_PARTICLES {
                        break;
                    }
                    let word = (d.material & 0xFF_FFFF) | (s.mode << crate::scene::BILLBOARD_SHIFT);
                    let mut draw = *d;
                    if mist {
                        draw.pipeline = PipelineKind::Translucent;
                    }
                    self.batches.push(Batch {
                        draw,
                        first: u32::try_from(self.instances.len()).unwrap_or(0),
                        count: u32::try_from(end - k).unwrap_or(0),
                        distance: self.drawn[k].distance,
                    });
                    self.instances.extend((k..end).map(|i| (i, word)));
                }
            }
            k = end;
        }
        self.blobs = self.runs(liquid, vapour).0;
        self.records.clear();
        self.run_words.clear();
        let (puffs, mut runs) = self.runs(vapour, self.drawn.len());
        runs.sort_by(|a, b| b.2.total_cmp(&a.2));
        self.puffs = puffs;
        self.puff_runs = runs;
        self.strike_segments();
        self.cpu_ms += started.elapsed().as_secs_f64() * 1e3;
    }

    /// The balls `from..to` of `drawn` as instances, each body's run together: their material
    /// word the run's first instance, its length and its kind. Where they are in the list, and
    /// each run (first, how many, its balls' mean distance).
    fn runs(&mut self, from: usize, to: usize) -> ((u32, u32), Vec<PuffRun>) {
        let first = u32::try_from(self.instances.len()).unwrap_or(0);
        let mut runs = Vec::new();
        let mut lit = std::collections::HashSet::new();
        let mut k = from;
        while k < to && self.instances.len() < MAX_PARTICLES {
            let (group, look) = (self.drawn[k].group, self.drawn[k].look);
            let mut end = k + 1;
            while end < to
                && self.drawn[end].group == group
                && self.drawn[end].look == look
                && end - k < BLOB_RUN
            {
                end += 1;
            }
            let start = u32::try_from(self.instances.len()).unwrap_or(0);
            // A volume's strikes trail its first run, as lines of light (flag 1).
            let lines: Vec<usize> = if matches!(look, Look::Puff(_)) && lit.insert(group) {
                self.strike_lines(group)
            } else {
                Vec::new()
            };
            let n = (end - k + lines.len()).min(BLOB_RUN);
            let count = u32::try_from(n).unwrap_or(0);
            let kind = match self.drawn[k].look {
                Look::Blob(b) | Look::Puff(b) => u32::from(b),
                _ => 0,
            };
            let word = start | count << 15 | kind << 23;
            self.instances
                .extend((k..end).chain(lines).take(n).map(|i| (i, word)));
            self.run_words.insert(group, word);
            #[allow(clippy::cast_precision_loss)]
            let distance =
                self.drawn[k..end].iter().map(|d| d.distance).sum::<f32>() / (end - k) as f32;
            runs.push((start, count, distance));
            k = end;
        }
        self.instances.truncate(MAX_PARTICLES);
        let last = u32::try_from(self.instances.len()).unwrap_or(0);
        runs.retain_mut(|r| {
            r.1 = r.1.min(last.saturating_sub(r.0));
            r.1 > 0
        });
        ((first, last - first), runs)
    }

    /// Body `group`'s strikes this frame as lines of light: records (their chords, their colour by
    /// their brightness), by their place past the particles.
    fn strike_lines(&mut self, group: u32) -> Vec<usize> {
        let mut out = Vec::new();
        for &(g, anchor, seed, flash, cell, _) in &self.struck {
            if g != group || flash <= 1e-3 {
                continue;
            }
            let Some(s) = self.strikes.get(&seed) else {
                continue;
            };
            let (a, b) = (anchor + s.chord.0, anchor + s.chord.1);
            let c = s.colour * flash;
            out.push(self.drawn.len() + self.records.len());
            self.records.push((
                [a.x, a.y, a.z, 0.0, b.x, b.y, b.z, 0.0, c.x, c.y, c.z, 0.0],
                flash,
                cell,
                1,
            ));
        }
        out
    }

    /// This frame's strikes' segments, after everything else in the list (each with its body's
    /// volume run, which dims it), and their lamps.
    fn strike_segments(&mut self) {
        let first = u32::try_from(self.instances.len()).unwrap_or(0);
        self.flashes.clear();
        // Each body's strikes light it as one lamp, no brighter than one strike's full flash.
        let mut lit: Vec<(u32, Vec3, Vec3, f32, u32)> = Vec::new();
        for &(group, anchor, seed, flash, cell, glow) in &self.struck {
            let Some(s) = self.strikes.get(&seed) else {
                continue;
            };
            if flash <= 1e-3 {
                continue;
            }
            let word = self.run_words.get(&group).copied().unwrap_or(u32::MAX);
            for piece in &s.pieces {
                if self.instances.len() >= MAX_PARTICLES {
                    break;
                }
                let (a, b) = (anchor + piece.a, anchor + piece.b);
                let c = s.colour;
                self.instances
                    .push((self.drawn.len() + self.records.len(), word));
                self.records.push((
                    [
                        a.x,
                        a.y,
                        a.z,
                        piece.width,
                        b.x,
                        b.y,
                        b.z,
                        piece.share,
                        c.x,
                        c.y,
                        c.z,
                        flash,
                    ],
                    flash,
                    cell,
                    1,
                ));
            }
            let mid = anchor + (s.chord.0 + s.chord.1) * 0.5;
            match lit.iter_mut().find(|l| l.0 == group) {
                Some(l) => {
                    l.1 += mid * glow;
                    l.2 += s.colour * glow;
                    l.3 += glow;
                }
                None => lit.push((group, mid * glow, s.colour * glow, glow, cell)),
            }
        }
        for (_, at, colour, flash, cell) in lit.into_iter().filter(|l| l.3 > 1e-3) {
            self.flashes.push(crate::bolts::lamp(
                at / flash,
                colour / flash,
                flash.min(crate::bolts::glow(0.2)),
                cell,
            ));
        }
        self.bolts = (
            first,
            u32::try_from(self.instances.len()).unwrap_or(0) - first,
        );
        self.struck.clear();
        let now = self.clock;
        self.strikes.retain(|_, s| now - s.born < f64::from(s.life));
    }

    /// Steps the blood dripping from the gathered blood balls and adds the drops to them: each
    /// ball now and then swells a drop under it, which lets go and falls to its body's feet.
    fn drip(&mut self, eye: Vec3) {
        let (now, last) = (self.blood.now, self.blood.dripped);
        self.blood.dripped = now;
        #[allow(clippy::cast_possible_truncation)] // a frame's seconds
        let dt = (now - last).clamp(0.0, 0.1) as f32;
        let balls: Vec<(u32, Vec3, f32, u32, u32)> = self
            .external
            .iter()
            .filter(|d| d.look == Look::Blob(1))
            .map(|d| {
                let r = d.transform.matrix3.x_axis.length() * BLOOD_SIZE * d.alpha.sqrt();
                (
                    d.seed,
                    Vec3::from(d.transform.translation),
                    r,
                    d.group,
                    d.cell,
                )
            })
            .collect();
        let floors = std::mem::take(&mut self.blood.floors);
        if last > 0.0 && now > last {
            for &(seed, at, _, group, cell) in &balls {
                let phase = f64::from(unit(seed)) * DRIP_BEAT;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // beats
                let (k0, k1) = (
                    ((last + phase) / DRIP_BEAT) as u32,
                    ((now + phase) / DRIP_BEAT) as u32,
                );
                let h = seed ^ k1.wrapping_mul(0x2C1B_3C6D);
                if k1 == k0 || h % DRIP_ODDS != 0 || self.blood.drips.len() >= DRIP_MOST {
                    continue;
                }
                let floor = floors
                    .iter()
                    .find(|f| f.0 == group)
                    .map_or(at.z - 3.0, |f| f.1);
                let a = unit(h ^ 0x51) * std::f32::consts::TAU;
                let spread = 0.45 * unit(h ^ 0xA7).sqrt();
                self.blood.drips.push(Drip {
                    ball: seed,
                    under: [cosf(a) * spread, sinf(a) * spread],
                    group,
                    cell,
                    at,
                    fall: 0.0,
                    radius: DRIP_RADIUS[0] + (DRIP_RADIUS[1] - DRIP_RADIUS[0]) * unit(h ^ 0x3D),
                    born: now,
                    floor,
                    hung: true,
                });
            }
        }
        let mut out = Vec::new();
        self.blood.drips.retain_mut(|d| {
            #[allow(clippy::cast_possible_truncation)] // seconds of a drop's life
            let age = (now - d.born) as f32;
            let mut r = d.radius;
            if d.hung {
                match balls.iter().find(|b| b.0 == d.ball) {
                    Some(&(_, at, br, group, cell)) if age < DRIP_HANG => {
                        let t = age / DRIP_HANG;
                        let grow = t * t * (3.0 - 2.0 * t);
                        r *= grow.max(0.05);
                        // Hangs from the ball's underside, sagging as it fills.
                        let down = (1.0 - hypotf(d.under[0], d.under[1])).max(0.0).sqrt();
                        d.at = at
                            + Vec3::new(
                                d.under[0] * br,
                                d.under[1] * br,
                                -down * br - r * 0.9 * grow,
                            );
                        d.group = group;
                        d.cell = cell;
                    }
                    _ => d.hung = false,
                }
            } else {
                d.fall += GRAVITY * dt;
                d.at.z -= d.fall * dt;
            }
            if d.at.z - r <= d.floor {
                return false;
            }
            out.push(Drawn {
                sprite: BLOB_SPRITES[1].0,
                transform: Affine3A::from_scale_rotation_translation(
                    Vec3::splat(r / BLOOD_SIZE),
                    glam::Quat::IDENTITY,
                    d.at,
                ),
                alpha: 1.0,
                cell: d.cell,
                distance: d.at.distance(eye),
                light: false,
                look: Look::Blob(1),
                age: 0.0,
                seed: d.ball,
                group: d.group,
            });
            true
        });
        self.external.extend(out);
    }

    /// This frame's particles as the GPU reads them.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.instances.len() * 64);
        for &(i, word) in &self.instances {
            // A strike's record: its rows as they stand.
            if let Some((rows, alpha, cell, flags)) = i
                .checked_sub(self.drawn.len())
                .and_then(|r| self.records.get(r))
            {
                for v in rows {
                    out.extend_from_slice(&v.to_le_bytes());
                }
                for w in [word, alpha.to_bits(), *cell, *flags] {
                    out.extend_from_slice(&w.to_le_bytes());
                }
                continue;
            }
            let p = &self.drawn[i];
            let m = p.transform.matrix3;
            let t = p.transform.translation;
            for row in [
                [m.x_axis.x, m.y_axis.x, m.z_axis.x, t.x],
                [m.x_axis.y, m.y_axis.y, m.z_axis.y, t.y],
                [m.x_axis.z, m.y_axis.z, m.z_axis.z, t.z],
            ] {
                for v in row {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 to 1023
            let age = (p.age.clamp(0.0, 1.0) * 1023.0) as u32;
            // A light sprite's seed is whether its picture is warm (firelight) or a magic lamp's.
            let seed = if p.light {
                u32::from(
                    matches!(self.sprites.get(&p.sprite), Some(Some(s)) if crate::lamps::warm(s.colour)),
                )
            } else {
                p.seed
            };
            // The seed's 17 bits, then whether it is a glow and its shape; fire sets bits 0 and 1.
            let mut flags = u32::from(p.light || p.look == Look::Fire)
                | u32::from(p.look == Look::Smoke || p.look == Look::Fire) << 1
                | age << 2
                | u32::from(p.look == Look::Mist) << 12
                | (seed & 0x1_FFFF) << 13
                | match p.look {
                    Look::Glow(kind) | Look::Flake(kind) => 1 << 30 | u32::from(kind) << 31,
                    _ => 0,
                };
            // A beacon's part rides in its seed's low two bits, under bit 31 alone.
            if let Look::Beacon(part) = p.look {
                flags = 1 << 31 | age << 2 | ((p.seed << 2 | u32::from(part)) & 0x1_FFFF) << 13;
            }
            // Smoke and dust carry their picture's colour in place of their seed.
            if let (Look::Puff(k), Some(Some(s))) = (p.look, self.sprites.get(&p.sprite)) {
                if (crate::puffs::SMOKE..crate::puffs::HAZE).contains(&k) {
                    flags = age << 2 | crate::puffs::tint_bits(s.colour) << 16;
                }
            }
            // A portal's colour rides in its seed; a mote is mist and smoke at once, its look's
            // word its alpha ([`crate::gates`]).
            match p.look {
                Look::Puff(crate::puffs::GATE) => flags = p.seed << 16,
                Look::Mote => flags = 2 | 4096,
                _ => {}
            }
            // A glow's alpha word carries its picture's colour and strength ([`crate::glow::Glow::word`]).
            let alpha = match (p.look, self.sprites.get(&p.sprite)) {
                (Look::Mote, _) => p.seed,
                (Look::Glow(_), Some(Some(Sprite { glow: Some(g), .. }))) => g.word(p.alpha),
                (Look::Flake(0), _) => crate::rime::FLAKE_LIGHT.word(p.alpha),
                (Look::Flake(_), _) => crate::rime::GLINT_LIGHT.word(p.alpha),
                (Look::Fire, Some(Some(s))) => fire_word(s.fire.unwrap_or(Vec3::ONE)),
                _ => p.alpha.to_bits(),
            };
            for w in [word, alpha, p.cell, flags] {
                out.extend_from_slice(&w.to_le_bytes());
            }
        }
        out
    }
}

/// How a particle draw blends: added to what is behind it (fire, glows, magic) or laid over it.
#[must_use]
pub fn additive(d: &Draw) -> bool {
    d.pipeline == PipelineKind::Additive
}

/// Makes the picture `gfx` ready to draw, once: its mesh, its turn and its reach.
pub fn prepare_sprite(
    engine: &mut crate::Engine,
    loader: &mut crate::world::AssetLoader,
    store: &RetailDatStore,
    gfx: DataId,
) {
    if engine.particles.sprites.contains_key(&gfx.0) {
        return;
    }
    let sprite = loader.mesh(engine, store, gfx, false).and_then(|mesh| {
        let m = engine.scene.mesh(mesh)?;
        let draws = m.draws.clone();
        let reach = crate::asset::object::degrade_chain(store, gfx)
            .iter()
            .find(|l| l.mesh.is_none())
            .map_or(DRAW_DISTANCE, |l| l.switch);
        let (size, colour, slender) = crate::puffs::picture(store, gfx);
        Some(Sprite {
            mesh,
            draws,
            mode: crate::asset::object::near_mode(store, gfx),
            reach,
            tone: crate::lamps::tone(store, gfx),
            size,
            colour,
            slender,
            half: crate::puffs::picture_half(store, gfx),
            fire: crate::body_fire::fire_picture(store, gfx),
            glow: crate::glow::glow_of(store, gfx),
        })
    });
    engine.particles.sprites.insert(gfx.0, sprite);
}

/// The widest reach of `driver`'s emitters, at least [`DRAW_DISTANCE`]: each its picture's reach
/// at its particles' largest scale, so an emitter keeps on while any particle of it is drawn, and
/// at `detail` ([`Particles::detail`]).
fn reach_of(sprites: &HashMap<u32, Option<Sprite>>, driver: &MotionDriver, detail: f32) -> f32 {
    driver
        .particles
        .iter()
        .filter_map(|e| {
            let s = sprites
                .get(&sprite_of(&e.info).0)
                .and_then(Option::as_ref)?;
            Some(s.reach * e.info.start_scale.max(e.info.final_scale).max(1.0))
        })
        .fold(DRAW_DISTANCE, f32::max)
        * detail
}
