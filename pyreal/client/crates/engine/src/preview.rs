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
    /// A creature's light drawn as the world draws it ([`crate::glow`]): its word and its flags,
    /// as the particle pass gives them.
    pub glow: Option<(u32, u32)>,
}

impl PreviewDraw {
    /// A part, drawn whole.
    #[must_use]
    pub fn part(mesh: MeshId, transform: Affine3A) -> Self {
        Self {
            mesh,
            transform,
            alpha: 1.0,
            glow: None,
        }
    }
}

/// The most lamps a preview lights with.
pub const MAX_LAMPS: usize = 4;

/// A body's particles in a preview space, drawn as the world draws them: its fire as one body of
/// fire ([`crate::body_fire`]), its creature's lights as light ([`crate::glow`]), the rest as
/// their pictures; and the lamps its fire and lights carry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PreviewParticles {
    /// Far to near, for the blending.
    pub draws: Vec<PreviewDraw>,
    /// Its fire's records ([`crate::flames::flame_bytes`]): a body, then its points.
    pub fire: Vec<crate::flames::Flame>,
    /// The world's lamps for them.
    pub lamps: Vec<crate::lamps::Lamp>,
}

/// The live particles of `driver`'s emitters seen from `eye`, as the world's particle pass draws
/// them ([`crate::particles::Particles::gather`]): each picture at the particle's frame and scale,
/// turned to the eye as its degrade mode turns it, faded by its translucency. Their pictures must
/// be ready ([`crate::particles::prepare_sprite`] for
/// [`crate::particles::Particles::missing_sprites`]).
#[must_use]
pub fn particles(
    particles: &mut crate::particles::Particles,
    driver: &pyreal_animation::MotionDriver,
    eye: Vec3,
) -> PreviewParticles {
    use crate::roles::Role;
    let roles: Vec<Option<Role>> = driver.particles.iter().map(|e| particles.role(e)).collect();
    let mut made = PreviewParticles::default();
    // A molten body's fire is its glowing cracks: its flames are left out.
    let fire_bodies = particles.fire_bodies;
    if fire_bodies && !crate::body_fire::molten_core(driver) {
        let burning = driver
            .particles
            .iter()
            .zip(&roles)
            .filter_map(|(e, r)| match r {
                Some(Role::Fire(c)) => Some((e, *c)),
                _ => None,
            });
        if let Some(f) = crate::body_fire::gather(driver, burning, Vec3::ZERO, 0) {
            let (flames, lamps) =
                crate::body_fire::frame(std::slice::from_ref(&f), &[], eye, |_| true, usize::MAX);
            made.fire = flames;
            made.lamps.extend(lamps);
        }
    }
    // The creature's light's lamp: where it is, weighed by how much shows, and its colour.
    let mut lamp = (Vec3::ZERO, Vec3::ZERO, 0.0f32);
    let mut out: Vec<(f32, PreviewDraw)> = Vec::new();
    for (e, role) in driver.particles.iter().zip(&roles) {
        let Some(Some(sprite)) = particles
            .sprites
            .get(&crate::particles::sprite_of(&e.info).0)
        else {
            continue;
        };
        let role = match role {
            Some(Role::Fire(_)) if fire_bodies => continue,
            Some(Role::Fire(_)) => crate::roles::lone(sprite, &e.info),
            Some(r) => *r,
            None => continue,
        };
        let glow = match (role, sprite.glow) {
            (Role::Glow(kind), Some(g)) if particles.smoke => Some((kind, g)),
            _ => None,
        };
        if let Some((_, g)) = glow.filter(|(_, g)| !g.glint()) {
            let shows: f32 = e
                .live()
                .map(|p| (1.0 - p.translucency).clamp(0.0, 1.0) * p.scale)
                .sum();
            let o = e.object_origin;
            lamp.0 += Vec3::new(o.x, o.y, o.z) * shows;
            lamp.1 += g.colour * shows;
            lamp.2 += shows;
        }
        for p in e.live() {
            if p.translucency >= 1.0 || p.scale <= 0.0 {
                continue;
            }
            let t = crate::asset::object::frame_affine(&p.frame)
                * Affine3A::from_scale(Vec3::splat(p.scale));
            let at = Vec3::from(t.translation);
            let alpha = (1.0 - p.translucency).clamp(0.0, 1.0);
            let glow = glow.map(|(kind, g)| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 to 1023
                let age = ((p.lifetime / p.lifespan.max(1e-3)).clamp(0.0, 1.0) * 1023.0) as u32;
                let seed = crate::particles::seed_of(p);
                let flags = age << 2 | (seed & 0x1_FFFF) << 13 | 1 << 30 | u32::from(kind) << 31;
                (g.word(alpha), flags)
            });
            out.push((
                at.distance(eye),
                PreviewDraw {
                    mesh: sprite.mesh,
                    transform: turned(t, sprite.mode, eye),
                    alpha,
                    glow,
                },
            ));
        }
    }
    if lamp.2 > 0.0 {
        let source = crate::glow::carried_source(lamp.1, false);
        let at = Affine3A::from_translation(lamp.0 / lamp.2);
        made.lamps
            .push(crate::lamps::place(&source, &at, 0, u32::MAX));
    }
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    made.draws = out.into_iter().map(|(_, d)| d).collect();
    made
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

