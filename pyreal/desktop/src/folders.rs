//! **Where the client keeps its own files**, and the first-run copy of the original game's.
//!
//! Everything the player saves lands in one directory, the *settings directory*, through the
//! parent of the preferences file:
//!
//! | file | built by |
//! |---|---|
//! | `UserPreferences.ini` | [`default_preferences_file`] |
//! | `dereth-modern.keymap` and the saved `<name>-modern.keymap` | the front end's `keymap_path_for` |
//! | `ScreenShot%05d.png` | `pyreal_client::app::App::screenshot_path` |
//! | `Journal-<world>-<char>.txt` | `pyreal_client_contract::journal::JournalIdentity::client_path` |
//!
//! The crash logs are here too, in [`crash_log_dir`], though they are written before anything else
//! starts: the directory is computed from the environment alone, so a log about a start-up that
//! failed does not depend on start-up having succeeded. A run that ends cleanly removes its own,
//! and each start keeps only the newest [`CRASH_LOGS_KEPT`] ([`prune_crash_logs`]). `@log`'s chat
//! log is the one write that keeps the original rule and is resolved against the working directory.
//!
//! # The per-platform place
//!
//! Pyreal's state lives in one folder per platform, the client's settings directory itself:
//!
//! | platform | the client's settings directory |
//! |---|---|
//! | Windows | `%APPDATA%\Pyreal` |
//! | macOS | `~/Library/Application Support/Pyreal` |
//! | Linux and other Unix | `$XDG_CONFIG_HOME/pyreal`, or `~/.config/pyreal` |
//!
//! The folder is named for this client and not for the original game (client divergence CD-007):
//! this client's files are not the original's, and a folder of its own keeps them from sharing a
//! directory with a retail installation. The client keeps nothing large or rebuildable, so it has
//! no cache folder.
//!
//! **`PYREAL_SETTINGS_DIR`** ([`SETTINGS_DIR_ENV`]) names the running product's settings directory
//! outright, in place of every rule in this section and of a `UserPreferences.ini` in the working
//! directory; only `-prefs` still wins over it. A relative value is taken against the working
//! directory. It is how a test, a script or a second copy of the client runs with folders of its
//! own and leaves the player's untouched, crash logs included. Its old name, `DERETH_SETTINGS_DIR`
//! ([`OLD_ENV_NAMES`]), is read when it is unset.
//!
//! On Windows `%APPDATA%` is the roaming profile folder, so a player's settings follow them the way
//! the rest of their profile does; a program that ships as a zip keeps nothing beside itself.
//! On Linux the base is the XDG base directory specification's, whose rule for a relative or unset
//! `XDG_CONFIG_HOME` is to ignore it and fall back.
//!
//! This is the desktop host's question: resolving a place from the environment is platform I/O,
//! and the runtime below takes the answer as a path.
//!
//! # The settings it starts from
//!
//! The first run **copies** settings in ([`copy_settings_from`]) when the client's directory
//! still holds nothing: the old Dereth client's folder (`Dereth/client` in [`legacy_state_root`])
//! as it is, else on Windows the original game's `Documents\Asheron's Call`, whose key maps come
//! in as the modern interface's (`<name>-modern.keymap`). Neither is changed. A run given its
//! folder by [`SETTINGS_DIR_ENV`] copies nothing in ([`import_source`]): that folder is the run's
//! own. The old launcher's folder beside it ([`legacy_launcher_dir`]) is where the client's world
//! list is first imported from.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use pyreal_client_runtime::config::{Config, ConfigError, PREFERENCES_FILE_NAME};

/// The client's folder inside the state root: the root itself.
pub const CLIENT_DIR_NAME: &str = "";

/// The old client's folder inside the old state root, which the first run copies.
pub const LEGACY_CLIENT_DIR_NAME: &str = "client";

/// The old launcher's folder inside the old state root.
pub const LEGACY_LAUNCHER_DIR_NAME: &str = "launcher";

/// The folder inside the settings directory that holds the crash logs, one
/// `pyreal-<pid>.log` per run.
pub const CRASH_LOG_DIR_NAME: &str = "crash-logs";

/// How many crash logs [`prune_crash_logs`] keeps at each start.
pub const CRASH_LOGS_KEPT: usize = 20;

/// The environment variable that names the settings directory outright. See the module
/// documentation.
pub const SETTINGS_DIR_ENV: &str = "PYREAL_SETTINGS_DIR";

/// For a developer's run from a build folder or a test: names an art pack, a packed file or its
/// folder, in place of the one chosen.
pub const ART_PACK_ENV: &str = "PYREAL_ART_PACK";

/// The base art pack built into the client: in the macOS bundle's `Contents/Resources`, and
/// beside the executable elsewhere ([`installed_base_pack`]); drawn under the pack chosen.
pub const BASE_PACK_FILE: &str = "base-pack.dat";

/// For a developer's run: names the base pack, a packed file or its folder, or `none` for no base
/// pack.
pub const BASE_PACK_ENV: &str = "PYREAL_BASE_PACK";

/// The folder beside a checkout's build folders that `cargo xtask base-pack build` writes the base
/// pack into (`target/base-pack`), read by a client run from `target/<profile>`.
pub const CHECKOUT_BASE_PACK_DIR: &str = "base-pack";

