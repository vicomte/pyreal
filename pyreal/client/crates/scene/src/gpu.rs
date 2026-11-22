//! The presentation on the engine: the device and its window, the world drawn by `pyreal-engine`
//! from the runtime's world state ([`crate::bridge`]), the UI drawn into the engine's UI picture
//! ([`pyreal_engine::overlay`]), and the preview spaces ([`crate::previews`]).
//!
//! The world itself (what is loaded, the body, the objects and their motion) is the runtime's
//! [`pyreal_client_runtime::sim_present::SimPresentation`]: this presentation holds one, forwards the world's lifetime to it, and
//! draws the world state it builds. A frame is: `start_frame` (the UI's list begins),
//! `draw_scene` (the engine's scene is made to say what the world says, and the camera and the
//! light are taken), `draw_overlay` (the UI's triangles and previews), `end_frame` (the UI's
//! picture is drawn, the engine draws the world under it to the back buffer, which goes to the
//! window).

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use pyreal_client_contract::overlay::{
    GroundMarker, OverlayDepth, OverlayItem, OverlayMaterial, OverlayReleased, OverlayTexture,
    PreviewSpace as PreviewId,
};
use pyreal_client_contract::window::WindowHandles;
use pyreal_client_runtime::present::{PresentError, Presentation, Scene, SceneMut};
use pyreal_client_runtime::render_prefs::RenderPreferences;
use pyreal_client_runtime::sim_present::SimPresentation;
use pyreal_client_runtime::world_state::WorldState;
use pyreal_dat::RetailDatStore;
use pyreal_engine::overlay::{Blend, Overlay, Sampling, Vertex};
pub use pyreal_engine::pack::EnginePack;
use pyreal_engine::world::AssetLoader;
use pyreal_engine::{Camera, Engine, Gpu, PoolSizes, SkyLight};
use pyreal_primitives::{DataId, Inset, ObjectId, Viewport};

use crate::bridge::{Bridge, Reach};
use crate::previews::Previews;

/// The projection [`SceneRenderer::screen_point`] gives a point's depth by: `z / w` from 0 at
/// `MARK_NEAR` metres to 1 at `MARK_FAR`.
const MARK_NEAR: f32 = 0.1;
const MARK_FAR: f32 = 2000.0;

/// A depth as [`SceneRenderer::screen_point`] gives it, as metres from the eye.
fn depth_metres(z: f32) -> f32 {
    MARK_FAR * MARK_NEAR / (MARK_FAR - z.clamp(0.0, 1.0) * (MARK_FAR - MARK_NEAR))
}

/// A half float's value.
fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from((h >> 10) & 0x1F);
    let frac = f32::from(h & 0x3FF);
    sign * match exp {
        0 => frac * 2f32.powi(-24),
        31 => f32::INFINITY,
        e => (1.0 + frac / 1024.0) * 2f32.powi(e - 15),
    }
}

/// A linear value as an sRGB-encoded byte, clamped to SDR white.
fn srgb_byte(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * pyreal_primitives::num::math::powf(v, 1.0 / 2.4) - 0.055
    };
    u8::try_from(pyreal_primitives::num::to_i32(s * 255.0 + 0.5).clamp(0, 255)).unwrap_or(255)
}

/// How many frames old a pick's answer may be and still be given.
const PICK_FRESH: u64 = 3;

/// The key of the first-pixel picture in the overlay.
const FIRST_PIXEL_KEY: u64 = u64::MAX - 1;
/// One white texel, which the world's marks are drawn with.
const WHITE_KEY: u64 = u64::MAX - 2;
/// Preview spaces' pictures in the overlay, from here up.
const PREVIEW_KEYS: u64 = u64::MAX - 64;

/// A failure of the presentation's device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceError(pub String);

impl std::fmt::Display for DeviceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DeviceError {}

/// Why the driver lost the device (a Windows TDR, a GPU reset), once it has.
static DEVICE_LOST: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Why the graphics device was lost, when it was: the run ends at the frame that sees it.
#[must_use]
pub fn device_lost() -> Option<&'static str> {
    DEVICE_LOST.get().map(String::as_str)
}

impl From<pyreal_engine::EngineError> for DeviceError {
    fn from(e: pyreal_engine::EngineError) -> Self {
        Self(e.to_string())
    }
}

impl From<DeviceError> for PresentError {
    fn from(e: DeviceError) -> Self {
        PresentError(e.0)
    }
}

/// A pick's answer: the viewport point asked, the object there, the frame it was read in and how
/// far the drawn surface there is (`None` for the sky).
type PickAnswer = ((f32, f32), ObjectId, u64, Option<f32>);

/// What the scene views read beside the world state: the selection the runtime names, the pick
/// read off the last frame, the frame rate and the game viewport.
#[derive(Debug, Default)]
pub struct Look {
    selected: Cell<Option<ObjectId>>,
    /// The last pick's answer: the viewport point it was asked at and the object there.
    picked: Cell<Option<PickAnswer>>,
    /// The frames drawn, which an answer is dated by: one older than [`PICK_FRESH`] frames is not
    /// given.
    frame: Cell<u64>,
    /// A point the runtime asked about that the last pick did not answer: picked after the frame.
    wanted: Cell<Option<(f32, f32)>>,
    viewport: Cell<Option<Viewport>>,
    /// The render preferences in force, for the field of view.
    prefs: RefCell<RenderPreferences>,
    /// The weather's sound as the last drawn frame left it.
    weather_sound: Cell<pyreal_audio::WeatherSound>,
    /// Thunder due now, waiting for the sound's next drain.
    thunder: RefCell<Vec<pyreal_client_runtime::audio::SoundTrigger>>,
}

impl Look {
    /// The last pick's object and drawn distance, when it was asked at `at` and is fresh.
    fn answer_at(&self, at: (f32, f32)) -> Option<(ObjectId, Option<f32>)> {
        let (p, id, when, distance) = self.picked.get()?;
        ((p.0 - at.0).abs() < 1.0
            && (p.1 - at.1).abs() < 1.0
            && self.frame.get().saturating_sub(when) <= PICK_FRESH)
            .then_some((id, distance))
    }

    /// The field of view the world is drawn with on a `width` x `height` back buffer: the game
    /// field of view over the game viewport's aspect, as the client's `view_params` takes it.
    #[must_use]
    pub fn fov_y(&self, width: u32, height: u32) -> f32 {
        let prefs = self.prefs.borrow();
        let vp = self.viewport.get().unwrap_or(Viewport {
            x: 0,
            y: 0,
            width,
            height,
        });
        #[allow(clippy::cast_precision_loss)]
        let (fw, fh) = (width.max(1) as f32, height.max(1) as f32);
        let display = prefs.aspect().display_aspect_ratio(fw, fh);
        #[allow(clippy::cast_precision_loss)]
        let aspect = pyreal_client_contract::camera::compute_aspect_for_viewport(
            vp.width.max(1) as f32,
            vp.height.max(1) as f32,
            display,
            false,
        );
        pyreal_client_contract::camera::fov_y_from_preference(prefs.game_fov_rad(), aspect)
    }
}

/// The scene as the frame reads it: the runtime's world view, with the field of view, the
/// viewport, the frame rate, the selection and the pick this presentation knows.
struct View<'a, S: ?Sized> {
    inner: Box<S>,
    look: &'a Look,
    size: (u32, u32),
}

impl<S: ?Sized + Scene> std::fmt::Debug for View<'_, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View").finish_non_exhaustive()
    }
}

macro_rules! view_reads {
    ($inner:ty) => {
        impl Scene for View<'_, $inner> {
            fn world(&self) -> &WorldState {
                self.inner.world()
            }
            fn character(&self) -> Option<&pyreal_client_runtime::character::Character> {
                self.inner.character()
            }
            fn census(&self) -> pyreal_client_runtime::present::SceneCensus {
                self.inner.census()
            }
            fn server_object_count(&self) -> usize {
                self.inner.server_object_count()
            }
            fn viewer_cell_id(&self) -> Option<pyreal_primitives::CellId> {
                self.inner.viewer_cell_id()
            }
            fn viewer_block(&self) -> Option<(i32, i32)> {
                self.inner.viewer_block()
            }
            fn loading_near_viewer(&self) -> bool {
                self.inner.loading_near_viewer()
            }
            fn blocks_pending(&self) -> usize {
                self.inner.blocks_pending()
            }
            fn interior_batches(&self) -> usize {
                self.inner.interior_batches()
            }
            fn game_date_time(&self) -> Option<(String, String)> {
                self.inner.game_date_time()
            }
            fn render_preferences(&self) -> RenderPreferences {
                *self.look.prefs.borrow()
            }
            fn server_object_frame(&self, id: ObjectId) -> Option<pyreal_primitives::Frame> {
                self.inner.server_object_frame(id)
            }
            fn effective_viewport(&self, width: u32, height: u32) -> Viewport {
                self.look.viewport.get().unwrap_or(Viewport {
                    x: 0,
                    y: 0,
                    width,
                    height,
                })
            }
            fn view_params(
                &self,
                width: u32,
                height: u32,
            ) -> pyreal_client_contract::camera::ViewParams {
                pyreal_client_contract::camera::ViewParams {
                    viewport: self.effective_viewport(width, height),
                    fov_y_rad: self.look.fov_y(width, height),
                    ..pyreal_client_contract::camera::ViewParams::default()
                }
            }
            fn as_pick_scene(&self) -> &dyn pyreal_client_runtime::pick::PickScene {
                self
            }
            fn take_selected_part_drawn(&self) -> bool {
                self.inner.take_selected_part_drawn()
            }
            fn set_selected_object_id(&self, id: Option<ObjectId>) {
                self.look.selected.set(id);
            }
            fn listener(&self) -> pyreal_audio::Listener {
                self.inner.listener()
            }
            fn terrain_neighbourhood(&self) -> pyreal_audio::TerrainNeighbourhood {
                self.inner.terrain_neighbourhood()
            }
            fn character_sound_table(&self) -> Option<DataId> {
                self.inner.character_sound_table()
            }
            fn weather_sound(&self) -> pyreal_audio::WeatherSound {
                self.look.weather_sound.get()
            }
            fn sound_room(&self) -> pyreal_audio::Room {
                self.inner.sound_room()
            }
            fn region(&self) -> &pyreal_assets::Region {
                self.inner.region()
            }
        }

        impl pyreal_client_runtime::pick::PickScene for View<'_, $inner> {
            fn viewer(&self) -> pyreal_primitives::Frame {
                self.inner.as_pick_scene().viewer()
            }
            fn object_frame(&self, id: ObjectId) -> Option<pyreal_primitives::Frame> {
                self.inner.as_pick_scene().object_frame(id)
            }
            fn fov_y_rad(&self, screen: (u32, u32)) -> f32 {
                self.look.fov_y(screen.0, screen.1)
            }
            fn local_body(&self) -> Option<pyreal_client_runtime::pick::LocalBody> {
                self.inner.as_pick_scene().local_body()
            }
            fn object_part_frames(&self, id: ObjectId) -> Option<Vec<pyreal_primitives::Frame>> {
                self.inner.as_pick_scene().object_part_frames(id)
            }
            fn picked(&self, at: (f32, f32)) -> Option<ObjectId> {
                let _ = self.size;
                // Asked again whatever the answer, so it stays a frame or two old.
                self.look.wanted.set(Some(at));
                self.look.answer_at(at).map(|a| a.0)
            }
            fn drawn_distance(&self, at: (f32, f32)) -> Option<f32> {
                self.look.answer_at(at).and_then(|a| a.1)
            }
        }
    };
}

