//! The executable's front end: the Horizon interface, the cursor, the clipboard and the overlay,
//! plugged into [`pyreal_client_runtime::app::App`]'s frame.
//!
//! The frame loop is the runtime's. What is here is [`App`], the application with this executable's
//! front end ([`ClientShell`], in [`crate::front_end`]) in it: the runtime's application beside
//! its shell, so each frame hands the one to the other. `App` dereferences to the runtime's
//! application, so every read and every step that needs no UI is the runtime's own method.
//!
//! The main loop is tiny: connect once, then run one frame at a time until a frame asks to stop.
//! [`App::run`] is that loop and [`App::frame`] is one frame of it.

use pyreal_client_runtime::app::*;

use pyreal_primitives::{AssetSource, DataId};

pub use crate::front_end::ClientShell;
use crate::front_end::{flycam_key, route_host_event};
use crate::platform::host::Host;
use {crate::present::ClientPresentation, pyreal_client_runtime::config::Config};

/// The runtime's application with this executable's front end in it.
pub type CoreApp<H> = pyreal_client_runtime::app::App<ClientShell<H>>;

/// The application: the runtime's, beside the front end its frames hand the UI steps to, on
/// host `H`.
pub struct App<H: Host> {
    core: CoreApp<H>,
    shell: ClientShell<H>,
}

impl<H: Host> std::ops::Deref for App<H> {
    type Target = CoreApp<H>;
    fn deref(&self) -> &CoreApp<H> {
        &self.core
    }
}

impl<H: Host> std::ops::DerefMut for App<H> {
    fn deref_mut(&mut self) -> &mut CoreApp<H> {
        &mut self.core
    }
}

impl<H: Host> std::fmt::Debug for App<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.core, f)
    }
}

impl<H: Host> App<H> {
    /// Run startup steps 8 through 12 in order, with absent subsystems named, on the device
    /// presentation.
    ///
    /// Preferences were loaded by [`Config::from_args_and_prefs_at`]; database
    /// initialization and UI initialization run in the documented order,
    /// because the database must precede the UI (the UI layouts are dat objects).
    ///
    /// # Errors
    /// [`StartupError`] for any of the failures the client treats as fatal.
    #[cfg(gpu)]
    pub fn new(cfg: Config) -> Result<Self, StartupError> {
        Self::bring_up(cfg, None, None)
    }

    /// The same bring-up with the presentation supplied.
    ///
    /// This is the ungated constructor: it is what a headless `App` is built with
    /// ([`pyreal_client_runtime::present::NullPresentation`]), and it is what a second backend is handed to.
    /// [`App::new`] is this function with the device presentation built for it.
    ///
    /// # Errors
    /// [`StartupError`] for any of the failures the client treats as fatal.
    pub fn with_presentation(
        cfg: Config,
        present: Box<dyn ClientPresentation>,
    ) -> Result<Self, StartupError> {
        Self::bring_up(cfg, Some(present), None)
    }

    /// The headless bring-up in full: a presentation, a window and a clock, none of which needs a
    /// device or a window system.
    ///
    /// `NullPresentation` + `NullWindow` + `FixedStepClock` is *the* headless configuration, and
    /// this is the one constructor that says so in one place.
    ///
    /// # Errors
    /// [`StartupError`] for any of the failures the client treats as fatal.
    pub fn with_platform(
        cfg: Config,
        present: Box<dyn ClientPresentation>,
        platform: Platform,
    ) -> Result<Self, StartupError> {
        Self::bring_up(cfg, Some(present), Some(platform))
    }

    /// The runtime's twelve startup steps, with this executable's platform and presentation
    /// supplied at the steps that build them, and the front end beside the result.
    fn bring_up(
        cfg: Config,
        present: Option<Box<dyn ClientPresentation>>,
        platform: Option<Platform>,
    ) -> Result<Self, StartupError> {
        // The host's answers the runtime cannot compute itself: the local zone, the URL launch,
        // the caret blink and the default audio endpoint.
        crate::hud::install_platform::<H>();
        H::install_default_output();
        // The window's events, shared by the window that queues them and the front end that
        // routes them. A platform supplied from outside queues none.
        let window_events = crate::platform::window::WindowEvents::default();
        let opened_events = std::rc::Rc::clone(&window_events);
        Self::bring_up_with_store(
            cfg,
            None,
            window_events,
            |cfg| match platform {
                Some(p) => Ok(p),
                // A headless session with a server keeps real time (unless `--simulated-time`),
                // and any headless run does with `--real-time`.
                None if cfg.headless && cfg.keeps_real_time() => {
                    Ok(Platform::headless_live(cfg.width, cfg.height))
                }
                None if cfg.headless => Ok(Platform::headless(cfg.width, cfg.height)),
                None => H::open_platform(cfg, opened_events),
            },
            |window, client_w, client_h, cfg| match present {
                Some(p) => Ok(p),
                #[cfg(gpu)]
                None => Self::device_presentation(window, client_w, client_h, cfg),
                #[cfg(not(gpu))]
                None => {
                    let _ = (window, cfg);
                    Ok(Box::new(
                        pyreal_client_runtime::present::NullPresentation::new(client_w, client_h),
                    ))
                }
            },
        )
    }

