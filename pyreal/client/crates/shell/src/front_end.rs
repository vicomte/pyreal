//! The executable's front end: the Horizon interface, the cursor, the clipboard and the overlay,
//! as [`ClientShell`], the [`Shell`] the executable supplies.
//!
//! It never holds the application. Each step of the frame it takes part in hands it a
//! [`UiContext`](pyreal_client_runtime::ui_context::UiContext), and that is all of the game it
//! reaches. The application it plugs into is assembled in [`crate::app`].

use pyreal_client_runtime::app::StartupError;
use pyreal_client_runtime::shell::{Shell, UiNotices};

use crate::platform::host::Host;
use crate::present::ClientPresentation;

mod horizon;
mod horizon_keys;
mod plugins;
mod scripted_run;

/// The executable's front end: the Horizon interface, once it is up, and the host services it
/// draws on.
pub struct ClientShell<H: Host> {
    pub(crate) host_overlay: Vec<pyreal_client_contract::overlay::OverlayItem>,
    pub(crate) host_hides_ui: bool,
    pub(crate) shared: FrontEndServices<H>,
    pub(crate) horizon: horizon::HorizonFace,
    /// The plugins' share of the interface.
    pub(crate) plugins: plugins::PluginSurface,
    /// Whether the interface was asked for at start-up: it comes up once its art is in.
    ui_started: bool,
}

/// The host services the interface draws on.
pub(crate) struct FrontEndServices<H: Host> {
    /// The game's cursors over the world. See [`crate::cursor`].
    pub(crate) cursor: crate::cursor::CursorSystem,
    /// The host's clipboard.
    host_clipboard: H::Clipboard,
    /// The host's pad, read once a frame; none without a window.
    gamepad: Box<dyn crate::gamepad::HostGamepad>,
    /// The pointer, held by the host during a camera drag.
    pub(crate) pointer: crate::pointer::PointerHold,
    /// A pad state set by an in-process driver, read in place of the host's pad while it is set.
    pub(crate) scripted_pad: Option<crate::gamepad::PadState>,
    /// The device input: the input manager and the registrations the client's systems make.
    pub(crate) input: Option<crate::input::InputShell>,
    /// The device half of the message mapping: the Alt state and the pointer position the button
    /// messages carry.
    devices: pyreal_input::pump::DeviceMessages,
    /// The window's queue of events, routed at the event-loop step.
    window_events: crate::platform::window::WindowEvents,
    /// The reopen of the data files the interface last read them at
    /// ([`pyreal_client_runtime::ui_context::UiContext::store_generation`]).
    files_read: u64,
}

impl<H: Host> std::fmt::Debug for ClientShell<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientShell")
            .field("ui", &self.horizon.ui.is_some())
            .finish_non_exhaustive()
    }
}

impl<H: Host> ClientShell<H> {
    /// A front end whose cursor is installed on `hwnd` by host `H`, with no interface up yet.
    #[must_use]
    pub fn new(hwnd: Option<isize>) -> Self {
        Self::with_window_events(hwnd, crate::platform::window::WindowEvents::default())
    }

    /// Queue a host event as the window would, to be routed with the next frame's: the way an
    /// in-process driver gives the client real input.
    pub fn queue_window_event(&self, event: pyreal_input::host::HostEvent) {
        self.shared.window_events.borrow_mut().push(event);
    }

    /// A front end that routes the events the window queues on `window_events`.
    #[must_use]
    pub fn with_window_events(
        hwnd: Option<isize>,
        window_events: crate::platform::window::WindowEvents,
    ) -> Self {
        Self {
            shared: FrontEndServices {
                input: None,
                devices: pyreal_input::pump::DeviceMessages::default(),
                window_events,
                cursor: crate::cursor::CursorSystem::with_images(H::cursor_images(hwnd)),
                host_clipboard: H::clipboard(),
                // A run with no window has no one at it to hold a pad, and opens none.
                gamepad: if hwnd.is_some() {
                    H::gamepad()
                } else {
                    Box::new(crate::gamepad::NoGamepad)
                },
                pointer: crate::pointer::PointerHold::new(H::pointer(hwnd)),
                scripted_pad: None,
                files_read: 0,
            },
            host_overlay: Vec::new(),
            host_hides_ui: false,
            horizon: horizon::HorizonFace::default(),
            plugins: plugins::PluginSurface::default(),
            ui_started: false,
        }
    }

