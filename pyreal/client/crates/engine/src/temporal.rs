//! The frame's last steps at the output's size: the world drawn at a render scale (fixed, or
//! dynamic to hold a frame budget), jittered for the temporal anti-aliasing, rebuilt at the
//! output's size by one of four upscalers, then composited with the UI's picture.
//!
//! - **MetalFX** (macOS): Apple's temporal scaler, through `wgpu`'s Metal objects
//!   ([`crate::metalfx`]).
//! - **Temporal**: the engine's own resolve (`resolve.wgsl`, render-hifi's ported): eight Halton
//!   offsets laid into a history at the output's size, followed by the G-buffer's motion, held
//!   to the colours round each pixel, and dropped where the material now seen was not there last
//!   frame. At a render scale of one it is the anti-aliasing.
//! - **Spatial**: EASU, the picture enlarged along its edges, then RCAS in the composite.
//! - **Plain**: filtered up and lightly sharpened, or copied texel for texel at one.
//!
//! The first that can run is taken ([`choose`]). The UI's picture is laid over at the output's
//! size, texel for texel, whatever the render scale ([`Ui`]).

use glam::{Mat4, Vec2};

/// A way to rebuild the world's picture at the output's size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Upscaler {
    MetalFx,
    Temporal,
    Spatial,
    Plain,
}

impl Upscaler {
    /// Every upscaler, the one chosen first first.
    pub const ORDER: [Self; 4] = [Self::MetalFx, Self::Temporal, Self::Spatial, Self::Plain];

    /// The upscaler a word names (`metalfx`, `temporal`, `spatial`, `plain`).
    #[must_use]
    pub fn named(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            "metalfx" => Some(Self::MetalFx),
            "temporal" | "taa" => Some(Self::Temporal),
            "spatial" | "easu" => Some(Self::Spatial),
            "plain" => Some(Self::Plain),
            _ => None,
        }
    }

    /// Whether it rebuilds from the frames before (and so wants the jitter).
    #[must_use]
    pub fn temporal(self) -> bool {
        matches!(self, Self::MetalFx | Self::Temporal)
    }
}

/// What can rebuild a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Can {
    /// A MetalFX scaler exists for these sizes and the anti-aliasing is on.
    pub metalfx: bool,
    /// The anti-aliasing is on.
    pub temporal: bool,
}

/// The first upscaler from `first` on that `can` run; the spatial and the plain always can.
#[must_use]
pub fn choose(first: Upscaler, can: Can) -> Upscaler {
    Upscaler::ORDER
        .into_iter()
        .skip_while(|u| *u != first)
        .find(|u| match u {
            Upscaler::MetalFx => can.metalfx,
            Upscaler::Temporal => can.temporal,
            Upscaler::Spatial | Upscaler::Plain => true,
        })
        .unwrap_or(Upscaler::Plain)
}

/// The offsets the jitter steps through before repeating.
pub const JITTER_PHASES: u32 = 8;

/// The `index`th number of the Halton sequence in `base`, 0..1.
#[must_use]
pub fn halton(mut index: u32, base: u32) -> f32 {
    let mut f = 1.0;
    let mut r = 0.0;
    #[allow(clippy::cast_precision_loss)]
    while index > 0 {
        f /= base as f32;
        r += f * (index % base) as f32;
        index /= base;
    }
    r
}

/// The jitter of frame `n` drawn at `size` pixels, in normalised device coordinates: within half
/// a pixel each way, Halton (2, 3) over [`JITTER_PHASES`] offsets.
#[must_use]
pub fn jitter(n: u32, size: (u32, u32)) -> Vec2 {
    let i = n % JITTER_PHASES + 1;
    let pixel = Vec2::new(halton(i, 2) - 0.5, halton(i, 3) - 0.5);
    #[allow(clippy::cast_precision_loss)]
    let s = Vec2::new(size.0.max(1) as f32, size.1.max(1) as f32);
    Vec2::new(pixel.x * 2.0 / s.x, -pixel.y * 2.0 / s.y)
}

