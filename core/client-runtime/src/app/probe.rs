//! Opt-in application observations and fixture control boundaries.

use super::*;

/// Read-only access to application observations used by fixtures.
pub struct AppProbe<'a, S: Shell> {
    app: &'a App<S>,
}

/// Mutable access to application fixture controls.
pub struct AppProbeMut<'a, S: Shell> {
    app: &'a mut App<S>,
}

impl<S: Shell> std::fmt::Debug for AppProbe<'_, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AppProbe").field(&self.app).finish()
    }
}

impl<S: Shell> std::fmt::Debug for AppProbeMut<'_, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AppProbeMut").field(&self.app).finish()
    }
}

impl<S: Shell> App<S> {
    /// Borrow the application's fixture observations.
    pub const fn probe(&self) -> AppProbe<'_, S> {
        AppProbe { app: self }
    }

    /// Borrow the application's fixture controls.
    pub fn probe_mut(&mut self) -> AppProbeMut<'_, S> {
        AppProbeMut { app: self }
    }
}

impl<'a, S: Shell> AppProbe<'a, S> {
    /// The character controls the input seam has latched, for the tests.
    ///
    /// The only writer is [`App::apply_input_actions`], which runs on what
    /// `fire::walk_input_maps` produced -- so reading `forward` here is reading "did a control
    /// survive the walk", which is precisely what the typing barrier decides.
    #[must_use]
    pub const fn char_input(self) -> crate::character::CharacterInput {
        self.app.char_input
    }

    /// The camera/flycam controls, for the tests.
    #[must_use]
    pub const fn camera_input(self) -> CameraInput {
        self.app.input
    }