    /// The Horizon interface, once it is up.
    #[must_use]
    pub fn horizon(&self) -> Option<&pyreal_horizon::runtime::HorizonFrontEnd> {
        self.horizon.ui.as_ref()
    }

    /// The Horizon interface, once it is up, mutably.
    pub fn horizon_mut(&mut self) -> Option<&mut pyreal_horizon::runtime::HorizonFrontEnd> {
        self.horizon.ui.as_mut()
    }
}

/// What this executable's front end is handed at each step of the frame.
type Cx<'a, H> = pyreal_client_runtime::ui_context::UiContext<'a, ClientShell<H>>;

/// The preferences, as the interface comes up: the store registered and loaded from the file,
/// and the renderer's live sampler over the file's.
fn start_preferences<H: Host>(cx: &mut Cx<'_, H>) {
    pyreal_client_contract::options::store::init();
    cx.start_preferences();
}

/// **The journal-path builder's three inputs**: the preferences file's directory, the world's
/// name from `0xF7E1 Login_WorldInfo` and the character entered. `None` while any of the three is
/// missing, which writes no journal at all rather than one at a guessed path.
fn journal_identity<H: Host>(cx: &Cx<'_, H>) -> Option<pyreal_client_contract::JournalIdentity> {
    let dir = cx.config().preferences_file.parent()?;
    let world = cx
        .pregame()
        .world_name
        .as_deref()
        .filter(|s| !s.is_empty())?;
    let character = cx
        .pregame()
        .entered_character
        .as_deref()
        .filter(|s| !s.is_empty())?;
    Some(pyreal_client_contract::JournalIdentity {
        directory: dir.to_path_buf(),
        world: world.to_owned(),
        character: character.to_owned(),
    })
}

/// The journal's file reads and writes the session owes.
fn service_journal<H: Host>(cx: &mut Cx<'_, H>) {
    use pyreal_client_contract::journal::JournalIo;
    use pyreal_client_runtime::platform::files;
    use pyreal_presentation::journal::{
        parse_pages, save_pages_text, LOAD_COMPLAINT, LOAD_COMPLAINT_CHANNEL,
    };
    let identity = journal_identity(cx);
    cx.hud_mut().journal_identity = identity.clone();
    cx.prepare_journal(identity);
    for io in cx.take_journal_io() {
        match io {
            JournalIo::Load {
                identity,
                generation,
                revision,
            } => {
                let read = files::read_to_string(&identity.client_path());
                cx.record_journal_io(&identity, generation, true, read.is_ok());
                let pages = match read {
                    Ok(text) => parse_pages(&text),
                    Err(_) => Ok(Vec::new()),
                };
                if cx.complete_journal_load(identity, generation, revision, pages) {
                    cx.add_scroll_line(LOAD_COMPLAINT, LOAD_COMPLAINT_CHANNEL);
                }
            }
            JournalIo::Save {
                identity,
                generation,
                pages,
            } => {
                let path = identity.client_path();
                let saved = files::make_dirs(&identity.directory)
                    .and_then(|()| files::write(&path, save_pages_text(&pages)));
                cx.record_journal_io(&identity, generation, false, saved.is_ok());
                if let Err(error) = saved {
                    tracing::warn!(%error, "the journal could not be saved");
                }
            }
        }
    }
}

impl<H: Host> ClientShell<H> {
    /// After the data files are reopened (a data patch), the interface reads them again.
    fn reread_files_when_patched(&mut self, cx: &mut Cx<'_, H>) {
        let now = cx.store_generation();
        if now == self.shared.files_read {
            return;
        }
        self.shared.files_read = now;
        if let Some(ui) = self.horizon.ui.as_mut() {
            ui.reread_files(cx);
            ui.set_chargen_tables(pyreal_ui_logic::chargen::load(cx.store()));
        }
    }

    /// Bring the interface up, once the UI has started.
    fn bring_up(&mut self, cx: &mut Cx<'_, H>) {
        if self.horizon.ui.is_some() || !self.ui_started {
            return;
        }
        let mut ui = horizon::build_horizon(cx);
        ui.set_chargen_tables(pyreal_ui_logic::chargen::load(cx.store()));
        self.horizon.ui = Some(ui);
        self.shared.files_read = cx.store_generation();
        self.enter_horizon(cx);
    }

