//! The loaded-sound cache and its reference counting.
//!
//! The cache is keyed by wave DataID and holds one buffer reference per loaded wave.
//!
//! The loading policy is eager and reference-counted: **every wave named by any loaded sound table or
//! animation hook is fully decoded and resident for as long as that table or hook is alive.** Nothing
//! is loaded on demand at play time, so there is never a "still loading" state to reproduce, and
//! nothing is evicted while referenced.
//!
//! | Creates the sound | Releases it |
//! |---|---|
//! | the sound table row | its own destructor |
//! | the animation sound hook | its own destructor |
//! | the tweaked sound hook | its own destructor |

use std::collections::BTreeMap;
use std::sync::Arc;

use pyreal_primitives::DataId;

use crate::error::AudioError;
use crate::prefs::SoundStyle;
use crate::wave::{Native, Sample};
use crate::AudioAssets;

/// The cached wave metadata read by voice selection.
///
/// The defaults are sound id 0, priority 0, probability 1 and volume 1.
/// For a cached wave **nothing ever writes these values again**. That is why
/// the voice-priority system is inert: the pool reads the cached metadata's priority, not the
/// row's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoundData {
    pub sound_id: DataId,
    pub priority: f32,
    pub probability: f32,
    pub volume: f32,
}

impl Default for SoundData {
    /// The client's own defaults for this block.
    fn default() -> Self {
        Self {
            sound_id: DataId(0),
            priority: 0.0,
            probability: 1.0,
            volume: 1.0,
        }
    }
}

/// One: the cached wave, its reference count, and its own `SoundData`.
#[derive(Debug, Clone)]
pub struct SoundBufRef {
    /// Reference count, initialized to 1.
    pub links: i32,
    /// The shared sample at the original's rate, never played directly. `None` when the wave
    /// could not be decoded (a silent, valid cache entry), or while the enhanced sound plays.
    pub sample: Option<Arc<Sample>>,
    /// The wave at its own rate, which the enhanced sound plays and the original's sample is made
    /// from. `None` when it could not be decoded.
    pub native: Option<Native>,
    /// Cached wave metadata. See [`SoundData`].
    pub data: SoundData,
}

/// The cache, keyed by wave DataID.
#[derive(Debug, Default)]
pub struct SoundCache {
    // ORDER-OK: the cache is only ever looked up by DataID and
    // walked once on shutdown to stop every entry --
    // an order-independent operation. A BTreeMap keeps that walk deterministic anyway.
    entries: BTreeMap<DataId, SoundBufRef>,
    /// Waves whose decode failed, so a retry is not attempted on every creation request. A failed
    /// buffer creation still leaves a cache entry in the observed client; this preserves that behavior.
    failed: BTreeMap<DataId, String>,
    /// Which engine plays: only the original's needs [`SoundBufRef::sample`].
    style: SoundStyle,
}

/// A wave decoded for the cache away from it ([`decode`]), so the mixer's lock is not held while
/// it decodes.
#[derive(Debug)]
pub struct Decoded {
    native: Native,
    sample: Option<Arc<Sample>>,
}

/// Decode wave `id` for a cache whose style is `style`: the original's sample is made only for
/// the original.
///
/// # Errors
/// Whatever the decode reports.
pub fn decode(
    id: DataId,
    assets: &dyn AudioAssets,
    style: SoundStyle,
) -> Result<Decoded, AudioError> {
    let native = assets.native(id)?;
    let sample =
        (style == SoundStyle::Original).then(|| Arc::new(crate::wave::retail_sample(&native)));
    Ok(Decoded { native, sample })
}