    /// Bring up this front end over a host-opened data store and host-provided factories.
    /// The host installs its clock-zone, URL, caret and audio services before this call.
    ///
    /// # Errors
    /// A startup failure from the runtime, platform or presentation factory.
    pub fn bring_up_with_store(
        cfg: Config,
        store: Option<std::sync::Arc<pyreal_dat::RetailDatStore>>,
        window_events: crate::platform::window::WindowEvents,
        platform: impl FnOnce(&Config) -> Result<Platform, StartupError>,
        present: impl FnOnce(
            &dyn pyreal_client_runtime::platform::window::WindowHost,
            u32,
            u32,
            &Config,
        ) -> Result<Box<dyn ClientPresentation>, StartupError>,
    ) -> Result<Self, StartupError> {
        let mut core = CoreApp::<H>::bring_up_with_store(cfg, store, platform, present)?;
        core.interaction.client_build_id = H::BUILD_ID;
        core.build_commit = H::BUILD_COMMIT;
        core.build_hash = H::BUILD_HASH;
        let shell = ClientShell::with_window_events(core.window.raw_handle(), window_events);
        Ok(Self { core, shell })
    }

    /// Run the main loop: frame after frame until one asks to stop. Returns the number of frames drawn.
    pub fn run(&mut self) -> u64 {
        self.core.run(&mut self.shell)
    }

    /// The main loop as [`Self::run`] runs it, calling `each` after every frame.
    pub fn run_each(&mut self, mut each: impl FnMut(&mut Self)) -> u64 {
        self.core.state = pyreal_client_runtime::app::AppState::Running;
        while self.core.frame(&mut self.shell) {
            each(self);
        }
        self.core.finish_run()
    }

    /// The main loop as [`Self::run`] runs it, calling `at` once, after the frame in which the
    /// character has been in the world `secs` seconds: a scripted run's moment to look at the
    /// world before `--linger` logs it off.
    pub fn run_with_moment(&mut self, secs: f64, mut at: impl FnMut(&mut Self)) -> u64 {
        self.core.state = pyreal_client_runtime::app::AppState::Running;
        let mut done = false;
        while self.core.frame(&mut self.shell) {
            let in_world = self
                .core
                .playable_at
                .map(|t| self.core.timer.local_time - t);
            if !done && in_world.is_some_and(|s| s >= secs) {
                done = true;
                at(self);
            }
        }
        self.core.state = pyreal_client_runtime::app::AppState::ShuttingDown;
        self.core.frames_drawn()
    }

    /// One iteration of the frame, in the documented order. Returns false when the loop should
    /// end.
    pub fn frame(&mut self) -> bool {
        self.core.frame(&mut self.shell)
    }

    /// Advance a hosted session while another view may own the display.
    pub fn frame_with_drawing(&mut self, draw: bool) -> bool {
        self.core.frame_with_drawing(&mut self.shell, draw)
    }

    /// Start input, sound, and the UI shell in the client's order.
    ///
    /// # Errors
    /// [`StartupError::Device`] when the UI's own two dat objects are missing.
    pub fn start_shell(&mut self) -> Result<(), StartupError> {
        self.core.start_shell(&mut self.shell)
    }

    /// Start a session on a world in process. See [`CoreApp::begin_session`].
    ///
    /// # Errors
    /// As [`CoreApp::begin_session`].
    pub fn begin_session(&mut self, start: &SessionStart) -> Result<(), StartupError> {
        self.core.begin_session(&mut self.shell, start)
    }