    /// The interface's own keys and camera, now that it is up.
    fn enter_horizon(&mut self, cx: &mut Cx<'_, H>) {
        if let Some(input) = self.shared.input.as_mut() {
            input.activate_horizon(true);
        }
        if let Some(ui) = self.horizon.ui.as_ref() {
            cx.set_orbit_camera(Some(ui.ui.options.orbit));
        }
        self.horizon.keys.invalidate();
        if let Some(ui) = self.horizon.ui.as_mut() {
            ui.reread_option_rows(cx);
        }
        tracing::info!("the Horizon interface is shown");
    }

    /// The interface's share of the input manager's actions this frame: its own keys and the mouse
    /// actions over the world; the rest go on to the game. Then where the objects whose names it
    /// shows are on screen, for its frame.
    fn horizon_input(&mut self, cx: &mut Cx<'_, H>, now: pyreal_primitives::LocalTime) {
        let Some(ui) = self.horizon.ui.as_mut() else {
            return;
        };
        if let Some(code) = cx.hud_mut().horizon.abuse.take() {
            ui.abuse_response(code);
        }
        if let Some(input) = self.shared.input.as_mut() {
            // The key bindings page: what it asked last frame, the key it waits for, and the
            // bindings as they are now.
            self.horizon
                .keys
                .serve(input, std::mem::take(&mut ui.key_requests));
            for text in self.horizon.keys.lines.drain(..) {
                ui.outbox
                    .emit(pyreal_client_contract::UiRequest::DisplayChatText {
                        feedback: pyreal_client_contract::feedback::Feedback::LOCAL,
                        channel: pyreal_ui_logic::panels::MOUSE_TURNING_CHANNEL,
                        text: text.to_owned(),
                    });
            }
            self.horizon.keys.set_era(cx.hud().era.features());
            if let Some(view) = self.horizon.keys.view(input, cx.store()) {
                ui.keys = Some(view);
            }
            // The input maps the interface's state calls for: the character session's while its
            // game screen is up, the pre-game screen's otherwise, the text box's while one has
            // the keyboard, and the target map while a use waits for its target.
            let game = ui.in_gameplay();
            input.set_character_session_input_maps(game);
            input.set_explore_input_maps(ui.exploring());
            input.set_mode_input_maps(if game { &[] } else { &[9] });
            input.set_focused_input_maps(if ui.ui.text_focus {
                &crate::input::FOCUSED_TEXT_MAP_REGISTRATIONS
            } else {
                &[]
            });
            input.set_target_input_map(
                cx.target_mode() != pyreal_client_runtime::interaction::TargetMode::None,
            );
            input.finish_session_retirement(|released| cx.accept_actions(released));
            if input.manager.text.text_mode != ui.ui.text_focus {
                input.set_text_mode(ui.ui.text_focus);
            }
            input.use_time(now);
            let events = input.take_events();
            let rest = ui.take_actions(cx, events, input.mouse_pos(), now);
            input.put_back_unconsumed(rest);
        }
        let wanted = std::mem::take(&mut ui.wanted_projections);
        let (present, world) = cx.present_with_world();
        ui.origins = wanted
            .iter()
            .filter_map(|&id| Some((id, present.target_origin(id, world)?)))
            .collect();
        ui.tops = wanted
            .iter()
            .filter_map(|&id| {
                let (x, y, _) = present.target_top(id, world)?;
                Some((id, (x, y)))
            })
            .collect();
        ui.projections = wanted
            .into_iter()
            .filter_map(|id| Some((id, present.target_projection(id, world)?)))
            .collect();
        ui.in_sight = present.drawn_objects();
    }

    /// The camera drag after one of the window's events: the pointer is held once the drag has
    /// moved it far enough, and shown again where the drag began when the camera stops turning
    /// with it, which the interface is told as the pointer moving there. Gamepad mode holds
    /// nothing.
    fn follow_pointer_drag(&mut self, cx: &mut Cx<'_, H>, time_ms: u32) {
        let gamepad_mode = self
            .horizon
            .ui
            .as_ref()
            .is_some_and(|ui| ui.ui.options.pad.enabled);
        self.shared.pointer.follow(cx.mouse_look(), !gamepad_mode);
        if let Some((x, y)) = self.shared.pointer.take_returned() {
            let back = pyreal_input::host::HostEvent::CursorMoved { x, y };
            route_host_event(cx, self, &back, time_ms);
        }
    }
}

impl<H: Host> Shell for ClientShell<H> {
    type Hud = crate::hud::Hud;
    type Present = dyn ClientPresentation;

