//! Portals in the world drawn as a vortex, with dust drawn into them.
//!
//! A portal's swirl is its setup's default script: a turning emitter whose pictures shrink as
//! they show, and a glow of the portal's colour. In place of those particles the portal is drawn
//! as one ball of the puffs' volume ([`crate::puffs::GATE`]): a disc turned to face the eye, so it
//! is seen face on from wherever it is seen, its arms spiralling in to a dark heart and turning
//! faster the nearer they come, as a smoke whirl's do ([`crate::puffs::VORTEX`]), glowing in the
//! colour of the portal's own glow picture. `shaders/gates.wgsl` draws it.
//!
//! It lights its surroundings in its own colour ([`lamp`]), in place of its setup's own light.
//!
//! Dust from round it drifts in and spirals into it, faster as it nears, lit as the dust it is
//! and turning purple as it enters ([`motes`]). A few dozen motes a portal at most, fewer with
//! distance and none past [`MOTE_FAR`].
//!
//! The portals are told by their setups' default scripts ([`SCRIPTS`]), which every era shares.
//! A broken or destroyed portal, a portal's swirl on a gem or an orb, and creatures that carry one
//! keep retail's look.

use glam::{Affine3A, Mat3, Vec3};
use pyreal_animation::data::ParticleType;
use pyreal_animation::hooks::HookKind;
use pyreal_animation::MotionDriver;
use pyreal_primitives::num::floor_to_i32;
use pyreal_primitives::num::math::{cosf, logf, powf, sinf};
use pyreal_primitives::DataId;

use crate::particles::{sprite_of, Drawn, Look, Sprite};

/// The portals' setups (each by its default script), with what they are.
pub const SCRIPTS: [(u32, u32, &str); 11] = [
    (0x3300_067A, 0x0200_01B3, "purple"),
    (0x3300_06FA, 0x0200_05D2, "blue"),
    (0x3300_06FB, 0x0200_05D3, "green"),
    (0x3300_06FC, 0x0200_05D4, "orange"),
    (0x3300_06FD, 0x0200_05D5, "red"),
    (0x3300_06FE, 0x0200_05D6, "yellow"),
    (0x3300_07A4, 0x0200_06F4, "white"),
    (0x3300_0952, 0x0200_08FD, "shadow"),
    (0x3300_0E06, 0x0200_1029, "red and shadow"),
    (0x3300_0E05, 0x0200_102A, "orange and shadow"),
    (0x3300_10EF, 0x0200_167E, "great purple"),
];

/// The disc's radius over the setup's height, how high its middle stands over the ground (over its
/// radius), and how far its colour is reckoned from its picture's mean colour (a glow picture's
/// mean is about half its colour; a swirl's darker).
const RADIUS: f32 = 0.98;
const LIFT: f32 = 0.85;
const GLOW_MEAN: f32 = 0.55;
const SWIRL_MEAN: f32 = 0.22;
/// How much deeper its colour's hue is than its picture's mean, which the picture's dark edge
/// greys.
const HUE: f32 = 1.6;
/// How deep the funnel is (over the radius), as `shaders/gates.wgsl`'s `GATE_FUNNEL`.
const FUNNEL: f32 = 0.5;
/// The ball's reach over the disc's radius: its arms' thickness kept inside the card.
const BALL_REACH: f32 = 1.15;
/// Where a portal fades out, a share of its swirl's reach.
const FADE_FROM: f32 = 0.85;

/// A portal as it is drawn: the middle of its disc, its facing (drawn toward the eye), its radius
/// and the ground under it (render space); its colour and its heart's (linear, the brightest
/// channel at most 1), a seed of its own, and a picture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gate {
    pub centre: Vec3,
    pub axis: Vec3,
    pub radius: f32,
    pub base: f32,
    pub colour: Vec3,
    pub heart: Vec3,
    pub seed: u32,
    /// The picture its dust is drawn with (its first emitter's).
    pub sprite: u32,
}

/// Whether a setup whose default script is `script` is a portal's.
#[must_use]
pub fn is_portal_script(script: DataId) -> bool {
    SCRIPTS.iter().any(|s| s.0 == script.0)
}

/// Whether `driver` is a portal's.
#[must_use]
pub fn is_portal(driver: &MotionDriver) -> bool {
    driver
        .part_array
        .setup
        .as_ref()
        .and_then(|s| s.default_script)
        .is_some_and(is_portal_script)
}

