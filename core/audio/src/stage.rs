//! The enhanced sound ([`crate::prefs::SoundStyle::Enhanced`]): this client's own engine under the
//! original's sound tables, hooks and ambience.
//!
//! What plays is chosen exactly as the original chooses it ([`crate::AudioSystem`]'s entry
//! points); how it is heard is decided here. Sounds play at their own rate on a [`Mixer`] at the
//! device's rate, through four neighbouring frames, in up to [`VOICES`] voices:
//!
//! * **Room for sixty-four.** When every voice is busy the quietest one that is neither an
//!   interface sound nor the player's own gives way, if it is quieter than the newcomer. A sound
//!   that finds no room is counted ([`StageStats::dropped`]).
//! * **Placed round the listener, every frame.** Gain falls off at 9 dB a doubling beyond
//!   [`NEAR`] and fades out by [`REACH`]; an equal-power pan follows the bearing; a sound behind
//!   is a little duller and quieter, and a far one loses its top end to the air. A sound made by
//!   an object follows the object ([`Stage::move_emitter`]), and gains glide from block to block.
//! * **An echo indoors.** A building's rooms and a dungeon's halls ([`Room`]) send the world's
//!   sounds into a reverberation, the farther the wetter.
//! * **Ambience that loops.** A constant ambient sound whose recording fills its interval loops
//!   with its seam crossfaded and fades with the terrain's share ([`Stage::loop_volume`]).
//! * **The sliders as they read.** Each category's own volume is applied once, live, to what is
//!   already playing; the interface sounds follow the interface slider.

use std::collections::BTreeMap;

use pyreal_primitives::num::math::{cosf, powf, sinf};
use pyreal_primitives::Vec3;

use crate::prefs::{Category, Prefs, SoundFeatures};
use crate::voices::{lowpass_share, Mixer, Source, VoiceId, VoiceSettings};
use crate::Listener;

/// How many sounds play at once.
pub const VOICES: usize = 64;
/// Inside this many metres a sound is at full gain and centred.
pub const NEAR: f32 = 5.0;
/// Beyond this many metres nothing is heard; the original's last audible distance.
pub const REACH: f32 = 96.0;
/// Where the fade to [`REACH`] begins.
const FADE_FROM: f32 = 72.0;
/// The amplitude's power of `NEAR / distance`: 9 dB a doubling.
const FALLOFF: f32 = 1.5;
/// How far a sound at the side moves the balance, of the whole way.
const SPREAD: f32 = 0.8;
/// The cutoff of a sound straight behind, Hz.
const BEHIND_CUTOFF: f32 = 5000.0;
/// The cutoff at [`REACH`], Hz, from the air's absorption.
const AIR_CUTOFF: f32 = 7000.0;
/// A sound straight behind is this much quieter.
const BEHIND_DIP: f32 = 0.2;
/// A new sound quieter than this (-66 dB) is not started.
const QUIET: f32 = 0.0005;
/// Seconds a loop takes to fade in or out.
const LOOP_FADE: f32 = 1.5;
/// The longest crossfade at a loop's seam, seconds.
const SEAM: f64 = 0.25;
/// A constant ambient sound loops when its recording is at least this share of its interval.
pub const LOOP_SHARE: f64 = 0.8;

/// What made a sound, for the room it is given and whether it may give way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// The interface: never gives way to a world sound.
    Interface,
    /// The player's own body: never gives way to a world sound.
    Own,
    /// Any other object, or a sound placed in the world.
    World,
    /// An ambient sound played once.
    Ambient,
    /// A constant ambient sound looping.
    Loop,
    /// The storm's thunder.
    Weather,
}

impl Class {
    const fn protected(self) -> bool {
        matches!(self, Self::Interface | Self::Own)
    }

    /// How much of the room's echo it takes.
    const fn echo(self) -> f32 {
        match self {
            Self::World | Self::Ambient => 1.0,
            Self::Own => 0.6,
            Self::Interface | Self::Loop | Self::Weather => 0.0,
        }
    }
}

/// The object a sound comes from, so it can follow it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Emitter {
    /// The object's id; `None` for a sound placed by itself.
    pub object: Option<u32>,
    /// The player's own body.
    pub own: bool,
}

impl Emitter {
    /// A sound of object `id`.
    #[must_use]
    pub const fn object(id: u32) -> Self {
        Self {
            object: Some(id),
            own: false,
        }
    }

    /// A sound of the player's own body, `id` when the body has its server id.
    #[must_use]
    pub const fn own(id: Option<u32>) -> Self {
        Self {
            object: id,
            own: true,
        }
    }
}

/// The space the listener stands in, which decides the echo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Room {
    /// Under the sky: no echo.
    #[default]
    Outdoors,
    /// A building's room, which sees the outside.
    Building,
    /// A dungeon's hall, which does not.
    Dungeon,
}