view_reads!(dyn Scene + '_);
view_reads!(dyn SceneMut + '_);

impl SceneMut for View<'_, dyn SceneMut + '_> {
    fn world_mut(&mut self) -> &mut WorldState {
        self.inner.world_mut()
    }
    fn take_sound_events(&mut self) -> Vec<pyreal_client_runtime::audio::SoundTrigger> {
        let mut v = self.inner.take_sound_events();
        v.append(&mut self.look.thunder.borrow_mut());
        v
    }
    fn update(
        &mut self,
        input: pyreal_client_runtime::camera::CameraInput,
        character: pyreal_client_runtime::character::CharacterInput,
        now: pyreal_primitives::LocalTime,
        dt: f32,
    ) {
        self.inner.update(input, character, now, dt);
    }
    fn apply_camera_translucency(&mut self) {
        self.inner.apply_camera_translucency();
    }
    fn release_landscape_for_teleport(&mut self, destination: pyreal_primitives::LandblockId) {
        self.inner.release_landscape_for_teleport(destination);
    }
    /// The body's particle manager emptied and the script played on it, as the barber's Apply
    /// does for a special heritage.
    fn replace_player_particle_script(&mut self, script: DataId) -> bool {
        let Some(character) = self.inner.world().character.as_ref() else {
            return false;
        };
        let mut driver = character.driver_mut();
        driver.particles = pyreal_animation::ParticleManager::default();
        // Heritage 10's two no-crown scripts are zero: the old manager still goes.
        script.0 == 0 || driver.play_script_id(script)
    }
    fn set_environment_override_state(
        &mut self,
        state: pyreal_client_runtime::environment::EnvironmentOverrideState,
    ) {
        self.inner.set_environment_override_state(state);
    }
    fn sync_environment_override_flags(&mut self) {
        self.inner.sync_environment_override_flags();
    }
    fn set_always_daylight(&mut self, on: bool) -> bool {
        self.inner.set_always_daylight(on)
    }
    fn set_weather_enabled(&mut self, on: bool) {
        self.inner.set_weather_enabled(on);
    }
    fn set_world_fog(&mut self, on: bool) -> bool {
        self.inner.set_world_fog(on)
    }
}

/// What the last frames took, for the report and the meter.
#[derive(Debug, Clone, Default)]
pub struct FrameTimes {
    /// The processor's time from `draw_scene` to the frame's submission, ms.
    pub cpu_ms: f64,
    /// The engine's GPU span of the last timed frame, ms.
    pub gpu_ms: f64,
    pub frames: u64,
    /// Each frame's period (from one frame's start to the next's) and the GPU span read then.
    pub history: Vec<(f64, f64)>,
    last_start: Option<std::time::Instant>,
}

impl FrameTimes {
    /// The median frame period and the median and 95th percentile GPU span, ms, of the frames
    /// after the first `skip`; and how many frames that is.
    #[must_use]
    pub fn summary(&self, skip: usize) -> Option<(usize, f64, f64, f64)> {
        let rows = self.history.get(skip..)?;
        if rows.is_empty() {
            return None;
        }
        let pick = |mut v: Vec<f64>, p: f64| {
            v.sort_by(f64::total_cmp);
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            let i = ((v.len() - 1) as f64 * p).round() as usize;
            v[i]
        };
        let period: Vec<f64> = rows.iter().map(|r| r.0).collect();
        let gpu: Vec<f64> = rows.iter().map(|r| r.1).filter(|g| *g > 0.0).collect();
        let (g50, g95) = if gpu.is_empty() {
            (0.0, 0.0)
        } else {
            (pick(gpu.clone(), 0.5), pick(gpu, 0.95))
        };
        Some((rows.len(), pick(period, 0.5), g50, g95))
    }
}

/// The presentation: see the module header.
pub struct SceneRenderer {
    pub gpu: Gpu,
    surface: Option<Arc<wgpu::Surface<'static>>>,
    /// Held by whoever puts a frame on the window or configures it: this thread, or the portal
    /// thread while portal space is up.
    presenting: Arc<std::sync::Mutex<()>>,
    config: Option<wgpu::SurfaceConfiguration>,
    format: wgpu::TextureFormat,
    pub engine: Engine,
    /// Bumped whenever the engine is made again (another set of dats), which the preview spaces'
    /// meshes are made in.
    generation: u64,
    preview_pass: pyreal_engine::preview::PreviewPass,
    /// The world, built and stepped by the runtime.
    pub sim: SimPresentation,
    store: Option<Arc<RetailDatStore>>,
    pub loader: AssetLoader,
    pack: Option<Arc<EnginePack>>,
    /// The base pack built into the client, drawn alone when no pack is chosen.
    base: Option<Arc<EnginePack>>,
    /// Surface capabilities a headless renderer stands in for a window's, for tests.
    offered_caps: Option<wgpu::SurfaceCapabilities>,
    /// The start-up packs opening on their own thread, joined before anything reads a pack.
    opening: Option<std::thread::JoinHandle<Opened>>,
    pub bridge: Option<Bridge>,
    overlay: Overlay,
    // ORDER-OK: looked up by key.
    overlay_keys: BTreeMap<OverlayTexture, u64>,
    next_key: u64,
    pub previews: Previews,
    size: (u32, u32),
    back: Option<wgpu::Texture>,
    /// The camera and the light `draw_scene` took for this frame, when it draws the world.
    frame: Option<(Camera, SkyLight)>,
    world_hidden: bool,
    pub view_distance: Option<f32>,
    /// The teleport's moment while it runs.
    portal_look: Option<pyreal_client_contract::teleport::PortalLook>,
    /// The colour of the portal stepped through, taken as the trip began.
    portal_colour: Option<glam::Vec3>,
    /// Portal space drawn by the engine (the tunnel flown through) in place of the swirl.
    pub portal_enhanced: bool,
    portal: Arc<std::sync::Mutex<pyreal_engine::portal::PortalSpace>>,
    /// Portal space drawn and shown on a thread of its own, with a window.
    portal_thread: Option<pyreal_engine::portal::PortalThread>,
    /// When the last frame ended while the portal thread had the screen, to pace this thread.
    paced: Option<std::time::Instant>,
    /// The world was on screen as the teleport began.
    portal_from_world: bool,
    world_drawn_last: bool,
    /// The portal lights behind the screens before the world, while they ask for it, and its clock.
    backdrop: bool,
    drift: Option<pyreal_engine::drift::Drift>,
    drift_clock: f64,
    drift_last: Option<std::time::Instant>,
    inset: Option<Inset>,
    first_pixel: bool,
    pub world_marks: Vec<crate::marks::WorldMark>,
    ground_markers: Vec<GroundMarker>,
    waypoint_ring: Option<pyreal_client_contract::overlay::WaypointRing>,
    pub look: Look,
    /// Thunder on its way: when it is heard (the engine's clock), which peal and how loud.
    thunder_due: Vec<(f64, u32, f32)>,
    /// The last frame's lightning, so a strike is heard once.
    last_flash: f32,
    /// The pick asked for after the last frame, at that viewport point.
    pick_at: Option<(f32, f32)>,
    sync: (bool, bool),
    hidden: bool,
    pub times: FrameTimes,
    frame_started: Option<std::time::Instant>,
    /// The settings the render preferences were last applied as.
    applied: Option<crate::settings::Settings>,
    /// Where the eye was last frame, in the engine's space.
    last_eye: Option<glam::Vec3>,
    /// When the world was last drawn, for the engine's clock.
    last_render: Option<std::time::Instant>,
    /// `PYREAL_FRAME_LOG`: a line a frame (times, where the eye is, what is loaded), for a run's
    /// traversal measured offline.
    frame_log: Option<std::io::BufWriter<std::fs::File>>,
    /// Last timed frame's UI Blur passes and composite, ms on the device.
    ui_blur_ms: (f64, f64),
    /// Where the eye stood this frame, its block and the loaded world, for the frame log.
    frame_note: String,
    /// What the frame did besides its passes, for the hitch reports.
    work: pyreal_client_runtime::report::PresentWork,
    /// `PYREAL_FLY=vx,vy,lift`: the eye carried at a steady speed (metres a second of the
    /// engine's clock) and lifted, for a run's traversal measured offline.
    fly: Option<glam::Vec3>,
    /// The ray-tracing builders' warm-up ([`pyreal_engine::rt::warm_up`]), run on a thread of its
    /// own from the start: when it began, and whether it is done.
    warm_up: Option<(std::time::Instant, Arc<std::sync::atomic::AtomicBool>)>,
}

/// How long the shaders' preparation runs before the screens say so: a warm driver cache is done
/// well within it, so a player only ever sees the note when the wait is real.
pub const PREPARING_NOTE_AFTER: std::time::Duration = std::time::Duration::from_millis(1500);

impl Drop for SceneRenderer {
    /// What the run added to the pipeline cache (pipelines made as the world first needed them) is
    /// kept for the next.
    fn drop(&mut self) {
        if self.gpu.cache.is_some() {
            log_cache_save(self.gpu.save_pipeline_cache());
        }
    }
}

impl std::fmt::Debug for SceneRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SceneRenderer")
            .field("size", &self.size)
            .field("bridge", &self.bridge)
            .finish_non_exhaustive()
    }
}

/// The start-up packs a thread opened: the base, and the one chosen over it.
#[derive(Default)]
struct Opened {
    base: Option<Arc<EnginePack>>,
    chosen: Option<Arc<EnginePack>>,
}

/// Opens the pack at `path`, over `base` when given, logged under `what`.
fn open_pack(what: &str, path: &Path, base: Option<&EnginePack>) -> Option<Arc<EnginePack>> {
    let started = std::time::Instant::now();
    match EnginePack::open_over(path, base.map(|b| &*b.pack)) {
        Ok(p) => {
            tracing::info!(
                "{what} {} ({}) opened in {:.0} ms",
                p.pack.name,
                p.pack.hash,
                started.elapsed().as_secs_f64() * 1e3
            );
            Some(Arc::new(p))
        }
        Err(e) => {
            tracing::warn!("the {what} {} would not open: {e}", path.display());
            None
        }
    }
}

/// What the start-up packs' thread opened; nothing when it panicked.
fn join_opening(h: std::thread::JoinHandle<Opened>) -> Opened {
    h.join().unwrap_or_else(|_| {
        tracing::warn!("the art packs' thread failed: drawn without them");
        Opened::default()
    })
}

/// The pipeline cache's save, logged.
fn log_cache_save(saved: Option<(std::path::PathBuf, std::io::Result<usize>)>) {
    match saved {
        Some((file, Ok(bytes))) if bytes > 0 => {
            tracing::debug!(
                "pipeline cache, {bytes} bytes, written to {}",
                file.display()
            );
        }
        Some((file, Err(e))) => {
            tracing::warn!(
                "the pipeline cache could not be written to {}: {e}",
                file.display()
            );
        }
        _ => {}
    }
}

/// Keep the device's pipeline cache in `dir` between runs, for this build (`version`: anything
/// that changes with a build). Set before the renderer is made; see
/// [`pyreal_engine::pipeline_cache`].
pub fn keep_pipeline_cache_in(dir: impl Into<std::path::PathBuf>, version: &str) {
    pyreal_engine::pipeline_cache::set_dir(dir, version);
}

/// What start-up made on this thread or is making on another.
enum Made<'s, T> {
    #[cfg_attr(windows, allow(dead_code))]
    Here(T),
    #[cfg_attr(not(windows), allow(dead_code))]
    There(std::thread::ScopedJoinHandle<'s, T>),
}

impl<T> Made<'_, T> {
    fn is_finished(&self) -> bool {
        match self {
            Self::Here(_) => true,
            Self::There(h) => h.is_finished(),
        }
    }

    fn join(self) -> T {
        match self {
            Self::Here(t) => t,
            Self::There(h) => h.join().unwrap_or_else(|p| std::panic::resume_unwind(p)),
        }
    }
}

fn engine_on(gpu: &Gpu, format: wgpu::TextureFormat) -> Result<Engine, DeviceError> {
    let mut engine = Engine::new(gpu.clone(), PoolSizes::default(), format)?;
    // Only server objects are picked on their translucent surfaces; a static's leaves the pick
    // to what is behind it, as retail's ray never tests one.
    engine.renderer.pick_from = crate::bridge::SERVER_NUMBERS;
    if let Some(e) = fixed_exposure() {
        engine.renderer.exposure = pyreal_engine::renderer::Exposure::Manual(e);
    }
    Ok(engine)
}

/// `PYREAL_EXPOSURE`: a fixed exposure in place of the eye's adaptation, for screenshots.
fn fixed_exposure() -> Option<f32> {
    std::env::var("PYREAL_EXPOSURE")
        .ok()
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|e| e.is_finite() && *e > 0.0)
}

/// The window's format: a float one in extended linear sRGB for HDR Output, an sRGB one otherwise.
#[must_use]
fn output_format(caps: &wgpu::SurfaceCapabilities, hdr: bool) -> wgpu::TextureFormat {
    let float = wgpu::TextureFormat::Rgba16Float;
    if hdr
        && caps
            .color_spaces(float)
            .contains(wgpu::SurfaceColorSpaces::EXTENDED_SRGB_LINEAR)
    {
        return float;
    }
    caps.formats
        .iter()
        .copied()
        .find(wgpu::TextureFormat::is_srgb)
        .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb)
}

/// The window's configuration: `format` at `size`, waiting on the display when `vsync`.
fn surface_configuration(
    caps: &wgpu::SurfaceCapabilities,
    format: wgpu::TextureFormat,
    size: (u32, u32),
    vsync: bool,
) -> wgpu::SurfaceConfiguration {
    wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | (caps.usages & wgpu::TextureUsages::COPY_DST),
        format,
        width: size.0,
        height: size.1,
        present_mode: if vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        },
        desired_maximum_frame_latency: 2,
        alpha_mode: alpha_mode(caps),
        view_formats: vec![],
        // A float output is extended linear sRGB: EDR on the Mac.
        color_space: if format == wgpu::TextureFormat::Rgba16Float {
            wgpu::SurfaceColorSpace::ExtendedSrgbLinear
        } else {
            wgpu::SurfaceColorSpace::Auto
        },
    }
}

/// The window's alpha: opaque when offered, the first offered otherwise.
#[must_use]
fn alpha_mode(caps: &wgpu::SurfaceCapabilities) -> wgpu::CompositeAlphaMode {
    let opaque = wgpu::CompositeAlphaMode::Opaque;
    if caps.alpha_modes.contains(&opaque) {
        return opaque;
    }
    caps.alpha_modes
        .first()
        .copied()
        .unwrap_or(wgpu::CompositeAlphaMode::Auto)
}