    /// The keymap first: the user's `.keymap` is merged **first** so a rebound key wins over both
    /// shipped defaults, and the merged map is written back on exit. Not fatal: a client with no
    /// key bindings is useless but can still draw, and saying so is better than refusing to start.
    fn start_input(&mut self, cx: &mut Cx<'_, H>) {
        let preferences_file = cx.config().preferences_file.clone();
        let keymap_file = pyreal_client_runtime::platform::files::read_to_string(&preferences_file)
            .ok()
            .and_then(|text| {
                pyreal_client_contract::persist::preferences::UserPreferences::parse(&text).ok()
            })
            .and_then(|prefs| {
                prefs
                    .get(pyreal_client_contract::persist::preferences::keys::KEYMAP_FILE)
                    .map(str::to_owned)
            });
        let keymap = crate::input::keymap_path_for(&preferences_file, keymap_file.as_deref());
        match crate::input::InputShell::new(&**cx.store(), keymap.as_ref()) {
            Ok(i) => self.shared.input = Some(i),
            Err(e) => tracing::warn!("no input: {e}"),
        }
    }

    /// The window's queued events, in arrival order, then the pad.
    ///
    /// Each event passes the pointer's hold first ([`crate::pointer`]): while a camera drag holds
    /// the pointer, the mouse's own movement turns the camera alone and the pointer's reports are
    /// not routed, and when the drag ends the interface is told the pointer is back where the
    /// drag began. A drag is never a click: before a button's release after a drag is routed,
    /// the device input forgets the press, so the next is no second click of a double-click, and
    /// a right button's release is said to end a drag, so it examines nothing.
    fn window_input(&mut self, cx: &mut Cx<'_, H>, time_ms: u32) {
        let events: Vec<_> = self.shared.window_events.borrow_mut().drain(..).collect();
        let pad = self.shared.gamepad.poll();
        let pad = self.shared.scripted_pad.or(pad);
        // A drag that ended after the last frame's events.
        self.follow_pointer_drag(cx, time_ms);
        for event in &events {
            let gate = self.shared.pointer.gate(event);
            if let Some((button, dragged)) = self.shared.pointer.take_release() {
                if dragged {
                    if let Some(input) = self.shared.input.as_mut() {
                        input.manager.forget_mouse_press(button);
                    }
                }
                if button == pyreal_input::keys::MouseButton::Right {
                    cx.note_right_release(dragged);
                }
            }
            match gate {
                crate::pointer::Gate::Route => route_host_event(cx, self, event, time_ms),
                crate::pointer::Gate::Camera(x, y) => cx.cursor_moved(x, y),
                crate::pointer::Gate::Drop => {}
            }
            self.follow_pointer_drag(cx, time_ms);
        }
        // The pad, after the window's events, so in gamepad mode the pad's pointer is the one
        // the interface sees.
        let keys = self
            .horizon
            .ui
            .as_mut()
            .map(|ui| ui.pad_input(cx, pad))
            .unwrap_or_default();
        for event in &keys {
            route_host_event(cx, self, event, time_ms);
        }
    }

    fn input_use_time(&mut self, _cx: &mut Cx<'_, H>, now: pyreal_primitives::LocalTime) {
        if let Some(input) = self.shared.input.as_mut() {
            input.use_time(now);
        }
    }

    fn hand_on_actions(&mut self, actions: &mut pyreal_client_runtime::actions::ActionQueue) {
        if let Some(input) = self.shared.input.as_mut() {
            input.hand_on(actions);
        }
    }

    fn control_notice(&mut self, notice: pyreal_client_runtime::shell::ControlNotice) {
        if let Some(input) = self.shared.input.as_mut() {
            input.apply_notice(notice);
        }
    }

    /// The key maps, when they were loaded and their configured filename is non-empty: the full
    /// merged map is written, defaults included, and the next run merges it back first so it
    /// wins. `Nothing` is the client's own skip when that filename is empty.
    fn save_bindings(&mut self) -> pyreal_client_runtime::shutdown::Outcome {
        use pyreal_client_runtime::shutdown::Outcome;
        if let Some(input) = self.shared.input.as_mut() {
            if let Err(e) = input.save_horizon_keymap() {
                tracing::warn!("the Horizon key map was not saved: {e}");
            }
        }
        match self
            .shared
            .input
            .as_ref()
            .map(crate::input::InputShell::save_keymap)
        {
            Some(Ok(true)) => Outcome::Ran,
            Some(Ok(false)) | None => Outcome::Nothing,
            Some(Err(e)) => {
                tracing::warn!("the keymap was not saved: {e}");
                Outcome::Nothing
            }
        }
    }