/// The camera and the lights.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewView {
    pub view_proj: Mat4,
    /// Toward the light.
    pub light_dir: Vec3,
    /// The light's colour times its intensity, and the ambient.
    pub light: Vec3,
    pub ambient: Vec3,
    /// Seconds on the space's clock, which its fire and lamps flicker by.
    pub time: f32,
    /// The lamps its bodies carry ([`PreviewParticles::lamps`]), the first [`MAX_LAMPS`].
    pub lamps: Vec<crate::lamps::Lamp>,
}

const RECORD_SIZE: u64 = 64;
const FRAME_SIZE: u64 = 144;
const FIRE_FRAME_SIZE: u64 = 176;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The pass: its pipelines and buffers.
#[derive(Debug)]
pub struct PreviewPass {
    layout: wgpu::BindGroupLayout,
    opaque: wgpu::RenderPipeline,
    blended: wgpu::RenderPipeline,
    frame: wgpu::Buffer,
    records: Option<wgpu::Buffer>,
    lamps: wgpu::Buffer,
    empty: wgpu::BindGroup,
    fire_layout: wgpu::BindGroupLayout,
    fire: wgpu::RenderPipeline,
    fire_frame: wgpu::Buffer,
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

/// `src` from where `start` begins to where `end` begins after it (to its end when `end` is
/// empty: past the first line that closes a function).
fn cut<'a>(src: &'a str, start: &str, end: &str) -> &'a str {
    let a = src.find(start).unwrap_or(0);
    let rest = &src[a..];
    let b = if end.is_empty() {
        rest.find("\n}\n").map_or(rest.len(), |e| e + 3)
    } else {
        rest.find(end).unwrap_or(rest.len())
    };
    &rest[..b]
}

/// The world's flame flicker (`lamp_light.wgsl`), on the preview's clock.
fn flicker_defs() -> String {
    cut(
        include_str!("shaders/lamp_light.wgsl"),
        "// Gradient noise at",
        "// The cluster holding",
    )
    .replace("frame.sun_light.w", "pf.time.x")
}

/// The pass's shader, with the world's creatures' light (`particles.wgsl`'s `glow_seen`) and its
/// lamps' records and outdoor light (`lamp_types.wgsl`).
#[must_use]
pub fn source() -> String {
    let particles = include_str!("shaders/particles.wgsl");
    let lamps = include_str!("shaders/lamp_types.wgsl");
    format!(
        "enable wgpu_binding_array;\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        material_defs(),
        include_str!("shaders/materials.wgsl"),
        include_str!("shaders/face.wgsl"),
        include_str!("shaders/preview.wgsl"),
        flicker_defs(),
        cut(lamps, "struct Lamp {", "// The most lamps"),
        cut(lamps, "const LAMPLIGHT", "// The outdoor lamps' gain"),
        cut(particles, "const GLOW_BALL", "\n\n"),
        cut(particles, "fn glow_seen(", "")
    )
}