impl SceneRenderer {
    /// The device, drawing to `window` when there is one and to an offscreen back buffer
    /// otherwise, `width` by `height`.
    ///
    /// # Errors
    /// No device that can draw the engine.
    pub fn new(
        window: Option<WindowHandles>,
        width: u32,
        height: u32,
    ) -> Result<Self, DeviceError> {
        Self::new_waiting(window, width, height, None, None)
    }

    /// [`Self::new`], with the window kept alive while the pipelines are made: the `splash` (a
    /// PNG, [`crate::splash`]) drawn as soon as there is a device, and `wait` called every frame
    /// or so until the pipelines and the ray-tracing builders' warm-up are ready -- the window
    /// system's messages let through, and the window's client size back, the splash drawn again
    /// at a new one once nothing holds the device's presents. Without `wait` the splash is drawn
    /// once, the warm-up is left running behind the first screens and the window waits for the
    /// pipelines as it always did. Only Windows takes the splash and `wait`; elsewhere start-up
    /// is as [`Self::new`].
    ///
    /// # Errors
    /// As [`Self::new`].
    pub fn new_waiting(
        window: Option<WindowHandles>,
        width: u32,
        height: u32,
        splash: Option<&[u8]>,
        wait: Option<&dyn Fn() -> (u32, u32)>,
    ) -> Result<Self, DeviceError> {
        #[cfg(not(windows))]
        let (splash, wait) = {
            let _ = (splash, wait);
            (None::<&[u8]>, None::<&dyn Fn() -> (u32, u32)>)
        };
        let (gpu, surface) = match window {
            Some(w) => {
                let (gpu, surface) = Gpu::for_window(w.display, w.window)?;
                // A window's device logs what wgpu rejects rather than panicking.
                gpu.device.on_uncaptured_error(Arc::new(|e| {
                    tracing::error!("the GPU rejected a call: {e}");
                }));
                (gpu, Some(Arc::new(surface)))
            }
            None => (Gpu::headless()?, None),
        };
        if pyreal_engine::gpu::ray_queries_turned_off() {
            tracing::warn!("ray queries are off (PYREAL_NO_RAY_QUERIES): lamps cast no shadows");
        }
        gpu.device.set_device_lost_callback(|reason, message| {
            if reason == wgpu::DeviceLostReason::Unknown {
                tracing::error!("the graphics device was lost: {message}");
                let _ = DEVICE_LOST.set(message);
            }
        });
        let format = surface
            .as_ref()
            .map_or(wgpu::TextureFormat::Rgba8UnormSrgb, |s| {
                output_format(&s.get_capabilities(&gpu.adapter), false)
            });
        match &gpu.cache_start {
            pyreal_engine::pipeline_cache::Start::None => {}
            pyreal_engine::pipeline_cache::Start::Empty => {
                tracing::info!("pipeline cache: none kept yet; starting an empty one");
            }
            pyreal_engine::pipeline_cache::Start::Read(f) => {
                tracing::info!("pipeline cache read from {}", f.display());
            }
            pyreal_engine::pipeline_cache::Start::Refused(f, why) => {
                tracing::info!(
                    "pipeline cache {} not used ({why}); starting an empty one",
                    f.display()
                );
            }
        }
        // The splash, before anything slow: one small pipeline, drawn while the rest are made.
        let splash = splash
            .filter(|_| surface.is_some())
            .and_then(|png| crate::splash::Splash::new(&gpu, format, png));
        // Drawn once at each size the window takes while it waits (and again until a draw takes:
        // a window being made or minimised gives no frame to draw to).
        let mut splashed = None;
        let mut configured = None;
        let mut show_splash = |size: (u32, u32)| {
            let (Some(splash), Some(surface)) = (splash.as_ref(), surface.as_ref()) else {
                return;
            };
            if size.0 == 0 || size.1 == 0 || splashed == Some(size) {
                return;
            }
            if configured != Some(size) {
                let caps = surface.get_capabilities(&gpu.adapter);
                surface.configure(
                    &gpu.device,
                    &surface_configuration(&caps, format, size, true),
                );
                configured = Some(size);
            }
            if splash.draw(&gpu, surface, size) {
                splashed = Some(size);
            } else {
                tracing::debug!("the splash could not be drawn at {}x{}", size.0, size.1);
            }
        };
        // A window made hidden (on Windows) is shown first: what is presented to a hidden window
        // is never seen.
        show_splash(wait.map_or((width, height), |wait| wait()));
        // The ray-tracing builders compiled on a thread of their own, so the world's first frame
        // finds them ready. Nothing is presented to the window while they compile (see
        // [`pyreal_engine::rt::warm_up`]), so a window's splash stays up until they are done.
        let mut warm_thread = None;
        let warm_up = gpu.rays().then(|| {
            let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let (g, flag) = (gpu.clone(), Arc::clone(&done));
            let began = std::time::Instant::now();
            let spawned = std::thread::Builder::new()
                .name("ray warm-up".into())
                .spawn(move || {
                    pyreal_engine::rt::warm_up(&g);
                    flag.store(true, std::sync::atomic::Ordering::Release);
                    tracing::info!(
                        "ray-tracing builders ready in {} ms",
                        began.elapsed().as_millis()
                    );
                });
            match spawned {
                Ok(thread) => warm_thread = Some(thread),
                Err(_) => done.store(true, std::sync::atomic::Ordering::Release),
            }
            (began, done)
        });
        // Its thread's end, not its flag: a warm-up that panicked is as finished as one that ran.
        let warming = || warm_thread.as_ref().is_some_and(|t| !t.is_finished());
        // The engine's pipelines, the interface's and portal space's, made side by side (the
        // engine's own on a pool of threads), so a cold driver compiles them on every core. This
        // thread keeps the window: it lets the window system's messages through while it waits, so
        // the window is never "not responding", and draws the splash again when its size changes.
        let began = std::time::Instant::now();
        let (made, overlay, portal) = std::thread::scope(|s| {
            let overlay = s.spawn(|| Overlay::new(&gpu));
            // Portal space is built here, so a trip's first frame waits on no pipeline.
            let portal = s.spawn(|| {
                let mut portal = pyreal_engine::portal::PortalSpace::new(&gpu);
                portal.prepare(&gpu, format);
                portal
            });
            let make = || {
                engine_on(&gpu, format).map(|engine| {
                    let preview = pyreal_engine::preview::PreviewPass::new(&gpu, &engine.materials);
                    (engine, preview)
                })
            };
            // Only Windows makes the engine on another thread, while its window is kept answering.
            #[cfg(not(windows))]
            let engine = Made::Here(make());
            #[cfg(windows)]
            let engine = Made::There(s.spawn(make));
            if let Some(wait) = wait {
                while warming()
                    || !(engine.is_finished() && overlay.is_finished() && portal.is_finished())
                {
                    let size = wait();
                    // A present waits for the warm-up's recording, the messages with it: the
                    // window keeps the splash it has, stretched, until the warm-up is done.
                    if !warming() {
                        show_splash(size);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(15));
                }
            }
            (
                engine.join(),
                overlay
                    .join()
                    .unwrap_or_else(|p| std::panic::resume_unwind(p)),
                portal
                    .join()
                    .unwrap_or_else(|p| std::panic::resume_unwind(p)),
            )
        });
        let (engine, preview_pass) = made?;
        tracing::info!(
            "pipelines made in {} ms{}",
            began.elapsed().as_millis(),
            if pyreal_engine::jobs::serial_asked() {
                " (one at a time: PYREAL_SERIAL_PIPELINES)"
            } else {
                ""
            }
        );
        // What the device compiled, kept for the next run, written off the frame's thread.
        if gpu.cache.is_some() {
            let g = gpu.clone();
            let _ = std::thread::Builder::new()
                .name("pipeline cache".into())
                .spawn(move || log_cache_save(g.save_pipeline_cache()));
        }
        let portal = Arc::new(std::sync::Mutex::new(portal));
        let presenting = Arc::default();
        let portal_thread = surface.as_ref().and_then(|s| {
            pyreal_engine::portal::PortalThread::spawn(
                gpu.clone(),
                Arc::clone(s),
                Arc::clone(&presenting),
                Arc::clone(&portal),
            )
            .map_err(|e| tracing::warn!("portal space keeps the frame's thread: {e}"))
            .ok()
        });
        let mut r = Self {
            gpu,
            surface,
            presenting,
            portal_thread,
            paced: None,
            config: None,
            format,
            engine,
            generation: 0,
            preview_pass,
            sim: SimPresentation::new(width, height),
            store: None,
            loader: AssetLoader::default(),
            pack: None,
            base: None,
            offered_caps: None,
            opening: None,
            bridge: None,
            overlay,
            overlay_keys: BTreeMap::new(),
            next_key: 1,
            previews: Previews::default(),
            size: (width.max(1), height.max(1)),
            back: None,
            frame: None,
            world_hidden: false,
            view_distance: None,
            portal_look: None,
            portal_colour: None,
            portal_enhanced: true,
            portal,
            portal_from_world: false,
            world_drawn_last: false,
            backdrop: false,
            drift: None,
            drift_clock: 0.0,
            drift_last: None,
            inset: None,
            first_pixel: false,
            world_marks: Vec::new(),
            ground_markers: Vec::new(),
            waypoint_ring: None,
            look: Look::default(),
            thunder_due: Vec::new(),
            last_flash: 0.0,
            pick_at: None,
            sync: (false, true),
            hidden: false,
            times: FrameTimes::default(),
            frame_started: None,
            applied: None,
            last_eye: None,
            last_render: None,
            ui_blur_ms: (0.0, 0.0),
            frame_log: std::env::var_os("PYREAL_FRAME_LOG")
                .and_then(|p| std::fs::File::create(p).ok())
                .map(std::io::BufWriter::new),
            frame_note: String::new(),
            work: pyreal_client_runtime::report::PresentWork::default(),
            fly: std::env::var("PYREAL_FLY").ok().and_then(|v| {
                let p: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                (p.len() == 3).then(|| glam::Vec3::new(p[0], p[1], p[2]))
            }),
            warm_up,
        };
        r.configure();
        r.apply_settings();
        if r.surface.is_none() {
            // Headless frames are a sixtieth of a second apart, whatever they take to draw.
            r.engine.renderer.frame_seconds = Some(1.0 / 60.0);
        }
        Ok(r)
    }

    /// The adapter's name.
    #[must_use]
    pub fn adapter_name(&self) -> String {
        self.gpu.adapter.get_info().name
    }

    /// The adapter drawing to the window, for the quality preset chosen for it; `None` offscreen.
    #[must_use]
    pub fn graphics_adapter(
        &self,
    ) -> Option<pyreal_client_contract::options::auto_quality::Adapter> {
        use pyreal_client_contract::options::auto_quality::{Adapter, Kind};
        self.surface.as_ref()?;
        let info = self.gpu.adapter.get_info();
        Some(Adapter {
            name: info.name,
            vendor: info.vendor,
            kind: match info.device_type {
                wgpu::DeviceType::IntegratedGpu => Kind::Integrated,
                wgpu::DeviceType::DiscreteGpu => Kind::Discrete,
                wgpu::DeviceType::VirtualGpu => Kind::Virtual,
                wgpu::DeviceType::Cpu => Kind::Software,
                wgpu::DeviceType::Other => Kind::Other,
            },
        })
    }

    /// Whether the shaders are still being prepared, [`PREPARING_NOTE_AFTER`] or more after the
    /// start: the screens before the world then say so. A warm driver is done long before, and a
    /// device without ray tracing has nothing to prepare.
    #[must_use]
    pub fn preparing_shaders(&self) -> bool {
        self.warm_up.as_ref().is_some_and(|(began, done)| {
            !done.load(std::sync::atomic::Ordering::Acquire)
                && began.elapsed() >= PREPARING_NOTE_AFTER
        })
    }

    /// The graphics API the device draws through (`Vulkan`, `Dx12`, `Metal`).
    #[must_use]
    pub fn backend(&self) -> String {
        format!("{:?}", self.gpu.adapter.get_info().backend)
    }

    fn configure(&mut self) {
        let Some(surface) = self.surface.as_ref() else {
            return;
        };
        let caps = surface.get_capabilities(&self.gpu.adapter);
        let config = surface_configuration(&caps, self.format, self.size, self.sync.1);
        {
            let _held = self
                .presenting
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            surface.configure(&self.gpu.device, &config);
        }
        self.config = Some(config);
    }

    /// The engine and the loader for `store`: kept when it is the store they were made for,
    /// made again otherwise (the meshes and pictures of other dats mean nothing here).
    fn use_store(&mut self, store: &Arc<RetailDatStore>) {
        if self.store.as_ref().is_some_and(|s| Arc::ptr_eq(s, store)) {
            return;
        }
        if self.store.is_some() {
            self.remake_engine();
        }
        // High-Res Textures is fixed for a dat set before any of its textures is made: a change
        // takes effect with the next one, as the engine is made anew for it.
        let prefs = self
            .sim
            .config()
            .map_or_else(|| *self.look.prefs.borrow(), |c| c.render);
        pyreal_engine::asset::texture::set_high_res_textures(
            crate::settings::Settings::from_preferences(&prefs).high_res,
        );
        self.store = Some(Arc::clone(store));
        self.fresh_loader();
        self.apply_settings();
    }