/// Where the client installed at `exe` keeps its base pack: `Contents/Resources` when `exe` is in
/// a bundle's `Contents/MacOS`, else beside `exe`.
#[must_use]
pub fn installed_base_pack(exe: &Path) -> PathBuf {
    let dir = exe.parent().unwrap_or_else(|| Path::new("."));
    if dir.file_name().is_some_and(|n| n == "MacOS") {
        if let Some(contents) = dir.parent() {
            return contents.join("Resources").join(BASE_PACK_FILE);
        }
    }
    dir.join(BASE_PACK_FILE)
}

/// The base pack a run draws: the one `env`'s [`BASE_PACK_ENV`] names (none for `none`), else the
/// installed one beside `exe`, else a checkout's in `target/base-pack`; none where there is none.
fn base_pack(env: Env<'_>, exe: Option<&Path>) -> Option<PathBuf> {
    if let Some(v) = env(BASE_PACK_ENV).filter(|v| !v.is_empty()) {
        return (v != "none").then(|| PathBuf::from(v));
    }
    let exe = exe?;
    let installed = installed_base_pack(exe);
    if installed.exists() {
        return Some(installed);
    }
    let checkout = exe
        .parent()?
        .parent()?
        .join(CHECKOUT_BASE_PACK_DIR)
        .join(BASE_PACK_FILE);
    checkout.exists().then_some(checkout)
}

/// The original game's settings directory under `Documents`, which a retail installation writes.
pub const RETAIL_SETTINGS_DIR_NAME: &str = "Asheron's Call";

/// The environment, as the resolvers below read it: one variable by name.
type Env<'a> = &'a dyn Fn(&str) -> Option<OsString>;

/// A variable's value as a path, when it is set, non-empty and absolute. A relative value is no
/// answer at all: it would resolve against whatever the working directory happens to be.
fn absolute(env: Env<'_>, name: &str) -> Option<PathBuf> {
    env(name).map(PathBuf::from).filter(|p| p.is_absolute())
}

/// The state root's folder name: `Pyreal`, and `pyreal` where names are lower case (XDG).
pub const STATE_ROOT_NAME: &str = "Pyreal";

/// The old Dereth launcher's and client's state root's folder name, which the first run copies
/// from ([`legacy_state_root`]).
pub const LEGACY_STATE_ROOT_NAME: &str = "Dereth";

/// Windows' state root, `%APPDATA%\<name>`, or `%USERPROFILE%\AppData\Roaming\<name>` when
/// `APPDATA` is unset. `None` when neither is.
fn windows_state_root(env: Env<'_>, name: &str) -> Option<PathBuf> {
    absolute(env, "APPDATA")
        .or_else(|| absolute(env, "USERPROFILE").map(|h| h.join("AppData").join("Roaming")))
        .map(|base| base.join(name))
}

/// macOS' state root, `~/Library/Application Support/<name>`.
fn macos_state_root(env: Env<'_>, name: &str) -> Option<PathBuf> {
    absolute(env, "HOME").map(|h| h.join("Library").join("Application Support").join(name))
}

/// The XDG state root, `$XDG_CONFIG_HOME/<name>`, or `~/.config/<name>`. Lower case, as the
/// convention there is.
fn xdg_state_root(env: Env<'_>, name: &str) -> Option<PathBuf> {
    absolute(env, "XDG_CONFIG_HOME")
        .or_else(|| absolute(env, "HOME").map(|h| h.join(".config")))
        .map(|base| base.join(name.to_lowercase()))
}

/// The state root named `name` on this platform.
fn state_root_named(env: Env<'_>, name: &str) -> Option<PathBuf> {
    if cfg!(windows) {
        windows_state_root(env, name)
    } else if cfg!(target_os = "macos") {
        macos_state_root(env, name)
    } else {
        xdg_state_root(env, name)
    }
}

/// `%USERPROFILE%\Documents`, spelled the way the original resolves its documents folder.
fn windows_documents(env: Env<'_>) -> Option<PathBuf> {
    absolute(env, "USERPROFILE").map(|h| h.join("Documents"))
}

/// The renamed variables' names before the rename to Pyreal, read when the new name is unset
/// (kept for one release).
pub const OLD_ENV_NAMES: &[(&str, &str)] = &[
    (SETTINGS_DIR_ENV, "DERETH_SETTINGS_DIR"),
    (ART_PACK_ENV, "DERETH_ART_PACK"),
];

/// The process environment, a renamed variable by its old name when the new one is unset.
fn process_env(name: &str) -> Option<OsString> {
    renamed_env(&|n: &str| std::env::var_os(n), name)
}

/// `name` in `env`, else its old name in [`OLD_ENV_NAMES`].
fn renamed_env(env: Env<'_>, name: &str) -> Option<OsString> {
    env(name).or_else(|| {
        let (_, old) = OLD_ENV_NAMES.iter().find(|(new, _)| *new == name)?;
        env(old)
    })
}

