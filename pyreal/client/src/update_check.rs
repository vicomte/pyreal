//! Whether a newer Pyreal is out: once a run, off the frame thread, the latest release's
//! `release.json` read from the repository the build names ([`SOURCE_URL`]), at most once a day
//! ([`pyreal_launch::update`]). A build with no source URL never checks. A read that fails is
//! only logged.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use pyreal_horizon::front_door::UpdateNotice;
use pyreal_launch::update::{self, Release, UpdateCache};

/// The public repository releases are published in, as a release build names it; none in a
/// local build.
pub const SOURCE_URL: Option<&str> = match option_env!("PYREAL_BUILD_SOURCE_URL") {
    Some(url) if !url.is_empty() => Some(url),
    _ => None,
};

/// The version compared with the latest release: the build's own, or for a test the one
/// `PYREAL_CHECK_AS_VERSION` named when it was built.
pub const VERSION: &str = match option_env!("PYREAL_CHECK_AS_VERSION") {
    Some(v) if !v.is_empty() => v,
    _ => env!("CARGO_PKG_VERSION"),
};

/// How long the read may take.
const TIMEOUT_SECS: u64 = 5;

/// The check and its last answer.
#[derive(Debug)]
pub struct UpdateCheck {
    dir: PathBuf,
    source: Option<String>,
    current: String,
    cache: UpdateCache,
    reading: Option<mpsc::Receiver<Result<Release, String>>>,
    /// Whether this run has checked, or found the last answer fresh.
    asked: bool,
}

impl UpdateCheck {
    /// The answer kept in `dir`, for a build of version `current` from the repository at
    /// `source`. Nothing is read from the network yet.
    #[must_use]
    pub fn open(dir: &Path, source: Option<&str>, current: &str) -> Self {
        Self {
            dir: dir.to_path_buf(),
            source: source.map(|s| s.trim_end_matches('/').to_owned()),
            current: current.to_owned(),
            cache: UpdateCache::load(dir),
            reading: None,
            asked: false,
        }
    }

    /// Check once this run when `enabled`, unless the last check is less than a day old.
    pub fn start(&mut self, enabled: bool, now: u64) {
        self.start_using(enabled, now, download);
    }

    /// [`Self::start`], with `fetch` reading the address on the reading thread.
    pub fn start_using(
        &mut self,
        enabled: bool,
        now: u64,
        fetch: impl FnOnce(&str) -> Result<Vec<u8>, String> + Send + 'static,
    ) {
        let Some(source) = self.source.clone() else {
            return;
        };
        if self.asked || !enabled {
            return;
        }
        self.asked = true;
        if self.cache.is_fresh(now) {
            return;
        }
        // The attempt counts, answered or not: at most one a day.
        self.cache.checked_at = now;
        self.save();
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("update-check".into())
            .spawn(move || {
                let got = fetch(&update::release_json_url(&source))
                    .and_then(|bytes| update::parse_release(&bytes, &source));
                let _ = tx.send(got);
            });
        match spawned {
            Ok(_) => self.reading = Some(rx),
            Err(e) => tracing::info!("the update check did not start: {e}"),
        }
    }

    /// Take in a read that finished.
    pub fn poll(&mut self) {
        let Some(rx) = &self.reading else {
            return;
        };
        let got = match rx.try_recv() {
            Ok(got) => got,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("the read stopped".to_owned()),
        };
        self.reading = None;
        match got {
            Ok(release) => {
                tracing::info!("the latest Pyreal release is {}", release.version);
                self.cache.latest = Some(release);
                self.save();
            }
            Err(e) => tracing::info!("the update check found nothing: {e}"),
        }
    }

    /// The newer release to tell the player about, when checking is `enabled`.
    #[must_use]
    pub fn notice(&self, enabled: bool) -> Option<UpdateNotice> {
        if !enabled || self.source.is_none() {
            return None;
        }
        self.cache.notice(&self.current).map(|r| UpdateNotice {
            version: r.version.clone(),
        })
    }

    /// The page of the release the notice names.
    #[must_use]
    pub fn page(&self) -> Option<&str> {
        self.cache.notice(&self.current).map(|r| r.url.as_str())
    }

    fn save(&self) {
        if let Err(e) = self.cache.save(&self.dir) {
            tracing::info!("the update check's answer was not kept: {e}");
        }
    }

    /// Wait for a read under way to finish, and take it in.
    #[cfg(test)]
    fn wait(&mut self) {
        if let Some(rx) = self.reading.take() {
            let got = rx.recv().unwrap_or_else(|_| Err("the read stopped".into()));
            let (tx, back) = mpsc::channel();
            let _ = tx.send(got);
            self.reading = Some(back);
            self.poll();
        }
    }
}

