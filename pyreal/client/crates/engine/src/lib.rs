//! The new renderer's engine: dat assets converted into GPU-resident pools and a bindless material
//! table, culled and drawn by the GPU through a frame graph of deferred passes.
//!
//! **Depends on** `pyreal-primitives`, `pyreal-dat` and `pyreal-assets` (the records),
//! `pyreal-terrain` and `pyreal-world-data` (the landscape's meshes, scenery, landblock ids and rooms),
//! `pyreal-world-render` (the sky's time-of-day light), `pyreal-animation` (a part's detail level,
//! and the motion driver that poses a body), `pyreal-codecs` (the texture codecs) and
//! `pyreal-art-pack` (the art pack, opened by the client's own reader). **Used by** the client's
//! drawn world (`pyreal-scene`, whose bridge hands it the runtime's world) and the engine viewer
//! (`pyreal-engine-view`).
//!
//! **Must never** reach the client runtime (`cargo xtask seams`, `seam: client crates`): it takes
//! the world as records (dat ids, transforms, object numbers, the client's own motion drivers),
//! not as the runtime's types.
//!
//! | Module | What it owns |
//! |---|---|
//! | [`asset`] | dat records to meshes, pictures and material descriptions, with no device |
//! | [`pool`], [`scene`] | the shared vertex, index and instance pools; the bucket layout |
//! | [`materials`] | the bindless texture array, the material records and ground pictures' companions |
//! | [`pack`] | the art pack: its materials' maps, its models in place of retail objects, its ground |
//! | [`terrain`], [`world`] | the ground's cells and layers; loading an area of landblocks |
//! | [`drawn`] | Landscape Smoothing: the drawn ground's offset, the scenery moved by it |
//! | [`cells`] | rooms' volumes and portals, the portal walk, the ground's cut-outs |
//! | [`far`] | the far terrain: CDLOD rings past the exact cells |
//! | [`graph`] | the frame graph: declared passes, run in order, timed per pass |
//! | [`renderer`] | the ground's depth pyramid, the cull pass, G-buffer, deferred lighting, translucency and tone map |
//! | [`post`] | auto-exposure, bloom and the light's calibration (physical or the client's) |
//! | [`body`] | animated bodies: parts posed by the client's animation code, as dynamic instances |
//! | [`camera`], [`sky`] | the view and the light of a time of day |
//! | [`rt`] | the world's ray-tracing structures, for the lamps' shadows and the bounce light's probes |
//! | [`lamps`] | the lamps from the dats' light data and the pack's, chosen per frame for the clusters |
//! | [`shadows`] | the sun's cascades: their splits and stable fit |
//! | [`atmosphere`], [`heavens`] | the sky's tables and the air; the moons and clouds from the sky objects |
//! | [`land`], [`water`] | the land field the GPU reads beside the ground; the water's surface, looks and ripples |
//! | [`wind`] | the breeze that bends growth and drives the water |
//! | [`webs`] | cobweb cards traced into fine silk, stirred by the bodies moving near |
//! | [`cover`] | the pack's ground cover, placed on the GPU round the eye and drawn by a few draws |
//! | [`particles`] | scripted objects' emitters, stepped by the client's own code, drawn as instanced cards |
//! | [`bolts`] | lightning in a creature's cloud: grown channels, their lines of light and lamps |
//! | [`glow`] | creatures' light: their balls of light and lens stars drawn as light, and the lamp each carries |
//! | [`temporal`], [`metalfx`] | the render scale (fixed or dynamic), the jitter, the temporal resolve and upscalers, the UI composite |
//! | [`overlay`] | a client's interface drawn as textured triangles into the UI's picture |
//! | [`portal`] | portal space: the tunnel flown through, drawn and shown on a thread of its own |
//! | [`preview`] | small scenes of pool meshes with their own camera and light, for the UI |

pub mod air;
pub mod asset;
pub mod atmosphere;
pub mod body;
pub mod body_fire;
pub mod bolts;
pub mod camera;
pub mod cells;
pub mod clouds;
pub mod cover;
pub mod drawn;
pub mod drift;
pub mod evict;
pub mod far;
pub mod flames;
pub mod glow;
pub mod gpu;
pub mod graph;
pub mod ground_tint;
pub mod heavens;
pub mod jobs;
pub mod lamps;
pub mod land;
pub mod materials;
pub mod metalfx;
pub mod overlay;
pub mod pack;
pub mod particles;
pub mod pipeline_cache;
pub mod pool;
pub mod portal;
pub mod post;
pub mod preview;
pub mod puffs;
pub mod renderer;
pub mod rime;
pub mod roles;
pub mod rt;
pub mod scene;
pub mod shadows;
pub mod shore;
pub mod skin;
pub mod sky;
pub mod snow;
pub mod stream;
pub mod temporal;
pub mod terrain;
pub mod water;
pub mod weather;
pub mod webs;
pub mod wind;
pub mod world;

