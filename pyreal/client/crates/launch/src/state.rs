//! Everything the player has told Pyreal about worlds, in one file: `worlds.json`.
//!
//! The player's own worlds (a name, a host and a port each), the data sets, the accounts (names
//! only) and each world's last choices. **Never a password**: those live only in the operating
//! system's vault ([`crate::vault`]), and a test below holds this file to that.
//!
//! # Where it lives
//!
//! In the client's settings folder, beside its preferences. The caller names the folder.
//!
//! # Writing
//!
//! Atomically: the new contents go to a temporary file which then replaces the old one, so a crash
//! or a full disk mid-write leaves the previous state intact rather than half a file.
//!
//! # Reading
//!
//! A file with a part that does not read keeps every part that does: only the parts that do not
//! are reset, each named with why ([`Loaded::Partly`]), and the file as it was is copied aside as
//! `worlds.json.bad`. A file that is not a state at all is set aside under that name and the
//! client starts fresh.
//!
//! # The old launcher's file
//!
//! The Tauri launcher kept `launcher-state.json` and `world-list.json` in its own folder.
//! [`import_launcher`] reads them, never writes them, and makes a state from what still applies:
//! the worlds the player added and the listed worlds an account or a choice names (their address
//! from the list's copy), their accounts, their choices and the data sets. Asheron's Call 2's
//! worlds and everything else of the launcher's own are left behind.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::datset::{DatOrigin, DatSet, SetKind, SHARED_SET_ID};
use crate::world::{Emulator, Endpoint, World};

pub const STATE_FILE: &str = "worlds.json";

/// The old launcher's state file, which [`import_launcher`] reads.
pub const LAUNCHER_STATE_FILE: &str = "launcher-state.json";

pub const SCHEMA: u32 = 1;

/// Write `bytes` to `path` whole or not at all: to a temporary file beside it, synced, then
/// renamed over it.
///
/// # Errors
/// The file could not be written or renamed.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

/// An account on one world. The password is in the vault, under [`crate::vault::target`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub world_slug: String,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Whether the password is kept in the vault between runs.
    #[serde(default)]
    pub remember: bool,
    /// First used on a world that makes accounts on first login.
    #[serde(default)]
    pub created_by_launcher: bool,
}

/// A saved combination, as the old launcher kept it. Read for its type's sake; Pyreal keeps none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Favourite {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub world_slug: String,
    pub account: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dat_set_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classic_set_id: Option<String>,
}

/// A world on the player's list: a name, a host and a port, and what the player said of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomWorld {
    /// Its identity among the worlds, so its accounts and choices are its own: `custom-<n>` for a
    /// world added here, or the old list's id for one carried over from it.
    pub slug: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    /// The rules the player said it plays (`PvE` or `PvP`), if they said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ruleset: Option<String>,
    /// The emulator the player said it runs; unknown when they did not say.
    #[serde(default)]
    pub emulator: Emulator,
    /// Where its status endpoint answers (`http://127.0.0.1:9100/status`), when the player said;
    /// the map's places are read beside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_url: Option<String>,
    /// Carried over from the old launcher ([`import_launcher`]), whose passwords it may take.
    #[serde(default)]
    pub imported: bool,
}

impl CustomWorld {
    /// The world record the rest of the crate works with. Its status and software are not known;
    /// its rules are what the player said, and a world that runs a client of its own
    /// ([`crate::known`]) is known by its id.
    pub fn to_world(&self) -> World {
        let mut w = World::new(self.slug.clone(), self.name.clone());
        w.ruleset.clone_from(&self.ruleset);
        w.emulator = self.emulator;
        w.status_url = self
            .status_url
            .clone()
            .filter(|u| crate::direct::is_status_url(u));
        w.endpoint = Some(Endpoint {
            address: self.host.clone(),
            port: self.port,
            transport: None,
        });
        crate::known::apply(&mut w);
        w
    }
}

/// What is wrong with a server typed by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomWorldError {
    NoHost,
    BadHost,
    BadPort,
}

impl core::fmt::Display for CustomWorldError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            CustomWorldError::NoHost => "Enter the world's host name or address.",
            CustomWorldError::BadHost => "The host cannot contain spaces, a colon or a slash.",
            CustomWorldError::BadPort => "The port must be a number from 1 to 65535.",
        })
    }
}

impl std::error::Error for CustomWorldError {}

/// A world's host as the player gives it, trimmed: a host name or an IPv4 address, with no space,
/// colon or slash in it.
///
/// # Errors
/// [`CustomWorldError::NoHost`] for none, [`CustomWorldError::BadHost`] for one that is not a host.
pub fn custom_host(host: &str) -> Result<&str, CustomWorldError> {
    let host = host.trim();
    if host.is_empty() {
        return Err(CustomWorldError::NoHost);
    }
    if host.contains(|c: char| c.is_whitespace() || c == ':' || c == '/') {
        return Err(CustomWorldError::BadHost);
    }
    Ok(host)
}

