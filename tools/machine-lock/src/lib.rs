//! One lock per machine with two holds: heavy work (builds, test tiers, renders) never waits and
//! marks the machine busy, and a quiet hold for timing and profiling runs waits for that work to
//! finish and then has the machine alone.
//!
//! **Depends on** no workspace crate. **Used by** `xtask` (the tiers, the gates, `golden` and
//! `cargo xtask lock`) and `pyreal-engine-view` (its headless runs); `build.py` takes a heavy hold
//! through `machine_lock.py` beside this crate, which speaks the same files.
//!
//! **Must never** be depended on by the client: it is for builds and measurements.
//!
//! A heavy hold is a shared `flock` on `machine.modes` in Pyreal's cache folder
//! (`~/Library/Caches/pyreal` on macOS) and a quiet hold an exclusive one, so a hold goes when its
//! holder exits, however it exits. A quiet taker first takes `machine.queue` alone and keeps it. A
//! heavy taker takes its share only while that queue is free; when a quiet run holds the machine or
//! is next, it runs beside it without a share and says so, so heavy work never waits and never
//! delays a quiet run that came first. A quiet hold also takes `machine.lock`, the single exclusive
//! lock of older checkouts, so it waits for their runs and keeps them out. Each holder, and each
//! waiting quiet run, writes who it is to a note in `machine.holders` that it keeps locked while it
//! lives; a note nobody keeps locked is a dead holder's and is removed. `PYREAL_MACHINE_LOCK=off`
//! skips the lock; a holder sets it to `heavy` or `quiet` for the processes it starts, so they do
//! not take it again.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The variable that skips the lock: `off` by choice, `heavy` or `quiet` when a parent process
/// holds it (`held` from older holders).
pub const ENV: &str = "PYREAL_MACHINE_LOCK";

/// How often a waiter tries the lock again.
const POLL: Duration = Duration::from_secs(2);

/// How often a waiter repeats who holds the lock when the holders have not changed.
const REPEAT: Duration = Duration::from_secs(60);

/// How long a waiter blocked by nobody it can name retries quietly before saying so: a heavy taker
/// passes through the queue in an instant, and writes its note just after taking its share.
const UNNAMED_GRACE: Duration = Duration::from_millis(500);

/// What a hold is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Builds, test tiers, renders: any number run at once, and none waits.
    Heavy,
    /// A timing or profiling run: waits until no heavy work runs, then has the machine alone.
    Quiet,
}

impl Mode {
    /// The name in notes and in [`ENV`].
    pub fn name(self) -> &'static str {
        match self {
            Mode::Heavy => "heavy",
            Mode::Quiet => "quiet",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "heavy" => Some(Mode::Heavy),
            "quiet" => Some(Mode::Quiet),
            _ => None,
        }
    }
}

/// A hold on the machine, let go when dropped.
#[derive(Debug)]
pub struct MachineLock {
    _note: Note,
    /// None for heavy work that runs beside a quiet run.
    _modes: Option<File>,
    /// A quiet hold's hold on older checkouts' lock.
    _older: Option<File>,
    /// A quiet hold keeps the queue too, so later heavy work takes no share.
    _queue: Option<File>,
}

/// Who holds the lock, or waits for a quiet hold, as they wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holder {
    pub pid: u32,
    pub what: String,
    pub dir: String,
    /// Seconds since the Unix epoch.
    pub since: u64,
    pub mode: Mode,
    /// A quiet run next in line, not yet holding.
    pub waiting: bool,
    /// Heavy work that started while a quiet run held the machine or was next, so it runs without
    /// a share and the quiet run does not wait for it.
    pub beside_quiet: bool,
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
            // An older holder's note has no mode; its hold was exclusive.
            mode: field("mode").map_or(Some(Mode::Quiet), |m| Mode::parse(&m))?,
            waiting: field("waiting").as_deref() == Some("1"),
            beside_quiet: field("beside_quiet").as_deref() == Some("1"),
        })
    }

    fn text(&self) -> String {
        format!(
            "pid={}\nwhat={}\ndir={}\nsince={}\nmode={}\nwaiting={}\nbeside_quiet={}\n",
            self.pid,
            self.what,
            self.dir,
            self.since,
            self.mode.name(),
            u8::from(self.waiting),
            u8::from(self.beside_quiet)
        )
    }

    /// One line for a person: what holds the lock, how, where, and for how long at `now`.
    pub fn describe(&self, now: u64) -> String {
        let secs = now.saturating_sub(self.since);
        let how = match (self.mode, self.waiting) {
            (Mode::Heavy, _) if self.beside_quiet => "heavy, beside a quiet run",
            (Mode::Heavy, _) => "heavy",
            (Mode::Quiet, false) => "quiet",
            (Mode::Quiet, true) => "waiting for quiet",
        };
        format!(
            "`{}` (pid {}, {how}) in {}, for {}m {:02}s",
            self.what,
            self.pid,
            self.dir,
            secs / 60,
            secs % 60
        )
    }

    /// Whether a quiet taker waits for this holder.
    fn holds_up_quiet(&self) -> bool {
        !(self.mode == Mode::Heavy && self.beside_quiet)
    }
}

