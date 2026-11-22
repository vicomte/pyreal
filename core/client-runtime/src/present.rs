//! The presentation seam: the device the frame loop draws through (`Presentation`), the
//! presentation a headless run has (`NullPresentation`), and the drawn world as the frame reads
//! and writes it (`Scene`, `SceneMut`).
//!
//! `Scene` is what the drawn world looks like from outside the device: the body, the census
//! lines, the sound listener, the environment options. `Presentation` is the device itself.
//!
//! Backend-neutral by construction: no method, type or argument here
//! names a graphics API.

use pyreal_client_contract::overlay::{
    OverlayItem, OverlayReleased, OverlayTexture, PreviewLight, PreviewSpace,
};
use pyreal_primitives::{CellId, DataId, Frame, Inset, ObjectId, Viewport};

/// The scene counters the three census lines in `App` print — `ViewerBlockReport`,
/// `ObjectReport` and `report_scene`.
///
/// Plain counters copied from the presentation, without exposing renderer-owned state to the
/// runtime. These ten fields describe the meshed world and server-object geometry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneCensus {
    pub blocks_meshed: usize,
    pub terrain_surfaces: usize,
    pub scenery_objects: usize,
    pub buildings: usize,
    pub static_objects: usize,
    pub object_triangles: usize,
    pub server_objects_animated: usize,
    pub server_objects_held: usize,
    pub server_object_setups: usize,
    pub server_object_triangles: usize,
}

/// The drawn world, seen from outside the device.
///
/// Implemented by the scene crate's world and by the device-free simulation. Everything here is either
/// plain data or a type from a crate that has no device in it (`pyreal_animation`, `pyreal_audio`,
/// `pyreal_assets`, `pyreal_physics`, `crate::character`).
pub trait Scene: std::fmt::Debug {
    /// The world state this scene draws: the application's, beside the drawing half.
    fn world(&self) -> &crate::world_state::WorldState;

    // ---------------------------------------------------------------------------------------
    // the local body
    // ---------------------------------------------------------------------------------------

    /// The local body, or `None` before `0xF746` has stood one up.
    fn character(&self) -> Option<&crate::character::Character>;

    // ---------------------------------------------------------------------------------------
    // what the census lines print
    // ---------------------------------------------------------------------------------------

    fn census(&self) -> SceneCensus;
    /// The world-object store's drawable count.
    fn server_object_count(&self) -> usize;
    /// The viewer's cell, which is what tells an indoor frame from an outdoor one.
    fn viewer_cell_id(&self) -> Option<CellId>;
    /// The landscape window's centre.
    fn viewer_block(&self) -> Option<(i32, i32)>;
    /// Whether a landblock within the player's reach is still waiting to be built. The portal
    /// tunnel is held while this is true. A scene that builds everything at once never is.
    fn loading_near_viewer(&self) -> bool {
        false
    }
    /// How many landblocks are still queued to be built.
    fn blocks_pending(&self) -> usize {
        0
    }
    /// `WorldScene::env_cell_counts().1` — interior draw batches in the viewer's own cell.
    fn interior_batches(&self) -> usize;

    // ---------------------------------------------------------------------------------------
    // reads the UI and the HUD make
    // ---------------------------------------------------------------------------------------

    /// The map panel's displayed date and time pair.
    fn game_date_time(&self) -> Option<(String, String)>;
    /// The render preferences held by this scene.
    fn render_preferences(&self) -> crate::render_prefs::RenderPreferences;
    /// A server object's frame, for the sound placement and the selection geometry.
    fn server_object_frame(&self, id: ObjectId) -> Option<Frame>;
    /// The 3D viewport and the projection the frame is drawn with -- the smart box element's
    /// rectangle, not the window's. `(0, 0)` asks for the whole back buffer.
    fn effective_viewport(&self, width: u32, height: u32) -> Viewport;
    fn view_params(&self, width: u32, height: u32) -> pyreal_client_contract::camera::ViewParams;
    /// The scene as the pick sweep reads it (`crate::pick::PickScene`).
    fn as_pick_scene(&self) -> &dyn crate::pick::PickScene;

    // ---------------------------------------------------------------------------------------
    // the draw's half of the selected-parts seam
    // ---------------------------------------------------------------------------------------