    /// [`Self::begin_session`] over an endpoint the caller carries.
    ///
    /// # Errors
    /// As [`CoreApp::begin_session_on`].
    pub fn begin_session_on(
        &mut self,
        start: &SessionStart,
        net: pyreal_client_runtime::net::ClientNetwork,
    ) -> Result<(), StartupError> {
        self.core.begin_session_on(&mut self.shell, start, net)
    }

    /// End the session and go back to the screens before any world. See
    /// [`CoreApp::end_session`].
    pub fn end_session(&mut self) {
        self.core.end_session(&mut self.shell);
    }

    /// Fly through a world's files with no server. See [`CoreApp::begin_explore`].
    ///
    /// # Errors
    /// As [`CoreApp::begin_explore`].
    pub fn begin_explore(
        &mut self,
        start: &pyreal_client_runtime::explore::ExploreStart,
    ) -> Result<(), StartupError> {
        self.core.begin_explore(&mut self.shell, start)
    }

    /// End explore mode and go back to the screens before any world.
    pub fn end_explore(&mut self) {
        self.core.end_explore(&mut self.shell);
    }

    /// Whether explore mode is up.
    #[must_use]
    pub fn exploring(&self) -> bool {
        self.core.exploring()
    }

    /// Read newly chosen data files before any session. See
    /// [`pyreal_client_runtime::app::App::use_data_files`].
    ///
    /// # Errors
    /// The files will not open.
    pub fn use_data_files(
        &mut self,
        dat_dir: &std::path::Path,
        classic: Option<&std::path::Path>,
    ) -> Result<(), pyreal_client_runtime::app::StartupError> {
        self.core.use_data_files(dat_dir, classic)
    }

    /// Run the normal cleanup path, in [`pyreal_client_runtime::shutdown::Step::ORDER`].
    pub fn shutdown(self) -> pyreal_client_runtime::shutdown::CleanupLog {
        let Self { core, mut shell } = self;
        core.shutdown(&mut shell)
    }

    /// Release this session while returning its shared presentation to the host.
    pub fn shutdown_retaining_presentation(
        self,
    ) -> (
        Box<dyn ClientPresentation>,
        pyreal_client_runtime::shutdown::CleanupLog,
    ) {
        let Self { core, mut shell } = self;
        core.shutdown_retaining_presentation(&mut shell)
    }

    /// The HUD's event application against this application's own object tables, as the frame
    /// makes it.
    pub fn apply_hud_events(
        &mut self,
        events: &[pyreal_client_net::client_session::SessionEvent],
    ) -> Vec<pyreal_client_contract::chat::interface::ChatMessage> {
        self.core.apply_hud_events(&mut self.shell, events)
    }

    /// Act on what the session decoded, as the frame does.
    pub fn process_logon_event_queue(
        &mut self,
        events: Vec<pyreal_client_net::client_session::SessionEvent>,
    ) {
        self.core.process_logon_event_queue(&mut self.shell, events);
    }

    /// The interaction boundary as the frame runs it, with the panels callback.
    #[cfg(any(test, feature = "test-support"))]
    pub fn apply_interaction_events_to_panels(
        &mut self,
        events: &[pyreal_client_net::client_session::SessionEvent],
    ) {
        self.core
            .probe_mut()
            .apply_interaction_events_to_panels(&mut self.shell, events);
    }

    /// Force display resolution with `(force, width, height)`.
    pub fn force_display_resolution(&mut self, force: bool, width: u32, height: u32) {
        self.core
            .force_display_resolution(&mut self.shell, force, width, height);
    }

    /// Load the static scene: one landblock's terrain and scenery, plus the LOD window around it.
    ///
    /// # Errors
    /// [`StartupError::Device`] when the region, the landblock or a device resource is unavailable.
    pub fn load_static_scene(
        &mut self,
        cfg: pyreal_client_runtime::scene::SceneConfig,
    ) -> Result<(), StartupError> {
        self.core.load_static_scene(cfg)
    }

    /// One host event, as the event loop consumes it: a lifecycle event reaches the runtime's
    /// window procedure, a device event this client's input.
    pub fn handle_window_event(&mut self, event: &pyreal_input::host::HostEvent, time_ms: u32) {
        route_host_event(&mut self.core.ui_context(), &mut self.shell, event, time_ms);
    }

    /// Queue a host event as the window would; the next frame routes it to whichever interface
    /// is shown. For in-process drivers and tests.
    pub fn queue_window_event(&mut self, event: pyreal_input::host::HostEvent) {
        self.shell.queue_window_event(event);
    }