/// The clip-space shift that draws a projection `j` (normalised device coordinates) off.
#[must_use]
pub fn jitter_matrix(j: Vec2) -> Mat4 {
    Mat4::from_translation(j.extend(0.0))
}

/// The drawn size at `scale` of an `output`, each side at least one pixel.
#[must_use]
pub fn scaled(output: (u32, u32), scale: f32) -> (u32, u32) {
    let s = scale.clamp(0.1, 1.0);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let side = |v: u32| ((v as f32 * s).round() as u32).clamp(1, v.max(1));
    (side(output.0), side(output.1))
}

/// A render scale chosen each frame to hold a GPU frame budget: the frame's GPU time (its passes'
/// span) eased, and the scale moved by the square root of budget over time (time goes with the
/// pixels), in steps, waiting some frames after each change for the new size's timings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DynamicResolution {
    /// The GPU time a frame may take, ms (8.3 for 120 frames a second).
    pub budget_ms: f64,
    pub min_scale: f32,
    pub max_scale: f32,
    /// The scale moves in multiples of this.
    pub step: f32,
    /// Frames to wait after a change.
    pub settle: u32,
    scale: f32,
    eased: Option<f64>,
    wait: u32,
}

impl DynamicResolution {
    /// A controller holding `budget_ms`, between half and full scale, starting at full.
    #[must_use]
    pub fn new(budget_ms: f64) -> Self {
        Self {
            budget_ms,
            min_scale: 0.5,
            max_scale: 1.0,
            step: 0.05,
            settle: 8,
            scale: 1.0,
            eased: None,
            wait: 0,
        }
    }

    /// The scale in force.
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Takes the last frame's GPU time (ms; zero when unknown) and gives the scale to draw at.
    pub fn observe(&mut self, gpu_ms: f64) -> f32 {
        if gpu_ms <= 0.0 {
            return self.scale;
        }
        let e = self.eased.map_or(gpu_ms, |e| e + (gpu_ms - e) * 0.25);
        self.eased = Some(e);
        if self.wait > 0 {
            self.wait -= 1;
            return self.scale;
        }
        let ratio = self.budget_ms / e.max(1e-6);
        // Over the budget, down at once; well under it (under 85%), up gently.
        let want = if ratio < 1.0 {
            f64::from(self.scale) * ratio.sqrt() * 0.97
        } else if ratio > 1.0 / 0.85 {
            f64::from(self.scale) * ratio.sqrt().min(1.1)
        } else {
            return self.scale;
        };
        #[allow(clippy::cast_possible_truncation)]
        let want = want as f32;
        let step = self.step.max(0.01);
        let q = (if want < self.scale {
            (want / step).floor()
        } else {
            (want / step).ceil().min((self.scale / step).round() + 1.0)
        }) * step;
        let next = q.clamp(self.min_scale, self.max_scale);
        if (next - self.scale).abs() > 1e-4 {
            self.scale = next;
            self.wait = self.settle;
            self.eased = None;
        }
        self.scale
    }
}

/// The UI's picture, laid over the world at the output's size, premultiplied by its alpha. Its
/// view must be the output's size, filterable float (an `Rgba8UnormSrgb` picture is read as linear
/// light); the engine never writes it.
#[derive(Debug, Clone)]
pub struct Ui {
    pub view: wgpu::TextureView,
    /// The picture holds sRGB-encoded values blended as the encoded numbers (a `Rgba8Unorm` view
    /// of a UI drawn as the old fixed-function devices drew one): it is laid over the world in
    /// that encoding too.
    pub gamma: bool,
    /// Its see-through grounds the world behind is blurred under (UI Blur).
    pub backdrops: Vec<crate::frost::Backdrop>,
}