    fn take_selected_part_drawn(&self) -> bool;
    /// Set the object whose selected parts the draw tracks.
    fn set_selected_object_id(&self, id: Option<ObjectId>);

    // ---------------------------------------------------------------------------------------
    // the sound system's reads
    // ---------------------------------------------------------------------------------------

    fn listener(&self) -> pyreal_audio::Listener;
    fn terrain_neighbourhood(&self) -> pyreal_audio::TerrainNeighbourhood;
    fn character_sound_table(&self) -> Option<DataId>;
    /// The weather's own sound as the presentation last drew the weather.
    fn weather_sound(&self) -> pyreal_audio::WeatherSound;
    /// The space the listener stands in, for the enhanced sound's echo.
    fn sound_room(&self) -> pyreal_audio::Room;
    fn region(&self) -> &pyreal_assets::Region;
}

/// The writes on the drawn world, split off `Scene`, which is the reads
/// alone: the application holds the world state and the presentation the drawing half, so a
/// read-only view of the two (one shared borrow of each) can be a `Scene` without being able to
/// write, and only a view holding both mutably is a `SceneMut`.
pub trait SceneMut: Scene {
    /// The world state, writable. The writes that touch nothing but the
    /// simulation are its own methods (`look`, `follow_character_now`, the movement dispatch and
    /// its latch, object geometry and lighting, the motion-table swap, the body itself); what
    /// stays on this trait is what also reaches the drawing half.
    fn world_mut(&mut self) -> &mut crate::world_state::WorldState;
    fn take_sound_events(&mut self) -> Vec<crate::audio::SoundTrigger>;

    // ---------------------------------------------------------------------------------------
    // writes
    // ---------------------------------------------------------------------------------------

    /// The smart-box simulation step.
    fn update(
        &mut self,
        input: crate::camera::CameraInput,
        character: crate::character::CharacterInput,
        now: pyreal_primitives::LocalTime,
        dt: f32,
    );
    /// The swept camera's frame becomes the render camera.
    fn apply_camera_translucency(&mut self);
    /// The landscape window's release of every block the player teleported away from.
    fn release_landscape_for_teleport(&mut self, destination: pyreal_primitives::LandblockId);
    /// The barber panel's two local effects.
    fn replace_player_particle_script(&mut self, script: DataId) -> bool;
    /// The player system's process-owned environment presets.
    fn set_environment_override_state(
        &mut self,
        state: crate::environment::EnvironmentOverrideState,
    );
    fn sync_environment_override_flags(&mut self);
    /// The player module's three environment-change arms.
    fn set_always_daylight(&mut self, on: bool) -> bool;
    fn set_weather_enabled(&mut self, on: bool);
    fn set_world_fog(&mut self, on: bool) -> bool;
}

/// A presentation's failure, as the application reports it: the device's own message.
///
/// The trait returns this rather than a renderer's error type, so the application loop names no
/// renderer; a device presentation converts its errors on the way out, and the message the
/// application prints (`Display`) is the device error's own text, unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentError(pub String);

impl std::fmt::Display for PresentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PresentError {}

/// The device, and every service the frame loop asks of it: the four device steps of the
/// documented frame order (`PrepareDevice`, `BeginFrame`, the world draw, `PresentFrame` -- see
/// [`crate::frame::FrameStep`]), the back buffer and its presentation parameters, the screenshot,
/// the preference names the device owns, and the world scene's lifetime.
///
/// What draws over the world -- the UI overlay, its textures, the preview spaces -- is the front
/// end's, and a front end that has one extends this trait with it.
///
/// Backend-neutral by construction: no method, type or argument here names a graphics API.
pub trait Presentation: std::fmt::Debug {
    /// For the accessors that hand the concrete device back to a front end or a test.
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    // ---------------------------------------------------------------------------------------
    // the frame bracket -- `FrameStep::PrepareDevice` .. `FrameStep::PresentFrame`
    // ---------------------------------------------------------------------------------------

    fn prepare_graphics_device(&mut self);
    fn start_frame(&mut self) -> Result<(), PresentError>;
    fn set_game_viewport(&mut self, viewport: Option<Viewport>);
    /// Draw the world half of the frame, from the world state the application owns.
    fn draw_scene(
        &mut self,
        world: Option<&crate::world_state::WorldState>,
    ) -> Result<(), PresentError>;
    fn end_frame(&mut self) -> Result<(), PresentError>;
    /// Whether the last frame found its window can't be seen, so nothing waited for the display.
    fn window_hidden(&self) -> bool {
        false
    }
    /// The last release of the shutdown sequence.
    fn wait_idle(&mut self) -> Result<(), PresentError>;