    /// Hold the pad at `pad` in place of the host's, from the next frame, for in-process drivers;
    /// `None` goes back to the host's pad.
    pub fn set_scripted_pad(&mut self, pad: Option<crate::gamepad::PadState>) {
        self.shell.shared.scripted_pad = pad;
    }

    /// The device input, for the tests and the console's state line.
    pub fn input_manager_mut(&mut self) -> Option<&mut crate::input::InputShell> {
        self.shell.shared.input.as_mut()
    }

    /// The device input, read-only.
    #[must_use]
    pub fn input_manager(&self) -> Option<&crate::input::InputShell> {
        self.shell.shared.input.as_ref()
    }

    /// One key transition of the residual flycam: `Space` raises it and `C` lowers it, unless a
    /// keyboard barrier stands (a focused text box stops these two keys for the same reason it
    /// stops every other one). Every other key is not the flycam's.
    pub fn flycam_key(&mut self, key: pyreal_input::keys::Key, down: bool) {
        flycam_key(&mut self.core.ui_context(), &self.shell, key, down);
    }

    /// The runtime's own focus-loss latches, for a lifecycle event; a device event has none.
    pub fn note_flycam_input(&mut self, event: &pyreal_input::host::HostEvent) {
        if let Some(lifecycle) = crate::platform::window::lifecycle(event) {
            self.core.note_flycam_input(&lifecycle);
        }
    }

    /// For the gated [`App::new`] path: construct the graphics engine and apply the
    /// presentation preferences it needs before anything draws.
    #[cfg(gpu)]
    fn device_presentation(
        window: &dyn pyreal_client_runtime::platform::window::WindowHost,
        client_w: u32,
        client_h: u32,
        cfg: &Config,
    ) -> Result<Box<dyn ClientPresentation>, StartupError> {
        let handles = window.render_handles();
        // The window kept alive, the splash on it, while the device's pipelines are made.
        let wait = || window.keep_responding();
        let mut renderer = Self::make_device(handles, client_w, client_h, H::SPLASH_PNG, &wait)?;
        // `Render.ScreenBrightness`: the exposure's compensation.
        renderer.set_brightness(cfg.render.screen_brightness);
        // `false`: the client starts windowed whatever the preference says, and
        // `App::change_presentation` sets this again on the gameplay edge.
        renderer.set_presentation_sync(false, cfg.display.sync_to_refresh);
        Ok(Box::new(renderer))
    }

    /// Bring the device and the engine up.
    #[cfg(gpu)]
    fn make_device(
        handles: Option<pyreal_client_contract::window::WindowHandles>,
        client_w: u32,
        client_h: u32,
        splash: Option<&[u8]>,
        wait: &dyn Fn() -> (u32, u32),
    ) -> Result<crate::gpu::Renderer, StartupError> {
        let r = crate::gpu::Renderer::new_waiting(handles, client_w, client_h, splash, wait)
            .map_err(|e| StartupError::Device {
                cause: format!("graphics engine: {e}"),
            })?;
        tracing::info!("graphics engine on {}", r.adapter_name());
        Ok(r)
    }

    /// Decode one retail surface and stand it up as the scene.
    ///
    /// # Errors
    /// [`StartupError::Device`] when the object is missing or will not decode.
    pub fn load_first_pixel_scene(&mut self) -> Result<(), StartupError> {
        let id = DataId(pyreal_client_runtime::assets::FIRST_PIXEL_SURFACE);
        let assets: &dyn AssetSource = &*self.core.store;
        self.core
            .present
            .load_first_pixel_scene(assets, id)
            .map_err(|e| StartupError::Device {
                cause: format!("{e}"),
            })
    }

    /// The front end, for the tests.
    #[must_use]
    pub fn shell(&self) -> &ClientShell<H> {
        &self.shell
    }

    /// The front end, mutably, for the tests.
    pub fn shell_mut(&mut self) -> &mut ClientShell<H> {
        &mut self.shell
    }

    /// The controls the interface's widget kit noted last frame, each with its place on screen and
    /// its panel, for the tests: none before the interface is up.
    #[must_use]
    pub fn kit_controls(&self) -> Option<&pyreal_ui_kit::nav::Snapshot> {
        self.shell.horizon().map(|ui| &ui.frame_input.nav.last)
    }

