//! The world's lamps, from the dats' light data and the art pack's `[[light]]` records.
//!
//! An object id is looked at once ([`examine`]):
//! - **authored lights**: a setup's own point lights (colour, intensity, falloff), what the
//!   client lights rooms and dungeons with. Many outdoor setups leave theirs at a placeholder two
//!   metres out on both level axes; such a light is moved to where the object's flame is started,
//!   or to the top of the object (render-hifi's repair).
//! - **flames**: where a setup has no authored light, its default script's particle emitters
//!   whose sprite is additive and fire-coloured (torches, braziers) give one each, a little above
//!   where their particles are born.
//! - **the pack's lights**: a `[[light]]` for the object, where it is placed (outdoors, indoors),
//!   stands in for retail's own, offset from where retail's light stood, in lumens and kelvin.
//! - **glows** ([`glows`]): a creature with none of these whose body glows (a wisp) gives one
//!   soft light of its glowing sprites' colour, under the physical calibration only.
//!
//! A placed object's lamps are kept in render space with the cell slot it stands in. Each frame
//! the lamps that can reach the view are chosen ([`frame_lamps`]), nearest first, for the light
//! clusters. Retail's lights become physical ones of the same reach: an inverse-square fall-off
//! windowed to the client's range (its falloff and a half), as bright a metre out as the client's
//! light is there, a room's light being [`crate::renderer::ROOM_LUX`] per unit.

use std::collections::HashMap;

use glam::{Affine3A, Vec3, Vec4};
use pyreal_assets::hook::HookData;
use pyreal_assets::motion::PhysicsScript;
use pyreal_assets::world::ParticleEmitterInfo;
use pyreal_assets::{Decode, GfxObj, Setup, Surface};
use pyreal_dat::{divine_type, DbType, RetailDatStore};
use pyreal_primitives::DataId;

use crate::asset::texture::TextureSource;

/// How a lamp moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Flicker {
    #[default]
    Steady,
    Flame,
    Embers,
    /// A creature of fire's: quicker and deeper than a torch's, a candle's flutter.
    Lively,
}

impl Flicker {
    fn word(self) -> u32 {
        match self {
            Self::Steady => 0,
            Self::Flame => 1,
            Self::Embers => 2,
            Self::Lively => 3,
        }
    }
}

/// Where a lamp's light was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LampKind {
    Authored,
    Flame,
    Pack,
    Glow,
    /// A portal's light ([`crate::gates::lamp`]): magic, washing the ground round it.
    Portal,
    /// A room's own authored light of a colour no lamp burns (a dungeon's green, blue or
    /// magenta), lit at retail's colour and intensity ([`fixture`]).
    Mood,
}

/// One light of an object, in the object's own space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LampSource {
    /// Where the client lights from, when it is not `offset`: the dats' own place of a light
    /// that is drawn at its flame.
    pub client_offset: Option<Vec3>,
    pub kind: LampKind,
    pub offset: Vec3,
    /// Linear colour, brightest channel 1.
    pub colour: Vec3,
    /// The client's intensity and falloff (metres); a pack light's lumens and reach.
    pub intensity: f32,
    pub falloff: f32,
    pub flicker: Flicker,
    /// Whether it casts shadows (the pack says; retail's lights all may).
    pub shadows: bool,
}

/// A placed lamp, render space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lamp {
    pub position: Vec3,
    pub colour: Vec3,
    /// The luminous intensity, scene units at a metre (lux / [`crate::sky::SCENE_LUX`]).
    pub candela: f32,
    /// Where its light ends, metres.
    pub range: f32,
    /// The client's light: intensity times colour, and its range.
    pub retail: Vec4,
    /// The client's intensity, which `retail`'s colour is multiplied by.
    pub intensity: f32,
    /// Where the client lights from, when it is not `position` (render space).
    pub client: Option<Vec3>,
    pub flicker: Flicker,
    pub shadows: bool,
    /// The cell slot it stands in; zero outdoors.
    pub cell: u32,
    pub kind: LampKind,
    /// The object it belongs to, whose own surfaces never shadow it.
    pub object: u32,
    /// A strike of lightning's ([`crate::bolts::lamp`]): its own colour, bright for a moment.
    pub flash: bool,
    /// Its flicker's seed; nought takes one from where it stands (a lamp that moves keeps one).
    pub seed: u32,
    /// A held thing's light ([`crate::glow::HELD_INTENSITY`]), as soft outdoors too.
    pub held: bool,
}