    // ---------------------------------------------------------------------------------------
    // the back buffer and the presentation parameters
    // ---------------------------------------------------------------------------------------

    fn size(&self) -> (u32, u32);
    fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError>;
    fn set_presentation_sync(&mut self, full_screen: bool, sync_to_refresh: bool);
    /// Draw the frames that follow into part of the back buffer, as a screen of their own: for a
    /// host that shows the game inside a view of its own window. While a region is set,
    /// [`Self::size`] and [`Self::resize`] are the region's screen's, and the back buffer stays
    /// the window's. `None` draws over the whole back buffer again. A presentation with no
    /// device has nothing to place.
    fn set_inset(&mut self, _region: Option<Inset>) {}
    /// The part of the back buffer the frames draw into, while one is set.
    fn inset(&self) -> Option<Inset> {
        None
    }
    /// Save the current back buffer as a PNG.
    ///
    /// # Errors
    /// Whatever the device answers; a presentation with no device answers `Ok(())`.
    fn capture_png(&mut self, path: &std::path::Path) -> Result<(), PresentError>;

    // ---------------------------------------------------------------------------------------
    // the preference names the device owns
    // ---------------------------------------------------------------------------------------

    /// Where the experimental rendering effects stand on this device, for the options page; a
    /// presentation without them says they are not built.
    fn hifi_availability(&self) -> pyreal_client_contract::options::fidelity::Availability {
        pyreal_client_contract::options::fidelity::Availability::default()
    }
    /// The graphics card drawing to the window, for the quality preset chosen for it; `None`
    /// without a window.
    fn graphics_adapter(&self) -> Option<pyreal_client_contract::options::auto_quality::Adapter> {
        None
    }
    /// The share of the frame's pixels the enhanced graphics draw the world at now, percent,
    /// while they draw: what a Dynamic render scale has moved it to.
    fn render_scale_in_use(&self) -> Option<u32> {
        None
    }
    /// The device-owned `Render.*` names alone.
    fn apply_device_preference_requests(
        &mut self,
        requests: Vec<pyreal_client_contract::UiRequest>,
    ) -> Vec<pyreal_client_contract::UiRequest>;
    /// The device-owned names **and** the scene-owned ones.
    fn apply_render_preference_requests(
        &mut self,
        requests: Vec<pyreal_client_contract::UiRequest>,
    ) -> Vec<pyreal_client_contract::UiRequest>;

    // ---------------------------------------------------------------------------------------
    // the world scene's lifetime
    // ---------------------------------------------------------------------------------------