    /// What the cursor path did this session — [`crate::cursor::CursorStats`].
    ///
    /// `updates` is the per-frame wire: it must equal the number of frames drawn.
    #[must_use]
    pub fn cursor_stats(&self) -> crate::cursor::CursorStats {
        self.shell.shared.cursor.stats
    }

    /// The game's cursor last chosen, with its hotspot, for a host that draws the pointer itself.
    #[must_use]
    pub fn last_cursor(&self) -> Option<(pyreal_primitives::DataId, i32, i32)> {
        self.shell.shared.cursor.last_cursor()
    }

    /// The cursor asset currently applied, exposed for tests.
    #[must_use]
    pub fn current_cursor_did(&self) -> Option<pyreal_primitives::DataId> {
        self.shell.shared.cursor.current()
    }

    /// Access to the renderer, for the capture the acceptance gate takes.
    ///
    /// `App` holds a [`pyreal_client_runtime::present::Presentation`], so this is a downcast of it and is gated on the device feature exactly as the renderer itself is. It panics if the
    /// presentation is not the device one, which is the same contract the field access had: a
    /// caller of this function has already decided it is driving a real device.
    #[cfg(gpu)]
    pub fn renderer_mut(&mut self) -> &mut crate::gpu::Renderer {
        self.core
            .present
            .as_any_mut()
            .downcast_mut::<crate::gpu::Renderer>()
            .expect("App::renderer_mut on a presentation that is not the device")
    }

    /// Read-only access to the renderer, for the counters the report prints.
    ///
    /// See [`App::renderer_mut`] for why this is a downcast and what it asserts.
    #[cfg(gpu)]
    #[must_use]
    pub fn renderer(&self) -> &crate::gpu::Renderer {
        self.core
            .present
            .as_any()
            .downcast_ref::<crate::gpu::Renderer>()
            .expect("App::renderer on a presentation that is not the device")
    }
}

#[cfg(gpu)]
impl<H: Host> App<H> {
    /// Report the world immediately after its load completes.
    pub fn log_load_report(&self, scene: pyreal_client_runtime::scene::SceneConfig) {
        let r = self.renderer();
        tracing::info!(target: "pyreal_client",
            "landblock 0x{:04X}, the engine on {}",
            scene.landblock,
            r.adapter_name()
        );
        if let Some(world) = self.core.world.as_ref() {
            if let Some(c) = world.character.as_ref() {
                tracing::debug!(target: "pyreal_client",
                    "{} interior cell(s) resident for physics ({:?}); character at {:?} in cell {:#010X}",
                    c.land().resident_cells(),
                    c.land().cell_stats(),
                    c.position().frame.origin,
                    c.position().cell.0
                );
            }
        }
    }

