//! The world's weenie list for the Admin window's Find and Spawn: fetched from the world's
//! `/weenies` once it is first wanted, on a thread of its own, and kept until the world changes.
//!
//! The address is the world's status address (`--world-status`) with `/weenies` for its path. A
//! world on this machine with no status address given is asked at its game port plus 100, where
//! a local Empyrean world's status endpoint sits. A list that couldn't be had is asked for again
//! after [`ASK_AGAIN`], so a world that starts publishing is found without a restart.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pyreal_client_model::cmd::weenies::WeenieList;

/// How long a failed list is kept before it is asked for again.
pub const ASK_AGAIN: Duration = Duration::from_secs(20);

/// A way to fetch a URL's text, given by the host (the desktop client's HTTP client).
pub type FetchText = Arc<dyn Fn(&str) -> Result<String, String> + Send + Sync>;

/// Where the list stands.
#[derive(Debug, Clone)]
pub enum WeenieListState {
    Fetching,
    Ready(Arc<WeenieList>),
    /// Why there is none, for the window to say.
    Failed(String),
}

/// The list of the world last asked about.
#[derive(Default)]
pub struct WeenieLists {
    /// The host's fetcher; with none, every list fails.
    pub fetch: Option<FetchText>,
    url: Option<String>,
    asked: Option<Instant>,
    slot: Arc<Mutex<Option<WeenieListState>>>,
}

impl std::fmt::Debug for WeenieLists {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WeenieLists")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

impl WeenieLists {
    /// The list at `url`, its fetch begun the first time it is asked for. `None` asks for no
    /// list, and says why.
    pub fn want(&mut self, url: Result<String, String>) -> WeenieListState {
        let url = match url {
            Ok(url) => url,
            Err(why) => return WeenieListState::Failed(why),
        };
        let stale = matches!(self.state(), WeenieListState::Failed(_))
            && self.asked.is_some_and(|t| t.elapsed() >= ASK_AGAIN);
        if self.url.as_deref() != Some(url.as_str()) || stale {
            self.url = Some(url.clone());
            self.asked = Some(Instant::now());
            self.slot = Arc::new(Mutex::new(Some(WeenieListState::Fetching)));
            let Some(fetch) = self.fetch.clone() else {
                self.put(WeenieListState::Failed(
                    "This client can't fetch the list.".to_owned(),
                ));
                return self.state();
            };
            let slot = Arc::clone(&self.slot);
            let spawned = std::thread::Builder::new()
                .name("Weenie List".to_owned())
                .spawn(move || {
                    let state = match fetch(&url).and_then(|t| WeenieList::parse(&t)) {
                        Ok(list) => WeenieListState::Ready(Arc::new(list)),
                        Err(e) => {
                            tracing::warn!("weenie list from {url}: {e}");
                            WeenieListState::Failed(
                                "This world doesn't publish its list of things to make.".to_owned(),
                            )
                        }
                    };
                    if let Ok(mut s) = slot.lock() {
                        *s = Some(state);
                    }
                });
            if spawned.is_err() {
                self.put(WeenieListState::Failed(
                    "The list couldn't be fetched.".to_owned(),
                ));
            }
        }
        self.state()
    }

    /// Asks again, the next time the list is wanted.
    pub fn forget(&mut self) {
        self.url = None;
    }

    fn put(&self, state: WeenieListState) {
        if let Ok(mut s) = self.slot.lock() {
            *s = Some(state);
        }
    }

