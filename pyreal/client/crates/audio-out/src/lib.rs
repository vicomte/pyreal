//! The default sound output endpoint through cpal, for any client whose mixer plays through the
//! sound engine's output seam.
//!
//! **Depends on** the sound engine's output seam (`pyreal-audio`) and `cpal`. **Used by** the
//! desktop host (`pyreal-desktop`) and the second game's desktop client (`auberean-client`), each
//! of which hands it to its mixer.
//!
//! **Must never** reach a client runtime or an application half: it is a device and nothing else,
//! so any executable can open sound with it (`cargo xtask seams`).

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SupportedStreamConfigRange;
use cpal::{FromSample, SampleFormat, SizedSample, StreamError, SupportedStreamConfig};

/// The mixer's fill, shared by each stream opened on its behalf in turn.
type SharedFill = Arc<Mutex<Box<dyn FnMut(&mut [f32]) + Send>>>;

/// How long a failed reopen waits before trying the default device again.
const REOPEN_WAIT: Duration = Duration::from_secs(1);

/// The default output endpoint through `cpal`: the executable's
/// [`AudioOutput`](pyreal_audio::output::AudioOutput) implementation. It owns the device probe and
/// the output stream.
#[derive(Default)]
pub struct CpalOutput {
    endpoint: Option<(cpal::Device, cpal::SupportedStreamConfig)>,
}

impl std::fmt::Debug for CpalOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpalOutput")
            .field("probed", &self.endpoint.is_some())
            .finish()
    }
}

impl pyreal_audio::output::AudioOutput for CpalOutput {
    fn probe(&mut self) -> Result<(u32, u16), String> {
        let host = cpal::default_host();
        let (device, cfg) = host
            .default_output_device()
            .ok_or_else(|| "no default output device".to_owned())
            .and_then(|d| {
                d.default_output_config()
                    .map(|c| (d, c))
                    .map_err(|e| e.to_string())
            })?;
        let found = (cfg.sample_rate(), cfg.channels());
        self.endpoint = Some((device, cfg));
        Ok(found)
    }