/// **The person's Documents folder**, where the files they make and keep go: the folder Windows
/// knows as Documents, wherever it has been moved (OneDrive among them), else
/// `%USERPROFILE%\Documents`; `~/Documents` on macOS; and on Linux the XDG user directory
/// `XDG_DOCUMENTS_DIR` that `user-dirs.dirs` names, else `~/Documents`. `None` when the
/// environment names no home at all.
#[must_use]
pub fn documents_dir() -> Option<PathBuf> {
    let env: Env<'_> = &process_env;
    #[cfg(windows)]
    {
        windows::Storage::UserDataPaths::GetDefault()
            .and_then(|paths| paths.Documents())
            .ok()
            .map(|path| PathBuf::from(path.to_os_string()))
            .filter(|path| path.is_absolute())
            .or_else(|| windows_documents(env))
    }
    #[cfg(not(windows))]
    {
        if cfg!(target_os = "macos") {
            return absolute(env, "HOME").map(|h| h.join("Documents"));
        }
        let config = absolute(env, "XDG_CONFIG_HOME")
            .or_else(|| absolute(env, "HOME").map(|h| h.join(".config")));
        let user_dirs = config.and_then(|c| std::fs::read_to_string(c.join("user-dirs.dirs")).ok());
        xdg_documents(env, user_dirs.as_deref())
    }
}

/// The XDG documents folder: what `XDG_DOCUMENTS_DIR` names in `user-dirs.dirs` (whose text is
/// `user_dirs`), a path under `$HOME/` or an absolute one; else `~/Documents`. A value that names
/// the home folder itself means the person has none, and `~/Documents` is taken then too.
#[cfg_attr(windows, allow(dead_code))]
fn xdg_documents(env: Env<'_>, user_dirs: Option<&str>) -> Option<PathBuf> {
    let home = absolute(env, "HOME")?;
    let named = user_dirs.and_then(|text| {
        text.lines().find_map(|line| {
            let value = line
                .trim()
                .strip_prefix("XDG_DOCUMENTS_DIR=")?
                .trim()
                .trim_matches('"');
            match value.strip_prefix("$HOME") {
                Some(rest) => Some(home.join(rest.trim_start_matches('/'))),
                None => Some(PathBuf::from(value)).filter(|p| p.has_root()),
            }
        })
    });
    Some(
        named
            .filter(|p| *p != home)
            .unwrap_or_else(|| home.join("Documents")),
    )
}

/// The folder Pyreal keeps its settings under, on this platform. `None` when the environment
/// names no home at all, in which case the client saves nothing.
#[must_use]
pub fn state_root() -> Option<PathBuf> {
    state_root_named(&process_env, STATE_ROOT_NAME)
}

/// The folder the old Dereth launcher and client kept theirs under, which Pyreal only reads.
#[must_use]
pub fn legacy_state_root() -> Option<PathBuf> {
    state_root_named(&process_env, LEGACY_STATE_ROOT_NAME)
}

/// The old launcher's folder, which holds the worlds and accounts the first run imports; `None`
/// when [`SETTINGS_DIR_ENV`] names the settings folder outright, whose run imports nothing.
#[must_use]
pub fn legacy_launcher_dir() -> Option<PathBuf> {
    settings_dir_override()
        .is_none()
        .then(legacy_state_root)
        .flatten()
        .map(|root| root.join(LEGACY_LAUNCHER_DIR_NAME))
}

/// **The one directory every setting a product writes lives in**: [`SETTINGS_DIR_ENV`] when it
/// is set, otherwise its folder `dir_name` in [`state_root`], or the root itself for an empty
/// name (the client's, [`CLIENT_DIR_NAME`]).
#[must_use]
pub fn default_settings_dir(dir_name: &str) -> Option<PathBuf> {
    settings_dir_override().or_else(|| {
        state_root().map(|root| {
            if dir_name.is_empty() {
                root
            } else {
                root.join(dir_name)
            }
        })
    })
}

/// [`SETTINGS_DIR_ENV`]'s directory, made absolute against the working directory; `None` when the
/// variable is unset or empty.
fn settings_dir_override() -> Option<PathBuf> {
    process_env(SETTINGS_DIR_ENV)
        .filter(|v| !v.is_empty())
        .and_then(|v| std::path::absolute(v).ok())
}

/// Where the client binary writes its crash logs: [`CRASH_LOG_DIR_NAME`] in
/// [`default_settings_dir`], whatever `-prefs` says, because the log is opened before the command
/// line is read.
#[must_use]
pub fn crash_log_dir(dir_name: &str) -> Option<PathBuf> {
    default_settings_dir(dir_name).map(|dir| dir.join(CRASH_LOG_DIR_NAME))
}

/// Remove all but the newest `keep` crash logs of the binary `binary` (`<binary>-*.log`) in `dir`,
/// newest by modification time, and return how many were removed. Nothing else in `dir` is
/// touched, and a file that cannot be read or removed is left where it is: this runs at start-up
/// and must never stop a run.
pub fn prune_crash_logs(dir: &Path, binary: &str, keep: usize) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let prefix = format!("{binary}-");
    let mut logs: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with(&prefix) && name.ends_with(".log")
        })
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            meta.is_file()
                .then(|| (meta.modified().unwrap_or(std::time::UNIX_EPOCH), e.path()))
        })
        .collect();
    // Newest first; a tie falls to the name, so the order is the same on every listing.
    logs.sort_by(|a, b| b.cmp(a));
    logs.iter()
        .skip(keep)
        .filter(|(_, path)| std::fs::remove_file(path).is_ok())
        .count()
}