/// The portal `driver` is, moved by `offset` into render space, once its pictures are ready
/// (`sprites`); `None` when it is no portal.
#[must_use]
pub fn gate_of(
    driver: &MotionDriver,
    sprites: &std::collections::HashMap<u32, Option<Sprite>>,
    offset: Vec3,
) -> Option<Gate> {
    let setup = driver.part_array.setup.as_ref()?;
    let script = setup.default_script.filter(|s| is_portal_script(*s))?;
    // Its colours: its glows', else its swirls', as its script starts them (not every one of them
    // may be live).
    let (mut glows, mut swirls, mut sprite) = (Vec::new(), Vec::new(), None);
    for e in emitters(&*driver.assets, script) {
        let id = sprite_of(&e).0;
        let s = sprites.get(&id)?.as_ref()?;
        match e.particle_type {
            ParticleType::ParabolicLvgagr => glows.push(s.colour / GLOW_MEAN),
            ParticleType::ParabolicLvlalr if e.final_scale < e.start_scale * 0.3 => {
                swirls.push(s.colour / SWIRL_MEAN);
            }
            _ => continue,
        }
        sprite.get_or_insert(id);
    }
    // Placed by its turning swirl, else by its frame.
    let (centre, axis) = driver
        .particles
        .iter()
        .find(|e| e.info.particle_type == ParticleType::ParabolicLvlalr)
        .map_or_else(
            || {
                let t = crate::asset::object::frame_affine(&driver.particle_parent_frame());
                (
                    Vec3::from(t.translation),
                    Vec3::from(t.matrix3.y_axis).normalize_or(Vec3::Y),
                )
            },
            |e| crate::particles::whirl(driver, e),
        );
    let colours = if glows.is_empty() { swirls } else { glows };
    // Its brightest channel at most 1, its hue deepened as its picture shows it.
    let held = |c: Vec3| {
        let most = c.max_element().max(1e-4);
        let h = c / most;
        Vec3::new(powf(h.x, HUE), powf(h.y, HUE), powf(h.z, HUE)) * most.min(1.0)
    };
    let colour = held(*colours.first()?);
    let heart = held(*colours.last()?);
    let scale = driver.part_array.scale.x.max(1e-3);
    let ground = driver.particle_parent_frame().origin;
    let radius = setup.height * RADIUS * scale;
    let centre = centre + offset;
    #[allow(clippy::cast_sign_loss)] // a hash of its place
    let seed = (floor_to_i32(ground.x * 7.31) as u32).wrapping_mul(0x9E37_79B9)
        ^ (floor_to_i32(ground.y * 5.17) as u32).wrapping_mul(0x85EB_CA6B);
    Some(Gate {
        centre: Vec3::new(centre.x, centre.y, ground.z + offset.z + radius * LIFT),
        axis,
        radius,
        base: ground.z + offset.z,
        colour,
        heart,
        seed,
        sprite: sprite?,
    })
}

/// The emitters `script` starts itself.
fn emitters(
    assets: &dyn pyreal_animation::data::AnimAssets,
    script: DataId,
) -> Vec<std::sync::Arc<pyreal_animation::data::ParticleEmitterInfo>> {
    assets.script(script).map_or_else(Vec::new, |s| {
        s.steps
            .iter()
            .filter_map(|step| match &step.hook.kind {
                HookKind::CreateParticle(c) | HookKind::CreateBlockingParticle(c) => {
                    assets.emitter_info(c.emitter_info_id)
                }
                _ => None,
            })
            .collect()
    })
}

/// The pictures portal `driver`'s script draws, for [`crate::particles::prepare_sprite`].
#[must_use]
pub fn pictures(driver: &MotionDriver) -> Vec<DataId> {
    let script = driver
        .part_array
        .setup
        .as_ref()
        .and_then(|s| s.default_script);
    match script.filter(|s| is_portal_script(*s)) {
        Some(s) => emitters(&*driver.assets, s)
            .iter()
            .map(|e| sprite_of(e))
            .collect(),
        None => Vec::new(),
    }
}

