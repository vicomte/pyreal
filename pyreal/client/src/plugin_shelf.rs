//! The plugins the player installs, on the Plugins screen before any world: the shelf
//! ([`pyreal_launch::plugins::Shelf`]) in `plugin-shelf` in the settings folder, copied once from
//! the old launcher's `plugins` folder (which is only read). Its installed zips and folders are
//! offered to the plugin host, which is built again whenever they change; a plugin's tick is the
//! host's. Sources start empty: Dereth's own is gone. Downloads run off the frame's thread, one at
//! a time.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use pyreal_horizon::plugin_shelf::{
    AvailableRow, InstalledRow, PluginShelfRequest, PluginShelfView, SourceRow,
};
use pyreal_launch::plugins::{Availability, Download, Refresh, Shelf, FROM_FILE};
use pyreal_plugin_host::PluginHook;

/// The shelf's folder in the settings folder.
pub const SHELF_DIR_NAME: &str = "plugin-shelf";

/// What a download under way is for.
#[derive(Debug)]
enum Fetching {
    /// A source's list, just added.
    List(String),
    /// Every source again, then the updates that apply on their own.
    Refresh(Refresh),
    Install {
        source: String,
        id: String,
    },
    Update(String),
}

/// Where a download's bytes, or why there are none, arrive.
type Arriving = mpsc::Receiver<Result<Vec<u8>, String>>;

/// The host's side of the Plugins screen.
#[derive(Debug)]
pub struct PluginShelf {
    shelf: Shelf,
    status: Option<String>,
    fetching: Option<(Fetching, Arriving)>,
    /// The plugins the run was told to offer, before the shelf's.
    offered: Vec<PathBuf>,
    /// What the shelf offers changed: the host is built again.
    changed: bool,
}

impl PluginShelf {
    /// The shelf in `settings`, copied from the old launcher's folder `legacy` on the first run.
    /// `offered` is what the run offers besides.
    #[must_use]
    pub fn open(settings: &Path, legacy: Option<&Path>, offered: Vec<PathBuf>) -> Self {
        let dir = settings.join(SHELF_DIR_NAME);
        if !dir.exists() {
            if let Some(old) = legacy.map(|l| l.join("plugins")).filter(|d| d.is_dir()) {
                if let Err(e) = copy_whole(&old, &dir) {
                    tracing::warn!("the old launcher's plugins were not copied: {e}");
                }
            }
        }
        let mut shelf = Shelf::open(dir);
        shelf.drop_first_party();
        Self {
            shelf,
            status: None,
            fetching: None,
            offered,
            changed: false,
        }
    }

    /// The folders and zips the plugin host is offered: the run's, then the shelf's.
    #[must_use]
    pub fn offered(&self) -> Vec<PathBuf> {
        let mut out = self.offered.clone();
        out.extend(self.shelf.offered_packages());
        out
    }

    /// One frame's part: a download that finished taken in, the screen shown the shelf, what the
    /// player asked carried out, and the plugin host built again when what it offers changed.
    pub fn step(
        &mut self,
        app: &mut crate::app::App,
        plugins: Option<&mut crate::plugins::DesktopPlugins>,
    ) {
        self.poll();
        let found = app
            .frame_hook_mut()
            .and_then(|h| h.as_any_mut().downcast_mut::<PluginHook>())
            .map(|h| h.host.found());
        let view = self.view(found.as_deref());
        let requests = match app.shell_mut().horizon_mut() {
            Some(h) => {
                h.ui.worlds.plugins = Some(view);
                std::mem::take(&mut h.ui.worlds.plugin_requests)
            }
            None => return,
        };
        for r in requests {
            self.apply(app, r);
        }
        if std::mem::take(&mut self.changed) {
            if let Some(p) = plugins {
                crate::plugins::flush(app);
                p.offered_dirs = self.offered();
                p.install(app);
            }
        }
    }