impl Lamp {
    /// Whether the client lights with it as a moving object's light, not burned into its room.
    #[must_use]
    pub fn moves(&self) -> bool {
        matches!(self.kind, LampKind::Glow | LampKind::Portal) || self.held || self.flash
    }
}

/// The client's light that follows the player in a room: white, intensity 2.25, falloff 10.
#[must_use]
pub fn viewer_lamp(at: Vec3, cell: u32) -> Lamp {
    const INTENSITY: f32 = 2.25;
    let range = 10.0 * RANGE_OF_FALLOFF;
    Lamp {
        position: at,
        colour: Vec3::ONE,
        candela: 0.0,
        range,
        retail: Vec3::splat(INTENSITY).extend(range),
        intensity: INTENSITY,
        client: None,
        flicker: Flicker::Steady,
        shadows: false,
        cell,
        kind: LampKind::Glow,
        object: u32::MAX,
        flash: false,
        seed: 1,
        held: false,
    }
}

/// The client's range for a light of `falloff`: Direct3D's `Range = falloff * 1.5`.
pub const RANGE_OF_FALLOFF: f32 = 1.5;
/// How far a retail lamp's colour is paled toward its grey under the physical light.
pub const RETAIL_PALING: f32 = 0.35;
/// How far a magic light's colour (a portal's, a spell's) is paled, as retail's clamp whitens it.
pub const MAGIC_PALING: f32 = 0.6;
/// The most a magic light's grey is of its brightest channel, a portal's: a green pales no more.
pub const MAGIC_GREY: f32 = 0.3;
/// The most intensity a magic light lights with: a pool round it, not a room's wash.
pub const MAGIC_INTENSITY: f32 = 4.0;
/// A glow's intensity: half a magic light's most.
pub const GLOW_INTENSITY: f32 = 2.0;
/// A glow's falloff, metres: a small pool.
pub const GLOW_FALLOFF: f32 = 4.0;
/// The distance at which a retail light is as bright as the client draws it, metres.
pub const MATCH_DISTANCE: f32 = 1.0;
/// The most lamps a frame lights with.
pub const MAX_FRAME_LAMPS: usize = 1024;
/// How far from the eye a lamp is kept, metres.
pub const LAMP_DISTANCE: f32 = 250.0;
/// A lamp's bytes on the GPU.
pub const LAMP_SIZE: u64 = 64;

/// The most room boxes a frame's lamps are held to, each one lamp record after the lamps.
pub const MAX_ROOM_BOXES: usize = 1024;

/// The most rooms one lamp lights before it is left to light everywhere.
pub const ROOMS_PER_LAMP: usize = 24;

/// A room box's bytes, laid as a lamp record: its least corner, then its greatest.
#[must_use]
pub fn room_box_bytes(lo: Vec3, hi: Vec3) -> [u8; 64] {
    let mut b = [0u8; 64];
    for (k, v) in [lo.x, lo.y, lo.z, 0.0, hi.x, hi.y, hi.z].iter().enumerate() {
        b[k * 4..k * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    b
}

pub(crate) fn decode<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

fn srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        pyreal_primitives::num::math::powf((c + 0.055) / 1.055, 2.4)
    }
}

/// `c` with its brightest channel at one.
fn normalised(c: Vec3) -> Vec3 {
    c / c.max_element().max(1e-4)
}

/// `0xAARRGGBB` as linear colour.
fn argb_linear(argb: u32) -> Vec3 {
    let [_, r, g, b] = argb.to_be_bytes();
    let f = crate::asset::material::srgb_to_linear;
    Vec3::new(f(r), f(g), f(b))
}

/// Hue (degrees) and saturation.
fn hue_saturation(c: Vec3) -> (f32, f32) {
    let max = c.max_element();
    let min = c.min_element();
    let d = max - min;
    let s = if max > 0.0 { d / max } else { 0.0 };
    let h = if d <= 1e-6 {
        0.0
    } else if (max - c.x).abs() < 1e-6 {
        60.0 * ((c.y - c.z) / d).rem_euclid(6.0)
    } else if (max - c.y).abs() < 1e-6 {
        60.0 * ((c.z - c.x) / d + 2.0)
    } else {
        60.0 * ((c.x - c.y) / d + 4.0)
    };
    (h, s)
}

/// Whether a light's colour reads as lamp or firelight: orange to yellow, or a warm white
/// (render-hifi's test); a portal's or a spell's does not.
#[must_use]
pub fn warm(c: Vec3) -> bool {
    let (h, s) = hue_saturation(c);
    c.max_element() > 0.0 && ((s < 0.18 && c.x >= c.z) || (10.0..=62.0).contains(&h))
}

