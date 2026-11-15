//! The art packs on the desktop: the player's library of packs, kept on the Data Files panel
//! (added with the system's file picker, taken out again), and which of them is drawn over the
//! built-in base pack: the one chosen in Settings, Display, or none for the base pack alone.
//! Remembered in [`LIBRARY_FILE`] in the settings folder.
//!
//! The base pack is the client's own (`pyreal_desktop::folders::BASE_PACK_FILE`). The first run
//! takes an older Dereth profile's `[Fidelity] ArtPack` into the library once;
//! `PYREAL_ART_PACK` names the pack for one run outright. A pack chosen is opened on a thread of
//! its own, and drawn from the frame it is open: the engine is made again and the world loaded
//! again round the eye.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use pyreal_horizon::art_packs::{ArtPackRequest, ArtPackRow, ArtPacksView};
use pyreal_scene::art_pack::packed::PackedFile;

/// The file in the settings folder that keeps the library and the choice.
pub const LIBRARY_FILE: &str = "art-packs.txt";

/// How often the library's files are looked at again for a change of size.
const RESCAN: Duration = Duration::from_secs(4);

/// The pack chosen among several.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Choice {
    /// Never chosen: the first run's.
    #[default]
    Unset,
    /// The base pack alone.
    None,
    Pack(PathBuf),
}

/// What [`LIBRARY_FILE`] holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Library {
    pub choice: Choice,
    /// The packs added, in order.
    pub packs: Vec<PathBuf>,
}

impl Library {
    /// The library a file's text holds; a line it cannot read is left out.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut l = Self::default();
        // Paths are escaped ([`escape_path`]) from the line that says so.
        let mut escaped = false;
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            let path = |v: &str| {
                if escaped {
                    unescape_path(v)
                } else {
                    PathBuf::from(v)
                }
            };
            match key.trim() {
                "paths" => escaped = value == ESCAPED,
                "chosen" if value.is_empty() || value == "none" => l.choice = Choice::None,
                "chosen" => l.choice = Choice::Pack(path(value)),
                "pack" if !value.is_empty() => l.add(path(value)),
                _ => {}
            }
        }
        l
    }

    /// The file's text.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::from(
            "# Pyreal's art packs: the library, and the one drawn over the built-in pack (none for\n\
             # the built-in pack alone).\n",
        );
        out.push_str(&format!("paths={ESCAPED}\n"));
        match &self.choice {
            Choice::Unset => {}
            Choice::None => out.push_str("chosen=none\n"),
            Choice::Pack(p) => out.push_str(&format!("chosen={}\n", escape_path(p))),
        }
        for p in &self.packs {
            out.push_str(&format!("pack={}\n", escape_path(p)));
        }
        out
    }

    /// The library kept in `settings`, or an empty one when there is no file; text that is not
    /// UTF-8 is read as Windows-1252, as Notepad saves "ANSI".
    ///
    /// # Errors
    /// The file is there but will not read.
    pub fn load(settings: &Path) -> std::io::Result<Self> {
        let bytes = match std::fs::read(settings.join(LIBRARY_FILE)) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e),
        };
        let text = String::from_utf8(bytes)
            .unwrap_or_else(|e| pyreal_primitives::text::cp1252::decode(e.as_bytes()));
        Ok(Self::parse(text.trim_start_matches('\u{feff}')))
    }

    /// Kept in `settings`.
    ///
    /// # Errors
    /// The file will not write.
    pub fn save(&self, settings: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(settings)?;
        std::fs::write(settings.join(LIBRARY_FILE), self.to_text())
    }

    /// `pack` added at the end, once.
    pub fn add(&mut self, pack: PathBuf) {
        if !self.packs.contains(&pack) {
            self.packs.push(pack);
        }
    }

    /// `pack` taken out.
    pub fn remove(&mut self, pack: &Path) {
        self.packs.retain(|p| p != pack);
    }

    /// The pack drawn: none from an empty library or when none is chosen, else the one chosen
    /// (the first when none was ever chosen, or the one chosen was taken out).
    #[must_use]
    pub fn drawn(&self) -> Option<PathBuf> {
        match (self.packs.as_slice(), &self.choice) {
            ([], _) | (_, Choice::None) => None,
            (all, Choice::Pack(p)) if all.contains(p) => Some(p.clone()),
            (all, _) => all.first().cloned(),
        }
    }

    /// A first run's library: an older profile's pack, where it is.
    pub fn first_run(&mut self, legacy: Option<PathBuf>) -> bool {
        if self.choice != Choice::Unset || !self.packs.is_empty() {
            return false;
        }
        let Some(p) = legacy.filter(|p| p.exists()) else {
            return false;
        };
        self.add(p.clone());
        self.choice = Choice::Pack(p);
        true
    }
}