    /// The preferences, then the interface itself, after input and sound. The interface comes up
    /// now when its art is in, else on the first frame it is.
    fn start_ui(&mut self, cx: &mut Cx<'_, H>) -> Result<(), StartupError> {
        start_preferences(cx);
        self.ui_started = true;
        // The interface comes up on a pre-game screen, where no character session exists yet, so
        // the session's maps go now: a key pressed before its first frame must not reach them.
        if let Some(input) = self.shared.input.as_mut() {
            input.set_character_session_input_maps(false);
        }
        self.bring_up(cx);
        Ok(())
    }

    fn has_ui(&self) -> bool {
        self.ui_started
    }

    /// Whether the interface is on its game screen, which means "the player is in the world".
    fn in_gameplay(&self) -> bool {
        self.horizon
            .ui
            .as_ref()
            .is_some_and(pyreal_horizon::runtime::HorizonFrontEnd::in_gameplay)
    }

    /// The interface draws its screens before the world at the player's own size.
    fn keeps_login_size(&self) -> bool {
        false
    }

    fn hides_world(&self) -> bool {
        false
    }

    /// A chat line raised for the game screen is the game screen's while it is up.
    fn chat_generation(&self) -> Option<u64> {
        self.in_gameplay().then_some(1)
    }

    fn clear_chat_history(&mut self) {
        if let Some(ui) = self.horizon.ui.as_mut() {
            ui.clear_chat();
        }
    }

    fn ui_requests(&mut self) -> Option<&mut pyreal_client_contract::requests::Outbox> {
        self.horizon.ui.as_mut().map(|ui| &mut ui.outbox)
    }

    /// The interface draws over the whole window: the scene owns the whole back buffer.
    fn game_viewport(&self) -> Option<pyreal_primitives::Viewport> {
        None
    }

    /// Is the pointer over the world, rather than over the interface? `true` with no interface.
    fn pointer_over_game_view(&self, _cursor: (i32, i32)) -> bool {
        self.horizon
            .ui
            .as_ref()
            .is_none_or(|ui| !ui.ui.pointer_over_ui)
    }

    fn examine_panel_open(&mut self) -> bool {
        self.horizon
            .ui
            .as_mut()
            .is_some_and(|ui| ui.examine_panel_open())
    }

    fn close_examine_panel(&mut self) {
        if let Some(ui) = self.horizon.ui.as_mut() {
            ui.close_examine_panel();
        }
    }

    fn service_dialogs(&mut self, cx: &mut Cx<'_, H>, now: pyreal_primitives::LocalTime) {
        if let Some(ui) = self.horizon.ui.as_mut() {
            ui.service_dialogs(cx, now);
        }
    }

    fn resolution_prompt(
        &mut self,
        _cx: &mut Cx<'_, H>,
        prompt: Option<pyreal_client_contract::resolution::ResolutionPrompt>,
    ) {
        if let Some(ui) = self.horizon.ui.as_mut() {
            ui.project_resolution(prompt);
        }
    }

    fn before_ui_input(&mut self, player_airborne: bool) {
        if let Some(ui) = self.horizon.ui.as_mut() {
            ui.before_ui_input(player_airborne);
        }
    }

    fn drive_world_script(&mut self, cx: &mut Cx<'_, H>, _now: pyreal_primitives::LocalTime) {
        self.scripted_world(cx);
    }

    /// The swirl goes under the whole interface, over the whole window.
    fn place_portal_space(&mut self, present: &mut Self::Present) {
        let own = present.draws_portal_space();
        if let Some(ui) = self.horizon.ui.as_mut() {
            ui.portal = !own;
        }
    }

    /// The interface carries `--enter-world` once it is up; until then the runtime does.
    fn drives_scripted_entry(&self) -> bool {
        self.horizon.ui.is_some()
    }

    fn ui_frame(
        &mut self,
        cx: &mut Cx<'_, H>,
        now: pyreal_primitives::LocalTime,
        notices: UiNotices,
    ) {
        service_journal(cx);
        self.bring_up(cx);
        self.reread_files_when_patched(cx);
        self.follow_plugins(cx);
        self.horizon_input(cx, now);
        let changed = self
            .horizon
            .ui
            .as_mut()
            .is_some_and(|ui| ui.frame(cx, now, notices));
        if let Some(input) = self.shared.input.as_mut() {
            input.finish_session_retirement(|released| cx.accept_actions(released));
        }
        if changed {
            cx.follow_screen_change(self);
        }
        cx.teleport_use_time(self);
        cx.portal_space_use_time(self);
        service_journal(cx);
    }

