//! The pipelines' cache kept between runs, where the device keeps one (Vulkan's).
//!
//! The driver keeps its own cache of compiled pipelines, which it may throw away (a driver update,
//! a full disk, a cleaned cache). This one is the engine's: what the device compiled the last
//! time, kept in a file of Pyreal's settings folder and handed back as the pipelines are made.
//!
//! The file is named for the adapter, its driver and Pyreal's version, so another device's, an old
//! driver's or an old build's cache is never offered. Inside, an envelope of its own (a mark, the
//! length and a checksum) wraps what the device gave; a file that is not exactly that is read as no
//! cache at all, and the device is asked for an empty one. Nothing here can stop the engine from
//! starting: every failure leaves the pipelines made without a cache.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Where the cache files go, and the version they are written for ([`set_dir`]).
static PLACE: OnceLock<(PathBuf, String)> = OnceLock::new();

/// The envelope's mark.
const MAGIC: &[u8; 8] = b"PYRLPC01";
/// The envelope: the mark, the payload's length and its checksum.
const HEADER: usize = 8 + 8 + 8;

/// Keep the cache in `dir`, for Pyreal `version` (anything that changes with a build: its version
/// and commit). Set once, before the device is made; a run that never sets it keeps no file.
pub fn set_dir(dir: impl Into<PathBuf>, version: &str) {
    let _ = PLACE.set((dir.into(), version.to_owned()));
}

/// The file the cache of the adapter `info` is kept in, under `dir` for `version`: `None` for a
/// backend that keeps no cache.
#[must_use]
pub fn file_for(dir: &Path, version: &str, info: &wgpu::AdapterInfo) -> Option<PathBuf> {
    file_named(
        dir,
        version,
        info.backend,
        (info.vendor, info.device),
        &format!("{} {}", info.driver, info.driver_info),
    )
}

/// [`file_for`] from the adapter's backend, vendor and device, and its driver's name and version.
fn file_named(
    dir: &Path,
    version: &str,
    backend: wgpu::Backend,
    (vendor, device): (u32, u32),
    driver: &str,
) -> Option<PathBuf> {
    if backend != wgpu::Backend::Vulkan {
        return None;
    }
    let name = format!(
        "pipelines-vulkan-{vendor:04x}-{device:04x}-{}-{}.bin",
        clean(driver),
        clean(version),
    );
    Some(dir.join(name))
}

/// `s` with only what a file name can hold everywhere, at most 48 characters.
fn clean(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    out.truncate(48);
    if out.is_empty() {
        out.push('_');
    }
    out
}

/// FNV-1a over `bytes`: enough to tell a torn or altered file from the one written.
fn checksum(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// `payload` in the envelope, as the file holds it.
#[must_use]
pub fn seal(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER + payload.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&checksum(payload).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// What the envelope `file` holds, when it is whole and unaltered.
#[must_use]
pub fn open(file: &[u8]) -> Option<&[u8]> {
    if file.len() < HEADER || &file[..8] != MAGIC {
        return None;
    }
    let len = u64::from_le_bytes(file[8..16].try_into().ok()?);
    let sum = u64::from_le_bytes(file[16..24].try_into().ok()?);
    let payload = &file[HEADER..];
    (payload.len() as u64 == len && checksum(payload) == sum).then_some(payload)
}

/// How the cache started, for the client's log (the engine logs nothing itself).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    /// The device keeps no cache (not Vulkan), or no cache could be made.
    None,
    /// No folder was given ([`set_dir`]), or there is no file yet: an empty cache.
    Empty,
    /// Filled from this file.
    Read(PathBuf),
    /// The file was there but not whole, or the device refused it: an empty cache, and why.
    Refused(PathBuf, String),
}

/// The device's cache for `adapter`, filled from its file when that is whole and the device takes
/// it; an empty one otherwise. `None` when the device keeps no cache.
#[must_use]
pub fn make(
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
) -> (Option<(wgpu::PipelineCache, Option<PathBuf>)>, Start) {
    if !device.features().contains(wgpu::Features::PIPELINE_CACHE) {
        return (None, Start::None);
    }
    let file = PLACE
        .get()
        .and_then(|(dir, version)| file_for(dir, version, &adapter.get_info()));
    let bytes = file.as_ref().and_then(|f| std::fs::read(f).ok());
    let data = bytes.as_deref().and_then(open);
    let attempt = |data: Option<&[u8]>| {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        // SAFETY: `data` is only ever what this device's own cache gave back for this adapter and
        // driver (the file is named for both), checked whole by its envelope; wgpu checks its own
        // header again and the driver its own.
        #[allow(unsafe_code)]
        let cache = unsafe {
            device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
                label: Some("pipelines"),
                data,
                fallback: true,
            })
        };
        let error = pollster::block_on(scope.pop());
        (cache, error)
    };
    let (cache, error) = attempt(data);
    let start = match (&file, bytes.is_some(), data.is_some(), error) {
        (Some(f), true, true, None) => Start::Read(f.clone()),
        (_, _, _, None) => match (&file, bytes.is_some()) {
            (Some(f), true) => Start::Refused(f.clone(), "the file is not whole".into()),
            _ => Start::Empty,
        },
        (f, _, _, Some(e)) => {
            let why = e.to_string();
            let (cache, again) = attempt(None);
            if again.is_some() {
                return (None, Start::None);
            }
            return (
                Some((cache, file.clone())),
                f.as_ref()
                    .map_or(Start::Empty, |f| Start::Refused(f.clone(), why)),
            );
        }
    };
    (Some((cache, file)), start)
}