/// The bytes at `url`: a plain GET naming only Pyreal and its version.
fn download(url: &str) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent(format!("Pyreal/{VERSION}"))
        .timeout_global(Some(std::time::Duration::from_secs(TIMEOUT_SECS)))
        .build()
        .into();
    let mut resp = agent.get(url).call().map_err(|e| e.to_string())?;
    resp.body_mut()
        .with_config()
        .limit(update::MAX_RELEASE_BYTES as u64)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    const REPO: &str = "https://github.com/jacobjohnston/pyreal";

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pyreal-update-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// A fake answer: `release.json` naming `version`, counting the reads in `reads`.
    fn answer(
        version: &'static str,
        reads: &Arc<AtomicUsize>,
    ) -> impl FnOnce(&str) -> Result<Vec<u8>, String> + Send + 'static {
        let reads = Arc::clone(reads);
        move |url| {
            assert_eq!(url, format!("{REPO}/releases/latest/download/release.json"));
            reads.fetch_add(1, Ordering::SeqCst);
            Ok(format!(
                r#"{{"schema":1,"product":"pyreal","version":"{version}","tag":"v{version}"}}"#
            )
            .into_bytes())
        }
    }

    #[test]
    fn a_newer_release_is_noticed_and_checked_once_a_day() {
        let d = tmp("daily");
        let reads = Arc::new(AtomicUsize::new(0));
        let mut check = UpdateCheck::open(&d, Some(REPO), "0.1.2");
        check.start_using(true, 1000, answer("0.1.3", &reads));
        check.wait();
        assert_eq!(check.notice(true).unwrap().version, "0.1.3");
        assert_eq!(check.page(), Some(&*format!("{REPO}/releases/tag/v0.1.3")));
        // Once a run.
        check.start_using(true, 1001, |_| panic!("checked twice in a run"));
        assert!(check.reading.is_none());
        // A later run within the day reads nothing, and keeps the answer.
        let mut later = UpdateCheck::open(&d, Some(REPO), "0.1.2");
        later.start_using(true, 1000 + update::MAX_AGE_SECS - 1, |_| {
            panic!("checked twice in a day")
        });
        assert!(later.reading.is_none());
        assert_eq!(later.notice(true).unwrap().version, "0.1.3");
        // A day on, it reads again.
        let mut next = UpdateCheck::open(&d, Some(REPO), "0.1.2");
        next.start_using(true, 1000 + update::MAX_AGE_SECS, answer("0.1.4", &reads));
        next.wait();
        assert_eq!(next.notice(true).unwrap().version, "0.1.4");
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn equal_or_older_says_nothing_and_a_failed_read_keeps_the_last_answer() {
        let d = tmp("same");
        let reads = Arc::new(AtomicUsize::new(0));
        let mut check = UpdateCheck::open(&d, Some(REPO), "0.1.3");
        check.start_using(true, 10, answer("0.1.3", &reads));
        check.wait();
        assert!(check.notice(true).is_none());
        let mut offline = UpdateCheck::open(&d, Some(REPO), "0.1.2");
        offline.start_using(true, 10 + update::MAX_AGE_SECS, |_| Err("offline".into()));
        offline.wait();
        assert_eq!(offline.notice(true).unwrap().version, "0.1.3");
        let mut garbage = UpdateCheck::open(&d, Some(REPO), "0.1.2");
        garbage.start_using(true, 10 + 2 * update::MAX_AGE_SECS, |_| {
            Ok(b"<html>rate limited</html>".to_vec())
        });
        garbage.wait();
        assert_eq!(garbage.notice(true).unwrap().version, "0.1.3");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_setting_turned_off_neither_checks_nor_shows() {
        let d = tmp("off");
        let reads = Arc::new(AtomicUsize::new(0));
        let mut check = UpdateCheck::open(&d, Some(REPO), "0.1.2");
        check.start_using(false, 1, |_| panic!("checked while off"));
        assert!(check.reading.is_none());
        assert!(!d.join(update::CACHE_FILE).exists());
        // Turned on later in the run, it checks then; turned off, the notice goes.
        check.start_using(true, 2, answer("0.1.3", &reads));
        check.wait();
        assert!(check.notice(true).is_some());
        assert!(check.notice(false).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_build_with_no_source_url_never_checks() {
        let d = tmp("local");
        let mut check = UpdateCheck::open(&d, None, "0.1.2");
        check.start_using(true, 1, |_| panic!("a local build checked"));
        assert!(check.reading.is_none());
        assert!(check.notice(true).is_none());
        assert!(!d.join(update::CACHE_FILE).exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