    /// The engine made anew: nothing it holds is drawn again.
    fn remake_engine(&mut self) {
        match engine_on(&self.gpu, self.format) {
            Ok(e) => {
                self.engine = e;
                if self.surface.is_none() {
                    self.engine.renderer.frame_seconds = Some(1.0 / 60.0);
                }
                self.preview_pass =
                    pyreal_engine::preview::PreviewPass::new(&self.gpu, &self.engine.materials);
                self.generation += 1;
                self.applied = None;
            }
            Err(e) => tracing::warn!("the engine could not be made again: {e}"),
        }
    }

    /// A loader for the store and the pack drawn, with empty caches.
    fn fresh_loader(&mut self) {
        self.loader = match self.pack.as_ref() {
            Some(p) => AssetLoader::with_shared_pack(Arc::clone(p)),
            None => AssetLoader::default(),
        };
        // The player collides with the pack's trees as they are drawn as well as retail's shapes.
        pyreal_world_data::env_cells::set_drawn_trunks(
            self.pack
                .as_ref()
                .map(|p| Arc::new(pyreal_engine::pack::drawn_trunks(&p.pack))),
        );
        if let Some(store) = self.store.as_ref() {
            self.engine.particles.assets = Some(Arc::new(
                pyreal_world_data::anim_assets::DatAnimAssets::new(Arc::clone(store)),
            ));
        }
    }

    /// The render preferences in force: the world's, or the defaults before one.
    #[must_use]
    pub fn render_preferences(&self) -> RenderPreferences {
        *self.look.prefs.borrow()
    }

    /// The engine's switches from the render preferences, when they changed.
    fn apply_settings(&mut self) {
        let prefs = self
            .sim
            .config()
            .map_or_else(|| *self.look.prefs.borrow(), |c| c.render);
        *self.look.prefs.borrow_mut() = prefs;
        // Ground Coloring: the land is laid again in the other colours.
        let coloring = if pyreal_client_contract::options::ground_coloring::classic() {
            pyreal_engine::ground_tint::GroundColoring::Classic
        } else {
            pyreal_engine::ground_tint::GroundColoring::ThroneOfDestiny
        };
        if self.loader.set_ground_coloring(coloring) {
            if let Some(b) = self.bridge.as_mut() {
                b.reload();
            }
        }
        // Portal Style: portals in the world whirl as the vortex, or as the original swirl.
        self.engine.particles.portals = pyreal_client_contract::options::portal_style::enhanced();
        // Detailed Faces: every head is taken again, plain or shaped, and the previews' with it.
        let faces = pyreal_client_contract::options::face_detail::on();
        if self.loader.set_face_detail(self.store.as_deref(), faces) {
            if let Some(b) = self.bridge.as_mut() {
                b.redress(&mut self.engine);
            }
            self.generation += 1;
        }
        let settings = crate::settings::Settings::from_preferences(&prefs);
        if self.applied.as_ref() == Some(&settings) {
            return;
        }
        self.choose_output(settings.hdr);
        settings.apply(&mut self.engine);
        if let Some(b) = self.bridge.as_mut() {
            if b.reach != settings.reach || b.smoothing != settings.smoothing {
                b.reach = settings.reach;
                b.smoothing = settings.smoothing;
                b.reload();
            }
        }
        if self.sync.1 != settings.vsync {
            self.sync.1 = settings.vsync;
            self.configure();
        }
        self.applied = Some(settings);
    }

    /// HDR Output: a window whose display can show past SDR white takes a float surface in
    /// extended linear sRGB (EDR on the Mac), the engine's tone curve reaching the display's
    /// headroom. A change makes the engine again on the new format and loads the world again.
    fn choose_output(&mut self, hdr: bool) {
        let want = match (self.surface.as_ref(), self.offered_caps.as_ref()) {
            (Some(surface), _) => output_format(&surface.get_capabilities(&self.gpu.adapter), hdr),
            (None, Some(caps)) => output_format(caps, hdr),
            (None, None) => return,
        };
        if want == self.format {
            return;
        }
        match engine_on(&self.gpu, want) {
            Ok(e) => {
                self.engine = e;
                if self.surface.is_none() {
                    self.engine.renderer.frame_seconds = Some(1.0 / 60.0);
                }
                self.preview_pass =
                    pyreal_engine::preview::PreviewPass::new(&self.gpu, &self.engine.materials);
                self.generation += 1;
                self.format = want;
                self.back = None;
                self.applied = None;
                self.configure();
                if self.store.is_some() {
                    self.fresh_loader();
                }
                if let Some(b) = self.bridge.as_ref() {
                    self.bridge = Some(b.again());
                }
                tracing::info!("the window draws to {want:?}");
            }
            Err(e) => tracing::warn!("the engine could not draw to {want:?}: {e}"),
        }
    }

    /// The player's saved render preferences, before any world is loaded (a world brings its own).
    pub fn use_saved_render_preferences(&mut self, prefs: RenderPreferences) {
        if self.sim.config().is_none() {
            *self.look.prefs.borrow_mut() = prefs;
            self.apply_settings();
        }
    }

    /// The format the window (or the back buffer) is drawn to.
    #[must_use]
    pub fn output_format_in_use(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// A headless renderer's stand-in for a window's surface capabilities, for tests.
    #[cfg(feature = "test-support")]
    pub fn offer_surface_capabilities(&mut self, caps: wgpu::SurfaceCapabilities) {
        self.offered_caps = Some(caps);
    }

    /// The display's headroom over SDR white, for an EDR output (1 otherwise), read now and then.
    fn track_headroom(&mut self) {
        if self.format != wgpu::TextureFormat::Rgba16Float || !self.times.frames.is_multiple_of(30)
        {
            return;
        }
        let Some(surface) = self.surface.as_ref() else {
            return;
        };
        let h = surface
            .display_hdr_info(&self.gpu.adapter)
            .headroom
            .and_then(|h| h.current)
            .unwrap_or(1.0);
        self.engine.renderer.headroom = h.clamp(1.0, 16.0);
    }

    /// Opens the base pack built into the client on a thread, drawn under the pack chosen and
    /// alone without one; every world loaded after draws it.
    pub fn open_base_pack(&mut self, path: &Path) {
        if !path.exists() {
            tracing::info!("no base art pack at {}: drawn without", path.display());
            return;
        }
        self.finish_opening();
        let path = path.to_path_buf();
        self.spawn_opening(move || Opened {
            base: open_pack("base art pack", &path, None),
            chosen: None,
        });
    }

    /// The base pack built into the client, which a pack chosen is drawn over.
    pub fn base_pack(&mut self) -> Option<Arc<pyreal_engine::pack::ArtPack>> {
        self.finish_opening();
        self.base.as_ref().map(|b| Arc::clone(&b.pack))
    }

    /// Opens the art pack chosen at start-up over the base pack, on the base's thread when it
    /// is still opening; every world loaded after draws it.
    pub fn open_art_pack(&mut self, path: &Path) {
        if !path.exists() {
            tracing::info!("no art pack at {}: drawn without", path.display());
            return;
        }
        let path = path.to_path_buf();
        let before = self.opening.take();
        let base = self.base.clone();
        self.spawn_opening(move || {
            let mut opened = before.map_or_else(|| Opened { base, chosen: None }, join_opening);
            opened.chosen = match crate::art_pack::packed::added_pack_problem(&path) {
                Some(e) => {
                    tracing::warn!("the art pack would not open: {e}");
                    None
                }
                None => open_pack("art pack", &path, opened.base.as_deref()),
            };
            opened
        });
    }

    /// Runs `open` on the start-up packs' thread, or on this one when no thread starts.
    fn spawn_opening(&mut self, open: impl FnOnce() -> Opened + Send + 'static) {
        let job = Arc::new(std::sync::Mutex::new(Some(open)));
        let theirs = Arc::clone(&job);
        let spawned = std::thread::Builder::new()
            .name("art-pack-open".into())
            .spawn(move || {
                let open = theirs.lock().ok().and_then(|mut j| j.take());
                open.map(|f| f()).unwrap_or_default()
            });
        match spawned {
            Ok(h) => self.opening = Some(h),
            Err(e) => {
                tracing::warn!("the art packs open on the frame's thread: {e}");
                let open = job.lock().ok().and_then(|mut j| j.take());
                if let Some(f) = open {
                    self.install_opened(f());
                }
            }
        }
    }

    /// The start-up packs, drawn from now on once their thread has opened them (waited for).
    fn finish_opening(&mut self) {
        if let Some(h) = self.opening.take() {
            self.install_opened(join_opening(h));
        }
    }

    /// The start-up packs opened: the base kept, and the one chosen drawn (the base without one).
    /// Nothing has been drawn through the loader before (each reader joins first), so the engine
    /// is kept.
    fn install_opened(&mut self, opened: Opened) {
        if let Some(base) = opened.base {
            self.base = Some(base);
        }
        let pack = opened.chosen.or_else(|| self.base.clone());
        if self.pack.is_none() && pack.is_some() {
            self.pack = pack;
            self.fresh_loader();
            self.apply_settings();
        }
    }

    /// Draws `pack` from now on, or the base pack alone (the dats alone without one): with a
    /// world loaded, the engine is made again and the world loaded again round the eye, so
    /// nothing of the last pack stays.
    pub fn use_art_pack(&mut self, pack: Option<Arc<EnginePack>>) {
        self.finish_opening();
        let pack = pack.or_else(|| self.base.clone());
        let same = match (&self.pack, &pack) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        self.pack = pack;
        if self.store.is_some() {
            self.remake_engine();
        }
        self.fresh_loader();
        if let Some(b) = self.bridge.as_ref() {
            self.bridge = Some(b.again());
        }
        self.apply_settings();
    }

    /// The art pack drawn: its name and hash.
    #[must_use]
    pub fn art_pack(&self) -> Option<(String, String)> {
        self.pack
            .as_ref()
            .map(|p| (p.pack.name.clone(), p.pack.hash.clone()))
    }

    // -------------------------------------------------------------------------------------------
    // the frame
    // -------------------------------------------------------------------------------------------

    pub fn start_frame(&mut self) {
        if self
            .opening
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished)
        {
            self.finish_opening();
        }
        self.track_headroom();
        self.overlay.begin();
        self.frame = None;
        let now = std::time::Instant::now();
        if let Some(last) = self.times.last_start.replace(now) {
            if self.times.history.len() < 100_000 {
                self.times.history.push((
                    now.duration_since(last).as_secs_f64() * 1e3,
                    self.times.gpu_ms,
                ));
            }
        }
        self.frame_started = Some(now);
        self.work = pyreal_client_runtime::report::PresentWork::default();
    }

    pub fn set_game_viewport(&mut self, viewport: Option<Viewport>) {
        self.look.viewport.set(viewport);
        self.sim.set_game_viewport(viewport);
    }

