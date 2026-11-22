//! UI Blur: the world behind the interface's see-through grounds (its windows, menus, tooltips and
//! chat) made a quarter as big and blurred, which the composite lays under them
//! (`composite.wgsl`); the screens before the world blur their backdrop's lights in place
//! (`drift.wgsl`). Nothing is drawn while no ground asks for it.

use crate::gpu::Gpu;

/// The shader.
#[must_use]
pub fn source() -> &'static str {
    include_str!("shaders/frost.wgsl")
}

/// A see-through ground: where it is on the output, in pixels (left, top, right, bottom), and how
/// strongly the world behind it is blurred (0..1; less while it fades in or out).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Backdrop {
    pub rect: [f32; 4],
    pub strength: f32,
}

/// How many grounds are blurred under at once; past that, the largest.
pub const MOST: usize = 16;

/// The grounds' uniform bytes: their count, the rectangles, then the strengths four to a row.
pub const UNIFORM_SIZE: u64 = 16 + 16 * MOST as u64 + 4 * MOST as u64;

/// How many rows the blurred picture keeps, about: a quarter of a 1080-line output, an eighth of a
/// Retina one, so its cost holds whatever the output's size.
pub const ROWS: u32 = 270;
/// The blur's width (sigma) in the blurred picture's texels: about eight points of the screen,
/// enough to quiet rain and texture while the land's shapes still read through.
pub const SIGMA: f32 = 2.0;
/// Taps either side of a texel: three widths.
pub const TAPS: u32 = 6;

/// The blur's width in output pixels, for a shader that blurs in place (`drift.wgsl`).
#[must_use]
pub fn blur_pixels(output_rows: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let d = divisor(output_rows) as f32;
    SIGMA * d
}

/// How many output pixels a blurred texel spans, across and down: an even number, 2 at least.
#[must_use]
pub fn divisor(output_rows: u32) -> u32 {
    (output_rows.div_ceil(ROWS) / 2 * 2).clamp(2, 16)
}

/// The blur's passes, as the frame graph declares them.
pub const PASSES: [&str; 3] = ["ui blur down", "ui blur across", "ui blur up and down"];