/// Whether a lamp is the dats' own light of a colour no lamp burns.
fn magic(kind: LampKind, colour: Vec3) -> bool {
    matches!(kind, LampKind::Glow | LampKind::Portal)
        || (kind == LampKind::Authored && !warm(colour))
}

/// `s` as a dungeon's placed object lights with: indoors, an authored light that is not warm is a
/// mood light, not a portal's or spell's.
#[must_use]
pub fn fixture(mut s: LampSource, indoors: bool) -> LampSource {
    if indoors && s.kind == LampKind::Authored && !warm(s.colour) {
        s.kind = LampKind::Mood;
    }
    s
}

/// A colour temperature's linear colour, brightest channel one (Helland's fit of the Planckian
/// locus).
#[must_use]
pub fn kelvin(k: f32) -> Vec3 {
    use pyreal_primitives::num::math::{logf, powf};
    let t = (k / 100.0).clamp(10.0, 400.0);
    let r = if t <= 66.0 {
        255.0
    } else {
        329.698_73 * powf(t - 60.0, -0.133_204_76)
    };
    let g = if t <= 66.0 {
        99.470_8 * logf(t) - 161.119_57
    } else {
        288.122_16 * powf(t - 60.0, -0.075_514_85)
    };
    let b = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_73 * logf(t - 10.0) - 305.044_8
    };
    normalised(Vec3::new(srgb(r / 255.0), srgb(g / 255.0), srgb(b / 255.0)))
}

/// A setup's parts with their resting frames and scales.
pub(crate) fn parts(setup: &Setup) -> Vec<(DataId, Affine3A)> {
    let frames = setup
        .placement_frames
        .get(&0x65)
        .or_else(|| setup.placement_frames.get(&0))
        .map(|p| p.frames.clone())
        .unwrap_or_default();
    setup
        .parts
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let f = frames.get(i).copied().unwrap_or_default();
            let s = setup
                .default_scale
                .as_ref()
                .and_then(|d| d.get(i).copied())
                .map_or(Vec3::ONE, |v| Vec3::new(v.x, v.y, v.z));
            (
                *id,
                crate::asset::object::frame_affine(&f) * Affine3A::from_scale(s),
            )
        })
        .collect()
}

/// The lowest and highest corners of an object's parts.
pub(crate) fn bounds(store: &RetailDatStore, parts: &[(DataId, Affine3A)]) -> Option<(Vec3, Vec3)> {
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for (gid, at) in parts {
        let gfx = decode::<GfxObj>(store, DbType::GfxObj, *gid)?;
        for v in &gfx.vertex_array.vertices {
            let p = at.transform_point3(Vec3::new(v.position.x, v.position.y, v.position.z));
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    (hi.z >= lo.z).then_some((lo, hi))
}

/// The emitters a setup's default script starts: the emitter and where, object space.
pub(crate) fn script_emitters(
    store: &RetailDatStore,
    setup: &Setup,
    parts: &[(DataId, Affine3A)],
) -> Vec<(ParticleEmitterInfo, Vec3)> {
    let id = setup.default_script_id;
    if id.0 == 0 {
        return Vec::new();
    }
    let Some(script) = decode::<PhysicsScript>(store, DbType::PhysicsScript, id) else {
        return Vec::new();
    };
    script
        .script_data
        .iter()
        .filter_map(|step| match &step.hook.data {
            HookData::CreateParticle(c) => {
                let e: ParticleEmitterInfo =
                    decode(store, DbType::ParticleEmitter, c.emitter_info_id)?;
                let o = c.offset.origin;
                let local = Vec3::new(o.x, o.y, o.z);
                let at = parts
                    .get(c.part_index as usize)
                    .map_or(local, |(_, f)| f.transform_point3(local));
                Some((e, at))
            }
            _ => None,
        })
        .collect()
}

/// Fire-coloured sprites that are not fire: the desert's drifting sand, and a floor grate's fume
/// ([`crate::puffs::FUME`], its picture and its software picture).
const NOT_FIRE: &[u32] = &[0x0100_1645, 0x0100_1318, 0x0100_1317];

/// Whether an emitter keeps on: a set count or time of particles is a puff or a jet that dies
/// away, not a standing fire.
pub(crate) fn burns_on(e: &ParticleEmitterInfo) -> bool {
    e.total_particles <= 0 && e.total_seconds <= 0.0
}

/// Whether an emitter is a broad glow, not a fire: still particles four metres or more across (a
/// dungeon's warm haze, a swirling orb, a sheet of light on the floor). Retail's fire puff is fire
/// at any size.
pub(crate) fn broad_glow(store: &RetailDatStore, e: &ParticleEmitterInfo) -> bool {
    if e.a.z > 0.05 {
        return false;
    }
    let gid = if e.hw_gfxobj_id.0 != 0 {
        e.hw_gfxobj_id
    } else {
        e.gfxobj_id
    };
    let Some(gfx) = decode::<GfxObj>(store, DbType::GfxObj, gid) else {
        return false;
    };
    let (lo, hi) = gfx.vertex_array.vertices.iter().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), v| {
            let p = Vec3::new(v.position.x, v.position.y, v.position.z);
            (lo.min(p), hi.max(p))
        },
    );
    (hi - lo).max_element() * e.start_scale.max(e.final_scale) >= 4.0
        && crate::body_fire::fire_picture(store, gid).is_none()
}