    /// The engine's scene made to say what `ws` says, and the frame's camera and light taken.
    pub fn draw_scene(&mut self, ws: Option<&WorldState>) {
        self.frame = None;
        self.look.frame.set(self.look.frame.get() + 1);
        // The last pick's answer, once it is back.
        if let Some(answer) = self.engine.renderer.take_pick(&self.gpu) {
            if let Some(at) = self.pick_at.take() {
                let id = answer
                    .object
                    .and_then(|n| self.bridge.as_ref()?.object_of(n))
                    .unwrap_or(ObjectId(0));
                self.look
                    .picked
                    .set(Some((at, id, self.look.frame.get(), answer.distance)));
            }
        }
        // The screens before the world show the portal lights, not a world kept from the game.
        if self.backdrop && self.portal_frame().is_none() {
            return;
        }
        // Behind portal space the land and objects still stream, so the destination loads while
        // the tunnel or the swirl plays; nothing of it is drawn.
        let behind_portal = self.world_hidden;
        if behind_portal && self.portal_look.is_none() {
            return;
        }
        let (Some(ws), Some(bridge)) = (ws, self.bridge.as_mut()) else {
            if self.first_pixel {
                self.push_first_pixel();
            }
            return;
        };
        let fallback = self.sim.config().map_or((0, 0), |c| {
            pyreal_world_data::landblock::block_xy(c.landblock)
        });
        let (w, h) = self.size;
        let mut fov = self.look.fov_y(w, h);
        if let Some(d) = self.view_distance {
            fov = pyreal_client_contract::camera::set_vdst(d).0;
        }
        let mut eye = ws.camera.frame();
        if let Some(v) = self.fly {
            #[allow(clippy::cast_possible_truncation)]
            let t = self.engine.renderer.clock as f32;
            let at = glam::Vec2::new(eye.origin.x, eye.origin.y) + v.truncate() * t;
            let block = (at / 192.0).floor();
            bridge.nudge = (
                pyreal_primitives::num::floor_to_i32(block.x),
                pyreal_primitives::num::floor_to_i32(block.y),
            );
            let local = at - block * 192.0;
            eye.origin.x = local.x;
            eye.origin.y = local.y;
            eye.origin.z += v.z;
        }
        bridge.staged = self.portal_look.is_some();
        let t = std::time::Instant::now();
        bridge.prepare(
            &mut self.engine,
            &mut self.loader,
            ws,
            fallback,
            glam::Vec2::new(eye.origin.x, eye.origin.y),
        );
        self.work.stream_ms += pyreal_engine::frame_work::ms_since(t);
        note_stream(&mut self.work, bridge);
        let offset = bridge.offset();
        #[allow(clippy::cast_precision_loss)]
        let far = (bridge.reach.radius.max(bridge.reach.far) as f32 + 0.5) * 192.0;
        let mut camera = crate::bridge::camera(&eye, offset, fov, far, ws.viewer_cell());
        if camera.cell.is_none() {
            let feet = ws
                .character
                .as_ref()
                .filter(|c| pyreal_physics::landdefs::is_outdoors(c.position().cell))
                .map(|c| crate::bridge::frame_point(&c.render_frame()) + offset);
            if let Some(land) = self.engine.land.as_ref() {
                camera.position += crate::bridge::camera_lift(land, camera.position, feet);
            }
            let surface = self
                .engine
                .water_surface_at(camera.position.truncate(), camera.position);
            camera.position += crate::bridge::water_lift(camera.position, surface);
        }
        let t = std::time::Instant::now();
        // Faces made off the frame taken on, and detail no near player drew let go.
        self.loader.tick_faces(&mut self.engine, &bridge.store);
        bridge.sync(&mut self.engine, &mut self.loader, ws, camera.position);
        self.work.sync_ms += pyreal_engine::frame_work::ms_since(t);
        let light = crate::bridge::light(&bridge.region, ws);
        let (fall, storm) = crate::bridge::fall(ws);
        hear_weather(
            &self.engine.renderer.weather,
            self.engine.renderer.clock,
            camera.cell.is_some(),
            &mut self.thunder_due,
            &mut self.last_flash,
            &self.look,
        );
        let weather = &mut self.engine.renderer.weather;
        weather.falls(fall, storm);
        weather.snowy_types = pyreal_engine::weather::snowy_types(&bridge.region);
        weather.soft_types = pyreal_engine::weather::soft_types(&bridge.region);
        self.engine.renderer.atmosphere.cloud_knobs = crate::bridge::cloud_knobs(&ws.clouds);
        let (year, day, time_of_day) = ws.game_time();
        self.engine.renderer.atmosphere.moon_phases = crate::bridge::moon_phases(
            &ws.moons,
            pyreal_client_runtime::moon::calendar_days(year, day, time_of_day),
        );
        if self.times.frames.is_multiple_of(600) {
            tracing::debug!(
                "the engine's camera at {:.2} (centre {:?}), yaw {:.3}, pitch {:.3}, vertical field of view {:.1} degrees, cell {:?}",
                camera.position,
                bridge.centre(),
                camera.yaw,
                camera.pitch,
                camera.fov_y.to_degrees(),
                camera.cell
            );
        }
        // The selection, the hover and the ring.
        let selected = self.look.selected.get();
        let number = selected.and_then(|id| bridge.number_of(id));
        // The ring is the interface's ground marker (the last one, the outer ring): under its
        // object, as wide as the marker asks of the object's selection radius, in its tint.
        let marker = self.ground_markers.last().copied();
        let ring = marker.and_then(|m| {
            let (frame, parts_radius) = if Some(m.object) == ws.player_object {
                let c = ws.character.as_ref()?;
                let r = c
                    .driver()
                    .part_array
                    .setup
                    .as_ref()
                    .map(|s| s.selection_sphere.radius);
                (c.render_frame(), r)
            } else {
                let o = ws.objects.get(&m.object).filter(|o| o.drawn)?;
                let driver = o.sim.driver.borrow();
                let parts = &driver.part_array;
                let r = parts
                    .setup
                    .as_ref()
                    .map(|s| s.selection_sphere.radius * parts.scale.z);
                (o.frame, r)
            };
            let r = parts_radius.filter(|r| *r > 0.05).unwrap_or(0.5) * m.scale;
            Some((
                crate::bridge::frame_point(&frame) + offset,
                r.clamp(0.2, 8.0),
            ))
        });
        let colour = marker.map_or(0, |m| {
            // 0xAARRGGBB to the engine's packed RGBA.
            let (a, r, g, b) = (
                m.tint >> 24,
                (m.tint >> 16) & 0xFF,
                (m.tint >> 8) & 0xFF,
                m.tint & 0xFF,
            );
            r | g << 8 | b << 16 | a << 24
        });
        let hovered = self
            .look
            .picked
            .get()
            .filter(|(_, _, when, _)| self.look.frame.get().saturating_sub(*when) <= PICK_FRESH)
            .and_then(|(_, id, _, _)| bridge.number_of(id));
        self.engine.renderer.selection = pyreal_engine::renderer::Selection {
            selected: number.or_else(|| marker.and_then(|m| bridge.number_of(m.object))),
            hovered: hovered.filter(|h| Some(*h) != number),
            ring,
            colour,
        };
        // The waypoint's ring round the player's feet, or with no player on the ground under the
        // camera, its arrow the way to the waypoint.
        self.engine.renderer.waypoint = self.waypoint_ring.and_then(|w| {
            let (at, around) = match (&ws.character, w.camera_height) {
                (Some(c), _) => (
                    crate::bridge::frame_point(&c.render_frame()) + offset,
                    ws.player_object.and_then(|id| bridge.number_of(id)),
                ),
                (None, Some(h)) => (camera.position - glam::Vec3::Z * h, None),
                (None, None) => return None,
            };
            let (sin, cos) = (
                pyreal_primitives::num::math::sinf(w.bearing),
                pyreal_primitives::num::math::cosf(w.bearing),
            );
            Some(pyreal_engine::renderer::WaypointMark {
                at,
                radius: w.radius,
                way: glam::Vec2::new(sin, cos),
                around,
            })
        });
        // The client's viewer light, two metres over the player.
        self.engine.viewer = ws
            .character
            .as_ref()
            .map(|c| crate::bridge::frame_point(&c.render_frame()) + offset + glam::Vec3::Z * 2.0);
        // A camera that jumps (a teleport, a portal, a recentred world) starts the temporal
        // history again rather than smearing the last place into the new one.
        let jumped = self
            .last_eye
            .is_none_or(|p| p.distance(camera.position) > 30.0);
        if jumped {
            self.engine.renderer.temporal.reset();
        }
        self.last_eye = Some(camera.position);
        if self.frame_log.is_some() {
            let c = &bridge.counts;
            let s = bridge.stream_stats();
            self.frame_note = format!(
                "{:?} {:.1} {:.1} {:.1} reloads {} ({:.1} ms) prepare {:.2} ms switches {} ({:.1} ms) rebases {} pending {} trees {} ({} {} worst {:.1} {:?}) evictions {} placed {} removed {}",
                bridge.centre(),
                camera.position.x,
                camera.position.y,
                camera.position.z,
                c.reloads,
                c.reload_ms,
                c.prepare_ms,
                s.switches,
                s.switch_ms,
                s.rebases,
                s.pending,
                s.tree_blocks,
                s.trees_asked,
                s.trees_planned,
                s.worst_step_ms,
                s.worst_step,
                s.evictions,
                s.blocks_placed,
                s.blocks_removed,
            );
        }
        if behind_portal {
            return;
        }
        self.frame = Some((camera, light));
        self.push_marks(ws);
    }

    /// The world's marks (a front end's lines and dots), laid over the world under the interface:
    /// each point where it stands on the screen, a line a strip of its width between them, a dot a
    /// square. Each is hidden wherever the world stands in front of it, unless it asks to stay on
    /// top.
    fn push_marks(&mut self, ws: &WorldState) {
        if self.world_marks.is_empty() {
            return;
        }
        if !self.overlay.contains(WHITE_KEY) {
            self.overlay
                .upload(&self.gpu, WHITE_KEY, 1, 1, &[vec![255, 255, 255, 255]]);
        }
        let (w, h) = self.size;
        #[allow(clippy::cast_precision_loss)]
        let (fw, fh) = (w.max(1) as f32, h.max(1) as f32);
        let ndc = |x: f32, y: f32, d: f32| [x / fw * 2.0 - 1.0, 1.0 - y / fh * 2.0, d];
        let (mut in_world, mut on_top) = (Vec::new(), Vec::new());
        for mark in &self.world_marks {
            // Where each point stands on the screen, and how far from the eye (metres).
            let screen: Vec<(glam::Vec2, f32)> = mark
                .points
                .iter()
                .filter_map(|p| self.screen_point(ws, glam::Vec3::from(*p)))
                .map(|(x, y, z)| (glam::Vec2::new(x, y), depth_metres(z)))
                .collect();
            let half = mark.width.max(1.0) * 0.5;
            let out = if mark.on_top {
                &mut on_top
            } else {
                &mut in_world
            };
            let mut quad = |c: [(glam::Vec2, f32); 4]| {
                for i in [0, 1, 2, 0, 2, 3] {
                    let (p, d) = c[i];
                    out.push(Vertex {
                        position: ndc(p.x, p.y, d),
                        colour: mark.color,
                        uv: [0.5, 0.5],
                    });
                }
            };
            if let [(p, d)] = screen.as_slice() {
                let e = glam::Vec2::splat(half);
                quad([
                    (*p - e, *d),
                    (*p + glam::Vec2::new(e.x, -e.y), *d),
                    (*p + e, *d),
                    (*p + glam::Vec2::new(-e.x, e.y), *d),
                ]);
            }
            for pair in screen.windows(2) {
                let ((a, da), (b, db)) = (pair[0], pair[1]);
                let n = (b - a).perp().normalize_or_zero() * half;
                quad([(a - n, da), (b - n, db), (b + n, db), (a + n, da)]);
            }
        }
        for v in &mut on_top {
            v.position[2] = 0.0;
        }
        let white = Sampling::default();
        self.overlay
            .push_world(WHITE_KEY, Blend::Image, white, &in_world, None);
        self.overlay
            .push(WHITE_KEY, Blend::Image, white, &on_top, None);
    }

    /// The first-pixel picture over the whole frame, while no world is drawn.
    fn push_first_pixel(&mut self) {
        let v = |x: f32, y: f32, u: f32, w: f32| Vertex {
            position: [x, y, 0.0],
            colour: 0xFFFF_FFFF,
            uv: [u, w],
        };
        let quad = [
            v(-1.0, 1.0, 0.0, 0.0),
            v(1.0, 1.0, 1.0, 0.0),
            v(1.0, -1.0, 1.0, 1.0),
            v(-1.0, 1.0, 0.0, 0.0),
            v(1.0, -1.0, 1.0, 1.0),
            v(-1.0, -1.0, 0.0, 1.0),
        ];
        self.overlay.push(
            FIRST_PIXEL_KEY,
            Blend::Image,
            Sampling::default(),
            &quad,
            None,
        );
    }

    /// The UI's list: its triangles and preview spaces, in order.
    pub fn draw_overlay(&mut self, items: &[OverlayItem]) {
        for item in items {
            match item {
                OverlayItem::Triangles {
                    material,
                    texture,
                    sampler,
                    depth,
                    vertices,
                } => {
                    let Some(&key) = self.overlay_keys.get(texture) else {
                        continue;
                    };
                    let blend = match material {
                        OverlayMaterial::Image => Blend::Image,
                        OverlayMaterial::Text => Blend::Text,
                        OverlayMaterial::Invert => Blend::Invert,
                    };
                    // A batch standing in the world carries each vertex's depth as the world's
                    // projection gives it ([`Self::screen_point`]'s): as metres, it is drawn
                    // against the engine's own depth.
                    let world = *depth == OverlayDepth::World;
                    let verts: Vec<Vertex> = vertices
                        .iter()
                        .map(|v| Vertex {
                            position: [
                                v.position[0],
                                v.position[1],
                                if world {
                                    depth_metres(v.position[2])
                                } else {
                                    0.0
                                },
                            ],
                            colour: v.color,
                            uv: v.uv,
                        })
                        .collect();
                    let sampling = Sampling {
                        point: sampler.point,
                        wrap_u: sampler.wrap_u,
                        wrap_v: sampler.wrap_v,
                    };
                    if world {
                        self.overlay.push_world(key, blend, sampling, &verts, None);
                    } else {
                        self.overlay.push(key, blend, sampling, &verts, None);
                    }
                }
                OverlayItem::Preview { space, rect } => self.draw_preview(*space, *rect),
                OverlayItem::Backdrop { rect, strength } => {
                    if pyreal_client_contract::options::ui_effects::blur_on() {
                        let r = (rect.x, rect.y, rect.width, rect.height);
                        self.overlay.push_backdrop(r, *strength);
                    }
                }
            }
        }
    }