/// Writes `cache` to `file` (beside it first, then moved over it, so a torn write never stands):
/// how many bytes, or why it could not be written. Nothing to write is `Ok(0)`.
///
/// # Errors
/// The folder or the file could not be written.
pub fn save(cache: &wgpu::PipelineCache, file: &Path) -> std::io::Result<usize> {
    let Some(data) = cache.get_data() else {
        return Ok(0);
    };
    let sealed = seal(&data);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let part = file.with_extension("part");
    std::fs::write(&part, &sealed)?;
    std::fs::rename(&part, file)?;
    Ok(sealed.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sealed_cache_opens_to_what_was_sealed() {
        let payload = b"what the device gave back".to_vec();
        let file = seal(&payload);
        assert_eq!(open(&file), Some(payload.as_slice()));
        assert_eq!(open(&seal(&[])), Some(&[][..]));
    }

    #[test]
    fn a_torn_altered_or_foreign_file_opens_to_nothing() {
        let file = seal(b"what the device gave back");
        // Torn.
        assert_eq!(open(&file[..file.len() - 1]), None);
        assert_eq!(open(&file[..10]), None);
        // Altered.
        let mut altered = file.clone();
        let last = altered.len() - 1;
        altered[last] ^= 1;
        assert_eq!(open(&altered), None);
        // Longer than written.
        let mut longer = file.clone();
        longer.push(0);
        assert_eq!(open(&longer), None);
        // Not ours at all.
        assert_eq!(open(b"anything else entirely, long enough"), None);
        assert_eq!(open(&[]), None);
    }

    #[test]
    fn the_file_is_named_for_the_adapter_its_driver_and_the_version() {
        let dir = Path::new("cache");
        let vk = wgpu::Backend::Vulkan;
        let driver = "Intel Corporation 101.8425";
        let a = file_named(dir, "0.1.0 abc", vk, (0x8086, 0x64a0), driver);
        let b = file_named(dir, "0.1.0 abc", vk, (0x8086, 0x64a1), driver);
        let c = file_named(
            dir,
            "0.1.0 abc",
            vk,
            (0x8086, 0x64a0),
            "Intel Corporation 101.9000",
        );
        let d = file_named(dir, "0.1.1 def", vk, (0x8086, 0x64a0), driver);
        assert!(a.is_some());
        assert_ne!(a, b, "another device");
        assert_ne!(a, c, "another driver");
        assert_ne!(a, d, "another build");
        assert_eq!(
            file_named(dir, "0.1.0", wgpu::Backend::Dx12, (0x8086, 0x64a0), driver),
            None
        );
        let name = a
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(
            name.chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)),
            "{name}"
        );
    }
}