/// A world's game port as the player gives it: a number from 1 to 65535.
///
/// # Errors
/// [`CustomWorldError::BadPort`] for anything else.
pub fn custom_port(port: &str) -> Result<u16, CustomWorldError> {
    port.trim()
        .parse::<u16>()
        .ok()
        .filter(|p| *p != 0)
        .ok_or(CustomWorldError::BadPort)
}

/// "Remember my choices for this world".
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WorldPrefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dat_set_id: Option<String>,
    /// The Classic set chosen, if one was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classic_set_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// Seconds since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_played: Option<u64>,
}

/// Settings across worlds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Offer "Play anyway" on a blocked check.
    #[serde(default)]
    pub allow_play_anyway: bool,
}

/// The whole file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherState {
    pub schema: u32,
    #[serde(default)]
    pub dat_sets: Vec<DatSet>,
    #[serde(default)]
    pub accounts: Vec<Account>,
    /// The player's worlds, in the order they are listed.
    #[serde(default, alias = "custom_worlds")]
    pub worlds: Vec<CustomWorld>,
    /// The highest `custom-<n>` given, so a removed world's slug is never given again.
    #[serde(default)]
    pub last_custom: u32,
    #[serde(default)]
    pub world_prefs: BTreeMap<String, WorldPrefs>,
    #[serde(default)]
    pub settings: Settings,
    /// The world last played, which the list selects first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_world: Option<String>,
    /// The entries of a list or of `world_prefs` that did not read, by part: written back as they
    /// were, so a bad value or a later build's data is never lost.
    #[serde(skip)]
    pub unread: BTreeMap<&'static str, serde_json::Value>,
}

impl Default for LauncherState {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            dat_sets: Vec::new(),
            accounts: Vec::new(),
            worlds: Vec::new(),
            last_custom: 0,
            world_prefs: BTreeMap::new(),
            settings: Settings::default(),
            last_world: None,
            unread: BTreeMap::new(),
        }
    }
}

/// How a load went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loaded {
    /// There was no file: a first run.
    Fresh,
    Read,
    /// Some parts of the file did not read: they are reset and every other part kept, and the
    /// file as it was is copied aside under `bad`.
    Partly {
        bad: PathBuf,
        reset: Vec<Unread>,
    },
    /// The file is not a state at all: it was set aside under this name.
    SetAside(PathBuf),
    /// The file is there but could not be read, for this reason: it is left as it is, and nothing
    /// is saved over it this session.
    Unreadable(String),
}

/// A part of the state file that did not read, and why, in serde's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unread {
    /// Its key in the file ([`PARTS`]).
    pub part: &'static str,
    pub error: String,
}

/// The state file's parts, by their keys, each read on its own when the whole does not read.
pub const PARTS: [&str; 8] = [
    "schema",
    "dat_sets",
    "accounts",
    "worlds",
    "world_prefs",
    "settings",
    "last_world",
    "last_custom",
];

/// A part of the state in words for the player: `Some saved settings couldn't be read and were
/// reset: your accounts.`
#[must_use]
pub fn part_name(part: &str) -> &'static str {
    match part {
        "schema" => "the file's version",
        "dat_sets" => "your data files",
        "accounts" => "your accounts",
        "worlds" => "your worlds",
        "world_prefs" => "each world's last choices",
        "settings" => "your settings",
        "last_world" => "the world last played",
        "last_custom" => "how your worlds are numbered",
        _ => "a part of them",
    }
}

/// `into` read from `parts[part]`, or the reason it did not read in `reset`.
fn take<T: serde::de::DeserializeOwned>(
    parts: &serde_json::Map<String, serde_json::Value>,
    part: &'static str,
    into: &mut T,
    reset: &mut Vec<Unread>,
) {
    let Some(value) = parts.get(part) else {
        return;
    };
    match serde_json::from_value(value.clone()) {
        Ok(v) => *into = v,
        Err(e) => reset.push(Unread {
            part,
            error: e.to_string(),
        }),
    }
}

/// `into` read from `parts[key]` entry by entry: each entry that reads, and each that does not
/// kept as it was in `unread` under `part`, named with why in `reset`.
fn take_each<T: serde::de::DeserializeOwned>(
    parts: &serde_json::Map<String, serde_json::Value>,
    (key, part): (&str, &'static str),
    into: &mut Vec<T>,
    unread: &mut BTreeMap<&'static str, serde_json::Value>,
    reset: &mut Vec<Unread>,
) {
    let Some(serde_json::Value::Array(entries)) = parts.get(key) else {
        let mut whole = Vec::new();
        take(parts, part, &mut whole, reset);
        *into = whole;
        return;
    };
    let mut kept = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        match serde_json::from_value(entry.clone()) {
            Ok(v) => into.push(v),
            Err(e) => {
                reset.push(Unread {
                    part,
                    error: format!("entry {} kept as it was: {e}", i + 1),
                });
                kept.push(entry.clone());
            }
        }
    }
    if !kept.is_empty() {
        unread.insert(part, serde_json::Value::Array(kept));
    }
}

