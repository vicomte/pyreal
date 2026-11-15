//! The air's light: volumetric fog and the sun's shafts through it (God Rays), a froxel at a time.
//!
//! A grid over the view, [`FROXELS`] cells across and down and [`SLICES`] deep (the slices spaced
//! out with distance to [`REACH`] metres, as far as the sun's cascades hold its shadow), is lit in
//! compute: each cell takes the sun through its cascade (so walls, roofs and leaves break the
//! light into shafts) and a little of the sky, by a density that thins with height, and the cells
//! are summed from the eye outward into the light scattered toward the eye and the share of what
//! lies behind that still shows. A full-screen pass then lays that over the lit picture at each
//! pixel's distance. The slices' samples are jittered frame to frame, which the temporal resolve
//! smooths. Off in a sealed room or with the retail calibration; the level ([`Air::level`], the
//! God Rays menu) scales how much of the sun the air scatters, and the weather thickens it. By day,
//! as render-hifi drew them, the air near the eye sends on more of a low sun and more in shade, and
//! the apply pass streaks the open sky's light from the sun on screen past leaves and roofs.

use crate::gpu::Gpu;

/// Cells across and down the view.
pub const FROXELS: (u32, u32) = (160, 90);
/// Slices deep.
pub const SLICES: u32 = 64;
/// The first slice's far side and the last's, metres.
pub const FIRST: f32 = 1.0;
pub const REACH: f32 = 320.0;
/// The air's density per metre at the eye's height, at each level (Low, Medium, High).
const DENSITY: [f32; 3] = [0.0007, 0.0013, 0.0024];
/// How much of the sun the air scatters at each level.
const SCATTER: [f32; 3] = [0.6, 1.0, 1.6];
/// By day, how many times more of the sun the air sends on with the sun on the horizon and with
/// all round the eye in shade, at each level (render-hifi's God Rays).
const LOW_SUN: [f32; 3] = [2.0, 2.5, 3.0];
const IN_SHADE: [f32; 3] = [1.5, 2.5, 4.0];
/// By day, how many times more of the sun the air near the eye sends on at each level.
const DAY_SCATTER: [f32; 3] = [1.2, 1.6, 2.0];
/// How far the day's shafts reach, metres: past it the air sends on only its own share.
const SHAFT_REACH: f32 = 80.0;
/// How strongly the shafts through gaps in leaves and roofs streak from the sun on screen.
const STREAKS: [f32; 3] = [0.6, 1.2, 2.0];
/// How much thinner the air is with the sun low by day, so its forward light does not veil the
/// land toward it.
const LOW_SUN_CLEAR: f32 = 0.8;
/// How fast the air thins with height, metres.
const THINS: f32 = 60.0;
/// How forward the sun's light scatters.
const FORWARD: f32 = 0.55;
/// The uniform's bytes.
const PARAMS_SIZE: u64 = 80;

/// The WGSL of the froxels' compute and of the apply pass, after the shared files.
#[must_use]
pub fn sources() -> [&'static str; 2] {
    [
        include_str!("shaders/air_volume.wgsl"),
        include_str!("shaders/air_apply.wgsl"),
    ]
}

/// The froxels, their passes and their level.
#[derive(Debug)]
pub struct Air {
    /// 0 off; 1 to 3 Low to High.
    pub level: u32,
    /// How much the weather thickens it, 0 (none) to 1 (a storm).
    pub weather: f32,
    volume: wgpu::Texture,
    storage: wgpu::TextureView,
    sampled: wgpu::TextureView,
    params: wgpu::Buffer,
    sampler: wgpu::Sampler,
    compute_layout: wgpu::BindGroupLayout,
    apply_layout: wgpu::BindGroupLayout,
    compute: wgpu::ComputePipeline,
    apply: wgpu::RenderPipeline,
    frame: u32,
}