/// Gate `g`'s ball and its dust as the particles draw them, seen through cell `cell` from `eye` at
/// `now`, `shown` as much as it shows, as body `group`.
#[must_use]
pub fn drawn(
    g: &Gate,
    cell: u32,
    eye: Vec3,
    now: f64,
    air: Air,
    shown: f32,
    group: u32,
) -> Vec<Drawn> {
    if shown <= 0.0 {
        return Vec::new();
    }
    // Its disc faced toward the eye; its dust drawn in about its own axis, which holds still
    // however the eye moves.
    let faced = Gate {
        axis: (eye - g.centre).normalize_or(g.axis),
        ..*g
    };
    let mut out = vec![Drawn {
        sprite: g.sprite,
        transform: ball(&faced),
        alpha: shown,
        cell,
        distance: g.centre.distance(eye),
        light: false,
        look: Look::Puff(crate::puffs::GATE),
        age: 0.0,
        seed: crate::puffs::tint_bits(g.colour),
        group,
    }];
    let dust = air.dust();
    out.extend(motes(g, now, eye, air).into_iter().map(|m| {
        let alpha = m.alpha * shown;
        Drawn {
            sprite: g.sprite,
            transform: streaked(m),
            alpha,
            cell,
            distance: m.at.distance(eye),
            light: false,
            look: Look::Mote,
            age: 0.0,
            seed: mote_word(dust, m.purple, alpha),
            group,
        }
    }));
    out
}

/// The light a portal gives: of its colour (a dark one's dimmer), from low in its disc, washing the
/// ground round it; its falloff (metres); indoors as bright as a magic light this many times over;
/// how far its colour is paled (a magic light's is paled more); and how high over its ground it
/// stands.
const LAMP_FALLOFF: f32 = 8.0;
const LAMP_GAIN: f32 = 4.0;
const LAMP_PALING: f32 = 0.15;
const LAMP_HIGH: f32 = 1.2;

/// Gate `g`'s lamp in cell slot `cell`, `shown` as much as it shows.
#[must_use]
pub fn lamp(g: &Gate, cell: u32, shown: f32) -> crate::lamps::Lamp {
    let most = g.colour.max_element().max(1e-4);
    let colour = g.colour / most;
    let source = crate::lamps::LampSource {
        kind: crate::lamps::LampKind::Portal,
        offset: Vec3::ZERO,
        colour,
        intensity: crate::lamps::MAGIC_INTENSITY,
        falloff: LAMP_FALLOFF,
        flicker: crate::lamps::Flicker::Steady,
        shadows: false,
    };
    let at = Vec3::new(g.centre.x, g.centre.y, g.base + LAMP_HIGH);
    let mut l = crate::lamps::place(&source, &Affine3A::from_translation(at), cell, u32::MAX);
    let luma = colour.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    l.colour = colour.lerp(Vec3::splat(luma), LAMP_PALING);
    l.candela *= LAMP_GAIN * most.min(1.0) * shown;
    l
}

/// How much of a portal shows at `distance` from the eye, its swirl reaching `reach`.
#[must_use]
pub fn shown(distance: f32, reach: f32) -> f32 {
    1.0 - smooth((distance - reach * FADE_FROM) / (reach * (1.0 - FADE_FROM)).max(1e-3))
}

/// Gate `g`'s ball as the puffs' list carries it: its centre, its reach in `x_axis.x`, its seed in
/// `x_axis.y`, its axis where a whirl's is (`y_axis.y`, `z_axis.z`, `x_axis.z`), its radius in
/// `y_axis.z` and its heart's colour in the entries left (`y_axis.x`, `z_axis.x`, `z_axis.y`).
/// Its colour rides in its seed ([`crate::puffs::tint_bits`]).
#[must_use]
pub fn ball(g: &Gate) -> Affine3A {
    #[allow(clippy::cast_precision_loss)] // a share
    let phase = (g.seed >> 8) as f32 / 16_777_216.0;
    Affine3A {
        matrix3: Mat3::from_cols(
            Vec3::new(
                g.radius * BALL_REACH / crate::puffs::PUFF_SIZE,
                phase,
                g.axis.z,
            ),
            Vec3::new(g.heart.x, g.axis.x, g.radius),
            Vec3::new(g.heart.y, g.heart.z, g.axis.y),
        )
        .into(),
        translation: g.centre.into(),
    }
}