/// The colour of a fire-coloured additive sprite, or `None` for any other.
pub(crate) fn flame_colour(store: &RetailDatStore, e: &ParticleEmitterInfo) -> Option<Vec3> {
    let gid = if e.hw_gfxobj_id.0 != 0 {
        e.hw_gfxobj_id
    } else {
        e.gfxobj_id
    };
    if NOT_FIRE.contains(&gid.0) {
        return None;
    }
    let gfx: GfxObj = decode(store, DbType::GfxObj, gid)?;
    let src = TextureSource::new(store);
    let mut sum = Vec3::ZERO;
    let mut glows = false;
    for s in &gfx.surfaces {
        let Some(surf) = decode::<Surface>(store, DbType::Surface, *s) else {
            continue;
        };
        glows |= surf.surface_type & 0x1_0000 != 0 || surf.luminosity > 0.0;
        let c = surf.color_value.map(argb_linear).or_else(|| {
            let t = surf.orig_texture_id?;
            let a = src.average(t).ok()?;
            let f = crate::asset::material::srgb_to_linear;
            Some(Vec3::new(f(a[0]), f(a[1]), f(a[2])))
        });
        sum += c.unwrap_or(Vec3::ZERO);
    }
    let (h, s) = hue_saturation(sum);
    if glows && sum.max_element() > 0.0 && s >= 0.3 && (8.0..=60.0).contains(&h) {
        return Some(normalised(sum));
    }
    coloured_flame(store, e)
}

/// The colour of a torch's flame of another colour than fire's (blue, green, violet): retail's
/// torch flame recoloured. `None` for any other sprite.
pub(crate) fn coloured_flame(store: &RetailDatStore, e: &ParticleEmitterInfo) -> Option<Vec3> {
    let gid = if e.hw_gfxobj_id.0 != 0 {
        e.hw_gfxobj_id
    } else {
        e.gfxobj_id
    };
    let c = crate::body_fire::torch_picture(store, gid)?;
    let (h, _) = hue_saturation(c);
    (!(8.0..=60.0).contains(&h)).then_some(c)
}

/// A sprite's tone: grey or white (smoke's), cool (blue to cyan, mist's), a strong green (acid's),
/// and lying flat (a sheet not turned to the eye, as ground mist is drawn).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tone {
    pub grey: bool,
    pub cool: bool,
    pub green: bool,
    pub flat: bool,
}

/// Picture `gid`'s [`Tone`].
pub fn tone(store: &RetailDatStore, gid: DataId) -> Tone {
    let Some(gfx) = decode::<GfxObj>(store, DbType::GfxObj, gid) else {
        return Tone::default();
    };
    let (lo, hi) = gfx.vertex_array.vertices.iter().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), v| {
            let p = Vec3::new(v.position.x, v.position.y, v.position.z);
            (lo.min(p), hi.max(p))
        },
    );
    let size = hi - lo;
    let flat = size.z < 0.6 * size.x.min(size.y);
    let src = TextureSource::new(store);
    let mut sum = Vec3::ZERO;
    for s in &gfx.surfaces {
        let Some(surf) = decode::<Surface>(store, DbType::Surface, *s) else {
            return Tone::default();
        };
        let c = surf.color_value.map(argb_linear).or_else(|| {
            let a = src.average_linear(surf.orig_texture_id?).ok()?;
            Some(Vec3::from(a))
        });
        sum += c.unwrap_or(Vec3::ZERO);
    }
    let (h, sat) = hue_saturation(sum);
    let lit = sum.max_element() > 0.0;
    Tone {
        grey: lit && sat < 0.25,
        cool: lit && (160.0..=280.0).contains(&h),
        green: lit && sat >= 0.7 && (95.0..=150.0).contains(&h),
        flat,
    }
}