/// The target's format.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// At most [`MOST`] of `backdrops`, the largest, as the shaders' uniform.
#[must_use]
pub fn uniform(backdrops: &[Backdrop]) -> Vec<u8> {
    let mut kept: Vec<Backdrop> = backdrops
        .iter()
        .filter(|b| b.strength > 0.0 && b.rect[2] > b.rect[0] && b.rect[3] > b.rect[1])
        .copied()
        .collect();
    if kept.len() > MOST {
        let area = |b: &Backdrop| (b.rect[2] - b.rect[0]) * (b.rect[3] - b.rect[1]);
        kept.sort_by(|a, b| area(b).total_cmp(&area(a)));
        kept.truncate(MOST);
    }
    let mut bytes = Vec::with_capacity(usize::try_from(UNIFORM_SIZE).unwrap_or(0));
    let n = u32::try_from(kept.len()).unwrap_or(0);
    for v in [n, 0, 0, 0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    for i in 0..MOST {
        let r = kept.get(i).map_or([0.0; 4], |b| b.rect);
        for v in r {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    for i in 0..MOST {
        let s = kept.get(i).map_or(0.0, |b| b.strength.min(1.0));
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    bytes
}

/// The grounds' layout entry, for a shader that reads them.
#[must_use]
pub fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
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

/// The blur's pipelines and its two quarter-size targets.
#[derive(Debug)]
pub struct Frost {
    layout: wgpu::BindGroupLayout,
    down: wgpu::RenderPipeline,
    across: wgpu::RenderPipeline,
    down_blur: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    /// The targets, for an output of this size.
    targets: Option<((u32, u32), [wgpu::TextureView; 2])>,
}

impl Frost {
    #[must_use]
    pub fn new(gpu: &Gpu) -> Self {
        let device = &gpu.device;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui blur"),
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
                uniform_entry(2),
            ],
        });
        let module = gpu.shader("ui blur", source());
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui blur"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |fs: &str| {
            gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                label: Some("ui blur"),
                layout: Some(&pipeline_layout),
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
                    targets: &[Some(FORMAT.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            down: pipeline("fs_down"),
            across: pipeline("fs_across"),
            down_blur: pipeline("fs_down_blur"),
            layout,
            uniform: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ui blur"),
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("ui blur"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            targets: None,
        }
    }

    /// Records the blur of `picture` (laid over an output `output_size` pixels) under `backdrops` into `rec`: the blurred picture, a quarter of the output's size,
    /// or `None` when no ground asks for it.
    pub fn record(
        &mut self,
        gpu: &Gpu,
        rec: &mut crate::graph::FrameRecorder<'_>,
        picture: &wgpu::TextureView,
        output_size: (u32, u32),
        backdrops: &[Backdrop],
    ) -> Option<wgpu::TextureView> {
        let d = divisor(output_size.1);
        let size = (
            output_size.0.div_ceil(d).max(1),
            output_size.1.div_ceil(d).max(1),
        );
        // Each ground on the blurred picture, widened by twice the blur's reach: the passes draw
        // there alone.
        #[allow(clippy::cast_precision_loss)]
        let pad = 2.0 * (TAPS + 1) as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a rectangle on a quarter-size target, a thousand texels or so.
        let texel = |v: f32, top: u32| (v.max(0.0) as u32).min(top);
        let scissors: Vec<(u32, u32, u32, u32)> = backdrops
            .iter()
            .filter(|b| b.strength > 0.0)
            .take(MOST)
            .filter_map(|b| {
                let r = b.rect;
                #[allow(clippy::cast_precision_loss)]
                let d = d as f32;
                let x0 = texel(r[0] / d - pad, size.0);
                let y0 = texel(r[1] / d - pad, size.1);
                let x1 = texel((r[2] / d + pad).ceil(), size.0);
                let y1 = texel((r[3] / d + pad).ceil(), size.1);
                (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
            })
            .collect();
        if scissors.is_empty() {
            Self::skip(rec);
            return None;
        }
        let blurred = self.draw(gpu, rec, picture, output_size, (size, d), &scissors);
        if blurred.is_none() {
            Self::skip(rec);
        }
        blurred
    }

    /// Passes over the blur's passes: no ground asks for it this frame.
    pub fn skip(rec: &mut crate::graph::FrameRecorder<'_>) {
        for name in PASSES {
            rec.skip(name, crate::graph::PassKind::Render);
        }
    }

    fn draw(
        &mut self,
        gpu: &Gpu,
        rec: &mut crate::graph::FrameRecorder<'_>,
        picture: &wgpu::TextureView,
        output_size: (u32, u32),
        (size, d): ((u32, u32), u32),
        scissors: &[(u32, u32, u32, u32)],
    ) -> Option<wgpu::TextureView> {
        if self.targets.as_ref().is_none_or(|(s, _)| *s != size) {
            let make = |label| {
                gpu.device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some(label),
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
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            };
            self.targets = Some((size, [make("ui blur a"), make("ui blur b")]));
        }
        let (_, [a, b]) = self.targets.as_ref()?;
        let f = |v: u32| {
            #[allow(clippy::cast_precision_loss)]
            let v = v.max(1) as f32;
            1.0 / v
        };
        #[allow(clippy::cast_precision_loss)]
        let words = [
            f(output_size.0),
            f(output_size.1),
            f(size.0),
            f(size.1),
            SIGMA,
            TAPS as f32,
            d as f32,
            0.0,
        ];
        let bytes: Vec<u8> = words.iter().flat_map(|v| v.to_le_bytes()).collect();
        gpu.queue.write_buffer(&self.uniform, 0, &bytes);
        let group = |from: &wgpu::TextureView| {
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ui blur"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(from),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.uniform.as_entire_binding(),
                    },
                ],
            })
        };
        for (name, pipeline, from, to) in [
            (PASSES[0], &self.down, picture, a),
            (PASSES[1], &self.across, a, b),
            (PASSES[2], &self.down_blur, b, a),
        ] {
            let g = group(from);
            let mut pass = rec.render(
                name,
                &[Some(wgpu::RenderPassColorAttachment {
                    view: to,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                None,
            );
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &g, &[]);
            for &(x, y, w, h) in scissors {
                pass.set_scissor_rect(x, y, w, h);
                pass.draw(0..3, 0..1);
            }
        }
        Some(a.clone())
    }
}
