//! The `Render.*` preference owners on the scene.
//!
//! The model half of this module -- `RenderPreferences`, its names and the choice decodes -- is
//! [`pyreal_client_runtime::render_prefs`], glob-re-exported here,
//! so `pyreal_client_runtime::render_prefs::RenderPreferences` and every one of its siblings resolve here
//! too. What this module adds is the scene-owned filter of the options page's Apply; the screen's
//! brightness is the presentation's own ([`crate::gpu::SceneRenderer::set_brightness`]).

pub use pyreal_client_runtime::render_prefs::*;

/// The `Render.*` names whose owner is [`pyreal_client_runtime::scene::SceneConfig::render`] rather than the
/// device: the projection pair and the degrade distance.
pub fn apply_scene_preference_requests(
    prefs: &mut RenderPreferences,
    requests: Vec<pyreal_client_contract::UiRequest>,
) -> Vec<pyreal_client_contract::UiRequest> {
    use pyreal_client_contract::UiRequest;
    requests
        .into_iter()
        .filter(|r| {
            if let UiRequest::SetPreference(name, value) = r {
                // `Render.ScreenBrightness` normally never reaches here: the device consumed it
                // above, and `Gpu::gamma()` is its live owner from that point on. It stays in
                // [`RenderPreferences::set_named`] so that a build with no device still records
                // it, which is what the start-up path hands the device in the first place.
                return !prefs.set_named(name, value);
            }
            true
        })
        .collect()
}
