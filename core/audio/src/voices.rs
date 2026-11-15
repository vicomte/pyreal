//! A general mixer: any output rate, any number of voices, looping, and each voice's gains and
//! playback rate changed while it plays.
//!
//! The first game's client has its own pool ([`crate::mixer`]): sixteen voices, whole samples at
//! one rate, nothing looping, nothing changed once started. This is the mixer a client with other
//! rules builds on, and it leaves that pool as it is. The enhanced sound ([`crate::stage`]) is
//! built on it.
//!
//! A [`Source`] is interleaved `f32` (one channel or two) at its own rate, decoded once and shared
//! between the voices that play it. A [`Stream`] is a sound made as it plays (a synthesizer): the
//! voice reads its frames as it needs them, and it ends when the stream says it has. A voice reads
//! its source at its own rate: `rate / mixer rate` source frames for every output frame,
//! interpolating linearly between neighbouring frames, or through four with
//! [`Mixer::with_cubic`]. A source at the mixer's rate played at that rate therefore comes out
//! sample for sample, and a voice's pitch (or a Doppler shift) is only its rate. A voice's two
//! gains multiply its left and right channels; changing them, or the rate, takes effect from the
//! next block mixed, with no ramp unless the voice glides.
//!
//! Each voice may also pass through a one-pole low pass, send a share of itself to a second bus
//! (an echo's input, [`Mixer::mix_with_send`]), and crossfade its loop's seam. All three are off
//! in [`VoiceSettings::of`].

use std::sync::Arc;

/// A sound ready to mix: interleaved frames of one or two channels at `rate`.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    frames: Arc<[f32]>,
    rate: u32,
    channels: u8,
}

impl Source {
    /// A source of interleaved stereo `frames` at `rate` (a trailing half frame is dropped).
    #[must_use]
    pub fn new(frames: Arc<[f32]>, rate: u32) -> Self {
        if frames.len().is_multiple_of(2) {
            Self {
                frames,
                rate,
                channels: 2,
            }
        } else {
            let even: Arc<[f32]> = frames[..frames.len() - 1].into();
            Self {
                frames: even,
                rate,
                channels: 2,
            }
        }
    }

    /// A one-channel source, heard the same on both sides, kept at half a stereo one's size.
    #[must_use]
    pub fn mono(samples: Arc<[f32]>, rate: u32) -> Self {
        Self {
            frames: samples,
            rate,
            channels: 1,
        }
    }

    /// A source from interleaved samples of `channels` channels: one channel goes to both sides,
    /// two keep their sides, and of more only the first two are kept. A trailing partial frame is
    /// dropped.
    #[must_use]
    pub fn from_interleaved(samples: &[f32], channels: u16, rate: u32) -> Self {
        let c = usize::from(channels.max(1));
        let n = samples.len() / c;
        let mut out = Vec::with_capacity(n * 2);
        for f in samples.chunks_exact(c) {
            let l = f[0];
            let r = if c == 1 { l } else { f[1] };
            out.push(l);
            out.push(r);
        }
        Self {
            frames: out.into(),
            rate,
            channels: 2,
        }
    }

    /// The interleaved frames, [`Self::channels`] samples each.
    #[must_use]
    pub fn frames(&self) -> &[f32] {
        &self.frames
    }

    /// One channel or two.
    #[must_use]
    pub fn channels(&self) -> u16 {
        u16::from(self.channels)
    }

    /// The rate the frames were recorded at, in frames a second.
    #[must_use]
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// How many frames it holds.
    #[must_use]
    pub fn len_frames(&self) -> usize {
        self.frames.len() / usize::from(self.channels.max(1))
    }

    /// Frame `i` as its left and right samples; a one-channel frame is both.
    #[must_use]
    pub fn frame(&self, i: usize) -> (f32, f32) {
        if self.channels == 1 {
            let s = self.frames[i];
            (s, s)
        } else {
            (self.frames[2 * i], self.frames[2 * i + 1])
        }
    }

    /// Its length in seconds at its own rate (0 for a rate of 0).
    #[must_use]
    pub fn seconds(&self) -> f64 {
        if self.rate == 0 {
            return 0.0;
        }
        let n = u32::try_from(self.len_frames()).unwrap_or(u32::MAX);
        f64::from(n) / f64::from(self.rate)
    }
}

/// A sound made as it plays: interleaved stereo frames at its own rate, read in order.
pub trait Stream: Send + std::fmt::Debug {
    /// The rate its frames are made at, in frames a second.
    fn rate(&self) -> u32;

    /// Write its next frames into `out` (interleaved stereo); how many frames it wrote. Fewer than
    /// `out` holds means it has ended.
    fn read(&mut self, out: &mut [f32]) -> usize;
}

/// Frames a stream's voice reads at a time.
const STREAM_CHUNK: usize = 256;

/// A stream as a voice plays it: the frames read and not yet passed.
#[derive(Debug)]
struct Feed {
    stream: Box<dyn Stream>,
    /// Interleaved frames from `base` on.
    buf: Vec<f32>,
    base: u64,
    ended: bool,
}