pub use camera::Camera;
pub use gpu::Gpu;
pub use renderer::Renderer;
pub use scene::{PoolSizes, Scene};
pub use sky::SkyLight;

use glam::Vec4;

/// Anything the engine can refuse.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("graphics device: {0}")]
    Device(String),
    #[error("the {0} is full ({1} more wanted)")]
    PoolFull(&'static str, u64),
    #[error("frame graph: {0}")]
    Graph(graph::GraphError),
}

/// The engine: the device, the retained scene, the material table and the passes.
#[derive(Debug)]
pub struct Engine {
    pub gpu: Gpu,
    pub scene: Scene,
    pub materials: materials::MaterialTable,
    pub renderer: Renderer,
    /// The rooms and their portals.
    pub cells: cells::CellGraph,
    /// What the portal walk saw last frame.
    pub visibility: cells::Visibility,
    /// The far terrain, when loaded.
    pub far: Option<far::FarTerrain>,
    /// The region's sky objects, when loaded.
    pub heavens: Option<heavens::Heavens>,
    /// Every placed object's lamps.
    pub lamps: Vec<lamps::Lamp>,
    /// Where the bodies near the eye stand this frame (their feet, render space): they stir the
    /// water and push the grass aside.
    pub bodies: Vec<glam::Vec3>,
    /// Where the shadows stand this frame (their feet, render space): each pools darkness.
    pub umbrae: Vec<glam::Vec3>,
    /// What the solid objects standing on the land clear of cover this frame: each one's foot
    /// (x, y, render space) and radius ([`cover::clearing_radius`]).
    pub clearings: Vec<glam::Vec3>,
    /// The loaded area's land field, when an area is loaded.
    pub land: Option<land::LandField>,
    /// Where the snowy land's share was last worked out, and what it was.
    snow_share_at: Option<(glam::Vec3, u32, f32)>,
    /// The scripted objects and their particles.
    pub particles: particles::Particles,
    /// The flames drawn where objects burn.
    pub flames: flames::Flames,
}

impl Engine {
    /// An engine on `gpu` with pools of `sizes`, drawing to `output_format`.
    ///
    /// # Errors
    /// A pool too large for the device, or a frame graph that does not validate.
    pub fn new(
        gpu: Gpu,
        sizes: PoolSizes,
        output_format: wgpu::TextureFormat,
    ) -> Result<Self, EngineError> {
        let scene = Scene::new(&gpu, sizes)?;
        let materials = materials::MaterialTable::new(&gpu);
        let renderer = Renderer::new(&gpu, &scene, &materials, output_format)?;
        Ok(Self {
            gpu,
            scene,
            materials,
            renderer,
            cells: cells::CellGraph::default(),
            visibility: cells::Visibility::everything(1),
            far: None,
            heavens: None,
            lamps: Vec::new(),
            bodies: Vec::new(),
            umbrae: Vec::new(),
            clearings: Vec::new(),
            land: None,
            snow_share_at: None,
            particles: particles::Particles::default(),
            flames: flames::Flames::default(),
        })
    }

    /// Takes the placed world out (instances, ground, rooms, far terrain, lamps, land, particles'
    /// hosts), keeping the meshes, pictures and materials loaded and the sky: an area is loaded
    /// again from nothing, round another centre, and finds its meshes there.
    pub fn clear_world(&mut self, loader: &mut world::AssetLoader) {
        self.scene.clear_world();
        loader.clear_world();
        self.cells = cells::CellGraph::default();
        self.visibility = cells::Visibility::everything(1);
        self.far = None;
        self.lamps.clear();
        self.flames.placed.clear();
        self.bodies.clear();
        self.umbrae.clear();
        self.clearings.clear();
        self.land = None;
        self.renderer.trails.clear();
        self.particles.hosts.clear();
        self.particles.external.clear();
        self.snow_share_at = None;
        self.renderer.water.sim = water::Sim::default();
        self.renderer.water.area = None;
        self.renderer.temporal.reset();
    }