    fn talk_focus_notice(
        &mut self,
        talk_focus: pyreal_client_model::chat::TalkFocus,
        notice: pyreal_client_model::chat::TalkFocusNotice,
    ) -> bool {
        self.horizon
            .ui
            .as_mut()
            .is_some_and(|ui| ui.talk_focus_notice(talk_focus, notice))
    }

    /// Jump's notices are synchronous with their input/control-loss owner. An absent subscriber
    /// leaves no retained notice history.
    fn deliver_power_bar_notices(
        &mut self,
        _hud: &mut crate::hud::Hud,
        notices: Vec<pyreal_client_model::combat::PowerBarNotice>,
    ) {
        if let Some(ui) = self.horizon.ui.as_mut() {
            ui.deliver_power_bar_notices(notices);
        }
    }

    fn emit_magic_notices(&mut self, notices: Vec<pyreal_client_contract::view::MagicNotice>) {
        if let Some(ui) = self.horizon.ui.as_mut() {
            ui.emit_magic_notices(notices);
        }
    }

    fn waypoint_command(
        &mut self,
        command: pyreal_client_model::cmd::waypoint::WaypointCommand,
    ) -> Option<String> {
        self.horizon
            .ui
            .as_mut()
            .map(|ui| ui.waypoint_command(command))
    }

    fn open_vendor_buying(&mut self, _hud: &mut crate::hud::Hud) {
        if let Some(ui) = self.horizon.ui.as_mut() {
            ui.open_vendor_buying();
        }
    }

    fn update_cursor(&mut self, cx: &mut Cx<'_, H>) {
        // The game's own cursors over the world only: over the interface the pointer is the
        // plain arrow.
        let over_world = self
            .horizon
            .ui
            .as_ref()
            .is_none_or(|ui| !ui.ui.pointer_over_ui);
        let found = cx.found_object();
        let inputs = crate::cursor::CursorInputs {
            busy: cx.busy_count(),
            target_mode: cx.target_mode().into(),
            combat_mode: cx.model().combat.combat_mode,
            hovering: over_world && found.0 != 0,
            target_compatible: over_world
                && crate::cursor::is_target_compatible_with_targeting_object(
                    cx.model(),
                    cx.model().targeting_object,
                    found,
                ),
        };
        let store = std::sync::Arc::clone(cx.store());
        self.shared.cursor.update_cursor(&store, inputs);
    }

    /// What the interface copied this frame goes to the host's clipboard.
    fn sync_clipboard(&mut self) {
        if let Some(text) = self
            .horizon
            .ui
            .as_mut()
            .and_then(pyreal_horizon::runtime::HorizonFrontEnd::take_copied)
        {
            use crate::clipboard::HostClipboard;
            if let Err(e) = self.shared.host_clipboard.set_text(&text) {
                tracing::warn!("clipboard write failed: {e:?}");
            }
        }
    }

    fn copy_text(&mut self, text: &str) -> bool {
        use crate::clipboard::HostClipboard;
        match self.shared.host_clipboard.set_text(text) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("clipboard write failed: {e:?}");
                false
            }
        }
    }

    fn compose_ui(&mut self, cx: &mut Cx<'_, H>) {
        if let Some(ui) = self.horizon.ui.as_mut() {
            horizon::compose(ui, cx);
        }
        Self::plugin_world_marks(cx);
    }

    /// The interface's overlay, then the host's own controls over the whole window.
    fn draw_ui(
        &mut self,
        present: &mut Self::Present,
    ) -> Result<(), pyreal_client_runtime::present::PresentError> {
        if !self.host_hides_ui {
            if let Some(ui) = self.horizon.ui.as_ref() {
                ui.draw(present)?;
            }
        }
        // The host's own controls go over its whole window, whatever part of it the game drew in.
        let region = present.inset();
        if region.is_some() {
            present.set_inset(None);
        }
        let drawn = present.draw_overlay(&self.host_overlay);
        if region.is_some() {
            present.set_inset(region);
        }
        drawn
    }

    /// The interface lets go of what it holds; a pointer held for a camera drag is shown again
    /// before the window goes.
    fn cleanup_ui(&mut self, _cx: &mut Cx<'_, H>) {
        self.shared.pointer.let_go();
        self.horizon.ui = None;
    }

    /// Back to the screens before any world: the interface's session state, the plugins' windows
    /// and a held pointer go.
    fn session_ended(&mut self, cx: &mut Cx<'_, H>) {
        self.shared.pointer.let_go();
        self.plugins = plugins::PluginSurface::default();
        if let Some(ui) = self.horizon.ui.as_mut() {
            ui.session_ended(cx);
        }
    }
}

