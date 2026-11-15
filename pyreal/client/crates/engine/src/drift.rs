//! The backdrop of the screens before the world: portal space's slate void with its violet
//! lights floating through it, and the interface laid over it.

use crate::gpu::Gpu;

/// The shader.
#[must_use]
pub fn source() -> &'static str {
    include_str!("shaders/drift.wgsl")
}

/// The backdrop's pipeline and what it binds.
pub struct Drift {
    module: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    /// Bound where there is no interface.
    blank: wgpu::TextureView,
    pipeline: Option<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
}

impl std::fmt::Debug for Drift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Drift").finish_non_exhaustive()
    }
}

impl Drift {
    #[must_use]
    pub fn new(gpu: &Gpu) -> Self {
        let device = &gpu.device;
        let module = gpu.shader("portal lights", source());
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("portal lights"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("portal lights"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let blank = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("portal lights blank"),
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
            module,
            layout,
            uniforms,
            blank,
            pipeline: None,
        }
    }

    fn pipeline(&mut self, gpu: &Gpu, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        if let Some((f, p)) = self.pipeline.as_ref() {
            if *f == format {
                return p.clone();
            }
        }
        let pl = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("portal lights"),
                bind_group_layouts: &[Some(&self.layout)],
                immediate_size: 0,
            });
        let p = gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
            label: Some("portal lights"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &self.module,
                entry_point: Some("vs_full"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.module,
                entry_point: Some("fs_drift"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        self.pipeline = Some((format, p.clone()));
        p
    }

    /// Draws the backdrop `seconds` into its drift over the whole of `target` (`format`, `size`),
    /// the interface `ui` over it.
    pub fn render(
        &mut self,
        gpu: &Gpu,
        seconds: f64,
        ui: Option<&crate::temporal::Ui>,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        size: (u32, u32),
    ) {
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let values: [f32; 8] = [
            // Wrapped well before f32 loses the fractions the drift moves by.
            (seconds % 20_000.0) as f32,
            size.0 as f32 / size.1.max(1) as f32,
            size.0.max(1) as f32,
            size.1.max(1) as f32,
            f32::from(u8::from(ui.is_some_and(|u| u.gamma))),
            f32::from(u8::from(ui.is_some())),
            0.0,
            0.0,
        ];
        let bytes: Vec<u8> = values.iter().flat_map(|f| f.to_ne_bytes()).collect();
        gpu.queue.write_buffer(&self.uniforms, 0, &bytes);
        let pipeline = self.pipeline(gpu, format);
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("portal lights"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        ui.map_or(&self.blank, |u| &u.view),
                    ),
                },
            ],
        });
        let mut enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("portal lights"),
            });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("portal lights"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        gpu.queue.submit([enc.finish()]);
    }
}