    /// The command interpreter's three command lists and the counters
    /// on them, for the tests.
    ///
    /// The one that matters is
    /// [`crate::character::MovementCommands::transient_motions_issued`], which is the denominator
    /// for `move_player`'s do-motion/stop-motion tail: *no emote key was pressed* and
    /// *the drain never ran* are the same reading of an empty queue without it.
    #[must_use]
    pub const fn movement(self) -> &'a crate::character::MovementCommands {
        &self.app.movement
    }

    /// The movement command interpreter's three command lists, for the tests.
    ///
    /// "Hold `A`, press `D`, release `D`, still turning left" is a statement about the **turn
    /// list**, not about a bool, so the test has to be able to read the list.
    #[must_use]
    pub const fn movement_commands(self) -> &'a crate::character::MovementCommands {
        &self.app.movement
    }

    /// What the three host-side action arms did:
    /// `(visibility-toggle dispatches, of those that answered TRUE, stop-completely calls,
    /// screenshots saved, screenshots failed)`.
    #[must_use]
    pub const fn action_arm_host_stats(self) -> (u64, u64, u64, u64, u64) {
        (
            self.app.events.total(FrameEventKind::EscapeOptionsToggle),
            self.app.events.amount(FrameEventKind::EscapeOptionsToggle),
            self.app.events.total(FrameEventKind::EscapeStopPerformed),
            self.app.events.total(FrameEventKind::ScreenshotSaved),
            self.app.events.total(FrameEventKind::ScreenshotFailed),
        )
    }

    /// The dat store this client reads, shared with its world. A probe for the tests: whether
    /// the DDD interrogation's product id granted `client_highres.dat`.
    #[must_use]
    pub fn dat_store(self) -> &'a std::sync::Arc<pyreal_dat::RetailDatStore> {
        &self.app.store
    }

    /// How many input actions [`App::apply_input_actions`] routed into the body or
    /// the camera this session.
    ///
    /// A denominator, not decoration: "typing did not walk the character" is the same observation
    /// as "no action arrived at all", and a test that cannot tell them apart passes on a build that
    /// blocks movement for ever.
    #[must_use]
    pub const fn actions_routed(self) -> u64 {
        self.app.events.total(FrameEventKind::ActionRouted)
    }

    /// `(edges consumed, `0x01B7`s the state gate let through)`.
    #[must_use]
    pub const fn new_forward_attack_aborts(self) -> (u64, u64) {
        (
            self.app
                .events
                .total(FrameEventKind::NewForwardAttackAborted),
            self.app
                .events
                .total(FrameEventKind::NewForwardAttackCancelSent),
        )
    }

    /// How many `0x01A1`s [`App::log_off_character`]'s player-module save has
    /// sent. The unforced `save_to_server`'s gate is
    /// dirty-or-forced, so this counts *dirty* logouts, not logouts.
    #[must_use]
    pub const fn player_modules_saved_at_logout(self) -> u64 {
        self.app
            .events
            .total(FrameEventKind::PlayerModuleSavedAtLogout)
    }

    /// How many body turns [`App::apply_camera_turn`] took off the camera and gave
    /// to the command interpreter this session.
    #[must_use]
    pub const fn camera_turns_applied(self) -> u64 {
        self.app
            .events
            .total(FrameEventKind::WorldCameraTurnApplied)
    }

    /// How many frames reached [`App::command_interpreter_control_transfer`], how
    /// many accepted calls ran `lose_control_to_server`, and how many took control back. Ordered
    /// player dispatch can lose control more than once in a frame; each accepted call is counted.
    ///
    /// Three numbers rather than one: a run where the server never sent a non-autonomous buffer
    /// and a run where the step was never called read `(n, 0, 0)` and `(0, 0, 0)`, and only the
    /// first is a client that is working.
    #[must_use]
    pub const fn control_transfer_counts(self) -> (u64, u64, u64) {
        (
            self.app.events.total(FrameEventKind::ControlTransfer),
            self.app.events.total(FrameEventKind::ServerControlLost),
            self.app.events.total(FrameEventKind::ServerControlRetaken),
        )
    }

    /// Pending releases attempted, and actual nonzero motion-interpreter results.
    #[must_use]
    pub const fn jump_counts(self) -> (u64, u64) {
        (
            self.app.events.total(FrameEventKind::JumpRequested),
            self.app.events.amount(FrameEventKind::JumpRequested),
        )
    }

    /// Input-dispatch passes, not autonomous `DoJump` calls per frame, despite the name.
    #[must_use]
    pub const fn jump_use_times(self) -> u64 {
        self.app.events.total(FrameEventKind::JumpUseTime)
    }

    /// The last actual successful jump request; offline observers see the same pack transport
    /// receives, not a substitute acceptance answer or a later post-physics reconstruction.
    #[must_use]
    pub fn last_jump_request(self) -> Option<&'a pyreal_protocol::movement::MovementJump> {
        self.app.last_jump_request.as_ref()
    }

    /// How many `0xF753` and `0xF61C` blobs this client has produced.
    ///
    /// Exposed so a test can assert **emission**, which no replay anchor can see.
    #[must_use]
    pub const fn position_reporter_stats(
        self,
    ) -> pyreal_client_net::client_session::PositionReporterStats {
        self.app.position.stats
    }

    /// How many frames have reached [`App::position_use_time`].
    ///
    /// One per drawn frame. A test that asserts this equals [`App::frames_drawn`] is asserting
    /// the **call site**, which no counter inside the reporter can do.
    #[must_use]
    pub const fn position_use_times(self) -> u64 {
        self.app.position_use_times()
    }

    /// How many frames have reached [`App::player_teleport_use_time`].
    ///
    /// One per drawn frame, plus one per admitted session event on a frame that had traffic — the
    /// per-blob smart-box event-dispatch boundary. A test that asserts this equals
    /// [`App::frames_drawn`] therefore drives an `App` with no link, and is asserting the
    /// **call site**, which no counter inside the object stream can do — a mutation deleting the
    /// call from `App::frame` is otherwise unobservable.
    #[must_use]
    pub const fn player_teleport_use_times(self) -> u64 {
        self.app.events.total(FrameEventKind::PlayerTeleportUseTime)
    }

    /// How many server teleports actually moved this client's body.
    ///
    /// Exposed for the same reason [`Self::position_reporter_stats`] is: replay anchors prove
    /// *encoding*, while a producer or consumer that
    /// never fires is invisible to them.
    #[must_use]
    pub const fn player_teleports_applied(self) -> u64 {
        self.app.events.total(FrameEventKind::PlayerTeleportApplied)
    }

    /// Teleports that arrived before there was a body to move. See
    /// [`App::player_teleport_use_time`] for why they are dropped rather than queued.
    #[must_use]
    pub const fn player_teleports_before_a_body(self) -> u64 {
        self.app
            .events
            .amount(FrameEventKind::PlayerTeleportBeforeABody)
    }

    /// How many times [`App::reset_world_view`] has run.
    #[must_use]
    pub const fn world_resets(self) -> u64 {
        self.app.events.total(FrameEventKind::WorldReset)
    }

    /// The switches the landscape is (re)built from, for the tests.
    #[must_use]
    pub fn scene_config(self) -> Option<crate::scene::SceneConfig> {
        self.app.scene_config
    }
}