    fn view(&self, found: Option<&[pyreal_plugin_host::FoundInfo]>) -> PluginShelfView {
        let v = self.shelf.view();
        let on = |id: &str| found.and_then(|f| f.iter().find(|p| p.id == id).map(|p| p.on));
        let mut installed: Vec<InstalledRow> = v
            .local
            .iter()
            .map(|l| InstalledRow {
                id: l.id.clone(),
                name: if l.name.is_empty() {
                    l.folder.clone()
                } else {
                    l.name.clone()
                },
                version: l.version.clone(),
                from: format!("Folder: {}", l.folder),
                on: on(&l.id),
                update: None,
                problem: l
                    .problem
                    .clone()
                    .or_else(|| l.needs_newer_pyreal.then(|| "Needs a newer Pyreal.".into()))
                    .or_else(|| {
                        l.id_taken
                            .then(|| "Another folder's plugin has its id.".into())
                    }),
                folder: Some(l.listed.clone()),
            })
            .collect();
        installed.extend(v.installed.iter().map(|p| {
            InstalledRow {
                id: p.id.clone(),
                name: p.name.clone(),
                version: p.version.clone(),
                from: if p.source == FROM_FILE {
                    "Installed from a file".to_owned()
                } else if p.source_removed {
                    format!("From {} (no longer a source)", p.source_name)
                } else {
                    format!("From {}", p.source_name)
                },
                on: on(&p.id),
                update: p
                    .update
                    .as_ref()
                    .filter(|u| !u.needs_newer_pyreal)
                    .map(|u| u.to.clone()),
                problem: p
                    .local_in_its_place
                    .then(|| "A folder's copy loads in its place.".into()),
                folder: None,
            }
        }));
        PluginShelfView {
            installed,
            sources: v
                .sources
                .iter()
                .map(|s| SourceRow {
                    url: s.url.clone(),
                    name: s.name.clone(),
                    plugins: s.plugins,
                    error: s.error.clone(),
                })
                .collect(),
            available: v
                .available
                .iter()
                .map(|a| AvailableRow {
                    source: a.source.clone(),
                    source_name: a.source_name.clone(),
                    id: a.id.clone(),
                    name: a.name.clone(),
                    version: a.version.clone(),
                    description: a.description.clone(),
                    problem: match a.state {
                        Availability::Install => None,
                        Availability::NeedsNewerPyreal => Some("Needs a newer Pyreal.".into()),
                        Availability::FirstPartyOnly => {
                            Some("Dereth's own: not from this source.".into())
                        }
                    },
                })
                .collect(),
            status: self.status.clone(),
            busy: self.fetching.is_some(),
            can_browse: cfg!(any(target_os = "macos", windows)),
        }
    }

