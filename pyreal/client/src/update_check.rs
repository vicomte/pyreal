//! Whether a newer Pyreal is out: once a run, off the frame thread, the latest release's
//! `release.json` read from the website the build names ([`UPDATE_URL`]), or from its repository
//! ([`SOURCE_URL`]) when the site does not answer, at most once a day ([`pyreal_launch::update`]).
//! A build that names neither never checks. A read that fails is only logged.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use pyreal_horizon::front_door::UpdateNotice;
use pyreal_launch::update::{self, Feed, Release, UpdateCache};

/// The website's copy of the latest `release.json`, as a release build names it; none in a
/// local build.
pub const UPDATE_URL: Option<&str> = match option_env!("PYREAL_BUILD_UPDATE_URL") {
    Some(url) if !url.is_empty() => Some(url),
    _ => None,
};

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
    /// Where the release is read from, in order.
    feeds: Vec<Feed>,
    current: String,
    cache: UpdateCache,
    reading: Option<mpsc::Receiver<Result<Release, String>>>,
    /// Whether this run has checked, or found the last answer fresh.
    asked: bool,
}

impl UpdateCheck {
    /// The answer kept in `dir`, for a build of version `current` that reads the website's
    /// `release.json` at `site`, then the repository at `source`. Nothing is read yet.
    #[must_use]
    pub fn open(dir: &Path, site: Option<&str>, source: Option<&str>, current: &str) -> Self {
        Self {
            dir: dir.to_path_buf(),
            feeds: Feed::list(site, source),
            current: current.to_owned(),
            cache: UpdateCache::load(dir),
            reading: None,
            asked: false,
        }
    }

    /// Check once this run, unless the last check is less than a day old.
    pub fn start(&mut self, now: u64) {
        self.start_using(now, download);
    }