impl LauncherState {
    /// The state in `dir`.
    pub fn load(dir: &Path) -> (Self, Loaded) {
        let path = dir.join(STATE_FILE);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return (Self::default(), Loaded::Fresh);
            }
            Err(e) => return (Self::default(), Loaded::Unreadable(e.to_string())),
        };
        if let Ok(s) = serde_json::from_slice::<Self>(&bytes) {
            return (s, Loaded::Read);
        }
        let bad = dir.join(format!("{STATE_FILE}.bad"));
        match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(serde_json::Value::Object(parts)) => {
                let (state, reset) = Self::from_parts(&parts);
                // Kept as it was, for whoever looks into what could not be read.
                let _ = std::fs::write(&bad, &bytes);
                (state, Loaded::Partly { bad, reset })
            }
            _ => {
                let _ = std::fs::rename(&path, &bad);
                (Self::default(), Loaded::SetAside(bad))
            }
        }
    }

    /// The state from the file's `parts`, each read on its own: every part that reads, and the
    /// default for each that does not, named with why.
    fn from_parts(parts: &serde_json::Map<String, serde_json::Value>) -> (Self, Vec<Unread>) {
        let mut s = Self::default();
        let mut reset = Vec::new();
        let worlds = if parts.contains_key("worlds") {
            "worlds"
        } else {
            "custom_worlds"
        };
        take(parts, "schema", &mut s.schema, &mut reset);
        let unread = &mut s.unread;
        take_each(
            parts,
            ("dat_sets", "dat_sets"),
            &mut s.dat_sets,
            unread,
            &mut reset,
        );
        take_each(
            parts,
            ("accounts", "accounts"),
            &mut s.accounts,
            unread,
            &mut reset,
        );
        take_each(parts, (worlds, "worlds"), &mut s.worlds, unread, &mut reset);
        if let Some(serde_json::Value::Object(prefs)) = parts.get("world_prefs") {
            let mut kept = serde_json::Map::new();
            for (slug, value) in prefs {
                match serde_json::from_value(value.clone()) {
                    Ok(p) => {
                        s.world_prefs.insert(slug.clone(), p);
                    }
                    Err(e) => {
                        reset.push(Unread {
                            part: "world_prefs",
                            error: format!("{slug} kept as it was: {e}"),
                        });
                        kept.insert(slug.clone(), value.clone());
                    }
                }
            }
            if !kept.is_empty() {
                s.unread
                    .insert("world_prefs", serde_json::Value::Object(kept));
            }
        } else {
            take(parts, "world_prefs", &mut s.world_prefs, &mut reset);
        }
        take(parts, "settings", &mut s.settings, &mut reset);
        take(parts, "last_world", &mut s.last_world, &mut reset);
        take(parts, "last_custom", &mut s.last_custom, &mut reset);
        (s, reset)
    }

    /// Write atomically into `dir`: a temporary file, then a rename over the old one.
    ///
    /// # Errors
    /// The folder or the file could not be written.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        if self.unread.is_empty() {
            let json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
            return write_atomic(&dir.join(STATE_FILE), &json);
        }
        let mut whole = serde_json::to_value(self).map_err(io::Error::other)?;
        if let Some(parts) = whole.as_object_mut() {
            for (part, kept) in &self.unread {
                match (parts.get_mut(*part), kept) {
                    (Some(serde_json::Value::Array(list)), serde_json::Value::Array(more)) => {
                        list.extend(more.iter().cloned());
                    }
                    (Some(serde_json::Value::Object(map)), serde_json::Value::Object(more)) => {
                        for (k, v) in more {
                            map.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                    }
                    (None, _) => {
                        parts.insert((*part).to_owned(), kept.clone());
                    }
                    _ => {}
                }
            }
        }
        let json = serde_json::to_vec_pretty(&whole).map_err(io::Error::other)?;
        write_atomic(&dir.join(STATE_FILE), &json)
    }

    /// The world kept under `slug`.
    #[must_use]
    pub fn world(&self, slug: &str) -> Option<&CustomWorld> {
        self.worlds.iter().find(|w| w.slug == slug)
    }

    pub fn dat_set(&self, id: &str) -> Option<&DatSet> {
        self.dat_sets.iter().find(|s| s.id == id)
    }

    pub fn dat_set_mut(&mut self, id: &str) -> Option<&mut DatSet> {
        self.dat_sets.iter_mut().find(|s| s.id == id)
    }

    /// The default Modern set.
    pub fn shared_set(&self) -> Option<&DatSet> {
        self.default_set(SetKind::Modern)
    }

    /// The default set of a kind.
    pub fn default_set(&self, kind: SetKind) -> Option<&DatSet> {
        self.dat_sets
            .iter()
            .find(|s| s.kind == kind && s.origin == DatOrigin::Shared)
    }

    /// The private set for a world, if one has been made.
    pub fn private_set_for(&self, slug: &str) -> Option<&DatSet> {
        self.dat_sets.iter().find(|s| s.owner_world() == Some(slug))
    }

    /// Put `set` in as the default of its kind, in place of any set at the same folder.
    pub fn adopt_default_set(&mut self, mut set: DatSet) {
        self.dat_sets
            .retain(|s| !(s.kind == set.kind && s.path == set.path));
        for s in &mut self.dat_sets {
            if s.kind == set.kind && s.origin == DatOrigin::Shared {
                s.origin = DatOrigin::Unassigned;
            }
        }
        set.origin = DatOrigin::Shared;
        self.dat_sets.push(set);
    }

    pub fn accounts_for<'a>(&'a self, slug: &'a str) -> impl Iterator<Item = &'a Account> + 'a {
        self.accounts.iter().filter(move |a| a.world_slug == slug)
    }

    pub fn account(&self, slug: &str, username: &str) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|a| a.world_slug == slug && a.username.eq_ignore_ascii_case(username))
    }

    /// Add or update an account. Account names are case-insensitive, as the client lower-cases them;
    /// an update with no label keeps the account's.
    pub fn upsert_account(&mut self, account: Account) {
        match self.accounts.iter_mut().find(|a| {
            a.world_slug == account.world_slug && a.username.eq_ignore_ascii_case(&account.username)
        }) {
            Some(a) => {
                let label = account.label.or_else(|| a.label.take());
                *a = Account { label, ..account };
            }
            None => self.accounts.push(account),
        }
    }

    /// Forget an account. The caller deletes its vault entry.
    pub fn forget_account(&mut self, slug: &str, username: &str) {
        self.accounts
            .retain(|a| !(a.world_slug == slug && a.username.eq_ignore_ascii_case(username)));
        if let Some(p) = self.world_prefs.get_mut(slug) {
            if p.account
                .as_deref()
                .is_some_and(|a| a.eq_ignore_ascii_case(username))
            {
                p.account = None;
            }
        }
    }

    /// Add a world, or change the one at the same host and port; its slug.
    ///
    /// # Errors
    /// [`CustomWorldError`] for a missing or malformed host, or a port outside 1..=65535.
    pub fn add_custom_world(
        &mut self,
        name: &str,
        host: &str,
        port: &str,
        ruleset: Option<&str>,
        emulator: Emulator,
    ) -> Result<String, CustomWorldError> {
        let host = custom_host(host)?;
        let port = custom_port(port)?;
        let name = if name.trim().is_empty() {
            host
        } else {
            name.trim()
        }
        .to_owned();
        let ruleset = ruleset
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(str::to_owned);
        if let Some(w) = self
            .worlds
            .iter_mut()
            .find(|w| w.host.eq_ignore_ascii_case(host) && w.port == port)
        {
            w.name = name;
            w.ruleset = ruleset;
            w.emulator = emulator;
            return Ok(w.slug.clone());
        }
        let slug = self.free_slug("custom-");
        self.worlds.push(CustomWorld {
            slug: slug.clone(),
            name,
            host: host.to_owned(),
            port,
            ruleset,
            emulator,
            status_url: None,
            imported: false,
        });
        Ok(slug)
    }

    /// Change the world kept under `slug`: its name, host and port. False when there is none.
    ///
    /// # Errors
    /// [`CustomWorldError`] as [`Self::add_custom_world`].
    pub fn edit_custom_world(
        &mut self,
        slug: &str,
        name: &str,
        host: &str,
        port: &str,
    ) -> Result<bool, CustomWorldError> {
        let host = custom_host(host)?.to_owned();
        let port = custom_port(port)?;
        let Some(w) = self.worlds.iter_mut().find(|w| w.slug == slug) else {
            return Ok(false);
        };
        w.name = if name.trim().is_empty() {
            host.clone()
        } else {
            name.trim().to_owned()
        };
        w.host = host;
        w.port = port;
        Ok(true)
    }

    /// Add a world from the list of known worlds, kept under its list slug when that is free; the
    /// world already at the same host and port stays as it is. Its slug.
    ///
    /// # Errors
    /// [`CustomWorldError`] for a world the list gives no usable host and port.
    pub fn add_listed_world(&mut self, listed: &World) -> Result<String, CustomWorldError> {
        let ep = listed.endpoint.as_ref().ok_or(CustomWorldError::NoHost)?;
        let host = custom_host(&ep.address)?;
        let port = custom_port(&ep.port.to_string())?;
        if let Some(w) = self
            .worlds
            .iter()
            .find(|w| w.host.eq_ignore_ascii_case(host) && w.port == port)
        {
            return Ok(w.slug.clone());
        }
        let slug = if listed.slug.is_empty() || self.world(&listed.slug).is_some() {
            self.free_slug("custom-")
        } else {
            listed.slug.clone()
        };
        self.worlds.push(CustomWorld {
            slug: slug.clone(),
            name: listed.name.clone(),
            host: host.to_owned(),
            port,
            ruleset: listed.ruleset.clone(),
            emulator: listed.emulator,
            status_url: listed.status_url.clone(),
            imported: false,
        });
        Ok(slug)
    }

    /// Move the world kept under `slug` one place up the list (`up`) or down it. False when there
    /// is none or it is already at that end.
    pub fn move_world(&mut self, slug: &str, up: bool) -> bool {
        let Some(i) = self.worlds.iter().position(|w| w.slug == slug) else {
            return false;
        };
        let j = if up { i.checked_sub(1) } else { Some(i + 1) };
        match j.filter(|j| *j < self.worlds.len()) {
            Some(j) => {
                self.worlds.swap(i, j);
                true
            }
            None => false,
        }
    }

    /// The next `<prefix><n>`, after the highest ever given or any world has.
    fn free_slug(&mut self, prefix: &str) -> String {
        let highest = self
            .worlds
            .iter()
            .filter_map(|w| w.slug.strip_prefix(prefix)?.parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        let n = highest.max(self.last_custom).saturating_add(1);
        self.last_custom = n;
        format!("{prefix}{n}")
    }

    /// Remove a world, with its accounts and remembered choices. The caller deletes the accounts'
    /// vault entries first.
    pub fn remove_custom_world(&mut self, slug: &str) {
        self.worlds.retain(|w| w.slug != slug);
        self.accounts.retain(|a| a.world_slug != slug);
        self.world_prefs.remove(slug);
        if self.last_world.as_deref() == Some(slug) {
            self.last_world = None;
        }
    }

    /// Remember that `slug` was played with `account`, now.
    pub fn remember_launch(&mut self, slug: &str, account: &str, now: u64) {
        let p = self.world_prefs.entry(slug.to_owned()).or_default();
        p.account = Some(account.to_owned());
        p.last_played = Some(now);
        self.last_world = Some(slug.to_owned());
    }

    /// Make the set `id` the default of its kind. Only a set the player added can be the default:
    /// a world's private copy or a custom set belongs to its world.
    pub fn set_default_set(&mut self, id: &str) -> bool {
        let Some(kind) = self
            .dat_sets
            .iter()
            .find(|s| s.id == id)
            .filter(|s| matches!(s.origin, DatOrigin::Shared | DatOrigin::Unassigned))
            .map(|s| s.kind)
        else {
            return false;
        };
        for s in &mut self.dat_sets {
            if s.kind == kind && s.origin == DatOrigin::Shared {
                s.origin = DatOrigin::Unassigned;
            }
            if s.id == id {
                s.origin = DatOrigin::Shared;
            }
        }
        true
    }

    /// A fresh id, unique within this state.
    pub fn new_id(&self, prefix: &str) -> String {
        let taken = |id: &str| id == SHARED_SET_ID || self.dat_sets.iter().any(|s| s.id == id);
        (1u32..)
            .map(|n| format!("{prefix}{n}"))
            .find(|id| !taken(id))
            .expect("some id is free")
    }
}

