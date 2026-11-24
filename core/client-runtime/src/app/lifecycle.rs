//! Sessions in process: starting one on a world and ending it, back to the screens before any
//! world, with the window, the device and the sound device kept.

use super::*;

/// What a session is started with in process: the server, the account, the data files and what
/// the world says of itself.
#[derive(Clone, Default)]
pub struct SessionStart {
    pub host: String,
    pub port: u16,
    pub account: String,
    pub password: String,
    /// The Modern set, and the Classic one when it is elsewhere.
    pub dat_dir: std::path::PathBuf,
    pub classic_dat_dir: Option<std::path::PathBuf>,
    /// The world's switches as the command line spells them, without the dashes: `world-base`,
    /// `era`, `era-features`, `logon-version`, `world-profile`, `world-status` and `wire`
    /// ([`crate::config::Config::apply_world_switch`]).
    pub world: Vec<(String, String)>,
    /// The world's plugin policy, a URL or a file.
    pub plugin_policy: Option<String>,
}

impl std::fmt::Debug for SessionStart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionStart")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("account", &self.account)
            .field("password", &"***")
            .field("dat_dir", &self.dat_dir)
            .field("classic_dat_dir", &self.classic_dat_dir)
            .field("world", &self.world)
            .field("plugin_policy", &self.plugin_policy)
            .finish()
    }
}

impl<S: Shell> App<S> {
    /// Whether a session is up: a link to a server, connected or connecting.
    #[must_use]
    pub fn in_session(&self) -> bool {
        self.link.is_some()
    }

    /// Whether the device is still preparing its shaders, long enough for the screens before the
    /// world to say so ([`crate::present::Presentation::preparing_shaders`]).
    #[must_use]
    pub fn preparing_shaders(&self) -> bool {
        self.present.preparing_shaders()
    }

    /// Start a session on `start`'s server: the world's facts into the configuration, its data
    /// files opened when they are not the ones open, and the link made. A session already up is
    /// ended first.
    ///
    /// # Errors
    /// [`StartupError::CommandLine`] for a world switch that does not read, [`StartupError::DataFiles`]
    /// for data files that do not open, [`StartupError::Net`] for a host that does not resolve or
    /// a socket that will not bind.
    pub fn begin_session(
        &mut self,
        shell: &mut S,
        start: &SessionStart,
    ) -> Result<(), StartupError> {
        self.prepare_session(shell, start)?;
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: the deliberate 32-bit wrap of a millisecond counter, not a float conversion.
        let seq = (crate::platform::clock::system_unix_time().map_or(0, |d| d.as_millis())
            & u128::from(u32::MAX)) as u32;
        let (port, client_port) = self
            .cfg
            .connect_ports()
            .map_err(|e| StartupError::CommandLine(e.detail))?;
        let link = if self.cfg.wire_1999 {
            NetLink::connect_1999(
                &self.cfg.host,
                port,
                client_port,
                &self.cfg.account,
                &self.cfg.vg_password,
                crate::net::login_iterations(&self.store),
                &crate::net::world_name_1999(&self.cfg),
            )?
        } else {
            let mut link = NetLink::connect(
                &self.cfg.host,
                port,
                client_port,
                &self.cfg.account,
                &self.cfg.vg_password,
                seq,
            )?;
            link.net.set_logon_version(&self.cfg.logon_version);
            link
        };
        self.interaction.set_extensions(Vec::new());
        self.link = Some(link);
        tracing::info!(
            "session begun: {} on {}:{}",
            self.cfg.account_name(),
            self.cfg.host,
            self.cfg.port
        );
        Ok(())
    }

    /// [`Self::begin_session`] over an endpoint the caller built: a socket-free one whose
    /// datagrams the caller carries (a test's shard).
    ///
    /// # Errors
    /// As [`Self::begin_session`], but for the network's.
    pub fn begin_session_on(
        &mut self,
        shell: &mut S,
        start: &SessionStart,
        mut net: crate::net::ClientNetwork,
    ) -> Result<(), StartupError> {
        self.prepare_session(shell, start)?;
        net.set_logon_version(&self.cfg.logon_version);
        self.link = Some(NetLink::replay(net));
        Ok(())
    }