    /// Moves everything placed by `delta` (render space's origin moved by `-delta`): instances,
    /// rooms and their cut volumes, lamps, particles, the far terrain and the land field. The
    /// histories that remember where things were (the temporal one, the probes, the water's
    /// ripples, the rays' top level) start again.
    pub fn translate(&mut self, loader: &mut world::AssetLoader, delta: glam::Vec3) {
        self.scene.translate(delta);
        self.cells.translate(delta);
        loader.translate_cuts(self, delta);
        for l in &mut self.lamps {
            // Its flicker's seed, taken from where it stood, stays its flame's.
            if l.seed == 0 {
                l.seed = flames::seed_of(l.position);
            }
            l.position += delta;
        }
        for f in &mut self.flames.placed {
            f.base += delta;
        }
        self.snow_share_at = None;
        let by = pyreal_primitives::Vec3::new(delta.x, delta.y, delta.z);
        for h in &mut self.particles.hosts {
            h.frame.origin = pyreal_primitives::Vec3::new(
                h.frame.origin.x + delta.x,
                h.frame.origin.y + delta.y,
                h.frame.origin.z + delta.z,
            );
            h.driver.particles.translate(by);
        }
        for b in self.bodies.iter_mut().chain(&mut self.umbrae) {
            *b += delta;
        }
        self.renderer.trails.translate(delta);
        for c in &mut self.clearings {
            *c += delta.truncate().extend(0.0);
        }
        if let Some(f) = self.far.as_mut() {
            f.translate(&self.gpu, delta);
        }
        if let Some(l) = self.land.as_mut() {
            l.origin += delta.truncate();
            for f in &mut l.footprints {
                f.at += delta.truncate();
            }
            for c in &mut l.clearings {
                *c += delta.truncate().extend(0.0).extend(0.0);
            }
            self.renderer.set_land(&self.gpu, l);
        }
        self.renderer.temporal.reset();
        self.renderer.probe_frames = 0;
        self.renderer.ao_frames = 0;
        self.renderer.water.sim = water::Sim::default();
        self.renderer.water.area = None;
        self.renderer.rays.invalidate();
    }

    /// The water's drawn surface at render-space `p` for an eye at `eye` (its level and its
    /// waves' height there), when the water is drawn as water and the land field holds it there.
    #[must_use]
    pub fn water_surface_at(&self, p: glam::Vec2, eye: glam::Vec3) -> Option<f32> {
        let r = &self.renderer;
        if r.calibration != renderer::Calibration::Physical || !r.water_on {
            return None;
        }
        let land = self.land.as_ref()?;
        let waves = water::wave_constants(r.clock, r.world_origin);
        let table = water::WaterTable::default();
        #[allow(clippy::cast_precision_loss)]
        let band = water::FINEST * (1u32 << r.water.finest.min(water::LEVELS - 1)) as f32;
        let at = water::HeightInputs {
            waves: &waves,
            shore: water::shore_phases(r.clock),
            table: &table,
            shore_table: &r.water.shore_table,
            gust: r.water.gust,
            eye,
            reach: water::height_reach(r.water.finest),
            band,
            global: r.world_origin,
        };
        water::height_at(land, &at, p)
    }

    /// How far the waves lift the water over its level where each body stands, for an eye at
    /// `eye` (its troughs held there): nought away from the water and past the ripples' reach.
    fn water_lifts(&self, eye: glam::Vec3) -> Vec<f32> {
        self.bodies
            .iter()
            .map(|b| {
                let p = b.truncate();
                if (p - eye.truncate()).length() > 40.0 {
                    return 0.0;
                }
                let level = self.land.as_ref().and_then(|l| l.water_at(p)).map(|w| w.0);
                match (self.water_surface_at(p, eye), level) {
                    (Some(s), Some(l)) => (s - l).max(0.0),
                    _ => 0.0,
                }
            })
            .collect()
    }

    /// Uploads whatever changed since the last frame.
    ///
    /// # Errors
    /// The scene's layout does not fit its pools.
    pub fn upload(&mut self) -> Result<(), EngineError> {
        self.materials.upload(&self.gpu);
        self.scene.upload(&self.gpu)
    }

    /// Draws one frame to `output`.
    ///
    /// # Errors
    /// As [`Self::upload`].
    pub fn render(
        &mut self,
        camera: &Camera,
        light: &SkyLight,
        output: &wgpu::TextureView,
        size: (u32, u32),
    ) -> Result<(), EngineError> {
        self.render_with(camera, light, output, size, None)
    }