    /// Stand one decoded surface up as the whole scene.
    ///
    /// # Errors
    /// When the surface is missing or will not decode.
    fn load_first_pixel_scene(
        &mut self,
        assets: &dyn pyreal_primitives::AssetSource,
        id: DataId,
    ) -> Result<(), PresentError>;
    /// Build the world around a landblock.
    ///
    /// # Errors
    /// When the region, the landblock or a device resource is unavailable.
    ///
    /// The world state it builds is handed back in `world`, which the application owns; the
    /// drawing half stays here. A presentation with no device leaves `world` as it was.
    fn load_world(
        &mut self,
        store: &std::sync::Arc<pyreal_dat::RetailDatStore>,
        cfg: crate::scene::SceneConfig,
        world: &mut Option<crate::world_state::WorldState>,
    ) -> Result<(), pyreal_world_data::landblock::WorldError>;
    /// Returns the number of textures released. The world state goes with the drawing half: a
    /// presentation that had a scene clears `world`.
    fn release_world(&mut self, world: &mut Option<crate::world_state::WorldState>) -> u32;
    /// Build the landblocks the window scrolled onto this frame.
    ///
    /// # Errors
    /// When a landblock will not build.
    fn stream_world(
        &mut self,
        store: &pyreal_dat::RetailDatStore,
        world: Option<&mut crate::world_state::WorldState>,
    ) -> Result<(), pyreal_world_data::landblock::WorldError>;
    /// # Errors
    /// When an object's geometry will not build.
    fn sync_objects(
        &mut self,
        store: &std::sync::Arc<pyreal_dat::RetailDatStore>,
        stream: &mut crate::objects::ObjectStream,
        world: Option<&mut crate::world_state::WorldState>,
    ) -> Result<(), pyreal_world_data::landblock::WorldError>;
    /// # Errors
    /// As [`Presentation::sync_objects`].
    fn prepare_object_dispatch(
        &mut self,
        store: &std::sync::Arc<pyreal_dat::RetailDatStore>,
        stream: &mut crate::objects::ObjectStream,
        world: Option<&mut crate::world_state::WorldState>,
    ) -> Result<(), pyreal_world_data::landblock::WorldError>;
    /// Poll changed render preferences and apply any requested scene rebuilds.
    ///
    /// # Errors
    /// When the rebuild a changed preference asked for fails.
    fn update_render_preferences(
        &mut self,
        store: &pyreal_dat::RetailDatStore,
        world: Option<&mut crate::world_state::WorldState>,
    ) -> Result<(), pyreal_world_data::landblock::WorldError>;
    /// Set whether the world is hidden and the optional view-distance override.
    fn set_world_view_state(&mut self, hidden: bool, view_distance: Option<f32>);
    /// Whether the backdrop of the screens before the world is drawn, every frame.
    fn set_backdrop(&mut self, _on: bool) {}
    /// The data files the preview spaces draw from while no world is built.
    fn use_data_files(&mut self, _store: &std::sync::Arc<pyreal_dat::RetailDatStore>) {}
    /// The teleport's moment, every frame, for a presentation that draws its own portal space.
    fn set_portal_look(&mut self, _look: Option<pyreal_client_contract::teleport::PortalLook>) {}
    /// Whether this presentation draws portal space itself, in place of the swirl's preview.
    fn draws_portal_space(&self) -> bool {
        false
    }
    /// Whether a teleport's messages wait for portal space to be on screen and are then taken a
    /// few at a time, so the destination loads behind it. A presentation drawn to a window does.
    fn paces_teleport(&self) -> bool {
        false
    }
    /// Whether a teleport's destination is still being placed behind portal space. The tunnel is
    /// held while this is true.
    fn loading_destination(&self) -> bool {
        false
    }
    /// Whether the device is still preparing its shaders (a driver whose cache is cold compiling
    /// them on a thread of their own) and has been for long enough that the screens before the
    /// world should say so. A presentation with no device never is.
    fn preparing_shaders(&self) -> bool {
        false
    }
    /// The object identity verdicts the application worked out for its store
    /// ([`crate::object_identity::IdentityBuild`]), for the world loaded now and every one after:
    /// the other era's look is drawn with them. A presentation that draws no world drops them.
    fn offer_object_identity(
        &mut self,
        identity: std::sync::Arc<crate::object_identity::ObjectIdentity>,
    ) {
        let _ = identity;
    }
    /// The art pack the client was installed with ([`crate::config::Config::art_pack`]): a
    /// presentation that draws it opens it, and draws its models in place of the objects they
    /// replace in the world loaded now and every one after. A presentation that draws none
    /// ignores it.
    fn open_art_pack(&mut self, path: &std::path::Path) {
        let _ = path;
    }
    /// The base art pack built into the client ([`crate::config::Config::base_pack`]): drawn
    /// under the chosen pack, and alone when none is chosen. A presentation that draws none
    /// ignores it.
    fn open_base_pack(&mut self, path: &std::path::Path) {
        let _ = path;
    }
    /// The player's saved render preferences, before any world is loaded: a presentation that
    /// draws chooses its window's output by them. A presentation that draws none ignores them.
    fn use_saved_render_preferences(&mut self, prefs: crate::render_prefs::RenderPreferences) {
        let _ = prefs;
    }

    // ---------------------------------------------------------------------------------------
    // what a report and a performance capture say about the frame
    // ---------------------------------------------------------------------------------------

    /// How the frame is drawn, for `@report` and `@perfcapture`: the renderer and the adapter,
    /// Better Lighting and its options, the art pack and the drawn ground's knobs, and where the
    /// frame is drawn from for the camera in `world`. A presentation with no device knows none of
    /// it.
    fn render_context(
        &self,
        world: Option<&crate::world_state::WorldState>,
    ) -> crate::report::RenderContext {
        let _ = world;
        crate::report::RenderContext::default()
    }
    /// What the presentation's own parts of the last frame took, for `@perfcapture`: asked only
    /// while a capture records. `None` from a presentation that does not time them.
    fn frame_costs(&self) -> Option<crate::report::PresentCosts> {
        None
    }