/// What [`import_launcher`] carried over.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Imported {
    pub worlds: usize,
    pub accounts: usize,
    pub dat_sets: usize,
    /// Worlds an account or a choice named that neither the player's list nor the list's copy
    /// gave an address for, by id: their accounts are left behind.
    pub unplaced: Vec<String>,
}

/// The old launcher's state from its folder `dir` ([`LAUNCHER_STATE_FILE`], and the world list's
/// copy beside it for the listed worlds' addresses), as Pyreal's: `None` when there is no file
/// there or it is not a state. Reads only.
#[must_use]
pub fn import_launcher(dir: &Path) -> Option<(LauncherState, Imported)> {
    let bytes = std::fs::read(dir.join(LAUNCHER_STATE_FILE)).ok()?;
    let serde_json::Value::Object(parts) = serde_json::from_slice(&bytes).ok()? else {
        return None;
    };
    let mut s = LauncherState::default();
    let mut ignored = Vec::new();
    // The sets of this game only: each read on its own, so one Pyreal cannot read costs no other.
    if let Some(serde_json::Value::Array(sets)) = parts.get("dat_sets") {
        s.dat_sets = sets
            .iter()
            .filter_map(|v| serde_json::from_value::<DatSet>(v.clone()).ok())
            .collect();
    }
    take(&parts, "accounts", &mut s.accounts, &mut ignored);
    take(&parts, "world_prefs", &mut s.world_prefs, &mut ignored);
    if let Some(serde_json::Value::Bool(b)) = parts
        .get("settings")
        .and_then(|v| v.get("allow_play_anyway"))
    {
        s.settings.allow_play_anyway = *b;
    }
    // The worlds the player added, but Asheron's Call 2's.
    if let Some(serde_json::Value::Array(worlds)) = parts.get("custom_worlds") {
        s.worlds = worlds
            .iter()
            .filter(|v| v.get("game").and_then(|g| g.as_str()) != Some("auberean"))
            .filter_map(|v| serde_json::from_value::<CustomWorld>(v.clone()).ok())
            .map(|w| CustomWorld {
                imported: true,
                ..w
            })
            .collect();
    }
    // The listed worlds the player used, from the list's copy.
    let named: Vec<String> = s
        .accounts
        .iter()
        .map(|a| a.world_slug.clone())
        .chain(s.world_prefs.keys().cloned())
        .collect();
    let listed = crate::serverlist::ListCache::load(dir)
        .and_then(|c| c.worlds().ok())
        .unwrap_or_default();
    let mut unplaced = Vec::new();
    for slug in named {
        if s.world(&slug).is_some() || unplaced.contains(&slug) {
            continue;
        }
        let found = listed
            .iter()
            .filter(|w| w.game == crate::world::Game::Dereth)
            .find(|w| w.slug == slug)
            .and_then(|w| {
                let ep = w.endpoint.as_ref()?;
                Some(CustomWorld {
                    slug: w.slug.clone(),
                    name: w.name.clone(),
                    host: ep.address.clone(),
                    port: ep.port,
                    ruleset: w.ruleset.clone(),
                    emulator: w.emulator,
                    status_url: w.status_url.clone(),
                    imported: true,
                })
            });
        match found {
            Some(w) => s.worlds.push(w),
            None => unplaced.push(slug),
        }
    }
    let placed: Vec<String> = s.worlds.iter().map(|w| w.slug.clone()).collect();
    s.accounts.retain(|a| placed.contains(&a.world_slug));
    s.world_prefs.retain(|slug, _| placed.contains(slug));
    s.last_world = s
        .world_prefs
        .iter()
        .filter_map(|(slug, p)| Some((p.last_played?, slug.clone())))
        .max()
        .map(|(_, slug)| slug);
    let imported = Imported {
        worlds: s.worlds.len(),
        accounts: s.accounts.len(),
        dat_sets: s.dat_sets.len(),
        unplaced,
    };
    Some((s, imported))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datset::tests::tmp;

    fn account(slug: &str, user: &str) -> Account {
        Account {
            world_slug: slug.into(),
            username: user.into(),
            label: None,
            remember: true,
            created_by_launcher: false,
        }
    }

    /// A state the 0.4.9 launcher wrote, every part filled through its own calls and saved by its
    /// own code (its paths made plain).
    const STATE_0_4_9: &str = include_str!("../fixtures/launcher-state-0.4.9.json");

    #[test]
    fn a_saved_state_reads_back_whole_and_holds_no_password() {
        let d = tmp("state-round-trip");
        let mut s = LauncherState::default();
        let slug = s
            .add_custom_world("Home", "127.0.0.1", "9000", None, Emulator::Ace)
            .unwrap();
        s.upsert_account(account(&slug, "Lark"));
        s.remember_launch(&slug, "Lark", 7);
        s.save(&d).unwrap();
        let (back, how) = LauncherState::load(&d);
        assert_eq!(how, Loaded::Read);
        assert_eq!(back, s);
        let text = std::fs::read_to_string(d.join(STATE_FILE)).unwrap();
        assert!(!text.to_lowercase().contains("password"), "{text}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_part_that_does_not_read_is_reset_and_the_rest_kept() {
        let d = tmp("state-partly");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join(STATE_FILE),
            r#"{"schema":1,"accounts":"nonsense","worlds":[{"slug":"custom-1","name":"Home","host":"h","port":9000}]}"#,
        )
        .unwrap();
        let (s, how) = LauncherState::load(&d);
        let Loaded::Partly { reset, .. } = how else {
            panic!("{how:?}")
        };
        assert_eq!(reset.len(), 1);
        assert_eq!(reset[0].part, "accounts");
        assert_eq!(s.worlds.len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_file_that_is_there_but_cannot_be_read_is_not_a_first_run() {
        let d = tmp("state-unreadable");
        // A folder where the file should be: there, and no read of it succeeds.
        std::fs::create_dir_all(d.join(STATE_FILE)).unwrap();
        let (_, how) = LauncherState::load(&d);
        assert!(matches!(how, Loaded::Unreadable(_)), "{how:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn one_world_that_does_not_read_leaves_the_others_and_is_written_back() {
        let d = tmp("state-one-bad-world");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join(STATE_FILE),
            r#"{"schema":1,"worlds":[
                {"slug":"custom-1","name":"Home","host":"h","port":9000},
                {"slug":"custom-2","name":"Far","host":"f","port":70000}]}"#,
        )
        .unwrap();
        let (s, how) = LauncherState::load(&d);
        assert!(matches!(how, Loaded::Partly { .. }), "{how:?}");
        assert_eq!(s.worlds.len(), 1);
        assert_eq!(s.worlds[0].name, "Home");
        s.save(&d).unwrap();
        let text = std::fs::read_to_string(d.join(STATE_FILE)).unwrap();
        assert!(text.contains("70000"), "the bad world kept: {text}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_data_set_of_a_kind_a_later_build_added_is_kept_and_written_back() {
        let d = tmp("state-later-set");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join(STATE_FILE),
            r#"{"schema":1,"dat_sets":[
                {"id":"eor","path":"/a","kind":"modern","origin":{"kind":"shared"},"files":[]},
                {"id":"x","path":"/b","kind":"future","origin":{"kind":"mirrored","from":"z"},"files":[]}]}"#,
        )
        .unwrap();
        let (s, _) = LauncherState::load(&d);
        assert_eq!(s.dat_sets.len(), 1);
        s.save(&d).unwrap();
        let text = std::fs::read_to_string(d.join(STATE_FILE)).unwrap();
        assert!(
            text.contains("future") && text.contains("mirrored"),
            "{text}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_old_name_for_the_worlds_reads_when_another_part_does_not() {
        let d = tmp("state-custom-worlds");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join(STATE_FILE),
            r#"{"schema":1,"accounts":"nonsense","custom_worlds":[{"slug":"custom-1","name":"Home","host":"h","port":9000}]}"#,
        )
        .unwrap();
        let (s, _) = LauncherState::load(&d);
        assert_eq!(s.worlds.len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_world_added_twice_at_one_address_is_one_world_and_edits_keep_its_slug() {
        let mut s = LauncherState::default();
        let a = s
            .add_custom_world("One", "example.org", "9000", None, Emulator::Unknown)
            .unwrap();
        let b = s
            .add_custom_world("Two", "EXAMPLE.org", "9000", None, Emulator::Unknown)
            .unwrap();
        assert_eq!(a, b);
        assert_eq!(s.worlds.len(), 1);
        assert_eq!(s.worlds[0].name, "Two");
        assert!(s.edit_custom_world(&a, "", "other.org", "9050").unwrap());
        assert_eq!(s.worlds[0].name, "other.org");
        assert_eq!(s.worlds[0].port, 9050);
        assert_eq!(
            s.edit_custom_world(&a, "x", "a b", "1"),
            Err(CustomWorldError::BadHost)
        );
        s.upsert_account(account(&a, "lark"));
        s.remove_custom_world(&a);
        assert!(s.worlds.is_empty() && s.accounts.is_empty());
    }

    /// The 0.4.9 launcher's file comes over: its Dereth worlds and their accounts and choices, its
    /// sets; Asheron's Call 2's world, the retail client and the launcher's own settings do not.
    #[test]
    fn the_old_launchers_state_is_imported_and_left_as_it_was() {
        let d = tmp("state-import");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(LAUNCHER_STATE_FILE), STATE_0_4_9).unwrap();
        let before = std::fs::read(d.join(LAUNCHER_STATE_FILE)).unwrap();
        let (s, imported) = import_launcher(&d).expect("the file reads");
        assert_eq!(
            std::fs::read(d.join(LAUNCHER_STATE_FILE)).unwrap(),
            before,
            "the old file is never written"
        );
        assert!(s.worlds.iter().any(|w| w.name == "Home"));
        assert!(s.worlds.iter().all(|w| w.imported));
        assert!(s.accounts.iter().all(|a| s.world(&a.world_slug).is_some()));
        assert_eq!(imported.worlds, s.worlds.len());
        assert_eq!(imported.dat_sets, s.dat_sets.len());
        assert!(!s.dat_sets.is_empty());
        assert!(import_launcher(&d.join("nowhere")).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A listed world an account names is placed from the list's copy beside the file; one the
    /// copy lacks is said, and its account left behind; an Asheron's Call 2 world never comes.
    #[test]
    fn listed_worlds_come_from_the_lists_copy() {
        let d = tmp("state-import-listed");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join(LAUNCHER_STATE_FILE),
            r#"{"schema":1,
                "accounts":[{"world_slug":"coldeve","username":"a"},{"world_slug":"gone","username":"b"},
                            {"world_slug":"auberean-1","username":"c"}],
                "custom_worlds":[{"slug":"auberean-1","name":"X","host":"x","port":1,"game":"auberean"}],
                "world_prefs":{"coldeve":{"account":"a","last_played":5}},
                "settings":{"realm":"auberean","auto_update":false}}"#,
        )
        .unwrap();
        let xml = "<ArrayOfServerItem><ServerItem><id>1</id><name>Coldeve</name>\
                   <emu>ACE</emu><server_host>coldeve.example</server_host>\
                   <server_port>9000</server_port><type>PvE</type></ServerItem></ArrayOfServerItem>";
        let cache = crate::serverlist::ListCache {
            fetched_at: 1,
            list: xml.into(),
            ..Default::default()
        };
        cache.save(&d).unwrap();
        let (s, imported) = import_launcher(&d).unwrap();
        let coldeve = s.world("coldeve").expect("placed from the copy");
        assert_eq!(
            (coldeve.host.as_str(), coldeve.port),
            ("coldeve.example", 9000)
        );
        assert_eq!(s.worlds.len(), 1);
        assert_eq!(s.accounts.len(), 1);
        assert_eq!(
            imported.unplaced,
            vec!["gone".to_owned(), "auberean-1".to_owned()]
        );
        assert_eq!(s.last_world.as_deref(), Some("coldeve"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A removed world's slug is never given again, across a save and a load; the next is after
    /// the highest a world already has.
    #[test]
    fn a_removed_worlds_slug_is_never_given_again() {
        let d = tmp("state-slugs");
        let add = |s: &mut LauncherState, host: &str| {
            s.add_custom_world("", host, "9000", None, Emulator::Unknown)
                .unwrap()
        };
        let mut s = LauncherState::default();
        add(&mut s, "one.example");
        let two = add(&mut s, "two.example");
        s.remove_custom_world(&two);
        let three = add(&mut s, "three.example");
        assert_ne!(three, two);
        s.remove_custom_world(&three);
        s.save(&d).unwrap();
        let (mut back, _) = LauncherState::load(&d);
        let four = add(&mut back, "four.example");
        assert!(four != two && four != three, "{four}");
        std::fs::write(
            d.join(STATE_FILE),
            r#"{"schema":1,"worlds":[{"slug":"custom-5","name":"Home","host":"h","port":9000}]}"#,
        )
        .unwrap();
        let (mut old, _) = LauncherState::load(&d);
        assert_eq!(add(&mut old, "six.example"), "custom-6");
        assert_eq!(old.worlds[0].slug, "custom-5");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// An update that names no label keeps the account's own, as the old launcher gave it.
    #[test]
    fn an_update_without_a_label_keeps_the_accounts_label() {
        let mut s = LauncherState::default();
        s.upsert_account(Account {
            label: Some("main".into()),
            ..account("custom-1", "Lark")
        });
        s.upsert_account(Account {
            remember: false,
            ..account("custom-1", "lark")
        });
        let a = s.account("custom-1", "lark").unwrap();
        assert_eq!(a.label.as_deref(), Some("main"));
        assert!(!a.remember);
        s.upsert_account(Account {
            label: Some("alt".into()),
            ..account("custom-1", "lark")
        });
        assert_eq!(
            s.account("custom-1", "lark").unwrap().label.as_deref(),
            Some("alt")
        );
    }

    /// A world moves one place at a time, not past either end, and the order is what is kept.
    #[test]
    fn worlds_move_up_and_down_and_the_order_is_kept() {
        let d = tmp("state-move");
        let mut s = LauncherState::default();
        for name in ["A", "B", "C"] {
            s.add_custom_world(
                name,
                &format!("{name}.example"),
                "9000",
                None,
                Emulator::Unknown,
            )
            .unwrap();
        }
        let order = |s: &LauncherState| s.worlds.iter().map(|w| w.name.clone()).collect::<Vec<_>>();
        let a = s.worlds[0].slug.clone();
        let c = s.worlds[2].slug.clone();
        assert!(!s.move_world(&a, true), "the first does not move up");
        assert!(!s.move_world(&c, false), "the last does not move down");
        assert!(!s.move_world("nowhere", true));
        assert!(s.move_world(&c, true));
        assert_eq!(order(&s), ["A", "C", "B"]);
        assert!(s.move_world(&a, false));
        assert_eq!(order(&s), ["C", "A", "B"]);
        s.save(&d).unwrap();
        let (back, _) = LauncherState::load(&d);
        assert_eq!(order(&back), ["C", "A", "B"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A known world is added once under its list slug, with what the list says of it.
    #[test]
    fn a_listed_world_is_added_once_under_its_list_slug() {
        let mut listed = World::new("coldeve", "Coldeve");
        listed.ruleset = Some("PvE".into());
        listed.emulator = Emulator::Ace;
        listed.endpoint = Some(Endpoint {
            address: "coldeve.example".into(),
            port: 9000,
            transport: None,
        });
        let mut s = LauncherState::default();
        assert_eq!(s.add_listed_world(&listed).unwrap(), "coldeve");
        assert_eq!(s.add_listed_world(&listed).unwrap(), "coldeve");
        assert_eq!(s.worlds.len(), 1);
        let w = s.world("coldeve").unwrap();
        assert_eq!((w.name.as_str(), w.port), ("Coldeve", 9000));
        assert_eq!(
            (w.ruleset.as_deref(), w.emulator),
            (Some("PvE"), Emulator::Ace)
        );
        // Another world the list names alike, at another address, takes a slug of its own.
        listed.endpoint.as_mut().unwrap().address = "other.example".into();
        assert_eq!(s.add_listed_world(&listed).unwrap(), "custom-1");
        listed.endpoint = None;
        assert_eq!(s.add_listed_world(&listed), Err(CustomWorldError::NoHost));
    }
}
