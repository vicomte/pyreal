//! The renderer: the scene's (`pyreal_scene::gpu::SceneRenderer`), as the presentation
//! seam names it. The device, the frame bracket, the world drawn by the engine, the preview spaces
//! and the UI's picture are the scene's.

/// The retail surface the first-pixel quad draws, re-exported from
/// [`pyreal_client_runtime::assets`] -- it is a dat id and nothing else -- so that
/// `pyreal_client_runtime::assets::FIRST_PIXEL_SURFACE` resolves too.
pub use pyreal_client_runtime::assets::FIRST_PIXEL_SURFACE;

/// The preview-space id.
pub use pyreal_client_contract::overlay::PreviewSpace as PreviewId;

/// The scene's half of the renderer, which [`Renderer`] holds and dereferences to.
#[cfg(gpu)]
pub use pyreal_scene::gpu::SceneRenderer;

#[cfg(gpu)]
pub use imp::*;

#[cfg(gpu)]
mod imp {
    use std::path::Path;

    use pyreal_client_contract::overlay::{OverlayItem, OverlayReleased, OverlayTexture};
    use pyreal_client_contract::window::WindowHandles;
    use pyreal_client_runtime::present::PresentError;
    use pyreal_client_runtime::world_state::WorldState;
    use pyreal_primitives::{AssetSource, DataId};

    use super::{PreviewId, SceneRenderer};

    fn err(e: pyreal_scene::gpu::DeviceError) -> PresentError {
        PresentError(e.0)
    }

    /// The scene's renderer.
    #[derive(Debug)]
    pub struct Renderer {
        /// The device, the drawn world and the preview spaces. The renderer dereferences to it.
        device: SceneRenderer,
    }

    impl std::ops::Deref for Renderer {
        type Target = SceneRenderer;
        fn deref(&self) -> &SceneRenderer {
            &self.device
        }
    }

    impl std::ops::DerefMut for Renderer {
        fn deref_mut(&mut self) -> &mut SceneRenderer {
            &mut self.device
        }
    }

    impl Renderer {
        /// Construct the renderer and its graphics device: [`SceneRenderer::new`].
        ///
        /// # Errors
        /// The device's message when no device can be created.
        pub fn new(
            window: Option<WindowHandles>,
            width: u32,
            height: u32,
        ) -> Result<Self, PresentError> {
            Ok(Self::around(
                SceneRenderer::new(window, width, height).map_err(err)?,
            ))
        }

        /// The renderer over `device`.
        #[must_use]
        pub fn around(device: SceneRenderer) -> Self {
            Self { device }
        }
    }

    /// The one implementation of the presentation seam: forwards to the scene's renderer.
    impl pyreal_client_runtime::present::Presentation for Renderer {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }

        fn prepare_graphics_device(&mut self) {}

        fn start_frame(&mut self) -> Result<(), PresentError> {
            self.device.start_frame();
            Ok(())
        }

        fn set_game_viewport(&mut self, viewport: Option<pyreal_primitives::Viewport>) {
            self.device.set_game_viewport(viewport);
        }

        fn draw_scene(&mut self, ws: Option<&WorldState>) -> Result<(), PresentError> {
            self.device.draw_scene(ws);
            Ok(())
        }

        fn end_frame(&mut self) -> Result<(), PresentError> {
            self.device.end_frame().map_err(err)
        }

        fn window_hidden(&self) -> bool {
            self.device.window_hidden()
        }

        fn wait_idle(&mut self) -> Result<(), PresentError> {
            self.device.wait_idle();
            Ok(())
        }

        fn size(&self) -> (u32, u32) {
            self.device.size()
        }

        fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError> {
            self.device.resize(width, height).map_err(err)
        }

        fn set_presentation_sync(&mut self, full_screen: bool, sync_to_refresh: bool) {
            self.device
                .set_presentation_sync(full_screen, sync_to_refresh);
        }

        fn set_inset(&mut self, region: Option<pyreal_primitives::Inset>) {
            self.device.set_inset(region);
        }

        fn inset(&self) -> Option<pyreal_primitives::Inset> {
            self.device.inset()
        }

        fn capture_png(&mut self, path: &Path) -> Result<(), PresentError> {
            self.device.capture_png(path).map_err(err)
        }