/// One host event: a lifecycle event to the runtime's window procedure (and its messages on to
/// the input manager, which releases every held control on a focus loss); a device event to the
/// interface first, and on to the latches, the window procedure and the input manager only when
/// the interface does not keep it.
///
/// A close request is always honoured, Alt+F4's included.
pub(crate) fn route_host_event<H: Host>(
    cx: &mut Cx<'_, H>,
    shell: &mut ClientShell<H>,
    event: &pyreal_input::host::HostEvent,
    time_ms: u32,
) {
    use pyreal_input::host::HostEvent;
    if let Some(lifecycle) = crate::platform::window::lifecycle(event) {
        cx.window_event(shell, &lifecycle, time_ms);
        if let Some(input) = shell.shared.input.as_mut() {
            for m in pyreal_client_runtime::pump::Pump::map_window_event(&lifecycle) {
                input.on_message(crate::pump::from_window(m, time_ms));
            }
        }
        return;
    }
    if let HostEvent::KeyboardInput { key, pressed, .. } = event {
        if plugins::plugin_hotkey(cx, shell, *key, *pressed) {
            return;
        }
    }
    // A paste key in a text box reads the clipboard on the keystroke.
    if let HostEvent::KeyboardInput {
        key, pressed: true, ..
    } = event
    {
        if let Some(ui) = shell.horizon.ui.as_mut() {
            if ui.wants_paste(key.virtual_key) {
                use crate::clipboard::HostClipboard;
                let text = shell.shared.host_clipboard.get_text().ok().flatten();
                ui.offer_paste(text);
            }
        }
    }
    let forward = shell
        .horizon
        .ui
        .as_mut()
        .is_none_or(|ui| ui.host_event(event, time_ms).forward);
    if cx.exploring() && explore_event(cx, event, forward) {
        return;
    }
    match event {
        HostEvent::CursorMoved { x, y } => cx.cursor_moved(*x, *y),
        // Either button held over the world turns the camera with the pointer, and both together
        // run forward; letting go ends it wherever the pointer is.
        HostEvent::MouseInput {
            button:
                button
                @ (pyreal_input::keys::MouseButton::Left | pyreal_input::keys::MouseButton::Right),
            pressed,
        } if forward || !*pressed => {
            cx.orbit_button(*button == pyreal_input::keys::MouseButton::Right, *pressed);
        }
        // The middle button over the world locks or frees the run.
        HostEvent::MouseInput {
            button: pyreal_input::keys::MouseButton::Middle,
            pressed: true,
        } if forward => {
            use pyreal_client_runtime::actions::{Action, ActionId};
            const AUTORUN: ActionId = pyreal_client_contract::actions::movement::AUTORUN;
            cx.inject_action(Action::begin(AUTORUN));
            cx.inject_action(Action::end(AUTORUN));
        }
        HostEvent::KeyboardInput { key, pressed, .. } if forward => {
            flycam_key(cx, shell, *key, *pressed);
        }
        _ => {}
    }
    if !forward {
        return;
    }
    for m in shell.shared.devices.map_device_event(event, time_ms) {
        cx.window_message(crate::pump::window_message(m), m.time_ms);
        if let Some(input) = shell.shared.input.as_mut() {
            input.on_message(m);
        }
    }
}

/// Explore mode's keys and the wheel's speed steps, taken whole: `true` when the event was one.
fn explore_event<H: Host>(
    cx: &mut Cx<'_, H>,
    event: &pyreal_input::host::HostEvent,
    forward: bool,
) -> bool {
    use pyreal_client_runtime::explore::Key;
    use pyreal_input::host::HostEvent;
    match event {
        HostEvent::KeyboardInput { key, pressed, .. } => {
            let Some(k) = explore_key_of(key.virtual_key) else {
                return false;
            };
            if forward || !*pressed {
                cx.explore_key(k, *pressed);
            }
            true
        }
        HostEvent::MouseWheel { notches } if forward => {
            let k = if *notches > 0.0 {
                Key::Faster
            } else {
                Key::Slower
            };
            if *notches != 0.0 {
                cx.explore_key(k, true);
            }
            true
        }
        _ => false,
    }
}