/// Who holds the lock now.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct State {
    /// Every live hold and waiting quiet run, oldest first.
    pub holders: Vec<Holder>,
    /// An older checkout's hold that left no note.
    pub unnamed: bool,
}

impl State {
    /// Whether nothing holds the lock or waits for it.
    pub fn is_free(&self) -> bool {
        self.holders.is_empty() && !self.unnamed
    }
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

/// The lock file of older checkouts; the two-mode lock's files sit beside it.
pub fn lock_path() -> Option<PathBuf> {
    cache_dir().map(|d| d.join("machine.lock"))
}

fn modes_path(lock: &Path) -> PathBuf {
    lock.with_extension("modes")
}

fn queue_path(lock: &Path) -> PathBuf {
    lock.with_extension("queue")
}

fn notes_dir(lock: &Path) -> PathBuf {
    lock.with_extension("holders")
}

/// The note older checkouts' holders write.
fn legacy_note(lock: &Path) -> PathBuf {
    lock.with_extension("holder")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// What a taker does with the [`ENV`] it inherited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Inherited {
    Take,
    Skip,
    /// A quiet run started inside heavy work's hold: waiting would wait on its own parent.
    NotQuiet,
}

fn inherited(env: Option<&str>, mode: Mode) -> Inherited {
    match env {
        Some("heavy") if mode == Mode::Quiet => Inherited::NotQuiet,
        Some("off" | "held" | "heavy" | "quiet") => Inherited::Skip,
        _ => Inherited::Take,
    }
}

/// Takes a `mode` hold on the machine for `what`, saying on the standard error who a quiet hold
/// waits for, or which quiet run heavy work runs beside. `None` when [`ENV`] says to skip it or a parent holds it; once
/// taken, [`ENV`] is set to the mode for the processes this one starts, so call it before starting
/// threads.
pub fn acquire(mode: Mode, what: &str) -> io::Result<Option<MachineLock>> {
    match inherited(std::env::var(ENV).ok().as_deref(), mode) {
        Inherited::Take => {}
        Inherited::Skip => return Ok(None),
        Inherited::NotQuiet => {
            eprintln!("{what} runs inside heavy work's hold on the machine, so it is not quiet");
            return Ok(None);
        }
    }
    let path = lock_path().ok_or_else(|| io::Error::other("no home folder for the cache"))?;
    let lock = acquire_at(&path, mode, what, POLL, &mut |line| eprintln!("{line}"))?;
    std::env::set_var(ENV, mode.name());
    Ok(Some(lock))
}

/// [`acquire`] beside the lock file `path`, a quiet taker trying every `poll`, passing each message
/// to `say`.
pub fn acquire_at(
    path: &Path,
    mode: Mode,
    what: &str,
    poll: Duration,
    say: &mut dyn FnMut(&str),
) -> io::Result<MachineLock> {
    let holder = |waiting, beside_quiet| Holder {
        pid: std::process::id(),
        what: what.to_owned(),
        dir: std::env::current_dir()
            .map(|d| d.display().to_string())
            .unwrap_or_default(),
        since: now(),
        mode,
        waiting,
        beside_quiet,
    };
    match mode {
        Mode::Heavy => {
            let queue = open(&queue_path(path))?;
            let modes = open(&modes_path(path))?;
            let shared = taken(queue.try_lock_shared())? && taken(modes.try_lock_shared())?;
            drop(queue);
            if !shared {
                let at = now();
                let quiet = live_notes(&notes_dir(path))
                    .into_iter()
                    .find(|h| h.mode == Mode::Quiet)
                    .map_or_else(|| "a quiet run".to_owned(), |h| h.describe(at));
                say(&format!(
                    "the machine lock: `{what}` runs beside {quiet} and may skew its timings"
                ));
            }
            Ok(MachineLock {
                _note: Note::write(&notes_dir(path), &holder(false, !shared))?,
                _modes: shared.then_some(modes),
                _older: None,
                _queue: None,
            })
        }
        Mode::Quiet => {
            let mut waiter = Waiter::new(poll, say);
            let queue = open(&queue_path(path))?;
            while !taken(queue.try_lock())? {
                waiter.wait(path);
            }
            let mut note = Note::write(&notes_dir(path), &holder(true, false))?;
            let modes = open(&modes_path(path))?;
            while !taken(modes.try_lock())? {
                waiter.wait(path);
            }
            let older = open(path)?;
            while !taken(older.try_lock())? {
                waiter.wait(path);
            }
            note.rewrite(&holder(false, false))?;
            Ok(MachineLock {
                _note: note,
                _modes: Some(modes),
                _older: Some(older),
                _queue: Some(queue),
            })
        }
    }
}

/// Whether a `try_lock` took the lock.
fn taken(result: Result<(), TryLockError>) -> io::Result<bool> {
    match result {
        Ok(()) => Ok(true),
        Err(TryLockError::WouldBlock) => Ok(false),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

/// A quiet taker between tries: says who it waits for when that changes, or every [`REPEAT`].
struct Waiter<'a> {
    poll: Duration,
    say: &'a mut dyn FnMut(&str),
    said: Option<(Vec<Holder>, Instant)>,
    unnamed_since: Option<Instant>,
}

impl<'a> Waiter<'a> {
    fn new(poll: Duration, say: &'a mut dyn FnMut(&str)) -> Self {
        Self {
            poll,
            say,
            said: None,
            unnamed_since: None,
        }
    }

    fn wait(&mut self, path: &Path) {
        let me = std::process::id();
        let mut blockers: Vec<Holder> = live_notes(&notes_dir(path))
            .into_iter()
            .filter(|h| h.holds_up_quiet() && !(h.pid == me && h.mode == Mode::Quiet && h.waiting))
            .collect();
        if blockers.is_empty() {
            let since = *self.unnamed_since.get_or_insert_with(Instant::now);
            if since.elapsed() < UNNAMED_GRACE {
                std::thread::sleep(self.poll.min(Duration::from_millis(20)));
                return;
            }
            blockers.extend(read_note(&legacy_note(path)));
        } else {
            self.unnamed_since = None;
        }
        let due = self
            .said
            .as_ref()
            .is_none_or(|(h, at)| *h != blockers || at.elapsed() >= REPEAT);
        if due {
            let at = now();
            let who = match blockers.as_slice() {
                [] => " another process".to_owned(),
                [one] => format!(" {}", one.describe(at)),
                many => {
                    let lines: String = many
                        .iter()
                        .map(|h| format!("\n  {}", h.describe(at)))
                        .collect();
                    format!(":{lines}")
                }
            };
            (self.say)(&format!(
                "waiting for the machine lock (quiet hold): held by{who}"
            ));
            self.said = Some((blockers, Instant::now()));
        }
        std::thread::sleep(self.poll);
    }
}

/// Who holds the lock beside `path`, without waiting. Dead holders' notes are removed on the way.
pub fn state_at(path: &Path) -> io::Result<State> {
    let holders = live_notes(&notes_dir(path));
    let exclusive = !taken(open(path)?.try_lock_shared())?;
    let unnamed = exclusive && !holders.iter().any(|h| h.mode == Mode::Quiet && !h.waiting);
    let mut state = State { holders, unnamed };
    if unnamed {
        if let Some(old) = read_note(&legacy_note(path)) {
            state.holders.push(old);
            state.unnamed = false;
        }
    }
    Ok(state)
}

/// Who holds the machine lock.
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

fn read_note(path: &Path) -> Option<Holder> {
    Holder::parse(&std::fs::read_to_string(path).ok()?)
}

/// A holder's note, kept locked (shared) while its holder lives and removed when dropped.
#[derive(Debug)]
struct Note {
    path: PathBuf,
    _file: File,
}

/// Tells this process's notes apart.
static NEXT_NOTE: AtomicU32 = AtomicU32::new(0);

impl Note {
    fn write(dir: &Path, holder: &Holder) -> io::Result<Self> {
        let id = format!(
            "{}-{}",
            std::process::id(),
            NEXT_NOTE.fetch_add(1, Ordering::Relaxed)
        );
        let path = dir.join(format!("{id}.note"));
        let file = Self::place(&path, holder)?;
        Ok(Self { path, _file: file })
    }

    fn rewrite(&mut self, holder: &Holder) -> io::Result<()> {
        self._file = Self::place(&self.path, holder)?;
        Ok(())
    }

    /// Writes `holder` beside `path`, locks it and moves it into place, so a reader never finds
    /// the note unlocked while its holder lives.
    fn place(path: &Path, holder: &Holder) -> io::Result<File> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp)?;
        file.write_all(holder.text().as_bytes())?;
        file.lock_shared()?;
        std::fs::rename(&tmp, path)?;
        Ok(file)
    }
}