    /// Report frame, UI, model, input and audio counters before capture and shutdown.
    pub fn log_run_report(&mut self, frames: u64, cfg_connect: bool) {
        tracing::info!(target: "pyreal_client", "{frames} frame(s) drawn");
        if let Some((n, period, gpu, gpu95)) = self.renderer().times.summary(10) {
            tracing::info!(target: "pyreal_client",
                "frame times over the last {n} frame(s): period {period:.2} ms median, GPU {gpu:.2} ms median ({gpu95:.2} ms p95), drawn at {:?} for {:?}",
                self.renderer().engine.renderer.internal,
                self.renderer().engine.renderer.output_size
            );
        }

        if cfg_connect {
            // What the server actually put in the world, and what was drawn.
            let s = self.objects().stats;
            tracing::info!(target: "pyreal_client",
                "objects -- {} created, {} merged, {} recreated, {} removed, \
                 {} stale instances; {} position updates ({} stale), {} movement buffers \
                 ({} undecodable)",
                s.creates,
                s.merges,
                s.recreates,
                s.removes,
                s.stale_instances,
                s.position_updates,
                s.stale_positions,
                s.movement_updates,
                s.movement_undecodable
            );
            if let Some(b) = self.renderer().bridge.as_ref() {
                tracing::info!(target: "pyreal_client",
                    "{} object(s) drawn in {} instance(s), {} as the art pack's models",
                    b.counts.objects,
                    b.counts.instances,
                    b.counts.packed
                );
            }
        }

        // The HUD's report line: what it read out of the server and what it did with it.
        {
            let h = self.hud();
            let s = h.stats;
            tracing::info!(target: "pyreal_client",
                "HUD -- {} player description(s), {} placement row(s) decoded, \
                 {} window visibility/ies applied, {} vital update(s) ({} unstorable), \
                 {} chat line(s) shown ({} filtered out), {} undecodable; \
                 {} vitals / {} toolbar / {} radar write(s), coords {:?}",
                s.player_desc_applied,
                s.placements_decoded,
                s.placements_applied,
                s.vital_updates,
                s.vital_updates_unstorable,
                s.chat_lines,
                s.chat_lines_dropped,
                s.undecodable,
                s.vitals_written,
                s.toolbar_written,
                s.radar_written,
                h.coords
            );
            for (id, row) in &h.placements.rows {
                tracing::debug!(target: "pyreal_client", "HUD placement window {id}: {row:?}");
            }
            // The housing subsystem's whole round trip in one line. If the client never sends
            // `0x021E House_QueryHouse`, the shard never sends either answer, and every counter below
            // stays 0 exactly like a shard with nothing to say. A live run that ends with
            // `0 status / 0 data` means the *request* did not go out, which is a different bug from an
            // unreceived answer.
            tracing::info!(target: "pyreal_client",
                "house -- {} status / {} data answer(s) to the login QueryHouse, \
                 {} rent-time ({} applied) / {} rent-payment ({} applied) update(s), \
                 {} restriction update(s) ({} applied)",
                s.house_status_notices,
                s.house_data_notices,
                s.house_rent_time_updates,
                s.house_rent_time_applied,
                s.house_rent_payment_updates,
                s.house_rent_payment_applied,
                s.house_restriction_updates,
                s.house_restrictions_applied
            );
            // The slumlord window's own round trip, beside the login one, because the
            // two are about different houses and fail in different ways. A `0 profile(s)` after using
            // a slumlord means either the use never went out or the shard refused it; a non-zero
            // profile count with `0 opened` would mean the receiver ran and the panel did not.
            tracing::info!(target: "pyreal_client",
                "slumlord -- {} house profile(s) received, \
                 {} payment request(s), {} lord re-quer(ies)",
                s.house_profile_notices,
                self.interaction().stats.house_payments_sent,
                self.interaction().stats.house_lord_queries
            );
        }
        {
            // The world's overlay over the locked data files: where it is, and how many reads its
            // own records and its deletions answered since the files were last opened.
            let s = &self.core.store;
            let files = [
                ("portal", Some(s.portal())),
                ("cell", Some(s.cell())),
                ("local", Some(s.local())),
                ("highres", s.highres()),
            ];
            let read: Vec<String> = files
                .iter()
                .filter_map(|(name, f)| {
                    let l = (*f)?.layer()?;
                    let (served, hidden) = l.reads();
                    Some(format!(
                        "{name} {} record(s), {} deletion(s), {served} read(s) answered, {hidden} hidden",
                        l.records().count(),
                        l.tombstones().len()
                    ))
                })
                .collect();
            tracing::info!(target: "pyreal_client",
                "overlay -- {}{}",
                s.overlay_dir()
                    .map_or_else(|| "none".to_owned(), |d| d.path().display().to_string()),
                if read.is_empty() {
                    String::new()
                } else {
                    format!(": {}", read.join("; "))
                }
            );
        }
        {
            // The era the client plays and the systems it takes the world to lack (the server's
            // announcement over the era's table), and what the screens last took away for them.
            let h = self.hud();
            let lacks = |f: pyreal_primitives::EraFeatures| {
                f.iter()
                    .filter(|(_, on)| !on)
                    .map(|(name, _)| name)
                    .collect::<Vec<_>>()
                    .join(",")
            };
            tracing::info!(target: "pyreal_client",
                "era -- {} ({}), {} system(s) announced; the world lacks [{}]",
                h.era.era,
                if h.era.era_announced {
                    "announced"
                } else {
                    "from the data files"
                },
                h.era.announced_features.iter().count(),
                lacks(h.era.features()),
            );
        }
        {
            let (offered, handled, no_scan, actions) =
                self.input_manager_mut().map_or((0, 0, 0, 0), |i| {
                    (
                        i.stats.messages_offered,
                        i.stats.messages_handled,
                        i.stats.keyboard_without_scan_code,
                        i.stats.actions_fired,
                    )
                });
            tracing::info!(target: "pyreal_client",
                "input -- {offered} message(s) offered, {handled} handled, \
                 {no_scan} without a scan code, {actions} action(s)"
            );
        }
        if let Some(audio) = self.audio_mut() {
            tracing::info!(target: "pyreal_client",
                "audio -- device {}, {} wave(s) created, {} decode failure(s), \
                 {} sound(s) started, {} voice(s) still playing, {} under-run(s), \
                 {} block(s) submitted, peak {:.4}",
                if audio.has_device() { "up" } else { "silent" },
                audio.stats.waves_created,
                audio.stats.wave_decode_failures,
                audio.stats.sounds_started,
                audio.active_voices(),
                audio
                    .stats
                    .underruns
                    .load(std::sync::atomic::Ordering::Relaxed),
                audio
                    .stats
                    .blocks_filled
                    .load(std::sync::atomic::Ordering::Relaxed),
                audio.peak_output()
            );
            let (stage, (pool_started, pool_dropped)) = audio.engine_stats();
            tracing::info!(target: "pyreal_client",
                "sound engine -- {}, {} sound(s) started, {} dropped, {} gave way, {} at most at \
                 once, {} loop(s), {} out of earshot; the original's pool started {} and dropped {}",
                audio.style().word(),
                stage.started,
                stage.dropped,
                stage.stolen,
                stage.most,
                stage.loops,
                stage.unheard,
                pool_started,
                pool_dropped
            );
            let w = audio.world_stats();
            tracing::info!(target: "pyreal_client",
                "world audio -- {} hook sound(s), {} unplayable; {} server sound(s), {} unplayable",
                w.triggers,
                w.trigger_misses,
                w.server_sounds,
                w.server_sound_misses
            );
        }
    }
}

