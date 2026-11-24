//! The screens before any world, on the desktop: the player's worlds, accounts and data files,
//! kept in `worlds.json` in the settings folder ([`pyreal_launch::state`]), passwords in the
//! system's secret store, and sessions started and ended in process.
//!
//! Each frame the host hands Horizon a [`FrontDoorView`] and carries out what the player asked
//! ([`FrontDoor::step`]). Logging in builds a [`pyreal_launch::LaunchPlan`] from the world, the
//! account and the data sets, and starts the session from it with the password from the vault.
//!
//! On the first run (no `worlds.json`) the old launcher's worlds, accounts and data folders are
//! imported from its folder, which is only read ([`pyreal_launch::state::import_launcher`]).
//!
//! A world whose wire is not known yet is probed on Play ([`pyreal_wire1999::probe`]), and the
//! answer is kept with it: a 1999 world signs in with `--wire 1999`, which picks the client's 1999
//! driver.

use std::path::{Path, PathBuf};

use pyreal_client_runtime::app::SessionStart;
use pyreal_horizon::front_door::{DatsFolder, DatsView, FrontDoorRequest, FrontDoorView, WorldRow};
use pyreal_launch::choices::MissingSet;
use pyreal_launch::datset::{
    check_folder, check_folder_over, DatFileState, DatOrigin, DatSet, SetKind,
};
use pyreal_launch::state::{Account, LauncherState, Loaded};
use pyreal_launch::vault::Vault;
use pyreal_launch::LaunchPlan;
use pyreal_transport::status_ping::StatusReply;
use pyreal_wire1999::probe::Answer;

/// Seconds since the epoch.
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The ids of the default Modern, Classic and Original sets.
const MODERN_SET: &str = "eor";
const CLASSIC_SET: &str = "classic";
const ORIGINAL_SET: &str = "original";

/// The host's side of the screens before any world.
#[derive(Debug)]
pub struct FrontDoor {
    dir: PathBuf,
    state: LauncherState,
    vault: Box<dyn Vault>,
    message: Option<String>,
    /// The plan the session up was started from, and the password typed for it, kept once the
    /// server takes the login.
    session: Option<(LaunchPlan, Option<Typed>)>,
    /// Each address's check this run: its status ping and its wire. An unanswered ping is asked
    /// again at the next sign-in.
    pings: std::collections::BTreeMap<(String, u16), Checked>,
    /// The checks being asked, each on a thread of its own.
    asking: std::collections::BTreeMap<(String, u16), std::sync::mpsc::Receiver<Checked>>,
    /// A sign-in waiting for its world's check: slug, account, password, remember.
    waiting: Option<(String, String, String, bool)>,
    /// The later data files' folder as [`Self::view`] last read it, which it reads again only
    /// where a file changed in size or time: the view is made every frame.
    checked: std::cell::RefCell<Option<(PathBuf, Vec<DatFileState>)>>,
    /// worlds.json was there and could not be read: nothing is saved over it this session.
    keep_file: bool,
    /// The list of known worlds.
    directory: crate::world_directory::WorldDirectory,
    /// The device is still preparing its shaders: the screens say so while nothing else is said.
    preparing: bool,
    /// Whether a newer Pyreal is out, and whether the player lets the client check.
    updates: crate::update_check::UpdateCheck,
}

/// What one check of a world's address found.
#[derive(Debug, Clone, Default)]
struct Checked {
    /// Its status ping's answer: its era and systems.
    status: Option<StatusReply>,
    /// The wire it speaks, when that was asked: a world that answers the status ping speaks the
    /// later one.
    wire: Option<Answer>,
}

/// What the screens say while the device is still preparing its shaders (a driver whose cache is
/// cold, the first run on a card or after its driver is updated).
pub const PREPARING_SHADERS: &str = "Preparing shaders...";

/// A password typed to sign in, which the secret store keeps once the server takes it.
struct Typed {
    slug: String,
    account: String,
    password: String,
}

impl std::fmt::Debug for Typed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Typed")
            .field("slug", &self.slug)
            .field("account", &self.account)
            .field("password", &"***")
            .finish()
    }
}

impl FrontDoor {
    /// The worlds kept in `dir`, or on a first run the old launcher's from `legacy` (only read).
    /// When no data files are known yet, `found` (the folder the run was pointed at, when it
    /// holds the later files) becomes the default set.
    #[must_use]
    pub fn open(
        dir: &Path,
        legacy: Option<&Path>,
        vault: Box<dyn Vault>,
        found: Option<&Path>,
    ) -> Self {
        let (mut state, loaded) = LauncherState::load(dir);
        let keep_file = matches!(loaded, Loaded::Unreadable(_));
        let mut save = false;
        match loaded {
            Loaded::Fresh => {
                if let Some((imported, what)) =
                    legacy.and_then(pyreal_launch::state::import_launcher)
                {
                    tracing::info!(
                        "first run -- imported {} world(s), {} account(s) and {} data set(s) from \
                         the old launcher{}",
                        what.worlds,
                        what.accounts,
                        what.dat_sets,
                        if what.unplaced.is_empty() {
                            String::new()
                        } else {
                            format!("; no address for {}", what.unplaced.join(", "))
                        }
                    );
                    state = imported;
                    save = true;
                }
            }
            Loaded::Read => {}
            Loaded::Partly { reset, .. } => {
                for r in reset {
                    tracing::warn!("worlds.json: {} reset: {}", r.part, r.error);
                }
            }
            Loaded::SetAside(bad) => {
                tracing::warn!("worlds.json did not read; kept as {}", bad.display());
            }
            Loaded::Unreadable(why) => {
                tracing::error!(
                    "worlds.json could not be read ({why}); it is left as it is and not saved \
                     over this session"
                );
            }
        }
        save |= give_the_1999_set_its_id(&mut state);
        if state.shared_set().is_none() {
            if let Some(set) = found.and_then(|d| check_folder(SetKind::Modern, d, MODERN_SET).ok())
            {
                state.adopt_default_set(set);
                save = true;
            }
        }
        let me = Self {
            dir: dir.to_path_buf(),
            state,
            vault,
            message: None,
            session: None,
            pings: std::collections::BTreeMap::new(),
            asking: std::collections::BTreeMap::new(),
            waiting: None,
            checked: std::cell::RefCell::new(None),
            keep_file,
            preparing: false,
            directory: crate::world_directory::WorldDirectory::open(dir),
            updates: crate::update_check::UpdateCheck::open(
                dir,
                crate::update_check::UPDATE_URL,
                crate::update_check::SOURCE_URL,
                crate::update_check::VERSION,
            ),
        };
        if save {
            me.save();
        }
        me
    }

    fn save(&self) {
        if self.keep_file {
            return;
        }
        if let Err(e) = self.state.save(&self.dir) {
            tracing::warn!("could not keep the worlds in {}: {e}", self.dir.display());
        }
    }

    /// The folders of the default Modern set and of a pair for the menu's world (the Classic
    /// set, else the Original one), when they are known.
    #[must_use]
    pub fn dat_dirs(&self) -> (Option<PathBuf>, Option<PathBuf>) {
        (
            self.state.shared_set().map(|s| s.path.clone()),
            self.state
                .default_set(SetKind::Classic)
                .or_else(|| self.state.default_set(SetKind::Original))
                .map(|s| s.path.clone()),
        )
    }

    /// The folder of the default pair of `kind`, when it is known.
    fn pair_dir(&self, kind: SetKind) -> Option<PathBuf> {
        self.state.default_set(kind).map(|s| s.path.clone())
    }