impl Drop for Note {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// How old an unlocked half-written note must be before it is a dead taker's.
const STALE_TMP: Duration = Duration::from_secs(60);

/// The notes in `dir` whose holders live, oldest first. A note nobody keeps locked is removed.
fn live_notes(dir: &Path) -> Vec<Holder> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut live = Vec::new();
    for path in entries.flatten().map(|e| e.path()) {
        let ext = path.extension().and_then(|e| e.to_str());
        if !matches!(ext, Some("note" | "tmp")) {
            continue;
        }
        let Ok(file) = File::open(&path) else {
            continue;
        };
        match file.try_lock() {
            Ok(()) => {
                let old = path
                    .metadata()
                    .and_then(|m| m.modified())
                    .is_ok_and(|t| t.elapsed().unwrap_or_default() >= STALE_TMP);
                if ext == Some("note") || old {
                    drop(file);
                    let _ = std::fs::remove_file(&path);
                }
            }
            Err(TryLockError::WouldBlock) if ext == Some("note") => {
                live.extend(read_note(&path));
            }
            Err(_) => {}
        }
    }
    live.sort_by_key(|h| (h.since, h.pid));
    live
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("machine-lock-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("machine.lock")
    }

    const FAST: Duration = Duration::from_millis(20);

    fn take(path: &Path, mode: Mode, what: &str) -> MachineLock {
        acquire_at(path, mode, what, FAST, &mut |l| {
            panic!("{what} waited: {l}")
        })
        .unwrap()
    }

