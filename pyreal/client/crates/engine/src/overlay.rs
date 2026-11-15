//! The UI's picture: a client's interface drawn as textured triangles into a picture of the
//! output's size, which the composite lays over the world ([`crate::temporal::Ui`]).
//!
//! The pictures and the triangles' colours are in their own sRGB encoding and are blended as those
//! numbers, premultiplied, as the old fixed-function devices drew a UI; the picture is laid over
//! the world in that encoding too ([`crate::temporal::Ui::gamma`]).
//!
//! Batches that stand in the world ([`Overlay::push_world`]: a name over a body, a plugin's line
//! on the ground) carry each vertex's distance from the eye and are drawn once the world is: under
//! what the interface drew, each pixel hidden where the world's depth stands nearer.

use std::collections::HashMap;

use crate::gpu::Gpu;

/// The overlay's shader, and the whole-target one.
#[must_use]
pub fn sources() -> [&'static str; 2] {
    [
        include_str!("shaders/overlay.wgsl"),
        include_str!("shaders/whole.wgsl"),
    ]
}

/// The picture's format: encoded numbers, blended as they are.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// How a batch is laid over what is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Blend {
    /// The picture times the vertex colour.
    Image,
    /// The vertex colour, the picture giving only coverage.
    Text,
    /// A selection highlight (a pale wash: the UI cannot see the world under it).
    Invert,
    /// A picture already premultiplied (a preview space's).
    Premultiplied,
}

impl Blend {
    const ALL: [Self; 4] = [Self::Image, Self::Text, Self::Invert, Self::Premultiplied];

    fn entry(self) -> &'static str {
        match self {
            Self::Image => "fs_image",
            Self::Text => "fs_text",
            Self::Invert => "fs_invert",
            Self::Premultiplied => "fs_premultiplied",
        }
    }

    fn world_entry(self) -> &'static str {
        match self {
            Self::Image => "fs_world_image",
            Self::Text => "fs_world_text",
            Self::Invert => "fs_world_invert",
            Self::Premultiplied => "fs_world_premultiplied",
        }
    }
}

/// How a batch's picture is sampled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Sampling {
    pub point: bool,
    pub wrap_u: bool,
    pub wrap_v: bool,
}

impl Sampling {
    fn index(self) -> usize {
        usize::from(self.point) | usize::from(self.wrap_u) << 1 | usize::from(self.wrap_v) << 2
    }
}

/// One vertex: clip-space position (`y` up), colour `0xAARRGGBB`, texture coordinate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub colour: u32,
    pub uv: [f32; 2],
}

const VERTEX_SIZE: u64 = 24;

/// A picture the batches name.
#[derive(Debug)]
struct Picture {
    view: wgpu::TextureView,
    /// Bind groups made for it, by sampling.
    groups: [Option<wgpu::BindGroup>; 8],
}

/// One draw of the frame's list.
#[derive(Debug, Clone, Copy)]
struct Batch {
    key: u64,
    blend: Blend,
    sampling: Sampling,
    first: u32,
    count: u32,
    /// A scissor, `(x, y, w, h)` in pixels.
    clip: Option<(u32, u32, u32, u32)>,
    /// It stands in the world, hidden behind what the world draws nearer.
    world: bool,
}

