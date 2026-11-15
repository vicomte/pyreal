//! The HUD.
//!
//! The model — the object rows, the chat composition, the property caches and the read-only
//! [`HudView`] projection that is the game's `GameView` — is [`pyreal_client_runtime::hud`], and every
//! item of it is re-exported here at its historical path. What this module adds is the
//! interface's receivers, kept beside the model in the slot every front end has (`HudSlot`),
//! which the model lends the receivers it feeds as events land.

use pyreal_client_runtime::hud::*;

/// The HUD model and the interface's receivers beside it. Everything else is the model's, through
/// `Deref`.
#[derive(Debug, Default)]
pub struct Hud {
    model: pyreal_client_runtime::hud::Hud,
    /// The Horizon interface's receivers, which take the model's offers.
    pub horizon: pyreal_horizon::runtime::HorizonHudPanels,
}

impl Hud {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a batch of session events to the model, with this panel set as its receivers. See
    /// [`pyreal_client_runtime::hud::Hud::apply_events`].
    pub fn apply_events(
        &mut self,
        events: &[pyreal_client_net::client_session::SessionEvent],
        world: &mut pyreal_client_model::World,
    ) -> Vec<pyreal_client_contract::chat::interface::ChatMessage> {
        self.model.apply_events_with_combat_mode_handler(
            events,
            world,
            &mut self.horizon,
            None,
            &mut |_, _| {},
        )
    }
}

impl Hud {
    /// [`Self::apply_events`] with the combat system's callback and the UI's request queue. See
    /// [`pyreal_client_runtime::hud::Hud::apply_events_with_combat_mode_handler`].
    pub fn apply_events_with_combat_mode_handler(
        &mut self,
        events: &[pyreal_client_net::client_session::SessionEvent],
        world: &mut pyreal_client_model::World,
        ui_requests: Option<&mut pyreal_client_contract::requests::Outbox>,
        on_combat_mode: &mut dyn FnMut(
            &mut pyreal_client_model::World,
            pyreal_client_model::combat::CombatMode,
        ),
    ) -> Vec<pyreal_client_contract::chat::interface::ChatMessage> {
        self.model.apply_events_with_combat_mode_handler(
            events,
            world,
            &mut self.horizon,
            ui_requests,
            on_combat_mode,
        )
    }
}

impl HudSlot for Hud {
    fn split(&mut self) -> (&mut pyreal_client_runtime::hud::Hud, &mut dyn HudPanels) {
        (&mut self.model, &mut self.horizon)
    }
}

impl std::ops::Deref for Hud {
    type Target = pyreal_client_runtime::hud::Hud;
    fn deref(&self) -> &Self::Target {
        &self.model
    }
}

impl std::ops::DerefMut for Hud {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.model
    }
}

/// Hand the model crate the platform answers it cannot compute itself: the local-time shift the
/// C runtime's `localtime` would apply and the URL launch, which are host `H`'s, and the caret
/// blink interval. Idempotent: the first host installed stands.
pub fn install_platform<H: crate::platform::host::Host>() {
    pyreal_client_runtime::platform::clock::install_local_utc_offset(H::local_utc_offset_secs);
    pyreal_client_runtime::platform::shell::install_uri_launcher(H::launch_uri);
    pyreal_client_runtime::platform::caret::install_caret_blink(H::caret_blink_secs);
}