/// `%USERPROFILE%\Documents\Asheron's Call`, the original game's settings directory. Windows only;
/// `None` everywhere else, because the original never ran anywhere else.
#[must_use]
pub fn retail_settings_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        windows_documents(&process_env).map(|d| d.join(RETAIL_SETTINGS_DIR_NAME))
    } else {
        None
    }
}

/// A folder the first run copies settings in from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsSource {
    /// The old Dereth client's folder, copied as it is.
    Dereth(PathBuf),
    /// The original game's, whose key maps come in as the modern interface's.
    Retail(PathBuf),
}

impl SettingsSource {
    /// The folder.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            Self::Dereth(p) | Self::Retail(p) => p,
        }
    }
}

/// Where the first run copies settings in from: the old Dereth client's folder when there is one,
/// else on Windows [`retail_settings_dir`]; nothing when [`SETTINGS_DIR_ENV`] names the settings
/// folder outright.
#[must_use]
pub fn import_source() -> Option<SettingsSource> {
    import_source_in(&process_env, cfg!(windows), &|p: &Path| p.is_dir())
}

fn import_source_in(
    env: Env<'_>,
    windows: bool,
    is_dir: &dyn Fn(&Path) -> bool,
) -> Option<SettingsSource> {
    if renamed_env(env, SETTINGS_DIR_ENV).is_some_and(|v| !v.is_empty()) {
        return None;
    }
    let dereth = state_root_named(env, LEGACY_STATE_ROOT_NAME)
        .map(|root| root.join(LEGACY_CLIENT_DIR_NAME))
        .filter(|d| is_dir(d));
    if let Some(d) = dereth {
        return Some(SettingsSource::Dereth(d));
    }
    if !windows {
        return None;
    }
    windows_documents(env).map(|d| SettingsSource::Retail(d.join(RETAIL_SETTINGS_DIR_NAME)))
}

/// Choose the one in [`SETTINGS_DIR_ENV`]'s folder when that is set, else
/// `<cwd>/UserPreferences.ini` when it exists, otherwise the one in [`default_settings_dir`];
/// `None` when there is none.
///
/// The cwd probe is the original's first branch and is kept on every platform: it is what makes a
/// portable install -- drop a `UserPreferences.ini` beside the binary and the whole settings
/// directory moves with it -- and it is the override that needs no new flag. `-prefs <file>`
/// replaces the answer outright.
#[must_use]
pub fn default_preferences_file(dir_name: &str) -> Option<PathBuf> {
    // A folder named outright is a stronger request than a file that happens to be in the cwd.
    if let Some(dir) = settings_dir_override() {
        return Some(dir.join(PREFERENCES_FILE_NAME));
    }
    let cwd = std::env::current_dir()
        .unwrap_or_default()
        .join(PREFERENCES_FILE_NAME);
    if cwd.exists() {
        return Some(cwd);
    }
    default_settings_dir(dir_name).map(|dir| dir.join(PREFERENCES_FILE_NAME))
}

/// Whether `dir` already holds settings: anything but the crash logs, which are written before the
/// first-run copy runs and so do not mean it has run.
fn holds_settings(dir: &Path) -> bool {
    std::fs::read_dir(dir).map_or(dir.exists(), |entries| {
        entries
            .flatten()
            .any(|e| e.file_name() != CRASH_LOG_DIR_NAME)
    })
}

/// **The first-run copy of the original game's settings**, `from` being [`retail_settings_dir`]
/// and `to` [`default_settings_dir`]. Returns how many files were copied.
///
/// It **copies and does not move**: `from` is `Documents\Asheron's Call`, which a live retail
/// installation is also using, and moving it would take the retail client's own preferences,
/// keymap, journals and screenshots away from it. Nothing is ever written into `from`.
///
/// A key map in `from` itself comes in as the modern interface's, `<name>-modern.keymap`, so the
/// modern interface's key page lists it.
///
/// "One time" is expressed by the state on disk rather than by a stamp file: **`to` must hold no
/// settings.** A `to` holding only [`CRASH_LOG_DIR_NAME`] still counts as new, because the crash
/// log is written before this runs; after a copy `to` holds the player's files, so a later start
/// cannot re-enter, and a player who already has this client's files keeps them. Subdirectories
/// are carried too, and an entry already present in `to` is left alone.
///
/// `Ok(0)` covers every reason not to act -- `to` already holding settings, `from` not there, or
/// not a directory. A copy that fails part-way leaves what it managed and reports the error; the
/// caller tolerates it, because a client that could not carry settings over must still run.
///
/// # Errors
/// The first `std::fs` error from creating `to` or walking and copying `from`.
pub fn copy_settings_dir(from: &Path, to: &Path) -> std::io::Result<u32> {
    copy_settings_from(&SettingsSource::Retail(from.to_path_buf()), to)
}

/// [`copy_settings_dir`] from either source: the old Dereth client's folder is copied as it is,
/// its crash logs left behind.
///
/// # Errors
/// As [`copy_settings_dir`].
pub fn copy_settings_from(source: &SettingsSource, to: &Path) -> std::io::Result<u32> {
    let from = source.path();
    if holds_settings(to) || !from.is_dir() {
        return Ok(0);
    }
    std::fs::create_dir_all(to)?;
    copy_dir_contents(from, to, matches!(source, SettingsSource::Retail(_)))
}