    fn start(
        &mut self,
        fill: Box<dyn FnMut(&mut [f32]) + Send>,
    ) -> Result<Box<dyn std::any::Any>, String> {
        let Some((device, cfg)) = self.endpoint.clone() else {
            return Err("no endpoint was probed".to_owned());
        };
        let fill: SharedFill = Arc::new(Mutex::new(fill));
        let (events, watch) = std::sync::mpsc::channel();
        let (ready_tx, ready) = std::sync::mpsc::channel();
        let notices = events.clone();
        // Every stream is opened, kept and dropped on this one thread, which reopens the output
        // on the default device when a stream fails.
        let thread = std::thread::Builder::new()
            .name("sound output".to_owned())
            .spawn(move || {
                let mix = (cfg.sample_rate(), cfg.channels());
                let first = open(&device, &cfg, mix.1, &fill, &notices);
                let stream = match first {
                    Ok(s) => {
                        let _ = ready_tx.send(Ok(()));
                        s
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                keep_playing(stream, mix, &fill, &notices, &watch);
            })
            .map_err(|e| e.to_string())?;
        match ready.recv() {
            Ok(Ok(())) => Ok(Box::new(Playing {
                events,
                thread: Some(thread),
            })),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err("the sound output thread ended before its stream started".to_owned()),
        }
    }
}

/// What the output thread is told.
enum Event {
    /// A stream failed in a way only reopening mends.
    Failed,
    /// The output is no longer wanted.
    Stop,
}

/// The handle `CpalOutput`'s [`pyreal_audio::output::AudioOutput::start`] returns: dropping it stops
/// the output.
struct Playing {
    events: Sender<Event>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Playing {
    fn drop(&mut self) {
        let _ = self.events.send(Event::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Hold `stream` until told to stop, reopening on the default device each time a stream fails.
fn keep_playing(
    stream: cpal::Stream,
    (rate, channels): (u32, u16),
    fill: &SharedFill,
    notices: &Sender<Event>,
    watch: &Receiver<Event>,
) {
    let mut stream = Some(stream);
    let mut logged = false;
    loop {
        let next = if stream.is_some() {
            watch.recv().map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            watch.recv_timeout(REOPEN_WAIT)
        };
        match next {
            Ok(Event::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(Event::Failed) => {
                stream = None;
                // Notices the old stream sent after the first are about it too.
                while let Ok(e) = watch.try_recv() {
                    if matches!(e, Event::Stop) {
                        return;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if stream.is_none() {
            match reopen(rate, channels, fill, notices) {
                Ok(s) => {
                    tracing::info!("sound output reopened on the default device");
                    stream = Some(s);
                    logged = false;
                }
                Err(e) if !logged => {
                    tracing::warn!("sound output could not reopen: {e}; trying again");
                    logged = true;
                }
                Err(_) => {}
            }
        }
    }
}

/// Open a stream on the default output device at the mix's rate and start it.
fn reopen(
    rate: u32,
    channels: u16,
    fill: &SharedFill,
    notices: &Sender<Event>,
) -> Result<cpal::Stream, String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| "no default output device".to_owned())?;
    let default = device.default_output_config().map_err(|e| e.to_string())?;
    let ranges: Vec<SupportedStreamConfigRange> = device
        .supported_output_configs()
        .map_err(|e| e.to_string())?
        .collect();
    let cfg = pick_config(default, &ranges, rate, channels)
        .ok_or_else(|| format!("the default output device does not play at {rate} Hz"))?;
    open(&device, &cfg, channels, fill, notices)
}

/// The configuration to reopen at: the device's default when it runs at the mix's `rate`, else
/// one of `ranges` that takes that rate, the mix's channel count and the default's sample format
/// first. The mix was made for that rate, so `None` without it.
fn pick_config(
    default: SupportedStreamConfig,
    ranges: &[SupportedStreamConfigRange],
    rate: u32,
    channels: u16,
) -> Option<SupportedStreamConfig> {
    if default.sample_rate() == rate {
        return Some(default);
    }
    ranges
        .iter()
        .filter(|r| r.min_sample_rate() <= rate && rate <= r.max_sample_rate())
        .max_by_key(|r| {
            (
                r.channels() == channels,
                r.sample_format() == default.sample_format(),
            )
        })
        .and_then(|r| (*r).try_with_sample_rate(rate))
}

/// Whether a stream error is one only reopening the output mends.
fn reopens_on(e: &StreamError) -> bool {
    !matches!(e, StreamError::BufferUnderrun)
}

/// Build and start a stream on `device` in its own sample format, converting the mix (`channels`
/// interleaved `f32` channels) for it.
fn open(
    device: &cpal::Device,
    cfg: &SupportedStreamConfig,
    channels: u16,
    fill: &SharedFill,
    notices: &Sender<Event>,
) -> Result<cpal::Stream, String> {
    let stream = match cfg.sample_format() {
        SampleFormat::F32 => build::<f32>(device, cfg, channels, fill, notices),
        SampleFormat::F64 => build::<f64>(device, cfg, channels, fill, notices),
        SampleFormat::I8 => build::<i8>(device, cfg, channels, fill, notices),
        SampleFormat::I16 => build::<i16>(device, cfg, channels, fill, notices),
        SampleFormat::I32 => build::<i32>(device, cfg, channels, fill, notices),
        SampleFormat::U8 => build::<u8>(device, cfg, channels, fill, notices),
        SampleFormat::U16 => build::<u16>(device, cfg, channels, fill, notices),
        SampleFormat::U32 => build::<u32>(device, cfg, channels, fill, notices),
        other => Err(format!("the output device wants {other} samples")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

/// [`open`] for a device that takes `T` samples.
fn build<T: SizedSample + FromSample<f32> + Send + 'static>(
    device: &cpal::Device,
    cfg: &SupportedStreamConfig,
    channels: u16,
    fill: &SharedFill,
    notices: &Sender<Event>,
) -> Result<cpal::Stream, String> {
    let (mix_ch, dev_ch) = (
        usize::from(channels.max(1)),
        usize::from(cfg.channels().max(1)),
    );
    let fill = Arc::clone(fill);
    let notices = notices.clone();
    // The mix in `f32`, kept between blocks; it grows only when a block does.
    let mut mix: Vec<f32> = Vec::new();
    device
        .build_output_stream(
            &cfg.config(),
            move |out: &mut [T], _: &cpal::OutputCallbackInfo| {
                mix.resize(out.len() / dev_ch * mix_ch, 0.0);
                (fill.lock().unwrap_or_else(PoisonError::into_inner))(&mut mix);
                write_frames(&mix, mix_ch, out, dev_ch);
            },
            move |e| {
                if reopens_on(&e) {
                    tracing::warn!("audio stream error: {e}");
                    let _ = notices.send(Event::Failed);
                } else {
                    tracing::debug!("audio stream error: {e}");
                }
            },
            None,
        )
        .map_err(|e| e.to_string())
}

/// Write the mix (`mix_ch` interleaved channels) into a device block of `dev_ch` channels:
/// converted to the device's samples, one channel hearing both sides of a stereo mix, a mono
/// mix in every channel and channels past the mix's silent.
fn write_frames<T: FromSample<f32>>(mix: &[f32], mix_ch: usize, out: &mut [T], dev_ch: usize) {
    let (mix_ch, dev_ch) = (mix_ch.max(1), dev_ch.max(1));
    for (src, dst) in mix.chunks_exact(mix_ch).zip(out.chunks_exact_mut(dev_ch)) {
        #[allow(clippy::cast_precision_loss)] // a channel count
        let mean = src.iter().sum::<f32>() / mix_ch as f32;
        for (c, slot) in dst.iter_mut().enumerate() {
            let v = if dev_ch == 1 {
                mean
            } else if mix_ch == 1 {
                src[0]
            } else {
                src.get(c).copied().unwrap_or(0.0)
            };
            *slot = T::from_sample_(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpal::{
        Sample, SampleFormat, StreamError, SupportedBufferSize, SupportedStreamConfigRange,
    };

    /// A mixed block reaches an integer device converted, sample for sample.
    #[test]
    fn a_block_reaches_an_integer_device_converted() {
        let mix = [0.5f32, -0.5, 1.0, -1.0];
        let mut i = [0i16; 4];
        write_frames(&mix, 2, &mut i, 2);
        assert_eq!(i, mix.map(i16::from_sample));
        assert!(i[0] > 16_000 && i[1] < -16_000);
        let mut u = [0u16; 4];
        write_frames(&mix, 2, &mut u, 2);
        assert_eq!(u, mix.map(u16::from_sample));
        assert!(u[0] > 48_000 && u[1] < 17_000);
        let mut f = [0f32; 4];
        write_frames(&mix, 2, &mut f, 2);
        assert_eq!(f, mix, "an f32 device is handed the mix unchanged");
    }

    /// A device with another channel count than the mix's: one channel hears both sides, a
    /// stereo mix fills the first two of many, and a mono mix fills every channel.
    #[test]
    fn a_block_reaches_a_device_of_another_channel_count() {
        let stereo = [0.25f32, 0.75, -1.0, 0.0];
        let mut mono = [0f32; 2];
        write_frames(&stereo, 2, &mut mono, 1);
        assert_eq!(mono, [0.5, -0.5]);
        let mut six = [9f32; 12];
        write_frames(&stereo, 2, &mut six, 6);
        assert_eq!(
            six,
            [0.25, 0.75, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0]
        );
        let mut wide = [0f32; 4];
        write_frames(&[0.5, -0.5], 1, &mut wide, 2);
        assert_eq!(wide, [0.5, 0.5, -0.5, -0.5]);
    }

    /// A device gone or a stream invalidated is reopened; an underrun is only a glitch.
    #[test]
    fn a_lost_device_is_reopened_and_an_underrun_is_not() {
        assert!(reopens_on(&StreamError::DeviceNotAvailable));
        assert!(reopens_on(&StreamError::StreamInvalidated));
        assert!(!reopens_on(&StreamError::BufferUnderrun));
    }

    /// On reopening, the new device is played at the mix's rate: its default when that rate,
    /// else a configuration at that rate, the mix's channel count first; none when it has no
    /// such rate.
    #[test]
    fn a_reopened_device_plays_at_the_mixs_rate() {
        let range = |ch, lo, hi, f| {
            SupportedStreamConfigRange::new(ch, lo, hi, SupportedBufferSize::Unknown, f)
        };
        let default = range(2, 48_000, 48_000, SampleFormat::F32).with_sample_rate(48_000);
        let ranges = [
            range(1, 8_000, 96_000, SampleFormat::I16),
            range(2, 8_000, 96_000, SampleFormat::I16),
            range(2, 48_000, 48_000, SampleFormat::F32),
        ];
        assert_eq!(
            pick_config(default.clone(), &ranges, 48_000, 2),
            Some(default.clone())
        );
        let picked = pick_config(default.clone(), &ranges, 44_100, 2).expect("a 44.1 kHz config");
        assert_eq!((picked.sample_rate(), picked.channels()), (44_100, 2));
        assert_eq!(pick_config(default, &ranges[2..], 44_100, 2), None);
    }

    /// On this machine's own default device, a stream starts in the device's format and asks
    /// the mix for blocks, and stops when let go.
    #[test]
    #[ignore = "needs a sound device"]
    fn a_stream_on_the_default_device_asks_for_blocks() {
        use pyreal_audio::output::AudioOutput;
        let mut out = CpalOutput::default();
        let (rate, channels) = out.probe().expect("a default device");
        let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = Arc::clone(&asked);
        let playing = out
            .start(Box::new(move |b: &mut [f32]| {
                b.fill(0.0);
                count.fetch_add(b.len(), std::sync::atomic::Ordering::Relaxed);
            }))
            .expect("a stream");
        std::thread::sleep(Duration::from_millis(500));
        drop(playing);
        let n = asked.load(std::sync::atomic::Ordering::Relaxed);
        assert!(
            n > 0,
            "no block was asked for at {rate} Hz, {channels} channel(s)"
        );
        let after = asked.load(std::sync::atomic::Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            asked.load(std::sync::atomic::Ordering::Relaxed),
            after,
            "it plays on"
        );
    }
}
