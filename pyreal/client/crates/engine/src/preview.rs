//! Preview spaces: a few meshes from the pools (a character on the creation screen, the paper
//! doll, an examined object, the portal swirl) drawn with their own camera and one light into a
//! picture the UI lays down ([`crate::overlay`]): sRGB-encoded numbers, premultiplied, over a
//! clear background.

use glam::{Affine3A, Mat4, Vec3};

use crate::gpu::Gpu;
use crate::materials::MaterialTable;
use crate::scene::{MeshId, Scene};

/// One drawn mesh and where it stands, and how much of it shows (a particle's fade; 1 a part).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewDraw {
    pub mesh: MeshId,
    pub transform: Affine3A,
    pub alpha: f32,
}

impl PreviewDraw {
    /// A part, drawn whole.
    #[must_use]
    pub fn part(mesh: MeshId, transform: Affine3A) -> Self {
        Self {
            mesh,
            transform,
            alpha: 1.0,
        }
    }
}

/// The live particles of `driver`'s emitters as preview draws seen from `eye`: each its emitter's
/// picture at the particle's frame and scale, turned to the eye as its degrade mode turns it,
/// faded by its translucency. Their pictures must be ready ([`crate::particles::prepare_sprite`]
/// for [`crate::particles::Particles::missing_sprites`]); far to near, for the blending.
#[must_use]
pub fn particle_draws(
    particles: &crate::particles::Particles,
    driver: &pyreal_animation::MotionDriver,
    eye: Vec3,
) -> Vec<PreviewDraw> {
    let mut out: Vec<(f32, PreviewDraw)> = Vec::new();
    for e in driver.particles.iter() {
        let Some(Some(sprite)) = particles
            .sprites
            .get(&crate::particles::sprite_of(&e.info).0)
        else {
            continue;
        };
        for p in e.live() {
            if p.translucency >= 1.0 || p.scale <= 0.0 {
                continue;
            }
            let t = crate::asset::object::frame_affine(&p.frame)
                * Affine3A::from_scale(Vec3::splat(p.scale));
            let at = Vec3::from(t.translation);
            out.push((
                at.distance(eye),
                PreviewDraw {
                    mesh: sprite.mesh,
                    transform: turned(t, sprite.mode, eye),
                    alpha: (1.0 - p.translucency).clamp(0.0, 1.0),
                },
            ));
        }
    }
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    out.into_iter().map(|(_, d)| d).collect()
}

/// `t` turned to `eye` as degrade mode `mode` turns a part (the world's vertex shader's
/// `billboard`): 2 aims its y axis at the eye, 3 to 5 keep their x, y or z axis and turn the next
/// toward the eye.
fn turned(t: Affine3A, mode: u32, eye: Vec3) -> Affine3A {
    if !(2..=5).contains(&mode) {
        return t;
    }
    let c = [
        Vec3::from(t.matrix3.x_axis),
        Vec3::from(t.matrix3.y_axis),
        Vec3::from(t.matrix3.z_axis),
    ];
    let s = c.map(Vec3::length);
    let mut a = [0, 1, 2].map(|k| c[k] / s[k].max(1e-6));
    let origin = Vec3::from(t.translation);
    let v = (origin - eye).try_normalize().unwrap_or(Vec3::Z);
    if mode == 2 {
        let x = v.cross(Vec3::Z).try_normalize().unwrap_or(Vec3::X);
        a = [x, v, x.cross(v)];
    } else {
        let nxt = [2usize, 0, 1];
        let k = (mode - 3) as usize;
        let axis = a[k];
        let p = (v - axis * axis.dot(v))
            .try_normalize()
            .unwrap_or(a[nxt[k]]);
        if p.dot(axis).abs() <= 0.001 {
            a[nxt[k]] = p;
            a[nxt[nxt[k]]] = p.cross(axis);
        }
    }
    Affine3A::from_cols(
        (a[0] * s[0]).into(),
        (a[1] * s[1]).into(),
        (a[2] * s[2]).into(),
        origin.into(),
    )
}

/// The camera and the light.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewView {
    pub view_proj: Mat4,
    /// Toward the light.
    pub light_dir: Vec3,
    /// The light's colour times its intensity, and the ambient.
    pub light: Vec3,
    pub ambient: Vec3,
}

const RECORD_SIZE: u64 = 64;
const FRAME_SIZE: u64 = 112;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The pass: its pipelines and buffers.
#[derive(Debug)]
pub struct PreviewPass {
    layout: wgpu::BindGroupLayout,
    opaque: wgpu::RenderPipeline,
    blended: wgpu::RenderPipeline,
    frame: wgpu::Buffer,
    records: Option<wgpu::Buffer>,
    empty: wgpu::BindGroup,
}

/// The material table's records and flags, cut from the shared header (which also declares the
/// frame's uniform, at a binding this pass uses for its own).
fn material_defs() -> &'static str {
    let common = include_str!("shaders/common.wgsl");
    let start = common.find("struct Material {").unwrap_or(0);
    let end = common[start..]
        .find("@group(0) @binding(0)")
        .map_or(common.len(), |e| start + e);
    &common[start..end]
}