impl Feed {
    /// Make frames `[from, to)` readable (as far as the stream goes); drop those before `from`.
    fn cover(&mut self, from: u64, to: u64) {
        if from > self.base {
            let drop = usize::try_from(from - self.base)
                .unwrap_or(usize::MAX)
                .min(self.buf.len() / 2);
            self.buf.drain(..drop * 2);
            self.base += drop as u64;
        }
        while !self.ended && self.base + (self.buf.len() / 2) as u64 <= to {
            let at = self.buf.len();
            self.buf.resize(at + STREAM_CHUNK * 2, 0.0);
            let got = self.stream.read(&mut self.buf[at..]).min(STREAM_CHUNK);
            self.buf.truncate(at + got * 2);
            if got < STREAM_CHUNK {
                self.ended = true;
            }
        }
    }

    fn frame(&self, i: u64) -> Option<(f32, f32)> {
        let k = usize::try_from(i.checked_sub(self.base)?).ok()?;
        Some((*self.buf.get(2 * k)?, *self.buf.get(2 * k + 1)?))
    }
}

/// A playing voice, as [`Mixer::start`] named it. Never reused for another voice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VoiceId(u64);

/// How a voice plays: its left and right gains, the rate its source is read at (frames a second;
/// the source's own rate plays it as recorded), whether it starts again from the beginning
/// when it reaches its end, and the three effects [`VoiceSettings::of`] leaves off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceSettings {
    pub gains: [f32; 2],
    pub rate: f64,
    pub looping: bool,
    /// The one-pole low pass: the share of the way each output frame moves toward its input.
    /// 1.0 passes the voice untouched; [`lowpass_share`] turns a cutoff into one.
    pub lowpass: f32,
    /// The share of the voice also added to the send bus, after its gains.
    pub send: f32,
    /// Changed gains are reached across the next block mixed, not at its first frame.
    pub glide: bool,
    /// Frames of a looping voice's end crossfaded into its start at the seam; 0 wraps at once.
    pub crossfade: u32,
}

impl VoiceSettings {
    /// These settings with a gain or send that is not a number made silent, such a low pass made
    /// open and such a read rate the source's `own_rate`, so nothing that is not a number reaches
    /// the mix.
    #[must_use]
    pub fn sanitised(self, own_rate: f64) -> Self {
        let real = |v: f32| if v.is_finite() { v } else { 0.0 };
        Self {
            rate: if self.rate.is_finite() {
                self.rate.max(0.0)
            } else {
                own_rate
            },
            gains: self.gains.map(real),
            lowpass: if self.lowpass.is_finite() {
                self.lowpass.clamp(0.0, 1.0)
            } else {
                1.0
            },
            send: real(self.send).max(0.0),
            ..self
        }
    }

    /// `source` as recorded, at full gain, once.
    #[must_use]
    pub fn of(source: &Source) -> Self {
        Self {
            gains: [1.0, 1.0],
            rate: f64::from(source.rate),
            looping: false,
            lowpass: 1.0,
            send: 0.0,
            glide: false,
            crossfade: 0,
        }
    }
}

/// The low-pass share for a cutoff of `cutoff` Hz at `rate` frames a second: 1.0 (no filter) at
/// or above the rate's half.
#[must_use]
pub fn lowpass_share(cutoff: f32, rate: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // a sample rate
    let rate = rate.max(1) as f32;
    if cutoff >= rate * 0.5 {
        return 1.0;
    }
    1.0 - pyreal_primitives::num::math::expf(-std::f32::consts::TAU * cutoff.max(1.0) / rate)
}

#[derive(Debug)]
struct Voice {
    id: VoiceId,
    source: Source,
    /// What it plays when it plays a stream (its `source` then empty, at the stream's rate).
    feed: Option<Feed>,
    /// Where it reads, in source frames.
    pos: f64,
    settings: VoiceSettings,
    /// The gains in force at the end of the last block, which a gliding voice moves from.
    current: [f32; 2],
    /// The low pass's last output on each side.
    held: [f32; 2],
}

/// The mixer: a fixed number of voices mixed into interleaved stereo at one output rate.
#[derive(Debug)]
pub struct Mixer {
    rate: u32,
    voices: Vec<Option<Voice>>,
    next: u64,
    cubic: bool,
}

impl Mixer {
    /// A mixer at `rate` output frames a second with room for `voices` voices.
    #[must_use]
    pub fn new(rate: u32, voices: usize) -> Self {
        Self {
            rate: rate.max(1),
            voices: (0..voices).map(|_| None).collect(),
            next: 1,
            cubic: false,
        }
    }

    /// The same mixer reading sources through four neighbouring frames (a Hermite curve), which
    /// keeps more of a source's top end when it is played above its own rate.
    #[must_use]
    pub fn with_cubic(mut self) -> Self {
        self.cubic = true;
        self
    }