/// The UI's pictures, its pipelines and the frame's list of triangles.
#[derive(Debug)]
pub struct Overlay {
    layout: wgpu::BindGroupLayout,
    pipelines: HashMap<Blend, wgpu::RenderPipeline>,
    /// The world's batches' pipelines (laid under what is drawn, against the world's depth), and
    /// the depth's group's layout.
    world_pipelines: HashMap<Blend, wgpu::RenderPipeline>,
    depth_layout: wgpu::BindGroupLayout,
    samplers: Vec<wgpu::Sampler>,
    // ORDER-OK: looked up by key, never iterated.
    pictures: HashMap<u64, Picture>,
    vertices: Vec<u8>,
    /// The world's batches' vertices, kept apart: they are uploaded after the rest, their
    /// triangles in reverse, so that laid under what is there the last asked still lands on top.
    world_vertices: Vec<u8>,
    batches: Vec<Batch>,
    vertex_buffer: Option<wgpu::Buffer>,
    target: Option<(wgpu::Texture, wgpu::TextureView)>,
    whole_layout: wgpu::BindGroupLayout,
    module: wgpu::ShaderModule,
    /// Whole-target pipelines, by target format and entry point.
    // ORDER-OK: looked up by key.
    whole: HashMap<(wgpu::TextureFormat, &'static str), wgpu::RenderPipeline>,
}

impl Overlay {
    #[must_use]
    pub fn new(gpu: &Gpu) -> Self {
        let device = &gpu.device;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("overlay"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let module = gpu.shader("overlay", sources()[0]);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("overlay"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("overlay world depth"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let world_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("overlay world"),
            bind_group_layouts: &[Some(&layout), Some(&depth_layout)],
            immediate_size: 0,
        });
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        // Laid under what is there: the interface drawn first stays over the world's marks.
        let under = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let make = |layout: &wgpu::PipelineLayout,
                    vs: &str,
                    fs: &str,
                    blend: wgpu::BlendComponent| {
            gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                label: Some("overlay"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: VERTEX_SIZE,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Uint32, 2 => Float32x2],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: FORMAT,
                        blend: Some(wgpu::BlendState {
                            color: blend,
                            alpha: blend,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let mut pipelines = HashMap::new();
        let mut world_pipelines = HashMap::new();
        for blend in Blend::ALL {
            pipelines.insert(
                blend,
                make(&pipeline_layout, "vs", blend.entry(), premultiplied),
            );
            world_pipelines.insert(
                blend,
                make(&world_layout, "vs_world", blend.world_entry(), under),
            );
        }
        let samplers = (0..8usize)
            .map(|i| {
                let filter = if i & 1 != 0 {
                    wgpu::FilterMode::Nearest
                } else {
                    wgpu::FilterMode::Linear
                };
                let mode = |wrap: bool| {
                    if wrap {
                        wgpu::AddressMode::Repeat
                    } else {
                        wgpu::AddressMode::ClampToEdge
                    }
                };
                device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("overlay"),
                    address_mode_u: mode(i & 2 != 0),
                    address_mode_v: mode(i & 4 != 0),
                    mag_filter: filter,
                    min_filter: filter,
                    mipmap_filter: wgpu::MipmapFilterMode::Linear,
                    ..Default::default()
                })
            })
            .collect();
        let whole_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("whole picture"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let module = gpu.shader("whole picture", sources()[1]);
        Self {
            layout,
            pipelines,
            world_pipelines,
            depth_layout,
            samplers,
            pictures: HashMap::new(),
            vertices: Vec::new(),
            world_vertices: Vec::new(),
            batches: Vec::new(),
            vertex_buffer: None,
            target: None,
            whole_layout,
            module,
            whole: HashMap::new(),
        }
    }

    fn whole_pipeline(
        &mut self,
        gpu: &Gpu,
        format: wgpu::TextureFormat,
        entry: &'static str,
    ) -> wgpu::RenderPipeline {
        if let Some(p) = self.whole.get(&(format, entry)) {
            return p.clone();
        }
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("whole picture"),
                bind_group_layouts: &[Some(&self.whole_layout)],
                immediate_size: 0,
            });
        let p = gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
            label: Some("whole picture"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &self.module,
                entry_point: Some("vs_whole"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.module,
                entry_point: Some(entry),
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
        self.whole.insert((format, entry), p.clone());
        p
    }

    fn draw_whole(
        &mut self,
        gpu: &Gpu,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        entry: &'static str,
        source: Option<&wgpu::TextureView>,
    ) {
        let pipeline = source.map(|_| self.whole_pipeline(gpu, format, entry));
        let group = source.map(|v| {
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("whole picture"),
                layout: &self.whole_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(v),
                }],
            })
        });
        let mut enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("whole picture"),
            });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("whole picture"),
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
            if let (Some(p), Some(g)) = (pipeline.as_ref(), group.as_ref()) {
                pass.set_pipeline(p);
                pass.set_bind_group(0, g, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        gpu.queue.submit([enc.finish()]);
    }

    /// Clears `target` (of `format`, the output's size) to black and lays the UI's picture `ui`
    /// over it: a frame with no world.
    pub fn lay_over(
        &mut self,
        gpu: &Gpu,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        ui: Option<&crate::temporal::Ui>,
    ) {
        self.draw_whole(gpu, target, format, "fs_ui_alone", ui.map(|u| &u.view));
    }

    /// Copies the finished frame `source` (the target's size and format) to `target`.
    pub fn blit(
        &mut self,
        gpu: &Gpu,
        source: &wgpu::Texture,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) {
        let view = source.create_view(&Default::default());
        self.draw_whole(gpu, target, format, "fs_copy", Some(&view));
    }

    /// Uploads `levels` (tightly packed RGBA8 rows, the first `width` by `height`, each next one
    /// half the last) under `key`, replacing what was there.
    pub fn upload(&mut self, gpu: &Gpu, key: u64, width: u32, height: u32, levels: &[Vec<u8>]) {
        let count = u32::try_from(levels.len().max(1)).unwrap_or(1);
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("overlay picture"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, bytes) in levels.iter().enumerate() {
            let level = u32::try_from(level).unwrap_or(0);
            let (w, h) = ((width >> level).max(1), (height >> level).max(1));
            if bytes.len() < (w * h * 4) as usize {
                break;
            }
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.insert_view(key, texture.create_view(&Default::default()));
    }

    /// Names `view` (a picture of [`FORMAT`] made elsewhere) `key`.
    pub fn insert_view(&mut self, key: u64, view: wgpu::TextureView) {
        self.pictures.insert(
            key,
            Picture {
                view,
                groups: Default::default(),
            },
        );
    }

    /// Forgets `key`'s picture; whether there was one.
    pub fn release(&mut self, key: u64) -> bool {
        self.pictures.remove(&key).is_some()
    }

    #[must_use]
    pub fn contains(&self, key: u64) -> bool {
        self.pictures.contains_key(&key)
    }

    /// Starts the frame's list.
    pub fn begin(&mut self) {
        self.vertices.clear();
        self.world_vertices.clear();
        self.batches.clear();
    }

    /// Adds triangles (three vertices each) over picture `key`, clipped to `clip` when given.
    pub fn push(
        &mut self,
        key: u64,
        blend: Blend,
        sampling: Sampling,
        vertices: &[Vertex],
        clip: Option<(u32, u32, u32, u32)>,
    ) {
        self.push_as(key, blend, sampling, vertices, clip, false);
    }

    /// [`Self::push`] for triangles standing in the world: each vertex's `position[2]` is its
    /// distance from the eye, metres (zero: never hidden). They are drawn once the world is
    /// ([`Self::draw_world`]), under the rest, hidden wherever the world stands nearer.
    pub fn push_world(
        &mut self,
        key: u64,
        blend: Blend,
        sampling: Sampling,
        vertices: &[Vertex],
        clip: Option<(u32, u32, u32, u32)>,
    ) {
        self.push_as(key, blend, sampling, vertices, clip, true);
    }

    fn push_as(
        &mut self,
        key: u64,
        blend: Blend,
        sampling: Sampling,
        vertices: &[Vertex],
        clip: Option<(u32, u32, u32, u32)>,
        world: bool,
    ) {
        if vertices.is_empty() {
            return;
        }
        let list = if world {
            &mut self.world_vertices
        } else {
            &mut self.vertices
        };
        let first = u32::try_from(list.len() as u64 / VERTEX_SIZE).unwrap_or(0);
        for v in vertices {
            for f in v.position {
                list.extend_from_slice(&f.to_le_bytes());
            }
            list.extend_from_slice(&v.colour.to_le_bytes());
            for f in v.uv {
                list.extend_from_slice(&f.to_le_bytes());
            }
        }
        let count = u32::try_from(vertices.len()).unwrap_or(0);
        match self.batches.last_mut() {
            Some(b)
                if b.key == key
                    && b.blend == blend
                    && b.sampling == sampling
                    && b.clip == clip
                    && b.world == world
                    && b.first + b.count == first =>
            {
                b.count += count;
            }
            _ => self.batches.push(Batch {
                key,
                blend,
                sampling,
                first,
                count,
                clip,
                world,
            }),
        }
    }

    /// Whether this frame's list has triangles standing in the world.
    #[must_use]
    pub fn has_world(&self) -> bool {
        self.batches.iter().any(|b| b.world)
    }

    /// Draws the frame's triangles standing in the world into the picture [`Self::finish`] drew,
    /// under what it holds, through `pass` (whose target is [`Self::view`]): each pixel hidden
    /// where `depth` (the world's, reverse Z with no far plane from `near`, `scale` of the
    /// picture's size) stands nearer.
    pub fn draw_world(
        &self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        depth: &wgpu::TextureView,
        scale: (f32, f32),
        near: f32,
    ) {
        let (Some(vb), Some((target, _))) = (self.vertex_buffer.as_ref(), self.target.as_ref())
        else {
            return;
        };
        let size = (target.width(), target.height());
        use wgpu::util::DeviceExt;
        let params: Vec<u8> = [scale.0, scale.1, near, 0.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let uniform = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("overlay world depth"),
                contents: &params,
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("overlay world depth"),
            layout: &self.depth_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_bind_group(1, &group, &[]);
        let ui = u32::try_from(self.vertices.len() as u64 / VERTEX_SIZE).unwrap_or(0);
        let world = u32::try_from(self.world_vertices.len() as u64 / VERTEX_SIZE).unwrap_or(0);
        // Laid under what is there, so the last asked goes first: the batches in reverse, each
        // one's triangles reversed at upload ([`Self::finish`]).
        for b in self.batches.iter().rev().filter(|b| b.world) {
            let Some(g) = self
                .pictures
                .get(&b.key)
                .and_then(|p| p.groups[b.sampling.index()].as_ref())
            else {
                continue;
            };
            let (x, y, w, h) = b.clip.unwrap_or((0, 0, size.0, size.1));
            let (x, y) = (x.min(size.0), y.min(size.1));
            let (w, h) = (w.min(size.0 - x), h.min(size.1 - y));
            if w == 0 || h == 0 {
                continue;
            }
            pass.set_scissor_rect(x, y, w, h);
            pass.set_pipeline(&self.world_pipelines[&b.blend]);
            pass.set_bind_group(0, g, &[]);
            let first = ui + world - b.first - b.count;
            pass.draw(first..first + b.count, 0..1);
        }
    }

    /// The picture [`Self::finish`] draws into.
    #[must_use]
    pub fn view(&self) -> Option<&wgpu::TextureView> {
        self.target.as_ref().map(|(_, v)| v)
    }

    /// Whether this frame's list draws anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.batches.is_empty()
    }

    /// Draws the frame's list into the picture, `size` big, cleared first; the picture for the
    /// composite ([`crate::temporal::Ui`]).
    pub fn finish(&mut self, gpu: &Gpu, size: (u32, u32)) -> crate::temporal::Ui {
        let size = (size.0.max(1), size.1.max(1));
        let fits = self
            .target
            .as_ref()
            .is_some_and(|(t, _)| (t.width(), t.height()) == size);
        if !fits {
            let t = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("ui"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let v = t.create_view(&Default::default());
            self.target = Some((t, v));
        }
        let bytes = (self.vertices.len() + self.world_vertices.len()) as u64;
        if bytes > 0 && self.vertex_buffer.as_ref().is_none_or(|b| b.size() < bytes) {
            self.vertex_buffer = Some(gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("overlay vertices"),
                size: bytes.next_power_of_two().max(64 << 10),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if let (Some(b), true) = (self.vertex_buffer.as_ref(), bytes > 0) {
            gpu.queue.write_buffer(b, 0, &self.vertices);
            if !self.world_vertices.is_empty() {
                let triangle = usize::try_from(3 * VERTEX_SIZE).expect("a triangle is small");
                let reversed: Vec<u8> = self
                    .world_vertices
                    .chunks_exact(triangle)
                    .rev()
                    .flatten()
                    .copied()
                    .collect();
                gpu.queue
                    .write_buffer(b, self.vertices.len() as u64, &reversed);
            }
        }
        // The bind groups the list needs.
        for b in &self.batches {
            let k = b.sampling.index();
            if let Some(p) = self.pictures.get_mut(&b.key) {
                if p.groups[k].is_none() {
                    p.groups[k] = Some(gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("overlay picture"),
                        layout: &self.layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&p.view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&self.samplers[k]),
                            },
                        ],
                    }));
                }
            }
        }
        let (_, view) = self.target.as_ref().expect("made above");
        let mut enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("overlay"),
            });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(vb) = self.vertex_buffer.as_ref() {
                pass.set_vertex_buffer(0, vb.slice(..));
                for b in self.batches.iter().filter(|b| !b.world) {
                    let Some(group) = self
                        .pictures
                        .get(&b.key)
                        .and_then(|p| p.groups[b.sampling.index()].as_ref())
                    else {
                        continue;
                    };
                    let (x, y, w, h) = b.clip.unwrap_or((0, 0, size.0, size.1));
                    let (x, y) = (x.min(size.0), y.min(size.1));
                    let (w, h) = (w.min(size.0 - x), h.min(size.1 - y));
                    if w == 0 || h == 0 {
                        continue;
                    }
                    pass.set_scissor_rect(x, y, w, h);
                    pass.set_pipeline(&self.pipelines[&b.blend]);
                    pass.set_bind_group(0, group, &[]);
                    pass.draw(b.first..b.first + b.count, 0..1);
                }
            }
        }
        gpu.queue.submit([enc.finish()]);
        crate::temporal::Ui {
            view: view.clone(),
            gamma: true,
        }
    }

    /// The picture drawn last, for a test or a capture.
    #[must_use]
    pub fn texture(&self) -> Option<&wgpu::Texture> {
        self.target.as_ref().map(|(t, _)| t)
    }
}