impl SoundCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create (or take another link on) a sound by data id.
    ///
    /// Present: increment the reference count. Absent: perform the dat read,
    /// decode and the buffer creation, and add it to the table. A decode failure is **not** an error
    /// to the caller — buffer creation always returns 0 and no caller checks it — so this returns
    /// the error only so that a test can see it; [`crate::AudioSystem`] drops it.
    pub fn create_sound(&mut self, id: DataId, assets: &dyn AudioAssets) -> Result<(), AudioError> {
        if self.link(id) {
            return Ok(());
        }
        let decoded = decode(id, assets, self.style);
        self.insert_decoded(id, decoded)
    }

    /// Take another link on a resident sound; whether it was resident.
    pub fn link(&mut self, id: DataId) -> bool {
        match self.entries.get_mut(&id) {
            Some(e) => {
                e.links += 1;
                true
            }
            None => false,
        }
    }

    /// [`Self::create_sound`] with the decode already done: a resident sound takes another link
    /// and the decode is dropped.
    ///
    /// # Errors
    /// The decode's own, kept as a silent entry exactly as [`Self::create_sound`] keeps it.
    pub fn insert_decoded(
        &mut self,
        id: DataId,
        decoded: Result<Decoded, AudioError>,
    ) -> Result<(), AudioError> {
        if self.link(id) {
            return Ok(());
        }
        let (native, sample, err) = match decoded {
            Ok(d) => {
                // The style may have changed while it decoded.
                let sample = match self.style {
                    SoundStyle::Original => d
                        .sample
                        .or_else(|| Some(Arc::new(crate::wave::retail_sample(&d.native)))),
                    SoundStyle::Enhanced => None,
                };
                (Some(d.native), sample, None)
            }
            Err(e) => {
                let msg = e.to_string();
                (None, None, Some((e, msg)))
            }
        };
        self.entries.insert(
            id,
            SoundBufRef {
                links: 1,
                sample,
                native,
                data: SoundData::default(),
            },
        );
        match err {
            Some((e, msg)) => {
                self.failed.insert(id, msg);
                Err(e)
            }
            None => Ok(()),
        }
    }

    /// The engine in force: the original's makes every resident wave's sample, the enhanced
    /// one lets them go.
    pub fn restyle(&mut self, style: SoundStyle) {
        if self.style == style {
            return;
        }
        self.style = style;
        for e in self.entries.values_mut() {
            e.sample = match style {
                SoundStyle::Original => e
                    .native
                    .as_ref()
                    .map(|n| Arc::new(crate::wave::retail_sample(n))),
                SoundStyle::Enhanced => None,
            };
        }
    }

    /// The engine the cache keeps samples for.
    #[must_use]
    pub fn style(&self) -> SoundStyle {
        self.style
    }

    /// Destroy a sound by data id: decrement `links_`; at zero, remove the
    /// node and delete the sound buffer.
    pub fn destroy_sound(&mut self, id: DataId) {
        let gone = match self.entries.get_mut(&id) {
            Some(e) => {
                e.links -= 1;
                e.links <= 0
            }
            None => false,
        };
        if gone {
            self.entries.remove(&id);
            self.failed.remove(&id);
        }
    }

    /// `sound_hash_.find(id)` — the lookup entries 1, 2 and 8 do by open-coded bucket walk.
    #[must_use]
    pub fn find(&self, id: DataId) -> Option<&SoundBufRef> {
        self.entries.get(&id)
    }

    /// How many waves are resident.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The reference count on one entry, for tests.
    #[must_use]
    pub fn links(&self, id: DataId) -> Option<i32> {
        self.entries.get(&id).map(|e| e.links)
    }

    /// Why a resident wave is silent, if it is.
    #[must_use]
    pub fn failure(&self, id: DataId) -> Option<&str> {
        self.failed.get(&id).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::StubAssets;

    /// Oracle: the recovered cache lifecycle — `create_sound` bumps the link count,
    /// `destroy_sound` decrements it, and at zero the node and its buffer go away.
    #[test]
    fn create_and_destroy_reference_count_the_entry() {
        let a = StubAssets::with_tone(DataId(0x0A00_0002), 8);
        let mut c = SoundCache::new();
        let id = DataId(0x0A00_0002);
        c.create_sound(id, &a).expect("decodes");
        assert_eq!(c.links(id), Some(1));
        c.create_sound(id, &a).expect("already resident");
        assert_eq!(c.links(id), Some(2));
        assert_eq!(
            c.len(),
            1,
            "a second create_sound must not decode the wave again"
        );
        c.destroy_sound(id);
        assert_eq!(c.links(id), Some(1));
        assert!(c.find(id).is_some());
        c.destroy_sound(id);
        assert!(c.find(id).is_none(), "at links_ == 0 the entry is removed");
        assert!(c.is_empty());
    }

    /// Every cached wave's own `SoundData` carries the constructor defaults and nothing writes them
    /// again -- which is why the priority system is inert. Oracle: the creation path and its
    /// verified absence of a later metadata writer.
    #[test]
    fn a_cached_sound_always_has_priority_zero() {
        let id = DataId(0x0A00_0002);
        let a = StubAssets::with_tone(id, 8);
        let mut c = SoundCache::new();
        c.create_sound(id, &a).expect("decodes");
        let e = c.find(id).expect("resident");
        assert_eq!(e.data.priority, 0.0);
        assert_eq!(e.data.probability, 1.0);
        assert_eq!(e.data.volume, 1.0);
        assert_eq!(e.data.sound_id, DataId(0));
    }

    /// A wave that will not decode is still a resident, silent entry: failed creation leaves
    /// the buffer null, `Play` returns 0, nothing is logged and nothing crashes.
    #[test]
    fn an_undecodable_wave_becomes_a_silent_resident_entry() {
        let a = StubAssets::default();
        let mut c = SoundCache::new();
        let id = DataId(0x0A00_9999);
        assert!(c.create_sound(id, &a).is_err());
        let e = c.find(id).expect("still resident");
        assert!(e.sample.is_none());
        assert_eq!(e.links, 1);
        assert!(c.failure(id).is_some());
    }

    /// Destroying an id that was never created is a no-op, not a panic. Destroying looks the id up
    /// first and does nothing on a miss.
    #[test]
    fn destroying_an_unknown_id_does_nothing() {
        let mut c = SoundCache::new();
        c.destroy_sound(DataId(0x0A00_1234));
        assert!(c.is_empty());
    }
}