    /// The output rate.
    #[must_use]
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// How many voices it has room for.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.voices.len()
    }

    /// How many voices are playing.
    #[must_use]
    pub fn active(&self) -> usize {
        self.voices.iter().flatten().count()
    }

    /// The playing voices' ids, in slot order.
    #[must_use]
    pub fn playing(&self) -> Vec<VoiceId> {
        self.voices.iter().flatten().map(|v| v.id).collect()
    }

    fn take_slot(&mut self) -> Option<(usize, VoiceId)> {
        let slot = self.voices.iter().position(Option::is_none)?;
        let id = VoiceId(self.next);
        self.next += 1;
        Some((slot, id))
    }

    /// Start `source` with `settings` in the first free voice; `None` when every voice is busy
    /// (nothing is stopped to make room: which sound gives way is the caller's rule) or the source
    /// is empty.
    pub fn start(&mut self, source: Source, settings: VoiceSettings) -> Option<VoiceId> {
        if source.len_frames() == 0 {
            return None;
        }
        let settings = settings.sanitised(f64::from(source.rate));
        let (slot, id) = self.take_slot()?;
        self.voices[slot] = Some(Voice {
            id,
            source,
            feed: None,
            pos: 0.0,
            current: settings.gains,
            held: [0.0; 2],
            settings,
        });
        Some(id)
    }

    /// Start a stream with `settings` in the first free voice (its `looping` is ignored: a stream
    /// ends when it says so); `None` when every voice is busy.
    pub fn start_stream(
        &mut self,
        stream: Box<dyn Stream>,
        settings: VoiceSettings,
    ) -> Option<VoiceId> {
        let (slot, id) = self.take_slot()?;
        let rate = stream.rate();
        let settings = settings.sanitised(f64::from(rate));
        self.voices[slot] = Some(Voice {
            id,
            source: Source::new(Arc::from(Vec::new()), rate),
            feed: Some(Feed {
                stream,
                buf: Vec::new(),
                base: 0,
                ended: false,
            }),
            pos: 0.0,
            current: settings.gains,
            held: [0.0; 2],
            settings: VoiceSettings {
                looping: false,
                ..settings
            },
        });
        Some(id)
    }

    fn voice_mut(&mut self, id: VoiceId) -> Option<&mut Voice> {
        self.voices.iter_mut().flatten().find(|v| v.id == id)
    }

    fn voice(&self, id: VoiceId) -> Option<&Voice> {
        self.voices.iter().flatten().find(|v| v.id == id)
    }

    /// Stop a voice; whether it was playing.
    pub fn stop(&mut self, id: VoiceId) -> bool {
        for v in &mut self.voices {
            if v.as_ref().is_some_and(|x| x.id == id) {
                *v = None;
                return true;
            }
        }
        false
    }

    /// Stop every voice.
    pub fn stop_all(&mut self) {
        for v in &mut self.voices {
            *v = None;
        }
    }

    /// Whether a voice is still playing (a voice that reached its end without looping is not).
    #[must_use]
    pub fn is_playing(&self, id: VoiceId) -> bool {
        self.voice(id).is_some()
    }

    /// A playing voice's settings.
    #[must_use]
    pub fn settings(&self, id: VoiceId) -> Option<VoiceSettings> {
        self.voice(id).map(|v| v.settings)
    }

    /// Where a playing voice reads, in source frames.
    #[must_use]
    pub fn position(&self, id: VoiceId) -> Option<f64> {
        self.voice(id).map(|v| v.pos)
    }

    /// The source a playing voice reads (empty for a stream).
    #[must_use]
    pub fn source(&self, id: VoiceId) -> Option<&Source> {
        self.voice(id).map(|v| &v.source)
    }

    /// Change a playing voice's gains; whether it is playing.
    pub fn set_gains(&mut self, id: VoiceId, gains: [f32; 2]) -> bool {
        self.voice_mut(id)
            .map(|v| v.settings.gains = gains)
            .is_some()
    }

    /// Change the rate a playing voice reads its source at; whether it is playing.
    /// A rate that is not a number is refused and the voice keeps its own.
    pub fn set_rate(&mut self, id: VoiceId, rate: f64) -> bool {
        self.voice_mut(id)
            .map(|v| {
                if rate.is_finite() {
                    v.settings.rate = rate.max(0.0);
                }
            })
            .is_some()
    }

    /// Change whether a playing voice loops; whether it is playing.
    pub fn set_looping(&mut self, id: VoiceId, looping: bool) -> bool {
        self.voice_mut(id)
            .map(|v| v.settings.looping = looping)
            .is_some()
    }

    /// Change a playing voice's gains, low pass and send together; whether it is playing.
    pub fn set_shape(&mut self, id: VoiceId, gains: [f32; 2], lowpass: f32, send: f32) -> bool {
        self.voice_mut(id)
            .map(|v| {
                v.settings = VoiceSettings {
                    gains,
                    lowpass,
                    send,
                    ..v.settings
                }
                .sanitised(f64::from(v.source.rate));
            })
            .is_some()
    }

    /// Add every playing voice into `out`, interleaved stereo at the output rate. `out` is not
    /// cleared first. A voice that reaches its end stops, unless it loops.
    pub fn mix(&mut self, out: &mut [f32]) {
        self.mix_into(out, None);
    }

    /// [`Self::mix`], each voice's send share also added into `send` (as long as `out`).
    pub fn mix_with_send(&mut self, out: &mut [f32], send: &mut [f32]) {
        self.mix_into(out, Some(send));
    }

    fn mix_into(&mut self, out: &mut [f32], mut send: Option<&mut [f32]>) {
        let frames = out.len() / 2;
        let out_rate = f64::from(self.rate);
        let cubic = self.cubic;
        for slot in &mut self.voices {
            let Some(v) = slot.as_mut() else { continue };
            let mut post = Post::of(v, frames);
            let ended = match v.feed.as_mut() {
                Some(feed) => mix_feed(
                    feed,
                    &mut v.pos,
                    &v.settings,
                    out_rate,
                    &mut post,
                    out,
                    send.as_deref_mut(),
                ),
                None => mix_source(
                    &v.source,
                    &mut v.pos,
                    &v.settings,
                    out_rate,
                    cubic,
                    &mut post,
                    out,
                    send.as_deref_mut(),
                ),
            };
            v.current = v.settings.gains;
            v.held = post.held;
            if ended {
                *slot = None;
            }
        }
    }

    /// Fill a device's block of `channels` interleaved channels: the mix on the first two (a
    /// one-channel device gets the two sides' mean) and silence on any others. `scratch` is the
    /// stereo block, kept by the caller so the device's thread does not allocate.
    pub fn fill(&mut self, out: &mut [f32], channels: usize, scratch: &mut Vec<f32>) {
        let channels = channels.max(1);
        let frames = out.len() / channels;
        scratch.clear();
        scratch.resize(frames * 2, 0.0);
        self.mix(scratch);
        out.fill(0.0);
        for f in 0..frames {
            let (l, r) = (scratch[2 * f], scratch[2 * f + 1]);
            if channels == 1 {
                out[f] = (l + r) * 0.5;
            } else {
                out[f * channels] = l;
                out[f * channels + 1] = r;
            }
        }
    }
}