/// The `paths=` value of a library whose paths are escaped.
const ESCAPED: &str = "escaped";

/// A path as a line of [`LIBRARY_FILE`] keeps it: `%`, line breaks and spaces or tabs at either
/// end as `%XX`, and what is not Unicode as `%XX` for each byte (`%uXXXX` for each UTF-16 unit on
/// Windows).
fn escape_path(p: &Path) -> String {
    let mut out = String::new();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        for chunk in p.as_os_str().as_bytes().utf8_chunks() {
            escape_text(chunk.valid(), &mut out);
            for b in chunk.invalid() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;
        let units: Vec<u16> = p.as_os_str().encode_wide().collect();
        for c in char::decode_utf16(units.iter().copied()) {
            match c {
                Ok(c) => escape_text(c.encode_utf8(&mut [0; 4]), &mut out),
                Err(e) => out.push_str(&format!("%u{:04X}", e.unpaired_surrogate())),
            }
        }
    }
    #[cfg(not(any(unix, windows)))]
    escape_text(&p.to_string_lossy(), &mut out);
    // Spaces and tabs at either end, which a hand-edited line loses.
    let blank = [' ', '\t'];
    let start = out.len() - out.trim_start_matches(blank).len();
    let end = out.trim_end_matches(blank).len().max(start);
    let hex = |s: &str| s.bytes().map(|b| format!("%{b:02X}")).collect::<String>();
    out = format!(
        "{}{}{}",
        hex(&out[..start]),
        &out[start..end],
        hex(&out[end..])
    );
    out
}

/// `text` with `%` and line breaks escaped.
fn escape_text(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '%' | '\n' | '\r' => out.push_str(&format!("%{:02X}", u32::from(c))),
            c => out.push(c),
        }
    }
}

/// A path [`escape_path`] kept.
fn unescape_path(s: &str) -> PathBuf {
    let hex = |h: &str| u32::from_str_radix(h, 16).ok();
    let mut bytes: Vec<u8> = Vec::new();
    let mut units: Vec<u16> = Vec::new();
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        let unit = rest
            .strip_prefix("%u")
            .and_then(|r| r.get(..4))
            .and_then(hex)
            .and_then(|u| u16::try_from(u).ok());
        let byte = rest
            .strip_prefix('%')
            .and_then(|r| r.get(..2))
            .and_then(hex)
            .and_then(|b| u8::try_from(b).ok());
        if let Some(u) = unit {
            units.push(u);
            rest = &rest[6..];
        } else if let Some(b) = byte {
            bytes.push(b);
            units.push(u16::from(b));
            rest = &rest[3..];
        } else {
            let mut buf = [0; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            units.extend(c.encode_utf16(&mut [0; 2]).iter().copied());
            rest = &rest[c.len_utf8()..];
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt as _;
        let _ = units;
        PathBuf::from(std::ffi::OsString::from_vec(bytes))
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt as _;
        let _ = bytes;
        PathBuf::from(std::ffi::OsString::from_wide(&units))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = units;
        PathBuf::from(String::from_utf8_lossy(&bytes).into_owned())
    }
}

/// An older profile's art pack: `ArtPack=` under `[Fidelity]` in `prefs`.
#[must_use]
pub fn legacy_pack(prefs: &str) -> Option<PathBuf> {
    let mut in_fidelity = false;
    for line in prefs.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_fidelity = line.eq_ignore_ascii_case("[Fidelity]");
        } else if in_fidelity {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim().eq_ignore_ascii_case("ArtPack") && !v.trim().is_empty() {
                    return Some(PathBuf::from(v.trim()));
                }
            }
        }
    }
    None
}