/// The most dust motes a portal draws, how far away it draws them at all and at all of them
/// (metres), and their least and greatest radius.
pub const MOTES: u32 = 36;
pub const MOTE_FAR: f32 = 40.0;
const MOTE_NEAR: f32 = 14.0;
const MOTE_SIZE: [f32; 2] = [0.035, 0.06];
/// How long a mote takes to be drawn in (seconds, least and most), how far out (metres) and how
/// high (over the ground) it starts, how many turns it makes as it closes in, and the share of a
/// trip it takes to show.
const MOTE_TRIP: [f32; 2] = [6.0, 10.0];
const MOTE_FROM: [f32; 2] = [2.2, 7.0];
const MOTE_HIGH: f32 = 2.2;
const MOTE_SPIN: f32 = 1.3;
const MOTE_SHOW: f32 = 0.15;
/// How a trip's way closes: the share left goes as the share of time left to this power, so a
/// mote drifts at first and plunges at the end.
const MOTE_FALL: f32 = 0.22;
/// Where (over the disc's radius) a mote crosses into the portal and turns purple, and over how
/// far: sharply, as over an edge.
const MOTE_HORIZON: [f32; 2] = [1.0, 0.15];
/// How near the heart (over the disc's radius) a mote is swallowed.
const MOTE_SWALLOWED: f32 = 0.3;
/// The share of a mote the disc hides when it is behind it.
const MOTE_HIDDEN: f32 = 0.7;

/// The air round the portals: how hard it rains and snows, and how much of the land is snowy.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Air {
    pub rain: f32,
    pub snow: f32,
    pub snowy: f32,
}

impl Air {
    /// The dust's colour: dry earth's, snow's on snowy land or in snowfall, darker in the wet.
    #[must_use]
    pub fn dust(&self) -> Vec3 {
        let dry = Vec3::new(0.62, 0.55, 0.44);
        let white = Vec3::new(0.85, 0.87, 0.9);
        let c = dry.lerp(white, self.snow.max(self.snowy).clamp(0.0, 1.0));
        c * (1.0 - 0.45 * self.rain.clamp(0.0, 1.0))
    }

    /// The share of the motes drawn: the rain lays the dust.
    #[must_use]
    pub fn stirred(&self) -> f32 {
        1.0 - 0.6 * self.rain.clamp(0.0, 1.0)
    }
}

/// One dust mote: where it is, its radius, how much of it shows, how purple it has turned, and
/// how far it moves while the eye takes it in (its streak).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mote {
    pub at: Vec3,
    pub radius: f32,
    pub alpha: f32,
    pub purple: f32,
    pub streak: Vec3,
}

/// How long the eye takes a mote in, seconds: how long its streak is.
const MOTE_GLIMPSE: f32 = 0.05;