impl Air {
    /// The passes, drawing into `hdr` targets with the frame's group (`frame_layout`) first.
    #[must_use]
    pub fn new(
        gpu: &Gpu,
        frame_layout: &wgpu::BindGroupLayout,
        common: &str,
        fullscreen: &str,
        hdr: wgpu::TextureFormat,
    ) -> Self {
        use wgpu::ShaderStages as S;
        let device = &gpu.device;
        let volume = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("air"),
            size: wgpu::Extent3d {
                width: FROXELS.0,
                height: FROXELS.1,
                depth_or_array_layers: SLICES,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let storage = volume.create_view(&Default::default());
        let sampled = volume.create_view(&Default::default());
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("air"),
            size: PARAMS_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("air"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = |binding, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let depth = |binding, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let compute_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("air froxels"),
            entries: &[
                depth(0, S::COMPUTE),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: S::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: S::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D3,
                    },
                    count: None,
                },
                uniform(3, S::COMPUTE),
            ],
        });
        let apply_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("air apply"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: S::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: S::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                depth(2, S::FRAGMENT),
                uniform(3, S::FRAGMENT),
            ],
        });
        let [volume_src, apply_src] = sources();
        let cmodule = gpu.shader("air froxels", &format!("{common}\n{volume_src}"));
        let amodule = gpu.shader("air apply", &format!("{common}\n{fullscreen}\n{apply_src}"));
        let clayout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("air froxels"),
            bind_group_layouts: &[Some(frame_layout), Some(&compute_layout)],
            immediate_size: 0,
        });
        let alayout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("air apply"),
            bind_group_layouts: &[Some(frame_layout), Some(&apply_layout)],
            immediate_size: 0,
        });
        let compute = gpu.compute_pipeline(wgpu::ComputePipelineDescriptor {
            label: Some("air froxels"),
            layout: Some(&clayout),
            module: &cmodule,
            entry_point: Some("froxels"),
            compilation_options: Default::default(),
            cache: None,
        });
        // What lies behind is kept by the share that still shows (the alpha), and the air's light
        // is added.
        let over = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::SrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let apply = gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
            label: Some("air apply"),
            layout: Some(&alayout),
            vertex: wgpu::VertexState {
                module: &amodule,
                entry_point: Some("vs_fullscreen"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &amodule,
                entry_point: Some("fs_air"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: hdr,
                    blend: Some(over),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            level: 0,
            weather: 0.0,
            volume,
            storage,
            sampled,
            params,
            sampler,
            compute_layout,
            apply_layout,
            compute,
            apply,
            frame: 0,
        }
    }

    /// Whether it is drawn: a level chosen, the eye outdoors or seeing them, the physical light.
    #[must_use]
    pub fn drawn(&self, physical: bool, sealed: bool) -> bool {
        self.level > 0 && physical && !sealed
    }

    /// Writes this frame's constants: `near` the camera's near plane, `eye_z` its height.
    pub fn prepare(&mut self, gpu: &Gpu, near: f32, eye_z: f32) {
        let k = (self.level.clamp(1, 3) - 1) as usize;
        self.frame = self.frame.wrapping_add(1);
        #[allow(clippy::cast_precision_loss)]
        let jitter = (self.frame as f32 * 0.618_034).fract();
        let thick = 1.0 + 2.5 * self.weather.clamp(0.0, 1.0);
        let words = [
            DENSITY[k] * thick,
            SCATTER[k],
            THINS,
            eye_z - 4.0,
            FIRST,
            REACH,
            #[allow(clippy::cast_precision_loss)]
            {
                SLICES as f32
            },
            FORWARD,
            near,
            jitter,
            1.0,
            0.0,
            LOW_SUN[k],
            IN_SHADE[k],
            DAY_SCATTER[k],
            SHAFT_REACH,
            STREAKS[k],
            LOW_SUN_CLEAR,
            0.0,
            0.0,
        ];
        let bytes: Vec<u8> = words.iter().flat_map(|v| v.to_le_bytes()).collect();
        gpu.queue.write_buffer(&self.params, 0, &bytes);
    }

    /// The froxels lit and summed.
    pub fn encode_froxels(
        &self,
        gpu: &Gpu,
        pass: &mut wgpu::ComputePass<'_>,
        frame: &wgpu::BindGroup,
        shadow_atlas: &wgpu::TextureView,
        shadow_cmp: &wgpu::Sampler,
    ) {
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("air froxels"),
            layout: &self.compute_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(shadow_atlas),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(shadow_cmp),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.storage),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.params.as_entire_binding(),
                },
            ],
        });
        pass.set_pipeline(&self.compute);
        pass.set_bind_group(0, frame, &[]);
        pass.set_bind_group(1, &group, &[]);
        pass.dispatch_workgroups(FROXELS.0.div_ceil(8), FROXELS.1.div_ceil(8), 1);
    }

    /// The air laid over the lit picture, by each pixel's distance in `depth`.
    pub fn encode_apply(
        &self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        frame: &wgpu::BindGroup,
        depth: &wgpu::TextureView,
    ) {
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("air apply"),
            layout: &self.apply_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.sampled),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.params.as_entire_binding(),
                },
            ],
        });
        pass.set_pipeline(&self.apply);
        pass.set_bind_group(0, frame, &[]);
        pass.set_bind_group(1, &group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// The froxel volume, for a test.
    #[must_use]
    pub fn volume(&self) -> &wgpu::Texture {
        &self.volume
    }
}