    /// [`Self::render`] with the interface's picture `overlay` (already finished as
    /// [`Renderer::ui`]), whose triangles standing in the world are drawn against its depth.
    ///
    /// # Errors
    /// As [`Self::upload`].
    pub fn render_with(
        &mut self,
        camera: &Camera,
        light: &SkyLight,
        output: &wgpu::TextureView,
        size: (u32, u32),
        overlay: Option<&overlay::Overlay>,
    ) -> Result<(), EngineError> {
        self.upload()?;
        #[allow(clippy::cast_precision_loss)]
        let aspect = size.0 as f32 / size.1.max(1) as f32;
        self.visibility = self.cells.visibility(
            &camera.view_projection(aspect),
            camera.position,
            camera.cell,
            camera.far,
            crate::renderer::lod_scale(camera, self.renderer.lod_bias),
        );
        self.scene
            .write_cell_rects(&self.gpu.queue, &self.visibility.rects);
        if let Some(f) = self.far.as_mut() {
            if self.visibility.outside() {
                f.update(&self.gpu, camera.position, &camera.frustum(aspect));
            } else {
                // Unseen outdoors: a plane with nothing in front of it picks no node.
                f.update(
                    &self.gpu,
                    camera.position,
                    &[Vec4::new(0.0, 0.0, 0.0, -1.0)],
                );
            }
        }
        let open_to_sky = self
            .visibility
            .eye_cell
            .is_none_or(|s| self.cells.cell(s).is_some_and(|c| c.seen_outside));
        // Whether cell slot `cell` is seen this frame (nought: the outdoors).
        let rects = &self.visibility.rects;
        let outside = self.visibility.outside();
        let seen = |cell: u32| {
            if cell == 0 {
                outside
            } else {
                rects
                    .get(cell as usize)
                    .is_some_and(|r| !crate::cells::is_empty(r))
            }
        };
        // The bodies of fire gathered since the last frame: their flames and their lamps.
        let fires = std::mem::take(&mut self.particles.fires);
        let fire_on = self.flames.on && self.renderer.lamps;
        self.particles.fire_bodies = fire_on;
        let (fire_flames, fire_lamps) = if fire_on {
            crate::body_fire::frame(
                &fires,
                &camera.frustum(aspect),
                camera.position,
                seen,
                crate::flames::MAX_FRAME_FLAMES / 2,
            )
        } else {
            (Vec::new(), Vec::new())
        };
        // A glow lights the physical calibration only. The creatures' lamps move with their light.
        let glow = |l: &lamps::Lamp| l.kind == lamps::LampKind::Glow;
        let carried = std::mem::take(&mut self.particles.glow_lamps);
        let physical = self.renderer.calibration == renderer::Calibration::Physical;
        let mut kept: std::borrow::Cow<'_, [lamps::Lamp]> = if physical && !carried.is_empty() {
            self.lamps.iter().chain(&carried).copied().collect()
        } else if physical || !self.lamps.iter().any(glow) {
            std::borrow::Cow::Borrowed(&self.lamps)
        } else {
            self.lamps.iter().filter(|l| !glow(l)).copied().collect()
        };
        if !fire_lamps.is_empty() {
            kept.to_mut().extend(fire_lamps);
        }
        let lamps_lit = self.renderer.lamps && !kept.is_empty();
        let indoor_physical = crate::renderer::indoor_physical(light, open_to_sky, lamps_lit);
        // The rooms seen this frame, by their boxes: a lamp lights them when its reach does,
        // whichever room its object stands in, as retail burns a room's light from every static
        // light in range (a torch on the far side of a doorway, or one whose object's cell is the
        // next room's).
        let seen_rooms: Vec<(glam::Vec3, glam::Vec3)> = rects
            .iter()
            .enumerate()
            .filter(|(_, r)| !crate::cells::is_empty(r))
            .filter_map(|(k, _)| {
                let v = self.cells.cell(u32::try_from(k).ok()?)?.volume.as_ref()?;
                Some((v.min, v.max))
            })
            .collect();
        let mut frame_lamps = crate::lamps::frame_lamps_reaching(
            &kept,
            &camera.frustum(aspect),
            camera.position,
            seen,
            &seen_rooms,
            self.renderer.lamp_reach,
        );
        // The probes' lamps: the same, wherever the eye looks, so turning it does not relight them.
        let probe_lamps = crate::lamps::probe_lamps(
            crate::lamps::frame_lamps_reaching(
                &kept,
                &[],
                camera.position,
                seen,
                &seen_rooms,
                self.renderer.lamp_reach,
            ),
            camera.position,
        );
        // Flame Flicker off: every flame's light holds steady.
        if !self.renderer.flicker {
            for l in &mut frame_lamps {
                l.flicker = crate::lamps::Flicker::Steady;
            }
        }
        let mut frame_flames = if self.flames.on && self.renderer.lamps {
            crate::flames::frame_flames(
                &self.flames.placed,
                &camera.frustum(aspect),
                camera.position,
                seen,
                crate::flames::FLAME_DISTANCE * self.renderer.effect_detail(),
            )
        } else {
            Vec::new()
        };
        frame_flames.truncate(crate::flames::MAX_FRAME_FLAMES - fire_flames.len());
        frame_flames.extend(fire_flames);
        self.particles.skip_flamed = self.flames.on && self.renderer.lamps;
        self.particles.wind = crate::wind::wind(light.time, self.renderer.clock);
        self.particles.detail = self.renderer.effect_detail();
        let heavens = self
            .heavens
            .as_ref()
            .map(|h| h.at(light.group, light.time, self.renderer.clock));
        if self.particles.on {
            self.particles.step(self.renderer.clock, camera.position);
            self.particles.pose(&mut self.scene);
            self.particles
                .collect(camera.position, &camera.frustum(aspect), seen);
            // A strike of lightning lights its surroundings for a moment.
            frame_lamps.extend(self.particles.flashes.iter().copied());
            frame_lamps.truncate(crate::lamps::MAX_FRAME_LAMPS);
        } else {
            self.particles.drawn.clear();
            self.particles.batches.clear();
            self.particles.external.clear();
            self.particles.blobs = (0, 0);
            self.particles.puffs = (0, 0);
        }
        let body_water = self.water_lifts(camera.position);
        // The snowy land round the eye, worked out again as the eye moves; the bodies' prints.
        let w = &mut self.renderer.weather;
        w.snow_share = match self.land.as_ref() {
            Some(l) if w.snowy_types != 0 => {
                weather::share_near(l, camera.position, w.snowy_types, &mut self.snow_share_at)
            }
            _ => 0.0,
        };
        let snowfall = w.snowfall;
        let clock = self.renderer.clock;
        self.renderer
            .trails
            .step(clock, &self.bodies, self.land.as_ref(), snowfall);
        self.renderer.render(
            &self.gpu,
            &self.scene,
            &self.materials,
            &crate::renderer::View {
                camera,
                light,
                outside: self.visibility.outside(),
                indoor: crate::renderer::indoor_light(light, open_to_sky, lamps_lit),
                indoor_physical,
                sealed: !open_to_sky,
                far: self.far.as_ref(),
                rects: &self.visibility.rects,
                heavens: heavens.as_ref(),
                lamps: &frame_lamps,
                probe_lamps: &probe_lamps,
                flames: &frame_flames,
                bodies: &self.bodies,
                body_water: &body_water,
                umbrae: &self.umbrae,
                clearings: &self.clearings,
                particles: Some(&self.particles),
                overlay,
                eye_room: self
                    .visibility
                    .eye_cell
                    .and_then(|s| self.cells.cell(s)?.volume.as_ref()),
            },
            output,
            size,
        )?;
        self.scene.settle();
        Ok(())
    }

    /// Draws one frame offscreen and reads it back as tightly packed RGBA8 rows. The output
    /// format must be [`wgpu::TextureFormat::Rgba8UnormSrgb`].
    ///
    /// # Errors
    /// As [`Self::render`].
    pub fn render_to_rgba(
        &mut self,
        camera: &Camera,
        light: &SkyLight,
        (width, height): (u32, u32),
    ) -> Result<Vec<u8>, EngineError> {
        let target = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("output"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.renderer.output_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        self.render(camera, light, &view, (width, height))?;
        Ok(read_texture(&self.gpu, &target, 4))
    }
}

/// A texture's first level read back as tightly packed rows of `bytes_per_texel`.
#[must_use]
pub fn read_texture(gpu: &Gpu, texture: &wgpu::Texture, bytes_per_texel: u32) -> Vec<u8> {
    let (w, h) = (texture.width(), texture.height());
    let row = w * bytes_per_texel;
    let padded = row.div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("texture readback"),
        size: u64::from(padded) * u64::from(h),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
        },
        texture.size(),
    );
    gpu.queue.submit([enc.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    gpu.wait();
    let mut out = Vec::with_capacity((row * h) as usize);
    if let Ok(view) = slice.get_mapped_range() {
        for y in 0..h as usize {
            let start = y * padded as usize;
            out.extend_from_slice(&view[start..start + row as usize]);
        }
    }
    buffer.unmap();
    out
}