    /// Add the world a join link (`pyreal://join?...`) names, and select it.
    pub fn add_join_link(&mut self, link: &str) {
        let added = pyreal_launch::join::parse(link)
            .map_err(|e| e.to_string())
            .and_then(|l| {
                self.state
                    .add_custom_world(
                        &l.name,
                        &l.host,
                        &l.port.to_string(),
                        None,
                        pyreal_launch::Emulator::Unknown,
                    )
                    .map_err(|e| e.to_string())
            });
        match added {
            Ok(slug) => {
                self.state.last_world = Some(slug);
                self.save();
            }
            Err(e) => self.message = Some(e),
        }
    }

    /// What the screens show this frame.
    #[must_use]
    pub fn view(&self) -> FrontDoorView {
        let modern = self.dat_dirs().0;
        let ready = modern.as_deref().is_some_and(|d| {
            let mut checked = self.checked.borrow_mut();
            let previous = checked
                .as_ref()
                .filter(|(at, _)| at == d)
                .map_or(&[][..], |(_, files)| files.as_slice());
            match check_folder_over(SetKind::Modern, d, MODERN_SET, previous) {
                Ok(set) => {
                    *checked = Some((d.to_path_buf(), set.files));
                    true
                }
                Err(_) => {
                    *checked = None;
                    false
                }
            }
        });
        FrontDoorView {
            worlds: self
                .state
                .worlds
                .iter()
                .map(|w| {
                    let account = self
                        .state
                        .world_prefs
                        .get(&w.slug)
                        .and_then(|p| p.account.clone());
                    let password_saved = account
                        .as_deref()
                        .and_then(|a| self.state.account(&w.slug, a))
                        .is_some_and(|a| a.remember);
                    WorldRow {
                        slug: w.slug.clone(),
                        name: w.name.clone(),
                        host: w.host.clone(),
                        port: w.port,
                        account,
                        password_saved,
                    }
                })
                .collect(),
            last_world: self.state.last_world.clone(),
            dats: DatsView {
                modern: modern.map(|d| d.display().to_string()),
                classic: self
                    .pair_dir(SetKind::Classic)
                    .map(|d| d.display().to_string()),
                original: self
                    .pair_dir(SetKind::Original)
                    .map(|d| d.display().to_string()),
                ready,
                problem: (!ready).then(|| "Choose the End of Retail folder to play.".to_owned()),
            },
            message: self
                .message
                .clone()
                .or_else(|| self.preparing.then(|| PREPARING_SHADERS.to_owned())),
            can_browse: cfg!(any(target_os = "macos", windows)),
            directory: self.directory.view(&self.state.worlds, now()),
            update: self.updates.notice(),
        }
    }

    /// One frame's part: the join links the system opened the app with, why a session ended on
    /// its own, the view into the interface, and what the player asked of it carried out.
    pub fn step(&mut self, app: &mut crate::app::App) {
        for link in pyreal_console::take_open_urls() {
            self.add_join_link(&link);
        }
        let ended = app.take_session_end_reason();
        let up = app.in_session();
        // The character list (0xF658) comes only once the server has taken the login.
        let taken = app.host_state().received_set;
        self.follow_session(ended, up, taken);
        self.directory.poll();
        self.updates.start(now());
        self.updates.poll();
        self.collect_pings();
        self.preparing = app.preparing_shaders();
        if let Some((slug, account, typed, remember)) = self.waiting.take() {
            if let Err(e) = self.sign_in(app, &slug, &account, &typed, remember) {
                tracing::warn!("{e}");
                self.message = Some(e);
            }
        }
        let view = self.view();
        let requests = match app.shell_mut().horizon_mut() {
            Some(h) => {
                h.ui.front_door = Some(view);
                std::mem::take(&mut h.ui.front_door_requests)
            }
            None => return,
        };
        for r in requests {
            self.message = None;
            if let Err(e) = self.apply(app, r) {
                tracing::warn!("{e}");
                self.message = Some(e);
            }
        }
    }

    /// The session as the frame found it: why it ended on its own, whether it is still `up`, and
    /// whether the server has `taken` the login, when a password typed for it is kept.
    fn follow_session(&mut self, ended: Option<String>, up: bool, taken: bool) {
        if let Some(reason) = ended {
            self.message = Some(reason);
            self.session = None;
        }
        if self.session.is_some() && !up {
            self.session = None;
        }
        if !taken {
            return;
        }
        let Some(typed) = self.session.as_mut().and_then(|(_, t)| t.take()) else {
            return;
        };
        let target = pyreal_launch::vault::target(&typed.slug, &typed.account);
        if let Err(e) = self.vault.set(&target, &typed.account, &typed.password) {
            tracing::warn!("the secret store: {e}");
            return;
        }
        if let Some(a) = self.state.account(&typed.slug, &typed.account).cloned() {
            self.state.upsert_account(Account {
                remember: true,
                ..a
            });
            self.save();
        }
    }

    fn apply(&mut self, app: &mut crate::app::App, r: FrontDoorRequest) -> Result<(), String> {
        match r {
            FrontDoorRequest::SaveWorld {
                slug,
                name,
                host,
                port,
            } => {
                match slug {
                    Some(slug) => {
                        self.state
                            .edit_custom_world(&slug, &name, &host, &port)
                            .map_err(|e| e.to_string())?;
                    }
                    None => {
                        let slug = self
                            .state
                            .add_custom_world(
                                &name,
                                &host,
                                &port,
                                None,
                                pyreal_launch::Emulator::Unknown,
                            )
                            .map_err(|e| e.to_string())?;
                        self.state.last_world = Some(slug);
                    }
                }
                self.save();
            }
            FrontDoorRequest::RemoveWorld { slug } => self.remove_world(&slug),
            FrontDoorRequest::MoveWorld { slug, up } => {
                if self.state.move_world(&slug, up) {
                    self.save();
                }
            }
            FrontDoorRequest::AddListedWorld { slug } => {
                let listed = self
                    .directory
                    .world(&slug)
                    .cloned()
                    .ok_or_else(|| "That world is no longer on the list.".to_owned())?;
                self.state
                    .add_listed_world(&listed)
                    .map_err(|e| e.to_string())?;
                self.save();
            }
            FrontDoorRequest::ReadDirectory { again } => self.directory.read(again, now()),
            FrontDoorRequest::ForgetPassword { slug, account } => {
                self.forget_password(&slug, &account)?;
            }
            FrontDoorRequest::SetDats {
                modern,
                classic,
                original,
            } => {
                self.set_dats(&modern, &classic, &original)?;
                // Read at once, so Holtburg stands behind the world list.
                if let (Some(modern), classic) = self.dat_dirs() {
                    app.use_data_files(&modern, classic.as_deref())
                        .map_err(|e| e.to_string())?;
                }
            }
            FrontDoorRequest::BrowseDats { folder } => {
                if let Some(picked) = pick_folder() {
                    let text = |p: Option<PathBuf>| p.map(|d| d.display().to_string());
                    let mut modern = text(self.dat_dirs().0).unwrap_or_default();
                    let mut classic = text(self.pair_dir(SetKind::Classic)).unwrap_or_default();
                    let mut original = text(self.pair_dir(SetKind::Original)).unwrap_or_default();
                    *match folder {
                        DatsFolder::Modern => &mut modern,
                        DatsFolder::Classic => &mut classic,
                        DatsFolder::Original => &mut original,
                    } = picked.display().to_string();
                    return self.apply(
                        app,
                        FrontDoorRequest::SetDats {
                            modern,
                            classic,
                            original,
                        },
                    );
                }
            }
            FrontDoorRequest::SignIn {
                slug,
                account,
                password,
                remember,
            } => self.sign_in(app, &slug, &account, &password, remember)?,
            FrontDoorRequest::LogOut => {
                app.end_session();
                self.session = None;
            }
            FrontDoorRequest::Explore { folder } => {
                let start = self.explore_start(folder)?;
                app.begin_explore(&start).map_err(|e| e.to_string())?;
            }
            FrontDoorRequest::EndExplore => app.end_explore(),
            FrontDoorRequest::OpenUpdate => {
                if let Some(url) = self.updates.page() {
                    if !pyreal_client_runtime::app::open_in_browser(url) {
                        return Err("The release page could not be opened.".to_owned());
                    }
                }
            }
        }
        Ok(())
    }

