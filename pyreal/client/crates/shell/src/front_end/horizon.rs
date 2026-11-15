//! The Horizon interface: brought up on its own art, which the client carries built in.

use pyreal_horizon::runtime::HorizonFrontEnd;

use super::Cx;
use crate::platform::host::Host;

/// The Horizon interface, once it has been brought up.
#[derive(Debug, Default)]
pub(crate) struct HorizonFace {
    pub ui: Option<HorizonFrontEnd>,
    /// Its key bindings page, carried out on the input manager.
    pub keys: super::horizon_keys::HorizonKeys,
}

/// Bring the Horizon interface up: its own art and its settings from its settings file.
pub(super) fn build_horizon<H: Host>(cx: &mut Cx<'_, H>) -> HorizonFrontEnd {
    let pieces = std::sync::Arc::new(pyreal_horizon::pieces::Pieces::built_in());
    let preferences = cx.config().preferences_file.clone();
    let settings = preferences
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .map(|d| d.join(pyreal_horizon::options::FILE_NAME));
    let mut options = settings
        .as_deref()
        .map(pyreal_horizon::options::HorizonOptions::load)
        .unwrap_or_default();
    // `--enter-world`: the screens log in (`-u`, else the first), or make the character `-r`
    // names first; then each `--say` line is said in the world.
    let config = cx.config();
    if config.enter_world {
        options.auto_login = true;
        options.immediate_entry = config.hosted_entry;
        let named = |s: &str| (!s.is_empty()).then(|| s.to_owned());
        options.start_character = named(&config.start_char);
        options.create_character = named(&config.create_char);
        options.script.extend(
            config
                .say
                .iter()
                .map(|l| pyreal_horizon::options::ScriptStep::Say(l.clone())),
        );
    }
    let art = std::sync::Arc::new(pyreal_horizon::art::Art::new(pieces));
    let mut ui = HorizonFrontEnd::new(art, options, settings);
    ui.start(cx);
    ui
}

/// The interface drew its frame before the world moved for this one: what stands over the world
/// is put where the world is drawn now, with this frame's camera, and a name takes the depth of
/// the point it stands on.
pub(super) fn compose<H: Host>(ui: &mut HorizonFrontEnd, cx: &mut Cx<'_, H>) {
    use pyreal_horizon::draw::{Anchor, Spot};
    let (present, world) = cx.present_with_world();
    let present = &*present;
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: a pixel on screen, a few thousand at most, exact in f32.
    ui.follow_drawn_world(|id, anchor| match anchor {
        Anchor::Top => present
            .target_top(id, world)
            .map(|(x, y, depth)| Spot { x, y, depth }),
        Anchor::Origin => present
            .target_origin(id, world)
            .map(|(x, y)| Spot::at(x as f32, y as f32)),
    });
    ui.compose(cx);
    for error in ui.errors.drain(..) {
        tracing::warn!("Horizon interface: {error}");
    }
}