/// The explore key a virtual key is: WASD or the arrows fly, E or Space rise, Q sinks, plus and
/// minus step the speed, C copies the place and R the report line, F9 saves the frame timings.
fn explore_key_of(virtual_key: usize) -> Option<pyreal_client_runtime::explore::Key> {
    use pyreal_client_runtime::explore::Key;
    Some(match virtual_key {
        0x57 | 0x26 => Key::Forward,
        0x53 | 0x28 => Key::Back,
        0x41 | 0x25 => Key::Left,
        0x44 | 0x27 => Key::Right,
        0x45 | 0x20 => Key::Up,
        0x51 => Key::Down,
        0xBB | 0x6B => Key::Faster,
        0xBD | 0x6D => Key::Slower,
        0x43 => Key::CopyPlace,
        0x52 => Key::CopyReport,
        0x78 => Key::SaveTimings,
        _ => return None,
    })
}

/// The residual flycam's two keys. See [`App::flycam_key`](crate::app::App::flycam_key).
pub(crate) fn flycam_key<H: Host>(
    cx: &mut Cx<'_, H>,
    shell: &ClientShell<H>,
    key: pyreal_input::keys::Key,
    down: bool,
) {
    use pyreal_input::keys::Key;
    if shell
        .shared
        .input
        .as_ref()
        .is_some_and(crate::input::InputShell::keyboard_blocked)
    {
        return;
    }
    if key == Key::SPACE {
        cx.flycam_rise(down);
    } else if key == Key::KEY_C {
        cx.flycam_sink(down);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pad with its south button held, whenever it is read.
    struct HeldPad;

    impl crate::gamepad::HostGamepad for HeldPad {
        fn poll(&mut self) -> Option<crate::gamepad::PadState> {
            let mut s = crate::gamepad::PadState::default();
            s.set(pyreal_input::pad::PadButton::South, true);
            Some(s)
        }
    }

    /// A host whose pad is [`HeldPad`], and nothing else under it.
    struct PadHost;

    impl Host for PadHost {
        const BUILD_ID: &'static str = "pad-test";
        type Clipboard = crate::clipboard::NoClipboard;
        fn open_platform(
            cfg: &pyreal_client_runtime::config::Config,
            events: crate::platform::window::WindowEvents,
        ) -> Result<pyreal_client_runtime::app::Platform, pyreal_client_runtime::app::StartupError>
        {
            crate::platform::host::NullHost::open_platform(cfg, events)
        }
        fn local_utc_offset_secs(_: i64) -> i32 {
            0
        }
        fn launch_uri(_: &str) -> i32 {
            0
        }
        fn install_default_output() {}
        fn clipboard() -> Self::Clipboard {
            crate::clipboard::NoClipboard
        }
        fn cursor_images(window: Option<isize>) -> Box<dyn crate::cursor::CursorImages> {
            crate::platform::host::NullHost::cursor_images(window)
        }
        fn gamepad() -> Box<dyn crate::gamepad::HostGamepad> {
            Box::new(HeldPad)
        }
    }

    /// A front end on a window reads the host's pad; one with no window, as a headless run has,
    /// reads none, so the pads plugged into the machine never reach a run nobody plays.
    #[test]
    fn a_front_end_reads_the_hosts_pad_on_a_window_and_none_without_one() {
        let mut windowed = ClientShell::<PadHost>::new(Some(1));
        let held = windowed.shared.gamepad.poll().expect("the host's pad");
        assert!(held.held(pyreal_input::pad::PadButton::South));
        let mut headless = ClientShell::<PadHost>::new(None);
        assert_eq!(headless.shared.gamepad.poll(), None);
    }
}

#[cfg(test)]
#[path = "../tests/horizon_tests.rs"]
mod horizon_tests;

#[cfg(test)]
#[path = "../tests/patch_reread_tests.rs"]
mod patch_reread_tests;

#[cfg(test)]
#[path = "../tests/pointer_hold_tests.rs"]
mod pointer_hold_tests;

#[cfg(test)]
#[path = "../tests/report_command_tests.rs"]
mod report_command_tests;

#[cfg(test)]
#[path = "../tests/plugin_tests.rs"]
mod plugin_tests;
