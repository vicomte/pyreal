//! The list of known worlds on the desktop: the day's copy in the settings folder
//! ([`pyreal_launch::serverlist`]), read again off the frame thread when it is old or the player
//! asks. A read that fails leaves the copy as it was.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use pyreal_horizon::front_door::{DirectoryView, ListedRow};
use pyreal_launch::serverlist::{self, ListCache};
use pyreal_launch::state::CustomWorld;
use pyreal_launch::world::Game;
use pyreal_launch::World;

/// The list of known worlds and its copy.
#[derive(Debug)]
pub struct WorldDirectory {
    dir: PathBuf,
    copy: Option<ListCache>,
    /// This game's worlds in the copy, each with an address.
    worlds: Vec<World>,
    reading: Option<mpsc::Receiver<Result<ListCache, String>>>,
    /// Whether the last read failed.
    failed: bool,
}

impl WorldDirectory {
    /// The copy kept in `dir`, if there is one. Nothing is read from the network yet.
    #[must_use]
    pub fn open(dir: &Path) -> Self {
        let mut me = Self {
            dir: dir.to_path_buf(),
            copy: None,
            worlds: Vec::new(),
            reading: None,
            failed: false,
        };
        if let Some(copy) = ListCache::load(dir) {
            me.keep(copy);
        }
        me
    }

    fn keep(&mut self, copy: ListCache) {
        self.worlds = copy
            .worlds()
            .unwrap_or_default()
            .into_iter()
            .filter(|w| w.game == Game::Dereth && w.endpoint.is_some())
            .collect();
        self.copy = Some(copy);
    }

    /// Read the list from the network when the copy is old or missing, or always when `again`.
    pub fn read(&mut self, again: bool, now: u64) {
        self.read_using(again, now, download);
    }