/// A number from 0 to 1 dealt by `h`.
fn unit(h: u32) -> f32 {
    let mut x = h.wrapping_mul(0x9E37_79B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    x = x.wrapping_mul(0xC2B2_AE35);
    x ^= x >> 16;
    #[allow(clippy::cast_precision_loss)] // 24 bits
    let u = (x >> 8) as f32 / 16_777_216.0;
    u
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The dust being drawn into gate `g` at `now` (seconds), seen from `eye`, in `air`: each mote on
/// a trip of its own from somewhere round the portal, closing in on the disc's middle and turning
/// about its axis ever faster as it nears, drawn into the disc's plane as it comes. A trip ends in
/// the portal and the next starts elsewhere.
#[must_use]
pub fn motes(g: &Gate, now: f64, eye: Vec3, air: Air) -> Vec<Mote> {
    let far = eye.distance(g.centre);
    let near = 1.0 - smooth((far - MOTE_NEAR) / (MOTE_FAR - MOTE_NEAR));
    if near <= 0.0 {
        return Vec::new();
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let count = (MOTES as f32 * near.sqrt() * air.stirred()).ceil() as u32;
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count {
        let h = g.seed ^ i.wrapping_mul(0x27D4_EB2F);
        let trip = f64::from(MOTE_TRIP[0] + (MOTE_TRIP[1] - MOTE_TRIP[0]) * unit(h));
        let run = now / trip + f64::from(unit(h ^ 0x5bd1));
        #[allow(clippy::cast_possible_truncation)] // a trip's number and share
        let (n, t) = (run.floor() as i64 as u32, run.fract() as f32);
        let k = h ^ n.wrapping_mul(0x1656_67B1);
        let at = place(g, k, t);
        #[allow(clippy::cast_possible_truncation)] // a trip's seconds
        let glimpse = MOTE_GLIMPSE / trip as f32;
        let streak = at - place(g, k, (t - glimpse).max(0.0));
        let inside = inside(g, at);
        // Shown as it starts, swallowed as it reaches the heart.
        let mut alpha = smooth(t / MOTE_SHOW) * smooth(inside / MOTE_SWALLOWED) * near;
        let purple = smooth((MOTE_HORIZON[0] - inside) / MOTE_HORIZON[1] + 0.5);
        // Behind the disc as the eye sees it (turned to face it): hidden by the swirl.
        let facing = (eye - g.centre).normalize_or(g.axis);
        let (de, dm) = ((eye - g.centre).dot(facing), (at - g.centre).dot(facing));
        if de * dm < 0.0 {
            let cross = eye + (at - eye) * (de / (de - dm));
            let r = cross.distance(g.centre) / g.radius;
            alpha *= 1.0 - MOTE_HIDDEN * (1.0 - smooth((r - 0.5) / 0.5));
        }
        if alpha <= 1e-3 {
            continue;
        }
        out.push(Mote {
            at,
            radius: MOTE_SIZE[0] + (MOTE_SIZE[1] - MOTE_SIZE[0]) * unit(k ^ 7),
            alpha,
            purple,
            streak,
        });
    }
    out
}

/// Mote `m` as the particles carry it: its radius in `x_axis.x`, its streak in the entries a
/// whirl's axis takes (`y_axis.y`, `z_axis.z`, `x_axis.z`).
#[must_use]
pub fn streaked(m: Mote) -> Affine3A {
    Affine3A {
        matrix3: Mat3::from_cols(
            Vec3::new(m.radius, 0.0, m.streak.z),
            Vec3::new(0.0, m.streak.x, 0.0),
            Vec3::new(0.0, 0.0, m.streak.y),
        )
        .into(),
        translation: m.at.into(),
    }
}

/// Where the mote of trip `k` is `t` (0 to 1) of the way through it: it starts round the portal,
/// mostly low (the ground's dust), and closes in ever faster on the disc's middle, turning about
/// its axis faster the nearer it is and drawn into the disc's plane as it comes.
#[must_use]
pub fn place(g: &Gate, k: u32, t: f32) -> Vec3 {
    let turn = std::f32::consts::TAU * unit(k ^ 1);
    let out_by = MOTE_FROM[0] + (MOTE_FROM[1] - MOTE_FROM[0]) * unit(k ^ 2);
    let high = MOTE_HIGH * unit(k ^ 3) * unit(k ^ 3);
    let start = Vec3::new(
        g.centre.x + cosf(turn) * out_by,
        g.centre.y + sinf(turn) * out_by,
        g.base + 0.05 + high,
    );
    let d = start - g.centre;
    let along = d.dot(g.axis);
    let across = d - g.axis * along;
    // What is left of the way: a slow drift that quickens without end as it nears, as a fall
    // into a well does; and the turn so far, faster the nearer.
    let left = powf((1.0 - t).max(0.0), MOTE_FALL);
    let spun = MOTE_SPIN * std::f32::consts::TAU * logf(1.0 / (left + 0.06)) / logf(1.0 / 0.06);
    let turned = across * cosf(spun) + g.axis.cross(across) * sinf(spun);
    let wobble = Vec3::new(
        sinf(t * 9.0 + unit(k ^ 4) * 6.0),
        sinf(t * 7.0 + unit(k ^ 5) * 6.0),
        sinf(t * 8.0 + unit(k ^ 6) * 6.0),
    ) * (0.12 * left);
    let at = g.centre + turned * left + g.axis * (along * powf(left, 1.3)) + wobble;
    // Down the funnel as it nears the heart.
    let off = at - g.centre;
    let flat = (off - g.axis * off.dot(g.axis)).length() / g.radius;
    at + g.axis * sunk(flat, g.radius)
}

/// How far `at` is from gate `g`'s heart, over its radius: across its axis, and off its funnel's
/// face along it.
#[must_use]
pub fn inside(g: &Gate, at: Vec3) -> f32 {
    let off = at - g.centre;
    let along = off.dot(g.axis);
    let flat = (off - g.axis * along).length() / g.radius;
    let behind = (along - sunk(flat, g.radius)) / g.radius;
    (flat * flat + behind * behind).sqrt()
}

/// How far the funnel's face lies behind the rim at `out` (over the radius) from the middle, as
/// `shaders/gates.wgsl`'s `gate_sunk`: the heart sunk back away from the eye.
#[must_use]
pub fn sunk(out: f32, radius: f32) -> f32 {
    let x = 1.0 - out.min(1.0);
    -FUNNEL * radius * x * x
}

/// A mote's look in a word: its dust's colour (4 bits a channel, gamma 2.2) at bits 20-31, how
/// purple it is at bits 8-15 and how much of it shows at bits 0-7.
#[must_use]
pub fn mote_word(dust: Vec3, purple: f32, alpha: f32) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 to the bits' most
    let q = |v: f32, most: f32| (v.clamp(0.0, 1.0) * most).round() as u32;
    let g = |v: f32| powf(v.max(0.0), 1.0 / 2.2);
    q(alpha, 255.0)
        | q(purple, 255.0) << 8
        | q(g(dust.z), 15.0) << 20
        | q(g(dust.y), 15.0) << 24
        | q(g(dust.x), 15.0) << 28
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> Gate {
        Gate {
            centre: Vec3::new(10.0, 20.0, 1.35),
            axis: Vec3::Y,
            radius: 1.35,
            base: 0.0,
            colour: Vec3::new(0.6, 0.05, 1.0),
            heart: Vec3::new(0.6, 0.05, 1.0),
            seed: 1234,
            sprite: 0x0100_16C9,
        }
    }

    /// Every mote comes nearer the portal over its trip, ends inside the disc and purple, and is
    /// faster at the end than at the start.
    #[test]
    fn motes_close_in_faster_and_turn_purple() {
        let g = gate();
        let eye = Vec3::new(10.0, 12.0, 1.7);
        let step = 1.0 / 60.0;
        let mut ends = 0;
        for f in 0..600 {
            let now = 100.0 + f64::from(f) * step;
            let a = motes(&g, now, eye, Air::default());
            assert!(a.len() <= MOTES as usize);
            for m in &a {
                assert!(m.at.distance(g.centre) < MOTE_FROM[1] + MOTE_HIGH + 0.5);
                if inside(&g, m.at) < 0.5 {
                    assert!(m.purple > 0.99, "{m:?}");
                    ends += 1;
                }
            }
        }
        assert!(ends > 0, "no mote reached the heart");
        // Each trip: closing slowly as it starts, fast as it ends, and ending at the heart.
        for k in 0..64 {
            let throat = g.centre + g.axis * sunk(0.0, g.radius);
            let left = |t: f32| place(&g, k, t).distance(throat);
            let (drift, plunge) = (
                (left(0.0) - left(0.5)) / 0.5,
                (left(0.95) - left(1.0)) / 0.05,
            );
            assert!(plunge > drift.abs() * 5.0, "trip {k}: {drift} {plunge}");
            assert!(place(&g, k, 1.0).distance(throat) < 0.05, "trip {k}");
        }
    }

    /// No dust past the far distance, all of it near, and less in the rain.
    #[test]
    fn dust_thins_with_distance_and_rain() {
        let g = gate();
        let at = |d: f32| g.centre + Vec3::new(0.0, -d, 0.3);
        let n = |d: f32, air: Air| motes(&g, 50.0, at(d), air).len();
        assert_eq!(n(MOTE_FAR + 1.0, Air::default()), 0);
        let wet = Air {
            rain: 1.0,
            ..Air::default()
        };
        assert!(n(5.0, wet) < n(5.0, Air::default()));
        assert!(
            Air {
                snowy: 1.0,
                ..Air::default()
            }
            .dust()
            .min_element()
                > 0.8
        );
    }

    /// A mote's word carries its colour, its purple and its alpha.
    #[test]
    fn a_motes_word_holds_its_look() {
        let w = mote_word(Vec3::ONE, 1.0, 1.0);
        assert_eq!(w & 0xFF, 255);
        assert_eq!((w >> 8) & 0xFF, 255);
        assert_eq!(w >> 20, 0xFFF);
    }

    /// The portal's colour and its reach ride in its ball's empty entries.
    #[test]
    fn a_gates_ball_carries_its_axis_radius_and_heart() {
        let g = gate();
        let b = ball(&g);
        let m = b.matrix3;
        assert_eq!(Vec3::new(m.y_axis.y, m.z_axis.z, m.x_axis.z), g.axis);
        assert!((m.y_axis.z - g.radius).abs() < 1e-6);
        assert_eq!(Vec3::new(m.y_axis.x, m.z_axis.x, m.z_axis.y), g.heart);
        assert!(m.x_axis.x * crate::puffs::PUFF_SIZE >= g.radius);
    }
}