/// The fire pass's shader: the world's bodies of fire (`body_fire.wgsl`) over the preview's
/// picture, from the world's flame records and flicker.
#[must_use]
pub fn fire_source() -> String {
    let flames = include_str!("shaders/flames.wgsl");
    format!(
        "{}\n{}\n{}\n{}\n{}",
        include_str!("shaders/preview_fire.wgsl"),
        cut(flames, "struct Flame {", "// A flame's half width")
            .replace("@group(3) @binding(12)", "@group(0) @binding(2)"),
        cut(flames, "fn fire_colour(", ""),
        flicker_defs().replace("pf.time.x", "frame.sun_light.w"),
        include_str!("shaders/body_fire.wgsl")
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
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
        let lamps = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("preview lamps"),
            size: 64 * MAX_LAMPS as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (fire_layout, fire) = fire_pipeline(gpu, premultiplied);
        let fire_frame = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("preview fire frame"),
            size: FIRE_FRAME_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            layout,
            opaque,
            blended,
            frame,
            records: None,
            lamps,
            empty,
            fire_layout,
            fire,
            fire_frame,
        }
    }

    /// Draws `draws` seen as `view` into `target` (of [`crate::overlay::FORMAT`]), cleared to
    /// nothing first, then the bodies of fire in `fire` ([`PreviewParticles::fire`]) over them. The
    /// pools and the material table must be uploaded.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        scene: &Scene,
        materials: &MaterialTable,
        view: &PreviewView,
        draws: &[PreviewDraw],
        fire: &[crate::flames::Flame],
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
                let (word, flags) = d.glow.unwrap_or((0, 0));
                for v in [sub.material & 0x00FF_FFFF, d.alpha.to_bits(), word, flags] {
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
        // The eye: where the view's projection takes nothing to a finite place.
        let e = view.view_proj.inverse() * glam::Vec4::new(0.0, 0.0, 1.0, 0.0);
        let eye = if e.w.abs() > 1e-9 {
            e.truncate() / e.w
        } else {
            Vec3::ZERO
        };
        let lamps = &view.lamps[..view.lamps.len().min(MAX_LAMPS)];
        let mut frame = Vec::with_capacity(usize::try_from(FRAME_SIZE).unwrap_or(0));
        let mut put = |v: &[f32]| {
            for x in v {
                frame.extend_from_slice(&x.to_le_bytes());
            }
        };
        put(&view.view_proj.to_cols_array());
        for v in [view.light_dir, view.light, view.ambient, eye] {
            put(&v.extend(0.0).to_array());
        }
        #[allow(clippy::cast_precision_loss)] // at most MAX_LAMPS
        put(&[view.time, lamps.len() as f32, 0.0, 0.0]);
        let lamp_bytes: Vec<u8> = lamps
            .iter()
            .flat_map(|l| crate::lamps::lamp_bytes(l, 0xFF, l.seed))
            .collect();
        gpu.queue.write_buffer(&self.lamps, 0, &lamp_bytes);
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
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
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.lamps.as_entire_binding(),
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
                        store: wgpu::StoreOp::Store,
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
        if !fire.is_empty() {
            self.draw_fire(
                gpu,
                &mut enc,
                (view, eye),
                fire,
                &depth_view,
                (&target_view, target.size()),
            );
        }
        gpu.queue.submit([enc.finish()]);
    }

    /// The bodies of fire in `fire` added over `target` (of `size`), seen as `view` from `eye`, cut
    /// by the depth its bodies left.
    fn draw_fire(
        &self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        (view, eye): (&PreviewView, Vec3),
        fire: &[crate::flames::Flame],
        depth: &wgpu::TextureView,
        (target, size): (&wgpu::TextureView, wgpu::Extent3d),
    ) {
        let flames: Vec<u8> = fire
            .iter()
            .flat_map(|f| crate::flames::flame_bytes(f, true))
            .collect();
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("preview flames"),
            size: flames.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue.write_buffer(&buffer, 0, &flames);
        let mut frame = Vec::with_capacity(usize::try_from(FIRE_FRAME_SIZE).unwrap_or(0));
        let mut put = |v: &[f32]| {
            for x in v {
                frame.extend_from_slice(&x.to_le_bytes());
            }
        };
        put(&view.view_proj.to_cols_array());
        put(&view.view_proj.inverse().to_cols_array());
        put(&eye.extend(0.0).to_array());
        put(&[0.0, 0.0, 0.0, view.time]);
        #[allow(clippy::cast_precision_loss)] // a picture's size
        put(&[0.0, 0.0, size.width as f32, size.height as f32]);
        gpu.queue.write_buffer(&self.fire_frame, 0, &frame);
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("preview fire"),
            layout: &self.fire_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.fire_frame.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buffer.as_entire_binding(),
                },
            ],
        });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("preview fire"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.fire);
        pass.set_bind_group(0, &group, &[]);
        // Each body a card over its ball; its points draw nothing of their own.
        for (k, f) in fire.iter().enumerate() {
            if f.kind == crate::flames::FlameKind::Body {
                let k = u32::try_from(k).unwrap_or(0);
                pass.draw(0..6, k..k + 1);
            }
        }
    }
}

/// The fire pass's layout and pipeline: a card over each body's ball, its light added.
fn fire_pipeline(
    gpu: &Gpu,
    premultiplied: wgpu::BlendComponent,
) -> (wgpu::BindGroupLayout, wgpu::RenderPipeline) {
    let device = &gpu.device;
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("preview fire"),
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
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let module = gpu.shader("preview fire", &fire_source());
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("preview fire"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
        label: Some("preview fire"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_fire"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_fire"),
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
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    (layout, pipeline)
}