/// The name a file of the original's folder takes in this client's: a key map at the top as the
/// modern interface's, `<name>-modern.keymap`; anything else its own name.
fn imported_name(name: &std::ffi::OsStr, top: bool) -> OsString {
    let path = Path::new(name);
    match (path.file_stem(), path.extension()) {
        (Some(stem), Some(ext)) if top && ext.eq_ignore_ascii_case("keymap") => {
            let mut out = stem.to_os_string();
            out.push("-modern.keymap");
            out
        }
        _ => name.to_os_string(),
    }
}

/// What the first-run copy did: from where, to where, and how many files (or why it failed).
pub type FirstRunCopy = (PathBuf, PathBuf, std::io::Result<u32>);

/// The command line parsed and the preferences loaded, **after** the first-run copy.
///
/// The copy runs when this run loads the settings folder's own preferences file (no `-prefs`, no
/// `UserPreferences.ini` in the working directory, not a headless run that named none) and that
/// folder holds nothing yet. When it copied anything, the configuration is read again, so the
/// window and the options screen both start from the copied settings rather than one of them from
/// the defaults loaded before the copy.
///
/// # Errors
/// [`ConfigError`] for a command line that does not parse.
pub fn load_config(
    argv: &[String],
    default_preferences: &Path,
    settings_dir: Option<&Path>,
    original_settings_dir: Option<&SettingsSource>,
) -> Result<(Config, Option<FirstRunCopy>), ConfigError> {
    let load = || -> Result<Config, ConfigError> {
        // A settings folder named in the environment is asked for outright: a headless run
        // reads its profile and writes it back at exit, as it does a `-prefs` file.
        let mut cfg = Config::from_args_and_prefs_named_at(
            argv,
            default_preferences,
            settings_dir_override().is_some(),
        )?;
        // The player's choice is the product's to read; only the override is the host's.
        cfg.art_pack = process_env(ART_PACK_ENV)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        cfg.base_pack = base_pack(
            &|n: &str| std::env::var_os(n),
            std::env::current_exe().ok().as_deref(),
        );
        if cfg.preferences_file.as_os_str().is_empty() && !cfg.headless {
            cfg.preferences_file = default_preferences.to_path_buf();
        }
        Ok(cfg)
    };
    let mut cfg = load()?;
    // `default_preferences` answers the working directory's file when there is one, so a path
    // equal to the settings folder's own file means no `-prefs`, no file beside the binary, and a
    // load from the settings folder: the three conditions in one comparison.
    let copied = settings_dir
        .filter(|to| cfg.preferences_file == to.join(PREFERENCES_FILE_NAME))
        .and_then(|to| {
            original_settings_dir.map(|from| {
                let outcome = copy_settings_from(from, to);
                (from.path().to_path_buf(), to.to_path_buf(), outcome)
            })
        });
    if matches!(copied, Some((_, _, Ok(n))) if n > 0) {
        cfg = load()?;
    }
    Ok((cfg, copied))
}