        /// The Standard look's filtering is gone: the engine filters every picture its own way.
        fn texture_filtering(&self) -> u32 {
            0
        }

        fn hifi_availability(&self) -> pyreal_client_contract::options::fidelity::Availability {
            pyreal_client_contract::options::fidelity::Availability {
                built: true,
                widened: true,
                supported: true,
                rays: Some(self.device.gpu.rays()),
                active: true,
                failed: None,
            }
        }

        fn render_scale_in_use(&self) -> Option<u32> {
            self.device.render_scale_in_use()
        }

        fn render_context(
            &self,
            world: Option<&WorldState>,
        ) -> pyreal_client_runtime::report::RenderContext {
            self.device.render_context(world)
        }

        fn frame_costs(&self) -> Option<pyreal_client_runtime::report::PresentCosts> {
            self.device.present_costs()
        }

        fn apply_device_preference_requests(
            &mut self,
            requests: Vec<pyreal_client_contract::UiRequest>,
        ) -> Vec<pyreal_client_contract::UiRequest> {
            self.device.apply_device_preference_requests(requests)
        }

        fn apply_render_preference_requests(
            &mut self,
            requests: Vec<pyreal_client_contract::UiRequest>,
        ) -> Vec<pyreal_client_contract::UiRequest> {
            self.device.apply_render_preference_requests(requests)
        }

        fn load_first_pixel_scene(
            &mut self,
            assets: &dyn AssetSource,
            id: DataId,
        ) -> Result<(), PresentError> {
            self.device.load_first_pixel_scene(assets, id).map_err(err)
        }

        fn load_world(
            &mut self,
            store: &std::sync::Arc<pyreal_dat::RetailDatStore>,
            cfg: pyreal_client_runtime::scene::SceneConfig,
            world: &mut Option<WorldState>,
        ) -> Result<(), pyreal_world_data::landblock::WorldError> {
            self.device.load_world(store, cfg, world)
        }

        fn release_world(&mut self, world: &mut Option<WorldState>) -> u32 {
            self.device.release_world(world)
        }

        fn stream_world(
            &mut self,
            store: &pyreal_dat::RetailDatStore,
            world: Option<&mut WorldState>,
        ) -> Result<(), pyreal_world_data::landblock::WorldError> {
            self.device.sim.stream_world(store, world)
        }

        fn sync_objects(
            &mut self,
            store: &std::sync::Arc<pyreal_dat::RetailDatStore>,
            stream: &mut pyreal_client_runtime::objects::ObjectStream,
            world: Option<&mut WorldState>,
        ) -> Result<(), pyreal_world_data::landblock::WorldError> {
            let r = self.device.sim.sync_objects(store, stream, world);
            self.device.note_objects(stream);
            r
        }

        fn prepare_object_dispatch(
            &mut self,
            store: &std::sync::Arc<pyreal_dat::RetailDatStore>,
            stream: &mut pyreal_client_runtime::objects::ObjectStream,
            world: Option<&mut WorldState>,
        ) -> Result<(), pyreal_world_data::landblock::WorldError> {
            self.device
                .sim
                .prepare_object_dispatch(store, stream, world)
        }

        fn update_render_preferences(
            &mut self,
            store: &pyreal_dat::RetailDatStore,
            world: Option<&mut WorldState>,
        ) -> Result<
            pyreal_client_runtime::frame_events::RenderPrefWork,
            pyreal_world_data::landblock::WorldError,
        > {
            let work = self.device.sim.update_render_preferences(store, world);
            self.device.update_render_preferences();
            work
        }

        fn set_world_view_state(&mut self, hidden: bool, view_distance: Option<f32>) {
            self.device.set_world_view_state(hidden, view_distance);
        }

        fn set_portal_look(&mut self, look: Option<pyreal_client_contract::teleport::PortalLook>) {
            self.device.set_portal_look(look);
        }

        fn set_backdrop(&mut self, on: bool) {
            self.device.set_backdrop(on);
        }

        fn use_data_files(&mut self, store: &std::sync::Arc<pyreal_dat::RetailDatStore>) {
            self.device.use_data_files(store);
        }

        fn draws_portal_space(&self) -> bool {
            self.device.draws_portal_space()
        }

        fn loading_destination(&self) -> bool {
            self.device.loading_destination()
        }

        fn preparing_shaders(&self) -> bool {
            self.device.preparing_shaders()
        }