/// What one object id gives light with, from the dats.
#[must_use]
pub fn examine(store: &RetailDatStore, id: DataId) -> Vec<LampSource> {
    if divine_type(id) != Some(DbType::Setup) {
        return Vec::new();
    }
    let Some(setup) = decode::<Setup>(store, DbType::Setup, id) else {
        return Vec::new();
    };
    let parts = parts(&setup);
    let emitters = script_emitters(store, &setup, &parts);
    // Each fire's place and colour, and whether it is a broad glow, which lights steadily.
    let flames: Vec<(Vec3, Vec3, bool)> = emitters
        .iter()
        .filter(|(e, _)| burns_on(e))
        .filter_map(|(e, at)| {
            let c = flame_colour(store, e)?;
            // The flame's light sits a little above where its particles are born.
            let rise = (e.a.z * e.max_a.max(e.min_a) * 0.15).clamp(0.0, 0.4);
            Some((*at + Vec3::Z * rise, c, broad_glow(store, e)))
        })
        .collect();
    let mut out = Vec::new();
    for l in setup.lights.values() {
        let dats = Vec3::new(l.frame.origin.x, l.frame.origin.y, l.frame.origin.z);
        let mut at = dats;
        if (at - Vec3::new(-2.0, -2.0, 2.0)).length() < 1e-3 {
            at = emitters
                .first()
                .map(|(_, p)| *p + Vec3::Z * 0.15)
                .or_else(|| {
                    bounds(store, &parts).map(|(lo, hi)| {
                        Vec3::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5, hi.z - 0.1)
                    })
                })
                .unwrap_or(at);
        }
        let near_flame = flames
            .iter()
            .any(|(p, _, glow)| !glow && (*p - at).length() < 1.5);
        out.push(LampSource {
            client_offset: (at != dats).then_some(dats),
            kind: LampKind::Authored,
            offset: at,
            colour: normalised(argb_linear(l.color_argb)),
            intensity: l.intensity.max(0.0),
            falloff: l.falloff.max(0.5),
            flicker: if near_flame {
                Flicker::Flame
            } else {
                Flicker::Steady
            },
            shadows: true,
        });
    }
    if out.is_empty() {
        for (p, c, glow) in flames {
            out.push(LampSource {
                client_offset: None,
                kind: LampKind::Flame,
                offset: p,
                colour: c,
                intensity: 1.0,
                falloff: 8.0,
                flicker: if glow {
                    Flicker::Steady
                } else {
                    Flicker::Flame
                },
                shadows: true,
            });
        }
    }
    out
}

/// A surface's linear colour, its own or its picture's mean.
fn surface_colour(src: &TextureSource<'_>, surf: &Surface) -> Option<Vec3> {
    surf.color_value
        .map(argb_linear)
        .or_else(|| Some(Vec3::from(src.average_linear(surf.orig_texture_id?).ok()?)))
}

/// Whether a surface glows: luminous or additive.
fn glowing(surf: &Surface) -> bool {
    surf.surface_type & 0x1_0000 != 0 || surf.luminosity > 0.0
}