    // ---------------------------------------------------------------------------------------
    // the overlay, any UI's -- see `pyreal_client_contract::overlay`
    // ---------------------------------------------------------------------------------------

    /// Upload `data` under `texture`, outside the frame bracket. An `Image` or `Glyphs` key that
    /// is already resident is a hit and uploads nothing; a resident `Local` key is replaced.
    ///
    /// # Errors
    /// Whatever the device answers.
    fn overlay_upload(
        &mut self,
        texture: OverlayTexture,
        data: &pyreal_primitives::TextureData,
    ) -> Result<(), PresentError>;
    /// Release the texture uploaded under `texture`.
    fn overlay_release(&mut self, texture: OverlayTexture) -> OverlayReleased;
    /// Draw the overlay over the finished world, inside the frame bracket, in order. A batch
    /// naming a texture that is not resident draws nothing.
    ///
    /// # Errors
    /// Whatever the device answers; the rest of the list is not drawn.
    fn draw_overlay(&mut self, items: &[OverlayItem]) -> Result<(), PresentError>;
    /// Lay `marker` on the ground under its object for the frames until the next call; `None`
    /// takes it away. A presentation with no world draws nothing.
    fn set_ground_markers(&mut self, markers: &[pyreal_client_contract::overlay::GroundMarker]) {
        let _ = markers;
    }
    /// Lay the waypoint's ring round the player for the frames until the next call; `None` takes
    /// it away. A presentation with no world draws nothing.
    fn set_waypoint_ring(&mut self, ring: Option<pyreal_client_contract::overlay::WaypointRing>) {
        let _ = ring;
    }

    // ---------------------------------------------------------------------------------------
    // the preview spaces -- small 3D scenes a UI draws into its overlay
    // ---------------------------------------------------------------------------------------

    /// Build the space if it does not exist. `true` when this call created it.
    fn preview_ensure(
        &mut self,
        id: PreviewSpace,
        assets: &std::sync::Arc<pyreal_world_data::anim_assets::DatAnimAssets>,
    ) -> bool;
    fn preview_set_light(
        &mut self,
        id: PreviewSpace,
        light: PreviewLight,
        intensity: f32,
        direction: pyreal_primitives::Vec3,
    );
    fn preview_use_world_fov(&mut self, id: PreviewSpace);
    /// Give the space a field of view of its own, `radians` high, whatever the world's camera
    /// does: a front end whose previews have their own lens.
    fn preview_set_fov(&mut self, id: PreviewSpace, radians: f32);
    fn preview_set_camera_position(&mut self, id: PreviewSpace, position: pyreal_primitives::Vec3);
    fn preview_set_camera_direction(
        &mut self,
        id: PreviewSpace,
        direction: pyreal_primitives::Vec3,
    );
    fn preview_set_camera_direction_degrees(
        &mut self,
        id: PreviewSpace,
        degrees: pyreal_primitives::Vec3,
    );
    fn preview_remove_all_objects(&mut self, id: PreviewSpace);
    /// # Errors
    /// Whatever the device answers while baking the object's meshes.
    fn preview_add_object(
        &mut self,
        id: PreviewSpace,
        store: &pyreal_dat::RetailDatStore,
        setup: DataId,
    ) -> Result<Option<usize>, PresentError>;
    /// # Errors
    /// As `Presentation::preview_add_object`.
    fn preview_add_object_dressed(
        &mut self,
        id: PreviewSpace,
        store: &pyreal_dat::RetailDatStore,
        setup: DataId,
        objdesc: Option<&pyreal_animation::parts::ObjDesc>,
    ) -> Result<Option<usize>, PresentError>;
    fn preview_set_heading(&mut self, id: PreviewSpace, index: usize, degrees: f32);
    fn preview_set_scale(&mut self, id: PreviewSpace, index: usize, scale: f32);
    fn preview_set_sequence_animation(
        &mut self,
        id: PreviewSpace,
        index: usize,
        animation: DataId,
        clear: bool,
        low_frame: i32,
        framerate: f32,
    ) -> bool;
    fn preview_clear_sequence_anims(&mut self, id: PreviewSpace, index: usize);
    fn preview_has_anims(&self, id: PreviewSpace, index: usize) -> bool;
    fn preview_use_time(&mut self, id: PreviewSpace, dt: f64);
    /// The current frame number of preview object `index`.
    fn preview_curr_frame_number(&self, id: PreviewSpace, index: usize) -> Option<u32>;