    /// Where explore mode starts over the files in `folder`: the End of Retail set alone, or with
    /// the February 2005 or the 1999 pair, drawn as its era.
    fn explore_start(
        &self,
        folder: DatsFolder,
    ) -> Result<pyreal_client_runtime::explore::ExploreStart, String> {
        let (era, pair) = match folder {
            DatsFolder::Modern => (None, None),
            DatsFolder::Classic => (
                Some(pyreal_primitives::EraId::Infiltration),
                self.pair_dir(SetKind::Classic),
            ),
            DatsFolder::Original => (
                Some(pyreal_primitives::EraId::Launch),
                self.pair_dir(SetKind::Original),
            ),
        };
        let dirs = pyreal_launch::choices::dat_dirs(
            era.map(pyreal_primitives::EraId::name),
            self.dat_dirs().0.as_deref(),
            pair.as_deref(),
        )
        .map_err(missing_set_text)?;
        Ok(pyreal_client_runtime::explore::ExploreStart {
            dat_dir: dirs.dat_dir,
            classic_dat_dir: dirs.classic_dat_dir,
            era,
            place: None,
        })
    }

    /// Whether the world kept under `slug` came from the old launcher.
    fn imported(&self, slug: &str) -> bool {
        self.state.world(slug).is_some_and(|w| w.imported)
    }

    /// Remove a world, with its accounts' passwords.
    fn remove_world(&mut self, slug: &str) {
        let imported = self.imported(slug);
        let accounts: Vec<String> = self
            .state
            .accounts_for(slug)
            .map(|a| a.username.clone())
            .collect();
        for a in accounts {
            let _ = pyreal_launch::vault::forget(self.vault.as_mut(), slug, &a, imported);
        }
        self.state.remove_custom_world(slug);
        self.save();
    }

    /// Forget the password kept for `account` on `slug`.
    fn forget_password(&mut self, slug: &str, account: &str) -> Result<(), String> {
        let imported = self.imported(slug);
        pyreal_launch::vault::forget(self.vault.as_mut(), slug, account, imported)
            .map_err(|e| e.to_string())?;
        if let Some(a) = self.state.account(slug, account).cloned() {
            self.state.upsert_account(Account {
                remember: false,
                ..a
            });
        }
        self.save();
        Ok(())
    }

    /// Keep the folders typed as the default sets, each checked first; an empty pair's field
    /// forgets that pair.
    fn set_dats(&mut self, modern: &str, classic: &str, original: &str) -> Result<(), String> {
        let set = check_folder(SetKind::Modern, Path::new(modern.trim()), MODERN_SET)?;
        let classic = check_pair(SetKind::Classic, classic, CLASSIC_SET)?;
        let original = check_pair(SetKind::Original, original, ORIGINAL_SET)?;
        self.state.adopt_default_set(set);
        for (kind, pair) in [(SetKind::Classic, classic), (SetKind::Original, original)] {
            match pair {
                Some(pair) => self.state.adopt_default_set(pair),
                None => self
                    .state
                    .dat_sets
                    .retain(|s| !(s.kind == kind && s.origin == DatOrigin::Shared)),
            }
        }
        self.save();
        Ok(())
    }