/// The glow of object `id` with no light of its own: where most of its parts' visible surfaces
/// glow, one light at its standing emitters whose sprites glow, of their colour.
#[must_use]
pub fn glows(store: &RetailDatStore, id: DataId) -> Vec<LampSource> {
    if divine_type(id) != Some(DbType::Setup) {
        return Vec::new();
    }
    let Some(setup) = decode::<Setup>(store, DbType::Setup, id) else {
        return Vec::new();
    };
    let src = TextureSource::new(store);
    let (mut seen, mut lit) = (0u32, 0u32);
    for gid in &setup.parts {
        let Some(gfx) = decode::<GfxObj>(store, DbType::GfxObj, *gid) else {
            continue;
        };
        for s in &gfx.surfaces {
            let Some(surf) = decode::<Surface>(store, DbType::Surface, *s) else {
                continue;
            };
            // A wholly translucent surface is not seen.
            if surf.translucency >= 1.0 {
                continue;
            }
            seen += 1;
            lit += u32::from(glowing(&surf));
        }
    }
    let parts = parts(&setup);
    let (mut sum, mut at, mut n, mut acid) = (Vec3::ZERO, Vec3::ZERO, 0u16, 0u16);
    for (e, p) in script_emitters(store, &setup, &parts) {
        if !burns_on(&e) {
            continue;
        }
        let gid = if e.hw_gfxobj_id.0 != 0 {
            e.hw_gfxobj_id
        } else {
            e.gfxobj_id
        };
        let Some(gfx) = decode::<GfxObj>(store, DbType::GfxObj, gid) else {
            continue;
        };
        let surfs: Vec<Surface> = gfx
            .surfaces
            .iter()
            .filter_map(|s| decode::<Surface>(store, DbType::Surface, *s))
            .collect();
        if surfs.is_empty() || !surfs.iter().all(glowing) {
            continue;
        }
        // A glint (a gem golem's white sparkles) gives no light.
        if crate::glow::glow_of(store, gid).is_some_and(|g| g.glint()) {
            continue;
        }
        // Steam drawn as a volume gives no light.
        let stream = crate::puffs::Stream {
            endless: true,
            lifespan: e.lifespan,
            start_scale: e.start_scale,
            final_scale: e.final_scale,
            light: e.max_particles <= 2
                && (e.start_scale - e.final_scale).abs() < 0.5
                && (e.start_trans - e.final_trans).abs() < 0.02,
            rise: crate::puffs::rise(
                pyreal_animation::data::ParticleType::from_i32(e.particle_type),
                [e.a.z * e.max_a, e.b.z * e.max_b, e.c.z * e.max_c],
            ),
        };
        if crate::puffs::steam_of(tone(store, gid), true, &stream).is_some() {
            continue;
        }
        acid += u16::from(crate::puffs::is_acid(
            tone(store, gid).green,
            surfs.iter().any(|s| s.surface_type & 0x1_0000 != 0),
            true,
            e.is_parent_local != 0,
            (e.start_scale, e.final_scale),
            e.lifespan,
            e.max_particles,
        ));
        sum += surfs
            .iter()
            .filter_map(|s| surface_colour(&src, s))
            .sum::<Vec3>();
        at += p;
        n += 1;
    }
    // A body with acid in it glows brighter and wider, whatever its parts.
    let acid = acid > 0;
    // None, a body not lit, or too dark to light anything.
    if n == 0 || (!acid && seen > 0 && lit * 2 < seen) || sum.max_element() < 0.02 {
        return Vec::new();
    }
    vec![LampSource {
        client_offset: None,
        kind: LampKind::Glow,
        offset: at / f32::from(n),
        colour: normalised(sum),
        intensity: if acid {
            crate::puffs::ACID_GLOW_INTENSITY
        } else {
            GLOW_INTENSITY
        },
        falloff: if acid {
            crate::puffs::ACID_GLOW_FALLOFF
        } else {
            GLOW_FALLOFF
        },
        flicker: Flicker::Steady,
        shadows: false,
    }]
}

/// Whether a glow is an acid body's ([`crate::puffs`]).
#[must_use]
pub fn acid(s: &LampSource) -> bool {
    s.kind == LampKind::Glow && (s.falloff - crate::puffs::ACID_GLOW_FALLOFF).abs() < 1e-6
}

/// The pack's lights for `id` placed outdoors (`outdoors`) or in a room, standing in for retail's
/// `retail` sources; `None` when the pack gives the object none there.
#[must_use]
pub fn pack_sources(
    pack: &pyreal_art_pack::ArtPack,
    id: DataId,
    outdoors: bool,
    retail: &[LampSource],
) -> Option<Vec<LampSource>> {
    use pyreal_assets::art_pack::LightFlicker;
    let mine: Vec<_> = pack
        .lights
        .iter()
        .filter(|l| l.object == id && l.place.takes(outdoors))
        .collect();
    if mine.is_empty() {
        return None;
    }
    let anchors: Vec<Vec3> = if retail.is_empty() {
        vec![Vec3::ZERO]
    } else {
        retail.iter().map(|s| s.offset).collect()
    };
    let mut out = Vec::new();
    for l in mine {
        for a in &anchors {
            out.push(LampSource {
                client_offset: None,
                kind: LampKind::Pack,
                offset: *a + Vec3::from(l.offset),
                colour: kelvin(l.kelvin),
                intensity: l.brightness,
                falloff: l.reach.max(0.5),
                flicker: match l.flicker {
                    LightFlicker::Flame => Flicker::Flame,
                    LightFlicker::Embers => Flicker::Embers,
                    LightFlicker::Steady => Flicker::Steady,
                },
                shadows: l.shadows,
            });
            if l.gather > 0.0 {
                break;
            }
        }
    }
    Some(out)
}