    fn state(&self) -> WeenieListState {
        self.slot
            .lock()
            .ok()
            .and_then(|s| s.clone())
            .unwrap_or(WeenieListState::Fetching)
    }
}

/// Where the list of the world at `host`:`port` is: beside `status`, the world's status address,
/// or for a world on this machine, at its game port plus 100.
///
/// # Errors
/// Neither is known, with what to say.
pub fn list_url(status: Option<&str>, host: &str, port: u32) -> Result<String, String> {
    beside_status(status, host, port, "weenies")
}

/// Where the world at `host`:`port` publishes `leaf` (`weenies`, `places`): beside `status`, the
/// world's status address, or for a world on this machine, at its game port plus 100.
///
/// # Errors
/// Neither is known, with what to say.
pub fn beside_status(
    status: Option<&str>,
    host: &str,
    port: u32,
    leaf: &str,
) -> Result<String, String> {
    if let Some(status) = status.map(str::trim).filter(|s| !s.is_empty()) {
        let (scheme, rest) = status.split_once("://").unwrap_or(("http", status));
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        let path = &rest[authority.len()..];
        let path = path.split(['?', '#']).next().unwrap_or("");
        let base = path
            .strip_suffix("/v1/world")
            .or_else(|| path.strip_suffix("/status"))
            .unwrap_or(path.trim_end_matches('/'));
        return Ok(format!("{scheme}://{authority}{base}/{leaf}"));
    }
    let local = matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]");
    if local && port > 0 && port + 100 <= 65535 {
        let host = if host == "::1" { "[::1]" } else { host };
        return Ok(format!("http://{host}:{}/{leaf}", port + 100));
    }
    Err("This world's status address isn't known.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_sits_beside_the_status_document_or_a_hundred_ports_up_on_this_machine() {
        let url = |s: Option<&str>, h: &str, p: u32| list_url(s, h, p).ok();
        assert_eq!(
            url(Some("http://127.0.0.1:9110/status"), "x", 1).as_deref(),
            Some("http://127.0.0.1:9110/weenies")
        );
        assert_eq!(
            url(Some("https://play.example.org/game/v1/world"), "x", 1).as_deref(),
            Some("https://play.example.org/game/weenies")
        );
        assert_eq!(
            url(Some("https://play.example.org"), "x", 1).as_deref(),
            Some("https://play.example.org/weenies")
        );
        assert_eq!(
            url(None, "127.0.0.1", 9020).as_deref(),
            Some("http://127.0.0.1:9120/weenies")
        );
        assert_eq!(url(None, "play.example.org", 9000), None);
        assert_eq!(
            beside_status(None, "127.0.0.1", 9020, "places")
                .ok()
                .as_deref(),
            Some("http://127.0.0.1:9120/places")
        );
    }

    #[test]
    fn a_list_is_fetched_once_per_world_and_a_failure_is_said() {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&asked);
        let mut lists = WeenieLists {
            fetch: Some(Arc::new(move |url: &str| {
                log.lock().unwrap().push(url.to_owned());
                if url.contains("9100") {
                    Ok("dereth-weenie-list\t1\nworld\tW\nweenie\t7\tskulker\t10\t16\t3\t8\t-\tSkulker\n".to_owned())
                } else {
                    Err("refused".to_owned())
                }
            })),
            ..WeenieLists::default()
        };
        let ready = |lists: &mut WeenieLists, url: &str| {
            for _ in 0..500 {
                match lists.want(Ok(url.to_owned())) {
                    WeenieListState::Fetching => {
                        std::thread::sleep(std::time::Duration::from_millis(2))
                    }
                    s => return s,
                }
            }
            WeenieListState::Fetching
        };
        let WeenieListState::Ready(list) = ready(&mut lists, "http://127.0.0.1:9100/weenies")
        else {
            panic!("the list");
        };
        assert_eq!(list.weenies.len(), 1);
        assert!(matches!(
            ready(&mut lists, "http://127.0.0.1:9200/weenies"),
            WeenieListState::Failed(_)
        ));
        assert_eq!(asked.lock().unwrap().len(), 2, "each world once");
        lists.asked = Instant::now().checked_sub(ASK_AGAIN);
        assert!(matches!(
            ready(&mut lists, "http://127.0.0.1:9200/weenies"),
            WeenieListState::Failed(_)
        ));
        assert_eq!(
            asked.lock().unwrap().len(),
            3,
            "a failed list is asked for again after a while"
        );
        assert!(matches!(
            lists.want(Err("unknown".to_owned())),
            WeenieListState::Failed(w) if w == "unknown"
        ));
    }
}