/// A pack's row: its name from its contents (or manifest), and its size.
#[must_use]
pub fn describe(path: &Path) -> ArtPackRow {
    let shown = path.display().to_string();
    let fallback = path
        .file_stem()
        .map_or_else(|| shown.clone(), |n| n.to_string_lossy().into_owned());
    let (name, bytes, problem) = if path.is_dir() {
        let manifest = std::fs::read_to_string(path.join(pyreal_scene::art_pack::MANIFEST_FILE))
            .map_err(|e| e.to_string())
            .and_then(|t| {
                pyreal_scene::art_pack::manifest_name(&t)
                    .ok_or_else(|| "its manifest will not read".to_owned())
            });
        match manifest {
            Ok(n) => (n, folder_bytes(path), None),
            Err(e) => (fallback, 0, Some(e)),
        }
    } else {
        let bytes = std::fs::metadata(path).map_or(0, |m| m.len());
        match PackedFile::open(path) {
            Ok(f) => (f.contents().name.clone(), bytes, None),
            Err(e) => (fallback, bytes, Some(e)),
        }
    };
    ArtPackRow {
        path: shown,
        name,
        bytes,
        problem,
    }
}

fn folder_bytes(dir: &Path) -> u64 {
    std::fs::read_dir(dir).map_or(0, |entries| {
        entries
            .filter_map(Result::ok)
            .map(|e| {
                let p = e.path();
                if p.is_dir() {
                    folder_bytes(&p)
                } else {
                    e.metadata().map_or(0, |m| m.len())
                }
            })
            .sum()
    })
}

/// What [`PackOpening`] asks of the frame.
#[derive(Debug, PartialEq)]
enum Opened<T> {
    /// Start opening this pack.
    Open(PathBuf),
    /// Draw this pack (none: the base pack alone).
    Draw(Option<(PathBuf, T)>),
    /// The pack would not open; the one drawn stays.
    Failed(String),
}

/// Which pack is drawn, which is wanted, and the one being opened: one opens at a time, the
/// latest choice is the one opened next, and a pack that will not open leaves the one drawn.
struct PackOpening<T> {
    /// The pack drawn.
    in_use: Option<PathBuf>,
    /// The pack chosen: the one drawn, or one being opened (or waiting to be) to take its place.
    wanted: Option<PathBuf>,
    opening: Option<(PathBuf, mpsc::Receiver<Result<T, String>>)>,
}

impl<T> PackOpening<T> {
    fn drawing(pack: Option<PathBuf>) -> Self {
        Self {
            in_use: pack.clone(),
            wanted: pack,
            opening: None,
        }
    }

    fn wanted(&self) -> Option<&PathBuf> {
        self.wanted.as_ref()
    }

    /// The player chose `want`: what to do now, if anything. While a pack opens, nothing: `want`
    /// is opened when it is done.
    fn choose(&mut self, want: Option<PathBuf>) -> Option<Opened<T>> {
        self.wanted = want;
        if self.opening.is_some() {
            return None;
        }
        self.next()
    }

    fn next(&mut self) -> Option<Opened<T>> {
        if self.wanted == self.in_use {
            return None;
        }
        Some(match self.wanted.clone() {
            None => {
                self.in_use = None;
                Opened::Draw(None)
            }
            Some(p) => Opened::Open(p),
        })
    }

    fn started(&mut self, pack: PathBuf, done: mpsc::Receiver<Result<T, String>>) {
        self.opening = Some((pack, done));
    }