        fn paces_teleport(&self) -> bool {
            self.device.windowed()
        }

        fn offer_object_identity(
            &mut self,
            identity: std::sync::Arc<pyreal_client_runtime::object_identity::ObjectIdentity>,
        ) {
            self.device.sim.offer_object_identity(identity);
        }

        fn open_art_pack(&mut self, path: &std::path::Path) {
            self.device.open_art_pack(path);
        }

        fn open_base_pack(&mut self, path: &std::path::Path) {
            self.device.open_base_pack(path);
        }

        fn use_saved_render_preferences(
            &mut self,
            prefs: pyreal_client_runtime::render_prefs::RenderPreferences,
        ) {
            self.device.use_saved_render_preferences(prefs);
        }

        fn overlay_upload(
            &mut self,
            texture: OverlayTexture,
            data: &pyreal_primitives::TextureData,
        ) -> Result<(), PresentError> {
            self.device.overlay_upload(texture, data);
            Ok(())
        }

        fn overlay_release(&mut self, texture: OverlayTexture) -> OverlayReleased {
            self.device.overlay_release(texture)
        }

        fn draw_overlay(&mut self, items: &[OverlayItem]) -> Result<(), PresentError> {
            self.device.draw_overlay(items);
            Ok(())
        }

        fn set_ground_markers(
            &mut self,
            markers: &[pyreal_client_contract::overlay::GroundMarker],
        ) {
            self.device.set_ground_markers(markers);
        }

        fn preview_ensure(
            &mut self,
            id: PreviewId,
            assets: &std::sync::Arc<pyreal_world_data::anim_assets::DatAnimAssets>,
        ) -> bool {
            self.device.previews.ensure(id, assets)
        }

        fn preview_set_light(
            &mut self,
            id: PreviewId,
            light: pyreal_client_contract::overlay::PreviewLight,
            intensity: f32,
            direction: pyreal_primitives::Vec3,
        ) {
            use pyreal_client_contract::overlay::PreviewLight as L;
            use pyreal_world_render::lighting::LightType;
            let light = match light {
                L::Point => LightType::Point,
                L::Directional => LightType::Directional,
                L::Spot => LightType::Spot,
            };
            if let Some(s) = self.device.previews.get_mut(id) {
                s.mode.set_light(light, intensity, direction);
            }
        }

        fn preview_use_sharp_mode(&mut self, id: PreviewId) {
            if let Some(s) = self.device.previews.get_mut(id) {
                s.mode.use_sharp_mode();
            }
        }

        fn preview_use_world_fov(&mut self, id: PreviewId) {
            if let Some(s) = self.device.previews.get_mut(id) {
                s.mode.use_world_fov();
            }
        }

        fn preview_set_fov(&mut self, id: PreviewId, radians: f32) {
            if let Some(s) = self.device.previews.get_mut(id) {
                s.mode.set_fov(radians);
            }
        }

        fn preview_set_camera_position(
            &mut self,
            id: PreviewId,
            position: pyreal_primitives::Vec3,
        ) {
            if let Some(s) = self.device.previews.get_mut(id) {
                s.mode.set_camera_position(position);
            }
        }

        fn preview_set_camera_direction(
            &mut self,
            id: PreviewId,
            direction: pyreal_primitives::Vec3,
        ) {
            if let Some(s) = self.device.previews.get_mut(id) {
                s.mode.set_camera_direction(direction);
            }
        }

        fn preview_set_camera_direction_degrees(
            &mut self,
            id: PreviewId,
            degrees: pyreal_primitives::Vec3,
        ) {
            if let Some(s) = self.device.previews.get_mut(id) {
                s.mode.set_camera_direction_degrees(degrees);
            }
        }

        fn preview_remove_all_objects(&mut self, id: PreviewId) {
            if let Some(s) = self.device.previews.get_mut(id) {
                s.remove_all_objects();
            }
        }

        fn preview_add_object(
            &mut self,
            id: PreviewId,
            store: &pyreal_dat::RetailDatStore,
            setup: DataId,
        ) -> Result<Option<usize>, PresentError> {
            let _ = store;
            Ok(self
                .device
                .previews
                .get_mut(id)
                .and_then(|s| s.add_object(setup, None)))
        }