/// `s` placed at `at` in cell slot `cell`, a part of object `object`.
#[must_use]
pub fn place(s: &LampSource, at: &Affine3A, cell: u32, object: u32) -> Lamp {
    let room = crate::renderer::ROOM_LUX / crate::sky::SCENE_LUX;
    let (candela, range) = match s.kind {
        // Lumens over the whole sphere, lux at a metre.
        LampKind::Pack => (
            s.intensity / (4.0 * std::f32::consts::PI) / crate::sky::SCENE_LUX,
            s.falloff,
        ),
        _ if magic(s.kind, s.colour) => (
            s.intensity.min(MAGIC_INTENSITY) * room * MATCH_DISTANCE,
            s.falloff * RANGE_OF_FALLOFF,
        ),
        _ => (
            s.intensity * room * MATCH_DISTANCE,
            s.falloff * RANGE_OF_FALLOFF,
        ),
    };
    let retail = (s.colour
        * if s.kind == LampKind::Pack {
            1.0
        } else {
            s.intensity
        })
    .extend(range);
    // Retail sums a lamp's colour with the white world ambient in its own colour space and
    // clamps, which pales it; a linear sum keeps it whole, so the dats' colours are paled here.
    let colour = if s.kind == LampKind::Pack {
        s.colour
    } else {
        let luma = s.colour.dot(Vec3::new(0.2126, 0.7152, 0.0722));
        let (grey, paling) = if magic(s.kind, s.colour) {
            (luma.min(MAGIC_GREY * s.colour.max_element()), MAGIC_PALING)
        } else if s.kind == LampKind::Mood {
            // A dungeon's mood light keeps retail's colour whole, its room being dark.
            (luma, 0.0)
        } else {
            (luma, RETAIL_PALING)
        };
        s.colour.lerp(Vec3::splat(grey), paling)
    };
    Lamp {
        position: at.transform_point3(s.offset),
        colour,
        candela,
        range,
        retail,
        intensity: if s.kind == LampKind::Pack {
            1.0
        } else {
            s.intensity
        },
        client: s.client_offset.map(|o| at.transform_point3(o)),
        flicker: s.flicker,
        shadows: s.shadows,
        cell,
        kind: s.kind,
        object,
        flash: false,
        held: false,
        seed: 0,
    }
}

/// The lamps that can reach what `planes` (the view's frustum) holds, `eye` the camera, those
/// in rooms only when their room is seen (`seen(cell)`), nearest first, at most
/// [`MAX_FRAME_LAMPS`].
#[must_use]
pub fn frame_lamps(
    lamps: &[Lamp],
    planes: &[Vec4],
    eye: Vec3,
    seen: impl Fn(u32) -> bool,
) -> Vec<Lamp> {
    frame_lamps_within(lamps, planes, eye, seen, LAMP_DISTANCE)
}

/// [`frame_lamps`] kept within `reach` metres of the eye (the Light Draw Distance), each fading
/// out over the last quarter of it rather than going out at a line.
pub fn frame_lamps_within(
    lamps: &[Lamp],
    planes: &[Vec4],
    eye: Vec3,
    seen: impl Fn(u32) -> bool,
    reach: f32,
) -> Vec<Lamp> {
    frame_lamps_within_by(lamps, planes, eye, |l| seen(l.cell), reach)
}

fn frame_lamps_within_by(
    lamps: &[Lamp],
    planes: &[Vec4],
    eye: Vec3,
    kept: impl Fn(&Lamp) -> bool,
    reach: f32,
) -> Vec<Lamp> {
    let mut near: Vec<(f32, Lamp)> = lamps
        .iter()
        .filter(|l| {
            let d = l.position.distance(eye);
            d - l.range < reach
                && planes
                    .iter()
                    .all(|p| p.truncate().dot(l.position) + p.w >= -l.range)
                && kept(l)
        })
        .map(|l| (l.position.distance_squared(eye), *l))
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    near.truncate(MAX_FRAME_LAMPS);
    near.into_iter()
        .map(|(d2, mut l)| {
            let fade = ((reach - d2.sqrt()) / (reach * 0.25)).clamp(0.0, 1.0);
            l.candela *= fade;
            l.retail *= fade;
            l
        })
        .filter(|l| l.candela > 0.0 || l.retail.truncate().length_squared() > 0.0)
        .collect()
}