    /// The pack opening, once it is done: drawn, or why not; or the choice made meanwhile opened.
    fn poll(&mut self) -> Option<Opened<T>> {
        let (_, done) = self.opening.as_ref()?;
        let result = match done.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            // The opener panicked.
            Err(mpsc::TryRecvError::Disconnected) => Err("the pack could not be read".to_owned()),
        };
        let (pack, _) = self.opening.take()?;
        if self.wanted.as_ref() != Some(&pack) {
            return self.next();
        }
        match result {
            Ok(t) => {
                self.in_use = Some(pack.clone());
                Some(Opened::Draw(Some((pack, t))))
            }
            Err(e) => {
                self.wanted.clone_from(&self.in_use);
                Some(Opened::Failed(e))
            }
        }
    }
}

/// The desktop's side of the art packs.
pub struct ArtPacks {
    settings: PathBuf,
    library: Library,
    /// The pack drawn, and the one chosen to take its place.
    packs: PackOpening<pyreal_scene::gpu::EnginePack>,
    /// The library's choice when the pack drawn was chosen, which a pack that will not open
    /// goes back to.
    kept: Choice,
    /// When the pack opening started.
    started: Option<Instant>,
    /// `PYREAL_ART_PACK`'s, which no choice changes this run.
    forced: bool,
    status: Option<String>,
    // ORDER-OK: looked up by path.
    described: BTreeMap<PathBuf, (u64, Option<SystemTime>, ArtPackRow)>,
    rows: Vec<ArtPackRow>,
    scanned: Option<Instant>,
    /// The library file was there and could not be read: nothing is saved over it this session.
    keep_file: bool,
}

impl std::fmt::Debug for ArtPacks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArtPacks")
            .field("library", &self.library)
            .field("drawn", &self.packs.wanted())
            .finish_non_exhaustive()
    }
}

impl ArtPacks {
    /// The library kept in `settings`.
    #[must_use]
    pub fn open(settings: &Path) -> Self {
        let (library, keep_file) = match Library::load(settings) {
            Ok(library) => (library, false),
            Err(e) => {
                tracing::error!(
                    "{LIBRARY_FILE} could not be read ({e}); it is left as it is and not saved over \
                     this session"
                );
                (Library::default(), true)
            }
        };
        Self {
            settings: settings.to_path_buf(),
            library,
            packs: PackOpening::drawing(None),
            kept: Choice::Unset,
            started: None,
            forced: false,
            status: None,
            described: BTreeMap::new(),
            rows: Vec::new(),
            scanned: None,
            keep_file,
        }
    }

    /// The pack the run starts with: `forced` (`PYREAL_ART_PACK`), else the library's, a first
    /// run's taken from the older profile read from `prefs`.
    pub fn start_pack(&mut self, forced: Option<PathBuf>, prefs: &[&Path]) -> Option<PathBuf> {
        if forced.is_some() {
            self.forced = true;
            self.packs = PackOpening::drawing(forced.clone());
            return forced;
        }
        let legacy = prefs
            .iter()
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .find_map(|t| legacy_pack(&t));
        if self.library.first_run(legacy) {
            tracing::info!("the art packs of the older settings are in the library");
            self.save();
        }
        self.packs = PackOpening::drawing(self.library.drawn());
        self.kept = self.library.choice.clone();
        self.packs.wanted().cloned()
    }

    fn save(&self) {
        if self.keep_file {
            return;
        }
        if let Err(e) = self.library.save(&self.settings) {
            tracing::warn!(
                "could not keep the art packs in {}: {e}",
                self.settings.display()
            );
        }
    }

    /// The library's packs, described (each file read again only when it changed).
    fn scan(&mut self) {
        let mut rows = Vec::with_capacity(self.library.packs.len());
        for p in &self.library.packs {
            let meta = std::fs::metadata(p).ok();
            let len = meta.as_ref().map_or(0, std::fs::Metadata::len);
            let modified = meta.and_then(|m| m.modified().ok());
            let row = match self.described.get(p) {
                Some((l, m, row)) if *l == len && *m == modified => row.clone(),
                _ => {
                    let row = describe(p);
                    self.described
                        .insert(p.clone(), (len, modified, row.clone()));
                    row
                }
            };
            rows.push(row);
        }
        self.rows = rows;
        self.scanned = Some(Instant::now());
    }