/// [`copy_settings_dir`]'s recursive half; `top` for the original's folder itself, whose key maps
/// are renamed.
fn copy_dir_contents(from: &Path, to: &Path, top: bool) -> std::io::Result<u32> {
    let mut copied = 0;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if entry.file_name() == CRASH_LOG_DIR_NAME {
            continue;
        }
        let target = to.join(imported_name(&entry.file_name(), top));
        if entry.file_type()?.is_dir() {
            if !target.exists() {
                std::fs::create_dir_all(&target)?;
            }
            copied += copy_dir_contents(&entry.path(), &target, false)?;
        } else if !target.exists() {
            std::fs::copy(entry.path(), &target)?;
            copied += 1;
        }
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The base pack is in a bundle's resources, and beside the executable anywhere else.
    #[test]
    fn the_installed_base_pack_is_in_the_bundles_resources_or_beside_the_client() {
        let bundled = Path::new("/Applications/Pyreal.app/Contents/MacOS/pyreal");
        assert_eq!(
            installed_base_pack(bundled),
            Path::new("/Applications/Pyreal.app/Contents/Resources/base-pack.dat")
        );
        let beside = Path::new("/opt/pyreal/pyreal");
        assert_eq!(
            installed_base_pack(beside),
            Path::new("/opt/pyreal/base-pack.dat")
        );
    }

    /// The base pack is the one named, none for `none`, else the installed one, else a
    /// checkout's.
    #[test]
    fn the_base_pack_is_named_installed_or_the_checkouts() {
        let dir = std::env::temp_dir().join(format!("pyreal-base-pack-{}", std::process::id()));
        let exe = dir.join("target").join("release").join("pyreal");
        std::fs::create_dir_all(exe.parent().expect("a folder")).expect("made");
        let none = env_of(&[]);
        assert_eq!(base_pack(&none, Some(&exe)), None);
        let checkout = dir.join("target").join("base-pack");
        std::fs::create_dir_all(&checkout).expect("made");
        std::fs::write(checkout.join(BASE_PACK_FILE), b"x").expect("written");
        assert_eq!(
            base_pack(&none, Some(&exe)),
            Some(checkout.join(BASE_PACK_FILE))
        );
        std::fs::write(installed_base_pack(&exe), b"x").expect("written");
        assert_eq!(
            base_pack(&none, Some(&exe)),
            Some(installed_base_pack(&exe))
        );
        assert_eq!(
            base_pack(&env_of(&[(BASE_PACK_ENV, "none")]), Some(&exe)),
            None
        );
        assert_eq!(
            base_pack(&env_of(&[(BASE_PACK_ENV, "/p.dat")]), Some(&exe)),
            Some(PathBuf::from("/p.dat"))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An environment of the given variables and nothing else.
    fn env_of(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let vars: Vec<(String, OsString)> = vars
            .iter()
            .map(|(k, v)| ((*k).to_string(), OsString::from(v)))
            .collect();
        move |name| vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
    }

    /// An absolute path that is absolute on the host running the test, whatever that host is.
    fn abs(tail: &str) -> String {
        let base = if cfg!(windows) { "C:\\home" } else { "/home" };
        format!("{base}{}{tail}", std::path::MAIN_SEPARATOR)
    }

    /// A scratch folder under the system temporary folder, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(what: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "pyreal-folders-{what}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos())
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch folder");
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// On Linux the Documents folder is the one the person's `user-dirs.dirs` names, under their
    /// home or anywhere; one naming the home folder itself, or none, means `~/Documents`.
    #[test]
    fn the_documents_folder_is_the_one_user_dirs_names_else_documents_at_home() {
        let home = abs("a");
        let env = env_of(&[("HOME", &home)]);
        let home = PathBuf::from(home);
        let named = |line: &str| {
            xdg_documents(
                &env,
                Some(&format!("# written by xdg-user-dirs-update\n{line}\n")),
            )
        };
        assert_eq!(xdg_documents(&env, None), Some(home.join("Documents")));
        assert_eq!(
            named("XDG_DOCUMENTS_DIR=\"$HOME/Papers\""),
            Some(home.join("Papers"))
        );
        assert_eq!(
            named("XDG_DOCUMENTS_DIR=\"$HOME/\""),
            Some(home.join("Documents"))
        );
        assert_eq!(
            named("XDG_MUSIC_DIR=\"$HOME/Music\""),
            Some(home.join("Documents"))
        );
        let elsewhere = abs("shared");
        assert_eq!(
            named(&format!("XDG_DOCUMENTS_DIR=\"{elsewhere}\"")),
            Some(PathBuf::from(elsewhere))
        );
        assert_eq!(xdg_documents(&env_of(&[]), None), None);
    }

    /// On Windows the Documents folder is the one the system names, wherever it was moved.
    #[cfg(windows)]
    #[test]
    fn windows_names_the_documents_folder() {
        let documents = documents_dir().expect("a Documents folder");
        assert!(documents.is_absolute(), "{}", documents.display());
    }

    /// Each platform's rule, asserted on every host: the resolvers take the environment as an
    /// argument, so all three are pure path arithmetic.
    #[test]
    fn the_settings_directory_follows_the_platform_convention() {
        let user = abs("user");
        let roaming = abs("roaming");
        let xdg = abs("xdg");

        // Windows: `%APPDATA%\Pyreal`, else the roaming folder under the profile.
        let env = env_of(&[("APPDATA", &roaming), ("USERPROFILE", &user)]);
        assert_eq!(
            windows_state_root(&env, STATE_ROOT_NAME),
            Some(PathBuf::from(&roaming).join("Pyreal"))
        );
        let env = env_of(&[("USERPROFILE", &user)]);
        assert_eq!(
            windows_state_root(&env, STATE_ROOT_NAME),
            Some(
                PathBuf::from(&user)
                    .join("AppData")
                    .join("Roaming")
                    .join("Pyreal")
            )
        );

        // macOS: Application Support, capitalised.
        let env = env_of(&[("HOME", &user)]);
        assert_eq!(
            macos_state_root(&env, STATE_ROOT_NAME),
            Some(
                PathBuf::from(&user)
                    .join("Library")
                    .join("Application Support")
                    .join("Pyreal")
            )
        );

        // XDG: `$XDG_CONFIG_HOME/pyreal` when set and absolute, else `~/.config/pyreal`.
        let env = env_of(&[("XDG_CONFIG_HOME", &xdg), ("HOME", &user)]);
        assert_eq!(
            xdg_state_root(&env, STATE_ROOT_NAME),
            Some(PathBuf::from(&xdg).join("pyreal"))
        );
        let env = env_of(&[("XDG_CONFIG_HOME", "relative/config"), ("HOME", &user)]);
        assert_eq!(
            xdg_state_root(&env, STATE_ROOT_NAME),
            Some(PathBuf::from(&user).join(".config").join("pyreal")),
            "a relative XDG_CONFIG_HOME is ignored"
        );

        // No home at all is no directory, rather than one relative to the working directory.
        let empty = env_of(&[]);
        assert_eq!(windows_state_root(&empty, STATE_ROOT_NAME), None);
        assert_eq!(macos_state_root(&empty, STATE_ROOT_NAME), None);
        assert_eq!(xdg_state_root(&empty, STATE_ROOT_NAME), None);
        let env = env_of(&[("HOME", "relative")]);
        assert_eq!(macos_state_root(&env, STATE_ROOT_NAME), None);

        // And this host's answer is its own platform's root itself, unless the environment
        // running the test names a folder outright.
        if let Some(dir) =
            default_settings_dir(CLIENT_DIR_NAME).filter(|_| settings_dir_override().is_none())
        {
            assert!(dir.is_absolute(), "{} is not absolute", dir.display());
            let root = dir.file_name().and_then(|n| n.to_str());
            if cfg!(any(windows, target_os = "macos")) {
                assert_eq!(root, Some("Pyreal"));
            } else {
                assert_eq!(root, Some("pyreal"));
            }
        }
    }

    /// Only Windows has the original game's folder to copy from, because the original never ran
    /// anywhere else.
    #[test]
    fn only_windows_has_the_original_games_settings_directory() {
        #[cfg(windows)]
        assert_eq!(
            retail_settings_dir().and_then(|d| d.file_name().map(ToOwned::to_owned)),
            Some(OsString::from(RETAIL_SETTINGS_DIR_NAME))
        );
        #[cfg(not(windows))]
        assert_eq!(retail_settings_dir(), None);
    }

    /// A run given its settings folder outright copies nothing in; without it, the old Dereth
    /// client's folder is copied when there is one, else on Windows `Documents\Asheron's Call`.
    #[test]
    fn a_settings_folder_given_outright_takes_nothing_from_the_originals() {
        let home = abs("home");
        let none = |_: &Path| false;
        let all = |_: &Path| true;
        let plain = env_of(&[
            ("USERPROFILE", home.as_str()),
            ("APPDATA", home.as_str()),
            ("HOME", home.as_str()),
        ]);
        assert_eq!(
            import_source_in(&plain, true, &none),
            Some(SettingsSource::Retail(
                PathBuf::from(&home)
                    .join("Documents")
                    .join(RETAIL_SETTINGS_DIR_NAME)
            ))
        );
        let old = state_root_named(&plain, LEGACY_STATE_ROOT_NAME)
            .unwrap()
            .join(LEGACY_CLIENT_DIR_NAME);
        assert_eq!(
            import_source_in(&plain, true, &all),
            Some(SettingsSource::Dereth(old))
        );
        let given = env_of(&[("USERPROFILE", home.as_str()), (SETTINGS_DIR_ENV, "run")]);
        assert_eq!(import_source_in(&given, true, &all), None);
        assert_eq!(import_source_in(&plain, false, &none), None);
    }

    /// The old Dereth client's folder comes over as it is, key maps by their own names, its crash
    /// logs left behind, and nothing in it changed.
    #[test]
    fn the_old_dereth_clients_folder_is_copied_as_it_is() {
        let scratch = Scratch::new("pyreal-copy");
        let (from, to) = (scratch.0.join("Dereth/client"), scratch.0.join("Pyreal"));
        std::fs::create_dir_all(from.join(CRASH_LOG_DIR_NAME)).unwrap();
        std::fs::create_dir_all(from.join("plugins/a")).unwrap();
        std::fs::write(from.join("dereth-horizon.keymap"), "k").unwrap();
        std::fs::write(from.join(PREFERENCES_FILE_NAME), "p").unwrap();
        std::fs::write(from.join("plugins/a/x.lua"), "x").unwrap();
        std::fs::write(from.join(CRASH_LOG_DIR_NAME).join("c.log"), "c").unwrap();
        let source = SettingsSource::Dereth(from.clone());
        assert_eq!(copy_settings_from(&source, &to).unwrap(), 3);
        assert!(to.join("dereth-horizon.keymap").is_file());
        assert!(to.join("plugins/a/x.lua").is_file());
        assert!(!to.join(CRASH_LOG_DIR_NAME).exists());
        assert!(from.join(CRASH_LOG_DIR_NAME).join("c.log").is_file());
        assert_eq!(copy_settings_from(&source, &to).unwrap(), 0, "one time");
    }

    /// The preferences file is the settings directory's `UserPreferences.ini` -- unless the
    /// working directory has one, which is the original's first branch and the portable-install
    /// override.
    #[test]
    fn the_preferences_file_is_the_settings_directorys_unless_the_cwd_has_one() {
        let cwd_file = std::env::current_dir()
            .unwrap_or_default()
            .join(PREFERENCES_FILE_NAME);
        let actual = default_preferences_file(CLIENT_DIR_NAME);
        if let Some(dir) = settings_dir_override() {
            assert_eq!(
                actual,
                Some(dir.join(PREFERENCES_FILE_NAME)),
                "a named folder wins"
            );
        } else if cwd_file.exists() {
            assert_eq!(actual, Some(cwd_file), "a cwd UserPreferences.ini wins");
        } else {
            assert_eq!(
                actual,
                default_settings_dir(CLIENT_DIR_NAME).map(|d| d.join(PREFERENCES_FILE_NAME)),
                "otherwise the settings directory's"
            );
        }
    }

    /// Only the newest [`CRASH_LOGS_KEPT`] crash logs survive a start, oldest removed first, and
    /// nothing in the folder that is not one of the binary's crash logs is touched.
    #[test]
    fn pruning_keeps_the_newest_twenty_crash_logs() {
        let scratch = Scratch::new("prune");
        let dir = &scratch.0;
        let base = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        for n in 0..25u64 {
            let path = dir.join(format!("pyreal-{}.log", 1000 + n));
            put(&path, b"[START]");
            let f = std::fs::File::options()
                .write(true)
                .open(&path)
                .expect("opens");
            // The pid order is the reverse of the age order, so a name sort would keep the wrong
            // twenty.
            f.set_modified(base + std::time::Duration::from_secs(100 - n))
                .expect("a settable time");
        }
        put(&dir.join("notes.txt"), b"not a log");
        // Another product's log in the same folder is that product's to prune.
        put(&dir.join("other-product-1.log"), b"[START]");
        assert_eq!(CRASH_LOGS_KEPT, 20);
        assert_eq!(prune_crash_logs(dir, "pyreal", CRASH_LOGS_KEPT), 5);
        let mut left: Vec<String> = std::fs::read_dir(dir)
            .expect("listed")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        let mut want: Vec<String> = (0..20u64)
            .map(|n| format!("pyreal-{}.log", 1000 + n))
            .collect();
        want.push("notes.txt".to_owned());
        want.push("other-product-1.log".to_owned());
        want.sort();
        assert_eq!(
            left, want,
            "the five oldest went, and the other files stayed"
        );
        assert_eq!(
            prune_crash_logs(dir, "pyreal", CRASH_LOGS_KEPT),
            0,
            "a second pass removes nothing"
        );
    }

    /// Write `bytes` at `path`, making its folder.
    fn put(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a folder");
        std::fs::write(path, bytes).expect("writable");
    }

    /// A machine with no original game's folder is the ordinary case; it is not a failure and it
    /// makes nothing.
    #[test]
    fn a_copy_with_nothing_to_copy_is_not_a_failure() {
        let scratch = Scratch::new("none");
        let to = scratch.0.join("new");
        assert_eq!(
            copy_settings_dir(&scratch.0.join("absent"), &to).expect("not an error"),
            0
        );
        assert!(!to.exists(), "and it did not create the destination");
    }

    /// The original game's folder is copied, everything including subfolders, and **left exactly
    /// as it was found** -- a retail installation is still using it. Only while the new folder
    /// holds nothing.
    #[test]
    fn the_first_run_copy_of_the_originals_folder_copies_everything_and_takes_nothing() {
        let scratch = Scratch::new("copy");
        let (from, to) = (scratch.0.join("Asheron's Call"), scratch.0.join("client"));
        // This run's own crash log, written before the copy, does not count as settings.
        put(&to.join(CRASH_LOG_DIR_NAME).join("pyreal-2.log"), b"new");
        put(
            &from.join("UserPreferences.ini"),
            b"[Net]\r\nUserName=keep-me\r\n",
        );
        put(&from.join("acclient.keymap"), b"keymap");
        put(&from.join("shots").join("ScreenShot00000.png"), b"png");

        assert_eq!(copy_settings_dir(&from, &to).expect("the copy runs"), 3);
        assert!(to.join("UserPreferences.ini").is_file());
        assert!(
            to.join("acclient-modern.keymap").is_file() && !to.join("acclient.keymap").exists(),
            "the key map comes in as the modern interface's"
        );
        assert!(to.join("shots").join("ScreenShot00000.png").is_file());
        assert!(
            from.join("UserPreferences.ini").is_file(),
            "the source keeps its files"
        );
        assert_eq!(std::fs::read_dir(&from).expect("listed").count(), 3);

        put(
            &to.join("UserPreferences.ini"),
            b"[Net]\r\nUserName=mine\r\n",
        );
        assert_eq!(
            copy_settings_dir(&from, &to).expect("refused, not failed"),
            0
        );
        assert_eq!(
            std::fs::read(to.join("UserPreferences.ini")).expect("still there"),
            b"[Net]\r\nUserName=mine\r\n"
        );
    }

    /// The first run's copy is what the run starts from: the original game's `FullScreen=True`
    /// and resolution reach the loaded configuration, not the defaults read before the copy.
    #[test]
    fn the_first_run_loads_the_settings_it_just_copied() {
        let root =
            std::env::temp_dir().join(format!("pyreal-first-run-load-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let original = root.join("Asheron's Call");
        let settings = root.join("Dereth").join("client");
        std::fs::create_dir_all(&original).unwrap();
        std::fs::write(
            original.join(PREFERENCES_FILE_NAME),
            "[Display]
FullScreen=True
",
        )
        .unwrap();
        let prefs = settings.join(PREFERENCES_FILE_NAME);
        let (cfg, copied) = load_config(
            &[],
            &prefs,
            Some(&settings),
            Some(&SettingsSource::Retail(original.clone())),
        )
        .unwrap();
        assert!(matches!(copied, Some((_, _, Ok(1)))), "{copied:?}");
        assert!(
            cfg.display.full_screen,
            "the copied FullScreen=True was loaded"
        );
        assert_eq!(cfg.preferences_file, prefs);
        // A second run copies nothing and loads the same file.
        let (again, copied) = load_config(
            &[],
            &prefs,
            Some(&settings),
            Some(&SettingsSource::Retail(original.clone())),
        )
        .unwrap();
        assert!(matches!(copied, Some((_, _, Ok(0)))), "{copied:?}");
        assert!(again.display.full_screen);
        let _ = std::fs::remove_dir_all(&root);
    }
}