    fn draw_preview(&mut self, space: PreviewId, rect: Viewport) {
        self.finish_opening();
        let Some(store) = self.store.clone() else {
            return;
        };
        let world_fov = self.look.prefs.borrow().game_fov_rad();
        if self.bridge.is_none() {
            self.loader.tick_faces(&mut self.engine, &store);
        }
        let Some(view) = self.previews.draw(
            space,
            &mut self.engine,
            &mut self.preview_pass,
            &mut self.loader,
            &store,
            self.generation,
            (rect.width, rect.height),
            world_fov,
            self.view_distance,
        ) else {
            return;
        };
        let key = PREVIEW_KEYS + space as u64;
        self.overlay.insert_view(key, view);
        let (w, h) = (self.size.0.max(1), self.size.1.max(1));
        #[allow(clippy::cast_precision_loss)]
        let ndc = |x: u32, y: u32| {
            [
                x as f32 / w as f32 * 2.0 - 1.0,
                1.0 - y as f32 / h as f32 * 2.0,
            ]
        };
        let (x0, y0) = (rect.x, rect.y);
        let (x1, y1) = (rect.x + rect.width, rect.y + rect.height);
        let v = |p: [f32; 2], u: f32, t: f32| Vertex {
            position: [p[0], p[1], 0.0],
            colour: 0xFFFF_FFFF,
            uv: [u, t],
        };
        let quad = [
            v(ndc(x0, y0), 0.0, 0.0),
            v(ndc(x1, y0), 1.0, 0.0),
            v(ndc(x1, y1), 1.0, 1.0),
            v(ndc(x0, y0), 0.0, 0.0),
            v(ndc(x1, y1), 1.0, 1.0),
            v(ndc(x0, y1), 0.0, 1.0),
        ];
        self.overlay.push(
            key,
            Blend::Premultiplied,
            Sampling {
                point: true,
                wrap_u: false,
                wrap_v: false,
            },
            &quad,
            None,
        );
    }