    /// Read the data files in `dat_dir` (and the Classic pair in `classic`) from now on, before
    /// any session: the screens before the world draw over them.
    ///
    /// # Errors
    /// [`StartupError::DataFiles`] when they will not open.
    pub fn use_data_files(
        &mut self,
        dat_dir: &std::path::Path,
        classic: Option<&std::path::Path>,
    ) -> Result<(), StartupError> {
        if self.link.is_some()
            || (self.cfg.dat_dir == dat_dir && self.cfg.classic_dat_dir.as_deref() == classic)
        {
            return Ok(());
        }
        let mut cfg = self.cfg.clone();
        cfg.dat_dir = dat_dir.to_path_buf();
        cfg.classic_dat_dir = classic.map(std::path::Path::to_path_buf);
        let (store, _) = crate::assets::open_store_noting(&cfg)?;
        self.cfg = cfg;
        self.base_store = None;
        self.adopt_store(std::sync::Arc::new(store));
        self.apply_world_facts();
        self.reload_store_tables();
        tracing::info!("the data files of {} opened", self.cfg.dat_dir.display());
        Ok(())
    }

    /// Everything [`Self::begin_session`] does before the link.
    fn prepare_session(&mut self, shell: &mut S, start: &SessionStart) -> Result<(), StartupError> {
        if self.link.is_some() {
            self.end_session(shell);
        }
        self.end_explore(shell);
        let defaults = crate::config::Config::default();
        let mut cfg = self.cfg.clone();
        cfg.era = defaults.era;
        cfg.world_base = defaults.world_base;
        cfg.era_features = defaults.era_features;
        cfg.logon_version = defaults.logon_version;
        cfg.wire_1999 = defaults.wire_1999;
        cfg.world_rules = defaults.world_rules;
        cfg.world_status = defaults.world_status;
        for (switch, value) in &start.world {
            cfg.apply_world_switch(switch, value)
                .map_err(|e| StartupError::CommandLine(e.to_string()))?;
        }
        cfg.host.clone_from(&start.host);
        cfg.port = u32::from(start.port);
        cfg.account = start.account.to_lowercase();
        cfg.account_as_typed.clone_from(&start.account);
        cfg.vg_password.clone_from(&start.password);
        cfg.connect = true;
        cfg.plugin_policy.clone_from(&start.plugin_policy);
        // The set the world is drawn from is held to the files open too: the world's folder may
        // have changed since they were opened (a world that named itself as another).
        let world_set = crate::assets::world_set(&cfg);
        let files_change = cfg.dat_dir != start.dat_dir
            || cfg.classic_dat_dir != start.classic_dat_dir
            || world_set != crate::assets::world_set(&self.cfg)
            || world_set.is_some_and(|set| set != self.store.era());
        cfg.dat_dir.clone_from(&start.dat_dir);
        cfg.classic_dat_dir.clone_from(&start.classic_dat_dir);
        // The world's served records, kept from its last session, are read before its characters
        // are: files opened at the front door carry none, or another server's.
        let files_change = files_change
            || crate::world_overlay::start_world(&cfg)
                != crate::world_overlay::laid_world(&self.store);
        if files_change {
            let (store, refused) = crate::assets::open_store_noting(&cfg)?;
            if let Some(refusal) = refused {
                return Err(StartupError::DataFiles(crate::assets::DataFilesError {
                    cause: format!("the world's overlay is refused: {refusal:?}"),
                }));
            }
            self.cfg = cfg;
            self.base_store = None;
            self.adopt_store(std::sync::Arc::new(store));
            self.apply_world_facts();
            // The tables read over the files before (the spells among them) are read again.
            self.reload_store_tables();
            tracing::info!("the data files of {} opened", self.cfg.dat_dir.display());
        } else {
            self.cfg = cfg;
            self.apply_world_facts();
        }
        self.ddd = crate::world_overlay::patcher(&self.store, &self.cfg);
        self.fetch_map_places();
        self.server_stub = None;
        self.connect_failure = None;
        self.last_net_error = None;
        self.script = EnterWorldScript::default();
        Ok(())
    }