/// What happens to a voice's frames after they are read: the low pass, the gains (gliding or
/// not) and the send.
struct Post {
    from: [f32; 2],
    to: [f32; 2],
    glide: bool,
    frames: f32,
    share: f32,
    held: [f32; 2],
    send: f32,
}

impl Post {
    fn of(v: &Voice, frames: usize) -> Self {
        #[allow(clippy::cast_precision_loss)] // a block's frame count
        let n = frames.max(1) as f32;
        Self {
            from: v.current,
            to: v.settings.gains,
            glide: v.settings.glide && v.current != v.settings.gains,
            frames: n,
            share: v.settings.lowpass,
            held: v.held,
            send: v.settings.send,
        }
    }

    #[inline]
    fn put(&mut self, f: usize, l: f32, r: f32, out: &mut [f32], send: Option<&mut [f32]>) {
        let (l, r) = if self.share < 1.0 {
            self.held[0] += (l - self.held[0]) * self.share;
            self.held[1] += (r - self.held[1]) * self.share;
            (self.held[0], self.held[1])
        } else {
            (l, r)
        };
        let [gl, gr] = if self.glide {
            #[allow(clippy::cast_precision_loss)] // a frame index within a block
            let t = (f + 1) as f32 / self.frames;
            [
                self.from[0] + (self.to[0] - self.from[0]) * t,
                self.from[1] + (self.to[1] - self.from[1]) * t,
            ]
        } else {
            self.to
        };
        let (l, r) = (l * gl, r * gr);
        out[2 * f] += l;
        out[2 * f + 1] += r;
        if self.send > 0.0 {
            if let Some(s) = send {
                s[2 * f] += l * self.send;
                s[2 * f + 1] += r * self.send;
            }
        }
    }
}

/// The Hermite curve through four neighbouring samples, at `t` between the middle two.
#[inline]
fn hermite(xm1: f32, x0: f32, x1: f32, x2: f32, t: f32) -> f32 {
    let c1 = 0.5 * (x1 - xm1);
    let c2 = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
    let c3 = 0.5 * (x2 - xm1) + 1.5 * (x0 - x1);
    ((c3 * t + c2) * t + c1) * t + x0
}