    fn apply(&mut self, app: &mut crate::app::App, r: PluginShelfRequest) {
        use PluginShelfRequest as R;
        let said: Result<Option<String>, String> = match r {
            R::SetOn { id, on } => app
                .frame_hook_mut()
                .and_then(|h| h.as_any_mut().downcast_mut::<PluginHook>())
                .ok_or_else(|| "Plugins are off in this run.".to_owned())
                .and_then(|h| h.host.set_ticked(&id, on))
                .map(|()| None),
            R::Remove { id } => noted(&mut self.changed, self.shelf.remove(&id).map(|()| None)),
            R::RemoveFolder { folder } => noted(
                &mut self.changed,
                self.shelf.remove_local(Path::new(&folder)).map(|()| None),
            ),
            R::InstallFile => match pick_zip() {
                Some(file) => match std::fs::read(&file) {
                    Ok(zip) => {
                        let name = file.display().to_string();
                        noted(
                            &mut self.changed,
                            self.shelf.install_file(&name, &zip).map(|d| Some(d.said())),
                        )
                    }
                    Err(e) => Err(format!("{}: {e}", file.display())),
                },
                None => Ok(None),
            },
            R::AddFolder => match pick_folder() {
                Some(folder) => noted(
                    &mut self.changed,
                    self.shelf
                        .add_local(&folder)
                        .map(|n| Some(format!("Added {n} plugin(s) from {}.", folder.display()))),
                ),
                None => Ok(None),
            },
            R::AddSource { url } => {
                self.shelf
                    .add_source(&url)
                    .map_err(|e| e.to_string())
                    .map(|url| {
                        self.fetch(
                            Fetching::List(url.clone()),
                            Download {
                                url,
                                limit: pyreal_launch::plugins::MAX_LIST,
                            },
                        );
                        Some("Reading the source's list...".to_owned())
                    })
            }
            R::RemoveSource { url } => self
                .shelf
                .remove_source(&url)
                .map(|()| None)
                .map_err(|e| e.to_string()),
            R::Refresh => {
                let mut walk = Refresh::new(&self.shelf);
                match walk.next(&mut self.shelf, None, now()) {
                    Some(d) => {
                        self.fetch(Fetching::Refresh(walk), d);
                        Ok(Some("Reading the sources...".to_owned()))
                    }
                    None => Ok(Some(walk.said(&self.shelf))),
                }
            }
            R::Install { source, id } => self
                .shelf
                .plan_install(&source, &id)
                .map(|d| {
                    self.fetch(Fetching::Install { source, id }, d);
                    Some("Downloading...".to_owned())
                })
                .map_err(|e| e.to_string()),
            R::Update { id } => self
                .shelf
                .plan_update(&id)
                .map(|d| {
                    self.fetch(Fetching::Update(id), d);
                    Some("Downloading...".to_owned())
                })
                .map_err(|e| e.to_string()),
        };
        match said {
            Ok(Some(s)) => self.status = Some(s),
            Ok(None) => self.status = None,
            Err(e) => {
                tracing::warn!("plugins: {e}");
                self.status = Some(e);
            }
        }
    }

    /// Start downloading `d`, for `what`.
    fn fetch(&mut self, what: Fetching, d: Download) {
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("plugin-shelf".into())
            .spawn(move || {
                let _ = tx.send(download(&d.url, d.limit));
            });
        match spawned {
            Ok(_) => self.fetching = Some((what, rx)),
            Err(e) => self.status = Some(e.to_string()),
        }
    }

    /// Take in a download that finished.
    fn poll(&mut self) {
        let Some((_, rx)) = &self.fetching else {
            return;
        };
        let got = match rx.try_recv() {
            Ok(got) => got,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("the download stopped".to_owned()),
        };
        let Some((what, _)) = self.fetching.take() else {
            return;
        };
        let said = match what {
            Fetching::List(url) => match self.shelf.record_read(&url, got, now()) {
                Ok(new) => {
                    let lines = self.shelf.said_of_read(&url, &new);
                    let listed = self
                        .shelf
                        .source(&url)
                        .map_or(0, |s| s.catalog.plugins.len());
                    if lines.is_empty() {
                        format!("The source lists {listed} plugin(s).")
                    } else {
                        lines.join(" ")
                    }
                }
                Err(e) => e.to_string(),
            },
            Fetching::Refresh(mut walk) => match walk.next(&mut self.shelf, Some(got), now()) {
                Some(d) => {
                    self.fetch(Fetching::Refresh(walk), d);
                    return;
                }
                None => {
                    self.changed = true;
                    let said = walk.said(&self.shelf).replace('\n', " ");
                    if said.is_empty() {
                        "Nothing new.".to_owned()
                    } else {
                        said
                    }
                }
            },
            Fetching::Install { source, id } => {
                let done = got.and_then(|zip| {
                    self.shelf
                        .install(&source, &id, &zip)
                        .map_err(|e| e.to_string())
                });
                self.changed |= done.is_ok();
                done.map_or_else(|e| e, |d| d.said())
            }
            Fetching::Update(id) => {
                let done =
                    got.and_then(|zip| self.shelf.update(&id, &zip).map_err(|e| e.to_string()));
                self.changed |= done.is_ok();
                done.map_or_else(|e| e, |d| d.said())
            }
        };
        self.status = Some(said);
    }
}