/// [`frame_lamps_within`], a lamp in a room not seen kept too where its reach touches one of
/// `seen_rooms` (each room's box): its light falls through the doorway, or its object stands in
/// the next room's cell.
pub fn frame_lamps_reaching(
    lamps: &[Lamp],
    planes: &[Vec4],
    eye: Vec3,
    seen: impl Fn(u32) -> bool,
    seen_rooms: &[(Vec3, Vec3)],
    reach: f32,
) -> Vec<Lamp> {
    let touches = |l: &Lamp| {
        l.cell != 0
            && seen_rooms.iter().any(|(lo, hi)| {
                l.position.clamp(*lo, *hi).distance_squared(l.position) < l.range * l.range
            })
    };
    frame_lamps_within_by(lamps, planes, eye, |l| seen(l.cell) || touches(l), reach)
}

/// The most lamps the probes' rays are lit by.
pub const PROBE_LAMPS: usize = 32;

/// The nearest [`PROBE_LAMPS`] of `near` (nearest `eye` first), the farthest fading out toward
/// the next one's distance, so a lamp joining or leaving them does not pop.
#[must_use]
pub fn probe_lamps(mut near: Vec<Lamp>, eye: Vec3) -> Vec<Lamp> {
    if let Some(next) = near.get(PROBE_LAMPS) {
        let cut = next.position.distance(eye);
        near.truncate(PROBE_LAMPS);
        for l in &mut near {
            let fade = ((cut - l.position.distance(eye)) / (cut * 0.25).max(1e-3)).clamp(0.0, 1.0);
            l.candela *= fade;
            l.retail *= fade;
        }
    }
    near
}

/// A lamp's bytes: position and range; colour times candela; the client's colour times
/// intensity; flicker and flags (indoors, shadowed, the dats', a found flame, magic, acid, a
/// strike, a portal); its shadow
/// slot and a seed; its object.
#[must_use]
pub fn lamp_bytes(l: &Lamp, slot: u32, seed: u32) -> [u8; 64] {
    lamp_bytes_in(l, slot, seed, (0, 0))
}

/// [`lamp_bytes`], lighting only the `rooms` (first box record, count; no count for everywhere).
#[must_use]
pub fn lamp_bytes_in(l: &Lamp, slot: u32, seed: u32, rooms: (u32, u32)) -> [u8; 64] {
    let mut b = [0u8; 64];
    let words: [u32; 16] = [
        l.position.x.to_bits(),
        l.position.y.to_bits(),
        l.position.z.to_bits(),
        l.range.to_bits(),
        (l.colour.x * l.candela).to_bits(),
        (l.colour.y * l.candela).to_bits(),
        (l.colour.z * l.candela).to_bits(),
        l.flicker.word()
            | (u32::from(l.cell != 0) << 8)
            | (u32::from(l.shadows) << 9)
            | (u32::from(l.kind != LampKind::Pack) << 10)
            | (u32::from(l.kind == LampKind::Flame) << 11)
            | (u32::from(magic(l.kind, l.retail.truncate())) << 12)
            | (u32::from(
                l.kind == LampKind::Glow
                    && (l.retail.w - crate::puffs::ACID_GLOW_FALLOFF * RANGE_OF_FALLOFF).abs()
                        < 1e-4,
            ) << 13)
            | (u32::from(l.flash) << 14)
            | (u32::from(l.kind == LampKind::Portal) << 15)
            | (u32::from(l.held) << 16)
            | (u32::from(l.moves()) << 17),
        l.retail.x.to_bits(),
        l.retail.y.to_bits(),
        l.retail.z.to_bits(),
        (slot & 0xFF) | (seed << 8),
        l.object & 0xFF_FFFF,
        rooms.0,
        rooms.1,
        l.intensity.to_bits(),
    ];
    for (k, w) in words.iter().enumerate() {
        b[k * 4..k * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    b
}

/// Each examined object id's lamps.
#[derive(Debug, Default)]
pub struct LampCache {
    // ORDER-OK: looked up by object id, never iterated.
    sources: HashMap<u32, Vec<LampSource>>,
    // ORDER-OK: looked up by object id, never iterated.
    glows: HashMap<u32, Vec<LampSource>>,
}

impl LampCache {
    /// The lamps of object `id`, examined the first time.
    pub fn sources(&mut self, store: &RetailDatStore, id: DataId) -> &[LampSource] {
        self.sources
            .entry(id.0)
            .or_insert_with(|| examine(store, id))
    }

    /// The glow of object `id` ([`glows`]), examined the first time.
    pub fn glows(&mut self, store: &RetailDatStore, id: DataId) -> &[LampSource] {
        self.glows.entry(id.0).or_insert_with(|| glows(store, id))
    }
}