    /// [`Self::read`], with `fetch` reading each address on the reading thread.
    pub fn read_using(
        &mut self,
        again: bool,
        now: u64,
        fetch: impl FnMut(&str) -> Result<Vec<u8>, String> + Send + 'static,
    ) {
        if self.reading.is_some() || (!again && self.copy.as_ref().is_some_and(|c| c.is_fresh(now)))
        {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("world-list".into())
            .spawn(move || {
                let _ = tx.send(serverlist::read_list(fetch, now));
            });
        match spawned {
            Ok(_) => self.reading = Some(rx),
            Err(e) => {
                tracing::info!("the world list was not read: {e}");
                self.failed = true;
            }
        }
    }

    /// Take in a read that finished: its copy kept and saved, or the old copy left as it was.
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
            Ok(copy) => {
                if let Err(e) = copy.save(&self.dir) {
                    tracing::info!("the world list's copy was not kept: {e}");
                }
                self.keep(copy);
                self.failed = false;
            }
            Err(e) => {
                tracing::info!("the world list was not read: {e}");
                self.failed = true;
            }
        }
    }

    /// The known world listed as `slug`.
    #[must_use]
    pub fn world(&self, slug: &str) -> Option<&World> {
        self.worlds.iter().find(|w| w.slug == slug)
    }

    /// What the Known Worlds screen shows: each world, and whether `mine` has it already.
    #[must_use]
    pub fn view(&self, mine: &[CustomWorld], now: u64) -> DirectoryView {
        let worlds = self
            .worlds
            .iter()
            .filter_map(|w| {
                let ep = w.endpoint.as_ref()?;
                Some(ListedRow {
                    slug: w.slug.clone(),
                    name: w.name.clone(),
                    host: ep.address.clone(),
                    port: ep.port,
                    emulator: w.emulator.label().to_owned(),
                    ruleset: w.ruleset.clone(),
                    maturity: w.development_status.clone(),
                    description: w.description.clone(),
                    added: mine
                        .iter()
                        .any(|m| m.host.eq_ignore_ascii_case(&ep.address) && m.port == ep.port),
                })
            })
            .collect();
        let note = self.copy.as_ref().map(|c| {
            let from = if c.is_hubs() {
                "the Dereth Network"
            } else {
                "the community list"
            };
            let mut note = format!(
                "Read from {from} {}.",
                ago(now.saturating_sub(c.fetched_at))
            );
            if self.failed {
                note.push_str(" It could not be read again just now.");
            }
            note
        });
        DirectoryView {
            worlds,
            reading: self.reading.is_some(),
            note,
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

/// How long ago, in words, `secs` seconds is.
fn ago(secs: u64) -> String {
    let (n, unit) = match secs {
        0..60 => return "just now".to_owned(),
        60..3600 => (secs / 60, "minute"),
        3600..86_400 => (secs / 3600, "hour"),
        _ => (secs / 86_400, "day"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

/// The bytes at `url`, up to the most a list is read to.
fn download(url: &str) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent(concat!("pyreal/", env!("CARGO_PKG_VERSION")))
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .build()
        .into();
    let mut resp = agent.get(url).call().map_err(|e| e.to_string())?;
    resp.body_mut()
        .with_config()
        .limit(serverlist::MAX_LIST_BYTES as u64)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("pyreal-directory-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn list_of(names: &[&str]) -> Vec<u8> {
        let rows: String = names
            .iter()
            .map(|n| {
                format!(
                    "<ServerItem><name>{n}</name><emu>ACE</emu><server_host>{n}.example</server_host>\
                     <server_port>9000</server_port><type>PvE</type></ServerItem>"
                )
            })
            .collect();
        format!("<ArrayOfServerItem>{rows}</ArrayOfServerItem>").into_bytes()
    }

    /// A fetch where only the community list answers, with `names`.
    fn community(names: &'static [&'static str]) -> impl FnMut(&str) -> Result<Vec<u8>, String> {
        move |url| {
            if url == serverlist::COMMUNITY_LIST_URL {
                Ok(list_of(names))
            } else {
                Err("no answer".into())
            }
        }
    }

    #[test]
    fn a_read_is_kept_and_a_failed_one_leaves_the_copy() {
        let d = tmp("keep");
        let mut dir = WorldDirectory::open(&d);
        assert!(dir.view(&[], 100).worlds.is_empty());
        dir.read_using(false, 100, community(&["Alpha", "Beta"]));
        assert!(dir.view(&[], 100).reading);
        dir.wait();
        let v = dir.view(&[], 160);
        assert_eq!(v.worlds.len(), 2);
        assert_eq!(v.worlds[0].name, "Alpha");
        assert_eq!(v.worlds[0].emulator, "ACE");
        assert_eq!(
            v.note.as_deref(),
            Some("Read from the community list 1 minute ago.")
        );
        // The copy is fresh, so opening the screen does not read again.
        dir.read_using(false, 200, |_| panic!("not read while the copy is fresh"));
        assert!(!dir.view(&[], 200).reading);
        // Asked again, a read that fails keeps the copy and says so quietly.
        dir.read_using(true, 200, |_| Err("offline".into()));
        dir.wait();
        let v = dir.view(&[], 200);
        assert_eq!(v.worlds.len(), 2);
        assert!(v
            .note
            .unwrap()
            .ends_with("It could not be read again just now."));
        // The copy was saved, and a later run starts from it.
        let again = WorldDirectory::open(&d);
        assert_eq!(again.view(&[], 200).worlds.len(), 2);
        assert!(again.world("beta").is_some());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_old_copy_is_read_again_and_a_world_on_the_list_shows_as_added() {
        let d = tmp("old");
        let mut dir = WorldDirectory::open(&d);
        dir.read_using(false, 0, community(&["Alpha"]));
        dir.wait();
        dir.read_using(
            false,
            serverlist::MAX_AGE_SECS,
            community(&["Alpha", "Gamma"]),
        );
        dir.wait();
        let mine = [CustomWorld {
            slug: "x".into(),
            name: "Mine".into(),
            host: "GAMMA.example".into(),
            port: 9000,
            ruleset: None,
            emulator: pyreal_launch::Emulator::Unknown,
            status_url: None,
            imported: false,
        }];
        let v = dir.view(&mine, serverlist::MAX_AGE_SECS);
        let added: Vec<_> = v
            .worlds
            .iter()
            .map(|w| (w.name.as_str(), w.added))
            .collect();
        assert_eq!(added, [("Alpha", false), ("Gamma", true)]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn ago_says_the_largest_whole_unit() {
        assert_eq!(ago(5), "just now");
        assert_eq!(ago(60), "1 minute ago");
        assert_eq!(ago(7200), "2 hours ago");
        assert_eq!(ago(3 * 86_400), "3 days ago");
    }
}
