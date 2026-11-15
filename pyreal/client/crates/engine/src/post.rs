//! The frame's end: auto-exposure from a histogram of the HDR target, bloom, and the tone map.
//!
//! The HDR target is drawn pre-exposed: every pass that writes light into it multiplies by the
//! exposure in force at the start of the frame (last frame's), so night and noon both sit well
//! inside half-float's range. The exposure pass measures the frame with that taken back out,
//! eases toward the exposure that brings its mean to the key, and the tone map applies the new
//! exposure over the old.

use crate::gpu::Gpu;

/// How the light is scaled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Calibration {
    /// Physical units ([`crate::sky::SCENE_LUX`]): sun, sky and lamps in their real ratios, the
    /// eye adapting to them, with bloom.
    #[default]
    Physical,
    /// The client's own light: the sun and the sky summed in the dats' colour space and clamped,
    /// at a fixed exposure and without bloom, so a frame compares with the client's for
    /// brightness.
    Retail,
}

/// What the HDR target is multiplied by before the tone curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Exposure {
    /// Adapted to the frame, `compensation` stops over the key.
    Auto { compensation: f32 },
    /// Fixed.
    Manual(f32),
}

impl Default for Exposure {
    fn default() -> Self {
        Self::Auto { compensation: 0.0 }
    }
}

/// The histogram's lowest log2 light, scene units, and its span.
pub const HISTOGRAM_LOW: f32 = -24.0;
pub const HISTOGRAM_SPAN: f32 = 32.0;
/// The middle grey the frame's mean light is brought to.
pub const KEY: f32 = 0.16;
/// The least and the most exposure: a sunlit snowfield, and the darkest a night is brought up.
pub const EXPOSURE_RANGE: (f32, f32) = (0.004, 30_000.0);
/// The key's share kept in the dark, and the mean log2 light (scene units) from which the whole
/// key is kept: a lamp-lit room is drawn darker than a day, a moonlit night far darker.
pub const NIGHT_KEY: (f32, f32) = (0.07, -4.0);
/// How fast the eye adapts, per second: into the dark (a brighter exposure), into the light.
pub const ADAPT_RATE: (f32, f32) = (1.1, 2.8);
/// The shares of the frame, darkest first, between which its mean is taken.
pub const TRIM: (f32, f32) = (0.1, 0.96);
/// The highlight guard: the share of the frame, darkest first, whose brightness is looked at, and
/// the most that brightness may be once exposed. A night's lamp pools or a brazier's glow pull the
/// exposure down rather than burning to white.
pub const GUARD: (f32, f32) = (0.985, 3.0);
/// Bright ground: the mean log2 light (scene units) over which the key rises, and the share of
/// each stop past it the key keeps, so sunlit sand or snow reads brighter than a meadow (a
/// Holtburg noon is under it).
pub const BRIGHT_KEY: (f32, f32) = (-0.3, 0.9);
/// The most exposure in a room sealed from the sky: over what draws the room's own light as
/// retail's, so a dungeon reads as retail's does, its lamp pools brighter still.
pub const ROOM_EXPOSURE: f32 = 1.6 * crate::sky::SCENE_LUX / crate::renderer::ROOM_LUX;
/// The bloom's levels below the half-size first one.
pub const BLOOM_LEVELS: u32 = 6;
/// The bloom's strength under the physical calibration: the share of what is past white that
/// glows round it.
pub const BLOOM_STRENGTH: f32 = 0.3;
/// The exposure state's bytes: the exposure in force, the mean log2 light, the target, unused.
pub const STATE_SIZE: u64 = 16;

/// What the auto-exposure found last frame, read back.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ExposureState {
    /// The exposure in force.
    pub exposure: f32,
    /// The frame's mean log2 light, scene units.
    pub mean_log2: f32,
    /// The exposure it is easing toward.
    pub target: f32,
}