/// Mix a source's voice into `out`; whether it has ended.
#[allow(clippy::too_many_arguments)]
fn mix_source(
    src: &Source,
    pos: &mut f64,
    settings: &VoiceSettings,
    out_rate: f64,
    cubic: bool,
    post: &mut Post,
    out: &mut [f32],
    mut send: Option<&mut [f32]>,
) -> bool {
    let frames = out.len() / 2;
    let n = src.len_frames();
    if n == 0 {
        return true;
    }
    let step = settings.rate / out_rate;
    let len = n as f64;
    let looping = settings.looping;
    // A loop's seam: its last `fade` frames are crossfaded into its start, and it then goes on
    // from `fade` frames in, so the loop is `period` frames long.
    let fade = if looping {
        (settings.crossfade as usize).min(n / 2)
    } else {
        0
    };
    let period = (n - fade).max(1);
    let wrap = |i: isize| -> usize {
        if i < 0 {
            0
        } else {
            #[allow(clippy::cast_sign_loss)] // not negative here
            let i = i as usize;
            if i < n {
                i
            } else if looping {
                fade + (i - n) % period
            } else {
                n - 1
            }
        }
    };
    let read = |p: f64| -> (f32, f32) {
        let i = p.floor();
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: a frame index below the source's length, which is a `usize`.
        let i0 = i as isize;
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: an interpolation weight in [0, 1).
        let t = (p - i) as f32;
        let (l0, r0) = src.frame(wrap(i0));
        if t == 0.0 {
            return (l0, r0);
        }
        let (l1, r1) = src.frame(wrap(i0 + 1));
        if cubic {
            let (lm, rm) = src.frame(wrap(i0 - 1));
            let (l2, r2) = src.frame(wrap(i0 + 2));
            (hermite(lm, l0, l1, l2, t), hermite(rm, r0, r1, r2, t))
        } else {
            (l0 + (l1 - l0) * t, r0 + (r1 - r0) * t)
        }
    };
    #[allow(clippy::cast_precision_loss)] // frame counts
    let (seam, span) = ((n - fade) as f64, fade as f64);
    for f in 0..frames {
        if *pos >= len {
            if looping {
                #[allow(clippy::cast_precision_loss)] // a frame count
                let p = period as f64;
                *pos = span + (*pos - len) % p;
            } else {
                return true;
            }
        }
        let (mut l, mut r) = read(*pos);
        if fade > 0 && *pos >= seam {
            #[allow(clippy::cast_possible_truncation)] // a share of the seam
            let a = ((*pos - seam) / span) as f32;
            let half = std::f32::consts::FRAC_PI_2;
            let (out_w, in_w) = (
                pyreal_primitives::num::math::cosf(a * half),
                pyreal_primitives::num::math::sinf(a * half),
            );
            let (hl, hr) = read(*pos - seam);
            l = l * out_w + hl * in_w;
            r = r * out_w + hr * in_w;
        }
        post.put(f, l, r, out, send.as_deref_mut());
        *pos += step;
    }
    !looping && *pos >= len
}

/// Mix a stream's voice into `out`; whether it has ended.
fn mix_feed(
    feed: &mut Feed,
    pos: &mut f64,
    settings: &VoiceSettings,
    out_rate: f64,
    post: &mut Post,
    out: &mut [f32],
    mut send: Option<&mut [f32]>,
) -> bool {
    let frames = out.len() / 2;
    let step = settings.rate / out_rate;
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: a block's frame count.
    let span = step * frames as f64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: frame positions, never negative, far below u64's range.
    let (first, last) = (pos.floor() as u64, (*pos + span).ceil() as u64 + 1);
    feed.cover(first, last);
    for f in 0..frames {
        let i = pos.floor();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a frame position, never negative.
        let i0 = i as u64;
        let Some((l0, r0)) = feed.frame(i0) else {
            return feed.ended;
        };
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: an interpolation weight in [0, 1).
        let t = (*pos - i) as f32;
        let (l1, r1) = feed.frame(i0 + 1).unwrap_or((l0, r0));
        post.put(
            f,
            l0 + (l1 - l0) * t,
            r0 + (r1 - r0) * t,
            out,
            send.as_deref_mut(),
        );
        *pos += step;
    }
    false
}

/// A gain from a level in millibels (hundredths of a decibel, as DirectSound takes its volumes):
/// `10^(mb / 2000)`.
#[must_use]
pub fn gain_from_millibels(mb: i32) -> f32 {
    pyreal_primitives::num::math::powf(10f32, mb as f32 / 2000.0)
}