    fn back_buffer(&mut self) -> wgpu::Texture {
        let size = self.size;
        if let Some(t) = self.back.as_ref() {
            if (t.width(), t.height()) == size {
                return t.clone();
            }
        }
        let t = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("back buffer"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        self.back = Some(t.clone());
        t
    }

    /// The UI's picture drawn, the world under it, and the frame to the window.
    ///
    /// # Errors
    /// The engine's own, or the device lost.
    pub fn end_frame(&mut self) -> Result<(), DeviceError> {
        if let Some(why) = device_lost() {
            return Err(DeviceError(format!("the graphics device was lost: {why}")));
        }
        let size = self.size;
        let ui = (!self.overlay.is_empty()).then(|| self.overlay.finish(&self.gpu, size));
        let back = self.back_buffer();
        let view = back.create_view(&Default::default());
        let portal = self.portal_frame();
        self.world_drawn_last = self.frame.is_some();
        match self.frame.take() {
            Some((camera, light)) => {
                // The engine's clock (wind, water, clouds, the dats' own emitters): a sixtieth of
                // a second a frame with no window, as a headless run's frames are; the wall clock's
                // time otherwise.
                let step = match self.engine.renderer.frame_seconds {
                    Some(s) => f64::from(s),
                    None => self
                        .last_render
                        .map_or(0.0, |t| t.elapsed().as_secs_f64().min(0.25)),
                };
                self.last_render = Some(std::time::Instant::now());
                self.engine.renderer.clock += step;
                self.engine.renderer.ui = ui.clone();
                let t = std::time::Instant::now();
                self.engine
                    .render_with(&camera, &light, &view, size, Some(&self.overlay))?;
                self.work.render_ms += pyreal_engine::frame_work::ms_since(t);
                // With no window nothing paces the frames: each waits for the device, so a
                // headless run's frames, and their times, are the device's own one by one.
                let headless = self.surface.is_none();
                if headless {
                    let t = std::time::Instant::now();
                    self.gpu.wait();
                    self.work.gpu_wait_ms += pyreal_engine::frame_work::ms_since(t);
                }
                let timings = self.engine.renderer.collect_timings(&self.gpu, headless);
                // UI Blur's passes and the composite, for the frame log.
                let sum = |f: &dyn Fn(&str) -> bool| {
                    timings
                        .iter()
                        .filter(|t| f(t.name))
                        .map(|t| t.ms)
                        .sum::<f64>()
                };
                self.ui_blur_ms = (
                    sum(&|n| n.starts_with("ui blur")),
                    sum(&|n| n == "composite"),
                );
                self.times.gpu_ms = self.engine.renderer.frame_span_ms();
            }
            None if portal.is_none() && self.backdrop => {
                // A sixtieth of a second a frame with no window, as the world's clock steps.
                let step = match self.engine.renderer.frame_seconds {
                    Some(s) => f64::from(s),
                    None => self
                        .drift_last
                        .map_or(0.0, |t| t.elapsed().as_secs_f64().min(0.25)),
                };
                self.drift_last = Some(std::time::Instant::now());
                self.drift_clock += step;
                let drift = self
                    .drift
                    .get_or_insert_with(|| pyreal_engine::drift::Drift::new(&self.gpu));
                drift.render(
                    &self.gpu,
                    self.drift_clock,
                    ui.as_ref(),
                    &view,
                    self.format,
                    size,
                );
            }
            None if portal.is_none() => {
                self.overlay
                    .lay_over(&self.gpu, &view, self.format, ui.as_ref());
            }
            None => {}
        }
        // Portal space on its own thread, which then has the screen; drawn here otherwise.
        let threaded = portal.is_some() && self.inset.is_none() && self.portal_thread.is_some();
        if let Some(f) = portal {
            // On the way in, the world's last picture is swallowed.
            let last = pyreal_engine::portal::Flight::swallowing(&f)
                .then(|| self.engine.renderer.world_picture())
                .flatten()
                .map(|t| t.create_view(&Default::default()));
            let show = pyreal_engine::portal::Show {
                frame: f,
                at: std::time::Instant::now(),
                world: last,
                ui: ui.clone(),
                format: self.format,
                collapse: self.portal_collapse(),
            };
            match self.portal_thread.as_ref().filter(|_| threaded) {
                Some(thread) => thread.show(show),
                None => {
                    let mut space = self
                        .portal
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    space.fov_y = show.fov_at(&f);
                    space.render(
                        &self.gpu,
                        &f,
                        show.world.as_ref(),
                        ui.as_ref(),
                        &view,
                        self.format,
                        size,
                    );
                }
            }
        }
        if !threaded {
            if let Some(thread) = self.portal_thread.as_ref() {
                thread.hide();
            }
        }
        // The pick the runtime asked about, read off this frame once the last one is back.
        if let Some(at) = self
            .look
            .wanted
            .get()
            .filter(|_| !self.engine.renderer.pick_in_flight())
        {
            self.look.wanted.set(None);
            let vp = self.look.viewport.get();
            let (ox, oy) = vp.map_or((0.0, 0.0), |v| {
                #[allow(clippy::cast_precision_loss)]
                (v.x as f32, v.y as f32)
            });
            let px = pyreal_primitives::num::to_i32(at.0 + ox).max(0);
            let py = pyreal_primitives::num::to_i32(at.1 + oy).max(0);
            self.engine.renderer.request_pick(
                &self.gpu,
                (
                    u32::try_from(px).unwrap_or(0),
                    u32::try_from(py).unwrap_or(0),
                ),
            );
            self.pick_at = Some(at);
        }
        if threaded {
            if self
                .portal_thread
                .as_ref()
                .is_some_and(|t| t.take_outdated())
            {
                self.configure();
            }
            // Nothing here waits for the display now: a short rest keeps the loop from spinning,
            // and the loading behind portal space takes the rest.
            const LEAST: std::time::Duration = std::time::Duration::from_millis(4);
            if let Some(gone) = self.paced.map(|t| t.elapsed()) {
                if gone < LEAST {
                    std::thread::sleep(LEAST - gone);
                }
            }
            self.paced = Some(std::time::Instant::now());
        } else {
            self.paced = None;
            self.present(&back);
        }
        if let Some(t) = self.frame_started.take() {
            self.times.cpu_ms = t.elapsed().as_secs_f64() * 1e3;
        }
        if let Some(log) = self.frame_log.as_mut() {
            use std::io::Write;
            let s = self.engine.scene.counts();
            let _ = writeln!(
                log,
                "{} period {:.2} cpu {:.2} gpu {:.2} ui_blur {:.3} composite {:.3} {} instances {} meshes {} vertices {} indices {} textures {} materials {} texture_mb {} largest_vertex_run {}",
                self.times.frames,
                self.times.history.last().map_or(0.0, |h| h.0),
                self.times.cpu_ms,
                self.times.gpu_ms,
                self.ui_blur_ms.0,
                self.ui_blur_ms.1,
                self.frame_note,
                s.instances,
                s.meshes,
                s.vertices_used,
                s.indices_used,
                self.engine.materials.live_textures(),
                self.engine.materials.live_materials(),
                self.engine.materials.texture_bytes() >> 20,
                self.engine.scene.vertices.largest_free(),
            );
        }
        self.times.frames += 1;
        let w = pyreal_engine::frame_work::take();
        let work = &mut self.work;
        work.textures = w.textures;
        work.texture_kb = u32::try_from(w.texture_bytes >> 10).unwrap_or(u32::MAX);
        work.meshes = w.meshes;
        work.pipelines = w.pipelines + w.shaders;
        work.pipeline_ms = w.pipeline_ms;
        work.blas = w.blas;
        work.tlas = w.tlas;
        work.rays_ms = w.rays_ms;
        work.bind_groups = w.bind_groups;
        work.upload_ms = w.upload_ms;
        Ok(())
    }

    fn present(&mut self, back: &wgpu::Texture) {
        let Some(surface) = self.surface.clone() else {
            return;
        };
        let presenting = Arc::clone(&self.presenting);
        let _held = presenting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let t = std::time::Instant::now();
        let got = surface.get_current_texture();
        self.work.gpu_wait_ms += pyreal_engine::frame_work::ms_since(t);
        let t = std::time::Instant::now();
        let frame = match got {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.configure();
                return;
            }
            _ => {
                self.hidden = true;
                return;
            }
        };
        self.hidden = false;
        let mut enc = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("present"),
            });
        let to = &frame.texture;
        if (to.width(), to.height()) == (back.width(), back.height())
            && to.usage().contains(wgpu::TextureUsages::COPY_DST)
        {
            enc.copy_texture_to_texture(back.as_image_copy(), to.as_image_copy(), back.size());
            self.gpu.queue.submit([enc.finish()]);
        } else {
            drop(enc);
            let view = to.create_view(&Default::default());
            self.overlay.blit(&self.gpu, back, &view, self.format);
        }
        self.gpu.queue.present(frame);
        self.work.present_ms += pyreal_engine::frame_work::ms_since(t);
    }

    pub fn window_hidden(&self) -> bool {
        self.hidden
    }

    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        self.inset.map_or(self.size, |i| i.extent)
    }

    /// # Errors
    /// A zero extent.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), DeviceError> {
        if width == 0 || height == 0 {
            return Err(DeviceError(format!(
                "bad dimensions {width}x{height}: a presentation must have a non-zero extent"
            )));
        }
        if let Some(region) = self.inset {
            self.inset = Some(Inset {
                extent: (width, height),
                ..region
            });
            return Ok(());
        }
        self.size = (width, height);
        self.configure();
        Ok(())
    }

    pub fn set_inset(&mut self, region: Option<Inset>) {
        self.inset = region;
    }

    #[must_use]
    pub fn inset(&self) -> Option<Inset> {
        self.inset
    }

    pub fn set_presentation_sync(&mut self, full_screen: bool, sync_to_refresh: bool) {
        let vsync = sync_to_refresh || !full_screen;
        if self.sync != (full_screen, vsync) {
            self.sync = (full_screen, vsync);
            self.configure();
        }
    }

    pub fn wait_idle(&mut self) {
        self.gpu.wait();
    }

    /// The last frame's pixels, tightly packed RGBA rows.
    #[must_use]
    pub fn capture_rgba(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        let back = self.back.as_ref()?;
        if self.format == wgpu::TextureFormat::Rgba16Float {
            // An EDR frame: SDR white and past it as white, sRGB-encoded.
            let half = pyreal_engine::read_texture(&self.gpu, back, 8);
            let px = half
                .as_chunks::<2>()
                .0
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    let v = half_to_f32(u16::from_le_bytes(*h));
                    if i % 4 == 3 {
                        255
                    } else {
                        srgb_byte(v)
                    }
                })
                .collect();
            return Some((back.width(), back.height(), px));
        }
        let mut px = pyreal_engine::read_texture(&self.gpu, back, 4);
        if matches!(
            self.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for p in px.as_chunks_mut::<4>().0 {
                p.swap(0, 2);
            }
        }
        Some((back.width(), back.height(), px))
    }

    /// The last frame's pixels as BGRA rows, for a test that compares two frames.
    #[must_use]
    pub fn capture_bgra(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        let (w, h, mut px) = self.capture_rgba()?;
        for p in px.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
        Some((w, h, px))
    }

    /// Makes preview space `id` if it is not there; whether this made it.
    pub fn ensure_preview(
        &mut self,
        id: PreviewId,
        assets: &Arc<pyreal_world_data::anim_assets::DatAnimAssets>,
    ) -> bool {
        self.previews.ensure(id, assets)
    }

    /// Adds setup `setup` to preview space `id`; its index.
    ///
    /// # Errors
    /// None today: a setup that will not load is `Ok(None)`.
    pub fn add_preview_object(
        &mut self,
        id: PreviewId,
        _store: &RetailDatStore,
        setup: DataId,
    ) -> Result<Option<usize>, DeviceError> {
        Ok(self
            .previews
            .get_mut(id)
            .and_then(|s| s.add_object(setup, None)))
    }

    #[must_use]
    pub fn preview(&self, id: PreviewId) -> Option<&crate::previews::Space> {
        self.previews.get(id)
    }

    pub fn preview_mut(&mut self, id: PreviewId) -> Option<&mut crate::previews::Space> {
        self.previews.get_mut(id)
    }

    /// The last frame as a PNG at `path`.
    ///
    /// # Errors
    /// No frame drawn yet, or the file.
    pub fn capture_png(&mut self, path: &Path) -> Result<(), DeviceError> {
        let (w, h, rgba) = self
            .capture_rgba()
            .ok_or_else(|| DeviceError("no frame has been drawn".into()))?;
        let io = |e: std::io::Error| DeviceError(format!("{}: {e}", path.display()));
        let file = std::fs::File::create(path).map_err(io)?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut out = enc
            .write_header()
            .map_err(|e| DeviceError(format!("png header: {e}")))?;
        out.write_image_data(&rgba)
            .map_err(|e| DeviceError(format!("png data: {e}")))?;
        Ok(())
    }

    // -------------------------------------------------------------------------------------------
    // the world's lifetime
    // -------------------------------------------------------------------------------------------

    /// Builds the world (the runtime's) and the engine's view of it.
    ///
    /// # Errors
    /// The world's own.
    pub fn load_world(
        &mut self,
        store: &Arc<RetailDatStore>,
        cfg: pyreal_client_runtime::scene::SceneConfig,
        world: &mut Option<WorldState>,
    ) -> Result<(), pyreal_world_data::landblock::WorldError> {
        // No world is drawn without the start-up packs.
        self.finish_opening();
        self.sim.load_world(store, cfg, world)?;
        self.use_store(store);
        let region = pyreal_world_data::landblock::load_region(store)?;
        self.apply_settings();
        let reach = self
            .applied
            .as_ref()
            .map_or_else(Reach::default, |s| s.reach);
        let mut bridge = Bridge::new(Arc::clone(store), region, reach);
        bridge.smoothing = self.applied.as_ref().map_or(
            pyreal_client_contract::options::landscape_smoothing::DEFAULT_STRENGTH,
            |s| s.smoothing,
        );
        // With no window (a capture), each frame waits for its land; a measured flight does not.
        bridge.settle = self.surface.is_none() && self.fly.is_none();
        self.bridge = Some(bridge);
        self.first_pixel = false;
        Ok(())
    }

    pub fn release_world(&mut self, world: &mut Option<WorldState>) -> u32 {
        if self.bridge.take().is_some() {
            self.engine.clear_world(&mut self.loader);
        }
        self.sim.release_world(world)
    }

    /// The data files the preview spaces draw from while no world is built.
    pub fn use_data_files(&mut self, store: &Arc<RetailDatStore>) {
        self.use_store(store);
    }

    /// Whether the portal lights are drawn behind the screens before the world, each frame.
    pub fn set_backdrop(&mut self, on: bool) {
        if !on {
            self.drift_last = None;
        }
        self.backdrop = on;
    }

    /// The teleport's moment, each frame.
    pub fn set_portal_look(&mut self, look: Option<pyreal_client_contract::teleport::PortalLook>) {
        // Behind portal space the landblocks are built a few milliseconds a frame.
        self.sim.stream_budget = (look.is_some() && self.windowed()).then_some(4.0);
        if look.is_some() && self.portal_look.is_none() {
            self.portal_from_world = self.world_drawn_last;
            self.portal_colour = look
                .and_then(|l| l.portal_setup)
                .and_then(|setup| self.engine.particles.portal_colour(setup));
        }
        self.portal_look = look;
    }

    /// Whether the device draws to a window.
    #[must_use]
    pub fn windowed(&self) -> bool {
        self.surface.is_some()
    }

    /// Whether the destination of a jump is still being placed behind portal space.
    #[must_use]
    pub fn loading_destination(&self) -> bool {
        self.portal_look.is_some()
            && self
                .bridge
                .as_ref()
                .is_some_and(crate::bridge::Bridge::jumping)
    }

    /// Whether the engine draws portal space here.
    #[must_use]
    pub fn draws_portal_space(&self) -> bool {
        self.portal_enhanced && pyreal_client_contract::options::portal_style::enhanced()
    }

    /// The tunnel's field of view over its last second, degrees: the teleport's own collapse, from
    /// the game's view to the collapsed one on the animation's ease, never narrower than the
    /// tunnel's.
    fn portal_collapse(&self) -> [f32; pyreal_engine::portal::COLLAPSE_SAMPLES] {
        use pyreal_client_contract::teleport::timing;
        let (w, h) = self.size;
        let game = pyreal_client_contract::camera::view_distance_from_fov(self.look.fov_y(w, h));
        let levels = pyreal_client_contract::media::level_array();
        std::array::from_fn(|i| {
            #[allow(clippy::cast_precision_loss)]
            let t = i as f32 / (pyreal_engine::portal::COLLAPSE_SAMPLES - 1) as f32;
            let a = f32::from(pyreal_client_contract::media::anim_level(&levels, t)) / 1024.0;
            let d = (timing::COLLAPSED_VIEW_DISTANCE - game) * a + game;
            pyreal_client_contract::camera::set_vdst(d)
                .0
                .to_degrees()
                .max(pyreal_engine::portal::FOV_Y)
        })
    }

    /// The moment portal space draws, while the engine draws it.
    fn portal_frame(&self) -> Option<pyreal_engine::portal::PortalFrame> {
        use pyreal_client_contract::teleport::TeleportAnimState as S;
        use pyreal_engine::portal::{Leg, Palette, PortalFrame};
        if !self.draws_portal_space() {
            return None;
        }
        let look = self.portal_look?;
        let leg = match look.state {
            // The world folds away and opens out again as the original's does.
            S::Off | S::WorldFadeOut | S::WorldFadeIn => return None,
            S::TunnelFadeIn => Leg::TunnelIn,
            S::Tunnel => Leg::Tunnel,
            S::TunnelContinue => Leg::Continue,
            S::TunnelFadeOut => Leg::TunnelOut,
        };
        #[allow(clippy::cast_possible_truncation)]
        Some(PortalFrame {
            leg,
            phase: look.phase as f32,
            travelled: look.travelled as f32,
            seed: look.seed,
            palette: {
                use pyreal_client_contract::teleport::PortalSource as Src;
                match look.source {
                    Src::Portal => self
                        .portal_colour
                        .map_or(Palette::PORTAL, |c| Palette::PORTAL.toward(c)),
                    Src::Login => Palette::LOGIN,
                    Src::Lifestone => Palette::LIFESTONE,
                    Src::House => Palette::HOUSE,
                    Src::Recall => Palette::RECALL,
                    Src::LogOff => Palette::LOGOFF,
                }
            },
            from_world: self.portal_from_world,
        })
    }

    pub fn set_world_view_state(&mut self, hidden: bool, view_distance: Option<f32>) {
        self.world_hidden = hidden;
        self.view_distance = view_distance;
        self.sim.set_world_view_state(hidden, view_distance);
    }

    /// The setups of the objects the stream holds, for their lamps.
    pub fn note_objects(&mut self, stream: &pyreal_client_runtime::objects::ObjectStream) {
        let Some(b) = self.bridge.as_mut() else {
            return;
        };
        let mut held = 0;
        for (id, p) in stream.presences() {
            held += 1;
            if let Some(setup) = p.setup_id {
                b.setups.entry(id).or_insert(setup);
            }
        }
        // Objects the stream no longer holds are let go.
        if b.setups.len() > held {
            b.setups.retain(|id, _| stream.presence(*id).is_some());
        }
    }

    // -------------------------------------------------------------------------------------------
    // the overlay's pictures
    // -------------------------------------------------------------------------------------------

    /// Uploads an overlay picture: a resident `Image` or `Glyphs` key is a hit, a `Local` one is
    /// replaced.
    pub fn overlay_upload(
        &mut self,
        texture: OverlayTexture,
        data: &pyreal_primitives::TextureData,
    ) {
        use pyreal_client_contract::overlay::OverlaySpace;
        if let Some(&key) = self.overlay_keys.get(&texture) {
            if texture.space != OverlaySpace::Local && self.overlay.contains(key) {
                return;
            }
        }
        let Some(levels) = rgba_levels(data) else {
            tracing::warn!("an overlay picture of {:?} would not decode", data.format);
            return;
        };
        let key = match self.overlay_keys.get(&texture) {
            Some(&k) => k,
            None => {
                let k = self.next_key;
                self.next_key += 1;
                self.overlay_keys.insert(texture, k);
                k
            }
        };
        self.overlay
            .upload(&self.gpu, key, data.width, data.height, &levels);
    }

    pub fn overlay_release(&mut self, texture: OverlayTexture) -> OverlayReleased {
        match self.overlay_keys.remove(&texture) {
            Some(k) if self.overlay.release(k) => OverlayReleased::Freed,
            Some(_) => OverlayReleased::Unknown,
            None => OverlayReleased::Absent,
        }
    }

    pub fn set_ground_markers(&mut self, markers: &[GroundMarker]) {
        self.ground_markers = markers.to_vec();
    }

    /// The waypoint's ring round the player for the frames until the next call; `None` takes it
    /// away.
    pub fn set_waypoint_ring(
        &mut self,
        ring: Option<pyreal_client_contract::overlay::WaypointRing>,
    ) {
        self.waypoint_ring = ring;
    }

    /// Decodes retail surface `id` and shows it over the whole frame while no world is drawn.
    ///
    /// # Errors
    /// The surface is missing or will not decode.
    pub fn load_first_pixel_scene(
        &mut self,
        assets: &dyn pyreal_primitives::AssetSource,
        id: DataId,
    ) -> Result<(), DeviceError> {
        let (w, h, rgba) = crate::first_pixel::decode(assets, id).map_err(DeviceError)?;
        self.overlay
            .upload(&self.gpu, FIRST_PIXEL_KEY, w, h, &[rgba]);
        self.first_pixel = true;
        Ok(())
    }

    // -------------------------------------------------------------------------------------------
    // what the interface asks of the world as drawn
    // -------------------------------------------------------------------------------------------

    /// Where render-space point `p` (the client's) stands on screen and its depth (0 near, 1
    /// far, as `z / w`); `None` behind the camera.
    fn screen_point(&self, ws: &WorldState, p: glam::Vec3) -> Option<(f32, f32, f32)> {
        let (w, h) = self.size;
        #[allow(clippy::cast_precision_loss)]
        let (fw, fh) = (w as f32, h as f32);
        let fov = self.look.fov_y(w, h);
        let f = ws.camera.frame();
        let cam = crate::bridge::camera(&f, glam::Vec3::ZERO, fov, 1000.0, None);
        let view = cam.view();
        let proj = glam::Mat4::perspective_rh(fov, fw / fh.max(1.0), MARK_NEAR, MARK_FAR);
        let q = proj * view * p.extend(1.0);
        if q.w < 0.0002 {
            return None;
        }
        Some((
            (q.x / q.w) * fw * 0.5 + fw * 0.5,
            fh * 0.5 - (q.y / q.w) * fh * 0.5,
            q.z / q.w,
        ))
    }

    /// Where an object's selection sphere stands on screen, for the target reticule.
    #[must_use]
    pub fn target_projection(
        &self,
        id: ObjectId,
        ws: Option<&WorldState>,
    ) -> Option<pyreal_client_contract::target::Projection> {
        use pyreal_client_contract::target::Projection;
        if self.world_hidden {
            return None;
        }
        let ws = ws?;
        let object = ws.objects.get(&id).filter(|o| o.drawn)?;
        let (center, radius) = {
            let driver = object.sim.driver.borrow();
            let parts = &driver.part_array;
            parts
                .setup
                .as_ref()
                .map_or((glam::Vec3::new(0.0, 0.0, 0.1), 0.1), |s| {
                    let c = s.selection_sphere.center;
                    (
                        glam::Vec3::new(
                            c.x * parts.scale.x,
                            c.y * parts.scale.y,
                            c.z * parts.scale.z,
                        ),
                        s.selection_sphere.radius * parts.scale.z,
                    )
                })
        };
        let at = pyreal_engine::asset::object::frame_affine(&object.frame);
        let world_center = at.transform_point3(center);
        let f = ws.camera.frame();
        let cam = crate::bridge::camera(&f, glam::Vec3::ZERO, 1.0, 1000.0, None);
        let right = cam.right();
        let up = right.cross(cam.forward()).normalize_or(glam::Vec3::Z);
        let a = world_center - right * radius + up * radius;
        let b = world_center + right * radius - up * radius;
        let to = world_center - cam.position;
        if to.dot(cam.forward()) > radius {
            if let (Some(a), Some(b)) = (self.screen_point(ws, a), self.screen_point(ws, b)) {
                let px = |v: f32| pyreal_primitives::num::to_i32(v);
                return Some(Projection::OnScreen((px(a.0), px(a.1), px(b.0), px(b.1))));
            }
        }
        // Off screen: the bearing in the camera's own frame.
        let local = glam::Vec3::new(to.dot(right), to.dot(cam.forward()), to.dot(up));
        let z = if local.y <= 0.0 { 0.0 } else { local.z };
        let degrees = 450.0 - pyreal_primitives::num::math::atan2f(z, local.x).to_degrees();
        Some(Projection::OffScreen(degrees % 360.0))
    }

    /// Where an object's own origin stands on screen, at its selection sphere's height.
    #[must_use]
    pub fn target_origin(&self, id: ObjectId, ws: Option<&WorldState>) -> Option<(i32, i32)> {
        if self.world_hidden {
            return None;
        }
        let ws = ws?;
        let object = ws.objects.get(&id)?;
        let height = {
            let driver = object.sim.driver.borrow();
            let parts = &driver.part_array;
            parts
                .setup
                .as_ref()
                .map_or(0.1, |s| s.selection_sphere.center.z * parts.scale.z)
        };
        let at = pyreal_engine::asset::object::frame_affine(&object.frame);
        let p = at.transform_point3(glam::Vec3::new(0.0, 0.0, height));
        let (x, y, _) = self.screen_point(ws, p)?;
        Some((
            pyreal_primitives::num::to_i32(x),
            pyreal_primitives::num::to_i32(y),
        ))
    }

    /// Where the point a name over an object stands on screen; no depth (the engine's is its own).
    #[must_use]
    pub fn target_top(
        &self,
        id: ObjectId,
        ws: Option<&WorldState>,
    ) -> Option<(f32, f32, Option<f32>)> {
        if self.world_hidden {
            return None;
        }
        let ws = ws?;
        let top = ws.name_top(id)?;
        let (x, y, depth) = self.screen_point(ws, glam::Vec3::new(top.x, top.y, top.z))?;
        // The name stands at its top's depth: hidden where a wall or a hill stands nearer.
        Some((x, y, Some(depth)))
    }

    // -------------------------------------------------------------------------------------------
    // the report
    // -------------------------------------------------------------------------------------------

    /// How the frame is drawn, for `@report`.
    #[must_use]
    pub fn render_context(
        &self,
        ws: Option<&WorldState>,
    ) -> pyreal_client_runtime::report::RenderContext {
        let _ = ws;
        let info = self.gpu.adapter.get_info();
        let prefs = self.render_preferences();
        pyreal_client_runtime::report::RenderContext {
            renderer: "engine".into(),
            adapter: info.name,
            driver: info.driver,
            present_mode: self
                .config
                .as_ref()
                .map_or_else(|| "offscreen".into(), |c| format!("{:?}", c.present_mode)),
            better_lighting: true,
            effects: crate::settings::effects_text(&prefs.fidelity),
            art_pack: self.art_pack(),
            render_scale: u32::try_from(pyreal_primitives::num::to_i32(
                self.engine.renderer.render_scale * 100.0,
            ))
            .unwrap_or(100),
            knobs_hash: None,
            changed_knobs: Vec::new(),
            drawn_eye: None,
        }
    }

    /// What the last frame's passes took on the GPU.
    #[must_use]
    pub fn present_costs(&self) -> Option<pyreal_client_runtime::report::PresentCosts> {
        #[allow(clippy::cast_possible_truncation)]
        Some(pyreal_client_runtime::report::PresentCosts {
            better_lighting: self.times.cpu_ms as f32,
            work: Some(self.work),
            ..pyreal_client_runtime::report::PresentCosts::default()
        })
    }

    /// The share of the frame's pixels the world is drawn at, percent.
    #[must_use]
    pub fn render_scale_in_use(&self) -> Option<u32> {
        let (iw, _) = self.engine.renderer.internal;
        let (ow, _) = self.engine.renderer.output_size;
        (ow > 0).then(|| iw * 100 / ow)
    }

    /// The scene as one read-only view.
    pub fn scene<'a>(&'a self, world: Option<&'a WorldState>) -> Option<Box<dyn Scene + 'a>> {
        let inner = self.sim.scene(world)?;
        Some(Box::new(View::<dyn Scene + 'a> {
            inner,
            look: &self.look,
            size: self.size,
        }))
    }

    /// …and writable.
    pub fn scene_mut<'a>(
        &'a mut self,
        world: Option<&'a mut WorldState>,
    ) -> Option<Box<dyn SceneMut + 'a>> {
        let size = self.size;
        let Self { sim, look, .. } = self;
        let inner = sim.scene_mut(world)?;
        Some(Box::new(View::<dyn SceneMut + 'a> { inner, look, size }))
    }

    /// The device-owned preferences: the screen's brightness, taken as the exposure's
    /// compensation.
    pub fn apply_device_preference_requests(
        &mut self,
        requests: Vec<pyreal_client_contract::UiRequest>,
    ) -> Vec<pyreal_client_contract::UiRequest> {
        use pyreal_client_contract::{PrefValue, UiRequest};
        requests
            .into_iter()
            .filter(|r| match r {
                UiRequest::SetPreference(name, PrefValue::Float(v))
                    if name.eq_ignore_ascii_case(
                        pyreal_client_runtime::render_prefs::SCREEN_BRIGHTNESS,
                    ) =>
                {
                    self.set_brightness(*v);
                    false
                }
                _ => true,
            })
            .collect()
    }

    /// The screen's brightness, `-0.2` to `1`: stops over the exposure's key.
    pub fn set_brightness(&mut self, brightness: f32) {
        if let Some(e) = fixed_exposure() {
            self.engine.renderer.exposure = pyreal_engine::renderer::Exposure::Manual(e);
            return;
        }
        let b = brightness.clamp(-0.2, 1.0);
        self.engine.renderer.exposure = pyreal_engine::renderer::Exposure::Auto {
            compensation: b * 1.5,
        };
    }

    /// The device's and the world's preferences, applied.
    pub fn apply_render_preference_requests(
        &mut self,
        requests: Vec<pyreal_client_contract::UiRequest>,
    ) -> Vec<pyreal_client_contract::UiRequest> {
        let rest = self.apply_device_preference_requests(requests);
        let rest = match self.sim.config_mut() {
            Some(cfg) => {
                crate::render_prefs::apply_scene_preference_requests(&mut cfg.render, rest)
            }
            None => crate::render_prefs::apply_scene_preference_requests(
                &mut self.look.prefs.borrow_mut(),
                rest,
            ),
        };
        self.apply_settings();
        rest
    }

    /// The preferences polled each frame.
    pub fn update_render_preferences(&mut self) {
        self.apply_settings();
    }
}