/// The resolve's uniform bytes.
pub const RESOLVE_SIZE: u64 = 128;
/// The composite's uniform bytes.
pub const COMPOSITE_SIZE: u64 = 64;
/// The drawn picture's format before the resolve: tone mapped, linear.
pub const LDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// How much the temporal's and MetalFX's rebuilt picture is sharpened (RCAS, 1 the most).
pub const TEMPORAL_SHARPNESS: f32 = 0.7;
/// How much the spatial's enlarged picture is sharpened.
pub const SPATIAL_SHARPNESS: f32 = 0.85;
/// How much the plain enlargement is sharpened, a share of the distance from its neighbours' mean.
pub const PLAIN_SHARPNESS: f32 = 0.35;
/// The current frame's share of the temporal blend while nothing moves, and at speed.
pub const STEADY_SHARE: f32 = 0.1;
pub const MOVING_SHARE: f32 = 0.25;
/// Pixels of motion at which the moving share is reached.
pub const MOVING_PIXELS: f32 = 24.0;

/// The resolve's WGSL.
#[must_use]
pub fn resolve_source() -> String {
    format!(
        "{}\n{}",
        include_str!("shaders/resolve.wgsl"),
        include_str!("shaders/upscale.wgsl")
    )
}

/// The composite's WGSL.
#[must_use]
pub fn composite_source() -> String {
    format!(
        "{}\n{}",
        include_str!("shaders/composite.wgsl"),
        include_str!("shaders/upscale.wgsl")
    )
}

/// The output-sized targets: the two histories and the enlarged picture.
#[derive(Debug)]
struct OutputTargets {
    size: (u32, u32),
    history: [wgpu::Texture; 2],
    upscaled: wgpu::Texture,
}

/// The pipelines, the output-sized targets and the history's state.
#[derive(Debug)]
pub struct Temporal {
    resolve_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    temporal_pipeline: wgpu::RenderPipeline,
    easu_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    resolve_uniform: wgpu::Buffer,
    composite_uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    no_ui: wgpu::TextureView,
    /// UI Blur's passes, and the grounds' uniform the composite reads.
    frost: crate::frost::Frost,
    grounds_uniform: wgpu::Buffer,
    out: Option<OutputTargets>,
    scaler: Option<crate::metalfx::TemporalScaler>,
    /// The output size a scaler could not be made for, so it is not asked again each frame.
    no_scaler: Option<(u32, u32)>,
    /// MetalFX scalers made, for a test.
    scalers_made: u32,
    /// How the scales are faring; MetalFX is used only while they are.
    watch: std::sync::Arc<crate::metalfx::Watch>,
    /// Frames drawn with the jitter, for its phase.
    pub frame: u32,
    /// Whether the newest history holds a frame that may be followed.
    valid: bool,
    /// The history holding the newest frame.
    newest: usize,
    /// The drawn size last frame, for the materials' rejection.
    last_input: (u32, u32),
    /// The upscaler used last frame, and whether MetalFX's history continues.
    pub used: Option<Upscaler>,
}

fn texture_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn tex_view(binding: u32, v: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(v),
    }
}

/// What one frame's resolve reads from the drawn targets.
#[derive(Debug, Clone, Copy)]
pub struct Drawn<'a> {
    pub size: (u32, u32),
    pub ldr: &'a wgpu::Texture,
    pub depth: &'a wgpu::Texture,
    pub motion: &'a wgpu::Texture,
    pub ids: &'a wgpu::Texture,
    pub prev_ids: &'a wgpu::Texture,
    pub reactive: &'a wgpu::Texture,
    /// The jitter this frame was drawn with (normalised device coordinates).
    pub jitter: Vec2,
    /// This frame's unjittered device coordinates to last frame's clip space.
    pub reproject: Mat4,
}