    /// What the world's server publishes, read from the documents its status names off the
    /// frame's thread ([`crate::published`]): its places for the map, handed to the interface when
    /// they come, and its portal table for the travel when `--nav-published` gives none. A fetch
    /// for an earlier world lands nowhere.
    fn fetch_map_places(&mut self) {
        self.host_state.map_places = None;
        self.published_places = std::sync::Arc::default();
        let cfg = &self.cfg;
        let status = crate::weenie_list::beside_status(
            cfg.world_status.as_deref(),
            &cfg.host,
            cfg.port,
            "status",
        );
        let (Ok(status), Some(fetch)) = (status, self.weenie_lists.fetch.clone()) else {
            return;
        };
        let want_portals = cfg.nav_published.is_none();
        let places = std::sync::Arc::clone(&self.published_places);
        let portals = std::sync::Arc::clone(&self.published_portals);
        let spawned = std::thread::Builder::new()
            .name("World Published".to_owned())
            .spawn(move || {
                let got = crate::published::read(&status, want_portals, &fetch, &mut |s| {
                    std::thread::sleep(std::time::Duration::from_secs_f64(s));
                });
                for (text, slot) in [(got.places, places), (got.portals, portals)] {
                    if let (Some(text), Ok(mut slot)) = (text, slot.lock()) {
                        *slot = Some(text);
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("what the world publishes is not fetched: {e}");
        }
    }

    /// End the session, if one is up, and everything of it: the character's world, its objects
    /// and sounds, the HUD's and the interface's state, the plugin host's frame and the link,
    /// after a goodbye to the server. The window, the device and the data files stay, and the
    /// front end is back before any world.
    pub fn end_session(&mut self, shell: &mut S) {
        use pyreal_client_net::client_session::SessionEvent;
        if let Some(link) = self.link.as_mut() {
            link.net.session.log_off();
            let (sent, queued) = link.log_off_server();
            tracing::info!("session ended: 0x8000 Disconnect to {sent} of {queued} connection(s)");
        }
        self.link = None;
        // The password is not kept past its session.
        self.cfg.vg_password.clear();
        crate::net::link_status_holder::disconnected();
        let now = pyreal_primitives::LocalTime(self.timer.cur_time);
        // What the server's log-off and the next entry's reset tear down.
        let _ = self.objects.apply_event(&SessionEvent::LoggedOff, now);
        self.objects.end_session();
        self.reset_world_view();
        if let Some(audio) = self.audio.as_mut() {
            audio.end_world();
        }
        self.interaction.on_end_character_session();
        self.objects.world.magic.busy_count = 0;
        self.command_interpreter_disable();
        self.movement = crate::character::MovementCommands::default();
        self.char_input = crate::character::CharacterInput::default();
        self.teleport = crate::teleport::Teleport::new();
        self.hud = S::Hud::default();
        self.dialogs = crate::dialogs::DialogService::default();
        self.host_state = HostState {
            chargen_seeds: self.host_state.chargen_seeds,
            world_rules: self.host_state.world_rules.clone(),
            ..HostState::default()
        };
        self.pending_ddd.clear();
        self.bulk = None;
        self.ddd_invalidation = None;
        self.connect_failure = None;
        self.last_net_error = None;
        self.last_link_status = None;
        self.last_rejected = 0;
        self.script = EnterWorldScript::default();
        self.playable_at = None;
        self.position =
            pyreal_client_net::client_session::PositionReporter::new(self.timer.cur_time);
        self.last_jump_request = None;
        self.nav_spells = None;
        self.nav_academies = None;
        self.travel_fizzles = 0;
        self.travel.end_session();
        self.held_events.clear();
        self.teleport_hold = None;
        self.travel_status = crate::travel::TravelStatus::default();
        self.hook_chat.clear();
        self.hook_outcomes.clear();
        self.hook_messages.clear();
        self.hook_quit_pending = false;
        // `movement` starts its loss count over, so the hook's copy does too.
        self.hook_control_losses = 0;
        self.duties.leaving_world = false;
        self.duties.quit_owed = false;
        self.apply_world_facts();
        self.load_store_tables();
        shell.session_ended(&mut UiContext::new(self));
    }

    /// Why the last session ended without the player asking (its login refused), once.
    pub fn take_session_end_reason(&mut self) -> Option<String> {
        self.session_end_reason.take()
    }

    /// End the session a failed login left owed, at the end of the frame that found it.
    pub(super) fn end_owed_session(&mut self, shell: &mut S) {
        if std::mem::take(&mut self.session_end_owed) {
            self.end_session(shell);
        }
    }

    /// The world's facts, from the configuration into what reads them: its era and systems, and
    /// the rules its own client played by.
    pub(super) fn apply_world_facts(&mut self) {
        self.hud.era.era_announced = self.cfg.era.is_some();
        self.hud.era.era = self.cfg.era.unwrap_or_else(|| {
            pyreal_client_contract::EraView::era_of_dats(self.hud.era.world_dats)
        });
        self.hud.era.announced_features = self.cfg.era_features;
        self.hud.era.wire_1999 = self.cfg.wire_1999;
        self.hud.era.world_rules = self.cfg.world_rules.clone();
        self.objects.world.world_rules = self.cfg.world_rules.clone();
        self.host_state.world_rules = self.cfg.world_rules.clone();
    }
}