impl<'a, S: Shell> AppProbeMut<'a, S> {
    /// The teleport state, mutably, so a test can feed it the session events a live server would
    /// and then let the application's own frame do the rest.
    pub const fn teleport_mut(self) -> &'a mut crate::teleport::Teleport {
        &mut self.app.teleport
    }

    /// The HUD state, mutably, so a test can feed it a recorded session's events.
    pub fn hud_mut(self) -> &'a mut S::Hud {
        &mut self.app.hud
    }

    /// The same tables, mutable. A test seeds a player and an inventory the
    /// way a `0x0013 Login_PlayerDescription` would, so that a combat-mode toggle has something to
    /// decide against without a server.
    pub fn objects_mut(self) -> &'a mut crate::objects::ObjectStream {
        &mut self.app.objects
    }

    /// The interaction layer and the game model **at the same time**.
    ///
    /// Several of the production entry points a harness has to call take both --
    /// `Interaction::on_world_object_found(found, &mut world, now)` is the one that named it -- and
    /// [`Self::interaction_mut`] and [`Self::objects_mut`] are two separate borrows of `self`, so a
    /// caller with only those two has to build a model host of its own per scenario. They are
    /// disjoint fields; this hands out both.
    pub fn interaction_and_world_mut(
        self,
    ) -> (
        &'a mut crate::interaction::Interaction,
        &'a mut pyreal_client_model::World,
    ) {
        (&mut self.app.interaction, &mut self.app.objects.world)
    }

    /// The same state, writable — the sibling of [`Self::objects_mut`] and
    /// for the same reason: a harness that drives a real `App` must be
    /// able to hand it the pointer events would have
    /// dispatched, which is [`crate::interaction::Interaction::queue`]. Without it
    /// the only way to exercise a click against a *resized* smart box is to build an `Interaction`
    /// by hand, and an `Interaction` built by hand has never had `App::frame` push
    /// `<SBOX>`'s rectangle into it, which is precisely the step under test.
    pub fn interaction_mut(self) -> &'a mut crate::interaction::Interaction {
        &mut self.app.interaction
    }

    /// The production UI-queue interaction boundary, also usable without a socket by replay
    /// harnesses. `App::frame` delivers the already ordered session events here.
    ///
    /// **It runs with no panels callback and no selection geometry**, which is not what
    /// [`App::frame`] does — see [`Self::apply_interaction_events_to_panels`] for the boundary
    /// the frame really uses. This one is kept as it is because eighty-odd stations under
    /// `tests/` are written against it.
    pub fn apply_interaction_events(
        self,
        events: &[pyreal_client_net::client_session::SessionEvent],
    ) {
        self.app.interaction.last_use_time = pyreal_primitives::LocalTime(self.app.timer.cur_time);
        crate::interaction::apply_events(
            &mut self.app.interaction,
            events,
            &mut self.app.objects.world,
        );
    }

    /// The same boundary **as `App::frame` runs it**: the current selection geometry, and the
    /// frame's own panels callback, so that a panel which is a cached join of the notice stream
    /// sees the message.
    ///
    /// [`Self::apply_interaction_events`] passes `None` geometry and a no-op
    /// `panels` argument, so a harness that delivers through it has every panel blind to the
    /// message it delivers -- silently, which makes a wrong assertion look right rather than
    /// red. The two lines below are `frame`'s own, lifted so a harness can reach them; `frame`
    /// itself is unchanged and still calls `apply_events_at_boundary` inline.
    pub fn apply_interaction_events_to_panels(
        self,
        shell: &mut S,
        events: &[pyreal_client_net::client_session::SessionEvent],
    ) {
        self.app.interaction.last_use_time = pyreal_primitives::LocalTime(self.app.timer.cur_time);
        let (geometry, radius) = self.app.current_selection_geometry();
        let hud = &mut self.app.hud;
        crate::interaction::apply_events_at_boundary(
            &mut self.app.interaction,
            events,
            &mut self.app.objects.world,
            Some((&geometry, radius)),
            &mut |inter, world, notice| object_panel_notice(shell, hud, inter, world, notice),
        );
    }

    /// …and writable.
    pub fn world_state_mut(self) -> Option<&'a mut crate::world_state::WorldState> {
        self.app.world.as_mut()
    }

    /// Have the presentation load a world, straight away and outside the frame:
    /// what the device tests did through the renderer, with the world state landing here.
    ///
    /// # Errors
    /// As [`crate::present::Presentation::load_world`].
    pub fn load_world(
        self,
        store: &std::sync::Arc<pyreal_dat::RetailDatStore>,
        cfg: crate::scene::SceneConfig,
    ) -> Result<(), pyreal_world_data::landblock::WorldError> {
        self.app.present.load_world(store, cfg, &mut self.app.world)
    }
}