    /// The checks answered since the last frame.
    fn collect_pings(&mut self) {
        let mut answered = Vec::new();
        for (at, reply) in &self.asking {
            match reply.try_recv() {
                Ok(r) => answered.push((at.clone(), r)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    answered.push((at.clone(), Checked::default()))
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        for (at, r) in answered {
            self.asking.remove(&at);
            self.pings.insert(at, r);
        }
    }

    /// A world nobody named an era or a wire for: the era and systems its status ping names, and
    /// the wire it answers the tracker login in ([`check_world`]), asked once a run on a thread of
    /// its own and remembered for the world. A ping that goes unanswered (a server still starting)
    /// is asked again at the next sign-in, and the era remembered stands in for it: without one the
    /// world would be played with the later files. A world that answers neither form of the
    /// tracker login is played as a later world and asked again next time. `false` while the
    /// answer is still to come.
    fn learn_world(&mut self, world: &mut pyreal_launch::World) -> bool {
        let Some(at) = world.endpoint.as_ref().map(|e| (e.address.clone(), e.port)) else {
            return true;
        };
        let ask_era = world.era.is_none() && world.may_answer_status_ping();
        let ask_wire = world.wire.is_none();
        if !ask_era && !ask_wire {
            return true;
        }
        let Some(checked) = self.pings.get(&at).cloned() else {
            if let std::collections::btree_map::Entry::Vacant(asking) = self.asking.entry(at) {
                let (tx, rx) = std::sync::mpsc::channel();
                let (host, port) = asking.key().clone();
                std::thread::spawn(move || {
                    let _ = tx.send(check_world(&host, port, ask_era, ask_wire));
                });
                asking.insert(rx);
            }
            return false;
        };
        if ask_wire {
            self.learn_wire(world, &at, checked.wire);
        }
        if !ask_era || world.era.is_some() {
            return true;
        }
        let Some(r) = checked.status else {
            self.pings.remove(&at);
            let remembered = self.state.remembered_era(&world.slug).map(str::to_owned);
            tracing::warn!(
                "{}:{} did not answer its status ping; it plays {remembered:?}, the era it named last",
                at.0,
                at.1
            );
            if world.era.is_none() && remembered.is_some() {
                world.era = remembered;
                world.era_source = Some(pyreal_launch::world::Told::World);
            }
            return true;
        };
        let systems = pyreal_primitives::EraFeatureBits {
            table_version: r.era_table_version,
            bytes: r.era_features.clone(),
        };
        let era = r.era.trim().to_owned();
        world.take_status_ping(&era, &systems);
        tracing::info!("{}:{} plays {:?} (its status ping)", at.0, at.1, world.era);
        if !era.is_empty() && self.state.remember_era(&world.slug, &era) {
            self.save();
        }
        true
    }

    /// The wire a check found `world` speaks, kept with it; an unanswered probe is asked again at
    /// the next sign-in.
    fn learn_wire(
        &mut self,
        world: &mut pyreal_launch::World,
        at: &(String, u16),
        wire: Option<Answer>,
    ) {
        let word = match wire {
            Some(Answer::Wire1999) => pyreal_launch::world::WIRE_1999,
            Some(Answer::Later) => pyreal_launch::world::WIRE_LATER,
            Some(Answer::Silent) | None => {
                self.pings.remove(at);
                tracing::warn!(
                    "{}:{} answered neither form of the tracker login; it is played as a later world",
                    at.0,
                    at.1
                );
                return;
            }
        };
        world.wire = Some(word.to_owned());
        pyreal_launch::known::apply(world);
        tracing::info!(
            "{}:{} speaks the {word} wire (its tracker login)",
            at.0,
            at.1
        );
        if self.state.remember_wire(&world.slug, word) {
            self.save();
        }
    }

    /// The folders a session on `world` starts with: the Modern set, and the pair its era takes
    /// (the Original set for Launch, the Classic one otherwise).
    fn dat_dirs_for(
        &self,
        world: &pyreal_launch::World,
    ) -> Result<pyreal_launch::choices::DatDirs, String> {
        let pair = self.pair_dir(pyreal_launch::choices::pair_kind_for(world));
        pyreal_launch::choices::dat_dirs_for(world, self.dat_dirs().0.as_deref(), pair.as_deref())
            .map_err(missing_set_text)
    }

    /// The password to log in with: the one `typed`, else (the kept one's dots left standing) the
    /// one kept, read from the secret store only now.
    fn password_for(&mut self, slug: &str, account: &str, typed: &str) -> Result<String, String> {
        if !typed.is_empty() {
            return Ok(typed.to_owned());
        }
        let imported = self.imported(slug);
        pyreal_launch::vault::password(self.vault.as_mut(), slug, account, imported)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Enter the password.".to_owned())
    }

    /// Log in to `slug` as `account`: the password `typed`, else the one kept; kept for next time
    /// when `remember`, once the server takes it.
    fn sign_in(
        &mut self,
        app: &mut crate::app::App,
        slug: &str,
        account: &str,
        typed: &str,
        remember: bool,
    ) -> Result<(), String> {
        let account = account.trim();
        let mut world = self
            .state
            .world(slug)
            .ok_or_else(|| "That world is no longer on the list.".to_owned())?
            .to_world();
        if !self.learn_world(&mut world) {
            self.waiting = Some((
                slug.to_owned(),
                account.to_owned(),
                typed.to_owned(),
                remember,
            ));
            self.message = Some("Checking the world...".to_owned());
            return Ok(());
        }
        self.message = None;
        let password = self.password_for(slug, account, typed)?;
        let dirs = self.dat_dirs_for(&world)?;
        let plan = pyreal_launch::plan(&pyreal_launch::LaunchRequest {
            world: &world,
            account,
            dat_dir: Some(dirs.dat_dir),
            classic_dat_dir: dirs.classic_dat_dir,
            plugin_policy_file: None,
            plugins: Vec::new(),
        })
        .map_err(|e| e.to_string())?;
        tracing::info!("logging in: {}", plan.redacted());
        app.begin_session(&session_start(&plan, &password))
            .map_err(|e| e.to_string())?;
        self.session_begun(plan, slug, account, typed, remember);
        Ok(())
    }

    /// The account, for a session just begun from `plan`; a password `typed` with `remember` waits
    /// for the server to take it ([`Self::follow_session`]).
    fn session_begun(
        &mut self,
        plan: LaunchPlan,
        slug: &str,
        account: &str,
        typed: &str,
        remember: bool,
    ) {
        let was_kept = self
            .state
            .account(slug, account)
            .is_some_and(|a| a.remember);
        let (kept, typed) = if !remember {
            let imported = self.imported(slug);
            if let Err(e) =
                pyreal_launch::vault::forget(self.vault.as_mut(), slug, account, imported)
            {
                tracing::warn!("the secret store: {e}");
            }
            (false, None)
        } else if typed.is_empty() {
            // The kept one, already in the store.
            (true, None)
        } else {
            let typed = Typed {
                slug: slug.to_owned(),
                account: account.to_owned(),
                password: typed.to_owned(),
            };
            (was_kept, Some(typed))
        };
        self.state.upsert_account(Account {
            world_slug: slug.to_owned(),
            username: account.to_owned(),
            label: None,
            remember: kept,
            created_by_launcher: false,
        });
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        self.state.remember_launch(slug, account, now);
        self.save();
        self.session = Some((plan, typed));
    }
}

/// The pair of `kind` in the folder typed, checked; `None` when the field is empty.
fn check_pair(kind: SetKind, typed: &str, id: &str) -> Result<Option<DatSet>, String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Ok(None);
    }
    let set = check_folder(kind, Path::new(typed), id)?;
    match (kind, set.kind) {
        (SetKind::Original, SetKind::Classic) => Err(format!(
            "The portal.dat and cell.dat in {typed} are later than the 1999 Retail CD. Choose \
             that folder for February 2005."
        )),
        (SetKind::Classic, SetKind::Original) => Err(format!(
            "The portal.dat and cell.dat in {typed} are from the 1999 Retail CD. Choose that \
             folder for 1999 Retail CD."
        )),
        _ => Ok(Some(set)),
    }
}

/// What a sign-in needs, naming the Data Files folder to choose.
fn missing_set_text(missing: MissingSet) -> String {
    match missing {
        MissingSet::Modern => "Choose the End of Retail folder in Data Files.",
        MissingSet::Classic => {
            "This world needs the February 2005 files. Choose that folder in Data Files."
        }
        MissingSet::Original => {
            "This world needs the 1999 Retail CD files. Choose that folder in Data Files."
        }
    }
    .to_owned()
}

/// A 1999 set kept as the default pair before it had a row of its own (under the Classic id)
/// takes its own id. Whether it changed.
fn give_the_1999_set_its_id(state: &mut LauncherState) -> bool {
    if state.dat_sets.iter().any(|s| s.id == ORIGINAL_SET) {
        return false;
    }
    match state
        .dat_sets
        .iter_mut()
        .find(|s| s.kind == SetKind::Original && s.id == CLASSIC_SET)
    {
        Some(s) => {
            s.id = ORIGINAL_SET.to_owned();
            true
        }
        None => false,
    }
}

/// One check of a world's address: its status ping when `ask_era`, and when `ask_wire` the wire it
/// speaks, probed only when the ping went unanswered (an answer is the later wire's).
fn check_world(host: &str, port: u16, ask_era: bool, ask_wire: bool) -> Checked {
    let status = if ask_era {
        ping_status(host, port)
    } else {
        None
    };
    let wire = match (&status, ask_wire) {
        (Some(_), _) => Some(Answer::Later),
        (None, true) => Some(probe_wire(host, port)),
        (None, false) => None,
    };
    Checked { status, wire }
}

/// The tracker-login probe ([`pyreal_wire1999::probe::Probe`]) against `host`, from a socket of its
/// own: the later form, then the 1999 one, about 2.4 seconds at most.
fn probe_wire(host: &str, port: u16) -> Answer {
    use std::net::{ToSocketAddrs, UdpSocket};
    use std::time::{Duration, Instant};
    let Some(addr) = (host, port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.find(std::net::SocketAddr::is_ipv4))
    else {
        return Answer::Silent;
    };
    let Ok(sock) = UdpSocket::bind(("0.0.0.0", 0)) else {
        return Answer::Silent;
    };
    if sock
        .set_read_timeout(Some(Duration::from_millis(50)))
        .is_err()
    {
        return Answer::Silent;
    }
    let start = Instant::now();
    let mut probe = pyreal_wire1999::probe::Probe::new();
    let mut buf = [0u8; 1500];
    loop {
        if let Some(a) = probe.answer() {
            return a;
        }
        if let Some(d) = probe.poll(start.elapsed().as_secs_f64()) {
            if sock.send_to(&d, addr).is_err() {
                return Answer::Silent;
            }
        }
        match sock.recv_from(&mut buf) {
            Ok((n, from)) if from.ip() == addr.ip() => probe.receive(&buf[..n]),
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            // The system reporting that nothing listens there: wait out the probe's own timing.
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// The status ping (`pyreal_transport::status_ping`): the hello, then the ask with the token it
/// brings back, two tries a step, 600 ms apiece. `None` from a server that does not answer it.
fn ping_status(host: &str, port: u16) -> Option<StatusReply> {
    use pyreal_transport::status_ping;
    use std::net::{ToSocketAddrs, UdpSocket};
    use std::time::{Duration, Instant};
    let addr = (host, port)
        .to_socket_addrs()
        .ok()?
        .find(std::net::SocketAddr::is_ipv4)?;
    let sock = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    let wait = Duration::from_millis(600);
    sock.set_read_timeout(Some(wait)).ok()?;
    let exchange = |request: &[u8], wanted: &dyn Fn(&[u8]) -> bool| {
        let mut buf = [0u8; 1500];
        for _ in 0..2 {
            sock.send_to(request, addr).ok()?;
            let until = Instant::now() + wait;
            while Instant::now() < until {
                match sock.recv_from(&mut buf) {
                    Ok((n, from)) if from == addr && wanted(&buf[..n]) => {
                        return Some(buf[..n].to_vec());
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }
        None
    };
    let token = exchange(&status_ping::hello(), &|d| {
        status_ping::parse_token_reply(d).is_some()
    })?;
    let ask = status_ping::ask(status_ping::parse_token_reply(&token)?);
    let reply = exchange(&ask, &|d| StatusReply::parse(d).is_some())?;
    StatusReply::parse(&reply)
}

/// A launch plan, with its password, as the runtime starts a session from it.
#[must_use]
pub fn session_start(plan: &LaunchPlan, password: &str) -> SessionStart {
    SessionStart {
        host: plan.host.clone(),
        port: plan.port,
        account: plan.account.clone(),
        password: password.to_owned(),
        dat_dir: plan.dat_dir.clone(),
        classic_dat_dir: plan.classic_dat_dir.clone(),
        world: plan
            .world
            .switches()
            .into_iter()
            .map(|(s, v)| (s.to_owned(), v))
            .chain(
                plan.world_status
                    .iter()
                    .map(|u| ("world-status".to_owned(), u.clone())),
            )
            .collect(),
        plugin_policy: plan.plugin_policy.clone(),
    }
}

/// The system's folder picker; `None` when it was cancelled or there is none.
#[cfg(any(target_os = "macos", windows))]
fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("The folder of the game's data files")
        .pick_folder()
}

/// No picker on this system: the folder is typed.
#[cfg(not(any(target_os = "macos", windows)))]
fn pick_folder() -> Option<PathBuf> {
    None
}

/// Passwords in the system's secret store: the login Keychain on macOS, Credential Manager on
/// Windows, the Secret Service on Linux.
#[derive(Debug, Default)]
pub struct SystemVault;

impl SystemVault {
    fn entry(target: &str) -> Result<keyring::Entry, pyreal_launch::vault::VaultError> {
        let user = target.rsplit('/').next().unwrap_or(target);
        keyring::Entry::new(target, user)
            .map_err(|e| pyreal_launch::vault::VaultError(format!("the secret store: {e}")))
    }
}

impl Vault for SystemVault {
    fn get(&self, target: &str) -> Result<Option<String>, pyreal_launch::vault::VaultError> {
        match Self::entry(target)?.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(pyreal_launch::vault::VaultError(format!(
                "The secret store could not read a password: {e}"
            ))),
        }
    }

    fn set(
        &mut self,
        target: &str,
        _username: &str,
        secret: &str,
    ) -> Result<(), pyreal_launch::vault::VaultError> {
        Self::entry(target)?.set_password(secret).map_err(|e| {
            pyreal_launch::vault::VaultError(format!(
                "The secret store could not keep a password: {e}"
            ))
        })
    }

    fn delete(&mut self, target: &str) -> Result<(), pyreal_launch::vault::VaultError> {
        match Self::entry(target)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(pyreal_launch::vault::VaultError(format!(
                "The secret store could not delete a password: {e}"
            ))),
        }
    }

    /// The store cannot be searched by prefix through this interface.
    fn list(&self, _prefix: &str) -> Result<Vec<String>, pyreal_launch::vault::VaultError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyreal_launch::vault::{legacy_target, MemoryVault};

    fn tmp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("pyreal-front-door-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// A first run imports the old launcher's file from its folder, keeps it as its own, and
    /// leaves the old one as it was; the next run reads its own.
    #[test]
    fn the_first_run_imports_the_old_launchers_worlds_once() {
        let root = tmp("import");
        let (dir, old) = (root.join("Pyreal"), root.join("Dereth/launcher"));
        std::fs::create_dir_all(&old).unwrap();
        let fixture = include_str!("../crates/launch/fixtures/launcher-state-0.4.9.json");
        std::fs::write(old.join("launcher-state.json"), fixture).unwrap();
        let door = FrontDoor::open(&dir, Some(&old), Box::new(MemoryVault::default()), None);
        let view = door.view();
        assert!(view.worlds.iter().any(|w| w.name == "Home"), "{view:?}");
        assert!(dir.join("worlds.json").is_file());
        assert_eq!(
            std::fs::read_to_string(old.join("launcher-state.json")).unwrap(),
            fixture
        );
        std::fs::remove_file(old.join("launcher-state.json")).unwrap();
        let again = FrontDoor::open(&dir, Some(&old), Box::new(MemoryVault::default()), None);
        assert_eq!(again.view().worlds, view.worlds, "its own file, read again");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A worlds.json that is there but cannot be read is no first run: nothing is saved over it.
    #[cfg(unix)]
    #[test]
    fn a_worlds_file_that_cannot_be_read_is_never_saved_over() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmp("unreadable");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("worlds.json");
        let theirs =
            r#"{"schema":1,"worlds":[{"slug":"custom-1","name":"Home","host":"h","port":9000}]}"#;
        std::fs::write(&file, theirs).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        let mut door = FrontDoor::open(&dir, None, Box::new(MemoryVault::default()), None);
        door.add_join_link("pyreal://join?host=play.example.org&port=9050&name=Far");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), theirs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A vault that counts its reads.
    #[derive(Debug, Default)]
    struct Counting {
        inner: MemoryVault,
        reads: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl Vault for Counting {
        fn get(&self, t: &str) -> Result<Option<String>, pyreal_launch::vault::VaultError> {
            self.reads.set(self.reads.get() + 1);
            self.inner.get(t)
        }
        fn set(
            &mut self,
            t: &str,
            u: &str,
            s: &str,
        ) -> Result<(), pyreal_launch::vault::VaultError> {
            self.inner.set(t, u, s)
        }
        fn delete(&mut self, t: &str) -> Result<(), pyreal_launch::vault::VaultError> {
            self.inner.delete(t)
        }
        fn list(&self, p: &str) -> Result<Vec<String>, pyreal_launch::vault::VaultError> {
            self.inner.list(p)
        }
    }

    /// The list says which accounts have a password kept from `worlds.json` alone, never asking
    /// the secret store (which may ask the player); a sign-in with the dots left standing reads
    /// the kept password then, and one typed is taken as typed.
    #[test]
    fn a_kept_password_is_known_without_the_secret_store_and_read_only_to_sign_in() {
        let dir = tmp("kept");
        let reads = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut vault = Counting {
            reads: std::rc::Rc::clone(&reads),
            ..Counting::default()
        };
        let mut door = FrontDoor::open(&dir, None, Box::new(MemoryVault::default()), None);
        door.add_join_link("pyreal://join?host=play.example.org&port=9050&name=Home");
        let slug = door.view().worlds[0].slug.clone();
        vault
            .set(
                &pyreal_launch::vault::target(&slug, "lark"),
                "lark",
                "sesame",
            )
            .unwrap();
        door.vault = Box::new(vault);
        door.state.upsert_account(Account {
            world_slug: slug.clone(),
            username: "lark".into(),
            label: None,
            remember: true,
            created_by_launcher: false,
        });
        door.state.remember_launch(&slug, "lark", 1);
        let view = door.view();
        assert_eq!(view.worlds[0].account.as_deref(), Some("lark"));
        assert!(view.worlds[0].password_saved);
        assert_eq!(reads.get(), 0, "the list never reads the secret store");
        assert!(!format!("{view:?}").contains("sesame"));
        assert_eq!(door.password_for(&slug, "lark", "").unwrap(), "sesame");
        assert_eq!(reads.get(), 1);
        assert_eq!(door.password_for(&slug, "lark", "typed").unwrap(), "typed");
        assert_eq!(reads.get(), 1);
        assert!(door.password_for(&slug, "wren", "").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A world added by hand takes the era and systems its status ping names, asked once a run.
    #[test]
    fn a_world_added_by_hand_takes_the_era_its_status_ping_names() {
        use pyreal_transport::status_ping::{self, Request, Token, WorldState};
        let server = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let answering = std::thread::spawn(move || {
            let token = Token {
                window: 7,
                mac: [3; 16],
            };
            let mut buf = [0u8; 512];
            for _ in 0..2 {
                let (n, from) = server.recv_from(&mut buf).unwrap();
                let reply = match status_ping::parse_request(&buf[..n]) {
                    Some(Request::Hello) => status_ping::token_reply(token),
                    Some(Request::Ask(t)) if t == token => StatusReply {
                        format_version: 1,
                        state: WorldState::Open,
                        players: 0,
                        era: "infiltration".into(),
                        era_table_version: pyreal_primitives::EraFeatureBits::of(
                            pyreal_primitives::EraFeatures::INFILTRATION,
                        )
                        .table_version,
                        era_features: pyreal_primitives::EraFeatureBits::of(
                            pyreal_primitives::EraFeatures::INFILTRATION,
                        )
                        .bytes,
                        software: "Empyrean".into(),
                        software_version: "0.3.11".into(),
                        world_name: "Local 2005".into(),
                    }
                    .encode(),
                    _ => continue,
                };
                server.send_to(&reply, from).unwrap();
            }
        });
        let dir = tmp("ping");
        let mut door = FrontDoor::open(&dir, None, Box::new(MemoryVault::default()), None);
        let slug = door
            .state
            .add_custom_world(
                "Home",
                "127.0.0.1",
                &port.to_string(),
                None,
                pyreal_launch::Emulator::Unknown,
            )
            .unwrap();
        let mut world = door.state.world(&slug).unwrap().to_world();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !door.learn_world(&mut world) {
            assert!(std::time::Instant::now() < deadline, "no answer");
            std::thread::sleep(std::time::Duration::from_millis(10));
            door.collect_pings();
        }
        answering.join().unwrap();
        assert_eq!(world.era.as_deref(), Some("infiltration"));
        let features = world.era_features.clone().expect("the systems");
        assert!(features.contains("ratings=false"), "{features}");
        // Asked once a run: the second sign-in takes the answer kept.
        let mut again = door.state.world(&slug).unwrap().to_world();
        assert!(door.learn_world(&mut again));
        assert_eq!(again.era, world.era);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A status ping answered with `era` once, on a port of its own, which then goes quiet.
    fn answer_one_ping(era: &'static str) -> (u16, std::thread::JoinHandle<()>) {
        use pyreal_transport::status_ping::{self, Request, Token, WorldState};
        let server = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let answering = std::thread::spawn(move || {
            let token = Token {
                window: 7,
                mac: [3; 16],
            };
            let bits =
                pyreal_primitives::EraFeatureBits::of(pyreal_primitives::EraFeatures::LAUNCH);
            let mut buf = [0u8; 512];
            for _ in 0..2 {
                let (n, from) = server.recv_from(&mut buf).unwrap();
                let reply = match status_ping::parse_request(&buf[..n]) {
                    Some(Request::Hello) => status_ping::token_reply(token),
                    Some(Request::Ask(t)) if t == token => StatusReply {
                        format_version: 1,
                        state: WorldState::Open,
                        players: 0,
                        era: era.into(),
                        era_table_version: bits.table_version,
                        era_features: bits.bytes.clone(),
                        software: "Empyrean".into(),
                        software_version: "0.3.15".into(),
                        world_name: "Local Launch".into(),
                    }
                    .encode(),
                    _ => continue,
                };
                server.send_to(&reply, from).unwrap();
            }
        });
        (port, answering)
    }

    /// The era a world's ping last named holds when its ping later goes unanswered, as it does
    /// while the server starts: a Launch world still signs in with the 1999 pair and is told its
    /// era, rather than being played with the later files. The unanswered ping is asked again.
    #[test]
    fn an_unanswered_status_ping_keeps_the_era_the_world_named_last() {
        let root = tmp("ping-lost");
        let [m, c, o] = three_sets(&root);
        let dir = root.join("Pyreal");
        let (port, answering) = answer_one_ping("launch");
        let learn = |door: &mut FrontDoor, world: &mut pyreal_launch::World| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !door.learn_world(world) {
                assert!(std::time::Instant::now() < deadline, "no answer");
                std::thread::sleep(std::time::Duration::from_millis(10));
                door.collect_pings();
            }
        };
        let slug = {
            let mut door = FrontDoor::open(&dir, None, Box::new(MemoryVault::default()), None);
            door.set_dats(&m, &c, &o).unwrap();
            let slug = door
                .state
                .add_custom_world(
                    "Launch",
                    "127.0.0.1",
                    &port.to_string(),
                    None,
                    pyreal_launch::Emulator::Unknown,
                )
                .unwrap();
            let mut world = door.state.world(&slug).unwrap().to_world();
            learn(&mut door, &mut world);
            assert_eq!(world.era.as_deref(), Some("launch"));
            assert_eq!(door.state.remembered_era(&slug), Some("launch"));
            slug
        };
        answering.join().unwrap();

        // The next run: the world's server is not up yet.
        let mut door = FrontDoor::open(&dir, None, Box::new(MemoryVault::default()), None);
        let mut world = door.state.world(&slug).unwrap().to_world();
        learn(&mut door, &mut world);
        assert_eq!(world.era.as_deref(), Some("launch"), "the era named last");
        assert!(door.pings.is_empty(), "asked again at the next sign-in");
        let dirs = door.dat_dirs_for(&world).unwrap();
        assert_eq!(
            (dirs.dat_dir.clone(), dirs.classic_dat_dir.clone()),
            (m.into(), Some(o.into()))
        );
        let plan = pyreal_launch::plan(&pyreal_launch::LaunchRequest {
            world: &world,
            account: "ethangar",
            dat_dir: Some(dirs.dat_dir),
            classic_dat_dir: dirs.classic_dat_dir,
            plugin_policy_file: None,
            plugins: Vec::new(),
        })
        .unwrap();
        assert!(session_start(&plan, "pw")
            .world
            .contains(&("era".to_owned(), "launch".to_owned())));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A world that does not answer its status ping does not hold the frame while it is asked.
    #[test]
    fn a_status_ping_is_not_waited_for_on_the_frame() {
        let silent = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = silent.local_addr().unwrap().port();
        let dir = tmp("silent-ping");
        let mut door = FrontDoor::open(&dir, None, Box::new(MemoryVault::default()), None);
        let slug = door
            .state
            .add_custom_world(
                "Quiet",
                "127.0.0.1",
                &port.to_string(),
                None,
                pyreal_launch::Emulator::Unknown,
            )
            .unwrap();
        let mut world = door.state.world(&slug).unwrap().to_world();
        let started = std::time::Instant::now();
        let _ = door.learn_world(&mut world);
        assert!(
            started.elapsed() < std::time::Duration::from_millis(300),
            "{:?}",
            started.elapsed()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A stand-in server on loopback that answers each datagram `answer` returns something for,
    /// until `count` datagrams have come.
    fn stand_in(
        count: usize,
        answer: impl Fn(&[u8]) -> Option<Vec<u8>> + Send + 'static,
    ) -> (u16, std::thread::JoinHandle<Vec<Vec<u8>>>) {
        let server = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        server
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let port = server.local_addr().unwrap().port();
        let serving = std::thread::spawn(move || {
            let mut seen = Vec::new();
            let mut buf = [0u8; 1500];
            while seen.len() < count {
                let Ok((n, from)) = server.recv_from(&mut buf) else {
                    break;
                };
                if let Some(reply) = answer(&buf[..n]) {
                    server.send_to(&reply, from).unwrap();
                }
                seen.push(buf[..n].to_vec());
            }
            seen
        });
        (port, serving)
    }

    /// Learns `slug`'s world as a sign-in does, waiting for the check.
    fn learned(door: &mut FrontDoor, slug: &str) -> pyreal_launch::World {
        let mut world = door.state.world(slug).unwrap().to_world();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !door.learn_world(&mut world) {
            assert!(std::time::Instant::now() < deadline, "no answer");
            std::thread::sleep(std::time::Duration::from_millis(10));
            door.collect_pings();
        }
        world
    }

    fn hand_added(door: &mut FrontDoor, port: u16) -> String {
        door.state
            .add_custom_world(
                "Hand",
                "127.0.0.1",
                &port.to_string(),
                None,
                pyreal_launch::Emulator::Unknown,
            )
            .unwrap()
    }

    /// A world added by hand that ignores the status ping and the later tracker login and refuses
    /// the 1999 one, as the AC1999 server does, is a 1999 world: it is remembered, plays Launch,
    /// and signs in with `--wire 1999`. The next run asks nothing.
    #[test]
    fn a_world_that_answers_only_the_1999_tracker_login_signs_in_over_the_1999_wire() {
        use pyreal_wire1999::packet::{flags, unkeyed, Datagram};
        let (port, serving) = stand_in(5, |d| {
            // The 1999 login: sequence 1, a 96-byte body.
            Datagram::parse(d)
                .filter(|d| d.header.seq == 1 && d.body.len() == 96)
                .map(|_| {
                    unkeyed(
                        0,
                        flags::NET_ERROR,
                        0,
                        0,
                        0,
                        &[0x54, 0x9C, 0xDF, 0x04, 0, 0, 0, 0, 0, 0, 0, 0],
                    )
                })
        });
        let root = tmp("probe-1999");
        let [m, c, o] = three_sets(&root);
        let dir = root.join("Pyreal");
        let slug = {
            let mut door = FrontDoor::open(&dir, None, Box::new(MemoryVault::default()), None);
            door.set_dats(&m, &c, &o).unwrap();
            let slug = hand_added(&mut door, port);
            door.save();
            let world = learned(&mut door, &slug);
            assert!(world.speaks_1999());
            assert_eq!(world.era.as_deref(), Some("launch"));
            let dirs = door.dat_dirs_for(&world).unwrap();
            assert_eq!(
                dirs.classic_dat_dir,
                Some(o.clone().into()),
                "the 1999 pair"
            );
            let plan = pyreal_launch::plan(&pyreal_launch::LaunchRequest {
                world: &world,
                account: "ethangar",
                dat_dir: Some(dirs.dat_dir),
                classic_dat_dir: dirs.classic_dat_dir,
                plugin_policy_file: None,
                plugins: Vec::new(),
            })
            .unwrap();
            let start = session_start(&plan, "pw");
            assert!(start
                .world
                .contains(&("wire".to_owned(), "1999".to_owned())));
            slug
        };
        let seen = serving.join().unwrap();
        let tracker = seen
            .iter()
            .filter(|d| pyreal_wire1999::probe::is_later_reply(d))
            .count();
        assert_eq!(tracker, 2, "the later form twice, after two status hellos");
        let mut door = FrontDoor::open(&dir, None, Box::new(MemoryVault::default()), None);
        let mut world = door.state.world(&slug).unwrap().to_world();
        assert!(world.speaks_1999(), "kept in worlds.json");
        assert!(door.learn_world(&mut world), "nothing to ask");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A world that answers the later tracker login is a later world, kept so, and signs in with no
    /// wire switch.
    #[test]
    fn a_world_that_answers_the_later_tracker_login_is_a_later_world() {
        use pyreal_transport::{OutPacket, PacketFlags, ProtoHeader};
        let (port, serving) = stand_in(3, |d| {
            pyreal_wire1999::probe::is_later_reply(d)
                .then(|| {
                    // What ACE answers: the connect request, unencrypted.
                    let mut p = OutPacket::new(ProtoHeader::default());
                    p.add_optional_header(PacketFlags::CONNECT_REQUEST, vec![0; 32])
                        .ok()?;
                    p.serialize(None).ok()
                })
                .flatten()
        });
        let dir = tmp("probe-later");
        let mut door = FrontDoor::open(&dir, None, Box::new(MemoryVault::default()), None);
        let slug = hand_added(&mut door, port);
        let world = learned(&mut door, &slug);
        assert_eq!(world.wire.as_deref(), Some("later"));
        assert!(!world.speaks_1999());
        assert_eq!(
            door.state.world(&slug).unwrap().wire.as_deref(),
            Some("later")
        );
        let seen = serving.join().unwrap();
        assert!(
            !seen.iter().any(|d| d.len() == 20 + 96),
            "no 1999 login is sent to a later world"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Folders holding the later files, the February 2005 pair and the 1999 pair, under `root`.
    fn three_sets(root: &Path) -> [String; 3] {
        use pyreal_launch::testing::{write_classic_set, write_dat_set};
        let (m, c, o) = (root.join("eor"), root.join("2005"), root.join("1999"));
        write_dat_set(&m, pyreal_launch::datset::Iterations::END_OF_RETAIL);
        write_classic_set(&c, 2112, 1593);
        write_classic_set(&o, 102, 67);
        [m, c, o].map(|d| d.display().to_string())
    }

    /// The 2005 pair and the 1999 pair each keep a row of their own; a folder of the wrong kind
    /// says so and changes nothing, and an empty field forgets its pair.
    #[test]
    fn the_2005_and_1999_pairs_each_keep_their_own_folder() {
        let root = tmp("pairs");
        let [m, c, o] = three_sets(&root);
        let mut door = FrontDoor::open(
            &root.join("Pyreal"),
            None,
            Box::new(MemoryVault::default()),
            None,
        );
        door.set_dats(&m, &c, &o).unwrap();
        let dats = door.view().dats;
        assert!(dats.ready);
        assert_eq!(
            (dats.classic.as_ref(), dats.original.as_ref()),
            (Some(&c), Some(&o))
        );
        let e = door.set_dats(&m, &o, &o).unwrap_err();
        assert!(e.contains("from the 1999 Retail CD"), "{e}");
        let e = door.set_dats(&m, &c, &c).unwrap_err();
        assert!(e.contains("later than the 1999 Retail CD"), "{e}");
        assert_eq!(door.view().dats, dats, "a refused folder changes nothing");
        door.set_dats(&m, &c, "").unwrap();
        let dats = door.view().dats;
        assert_eq!((dats.classic.as_ref(), dats.original), (Some(&c), None));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A Launch world signs in with the 1999 pair and a 2005 world with the February 2005 one;
    /// a missing pair names the folder to choose.
    #[test]
    fn a_world_signs_in_with_the_pair_of_its_era() {
        let root = tmp("era-pair");
        let [m, c, o] = three_sets(&root);
        let mut door = FrontDoor::open(
            &root.join("Pyreal"),
            None,
            Box::new(MemoryVault::default()),
            None,
        );
        door.set_dats(&m, &c, &o).unwrap();
        let mut w = pyreal_launch::World::new("custom-1", "Home");
        let second = |door: &FrontDoor, w: &pyreal_launch::World| {
            door.dat_dirs_for(w).map(|d| (d.dat_dir, d.classic_dat_dir))
        };
        w.era = Some("launch".into());
        assert_eq!(
            second(&door, &w),
            Ok((m.clone().into(), Some(o.clone().into())))
        );
        w.era = Some("infiltration".into());
        assert_eq!(
            second(&door, &w),
            Ok((m.clone().into(), Some(c.clone().into())))
        );
        w.era = None;
        assert_eq!(
            second(&door, &w),
            Ok((m.clone().into(), Some(c.clone().into())))
        );
        door.set_dats(&m, "", "").unwrap();
        w.era = Some("launch".into());
        let e = second(&door, &w).unwrap_err();
        assert!(e.contains("needs the 1999 Retail CD files"), "{e}");
        w.era = Some("infiltration".into());
        let e = second(&door, &w).unwrap_err();
        assert!(e.contains("needs the February 2005 files"), "{e}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A 1999 set kept as the default pair under the Classic id takes its own, once.
    #[test]
    fn a_1999_set_kept_as_the_old_pair_takes_its_own_id() {
        let root = tmp("old-pair");
        let [_, _, o] = three_sets(&root);
        let mut state = LauncherState::default();
        state
            .adopt_default_set(check_folder(SetKind::Classic, Path::new(&o), CLASSIC_SET).unwrap());
        assert_eq!(state.dat_sets[0].kind, SetKind::Original);
        assert!(give_the_1999_set_its_id(&mut state));
        assert_eq!(state.dat_sets[0].id, ORIGINAL_SET);
        assert!(!give_the_1999_set_its_id(&mut state));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A join link adds its world and selects it; a bad one says why.
    #[test]
    fn a_join_link_adds_its_world() {
        let dir = tmp("join");
        let mut door = FrontDoor::open(&dir, None, Box::new(MemoryVault::default()), None);
        door.add_join_link("pyreal://join?host=play.example.org&port=9050&name=Home");
        let view = door.view();
        assert_eq!(view.worlds.len(), 1);
        assert_eq!(view.worlds[0].port, 9050);
        assert_eq!(
            view.last_world.as_deref(),
            Some(view.worlds[0].slug.as_str())
        );
        door.add_join_link("pyreal://join?host=a%20b&port=1");
        assert!(door.view().message.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// While the device prepares its shaders the screens say so, unless they have something of
    /// their own to say; once it is done the note goes.
    #[test]
    fn the_screens_say_the_shaders_are_being_prepared() {
        let dir = tmp("preparing");
        let mut door = FrontDoor::open(&dir, None, Box::new(MemoryVault::default()), None);
        assert_eq!(door.view().message, None);
        door.preparing = true;
        assert_eq!(door.view().message.as_deref(), Some(PREPARING_SHADERS));
        door.message = Some("The world did not answer.".into());
        assert_eq!(
            door.view().message.as_deref(),
            Some("The world did not answer.")
        );
        door.message = None;
        door.preparing = false;
        assert_eq!(door.view().message, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A launch plan becomes the session the runtime starts: the address, the account, the
    /// password, the data folders and the world's switches.
    #[test]
    fn a_plan_becomes_the_session_start() {
        let mut w = pyreal_launch::World::new("custom-1", "Home");
        w.endpoint = Some(pyreal_launch::world::Endpoint {
            address: "127.0.0.1".into(),
            port: 9000,
            transport: None,
        });
        w.era = Some("infiltration".into());
        let plan = pyreal_launch::plan(&pyreal_launch::LaunchRequest {
            world: &w,
            account: "Lark",
            dat_dir: Some(PathBuf::from("/dats")),
            classic_dat_dir: Some(PathBuf::from("/classic")),
            plugin_policy_file: None,
            plugins: Vec::new(),
        })
        .unwrap();
        let start = session_start(&plan, "pw");
        assert_eq!((start.host.as_str(), start.port), ("127.0.0.1", 9000));
        assert_eq!(
            (start.account.as_str(), start.password.as_str()),
            ("Lark", "pw")
        );
        assert_eq!(start.classic_dat_dir, Some(PathBuf::from("/classic")));
        assert_eq!(
            start.world,
            vec![("era".to_owned(), "infiltration".to_owned())]
        );
        let mut cfg = pyreal_client_runtime::config::Config::default();
        for (s, v) in &start.world {
            cfg.apply_world_switch(s, v).unwrap();
        }
        assert_eq!(cfg.era, Some(pyreal_primitives::EraId::Infiltration));
        assert!(
            !format!("{start:?}").contains("pw\""),
            "the password is never shown"
        );
    }

    /// A door whose world `custom-1` and its account `lark` came from the old launcher, whose
    /// password for it is in the vault.
    fn imported_door(name: &str) -> (PathBuf, FrontDoor) {
        let root = tmp(name);
        let old = root.join("Dereth/launcher");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(
            old.join("launcher-state.json"),
            r#"{"schema":1,
                "custom_worlds":[{"slug":"custom-1","name":"Home","host":"127.0.0.1","port":9000}],
                "accounts":[{"world_slug":"custom-1","username":"lark","remember":true}]}"#,
        )
        .unwrap();
        let mut vault = MemoryVault::default();
        vault
            .set(&legacy_target("custom-1", "lark"), "lark", "old")
            .unwrap();
        let door = FrontDoor::open(&root.join("Pyreal"), Some(&old), Box::new(vault), None);
        (root, door)
    }

    /// The old launcher's password is found for a world imported from it, never for one added
    /// here under the same slug.
    #[test]
    fn only_an_imported_world_takes_the_old_launchers_password() {
        let (root, mut door) = imported_door("legacy-imported");
        assert_eq!(door.password_for("custom-1", "lark", "").unwrap(), "old");
        let _ = std::fs::remove_dir_all(&root);

        let dir = tmp("legacy-added");
        let mut vault = MemoryVault::default();
        vault
            .set(&legacy_target("custom-1", "lark"), "lark", "old")
            .unwrap();
        let mut door = FrontDoor::open(&dir, None, Box::new(vault), None);
        door.add_join_link("pyreal://join?host=play.example.org&port=9050&name=New");
        let slug = door.view().worlds[0].slug.clone();
        assert_eq!(slug, "custom-1");
        assert_eq!(
            door.password_for(&slug, "lark", ""),
            Err("Enter the password.".to_owned()),
            "a new world never takes an old world's password"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Forgetting a password or removing its world forgets the old launcher's entry too, so the
    /// password never comes back.
    #[test]
    fn forgetting_or_removing_forgets_the_old_launchers_password_too() {
        let (root, mut door) = imported_door("legacy-forget");
        door.forget_password("custom-1", "lark").unwrap();
        assert_eq!(
            door.vault.get(&legacy_target("custom-1", "lark")).unwrap(),
            None
        );
        assert!(door.password_for("custom-1", "lark", "").is_err());
        let _ = std::fs::remove_dir_all(&root);

        let (root, mut door) = imported_door("legacy-remove");
        door.remove_world("custom-1");
        assert_eq!(
            door.vault.get(&legacy_target("custom-1", "lark")).unwrap(),
            None
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A password typed to sign in is kept only once the server has taken it.
    #[test]
    fn a_typed_password_is_kept_only_once_the_server_takes_it() {
        let dir = tmp("kept-on-accept");
        let mut door = FrontDoor::open(&dir, None, Box::new(MemoryVault::default()), None);
        door.add_join_link("pyreal://join?host=127.0.0.1&port=9000&name=Home");
        let slug = door.view().worlds[0].slug.clone();
        let plan_for = |door: &FrontDoor| {
            let world = door.state.world(&slug).unwrap().to_world();
            pyreal_launch::plan(&pyreal_launch::LaunchRequest {
                world: &world,
                account: "lark",
                dat_dir: Some(PathBuf::from("/dats")),
                classic_dat_dir: None,
                plugin_policy_file: None,
                plugins: Vec::new(),
            })
            .unwrap()
        };
        let target = pyreal_launch::vault::target(&slug, "lark");
        door.session_begun(plan_for(&door), &slug, "lark", "sesame", true);
        assert_eq!(
            door.vault.get(&target).unwrap(),
            None,
            "not kept before the server takes it"
        );
        assert!(!door.view().worlds[0].password_saved);
        door.follow_session(None, true, false);
        assert_eq!(door.vault.get(&target).unwrap(), None, "still connecting");
        door.follow_session(None, true, true);
        assert_eq!(door.vault.get(&target).unwrap().as_deref(), Some("sesame"));
        assert!(door.view().worlds[0].password_saved);
        // A login refused keeps nothing, and the password kept before stays.
        door.session_begun(plan_for(&door), &slug, "lark", "wrong", true);
        door.follow_session(Some("refused".into()), false, false);
        door.follow_session(None, false, true);
        assert_eq!(door.vault.get(&target).unwrap().as_deref(), Some("sesame"));
        assert!(door.view().worlds[0].password_saved);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