    /// What the interface shows.
    #[must_use]
    pub fn view(&self) -> ArtPacksView {
        ArtPacksView {
            packs: self.rows.clone(),
            chosen: self.packs.wanted().map(|p| p.display().to_string()),
            status: self.status.clone(),
            can_browse: cfg!(any(target_os = "macos", windows)),
        }
    }

    /// The library changed: kept, and the pack it draws now drawn.
    fn changed(&mut self, app: &mut crate::app::App) {
        self.save();
        self.scanned = None;
        if self.forced {
            return;
        }
        let want = self.library.drawn();
        if want.as_ref() == self.packs.wanted() {
            return;
        }
        let next = self.packs.choose(want);
        self.carry_out(app, next);
    }

    /// What [`PackOpening`] asked for, done.
    fn carry_out(
        &mut self,
        app: &mut crate::app::App,
        next: Option<Opened<pyreal_scene::gpu::EnginePack>>,
    ) {
        match next {
            None => {
                if self.packs.opening.is_none() {
                    self.status = None;
                }
            }
            Some(Opened::Draw(None)) => {
                self.status = None;
                self.kept = self.library.choice.clone();
                app.renderer_mut().use_art_pack(None);
                tracing::info!("no art pack chosen: the base pack drawn alone");
            }
            Some(Opened::Draw(Some((_, pack)))) => {
                tracing::info!(
                    "art pack {} ({}) opened in {:.0} ms; drawn from now on",
                    pack.pack.name,
                    pack.pack.hash,
                    self.started
                        .map_or(0.0, |t| t.elapsed().as_secs_f64() * 1e3)
                );
                app.renderer_mut().use_art_pack(Some(Arc::new(pack)));
                self.status = None;
                self.kept = self.library.choice.clone();
            }
            Some(Opened::Open(at)) => {
                let base = app.renderer_mut().base_pack();
                let (tx, rx) = mpsc::channel();
                let path = at.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(pyreal_scene::gpu::EnginePack::open_over(
                        &path,
                        base.as_deref(),
                    ));
                });
                self.packs.started(at, rx);
                self.started = Some(Instant::now());
                self.status = Some("Opening the pack...".to_owned());
            }
            Some(Opened::Failed(e)) => {
                tracing::warn!("the art pack would not open: {e}");
                self.status = Some(format!("Would not open: {e}"));
                // The pack drawn stays chosen.
                self.library.choice = self.kept.clone();
                self.save();
            }
        }
    }

    /// One frame's part: a pack opened is drawn; the interface shown the library and what the
    /// player asked of it carried out.
    pub fn step(&mut self, app: &mut crate::app::App) {
        let done = self.packs.poll();
        if done.is_some() {
            self.carry_out(app, done);
        }
        if app.shell_mut().horizon_mut().is_none() {
            return;
        }
        if self.scanned.is_none_or(|t| t.elapsed() > RESCAN) {
            self.scan();
        }
        let view = self.view();
        let mut requests = Vec::new();
        if let Some(h) = app.shell_mut().horizon_mut() {
            h.ui.worlds.art_packs = Some(view.clone());
            h.ui.windows.options_page.art_packs = Some(view);
            requests.append(&mut h.ui.worlds.art_pack_requests);
            requests.append(&mut h.ui.windows.options_page.art_pack_requests);
        }
        for r in requests {
            self.apply(app, r);
        }
    }

    fn apply(&mut self, app: &mut crate::app::App, r: ArtPackRequest) {
        match r {
            ArtPackRequest::Choose(path) => {
                self.library.choice = path.map_or(Choice::None, |p| Choice::Pack(p.into()));
            }
            ArtPackRequest::AddPack => {
                let Some(p) = pick_pack() else {
                    return;
                };
                self.library.add(p.clone());
                self.library.choice = Choice::Pack(p);
            }
            ArtPackRequest::Remove(path) => self.library.remove(Path::new(&path)),
        }
        self.changed(app);
    }
}

/// The system's file picker for a pack; `None` when cancelled or there is none.
#[cfg(any(target_os = "macos", windows))]
fn pick_pack() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("An art pack")
        .add_filter("Art pack", &["dat"])
        .pick_file()
}