impl Room {
    /// The echo's send, its room size and its damping.
    const fn echo(self) -> (f32, f32, f32) {
        match self {
            Self::Outdoors => (0.0, 0.5, 0.5),
            Self::Building => (0.22, 0.5, 0.55),
            Self::Dungeon => (0.42, 0.84, 0.3),
        }
    }
}

/// What the stage did, in numbers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StageStats {
    /// Sounds started.
    pub started: u64,
    /// Sounds that found every voice busy with something louder or protected.
    pub dropped: u64,
    /// Voices that gave way to a louder sound.
    pub stolen: u64,
    /// The most voices playing at once.
    pub most: usize,
    /// Constant ambient sounds started as loops.
    pub loops: u64,
    /// Sounds too far off or too quiet to be heard, never started.
    pub unheard: u64,
}

/// What [`Stage::play`] is asked to play.
#[derive(Debug, Clone, Copy)]
pub struct Play {
    /// Where it is; `None` from the listener's centre.
    pub at: Option<Vec3>,
    /// Its own volume, before its category's.
    pub volume: f32,
    /// The slider it answers to; `None` for none.
    pub category: Option<Category>,
    pub class: Class,
    pub emitter: Option<u32>,
    /// The constant ambient sound it loops for.
    pub loop_key: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
struct Meta {
    play: Play,
    fade: f32,
    fade_to: f32,
    order: u64,
    audible: f32,
}

/// A voice's gains, low pass and send for where it stands now.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Shape {
    gains: [f32; 2],
    lowpass: f32,
    send: f32,
    audible: f32,
}

/// The enhanced sound's voices, at the device's rate.
#[derive(Debug)]
pub struct Stage {
    mixer: Mixer,
    meta: BTreeMap<VoiceId, Meta>,
    loops: BTreeMap<usize, VoiceId>,
    listener: Listener,
    features: SoundFeatures,
    room: Room,
    reverb: Reverb,
    scratch: Vec<f32>,
    send: Vec<f32>,
    order: u64,
    pub stats: StageStats,
}

impl Stage {
    /// A stage mixing at `rate` frames a second.
    #[must_use]
    pub fn new(rate: u32) -> Self {
        Self {
            mixer: Mixer::new(rate, VOICES).with_cubic(),
            meta: BTreeMap::new(),
            loops: BTreeMap::new(),
            listener: Listener::default(),
            features: SoundFeatures::Stereo,
            room: Room::Outdoors,
            reverb: Reverb::new(rate),
            scratch: Vec::new(),
            send: Vec::new(),
            order: 0,
            stats: StageStats::default(),
        }
    }

    /// The rate it mixes at.
    #[must_use]
    pub fn rate(&self) -> u32 {
        self.mixer.rate()
    }

    /// How many voices are playing.
    #[must_use]
    pub fn active(&self) -> usize {
        self.mixer.active()
    }

    /// The listener and the stereo/mono choice, for the next shapes worked out. A listener that
    /// is not at a real place keeps the last one.
    pub fn set_listener(&mut self, l: Listener, features: SoundFeatures) {
        if finite(l.pos) && l.heading.is_finite() {
            self.listener = l;
        }
        self.features = features;
    }

    /// The space the listener stands in.
    pub fn set_room(&mut self, room: Room) {
        self.room = room;
        let (_, size, damp) = room.echo();
        self.reverb.set(size, damp);
    }

    /// The room in force.
    #[must_use]
    pub fn room(&self) -> Room {
        self.room
    }

    /// Stop everything.
    pub fn stop_all(&mut self) {
        self.mixer.stop_all();
        self.meta.clear();
        self.loops.clear();
    }