    /// The drawn world as one read-only view: the application's world state beside this
    /// presentation's drawing half. `None` when either is missing.
    fn scene<'a>(
        &'a self,
        world: Option<&'a crate::world_state::WorldState>,
    ) -> Option<Box<dyn Scene + 'a>>;
    /// …and the same view with both halves writable.
    fn scene_mut<'a>(
        &'a mut self,
        world: Option<&'a mut crate::world_state::WorldState>,
    ) -> Option<Box<dyn SceneMut + 'a>>;
}

/// How many times a `NullPresentation` was asked for each device step.
///
/// The headless path performs no work, but it is still *asked*, and the fourteen `FrameStep`s are
/// still recorded in order. These counters are what lets a test say the frame reached the device
/// steps rather than only that it did not crash. The overlay's counters are counted by the front
/// end that draws one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NullPresentationCounts {
    pub prepare_graphics_device: u64,
    pub start_frame: u64,
    pub set_game_viewport: u64,
    pub draw_scene: u64,
    pub end_frame: u64,
    pub wait_idle: u64,
    pub resize: u64,
    pub capture_png: u64,
    pub load_world: u64,
    pub release_world: u64,
    pub stream_world: u64,
    pub sync_objects: u64,
    pub prepare_object_dispatch: u64,
    pub update_render_preferences: u64,
    pub preview_calls: u64,
    pub overlay_uploads: u64,
    pub overlay_releases: u64,
    pub draw_overlay: u64,
    /// How many marks the world was last given to draw (a front end's own, such as plugins').
    pub world_marks: u64,
}

/// The presentation a headless `App` gets: it draws nothing and holds no scene, and records what
/// it was asked to do.
///
/// It is deliberately **not** a silent stub. `NullPresentation::size` answers the extent it was
/// constructed with, so a UI lays out against a real display rectangle exactly as it does with a
/// device, and every device step increments its counter.
#[derive(Debug, Clone, Default)]
pub struct NullPresentation {
    size: (u32, u32),
    counts: NullPresentationCounts,
    ground_markers: Vec<pyreal_client_contract::overlay::GroundMarker>,
    waypoint_ring: Option<pyreal_client_contract::overlay::WaypointRing>,
    /// Told when this presentation is dropped.
    #[cfg(test)]
    pub(crate) drop_log: Option<std::rc::Rc<std::cell::RefCell<Vec<&'static str>>>>,
    /// The graphics card it answers with.
    #[cfg(test)]
    pub(crate) graphics_adapter: Option<pyreal_client_contract::options::auto_quality::Adapter>,
}

#[cfg(test)]
impl Drop for NullPresentation {
    fn drop(&mut self) {
        if let Some(log) = &self.drop_log {
            log.borrow_mut().push("device");
        }
    }
}

impl NullPresentation {
    /// A presentation whose back buffer is `width` x `height` and which draws nothing.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            size: (width, height),
            counts: NullPresentationCounts::default(),
            ground_markers: Vec::new(),
            waypoint_ring: None,
            #[cfg(test)]
            drop_log: None,
            #[cfg(test)]
            graphics_adapter: None,
        }
    }

    /// What this presentation has been asked to do.
    #[must_use]
    pub const fn counts(&self) -> NullPresentationCounts {
        self.counts
    }

    /// The counters, for a front end that asks this presentation for its overlay too.
    pub fn counts_mut(&mut self) -> &mut NullPresentationCounts {
        &mut self.counts
    }

    /// The ground markers last asked for, as a world would draw them.
    #[must_use]
    pub fn ground_markers(&self) -> &[pyreal_client_contract::overlay::GroundMarker] {
        &self.ground_markers
    }

    /// The waypoint's ring last asked for, as a world would draw it.
    #[must_use]
    pub const fn waypoint_ring(&self) -> Option<pyreal_client_contract::overlay::WaypointRing> {
        self.waypoint_ring
    }
}