/// A texture's levels as tightly packed RGBA8 rows.
pub(crate) fn rgba_levels(data: &pyreal_primitives::TextureData) -> Option<Vec<Vec<u8>>> {
    use pyreal_codecs::pixel_format::PixelFormatId as P;
    use pyreal_primitives::TextureFormat as F;
    let mut out = Vec::with_capacity(data.levels.len());
    for (k, level) in data.levels.iter().enumerate() {
        let k = u32::try_from(k).unwrap_or(0);
        let (w, h) = ((data.width >> k).max(1), (data.height >> k).max(1));
        let bgra = match data.format {
            F::Bgra8 => level.clone(),
            F::Bc1 => pyreal_codecs::dxt::decode(P::Dxt1, level, w, h).ok()?,
            F::Bc2 | F::Bc2Premultiplied => {
                pyreal_codecs::dxt::decode(P::Dxt3, level, w, h).ok()?
            }
            F::Bc3 | F::Bc3Premultiplied => {
                pyreal_codecs::dxt::decode(P::Dxt5, level, w, h).ok()?
            }
            _ => return None,
        };
        let mut rgba = bgra;
        for p in rgba.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
        out.push(rgba);
    }
    Some(out)
}

/// The weather's sound from the weather the engine drew last: its rain (none of its snow), the
/// storm and its gusts, and whether the eye is indoors. A strike of lightning is heard as thunder
/// one to five seconds later, a peal and a loudness of its own.
fn hear_weather(
    w: &pyreal_engine::weather::Weather,
    clock: f64,
    indoors: bool,
    due: &mut Vec<(f64, u32, f32)>,
    last_flash: &mut f32,
    look: &Look,
) {
    let storm = w.storminess();
    look.weather_sound.set(pyreal_audio::WeatherSound {
        rain: if w.on { w.rain.clamp(0.0, 1.0) } else { 0.0 },
        storm,
        gust: (w.gust() * 0.5 + 0.5) * storm,
        indoors: if indoors { 1.0 } else { 0.0 },
        gain: 0.0,
    });
    if w.flash > 0.3 && *last_flash <= 0.3 {
        let h = clock.to_bits().wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32;
        #[allow(clippy::cast_precision_loss)] // under a thousand
        let delay = 1.0 + (h % 1000) as f64 / 250.0;
        #[allow(clippy::cast_possible_truncation)] // under six
        let peal = ((h / 1000) % 6) as u32;
        due.push((clock + delay, peal, (0.6 + 0.4 * storm).min(1.0)));
    }
    *last_flash = w.flash;
    let mut heard = look.thunder.borrow_mut();
    due.retain(|&(at, peal, volume)| {
        if at <= clock {
            heard.push(pyreal_client_runtime::audio::SoundTrigger::Thunder { peal, volume });
            false
        } else {
            true
        }
    });
}

/// The land's streaming as `bridge` has it after this frame's prepare, into `work`.
fn note_stream(
    work: &mut pyreal_client_runtime::report::PresentWork,
    bridge: &crate::bridge::Bridge,
) {
    let Some(stream) = bridge.stream.as_ref() else {
        return;
    };
    let s = &stream.stats;
    let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
    work.retarget_ms = s.frame_retarget_ms;
    work.load_ms = s.frame_load_ms;
    work.load_wait_ms = s.frame_wait_ms;
    work.place_ms = s.frame_place_ms;
    work.switch_ms = s.frame_switch_ms;
    work.evict_ms = s.frame_evict_ms;
    work.steps = s.frame_steps;
    work.centre = stream.centre();
    work.target = stream.target();
    work.far_centre = stream.far_centre();
    work.far_asked = stream.far_asked();
    work.blocks = n(stream.blocks().count());
    work.pending = n(s.pending);
    work.asked = n(s.asked);
    work.planned = n(s.planned);
    work.queued = n(s.queued);
    work.trees_asked = n(s.trees_asked);
    work.full_loads = s.full_loads;
    work.switches = s.switches;
    work.rebases = s.rebases;
    work.evictions = s.evictions;
    work.blocks_placed = s.blocks_placed;
    work.blocks_removed = s.blocks_removed;
    work.last_step = s.last_step();
    #[allow(clippy::cast_possible_truncation)]
    {
        work.worst_step_ms = s.worst_step_ms as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::{alpha_mode, output_format};
    use wgpu::{
        SurfaceCapabilities, SurfaceColorSpaces as Cs, SurfaceFormatCapabilities,
        TextureFormat as F,
    };

    /// Capabilities listing `formats`, each with its colour spaces.
    fn caps(formats: &[(F, Cs)]) -> SurfaceCapabilities {
        SurfaceCapabilities {
            formats: formats.iter().map(|f| f.0).collect(),
            format_capabilities: formats
                .iter()
                .map(|&(format, color_spaces)| SurfaceFormatCapabilities {
                    format,
                    color_spaces,
                })
                .collect(),
            ..SurfaceCapabilities::default()
        }
    }

    #[test]
    fn hdr_takes_the_float_format_in_extended_linear_srgb() {
        let c = caps(&[
            (F::Bgra8UnormSrgb, Cs::SRGB),
            (F::Rgba16Float, Cs::SRGB | Cs::EXTENDED_SRGB_LINEAR),
        ]);
        assert_eq!(output_format(&c, true), F::Rgba16Float);
        assert_eq!(output_format(&c, false), F::Bgra8UnormSrgb);
    }

    /// Vulkan lists fp16 where only sRGB is supported for it (system HDR off): drawn as SDR.
    #[test]
    fn hdr_without_extended_linear_srgb_takes_the_srgb_format() {
        let c = caps(&[(F::Bgra8UnormSrgb, Cs::SRGB), (F::Rgba16Float, Cs::SRGB)]);
        assert_eq!(output_format(&c, true), F::Bgra8UnormSrgb);
    }

    #[test]
    fn hdr_without_a_float_format_takes_the_srgb_format() {
        let c = caps(&[(F::Bgra8Unorm, Cs::SRGB), (F::Rgba8UnormSrgb, Cs::SRGB)]);
        assert_eq!(output_format(&c, true), F::Rgba8UnormSrgb);
    }

    #[test]
    fn alpha_is_opaque_when_offered_and_the_first_offered_otherwise() {
        use wgpu::CompositeAlphaMode as A;
        let with = |modes: &[A]| SurfaceCapabilities {
            alpha_modes: modes.to_vec(),
            ..SurfaceCapabilities::default()
        };
        assert_eq!(
            alpha_mode(&with(&[A::PostMultiplied, A::Opaque])),
            A::Opaque
        );
        assert_eq!(
            alpha_mode(&with(&[A::Inherit, A::PreMultiplied])),
            A::Inherit
        );
        assert_eq!(alpha_mode(&with(&[])), A::Auto);
    }
}