    /// The objects whose sounds are playing, each once.
    #[must_use]
    pub fn emitters(&self) -> Vec<u32> {
        let mut v = Vec::new();
        self.emitters_into(&mut v);
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Add the object of every playing sound to `out`, unsorted and repeated, without allocating
    /// when `out` has room: the caller sorts it after letting go of the mixer.
    pub fn emitters_into(&self, out: &mut Vec<u32>) {
        out.extend(self.meta.values().filter_map(|m| m.play.emitter));
    }

    /// Object `id` stands at `at` now: its sounds follow it.
    pub fn move_emitter(&mut self, id: u32, at: Vec3) {
        if !finite(at) {
            return;
        }
        for m in self.meta.values_mut() {
            if m.play.emitter == Some(id) && m.play.at.is_some() {
                m.play.at = Some(at);
            }
        }
    }

    /// The player's body stands at `at` now: its placed sounds follow it.
    pub fn move_own(&mut self, at: Vec3) {
        if !finite(at) {
            return;
        }
        for m in self.meta.values_mut() {
            if m.play.class == Class::Own && m.play.at.is_some() {
                m.play.at = Some(at);
            }
        }
    }

    /// [`Self::move_emitter`] for each of `at` (sorted by object), then [`Self::move_own`], in
    /// one pass over the playing sounds.
    pub fn move_emitters(&mut self, at: &[(u32, Vec3)], own: Option<Vec3>) {
        let own = own.filter(|p| finite(*p));
        for m in self.meta.values_mut() {
            if m.play.at.is_none() {
                continue;
            }
            if let Some(id) = m.play.emitter {
                if let Ok(i) = at.binary_search_by_key(&id, |(e, _)| *e) {
                    if finite(at[i].1) {
                        m.play.at = Some(at[i].1);
                    }
                }
            }
            if let (Class::Own, Some(p)) = (m.play.class, own) {
                m.play.at = Some(p);
            }
        }
    }

    /// Start `source`; whether a voice took it.
    pub fn play(&mut self, source: &Source, play: Play, prefs: &Prefs) -> bool {
        // Nothing to play, or nowhere real to play it: no voice is asked to give way.
        if source.len_frames() == 0 || !play.volume.is_finite() || !play.at.is_none_or(finite) {
            self.stats.dropped += 1;
            return false;
        }
        self.forget_ended();
        let looping = play.loop_key.is_some();
        let fade = if looping { 0.0 } else { 1.0 };
        let mut meta = Meta {
            play,
            fade,
            fade_to: 1.0,
            order: self.order,
            audible: 0.0,
        };
        // A loop starts silent and fades in, so its loudness to come is what it is judged by.
        let judged = Meta { fade: 1.0, ..meta };
        let shape = self.shape(&judged, prefs);
        if shape.audible < QUIET {
            self.stats.unheard += 1;
            return false;
        }
        meta.audible = shape.audible;
        let start = self.shape(&meta, prefs);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a seam of at most a quarter second of frames.
        let crossfade = if looping {
            (f64::from(source.rate()) * SEAM).min(source.len_frames() as f64 / 4.0) as u32
        } else {
            0
        };
        let settings = VoiceSettings {
            gains: start.gains,
            lowpass: start.lowpass,
            send: start.send,
            glide: true,
            looping,
            crossfade,
            ..VoiceSettings::of(source)
        };
        let id = match self.mixer.start(source.clone(), settings) {
            Some(id) => id,
            None => {
                let Some(victim) = self.victim(play.class, shape.audible) else {
                    self.stats.dropped += 1;
                    return false;
                };
                self.stop(victim);
                self.stats.stolen += 1;
                match self.mixer.start(source.clone(), settings) {
                    Some(id) => id,
                    None => {
                        self.stats.dropped += 1;
                        return false;
                    }
                }
            }
        };
        self.order += 1;
        self.meta.insert(id, meta);
        if let Some(key) = play.loop_key {
            self.loops.insert(key, id);
            self.stats.loops += 1;
        }
        self.stats.started += 1;
        self.stats.most = self.stats.most.max(self.mixer.active());
        true
    }

    /// The voice to give way to a sound of `class` as loud as `audible`: the quietest that is not
    /// protected and is quieter, oldest first among equals; for a protected newcomer with only
    /// protected voices left, the oldest.
    fn victim(&self, class: Class, audible: f32) -> Option<VoiceId> {
        let open = self
            .meta
            .iter()
            .filter(|(_, m)| !m.play.class.protected())
            .min_by(|a, b| {
                a.1.audible
                    .total_cmp(&b.1.audible)
                    .then(a.1.order.cmp(&b.1.order))
            });
        match open {
            Some((id, m)) if m.audible < audible || class.protected() => Some(*id),
            Some(_) => None,
            None if class.protected() => self
                .meta
                .iter()
                .min_by_key(|(_, m)| m.order)
                .map(|(id, _)| *id),
            None => None,
        }
    }

    fn stop(&mut self, id: VoiceId) {
        self.mixer.stop(id);
        if let Some(m) = self.meta.remove(&id) {
            if let Some(key) = m.play.loop_key {
                self.loops.remove(&key);
            }
        }
    }

    fn forget_ended(&mut self) {
        let ended: Vec<VoiceId> = self
            .meta
            .keys()
            .filter(|id| !self.mixer.is_playing(**id))
            .copied()
            .collect();
        for id in ended {
            self.stop(id);
        }
    }

    /// The loop for constant ambient sound `key` plays at `volume` now; whether there is one.
    pub fn loop_volume(&mut self, key: usize, volume: f32) -> bool {
        let Some(id) = self.loops.get(&key) else {
            return false;
        };
        match self.meta.get_mut(id) {
            Some(m) => {
                m.play.volume = volume;
                m.fade_to = 1.0;
                true
            }
            None => false,
        }
    }

    /// Fade out every loop whose constant ambient sound `keep` no longer hears.
    pub fn retire_loops(&mut self, keep: impl Fn(usize) -> bool) {
        for (key, id) in &self.loops {
            if !keep(*key) {
                if let Some(m) = self.meta.get_mut(id) {
                    m.fade_to = 0.0;
                }
            }
        }
    }

    /// How many loops are playing.
    #[must_use]
    pub fn loops(&self) -> usize {
        self.loops.len()
    }

    /// Work out every voice's shape for the listener, the emitters and `prefs` as they stand,
    /// `dt` seconds after the last time; loops fade, and a loop faded out stops.
    pub fn update(&mut self, prefs: &Prefs, dt: f32) {
        self.forget_ended();
        let step = (dt / LOOP_FADE).max(0.0);
        let ids: Vec<VoiceId> = self.meta.keys().copied().collect();
        for id in ids {
            let Some(mut m) = self.meta.get(&id).copied() else {
                continue;
            };
            if m.fade < m.fade_to {
                m.fade = (m.fade + step).min(m.fade_to);
            } else if m.fade > m.fade_to {
                m.fade = (m.fade - step).max(m.fade_to);
            }
            if m.fade_to == 0.0 && m.fade == 0.0 {
                self.stop(id);
                continue;
            }
            let s = self.shape(&m, prefs);
            m.audible = s.audible;
            self.mixer.set_shape(id, s.gains, s.lowpass, s.send);
            self.meta.insert(id, m);
        }
    }

    fn shape(&self, m: &Meta, prefs: &Prefs) -> Shape {
        let category = m.play.category.map_or(1.0, |c| category_volume(prefs, c));
        let base = (m.play.volume * category * m.fade).max(0.0);
        let (send, _, _) = self.room.echo();
        let echo = send * m.play.class.echo();
        let Some(at) = m.play.at else {
            return Shape {
                gains: [base, base],
                lowpass: 1.0,
                send: echo * 0.5,
                audible: base,
            };
        };
        let l = self.listener.pos;
        let d = Vec3::new(at.x - l.x, at.y - l.y, at.z - l.z).magnitude();
        // How far off to the side, which places it: one overhead or underfoot is centred and
        // neither ahead nor behind however it drifts.
        let flat = Vec3::new(at.x - l.x, at.y - l.y, 0.0).magnitude();
        let azimuth =
            (crate::AudioSystem::compass_heading(l, at) - self.listener.heading).to_radians();
        let behind = (-cosf(azimuth)).max(0.0) * (flat / NEAR).min(1.0);
        let g = base * distance_gain(d) * (1.0 - BEHIND_DIP * behind);
        let [pl, pr] = pan(azimuth, flat, self.features);
        let cutoff = air_cutoff(d).min(20_000.0 + (BEHIND_CUTOFF - 20_000.0) * behind);
        Shape {
            gains: [g * pl, g * pr],
            lowpass: lowpass_share(cutoff, self.mixer.rate()),
            send: echo * (0.5 + 0.5 * (d / 30.0).min(1.0)),
            audible: g,
        }
    }

    /// Add the stage into a device's block of `channels` interleaved channels (a one-channel
    /// device gets the mean of the two sides). Muted, the voices go on and nothing is added.
    pub fn mix(&mut self, out: &mut [f32], channels: usize, muted: bool) {
        let channels = channels.max(1);
        let frames = out.len() / channels;
        self.scratch.clear();
        self.scratch.resize(frames * 2, 0.0);
        self.send.clear();
        self.send.resize(frames * 2, 0.0);
        self.mixer.mix_with_send(&mut self.scratch, &mut self.send);
        self.reverb.process(&self.send, &mut self.scratch);
        if muted {
            return;
        }
        for f in 0..frames {
            let (l, r) = (self.scratch[2 * f], self.scratch[2 * f + 1]);
            if channels == 1 {
                out[f] += (l + r) * 0.5;
            } else {
                out[f * channels] += l;
                out[f * channels + 1] += r;
            }
        }
    }
}

/// Whether every part of `v` is a number.
fn finite(v: Vec3) -> bool {
    v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
}

/// The volume a category's slider gives, each applied once; the interface's is its own.
#[must_use]
pub fn category_volume(prefs: &Prefs, c: Category) -> f32 {
    match c {
        Category::Effect => prefs.effect_volume,
        Category::Ambient => prefs.ambient_volume,
        Category::Interface => prefs.interface_volume,
    }
}

/// The gain at `d` metres: 1 inside [`NEAR`], 9 dB down a doubling beyond it, faded to nothing by
/// [`REACH`].
#[must_use]
pub fn distance_gain(d: f32) -> f32 {
    if d >= REACH {
        return 0.0;
    }
    let g = if d <= NEAR {
        1.0
    } else {
        powf(NEAR / d, FALLOFF)
    };
    if d > FADE_FROM {
        let t = (REACH - d) / (REACH - FADE_FROM);
        g * t * t * (3.0 - 2.0 * t)
    } else {
        g
    }
}

/// The left and right gains for a sound at `azimuth` radians (clockwise from straight ahead)
/// and `d` metres: an equal-power balance, 1 on both sides at the centre, drawn toward the centre
/// within two metres. Mono is centred.
#[must_use]
pub fn pan(azimuth: f32, d: f32, features: SoundFeatures) -> [f32; 2] {
    if features == SoundFeatures::Mono {
        return [1.0, 1.0];
    }
    let p = sinf(azimuth) * SPREAD * (d / 2.0).min(1.0);
    let theta = (p + 1.0) * std::f32::consts::FRAC_PI_4;
    let k = std::f32::consts::SQRT_2;
    [cosf(theta) * k, sinf(theta) * k]
}

/// The cutoff the air leaves a sound `d` metres away, Hz.
#[must_use]
pub fn air_cutoff(d: f32) -> f32 {
    let t = (d / REACH).clamp(0.0, 1.0);
    20_000.0 * powf(AIR_CUTOFF / 20_000.0, t)
}

/// A Freeverb reverberation (Jezar's eight combs and four all-passes a side), its delays scaled
/// to the rate and its memory made once.
#[derive(Debug)]
struct Reverb {
    combs: [Vec<Comb>; 2],
    passes: [Vec<Pass>; 2],
    feedback: f32,
    damp: f32,
    /// Frames since anything was sent, to stop working once the tail has died.
    quiet: usize,
    tail: usize,
}

#[derive(Debug)]
struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32,
}