    fn whats(state: &State) -> Vec<(&str, Mode, bool)> {
        state
            .holders
            .iter()
            .map(|h| (h.what.as_str(), h.mode, h.waiting))
            .collect()
    }

    /// Polls `state_at(path)` until `done` holds, for up to ten seconds.
    fn until(path: &Path, done: impl Fn(&State) -> bool) -> State {
        let start = Instant::now();
        loop {
            let s = state_at(path).unwrap();
            if done(&s) {
                return s;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "timed out: {s:?}"
            );
            std::thread::sleep(FAST);
        }
    }

    /// **Heavy holds share the machine**: a second and third take it at once, and all three are
    /// listed.
    #[test]
    fn heavy_holds_share_the_machine() {
        let path = scratch("heavy");
        let a = take(&path, Mode::Heavy, "ci tier1");
        let b = take(&path, Mode::Heavy, "build.py");
        let c = take(&path, Mode::Heavy, "engine-view");
        let state = state_at(&path).unwrap();
        let mut held: Vec<_> = whats(&state).into_iter().map(|w| w.0).collect();
        held.sort_unstable();
        assert_eq!(held, ["build.py", "ci tier1", "engine-view"]);
        assert!(!state.unnamed);
        drop((a, b, c));
        assert!(state_at(&path).unwrap().is_free());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// **A quiet hold waits for every heavy hold, naming them all, then has the machine alone**;
    /// heavy work started under it does not wait, and says which quiet run it may skew.
    #[test]
    fn a_quiet_hold_waits_for_every_heavy_hold_and_heavy_work_never_waits() {
        let path = scratch("quiet");
        let a = take(&path, Mode::Heavy, "ci tier1");
        let b = take(&path, Mode::Heavy, "build.py");
        let (tx, rx) = mpsc::channel();
        let p = path.clone();
        let timer = std::thread::spawn(move || {
            let mut lines = Vec::new();
            let q = acquire_at(&p, Mode::Quiet, "frame-time sweep", FAST, &mut |l| {
                lines.push(l.to_owned());
            })
            .unwrap();
            tx.send(lines).unwrap();
            q
        });
        until(&path, |s| s.holders.iter().any(|h| h.waiting));
        drop(a);
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            rx.try_recv().is_err(),
            "the quiet hold went ahead of build.py"
        );
        drop(b);
        let lines = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let quiet = timer.join().unwrap();
        assert!(lines[0].starts_with("waiting for the machine lock (quiet hold): held by:\n  `"));
        assert!(
            lines[0].contains("`ci tier1`") && lines[0].contains("`build.py`"),
            "{lines:?}"
        );
        assert_eq!(
            whats(&state_at(&path).unwrap()),
            [("frame-time sweep", Mode::Quiet, false)]
        );

        let mut heavy_lines = Vec::new();
        let h = acquire_at(&path, Mode::Heavy, "golden", FAST, &mut |l| {
            heavy_lines.push(l.to_owned());
        })
        .unwrap();
        assert_eq!(heavy_lines.len(), 1, "{heavy_lines:?}");
        assert!(
            heavy_lines[0].contains("`golden` runs beside `frame-time sweep`"),
            "{heavy_lines:?}"
        );
        let state = state_at(&path).unwrap();
        let golden = state.holders.iter().find(|h| h.what == "golden").unwrap();
        assert!(golden.beside_quiet);
        assert!(golden
            .describe(golden.since)
            .contains("heavy, beside a quiet run"));
        drop((h, quiet));
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// **A waiting quiet run is not starved**: heavy work that comes after it runs at once, without
    /// a share, and the quiet run starts when the work that came before it ends.
    #[test]
    fn heavy_work_after_a_waiting_quiet_run_does_not_delay_it() {
        let path = scratch("priority");
        let first = take(&path, Mode::Heavy, "ci tier2");
        let (tx, rx) = mpsc::channel();
        let p = path.clone();
        let quiet = std::thread::spawn(move || {
            let q = acquire_at(&p, Mode::Quiet, "profile", FAST, &mut |_| {}).unwrap();
            tx.send(()).unwrap();
            drop(q);
        });
        until(&path, |s| s.holders.iter().any(|h| h.waiting));
        let mut lines = Vec::new();
        let late = acquire_at(&path, Mode::Heavy, "build.py", FAST, &mut |l| {
            lines.push(l.to_owned());
        })
        .unwrap();
        assert!(lines[0].contains("`profile`") && lines[0].contains("waiting for quiet"));
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            rx.try_recv().is_err(),
            "the quiet hold went ahead of ci tier2"
        );
        drop(first);
        rx.recv_timeout(Duration::from_secs(10))
            .expect("the quiet hold waited for heavy work that came after it");
        quiet.join().unwrap();
        drop(late);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// **A note nobody keeps locked is a dead holder's**: it is not listed and is removed.
    #[test]
    fn a_dead_holders_note_is_removed() {
        let path = scratch("stale");
        let dir = notes_dir(&path);
        std::fs::create_dir_all(&dir).unwrap();
        let dead = Holder {
            pid: 999_999,
            what: "ci tier1".into(),
            dir: "/gone".into(),
            since: 1,
            mode: Mode::Quiet,
            waiting: false,
            beside_quiet: false,
        };
        let note = dir.join("999999-0.note");
        std::fs::write(&note, dead.text()).unwrap();
        let live = take(&path, Mode::Heavy, "build.py");
        assert_eq!(
            whats(&state_at(&path).unwrap()),
            [("build.py", Mode::Heavy, false)]
        );
        assert!(!note.exists());
        drop(live);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// The process [`a_killed_holder_lets_go`] starts: holds the lock its parent names, then
    /// sleeps until killed.
    #[test]
    #[ignore = "started by a_killed_holder_lets_go"]
    fn hold_until_killed() {
        let Some(path) = std::env::var_os("MACHINE_LOCK_TEST_HOLD") else {
            return;
        };
        let _held = take(Path::new(&path), Mode::Heavy, "killed holder");
        std::thread::sleep(Duration::from_secs(60));
    }

    /// **A holder that is killed lets go**: its hold goes with its process, its note is removed,
    /// and a quiet hold is taken without waiting.
    #[test]
    fn a_killed_holder_lets_go() {
        let path = scratch("killed");
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["tests::hold_until_killed", "--exact", "--ignored"])
            .env("MACHINE_LOCK_TEST_HOLD", &path)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        until(&path, |s| s.holders.iter().any(|h| h.pid == pid));
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(state_at(&path).unwrap().is_free());
        assert_eq!(std::fs::read_dir(notes_dir(&path)).unwrap().count(), 0);
        drop(take(&path, Mode::Quiet, "profile"));
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// **An older checkout's exclusive hold leaves heavy holds alone**, and a quiet hold waits for
    /// it, naming it from its single note.
    #[test]
    fn an_older_checkouts_hold_holds_up_only_quiet_runs() {
        let path = scratch("older");
        let old = open(&path).unwrap();
        old.lock().unwrap();
        std::fs::write(
            legacy_note(&path),
            "pid=42\nwhat=cargo xtask ci tier1\ndir=/w\nsince=1\n",
        )
        .unwrap();
        let state = state_at(&path).unwrap();
        assert_eq!(
            whats(&state),
            [("cargo xtask ci tier1", Mode::Quiet, false)]
        );
        drop(take(&path, Mode::Heavy, "build.py"));
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1500));
            drop(old);
        });
        let mut lines = Vec::new();
        let q = acquire_at(&path, Mode::Quiet, "profile", FAST, &mut |l| {
            lines.push(l.to_owned());
        })
        .unwrap();
        releaser.join().unwrap();
        assert!(lines[0].contains("`cargo xtask ci tier1`"), "{lines:?}");
        drop(q);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// **A child of a holder does not wait on its parent**, and a quiet run inside heavy work is
    /// told it is not quiet.
    #[test]
    fn a_holders_children_skip_the_lock() {
        assert_eq!(inherited(None, Mode::Heavy), Inherited::Take);
        assert_eq!(inherited(Some(""), Mode::Quiet), Inherited::Take);
        assert_eq!(inherited(Some("off"), Mode::Quiet), Inherited::Skip);
        assert_eq!(inherited(Some("heavy"), Mode::Heavy), Inherited::Skip);
        assert_eq!(inherited(Some("quiet"), Mode::Heavy), Inherited::Skip);
        assert_eq!(inherited(Some("quiet"), Mode::Quiet), Inherited::Skip);
        assert_eq!(inherited(Some("held"), Mode::Quiet), Inherited::Skip);
        assert_eq!(inherited(Some("heavy"), Mode::Quiet), Inherited::NotQuiet);
    }

    /// **The holder's note reads back as written**, an older note reads as an exclusive hold, and
    /// a damaged one reads as no note.
    #[test]
    fn the_holder_note_round_trips() {
        let h = Holder {
            pid: 42,
            what: "cargo xtask ci tier1".into(),
            dir: "/work/pyreal".into(),
            since: 1_000,
            mode: Mode::Heavy,
            waiting: false,
            beside_quiet: false,
        };
        assert_eq!(Holder::parse(&h.text()), Some(h.clone()));
        let w = Holder {
            mode: Mode::Quiet,
            waiting: true,
            ..h.clone()
        };
        assert_eq!(Holder::parse(&w.text()), Some(w.clone()));
        let beside = Holder {
            beside_quiet: true,
            ..h.clone()
        };
        assert_eq!(Holder::parse(&beside.text()), Some(beside));
        assert_eq!(
            Holder::parse("pid=1\nwhat=x\nsince=2\n").map(|o| o.mode),
            Some(Mode::Quiet)
        );
        assert_eq!(Holder::parse("pid=x\nwhat=y\nsince=1\n"), None);
        assert_eq!(
            h.describe(1_125),
            "`cargo xtask ci tier1` (pid 42, heavy) in /work/pyreal, for 2m 05s"
        );
        assert_eq!(
            w.describe(1_000),
            "`cargo xtask ci tier1` (pid 42, waiting for quiet) in /work/pyreal, for 0m 00s"
        );
    }
}
