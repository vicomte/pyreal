//! One lock per machine for work that takes the GPU or every core, so timing runs, the GPU test
//! tiers and release builds queue instead of skewing each other.
//!
//! **Depends on** no workspace crate. **Used by** `xtask` (the GPU tiers, the gates and `cargo
//! xtask lock`) and `pyreal-engine-view` (its headless runs); `build.py` takes the same lock
//! itself.
//!
//! **Must never** be depended on by the client: it is for builds and measurements.
//!
//! The lock is `flock` on `machine.lock` in Pyreal's cache folder (`~/Library/Caches/pyreal` on
//! macOS), so it is let go when its holder exits, however it exits. The holder writes who it is to
//! `machine.holder` beside it, which a waiter prints. `PYREAL_MACHINE_LOCK=off` skips the lock; a
//! holder sets it to `held` for the processes it starts, so they do not wait on their parent.

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The variable that skips the lock: `off` by choice, `held` when a parent process holds it.
pub const ENV: &str = "PYREAL_MACHINE_LOCK";

/// How often a waiter tries the lock again.
const POLL: Duration = Duration::from_secs(2);

/// How often a waiter repeats who holds the lock when the holder has not changed.
const REPEAT: Duration = Duration::from_secs(60);

/// The lock, held until it is dropped.
#[derive(Debug)]
pub struct MachineLock {
    _file: File,
}

/// Who holds the lock, as the holder wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holder {
    pub pid: u32,
    pub what: String,
    pub dir: String,
    /// Seconds since the Unix epoch.
    pub since: u64,
}

impl Holder {
    fn parse(text: &str) -> Option<Self> {
        let field = |key: &str| {
            text.lines()
                .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
                .map(str::to_owned)
        };
        Some(Self {
            pid: field("pid")?.parse().ok()?,
            what: field("what")?,
            dir: field("dir").unwrap_or_default(),
            since: field("since")?.parse().ok()?,
        })
    }

    fn text(&self) -> String {
        format!(
            "pid={}\nwhat={}\ndir={}\nsince={}\n",
            self.pid, self.what, self.dir, self.since
        )
    }

    /// One line for a person: what holds the lock, where, and for how long at `now`.
    pub fn describe(&self, now: u64) -> String {
        let secs = now.saturating_sub(self.since);
        format!(
            "`{}` (pid {}) in {}, for {}m {:02}s",
            self.what,
            self.pid,
            self.dir,
            secs / 60,
            secs % 60
        )
    }
}

/// Whether the lock is free, or who holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Free,
    /// Held; the holder's own note, when it wrote one.
    Held(Option<Holder>),
}

/// Pyreal's per-user cache folder.
pub fn cache_dir() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
    if cfg!(target_os = "macos") {
        var("HOME").map(|h| PathBuf::from(h).join("Library/Caches/pyreal"))
    } else if cfg!(windows) {
        var("LOCALAPPDATA").map(|d| PathBuf::from(d).join("pyreal"))
    } else {
        var("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| var("HOME").map(|h| PathBuf::from(h).join(".cache")))
            .map(|d| d.join("pyreal"))
    }
}

/// The lock file.
pub fn lock_path() -> Option<PathBuf> {
    cache_dir().map(|d| d.join("machine.lock"))
}

fn holder_path(lock: &Path) -> PathBuf {
    lock.with_extension("holder")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Takes the machine lock for `what`, waiting while another process holds it and saying on the
/// standard error who does. `None` when [`ENV`] is `off` or `held`; once taken, [`ENV`] is set to
/// `held` for the processes this one starts, so call it before starting threads.
pub fn acquire(what: &str) -> io::Result<Option<MachineLock>> {
    if matches!(std::env::var(ENV).as_deref(), Ok("off" | "held")) {
        return Ok(None);
    }
    let path = lock_path().ok_or_else(|| io::Error::other("no home folder for the cache"))?;
    let lock = acquire_at(&path, what, POLL, &mut |line| eprintln!("{line}"))?;
    std::env::set_var(ENV, "held");
    Ok(Some(lock))
}

/// [`acquire`] on the lock file `path`, trying every `poll` and passing each waiting line to
/// `say`.
pub fn acquire_at(
    path: &Path,
    what: &str,
    poll: Duration,
    say: &mut dyn FnMut(&str),
) -> io::Result<MachineLock> {
    let file = open(path)?;
    let mut said: Option<(Option<Holder>, Instant)> = None;
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) => {
                let holder = read_holder(path);
                let due = said
                    .as_ref()
                    .is_none_or(|(h, at)| *h != holder || at.elapsed() >= REPEAT);
                if due {
                    let who = holder
                        .as_ref()
                        .map_or_else(|| "another process".to_owned(), |h| h.describe(now()));
                    say(&format!("waiting for the machine lock: held by {who}"));
                    said = Some((holder, Instant::now()));
                }
                std::thread::sleep(poll);
            }
            Err(TryLockError::Error(e)) => return Err(e),
        }
    }
    let holder = Holder {
        pid: std::process::id(),
        what: what.to_owned(),
        dir: std::env::current_dir()
            .map(|d| d.display().to_string())
            .unwrap_or_default(),
        since: now(),
    };
    std::fs::write(holder_path(path), holder.text())?;
    Ok(MachineLock { _file: file })
}

/// Whether the lock at `path` is free, without waiting.
pub fn state_at(path: &Path) -> io::Result<State> {
    let file = open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(State::Free),
        Err(TryLockError::WouldBlock) => Ok(State::Held(read_holder(path))),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

/// Whether the machine lock is free.
pub fn state() -> io::Result<State> {
    let path = lock_path().ok_or_else(|| io::Error::other("no home folder for the cache"))?;
    state_at(&path)
}

fn open(path: &Path) -> io::Result<File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
}

fn read_holder(lock: &Path) -> Option<Holder> {
    Holder::parse(&std::fs::read_to_string(holder_path(lock)).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("machine-lock-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("machine.lock")
    }

    /// **A second taker waits for the first, says who holds it, and gets the lock once it is let
    /// go.**
    #[test]
    fn a_second_taker_waits_and_names_the_holder() {
        let path = scratch("wait");
        let first = acquire_at(&path, "ci tier2", POLL, &mut |_| {}).unwrap();
        match state_at(&path).unwrap() {
            State::Held(Some(h)) => {
                assert_eq!(h.pid, std::process::id());
                assert_eq!(h.what, "ci tier2");
            }
            other => panic!("expected the lock held, got {other:?}"),
        }
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(first);
        });
        let mut lines = Vec::new();
        let second = acquire_at(&path, "build.py", Duration::from_millis(50), &mut |l| {
            lines.push(l.to_owned());
        })
        .unwrap();
        releaser.join().unwrap();
        assert_eq!(lines.len(), 1, "the same holder is named once: {lines:?}");
        assert!(lines[0].contains("`ci tier2`"), "{}", lines[0]);
        drop(second);
        assert_eq!(state_at(&path).unwrap(), State::Free);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// **The holder's note reads back as written**, and a damaged one reads as no note.
    #[test]
    fn the_holder_note_round_trips() {
        let h = Holder {
            pid: 42,
            what: "cargo xtask ci tier1".into(),
            dir: "/work/pyreal".into(),
            since: 1_000,
        };
        assert_eq!(Holder::parse(&h.text()), Some(h.clone()));
        assert_eq!(Holder::parse("pid=x\nwhat=y\nsince=1\n"), None);
        assert_eq!(
            h.describe(1_125),
            "`cargo xtask ci tier1` (pid 42) in /work/pyreal, for 2m 05s"
        );
    }
}