    /// [`Self::start`], with `fetch` reading an address on the reading thread.
    pub fn start_using(
        &mut self,
        now: u64,
        mut fetch: impl FnMut(&str) -> Result<Vec<u8>, String> + Send + 'static,
    ) {
        if self.feeds.is_empty() || self.asked {
            return;
        }
        self.asked = true;
        if self.cache.is_fresh(now) {
            return;
        }
        // The attempt counts, answered or not: at most one a day.
        self.cache.checked_at = now;
        self.save();
        let feeds = self.feeds.clone();
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("update-check".into())
            .spawn(move || {
                // The first feed that answers with a release; the repository stands in for the site.
                let mut failed = Vec::new();
                for feed in &feeds {
                    let url = feed.url();
                    match fetch(&url).and_then(|bytes| feed.parse(&bytes)) {
                        Ok(release) => {
                            let _ = tx.send(Ok(release));
                            return;
                        }
                        Err(e) => failed.push(format!("{url}: {e}")),
                    }
                }
                let _ = tx.send(Err(failed.join("; ")));
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

    /// The newer release to tell the player about.
    #[must_use]
    pub fn notice(&self) -> Option<UpdateNotice> {
        if self.feeds.is_empty() {
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

    const SITE: &str = "https://pyrealclient.app/release.json";
    const SITE_PAGE: &str = "https://pyrealclient.app/#download";
    const REPO: &str = "https://github.com/jacobjohnston/pyreal";

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pyreal-update-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn index(version: &str, page: &str) -> Vec<u8> {
        format!(
            r#"{{"schema":1,"product":"pyreal","version":"{version}","tag":"v{version}"{page}}}"#
        )
        .into_bytes()
    }

    /// A fake site whose `release.json` names `version`, counting the reads in `reads`.
    fn answer(
        version: &'static str,
        reads: &Arc<AtomicUsize>,
    ) -> impl FnMut(&str) -> Result<Vec<u8>, String> + Send + 'static {
        let reads = Arc::clone(reads);
        move |url| {
            assert_eq!(url, SITE, "the site is read first");
            reads.fetch_add(1, Ordering::SeqCst);
            Ok(index(version, &format!(r#","page":"{SITE_PAGE}""#)))
        }
    }

    /// A fake network: `site` answers the site's address, the repository's latest release names
    /// `version`, and each address read is added to `read`.
    fn network(
        site: Result<&'static [u8], &'static str>,
        version: &'static str,
        read: &Arc<std::sync::Mutex<Vec<String>>>,
    ) -> impl FnMut(&str) -> Result<Vec<u8>, String> + Send + 'static {
        let read = Arc::clone(read);
        move |url| {
            read.lock().unwrap().push(url.to_owned());
            if url == SITE {
                site.map(<[u8]>::to_vec).map_err(str::to_owned)
            } else {
                assert_eq!(url, format!("{REPO}/releases/latest/download/release.json"));
                Ok(index(version, ""))
            }
        }
    }

    #[test]
    fn a_newer_release_is_noticed_and_checked_once_a_day() {
        let d = tmp("daily");
        let reads = Arc::new(AtomicUsize::new(0));
        let mut check = UpdateCheck::open(&d, Some(SITE), Some(REPO), "0.1.2");
        check.start_using(1000, answer("0.1.3", &reads));
        check.wait();
        assert_eq!(check.notice().unwrap().version, "0.1.3");
        assert_eq!(check.page(), Some(SITE_PAGE));
        // Once a run.
        check.start_using(1001, |_| panic!("checked twice in a run"));
        assert!(check.reading.is_none());
        // A later run within the day reads nothing, and keeps the answer.
        let mut later = UpdateCheck::open(&d, Some(SITE), Some(REPO), "0.1.2");
        later.start_using(1000 + update::MAX_AGE_SECS - 1, |_| {
            panic!("checked twice in a day")
        });
        assert!(later.reading.is_none());
        assert_eq!(later.notice().unwrap().version, "0.1.3");
        // A day on, it reads again.
        let mut next = UpdateCheck::open(&d, Some(SITE), Some(REPO), "0.1.2");
        next.start_using(1000 + update::MAX_AGE_SECS, answer("0.1.4", &reads));
        next.wait();
        assert_eq!(next.notice().unwrap().version, "0.1.4");
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn equal_or_older_says_nothing_and_a_failed_read_keeps_the_last_answer() {
        let d = tmp("same");
        let reads = Arc::new(AtomicUsize::new(0));
        let mut check = UpdateCheck::open(&d, Some(SITE), Some(REPO), "0.1.3");
        check.start_using(10, answer("0.1.3", &reads));
        check.wait();
        assert!(check.notice().is_none());
        let mut offline = UpdateCheck::open(&d, Some(SITE), Some(REPO), "0.1.2");
        offline.start_using(10 + update::MAX_AGE_SECS, |_| Err("offline".into()));
        offline.wait();
        assert_eq!(offline.notice().unwrap().version, "0.1.3");
        let mut garbage = UpdateCheck::open(&d, Some(SITE), Some(REPO), "0.1.2");
        garbage.start_using(10 + 2 * update::MAX_AGE_SECS, |_| {
            Ok(b"<html>rate limited</html>".to_vec())
        });
        garbage.wait();
        assert_eq!(garbage.notice().unwrap().version, "0.1.3");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_repository_stands_in_when_the_site_does_not_answer() {
        let html: &[u8] = b"<html>Not found</html>";
        for (name, site) in [("down", Err("connection refused")), ("html", Ok(html))] {
            let d = tmp(&format!("fallback-{name}"));
            let read = Arc::new(std::sync::Mutex::new(Vec::new()));
            let mut check = UpdateCheck::open(&d, Some(SITE), Some(REPO), "0.2.0");
            check.start_using(1, network(site, "0.2.1", &read));
            check.wait();
            assert_eq!(check.notice().unwrap().version, "0.2.1", "{name}");
            assert_eq!(
                check.page(),
                Some(&*format!("{REPO}/releases/tag/v0.2.1")),
                "{name}: the page is the release's own"
            );
            assert_eq!(read.lock().unwrap().len(), 2, "{name}");
            let _ = std::fs::remove_dir_all(&d);
        }
    }

    #[test]
    fn a_site_that_answers_is_the_only_read() {
        let d = tmp("site-only");
        let read = Arc::new(std::sync::Mutex::new(Vec::new()));
        let same: &[u8] = br#"{"product":"pyreal","version":"0.2.0","tag":"v0.2.0"}"#;
        let mut check = UpdateCheck::open(&d, Some(SITE), Some(REPO), "0.2.0");
        check.start_using(1, network(Ok(same), "0.2.1", &read));
        check.wait();
        assert!(check.notice().is_none());
        assert_eq!(*read.lock().unwrap(), [SITE]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_stable_build_never_hears_of_a_pre_release_from_the_site() {
        let d = tmp("site-pre");
        let read = Arc::new(std::sync::Mutex::new(Vec::new()));
        let pre: &[u8] = br#"{"product":"pyreal","version":"0.3.0-rc.1","prerelease":true}"#;
        let mut check = UpdateCheck::open(&d, Some(SITE), Some(REPO), "0.2.0");
        check.start_using(1, network(Ok(pre), "0.2.0", &read));
        check.wait();
        assert!(check.notice().is_none());
        assert!(check.page().is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_build_naming_only_one_place_reads_only_there() {
        // A fork's release names only its repository.
        let d = tmp("repo-only");
        let read = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut check = UpdateCheck::open(&d, None, Some(REPO), "0.2.0");
        check.start_using(1, network(Err("unused"), "0.2.1", &read));
        check.wait();
        assert_eq!(check.notice().unwrap().version, "0.2.1");
        assert_eq!(read.lock().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&d);
        // A build naming only the site has nothing to fall back on.
        let d = tmp("site-alone");
        let read = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut check = UpdateCheck::open(&d, Some(SITE), None, "0.2.0");
        check.start_using(1, network(Err("offline"), "0.2.1", &read));
        check.wait();
        assert!(check.notice().is_none());
        assert_eq!(*read.lock().unwrap(), [SITE]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A server on this machine that answers each path in `routes` with its status and body.
    fn serve(routes: &'static [(&'static str, u16, &'static str)]) -> String {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut line = String::new();
                let mut reader = BufReader::new(&stream);
                let _ = reader.read_line(&mut line);
                let path = line.split(' ').nth(1).unwrap_or("");
                let (status, body) = routes
                    .iter()
                    .find(|(p, _, _)| *p == path)
                    .map_or((404, "<html>Not found</html>"), |(_, s, b)| (*s, *b));
                let _ = write!(
                    &stream,
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn the_real_read_falls_back_when_the_site_has_no_file() {
        let base = serve(&[(
            "/repo/releases/latest/download/release.json",
            200,
            r#"{"product":"pyreal","version":"0.2.1","tag":"v0.2.1"}"#,
        )]);
        let d = tmp("http-fallback");
        let site = format!("{base}/release.json");
        let repo = format!("{base}/repo");
        let mut check = UpdateCheck::open(&d, Some(&site), Some(&repo), "0.2.0");
        check.start_using(1, download);
        check.wait();
        assert_eq!(check.notice().unwrap().version, "0.2.1");
        assert_eq!(check.page(), Some(&*format!("{repo}/releases/tag/v0.2.1")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_real_read_takes_the_site_file() {
        let base = serve(&[(
            "/release.json",
            200,
            r#"{"product":"pyreal","version":"0.2.1","tag":"v0.2.1","prerelease":false}"#,
        )]);
        let d = tmp("http-site");
        let site = format!("{base}/release.json");
        let mut check = UpdateCheck::open(&d, Some(&site), None, "0.2.0");
        check.start_using(1, download);
        check.wait();
        assert_eq!(check.notice().unwrap().version, "0.2.1");
        assert_eq!(check.page(), Some(&*format!("{base}/")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_build_with_no_source_url_never_checks() {
        let d = tmp("local");
        let mut check = UpdateCheck::open(&d, None, None, "0.1.2");
        check.start_using(1, |_| panic!("a local build checked"));
        assert!(check.reading.is_none());
        assert!(check.notice().is_none());
        assert!(!d.join(update::CACHE_FILE).exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