impl Temporal {
    /// The pipelines, the composite drawing to `output_format`.
    #[must_use]
    pub fn new(gpu: &crate::gpu::Gpu, output_format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let float = wgpu::TextureSampleType::Float { filterable: true };
        let unfiltered = wgpu::TextureSampleType::Float { filterable: false };
        let resolve_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("resolve"),
            entries: &[
                texture_entry(0, unfiltered),
                texture_entry(1, wgpu::TextureSampleType::Depth),
                texture_entry(2, unfiltered),
                texture_entry(3, wgpu::TextureSampleType::Uint),
                texture_entry(4, wgpu::TextureSampleType::Uint),
                texture_entry(5, float),
                sampler_entry(6),
                uniform_entry(7),
            ],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite"),
            entries: &[
                texture_entry(0, float),
                texture_entry(1, float),
                sampler_entry(2),
                uniform_entry(3),
                texture_entry(4, float),
                uniform_entry(5),
            ],
        });
        let pipeline = |label: &str,
                        src: &str,
                        fs: &str,
                        layout: &wgpu::BindGroupLayout,
                        format: wgpu::TextureFormat| {
            let module = gpu.shader(label, src);
            gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some(label),
                        bind_group_layouts: &[Some(layout)],
                        immediate_size: 0,
                    }),
                ),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(format.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let resolve_src = resolve_source();
        let temporal_pipeline = pipeline(
            "temporal resolve",
            &resolve_src,
            "fs_temporal",
            &resolve_layout,
            crate::metalfx::COLOUR_FORMAT,
        );
        let easu_pipeline = pipeline(
            "spatial upscale",
            &resolve_src,
            "fs_easu",
            &resolve_layout,
            crate::metalfx::COLOUR_FORMAT,
        );
        let composite_pipeline = pipeline(
            "composite",
            &composite_source(),
            "fs_composite",
            &composite_layout,
            output_format,
        );
        let buffer = |label, size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("resolve"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let no_ui = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("no UI"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        Self {
            resolve_layout,
            composite_layout,
            temporal_pipeline,
            easu_pipeline,
            composite_pipeline,
            resolve_uniform: buffer("resolve", RESOLVE_SIZE),
            composite_uniform: buffer("composite", COMPOSITE_SIZE),
            grounds_uniform: buffer("ui blur grounds", crate::frost::UNIFORM_SIZE),
            frost: crate::frost::Frost::new(gpu),
            sampler,
            no_ui,
            out: None,
            scaler: None,
            no_scaler: None,
            scalers_made: 0,
            watch: std::sync::Arc::default(),
            frame: 0,
            valid: false,
            newest: 0,
            last_input: (0, 0),
            used: None,
        }
    }

    /// Forgets the frames before: after a cut, a teleport, or a change of what is drawn.
    pub fn reset(&mut self) {
        self.valid = false;
    }

    fn output_targets(&mut self, gpu: &crate::gpu::Gpu, size: (u32, u32)) {
        if self.out.as_ref().is_some_and(|o| o.size == size) {
            return;
        }
        let make = |label, storage: bool| {
            gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: crate::metalfx::COLOUR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | if storage {
                        wgpu::TextureUsages::STORAGE_BINDING
                    } else {
                        wgpu::TextureUsages::empty()
                    },
                view_formats: &[],
            })
        };
        self.out = Some(OutputTargets {
            size,
            history: [make("history", false), make("history", false)],
            upscaled: make("upscaled", true),
        });
        self.valid = false;
    }

    /// The upscaler this frame takes from `first`, with the anti-aliasing `aa`, drawn at `input`
    /// for `output`: MetalFX where a scaler is (or can be) made for the output, accepts the drawn
    /// size, and its scales are faring well ([`crate::metalfx::Watch`]). A new drawn size keeps
    /// the scaler and its history.
    pub fn choose(
        &mut self,
        gpu: &crate::gpu::Gpu,
        first: Upscaler,
        aa: bool,
        input: (u32, u32),
        output: (u32, u32),
    ) -> Upscaler {
        if !aa && input == output && first != Upscaler::Spatial {
            return Upscaler::Plain;
        }
        let mut metalfx = false;
        if self.watch.failed() {
            self.scaler = None;
        } else if aa && first == Upscaler::MetalFx {
            let fits = self.scaler.as_ref().is_some_and(|s| s.output == output);
            if !fits && self.no_scaler != Some(output) {
                self.scaler = crate::metalfx::TemporalScaler::new(&gpu.device, output);
                if self.scaler.is_some() {
                    self.scalers_made += 1;
                } else {
                    self.no_scaler = Some(output);
                }
                self.valid = false;
            }
            metalfx = self.watch.ready()
                && self
                    .scaler
                    .as_ref()
                    .is_some_and(|s| s.output == output && s.accepts(input));
        }
        choose(
            first,
            Can {
                metalfx,
                temporal: aa,
            },
        )
    }

    /// Records the rebuild (`"resolve"`) and the composite (`"composite"`) into `rec`, the
    /// world's picture `drawn` going to `output` with the UI `ui` over it.
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        gpu: &crate::gpu::Gpu,
        rec: &mut crate::graph::FrameRecorder<'_>,
        how: Upscaler,
        drawn: &Drawn<'_>,
        output: &wgpu::TextureView,
        output_size: (u32, u32),
        ui: Option<&Ui>,
    ) {
        self.output_targets(gpu, output_size);
        let Some(out) = self.out.as_ref() else {
            return;
        };
        let f = |v: u32| {
            #[allow(clippy::cast_precision_loss)]
            let v = v.max(1) as f32;
            [v, 1.0 / v]
        };
        let (iw, ih) = (f(drawn.size.0), f(drawn.size.1));
        let (ow, oh) = (f(output_size.0), f(output_size.1));
        let continues = self.valid && self.used == Some(how);
        let ids_match = self.last_input == drawn.size;
        let mut bytes = Vec::with_capacity(128);
        let mut put = |v: [f32; 4]| {
            for x in v {
                bytes.extend_from_slice(&x.to_le_bytes());
            }
        };
        put([iw[0], ih[0], iw[1], ih[1]]);
        put([ow[0], oh[0], ow[1], oh[1]]);
        put([
            drawn.jitter.x,
            drawn.jitter.y,
            if continues { 1.0 } else { 0.0 },
            STEADY_SHARE,
        ]);
        put([
            MOVING_SHARE,
            MOVING_PIXELS,
            if ids_match { 1.0 } else { 0.0 },
            0.0,
        ]);
        for c in drawn.reproject.to_cols_array().as_chunks::<4>().0 {
            put(*c);
        }
        gpu.queue.write_buffer(&self.resolve_uniform, 0, &bytes);
        let view = |t: &wgpu::Texture| t.create_view(&Default::default());
        let (ldr, depth, motion, ids, prev_ids) = (
            view(drawn.ldr),
            view(drawn.depth),
            view(drawn.motion),
            view(drawn.ids),
            view(drawn.prev_ids),
        );
        let older = 1 - self.newest;
        let history_in = view(&out.history[self.newest]);
        let history_out = view(&out.history[older]);
        let upscaled = view(&out.upscaled);
        let resolve_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("resolve"),
            layout: &self.resolve_layout,
            entries: &[
                tex_view(0, &ldr),
                tex_view(1, &depth),
                tex_view(2, &motion),
                tex_view(3, &ids),
                tex_view(4, &prev_ids),
                tex_view(5, &history_in),
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: self.resolve_uniform.as_entire_binding(),
                },
            ],
        });
        let attach = |v| {
            [Some(wgpu::RenderPassColorAttachment {
                view: v,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })]
        };
        let keep = |v| {
            [Some(wgpu::RenderPassColorAttachment {
                view: v,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })]
        };
        // The rebuild, and which picture the composite reads; an empty pass where it is done
        // elsewhere (MetalFX) or not at all.
        let (picture, mode, sharpness) = match how {
            Upscaler::Temporal => {
                let mut pass = rec.render("resolve", &attach(&history_out), None);
                pass.set_pipeline(&self.temporal_pipeline);
                pass.set_bind_group(0, &resolve_group, &[]);
                pass.draw(0..3, 0..1);
                drop(pass);
                self.newest = older;
                (history_out.clone(), 2u32, TEMPORAL_SHARPNESS)
            }
            Upscaler::Spatial => {
                let mut pass = rec.render("resolve", &attach(&upscaled), None);
                pass.set_pipeline(&self.easu_pipeline);
                pass.set_bind_group(0, &resolve_group, &[]);
                pass.draw(0..3, 0..1);
                (upscaled.clone(), 2, SPATIAL_SHARPNESS)
            }
            Upscaler::MetalFx => {
                drop(rec.render("resolve", &keep(&upscaled), None));
                rec.flush(gpu);
                let scaled = self.scaler.as_ref().is_some_and(|s| {
                    s.encode(
                        &gpu.device,
                        &gpu.queue,
                        &crate::metalfx::ScaleInputs {
                            colour: drawn.ldr,
                            depth: drawn.depth,
                            motion: drawn.motion,
                            reactive: drawn.reactive,
                            output: &out.upscaled,
                            jitter: [drawn.jitter.x * iw[0] * 0.5, -drawn.jitter.y * ih[0] * 0.5],
                            motion_scale: [-iw[0], -ih[0]],
                            reset: !continues,
                            size: drawn.size,
                        },
                        &self.watch,
                    )
                });
                if scaled {
                    (upscaled.clone(), 2, TEMPORAL_SHARPNESS)
                } else {
                    (ldr.clone(), 1, PLAIN_SHARPNESS)
                }
            }
            Upscaler::Plain => {
                drop(rec.render("resolve", &keep(&upscaled), None));
                let mode = u32::from(drawn.size != output_size);
                (ldr.clone(), mode, PLAIN_SHARPNESS)
            }
        };
        let (pw, ph) = if mode == 2 { (ow, oh) } else { (iw, ih) };
        // UI Blur, where a see-through ground asks for it.
        let grounds: &[crate::frost::Backdrop] = ui.map_or(&[], |u| &u.backdrops);
        let frost = if grounds.is_empty() {
            crate::frost::Frost::skip(rec);
            None
        } else {
            self.frost.record(gpu, rec, &picture, output_size, grounds)
        };
        let grounds = if frost.is_some() { grounds } else { &[] };
        gpu.queue
            .write_buffer(&self.grounds_uniform, 0, &crate::frost::uniform(grounds));
        let mut bytes = Vec::with_capacity(64);
        for v in [
            mode,
            u32::from(ui.is_some()),
            u32::from(ui.is_some_and(|u| u.gamma)),
            0,
        ] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        for v in [
            sharpness, 0.0, 0.0, 0.0, pw[0], ph[0], pw[1], ph[1], ow[0], oh[0], ow[1], oh[1],
        ] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        gpu.queue.write_buffer(&self.composite_uniform, 0, &bytes);
        let composite_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite"),
            layout: &self.composite_layout,
            entries: &[
                tex_view(0, &picture),
                tex_view(1, ui.map_or(&self.no_ui, |u| &u.view)),
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.composite_uniform.as_entire_binding(),
                },
                tex_view(4, frost.as_ref().unwrap_or(&self.no_ui)),
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: self.grounds_uniform.as_entire_binding(),
                },
            ],
        });
        {
            let mut pass = rec.render("composite", &attach(output), None);
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &composite_group, &[]);
            pass.draw(0..3, 0..1);
        }
        // Last frame's materials, for the next frame's rejection.
        rec.encoder().copy_texture_to_texture(
            drawn.ids.as_image_copy(),
            drawn.prev_ids.as_image_copy(),
            drawn.ids.size(),
        );
        self.valid = how.temporal();
        self.used = Some(how);
        self.last_input = drawn.size;
        if how.temporal() {
            self.frame = self.frame.wrapping_add(1);
        }
    }

    /// How MetalFX's scales are faring.
    #[must_use]
    pub fn metalfx_watch(&self) -> &crate::metalfx::Watch {
        &self.watch
    }

    /// MetalFX scalers made so far, for a test.
    #[must_use]
    pub fn metalfx_scalers_made(&self) -> u32 {
        self.scalers_made
    }

    /// The newest history (the temporal's), for a test.
    #[must_use]
    pub fn history(&self) -> Option<&wgpu::Texture> {
        self.out.as_ref().map(|o| &o.history[self.newest])
    }
}