/// `done`, noting in `changed` that what the host is offered changed when it went through.
fn noted<E: std::fmt::Display>(
    changed: &mut bool,
    done: Result<Option<String>, E>,
) -> Result<Option<String>, String> {
    *changed |= done.is_ok();
    done.map_err(|e| e.to_string())
}

/// Seconds since the epoch.
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The bytes at `url`, at most `limit` of them.
fn download(url: &str, limit: usize) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent(concat!("pyreal/", env!("CARGO_PKG_VERSION")))
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build()
        .into();
    let mut resp = agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
    resp.body_mut()
        .with_config()
        .limit(limit as u64)
        .read_to_vec()
        .map_err(|e| format!("{url}: {e}"))
}

/// Copy the folder `from` to `to` beside it first, so a copy that stops part-way leaves no `to`
/// and is tried again on the next run.
fn copy_whole(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut name = to.file_name().unwrap_or_default().to_os_string();
    name.push(".copying");
    let partial = to.with_file_name(name);
    let _ = std::fs::remove_dir_all(&partial);
    let copied = copy_dir(from, &partial).and_then(|()| std::fs::rename(&partial, to));
    if copied.is_err() {
        let _ = std::fs::remove_dir_all(&partial);
    }
    copied
}

/// Copy the folder `from` to `to`, files and folders.
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let at = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &at)?;
        } else {
            std::fs::copy(entry.path(), at)?;
        }
    }
    Ok(())
}

/// The system's file picker, for a plugin package's zip.
#[cfg(any(target_os = "macos", windows))]
fn pick_zip() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("A plugin package")
        .add_filter("Plugin package", &["zip"])
        .pick_file()
}

/// The system's folder picker, for a plugin's folder or a folder of plugins.
#[cfg(any(target_os = "macos", windows))]
fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("A plugin's folder, or a folder of plugins")
        .pick_folder()
}

/// No picker on this system.
#[cfg(not(any(target_os = "macos", windows)))]
fn pick_zip() -> Option<PathBuf> {
    None
}

/// No picker on this system.
#[cfg(not(any(target_os = "macos", windows)))]
fn pick_folder() -> Option<PathBuf> {
    None
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A first run's copy of the old launcher's plugins that stops part-way is tried again on the
    /// next run, not left half done for good.
    #[test]
    fn a_first_run_copy_that_stops_part_way_is_tried_again() {
        use std::os::unix::fs::PermissionsExt as _;
        let root = std::env::temp_dir().join(format!("pyreal-shelf-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (settings, launcher) = (root.join("settings"), root.join("launcher"));
        let old = launcher.join("plugins");
        std::fs::create_dir_all(old.join("a")).unwrap();
        std::fs::write(old.join("a/one.lua"), "-- one").unwrap();
        std::fs::create_dir_all(old.join("b")).unwrap();
        std::fs::write(old.join("b/two.lua"), "-- two").unwrap();
        // Unreadable: the copy stops at it.
        std::fs::set_permissions(
            old.join("b/two.lua"),
            std::fs::Permissions::from_mode(0o000),
        )
        .unwrap();
        let _ = PluginShelf::open(&settings, Some(&launcher), Vec::new());
        std::fs::set_permissions(
            old.join("b/two.lua"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let _ = PluginShelf::open(&settings, Some(&launcher), Vec::new());
        let copied = settings.join(SHELF_DIR_NAME);
        assert_eq!(
            std::fs::read_to_string(copied.join("a/one.lua")).unwrap(),
            "-- one"
        );
        assert_eq!(
            std::fs::read_to_string(copied.join("b/two.lua")).unwrap(),
            "-- two"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