#[derive(Debug)]
struct Pass {
    buf: Vec<f32>,
    i: usize,
}

const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const PASSES: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;
const REVERB_IN: f32 = 0.015;
const REVERB_OUT: f32 = 3.0;

impl Reverb {
    fn new(rate: u32) -> Self {
        let scale = f64::from(rate.max(1)) / 44_100.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: delay lengths of a few thousand frames.
        let len = |n: usize| ((n as f64 * scale) as usize).max(1);
        let side = |spread: usize| -> (Vec<Comb>, Vec<Pass>) {
            (
                COMBS
                    .iter()
                    .map(|n| Comb {
                        buf: vec![0.0; len(n + spread)],
                        i: 0,
                        store: 0.0,
                    })
                    .collect(),
                PASSES
                    .iter()
                    .map(|n| Pass {
                        buf: vec![0.0; len(n + spread)],
                        i: 0,
                    })
                    .collect(),
            )
        };
        let (cl, pl) = side(0);
        let (cr, pr) = side(STEREO_SPREAD);
        let tail = len(COMBS[7] + STEREO_SPREAD) * 40;
        let mut r = Self {
            combs: [cl, cr],
            passes: [pl, pr],
            feedback: 0.84,
            damp: 0.2,
            quiet: tail,
            tail,
        };
        r.set(0.5, 0.5);
        r
    }