        fn preview_add_object_dressed(
            &mut self,
            id: PreviewId,
            store: &pyreal_dat::RetailDatStore,
            setup: DataId,
            objdesc: Option<&pyreal_animation::parts::ObjDesc>,
        ) -> Result<Option<usize>, PresentError> {
            let _ = store;
            Ok(self
                .device
                .previews
                .get_mut(id)
                .and_then(|s| s.add_object(setup, objdesc)))
        }

        fn preview_set_heading(&mut self, id: PreviewId, index: usize, degrees: f32) {
            if let Some(s) = self.device.previews.get_mut(id) {
                s.set_heading(index, degrees);
            }
        }

        fn preview_set_scale(&mut self, id: PreviewId, index: usize, scale: f32) {
            if let Some(s) = self.device.previews.get_mut(id) {
                s.set_scale(index, scale);
            }
        }

        fn preview_set_sequence_animation(
            &mut self,
            id: PreviewId,
            index: usize,
            animation: DataId,
            clear: bool,
            low_frame: i32,
            framerate: f32,
        ) -> bool {
            self.device.previews.get_mut(id).is_some_and(|s| {
                s.set_sequence_animation(index, animation, clear, low_frame, framerate)
            })
        }

        fn preview_clear_sequence_anims(&mut self, id: PreviewId, index: usize) {
            if let Some(s) = self.device.previews.get_mut(id) {
                s.clear_sequence_anims(index);
            }
        }

        fn preview_has_anims(&self, id: PreviewId, index: usize) -> bool {
            self.device
                .previews
                .get(id)
                .is_some_and(|s| s.has_anims(index))
        }

        fn preview_use_time(&mut self, id: PreviewId, dt: f64) {
            if let Some(s) = self.device.previews.get_mut(id) {
                s.use_time(dt);
            }
        }

        fn preview_curr_frame_number(&self, id: PreviewId, index: usize) -> Option<u32> {
            self.device
                .previews
                .get(id)
                .map(|s| s.curr_frame_number(index))
        }

        fn preview_object_bounding_box(
            &self,
            id: PreviewId,
            index: usize,
            store: &pyreal_dat::RetailDatStore,
        ) -> Option<pyreal_physics::geom::BBox> {
            self.device
                .previews
                .get(id)
                .and_then(|s| s.object(index))
                .map(|o| pyreal_scene::preview::parts_bounding_box(&o.body.part_array, store))
        }

        fn preview_part_array_mut(
            &mut self,
            id: PreviewId,
            index: usize,
        ) -> Option<&mut pyreal_animation::parts::PartArray> {
            self.device
                .previews
                .get_mut(id)
                .and_then(|s| s.part_array_mut(index))
        }

        fn scene<'a>(
            &'a self,
            world: Option<&'a WorldState>,
        ) -> Option<Box<dyn pyreal_client_runtime::present::Scene + 'a>> {
            self.device.scene(world)
        }

        fn scene_mut<'a>(
            &'a mut self,
            world: Option<&'a mut WorldState>,
        ) -> Option<Box<dyn pyreal_client_runtime::present::SceneMut + 'a>> {
            self.device.scene_mut(world)
        }
    }

    impl crate::present::ClientPresentation for Renderer {
        fn set_world_marks(&mut self, marks: Vec<crate::present::WorldMark>) {
            self.device.world_marks = marks;
        }

        /// The engine keeps its depth to itself: every label is shown.
        fn depth_probes_supported(&self) -> bool {
            false
        }

        fn request_depth_probes(&mut self, _tag: u64, _pixels: Vec<(u32, u32)>) {}

        fn take_depth_probes(&mut self) -> Option<(u64, Vec<f32>)> {
            None
        }

        fn target_projection(
            &self,
            id: pyreal_primitives::ObjectId,
            world: Option<&WorldState>,
        ) -> Option<pyreal_client_contract::target::Projection> {
            self.device.target_projection(id, world)
        }

        fn target_origin(
            &self,
            id: pyreal_primitives::ObjectId,
            world: Option<&WorldState>,
        ) -> Option<(i32, i32)> {
            self.device.target_origin(id, world)
        }

        fn target_top(
            &self,
            id: pyreal_primitives::ObjectId,
            world: Option<&WorldState>,
        ) -> Option<(f32, f32, Option<f32>)> {
            self.device.target_top(id, world)
        }
    }
}