/// The bloom's chains and the bind groups of one target size.
#[derive(Debug)]
struct BloomTargets {
    width: u32,
    height: u32,
    /// One bind group per dispatch, down then up, with the size each writes.
    steps: Vec<(wgpu::BindGroup, (u32, u32), Step)>,
    /// The finished bloom (the up chain's first level).
    result: wgpu::TextureView,
    _textures: [wgpu::Texture; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    First,
    Down,
    Up,
}

/// The exposure and bloom passes' state.
#[derive(Debug)]
pub struct Post {
    /// The exposure the HDR target is drawn at this frame, bound with the frame uniform.
    pub pre: wgpu::Buffer,
    /// The exposure pass's state ([`ExposureState`]).
    pub state: wgpu::Buffer,
    params: wgpu::Buffer,
    histogram: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    histogram_pipeline: wgpu::ComputePipeline,
    average_pipeline: wgpu::ComputePipeline,
    bloom_layout: wgpu::BindGroupLayout,
    bloom_first: wgpu::ComputePipeline,
    bloom_down: wgpu::ComputePipeline,
    bloom_up: wgpu::ComputePipeline,
    pub sampler: wgpu::Sampler,
    bloom: Option<BloomTargets>,
    group: Option<wgpu::BindGroup>,
    /// The exposure is taken at once on the next frame (the first, or after a jump).
    pub snap: bool,
}

fn f32_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

impl Post {
    /// The passes' pipelines; `frame_layout` is group 0 of the exposure pass.
    #[must_use]
    pub fn new(gpu: &Gpu, frame_layout: &wgpu::BindGroupLayout) -> Self {
        use wgpu::ShaderStages as S;
        let device = &gpu.device;
        let buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        use wgpu::BufferUsages as U;
        let pre = buffer("pre-exposure", STATE_SIZE, U::STORAGE | U::COPY_DST);
        let state = buffer(
            "exposure state",
            STATE_SIZE,
            U::STORAGE | U::COPY_SRC | U::COPY_DST,
        );
        let one = f32_bytes(&[1.0, 0.0, 1.0, 0.0]);
        gpu.queue.write_buffer(&pre, 0, &one);
        gpu.queue.write_buffer(&state, 0, &one);
        let params = buffer("exposure params", 80, U::UNIFORM | U::COPY_DST);
        let histogram = buffer("exposure histogram", 256 * 4, U::STORAGE | U::COPY_DST);
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: S::COMPUTE,
            ty,
            count: None,
        };
        let storage = |read_only| wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let sampled = |filterable| wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("exposure"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(1, sampled(false)),
                entry(2, storage(false)),
                entry(3, storage(false)),
                entry(4, sampled(false)),
            ],
        });
        let module = gpu.shader("exposure", &crate::renderer::source::exposure());
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("exposure"),
            bind_group_layouts: &[Some(frame_layout), Some(&layout)],
            immediate_size: 0,
        });
        let compute = |entry_point: &str, layout: &wgpu::PipelineLayout, module| {
            gpu.compute_pipeline(wgpu::ComputePipelineDescriptor {
                label: Some(entry_point),
                layout: Some(layout),
                module,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let histogram_pipeline = compute("build_histogram", &pipeline_layout, &module);
        let average_pipeline = compute("average", &pipeline_layout, &module);
        let bloom_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom"),
            entries: &[
                entry(0, sampled(true)),
                entry(
                    1,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(
                    2,
                    wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: crate::renderer::HDR,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                ),
                entry(3, sampled(false)),
                entry(
                    4,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
            ],
        });
        let bloom_module = gpu.shader("bloom", include_str!("shaders/bloom.wgsl"));
        let bloom_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("bloom"),
                bind_group_layouts: &[Some(&bloom_layout)],
                immediate_size: 0,
            });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        Self {
            pre,
            state,
            params,
            histogram,
            histogram_pipeline,
            average_pipeline,
            bloom_first: compute("down_first", &bloom_pipeline_layout, &bloom_module),
            bloom_down: compute("down", &bloom_pipeline_layout, &bloom_module),
            bloom_up: compute("up", &bloom_pipeline_layout, &bloom_module),
            layout,
            bloom_layout,
            sampler,
            bloom: None,
            group: None,
            snap: true,
        }
    }

    /// The bloom's chains for a `width` by `height` HDR target, `hdr`, with its `depth` (where
    /// the sky is); the bind groups that read it, the exposure's with the G-buffer's `material`
    /// beside it. Made again when the size changes.
    pub fn resize(
        &mut self,
        gpu: &Gpu,
        hdr: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        material: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) {
        let device = &gpu.device;
        let half = (width.div_ceil(2).max(1), height.div_ceil(2).max(1));
        let levels = BLOOM_LEVELS
            .min(32 - half.0.min(half.1).leading_zeros())
            .max(1);
        let chain = |label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: half.0,
                    height: half.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: levels,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: crate::renderer::HDR,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let down = chain("bloom down");
        let up = chain("bloom up");
        let level = |t: &wgpu::Texture, k: u32| {
            t.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: k,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let size = |k: usize| {
            let k = u32::try_from(k).unwrap_or(31);
            ((half.0 >> k).max(1), (half.1 >> k).max(1))
        };
        let group = |src: &wgpu::TextureView, dst: &wgpu::TextureView, add: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bloom"),
                layout: &self.bloom_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(dst),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(add),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(depth),
                    },
                ],
            })
        };
        let downs: Vec<wgpu::TextureView> = (0..levels).map(|k| level(&down, k)).collect();
        let ups: Vec<wgpu::TextureView> = (0..levels).map(|k| level(&up, k)).collect();
        let mut steps = Vec::new();
        // The add binding is unread going down; any other level stands in.
        let spare = &ups[0];
        steps.push((group(hdr, &downs[0], spare), size(0), Step::First));
        for k in 1..levels as usize {
            steps.push((group(&downs[k - 1], &downs[k], spare), size(k), Step::Down));
        }
        let last = levels as usize - 1;
        // The coarsest level stands as it is: copied up through the add of a zero-weight tent
        // is not needed; the first up reads it directly.
        for k in (0..last).rev() {
            let src = if k + 1 == last {
                &downs[last]
            } else {
                &ups[k + 1]
            };
            steps.push((group(src, &ups[k], &downs[k]), size(k), Step::Up));
        }
        let result = if last == 0 {
            level(&down, 0)
        } else {
            level(&up, 0)
        };
        self.bloom = Some(BloomTargets {
            width,
            height,
            steps,
            result,
            _textures: [down, up],
        });
        self.group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("exposure"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(hdr),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.histogram.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.state.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(material),
                },
            ],
        }));
    }

    /// The finished bloom, for the tone map.
    #[must_use]
    pub fn bloom_view(&self) -> Option<&wgpu::TextureView> {
        self.bloom.as_ref().map(|b| &b.result)
    }

    /// Writes this frame's exposure parameters: `exposure` as asked, at most `most`, `dt`
    /// seconds since the last frame. Before the frame's passes: last frame's exposure becomes
    /// this frame's pre-exposure.
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        exposure: Exposure,
        most: f32,
        dt: f32,
    ) {
        let Some(b) = self.bloom.as_ref() else {
            return;
        };
        #[allow(clippy::cast_precision_loss)]
        let size = [b.width as f32, b.height as f32];
        let (key, manual) = match exposure {
            Exposure::Auto { compensation } => (
                KEY * pyreal_primitives::num::math::powf(2.0, compensation),
                0.0,
            ),
            Exposure::Manual(e) => (KEY, e.max(1e-6)),
        };
        let p = f32_bytes(&[
            HISTOGRAM_LOW,
            1.0 / HISTOGRAM_SPAN,
            size[0],
            size[1],
            key,
            EXPOSURE_RANGE.0,
            most.min(EXPOSURE_RANGE.1),
            dt.max(0.0),
            ADAPT_RATE.0,
            ADAPT_RATE.1,
            f32::from(u8::from(std::mem::take(&mut self.snap))),
            manual,
            TRIM.0,
            TRIM.1,
            NIGHT_KEY.0,
            NIGHT_KEY.1,
            GUARD.0,
            GUARD.1,
            BRIGHT_KEY.0,
            BRIGHT_KEY.1,
        ]);
        gpu.queue.write_buffer(&self.params, 0, &p);
        enc.copy_buffer_to_buffer(&self.state, 0, &self.pre, 0, 4);
    }

    /// The exposure pass: the histogram, then its mean and the adapted exposure.
    pub fn encode_exposure(&self, pass: &mut wgpu::ComputePass<'_>, frame: &wgpu::BindGroup) {
        let (Some(b), Some(group)) = (self.bloom.as_ref(), self.group.as_ref()) else {
            return;
        };
        pass.set_bind_group(0, frame, &[]);
        pass.set_bind_group(1, group, &[]);
        pass.set_pipeline(&self.histogram_pipeline);
        pass.dispatch_workgroups(b.width.div_ceil(32), b.height.div_ceil(32), 1);
        pass.set_pipeline(&self.average_pipeline);
        pass.dispatch_workgroups(1, 1, 1);
    }

    /// The bloom pass: down the chain, then up it.
    pub fn encode_bloom(&self, pass: &mut wgpu::ComputePass<'_>) {
        let Some(b) = self.bloom.as_ref() else {
            return;
        };
        for (group, (w, h), step) in &b.steps {
            pass.set_pipeline(match step {
                Step::First => &self.bloom_first,
                Step::Down => &self.bloom_down,
                Step::Up => &self.bloom_up,
            });
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        }
    }

    /// The exposure state, read back (blocking).
    #[must_use]
    pub fn read_state(&self, gpu: &Gpu) -> ExposureState {
        let b = gpu.read_buffer(&self.state, 0, STATE_SIZE);
        let f = |i: usize| {
            b.get(i * 4..i * 4 + 4)
                .map_or(0.0, |c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        };
        ExposureState {
            exposure: f(0),
            mean_log2: f(1),
            target: f(2),
        }
    }
}