// Both tests draw on the device.
#[cfg(all(test, gpu))]
mod tests {
    use super::*;
    use crate::platform::host::NullHost;
    use pyreal_input::host::HostEvent;

    /// The host's physical resize reaches both the real backbuffer and the UI coordinate space, so
    /// the picture is the window's real pixels at any desktop scaling (retail runs DPI-unaware and
    /// lets Windows scale it). Client divergence CD-004.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn physical_window_resize_reaches_the_backbuffer_and_ui_without_changing_preferences() {
        let mut cfg = Config {
            headless: true,
            connect: false,
            sound: false,
            world: false,
            dat_dir: pyreal_dat::testing::dat_dir(),
            preferences_file: std::env::temp_dir().join("pyreal-uncreated-preferences/prefs.ini"),
            ..Config::default()
        };
        cfg.display.full_screen = false;
        let mut app = App::<NullHost>::new(cfg).expect("offscreen App with retail UI assets");
        app.start_shell().expect("real UI");
        let preference = app.config().display.resolution;
        let position = app.window_rect();
        for (w, h) in [(1920, 1080), (800, 600)] {
            app.handle_window_event(
                &HostEvent::Resized {
                    width: w,
                    height: h,
                },
                0,
            );
            assert_eq!(
                app.present.size(),
                (w, h),
                "backbuffer follows physical client size"
            );
            assert_eq!(
                app.config().display.resolution,
                preference,
                "OS events do not save a preference"
            );
            assert_eq!(
                app.window_rect(),
                position,
                "a resize notification does not recenter"
            );
        }
        app.handle_window_event(
            &HostEvent::Resized {
                width: 0,
                height: 0,
            },
            0,
        );
        assert_eq!(
            app.present.size(),
            (800, 600),
            "minimizing does not create a zero buffer"
        );
    }

    /// A second client starts while the first is still running: startup takes no
    /// one-client-per-machine claim, so the second is not refused and both run their interface.
    ///
    /// Client divergence CD-008.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_second_client_starts_while_the_first_is_running() {
        let cfg = || Config {
            headless: true,
            connect: false,
            sound: false,
            world: false,
            dat_dir: pyreal_dat::testing::dat_dir(),
            preferences_file: std::env::temp_dir().join("pyreal-two-clients-not-created/prefs.ini"),
            ..Config::default()
        };
        let mut first = App::<NullHost>::new(cfg()).expect("the first client starts");
        first.start_shell().expect("the first client's interface");
        let mut second =
            App::<NullHost>::new(cfg()).expect("a second client starts beside the first");
        second
            .start_shell()
            .expect("the second client's interface, with the first still running");
        assert!(first.shell().horizon().is_some() && second.shell().horizon().is_some());
        drop(second);
        assert!(
            first.shell().horizon().is_some(),
            "the first client runs on"
        );
    }
}