#[cfg(not(any(target_os = "macos", windows)))]
fn pick_pack() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_library_keeps_its_packs_and_choice() {
        let l = Library {
            choice: Choice::Pack(PathBuf::from("/a b/p.dat")),
            packs: vec![PathBuf::from("/a b/p.dat"), PathBuf::from("/one.dat")],
        };
        assert_eq!(Library::parse(&l.to_text()), l);
        let none = Library {
            choice: Choice::None,
            ..Library::default()
        };
        assert_eq!(Library::parse(&none.to_text()), none);
        assert_eq!(Library::parse("").choice, Choice::Unset);
        // An older library's folders are passed over.
        assert!(Library::parse("folder=/packs\n").packs.is_empty());
    }

    /// Paths with spaces at either end, a percent sign or a line break, and (on Unix) bytes
    /// that are not UTF-8, read back as they were kept.
    #[test]
    fn any_path_reads_back_as_it_was_kept() {
        #[cfg_attr(not(unix), allow(unused_mut))]
        let mut packs = vec![
            PathBuf::from(" /lead and trail/p.dat "),
            PathBuf::from("/100%25/p.dat"),
            PathBuf::from("/two\nlines/p.dat"),
            PathBuf::from("C:\\Packs\\p.dat"),
        ];
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt as _;
            packs.push(PathBuf::from(std::ffi::OsString::from_vec(
                b"/caf\xe9/p.dat".to_vec(),
            )));
        }
        let l = Library {
            choice: Choice::Pack(packs[0].clone()),
            packs,
        };
        assert_eq!(Library::parse(&l.to_text()), l, "{}", l.to_text());
        // A library written before reads as it did.
        let old = Library::parse("chosen=/a b/p.dat\npack=/a b/p.dat\npack= /c.dat \n");
        assert_eq!(
            old.packs,
            [PathBuf::from("/a b/p.dat"), PathBuf::from("/c.dat")]
        );
    }

    /// A pack that will not open leaves the one drawn, and can be chosen again.
    #[test]
    fn a_pack_that_will_not_open_leaves_the_one_drawn() {
        let (a, b) = (PathBuf::from("/a.dat"), PathBuf::from("/b.dat"));
        let mut o = PackOpening::<u32>::drawing(Some(a.clone()));
        assert_eq!(o.choose(Some(b.clone())), Some(Opened::Open(b.clone())));
        let (tx, rx) = mpsc::channel();
        o.started(b.clone(), rx);
        tx.send(Err("broken".into())).unwrap();
        assert_eq!(o.poll(), Some(Opened::Failed("broken".into())));
        assert_eq!(o.wanted(), Some(&a), "the one drawn stays chosen");
        assert_eq!(
            o.choose(Some(b.clone())),
            Some(Opened::Open(b)),
            "and again"
        );
    }

    /// An opener that panics is a pack that would not open, not one opening for ever.
    #[test]
    fn an_opener_that_dies_is_a_pack_that_would_not_open() {
        let b = PathBuf::from("/b.dat");
        let mut o = PackOpening::<u32>::drawing(None);
        o.choose(Some(b.clone()));
        let (tx, rx) = mpsc::channel::<Result<u32, String>>();
        o.started(b, rx);
        drop(tx);
        assert!(matches!(o.poll(), Some(Opened::Failed(_))));
        assert_eq!(o.wanted(), None);
    }

    /// One pack opens at a time: a choice made meanwhile is opened when it is done, and the pack
    /// it took the place of is never drawn.
    #[test]
    fn a_choice_made_while_a_pack_opens_takes_its_place() {
        let (b, c) = (PathBuf::from("/b.dat"), PathBuf::from("/c.dat"));
        let mut o = PackOpening::<u32>::drawing(None);
        o.choose(Some(b.clone()));
        let (tx, rx) = mpsc::channel();
        o.started(b, rx);
        assert_eq!(o.choose(Some(c.clone())), None, "no second opener");
        assert_eq!(o.poll(), None);
        tx.send(Ok(1)).unwrap();
        assert_eq!(o.poll(), Some(Opened::Open(c.clone())));
        let (tx, rx) = mpsc::channel();
        o.started(c.clone(), rx);
        tx.send(Ok(2)).unwrap();
        assert!(matches!(o.poll(), Some(Opened::Draw(Some((p, 2)))) if p == c));
    }

    #[test]
    fn the_pack_chosen_is_drawn_or_none() {
        let (a, b) = (PathBuf::from("/a.dat"), PathBuf::from("/b.dat"));
        let mut l = Library::default();
        assert_eq!(l.drawn(), None);
        l.add(a.clone());
        assert_eq!(l.drawn(), Some(a.clone()), "one pack never chosen is drawn");
        l.choice = Choice::None;
        assert_eq!(l.drawn(), None, "the choice of none stands");
        l.add(b.clone());
        assert_eq!(l.drawn(), None);
        l.choice = Choice::Pack(b.clone());
        assert_eq!(l.drawn(), Some(b.clone()));
        l.remove(&b);
        assert_eq!(l.drawn(), Some(a.clone()));
        l.add(b.clone());
        l.choice = Choice::Pack(PathBuf::from("/gone.dat"));
        assert_eq!(l.drawn(), Some(a), "a choice taken out falls to the first");
    }

    #[test]
    fn a_first_run_takes_the_older_profiles_pack() {
        let dir = std::env::temp_dir().join(format!("pyreal-art-packs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("made");
        let old = dir.join("old.dat");
        std::fs::write(&old, b"x").expect("written");
        let prefs = "[Display]\nWidth=1\n[Fidelity]\nArtPack = ".to_owned()
            + &old.display().to_string()
            + "\nLighting=true\n";
        assert_eq!(legacy_pack(&prefs), Some(old.clone()));
        let mut l = Library::default();
        assert!(l.first_run(legacy_pack(&prefs)));
        assert_eq!(l.packs, std::slice::from_ref(&old));
        assert_eq!(l.drawn(), Some(old.clone()));
        assert!(!l.first_run(Some(dir.join("other.dat"))), "only once");
        // A player's library, even an empty one with none chosen, is not added to.
        let mut none = Library {
            choice: Choice::None,
            ..Library::default()
        };
        assert!(!none.first_run(Some(old)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_is_no_pack_is_listed_by_its_name_with_why() {
        let dir = std::env::temp_dir().join(format!("pyreal-art-row-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("made");
        let p = dir.join("broken.dat");
        std::fs::write(&p, vec![0u8; 2048]).expect("written");
        let row = describe(&p);
        assert_eq!((row.name.as_str(), row.bytes), ("broken", 2048));
        assert!(row.problem.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn tmp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("pyreal-art-packs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_library_saved_as_ansi_still_reads() {
        let d = tmp("ansi");
        std::fs::write(
            d.join(LIBRARY_FILE),
            b"chosen=C:\\Caf\xe9\\p.dat\npack=C:\\Caf\xe9\\p.dat\n",
        )
        .unwrap();
        let l = Library::load(&d).expect("read");
        assert_eq!(l.packs, vec![PathBuf::from("C:\\Caf\u{e9}\\p.dat")]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A library file that is there but cannot be read is no first run: nothing is saved over it.
    #[cfg(unix)]
    #[test]
    fn a_library_that_cannot_be_read_is_never_saved_over() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmp("unreadable");
        let file = d.join(LIBRARY_FILE);
        let theirs = "chosen=/mine.dat\npack=/mine.dat\n";
        std::fs::write(&file, theirs).unwrap();
        let old = d.join("old.dat");
        std::fs::write(&old, b"pack").unwrap();
        let prefs = d.join("UserPreferences.ini");
        std::fs::write(&prefs, format!("[Fidelity]\nArtPack={}\n", old.display())).unwrap();
        // Writable, not readable.
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o200)).unwrap();
        let mut packs = ArtPacks::open(&d);
        packs.start_pack(None, &[prefs.as_path()]);
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), theirs);
        let _ = std::fs::remove_dir_all(&d);
    }
}