/// DirectSound's pan in millibels as two gains: a positive pan attenuates the left channel by
/// that much, a negative one the right, and the other side stays at full level.
#[must_use]
pub fn pan_gains_millibels(pan: i32) -> [f32; 2] {
    let att = gain_from_millibels(-pan.abs());
    if pan >= 0 {
        [att, 1.0]
    } else {
        [1.0, att]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(frames: usize, rate: u32) -> Source {
        let mut v = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let s = i as f32 / 4.0;
            v.push(s);
            v.push(-s);
        }
        Source::new(v.into(), rate)
    }

    /// A source at the mixer's rate, read at that rate, comes out sample for sample times its
    /// gains.
    #[test]
    fn a_source_at_the_mixer_rate_comes_out_sample_for_sample() {
        let mut m = Mixer::new(22050, 4);
        let s = ramp(5, 22050);
        let mut set = VoiceSettings::of(&s);
        set.gains = [0.5, 2.0];
        let id = m.start(s, set).expect("a free voice");
        let mut out = vec![0.0f32; 10];
        m.mix(&mut out);
        let want: Vec<f32> = (0..5)
            .flat_map(|i| [i as f32 / 4.0 * 0.5, -(i as f32) / 4.0 * 2.0])
            .collect();
        assert_eq!(out, want);
        assert!(!m.is_playing(id), "it ended exactly at the block's end");
    }

    /// A source at half the mixer's rate is read half a frame a step, the odd frames the mean of
    /// their neighbours.
    #[test]
    fn a_source_at_half_the_rate_is_interpolated_between_frames() {
        let mut m = Mixer::new(22050, 1);
        let s = ramp(3, 11025);
        let set = VoiceSettings::of(&s);
        m.start(s, set);
        let mut out = vec![0.0f32; 12];
        m.mix(&mut out);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        // 0, 0.125, 0.25, 0.375, 0.5 and the last frame held (no next frame to lean on).
        assert_eq!(left, vec![0.0, 0.125, 0.25, 0.375, 0.5, 0.5]);
    }

    /// A looping voice starts again from its first frame and never ends; a voice that does not
    /// loop stops and leaves silence.
    #[test]
    fn a_looping_voice_wraps_and_one_that_does_not_ends() {
        let mut m = Mixer::new(100, 2);
        let s = ramp(3, 100);
        let mut set = VoiceSettings::of(&s);
        set.looping = true;
        let looping = m.start(s.clone(), set).expect("free");
        let mut out = vec![0.0f32; 16];
        m.mix(&mut out);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        assert_eq!(left, vec![0.0, 0.25, 0.5, 0.0, 0.25, 0.5, 0.0, 0.25]);
        assert!(m.is_playing(looping));
        assert!(m.set_looping(looping, false));
        let mut out = vec![0.0f32; 8];
        m.mix(&mut out);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        assert_eq!(left, vec![0.5, 0.0, 0.0, 0.0]);
        assert!(!m.is_playing(looping));
    }

    /// Gains and the rate change while the voice plays: the next block hears them.
    #[test]
    fn gains_and_rate_change_while_the_voice_plays() {
        let mut m = Mixer::new(100, 1);
        let s = Source::new(vec![1.0; 40].into(), 100);
        let set = VoiceSettings::of(&s);
        let id = m.start(s, set).expect("free");
        let mut out = vec![0.0f32; 4];
        m.mix(&mut out);
        assert_eq!(out, vec![1.0; 4]);
        m.set_gains(id, [0.25, 0.0]);
        m.set_rate(id, 200.0);
        let mut out = vec![0.0f32; 4];
        m.mix(&mut out);
        assert_eq!(out, vec![0.25, 0.0, 0.25, 0.0]);
        assert_eq!(m.position(id), Some(6.0), "two frames at 1, then two at 2");
    }

    /// With every voice busy a new one is refused, and nothing playing is stopped for it.
    #[test]
    fn a_full_mixer_refuses_and_stops_nothing() {
        let mut m = Mixer::new(100, 2);
        let s = ramp(10, 100);
        let a = m.start(s.clone(), VoiceSettings::of(&s));
        let b = m.start(s.clone(), VoiceSettings::of(&s));
        assert!(a.is_some() && b.is_some() && a != b);
        assert_eq!(m.start(s.clone(), VoiceSettings::of(&s)), None);
        assert_eq!(m.active(), 2);
        assert!(m.stop(a.expect("a")));
        assert!(m.start(s.clone(), VoiceSettings::of(&s)).is_some());
    }

    /// Voices sum into the caller's block, which is not cleared.
    #[test]
    fn voices_sum_into_the_block() {
        let mut m = Mixer::new(10, 2);
        let s = Source::new(vec![1.0, 2.0].into(), 10);
        m.start(s.clone(), VoiceSettings::of(&s));
        m.start(s.clone(), VoiceSettings::of(&s));
        let mut out = vec![0.5, 0.5, 0.5, 0.5];
        m.mix(&mut out);
        assert_eq!(out, vec![2.5, 4.5, 0.5, 0.5]);
    }

    /// A device block takes the mix on its first two channels and silence on the rest; a
    /// one-channel device takes the sides' mean.
    #[test]
    fn a_device_block_takes_the_mix_on_its_first_two_channels() {
        let s = Source::new(vec![1.0, 3.0, 1.0, 3.0].into(), 10);
        let mut m = Mixer::new(10, 1);
        m.start(s.clone(), VoiceSettings::of(&s));
        let mut scratch = Vec::new();
        let mut out = vec![9.0f32; 8];
        m.fill(&mut out, 4, &mut scratch);
        assert_eq!(out, vec![1.0, 3.0, 0.0, 0.0, 1.0, 3.0, 0.0, 0.0]);
        let mut m = Mixer::new(10, 1);
        m.start(s.clone(), VoiceSettings::of(&s));
        let mut out = vec![0.0f32; 2];
        m.fill(&mut out, 1, &mut scratch);
        assert_eq!(out, vec![2.0, 2.0]);
    }

    /// Mono is spread to both sides, stereo keeps its sides, and more channels keep the first two.
    #[test]
    fn sources_are_widened_to_stereo() {
        let s = Source::from_interleaved(&[0.1, 0.2], 1, 8000);
        assert_eq!(s.frames(), &[0.1, 0.1, 0.2, 0.2]);
        let s = Source::from_interleaved(&[0.1, 0.2, 0.3, 0.4], 2, 8000);
        assert_eq!(s.frames(), &[0.1, 0.2, 0.3, 0.4]);
        let s = Source::from_interleaved(&[0.1, 0.2, 0.9, 0.3, 0.4, 0.9, 0.5], 3, 8000);
        assert_eq!(s.frames(), &[0.1, 0.2, 0.3, 0.4]);
        assert!((s.seconds() - 2.0 / 8000.0).abs() < 1e-12);
    }

    /// A one-channel source keeps one sample a frame and is heard on both sides.
    #[test]
    fn a_mono_source_is_heard_on_both_sides() {
        let s = Source::mono(vec![0.5, 0.25].into(), 100);
        assert_eq!(s.len_frames(), 2);
        assert_eq!(s.frame(1), (0.25, 0.25));
        let mut m = Mixer::new(100, 1);
        m.start(s.clone(), VoiceSettings::of(&s));
        let mut out = vec![0.0f32; 4];
        m.mix(&mut out);
        assert_eq!(out, vec![0.5, 0.5, 0.25, 0.25]);
    }

    /// A gliding voice reaches new gains across the block; one that does not jumps to them.
    #[test]
    fn a_gliding_voice_moves_to_its_new_gains_across_the_block() {
        let s = Source::mono(vec![1.0; 16].into(), 100);
        let mut m = Mixer::new(100, 1);
        let id = m
            .start(
                s.clone(),
                VoiceSettings {
                    glide: true,
                    ..VoiceSettings::of(&s)
                },
            )
            .expect("a voice");
        m.set_gains(id, [0.0, 0.0]);
        let mut out = vec![0.0f32; 8];
        m.mix(&mut out);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        assert_eq!(left, vec![0.75, 0.5, 0.25, 0.0]);
    }

    /// The low pass smooths a step, and the send carries the voice's share after its gains.
    #[test]
    fn the_low_pass_smooths_and_the_send_takes_its_share() {
        let s = Source::mono(vec![1.0; 8].into(), 100);
        let mut m = Mixer::new(100, 1);
        m.start(
            s.clone(),
            VoiceSettings {
                gains: [0.5, 0.5],
                lowpass: 0.5,
                send: 0.5,
                ..VoiceSettings::of(&s)
            },
        );
        let mut out = vec![0.0f32; 6];
        let mut send = vec![0.0f32; 6];
        m.mix_with_send(&mut out, &mut send);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        assert_eq!(left, vec![0.25, 0.375, 0.4375]);
        assert_eq!(send[0], 0.125);
        assert!((lowpass_share(24_000.0, 48_000) - 1.0).abs() < f32::EPSILON);
        assert!(lowpass_share(1000.0, 48_000) < 0.2);
    }

    /// A crossfaded loop runs its end into its start and never comes back through its first
    /// `crossfade` frames.
    #[test]
    fn a_crossfaded_loop_runs_its_end_into_its_start() {
        // Eight frames: four of 1.0 then four of 0.0, crossfading the last two into the first two.
        let s = Source::mono(vec![1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0].into(), 100);
        let mut m = Mixer::new(100, 1);
        m.start(
            s.clone(),
            VoiceSettings {
                looping: true,
                crossfade: 2,
                ..VoiceSettings::of(&s)
            },
        );
        let mut out = vec![0.0f32; 2 * 14];
        m.mix(&mut out);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        // Frame 6 is all end, frame 7 half way into the start's 1.0.
        assert_eq!(&left[..6], &[1.0, 1.0, 1.0, 1.0, 0.0, 0.0]);
        assert!((left[6] - 0.0).abs() < 1e-6);
        assert!((left[7] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        // Then on from frame 2: 1.0, 1.0, 0.0, 0.0 and the seam again.
        assert_eq!(&left[8..12], &[1.0, 1.0, 0.0, 0.0]);
    }

    /// Through four neighbours a source at half the rate follows its curve; at its own rate it
    /// still comes out sample for sample.
    #[test]
    fn a_cubic_mixer_follows_the_curve_and_keeps_exact_frames() {
        let s = Source::mono(vec![0.0, 1.0, 4.0, 9.0, 16.0].into(), 50);
        let mut m = Mixer::new(100, 1).with_cubic();
        m.start(s.clone(), VoiceSettings::of(&s));
        let mut out = vec![0.0f32; 2 * 6];
        m.mix(&mut out);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        assert_eq!(left[0], 0.0);
        assert_eq!(left[2], 1.0);
        assert_eq!(left[4], 4.0);
        // x squared at 1.5 is 2.25; the curve through 0, 1, 4, 9 gives it exactly.
        assert!((left[3] - 2.25).abs() < 1e-5, "{}", left[3]);
    }

    /// A ramp made as it is read, `limit` frames long.
    #[derive(Debug)]
    struct Ramp {
        next: usize,
        limit: usize,
        rate: u32,
    }

    impl Stream for Ramp {
        fn rate(&self) -> u32 {
            self.rate
        }

        fn read(&mut self, out: &mut [f32]) -> usize {
            let n = (out.len() / 2).min(self.limit - self.next);
            for k in 0..n {
                let s = (self.next + k) as f32 / 4.0;
                out[2 * k] = s;
                out[2 * k + 1] = -s;
            }
            self.next += n;
            n
        }
    }

    /// A stream at the mixer's rate comes out sample for sample, read as it is needed across
    /// blocks, and its voice ends when the stream does.
    #[test]
    fn a_stream_is_read_as_it_plays_and_ends_with_its_stream() {
        let mut m = Mixer::new(22050, 2);
        let ramp = Ramp {
            next: 0,
            limit: 300,
            rate: 22050,
        };
        let set = VoiceSettings {
            gains: [1.0, 0.5],
            rate: 22050.0,
            looping: false,
            lowpass: 1.0,
            send: 0.0,
            glide: false,
            crossfade: 0,
        };
        let id = m.start_stream(Box::new(ramp), set).expect("a free voice");
        let mut out = vec![0.0f32; 2 * 200];
        m.mix(&mut out);
        assert_eq!(out[2 * 199], 199.0 / 4.0);
        assert_eq!(out[2 * 199 + 1], -199.0 / 8.0);
        assert!(m.is_playing(id));
        let mut out = vec![0.0f32; 2 * 200];
        m.mix(&mut out);
        assert_eq!(
            out[0],
            200.0 / 4.0,
            "the next block goes on where the last one stopped"
        );
        assert_eq!(out[2 * 99], 299.0 / 4.0);
        assert_eq!(out[2 * 100], 0.0, "silence after its end");
        assert!(!m.is_playing(id), "the voice ended with its stream");
    }

    /// A stream read at half the mixer's rate is interpolated between its frames.
    #[test]
    fn a_stream_read_at_another_rate_is_interpolated() {
        let mut m = Mixer::new(200, 1);
        let ramp = Ramp {
            next: 0,
            limit: 1000,
            rate: 100,
        };
        let set = VoiceSettings {
            gains: [1.0, 1.0],
            rate: 100.0,
            looping: false,
            lowpass: 1.0,
            send: 0.0,
            glide: false,
            crossfade: 0,
        };
        m.start_stream(Box::new(ramp), set);
        let mut out = vec![0.0f32; 2 * 5];
        m.mix(&mut out);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        assert_eq!(left, vec![0.0, 0.125, 0.25, 0.375, 0.5]);
    }

    /// Millibels: 0 is unity, -2000 a tenth, and a positive pan quietens the left side only.
    #[test]
    fn millibels_and_pan_follow_directsound() {
        assert_eq!(gain_from_millibels(0), 1.0);
        assert!((gain_from_millibels(-2000) - 0.1).abs() < 1e-6);
        let [l, r] = pan_gains_millibels(2000);
        assert!((l - 0.1).abs() < 1e-6 && r == 1.0);
        let [l, r] = pan_gains_millibels(-2000);
        assert!(l == 1.0 && (r - 0.1).abs() < 1e-6);
    }

    /// Gains, a low pass or a send that are not numbers are taken as silence, never mixed.
    #[test]
    fn a_non_finite_shape_is_never_mixed() {
        let mut m = Mixer::new(22050, 4);
        let s = ramp(64, 22050);
        let mut set = VoiceSettings::of(&s);
        set.gains = [f32::NAN, 1.0];
        m.start(s.clone(), set).expect("a free voice");
        let b = m
            .start(s, VoiceSettings::of(&ramp(64, 22050)))
            .expect("a free voice");
        assert!(m.set_shape(b, [f32::INFINITY, 1.0], f32::NAN, f32::NAN));
        let mut out = vec![0.0f32; 128];
        let mut send = vec![0.0f32; 128];
        m.mix_with_send(&mut out, &mut send);
        assert!(out.iter().all(|v| v.is_finite()), "{out:?}");
        assert!(send.iter().all(|v| v.is_finite()), "{send:?}");
    }

    /// A read rate that is not a number plays the source at its own rate; a change to one is
    /// refused.
    #[test]
    fn a_non_finite_rate_plays_at_the_sources_own() {
        let mut m = Mixer::new(22050, 4);
        let s = ramp(64, 22050);
        let mut set = VoiceSettings::of(&s);
        set.rate = f64::NAN;
        let id = m.start(s, set).expect("a free voice");
        assert!(m.set_rate(id, f64::INFINITY));
        let mut out = vec![0.0f32; 32 * 2];
        m.mix(&mut out);
        assert!(out.iter().all(|v| v.is_finite()));
        assert!(m.is_playing(id), "half way through it had stopped");
        assert!(out.iter().any(|v| *v != 0.0), "nothing was heard");
        m.mix(&mut out);
        assert!(!m.is_playing(id), "it did not play through at its own rate");
    }
}
