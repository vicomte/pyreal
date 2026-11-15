//! The `Render.*` preference owners, over the scene's: every item of
//! [`pyreal_scene::render_prefs`] is glob-re-exported here, so
//! `pyreal_client_runtime::render_prefs::RenderPreferences` and its siblings resolve here too. What this
//! module adds is the Apply over the whole renderer, which holds both owners.

pub use pyreal_scene::render_prefs::*;

/// The options page's Apply, for every `Render.*` name this module owns.
///
/// Retail has no per-name apply at all: every
/// registration passes the same render-preference-changed callback, which only recomputes the
/// cached overall graphics quality; the next frame's preference poll performs the work.
/// So the page writes the variable and the renderer
/// notices — which is what this does: the gpu-owned names land on the device, the scene-owned
/// ones land on [`pyreal_client_runtime::scene::SceneConfig::render`], and `view_params` and the part pass read
/// them on the next frame.
#[cfg(gpu)]
pub fn apply_preference_requests(
    renderer: &mut crate::gpu::Renderer,
    requests: Vec<pyreal_client_contract::UiRequest>,
) -> Vec<pyreal_client_contract::UiRequest> {
    renderer.apply_render_preference_requests(requests)
}