/// The pass's shader.
#[must_use]
pub fn source() -> String {
    format!(
        "enable wgpu_binding_array;\n{}\n{}\n{}",
        material_defs(),
        include_str!("shaders/materials.wgsl"),
        include_str!("shaders/preview.wgsl")
    )
}

impl PreviewPass {
    #[must_use]
    pub fn new(gpu: &Gpu, materials: &MaterialTable) -> Self {
        let device = &gpu.device;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("preview"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let empty = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("preview (unused)"),
            entries: &[],
        });
        let module = gpu.shader("preview", &source());
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("preview"),
            bind_group_layouts: &[Some(&layout), Some(&empty), Some(&materials.layout)],
            immediate_size: 0,
        });
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let make = |blend: bool| {
            gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                label: Some("preview"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: crate::asset::mesh::VERTEX_SIZE,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x3,
                            1 => Snorm16x2,
                            2 => Float32x2,
                            3 => Uint32,
                            4 => Uint32,
                        ],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: crate::overlay::FORMAT,
                        blend: Some(wgpu::BlendState {
                            color: premultiplied,
                            alpha: premultiplied,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(!blend),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let opaque = make(false);
        let blended = make(true);
        let frame = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("preview frame"),
            size: FRAME_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let empty = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("preview (unused)"),
            layout: &empty,
            entries: &[],
        });
        Self {
            layout,
            opaque,
            blended,
            frame,
            records: None,
            empty,
        }
    }

    /// Draws `draws` seen as `view` into `target` (of [`crate::overlay::FORMAT`]), cleared to
    /// nothing first. The pools and the material table must be uploaded.
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        scene: &Scene,
        materials: &MaterialTable,
        view: &PreviewView,
        draws: &[PreviewDraw],
        target: &wgpu::Texture,
    ) {
        let Some(material_group) = materials.group() else {
            return;
        };
        // One record per drawn sub-mesh: opaque ones first, then the blended.
        let mut records: Vec<u8> = Vec::new();
        let mut opaque: Vec<(u32, u32, u32)> = Vec::new();
        let mut blended: Vec<(u32, u32, u32)> = Vec::new();
        for d in draws {
            let Some(mesh) = scene.mesh(d.mesh) else {
                continue;
            };
            for sub in &mesh.draws {
                let k = u32::try_from(records.len() as u64 / RECORD_SIZE).unwrap_or(0);
                let m = d.transform.matrix3;
                let t = d.transform.translation;
                for r in 0..3 {
                    for v in [m.x_axis[r], m.y_axis[r], m.z_axis[r], t[r]] {
                        records.extend_from_slice(&v.to_le_bytes());
                    }
                }
                for v in [sub.material & 0x00FF_FFFF, d.alpha.to_bits(), 0, 0] {
                    records.extend_from_slice(&v.to_le_bytes());
                }
                let entry = (sub.first_index, sub.index_count, k);
                if sub.pipeline.deferred() {
                    opaque.push(entry);
                } else {
                    blended.push(entry);
                }
            }
        }
        let bytes = records.len() as u64;
        if bytes > 0 && self.records.as_ref().is_none_or(|b| b.size() < bytes) {
            self.records = Some(gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("preview records"),
                size: bytes.next_power_of_two().max(4096),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let mut frame = Vec::with_capacity(usize::try_from(FRAME_SIZE).unwrap_or(0));
        for v in view.view_proj.to_cols_array() {
            frame.extend_from_slice(&v.to_le_bytes());
        }
        for v in [
            view.light_dir.extend(0.0),
            view.light.extend(0.0),
            view.ambient.extend(0.0),
        ] {
            for x in v.to_array() {
                frame.extend_from_slice(&x.to_le_bytes());
            }
        }
        gpu.queue.write_buffer(&self.frame, 0, &frame);
        if let (Some(b), true) = (self.records.as_ref(), bytes > 0) {
            gpu.queue.write_buffer(b, 0, &records);
        }
        let depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("preview depth"),
            size: target.size(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&Default::default());
        let target_view = target.create_view(&Default::default());
        let group = self.records.as_ref().map(|records| {
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("preview"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.frame.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: records.as_entire_binding(),
                    },
                ],
            })
        });
        let mut enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("preview"),
            });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("preview"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(group) = group.as_ref() {
                pass.set_bind_group(0, group, &[]);
                pass.set_bind_group(1, &self.empty, &[]);
                pass.set_bind_group(2, material_group, &[]);
                pass.set_vertex_buffer(0, scene.vertices.buffer.slice(..));
                pass.set_index_buffer(scene.indices.buffer.slice(..), wgpu::IndexFormat::Uint32);
                for (pipeline, list) in [(&self.opaque, &opaque), (&self.blended, &blended)] {
                    pass.set_pipeline(pipeline);
                    for &(first, count, k) in list {
                        pass.draw_indexed(first..first + count, 0, k..k + 1);
                    }
                }
            }
        }
        gpu.queue.submit([enc.finish()]);
    }
}