    /// The room's size and damping, each 0 to 1, as Freeverb takes them.
    fn set(&mut self, size: f32, damp: f32) {
        self.feedback = size.clamp(0.0, 1.0) * 0.28 + 0.7;
        self.damp = damp.clamp(0.0, 1.0) * 0.4;
    }

    /// Empty every delay line.
    fn clear(&mut self) {
        for c in self.combs.iter_mut().flatten() {
            c.buf.fill(0.0);
            c.store = 0.0;
        }
        for p in self.passes.iter_mut().flatten() {
            p.buf.fill(0.0);
        }
    }

    /// Add the echo of `send` into `out`, both interleaved stereo. An echo that has picked up a
    /// sample that is not a number is emptied rather than kept.
    fn process(&mut self, send: &[f32], out: &mut [f32]) {
        let frames = out.len() / 2;
        if send.iter().any(|s| *s != 0.0) {
            self.quiet = 0;
        } else if self.quiet >= self.tail {
            return;
        } else {
            self.quiet += frames;
        }
        let (fb, damp) = (self.feedback, self.damp);
        for f in 0..frames {
            let input = (send[2 * f] + send[2 * f + 1]) * REVERB_IN;
            for side in 0..2 {
                let mut acc = 0.0;
                for c in &mut self.combs[side] {
                    let y = c.buf[c.i];
                    c.store = y * (1.0 - damp) + c.store * damp;
                    c.buf[c.i] = input + c.store * fb;
                    c.i = (c.i + 1) % c.buf.len();
                    acc += y;
                }
                for p in &mut self.passes[side] {
                    let b = p.buf[p.i];
                    let y = b - acc;
                    p.buf[p.i] = acc + b * 0.5;
                    p.i = (p.i + 1) % p.buf.len();
                    acc = y;
                }
                if !acc.is_finite() {
                    self.clear();
                    return;
                }
                out[2 * f + side] += acc * REVERB_OUT;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone() -> Source {
        Source::mono(vec![0.5; 4800].into(), 48_000)
    }

    fn at(x: f32, y: f32) -> Play {
        Play {
            at: Some(Vec3::new(x, y, 0.0)),
            volume: 1.0,
            category: Some(Category::Effect),
            class: Class::World,
            emitter: None,
            loop_key: None,
        }
    }

    /// The falloff is 9 dB a doubling past five metres and nothing at all by the reach.
    #[test]
    fn the_falloff_is_nine_decibels_a_doubling_and_ends_at_the_reach() {
        assert_eq!(distance_gain(3.0), 1.0);
        let db = |g: f32| {
            pyreal_primitives::num::math::log(f64::from(g)) * 20.0 / std::f64::consts::LN_10
        };
        assert!((db(distance_gain(10.0)) + 9.03).abs() < 0.05);
        assert!((db(distance_gain(20.0)) + 18.06).abs() < 0.05);
        assert_eq!(distance_gain(REACH), 0.0);
        assert!(distance_gain(90.0) < distance_gain(70.0) * 0.2);
    }

    /// The balance keeps its power: full on both sides at the centre, louder on the near side
    /// and never silent on the far one; mono stays centred.
    #[test]
    fn the_pan_keeps_its_power_and_leans_to_the_near_side() {
        let [l, r] = pan(0.0, 10.0, SoundFeatures::Stereo);
        assert!((l - 1.0).abs() < 1e-6 && (r - 1.0).abs() < 1e-6);
        let [l, r] = pan(std::f32::consts::FRAC_PI_2, 10.0, SoundFeatures::Stereo);
        assert!(r > 1.3 && l > 0.2 && l < 0.5, "{l} {r}");
        assert!((l * l + r * r - 2.0).abs() < 1e-5);
        assert_eq!(
            pan(std::f32::consts::FRAC_PI_2, 10.0, SoundFeatures::Mono),
            [1.0, 1.0]
        );
    }

    /// A sound to the east of a listener facing north is on the right; behind it is duller.
    #[test]
    fn a_sound_is_placed_by_its_bearing_and_dulled_behind() {
        let mut s = Stage::new(48_000);
        s.set_listener(
            Listener {
                pos: Vec3::ZERO,
                heading: 0.0,
            },
            SoundFeatures::Stereo,
        );
        let p = Prefs::default();
        let meta = |play| Meta {
            play,
            fade: 1.0,
            fade_to: 1.0,
            order: 0,
            audible: 0.0,
        };
        let east = s.shape(&meta(at(10.0, 0.0)), &p);
        assert!(east.gains[1] > east.gains[0]);
        let ahead = s.shape(&meta(at(0.0, 10.0)), &p);
        let behind = s.shape(&meta(at(0.0, -10.0)), &p);
        assert!(behind.lowpass < ahead.lowpass);
        assert!(behind.audible < ahead.audible);
    }

    /// With every voice busy the quietest world sound gives way to a louder one, a quieter
    /// newcomer is dropped and counted, and the interface never gives way to the world.
    #[test]
    fn a_full_stage_gives_the_quietest_way_and_keeps_the_interface() {
        let mut s = Stage::new(48_000);
        let p = Prefs::default();
        let src = tone();
        // One interface sound, then far world sounds until every voice is busy.
        let ui = Play {
            at: None,
            category: Some(Category::Interface),
            class: Class::Interface,
            ..at(0.0, 0.0)
        };
        assert!(s.play(&src, ui, &p));
        for _ in 1..VOICES {
            assert!(s.play(&src, at(60.0, 0.0), &p));
        }
        assert_eq!(s.active(), VOICES);
        // Farther still: dropped.
        assert!(!s.play(&src, at(80.0, 0.0), &p));
        assert_eq!(s.stats.dropped, 1);
        // Near: a far one gives way.
        assert!(s.play(&src, at(1.0, 0.0), &p));
        assert_eq!(s.stats.stolen, 1);
        // The interface sound is still there.
        assert!(s.meta.values().any(|m| m.play.class == Class::Interface));
    }

    /// A sound of an object follows it as it moves.
    #[test]
    fn a_sound_follows_the_object_that_makes_it() {
        let mut s = Stage::new(48_000);
        let p = Prefs::default();
        let play = Play {
            emitter: Some(7),
            ..at(1.0, 0.0)
        };
        assert!(s.play(&tone(), play, &p));
        assert_eq!(s.emitters(), vec![7]);
        s.move_emitter(7, Vec3::new(50.0, 0.0, 0.0));
        s.update(&p, 0.0);
        let m = s.meta.values().next().expect("playing");
        assert_eq!(m.play.at, Some(Vec3::new(50.0, 0.0, 0.0)));
        assert!(m.audible < 0.05);
    }

    /// A loop fades in, takes its new volume, and fades out and stops once retired.
    #[test]
    fn a_loop_fades_in_and_out() {
        let mut s = Stage::new(48_000);
        let p = Prefs::default();
        let play = Play {
            at: None,
            category: Some(Category::Ambient),
            class: Class::Loop,
            loop_key: Some(3),
            ..at(0.0, 0.0)
        };
        assert!(s.play(&tone(), play, &p));
        assert_eq!(s.loops(), 1);
        assert!(s.loop_volume(3, 0.5));
        s.update(&p, LOOP_FADE);
        let m = *s.meta.values().next().expect("playing");
        assert!((m.audible - 0.5).abs() < 1e-6);
        s.retire_loops(|_| false);
        s.update(&p, LOOP_FADE);
        assert_eq!(s.loops(), 0);
        assert_eq!(s.active(), 0);
    }

    /// The interface follows its own slider, each slider once.
    #[test]
    fn each_slider_is_applied_once_and_the_interface_has_its_own() {
        let p = Prefs {
            effect_volume: 0.5,
            ambient_volume: 0.5,
            interface_volume: 0.25,
            ..Prefs::default()
        };
        assert_eq!(category_volume(&p, Category::Ambient), 0.5);
        assert_eq!(category_volume(&p, Category::Interface), 0.25);
    }

    /// Indoors the world's sounds ring on after they stop; outdoors they do not.
    #[test]
    fn a_dungeon_echoes_and_the_open_air_does_not() {
        let click = Source::mono(
            {
                let mut v = vec![0.0; 4800];
                v[0] = 1.0;
                v
            }
            .into(),
            48_000,
        );
        let tail = |room| {
            let mut s = Stage::new(48_000);
            s.set_room(room);
            let p = Prefs::default();
            assert!(s.play(&click, at(3.0, 0.0), &p));
            let mut out = vec![0.0f32; 2 * 9600];
            s.mix(&mut out, 2, false);
            out[2 * 4800..].iter().map(|x| x.abs()).fold(0.0, f32::max)
        };
        assert_eq!(tail(Room::Outdoors), 0.0);
        assert!(tail(Room::Dungeon) > 1e-4);
    }

    /// A sound that decodes to nothing is refused at once; it never takes a playing voice's
    /// place.
    #[test]
    fn an_empty_source_never_evicts_a_voice() {
        let mut s = Stage::new(48_000);
        let p = Prefs::default();
        for _ in 0..VOICES {
            assert!(s.play(&tone(), at(60.0, 0.0), &p));
        }
        let empty = Source::mono(Vec::new().into(), 48_000);
        assert!(!s.play(&empty, at(1.0, 0.0), &p));
        assert_eq!(s.stats.stolen, 0, "a voice gave way to nothing");
        assert_eq!(s.active(), VOICES);
    }

    /// A position or a volume that is not a number never reaches the mix, and the listener
    /// keeps its last good place.
    #[test]
    fn a_non_finite_position_or_gain_is_refused() {
        let mut s = Stage::new(48_000);
        let p = Prefs::default();
        s.set_room(Room::Dungeon);
        assert!(!s.play(&tone(), at(f32::NAN, 0.0), &p));
        let loud = Play {
            volume: f32::INFINITY,
            ..at(1.0, 0.0)
        };
        assert!(!s.play(&tone(), loud, &p));
        let play = Play {
            emitter: Some(7),
            ..at(1.0, 0.0)
        };
        assert!(s.play(&tone(), play, &p));
        s.move_emitter(7, Vec3::new(f32::NAN, 0.0, 0.0));
        s.move_own(Vec3::new(0.0, f32::INFINITY, 0.0));
        let good = s.listener;
        s.set_listener(
            Listener {
                pos: Vec3::new(f32::NAN, 0.0, 0.0),
                heading: 0.0,
            },
            SoundFeatures::Stereo,
        );
        assert_eq!(s.listener, good, "the listener moved to nowhere");
        s.set_listener(
            Listener {
                pos: Vec3::ZERO,
                heading: f32::NAN,
            },
            SoundFeatures::Stereo,
        );
        assert_eq!(s.listener, good, "the listener turned to nowhere");
        s.update(&p, 0.01);
        let mut out = vec![0.0f32; 4800 * 2];
        s.mix(&mut out, 2, false);
        assert!(out.iter().all(|v| v.is_finite()));
        assert!(out.iter().any(|v| *v != 0.0), "the good sound still plays");
    }

    /// An echo that picks up a sample that is not a number is cleared, not kept for the session.
    #[test]
    fn a_reverb_that_picks_up_nan_is_cleared() {
        let mut r = Reverb::new(48_000);
        let mut send = vec![0.1f32; 256 * 2];
        send[10] = f32::NAN;
        let mut out = vec![0.0f32; 256 * 2];
        r.process(&send, &mut out);
        let send = vec![0.1f32; 4800 * 2];
        let mut out = vec![0.0f32; 4800 * 2];
        r.process(&send, &mut out);
        assert!(
            out.iter().all(|v| v.is_finite()),
            "the echo still holds NaN"
        );
        assert!(
            out.iter().any(|v| *v != 0.0),
            "the echo went silent for good"
        );
    }

    /// The stage under load: a device thread mixes a block every 5.3 ms, taking the stage only if
    /// it is free (a block it cannot take is an underrun), while the frame loop follows 64
    /// moving emitters back to back. Run with `--ignored --nocapture` under the perf lock.
    #[test]
    #[ignore = "a timing run"]
    fn following_emitters_rarely_keeps_the_device_waiting() {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        use std::sync::{Arc, Mutex};

        let busy = |new: bool| -> (u64, u64, u64) {
            let mut s = Stage::new(48_000);
            let p = Prefs::default();
            let long = Source::mono(vec![0.25; 48_000 * 30].into(), 48_000);
            for id in 0..VOICES {
                #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
                let play = Play {
                    emitter: Some(1000 + id as u32),
                    ..at(2.0 + id as f32 * 0.5, 0.0)
                };
                assert!(s.play(&long, play, &p));
            }
            let stage = Arc::new(Mutex::new(s));
            let done = Arc::new(AtomicBool::new(false));
            let (blocks, underruns) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
            let device = {
                let (stage, done) = (Arc::clone(&stage), Arc::clone(&done));
                let (blocks, underruns) = (Arc::clone(&blocks), Arc::clone(&underruns));
                std::thread::spawn(move || {
                    let mut out = vec![0.0f32; 256 * 2];
                    let period = std::time::Duration::from_micros(5_333);
                    let mut next = std::time::Instant::now();
                    while !done.load(Ordering::Relaxed) {
                        out.fill(0.0);
                        match stage.try_lock() {
                            Ok(mut s) => s.mix(&mut out, 2, false),
                            Err(_) => {
                                underruns.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        blocks.fetch_add(1, Ordering::Relaxed);
                        next += period;
                        std::thread::sleep(
                            next.saturating_duration_since(std::time::Instant::now()),
                        );
                    }
                })
            };
            let start = std::time::Instant::now();
            let mut frames = 0u64;
            let mut t = 0.0f32;
            while start.elapsed() < std::time::Duration::from_secs(4) {
                t += 0.01;
                let place = |id: u32| {
                    #[allow(clippy::cast_precision_loss)]
                    Some(Vec3::new(2.0 + (id - 1000) as f32 * 0.5, sinf(t), 0.0))
                };
                if new {
                    // `Audio::follow_emitters` as it is now.
                    let mut ids = Vec::with_capacity(VOICES);
                    stage.lock().unwrap().emitters_into(&mut ids);
                    ids.sort_unstable();
                    ids.dedup();
                    let at: Vec<(u32, Vec3)> = ids
                        .into_iter()
                        .filter_map(|id| place(id).map(|p| (id, p)))
                        .collect();
                    stage.lock().unwrap().move_emitters(&at, Some(Vec3::ZERO));
                } else {
                    // As it was: the list made and sorted under the lock, then one move a voice.
                    let ids = stage.lock().unwrap().emitters();
                    let at: Vec<(u32, Vec3)> = ids
                        .into_iter()
                        .filter_map(|id| place(id).map(|p| (id, p)))
                        .collect();
                    let mut s = stage.lock().unwrap();
                    for (id, p) in at {
                        s.move_emitter(id, p);
                    }
                    s.move_own(Vec3::ZERO);
                }
                frames += 1;
            }
            done.store(true, Ordering::Relaxed);
            device.join().unwrap();
            (
                frames,
                blocks.load(Ordering::Relaxed),
                underruns.load(Ordering::Relaxed),
            )
        };
        for (name, new) in [("before", false), ("after", true)] {
            let (frames, blocks, underruns) = busy(new);
            #[allow(clippy::cast_precision_loss)]
            let share = underruns as f64 / blocks.max(1) as f64 * 100.0;
            println!(
                "{name}: {frames} frames, {blocks} blocks, {underruns} underruns ({share:.2}%)"
            );
        }
    }

    /// A sound just above or below the listener is placed by how far off to the side it is, not
    /// by how far away it is: centimetres of drift neither swing it across nor flip it behind.
    #[test]
    fn a_sound_overhead_or_underfoot_holds_its_place() {
        let mut s = Stage::new(48_000);
        s.set_listener(
            Listener {
                pos: Vec3::ZERO,
                heading: 0.0,
            },
            SoundFeatures::Stereo,
        );
        let p = Prefs::default();
        let meta = |play| Meta {
            play,
            fade: 1.0,
            fade_to: 1.0,
            order: 0,
            audible: 0.0,
        };
        let under = |x: f32, y: f32| {
            s.shape(
                &meta(Play {
                    at: Some(Vec3::new(x, y, -1.7)),
                    ..at(0.0, 0.0)
                }),
                &p,
            )
        };
        let (left, right) = (under(-0.03, 0.0), under(0.03, 0.0));
        let balance = |s: &Shape| s.gains[1] / (s.gains[0] + s.gains[1]);
        assert!(
            (balance(&left) - balance(&right)).abs() < 0.05,
            "six centimetres swung it: {:?} {:?}",
            left.gains,
            right.gains
        );
        let (ahead, behind) = (under(0.0, 0.03), under(0.0, -0.03));
        assert!(
            (ahead.lowpass - behind.lowpass).abs() < 0.05,
            "six centimetres put it behind: {} {}",
            ahead.lowpass,
            behind.lowpass
        );
    }
}