impl Presentation for NullPresentation {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    #[cfg(test)]
    fn graphics_adapter(&self) -> Option<pyreal_client_contract::options::auto_quality::Adapter> {
        self.graphics_adapter.clone()
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn prepare_graphics_device(&mut self) {
        self.counts.prepare_graphics_device += 1;
    }
    fn start_frame(&mut self) -> Result<(), PresentError> {
        self.counts.start_frame += 1;
        Ok(())
    }
    fn set_game_viewport(&mut self, _viewport: Option<Viewport>) {
        self.counts.set_game_viewport += 1;
    }
    fn draw_scene(
        &mut self,
        _world: Option<&crate::world_state::WorldState>,
    ) -> Result<(), PresentError> {
        self.counts.draw_scene += 1;
        Ok(())
    }
    fn end_frame(&mut self) -> Result<(), PresentError> {
        self.counts.end_frame += 1;
        Ok(())
    }
    fn wait_idle(&mut self) -> Result<(), PresentError> {
        self.counts.wait_idle += 1;
        Ok(())
    }

    fn size(&self) -> (u32, u32) {
        self.size
    }
    fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError> {
        self.counts.resize += 1;
        self.size = (width, height);
        Ok(())
    }
    fn set_presentation_sync(&mut self, _full_screen: bool, _sync_to_refresh: bool) {}
    fn capture_png(&mut self, _path: &std::path::Path) -> Result<(), PresentError> {
        self.counts.capture_png += 1;
        Ok(())
    }

    fn overlay_upload(
        &mut self,
        _texture: OverlayTexture,
        _data: &pyreal_primitives::TextureData,
    ) -> Result<(), PresentError> {
        self.counts.overlay_uploads += 1;
        Ok(())
    }
    fn overlay_release(&mut self, _texture: OverlayTexture) -> OverlayReleased {
        self.counts.overlay_releases += 1;
        OverlayReleased::Absent
    }
    fn draw_overlay(&mut self, _items: &[OverlayItem]) -> Result<(), PresentError> {
        self.counts.draw_overlay += 1;
        Ok(())
    }
    fn set_ground_markers(&mut self, markers: &[pyreal_client_contract::overlay::GroundMarker]) {
        self.ground_markers = markers.to_vec();
    }
    fn set_waypoint_ring(&mut self, ring: Option<pyreal_client_contract::overlay::WaypointRing>) {
        self.waypoint_ring = ring;
    }

    fn preview_ensure(
        &mut self,
        _id: PreviewSpace,
        _assets: &std::sync::Arc<pyreal_world_data::anim_assets::DatAnimAssets>,
    ) -> bool {
        self.counts.preview_calls += 1;
        false
    }
    fn preview_set_light(
        &mut self,
        _id: PreviewSpace,
        _light: PreviewLight,
        _intensity: f32,
        _direction: pyreal_primitives::Vec3,
    ) {
        self.counts.preview_calls += 1;
    }
    fn preview_use_world_fov(&mut self, _id: PreviewSpace) {
        self.counts.preview_calls += 1;
    }
    fn preview_set_fov(&mut self, _id: PreviewSpace, _radians: f32) {
        self.counts.preview_calls += 1;
    }
    fn preview_set_camera_position(
        &mut self,
        _id: PreviewSpace,
        _position: pyreal_primitives::Vec3,
    ) {
        self.counts.preview_calls += 1;
    }
    fn preview_set_camera_direction(
        &mut self,
        _id: PreviewSpace,
        _direction: pyreal_primitives::Vec3,
    ) {
        self.counts.preview_calls += 1;
    }
    fn preview_set_camera_direction_degrees(
        &mut self,
        _id: PreviewSpace,
        _degrees: pyreal_primitives::Vec3,
    ) {
        self.counts.preview_calls += 1;
    }
    fn preview_remove_all_objects(&mut self, _id: PreviewSpace) {
        self.counts.preview_calls += 1;
    }
    fn preview_add_object(
        &mut self,
        _id: PreviewSpace,
        _store: &pyreal_dat::RetailDatStore,
        _setup: DataId,
    ) -> Result<Option<usize>, PresentError> {
        self.counts.preview_calls += 1;
        Ok(None)
    }
    fn preview_add_object_dressed(
        &mut self,
        _id: PreviewSpace,
        _store: &pyreal_dat::RetailDatStore,
        _setup: DataId,
        _objdesc: Option<&pyreal_animation::parts::ObjDesc>,
    ) -> Result<Option<usize>, PresentError> {
        self.counts.preview_calls += 1;
        Ok(None)
    }
    fn preview_set_heading(&mut self, _id: PreviewSpace, _index: usize, _degrees: f32) {
        self.counts.preview_calls += 1;
    }
    fn preview_set_scale(&mut self, _id: PreviewSpace, _index: usize, _scale: f32) {
        self.counts.preview_calls += 1;
    }
    fn preview_set_sequence_animation(
        &mut self,
        _id: PreviewSpace,
        _index: usize,
        _animation: DataId,
        _clear: bool,
        _low_frame: i32,
        _framerate: f32,
    ) -> bool {
        self.counts.preview_calls += 1;
        false
    }
    fn preview_clear_sequence_anims(&mut self, _id: PreviewSpace, _index: usize) {
        self.counts.preview_calls += 1;
    }
    fn preview_has_anims(&self, _id: PreviewSpace, _index: usize) -> bool {
        false
    }
    fn preview_use_time(&mut self, _id: PreviewSpace, _dt: f64) {
        self.counts.preview_calls += 1;
    }
    fn preview_curr_frame_number(&self, _id: PreviewSpace, _index: usize) -> Option<u32> {
        None
    }

    fn apply_device_preference_requests(
        &mut self,
        requests: Vec<pyreal_client_contract::UiRequest>,
    ) -> Vec<pyreal_client_contract::UiRequest> {
        requests
    }
    fn apply_render_preference_requests(
        &mut self,
        requests: Vec<pyreal_client_contract::UiRequest>,
    ) -> Vec<pyreal_client_contract::UiRequest> {
        requests
    }

    fn load_first_pixel_scene(
        &mut self,
        _assets: &dyn pyreal_primitives::AssetSource,
        _id: DataId,
    ) -> Result<(), PresentError> {
        Ok(())
    }
    fn load_world(
        &mut self,
        _store: &std::sync::Arc<pyreal_dat::RetailDatStore>,
        _cfg: crate::scene::SceneConfig,
        _world: &mut Option<crate::world_state::WorldState>,
    ) -> Result<(), pyreal_world_data::landblock::WorldError> {
        self.counts.load_world += 1;
        Ok(())
    }
    fn release_world(&mut self, _world: &mut Option<crate::world_state::WorldState>) -> u32 {
        self.counts.release_world += 1;
        0
    }
    fn stream_world(
        &mut self,
        _store: &pyreal_dat::RetailDatStore,
        _world: Option<&mut crate::world_state::WorldState>,
    ) -> Result<(), pyreal_world_data::landblock::WorldError> {
        self.counts.stream_world += 1;
        Ok(())
    }
    fn sync_objects(
        &mut self,
        _store: &std::sync::Arc<pyreal_dat::RetailDatStore>,
        _stream: &mut crate::objects::ObjectStream,
        _world: Option<&mut crate::world_state::WorldState>,
    ) -> Result<(), pyreal_world_data::landblock::WorldError> {
        self.counts.sync_objects += 1;
        Ok(())
    }
    fn prepare_object_dispatch(
        &mut self,
        _store: &std::sync::Arc<pyreal_dat::RetailDatStore>,
        _stream: &mut crate::objects::ObjectStream,
        _world: Option<&mut crate::world_state::WorldState>,
    ) -> Result<(), pyreal_world_data::landblock::WorldError> {
        self.counts.prepare_object_dispatch += 1;
        Ok(())
    }
    fn update_render_preferences(
        &mut self,
        _store: &pyreal_dat::RetailDatStore,
        _world: Option<&mut crate::world_state::WorldState>,
    ) -> Result<(), pyreal_world_data::landblock::WorldError> {
        self.counts.update_render_preferences += 1;
        Ok(())
    }
    fn set_world_view_state(&mut self, _hidden: bool, _view_distance: Option<f32>) {}

    fn scene<'a>(
        &'a self,
        _world: Option<&'a crate::world_state::WorldState>,
    ) -> Option<Box<dyn Scene + 'a>> {
        None
    }
    fn scene_mut<'a>(
        &'a mut self,
        _world: Option<&'a mut crate::world_state::WorldState>,
    ) -> Option<Box<dyn SceneMut + 'a>> {
        None
    }
}
