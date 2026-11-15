//! Snow underfoot: the prints the bodies walking near the eye leave in snowy ground.
//!
//! Each body is followed from frame to frame by where its feet stand ([`Trails::step`]): one that
//! moves along the ground lays a boot print every stride, left and right in turn, a stride as long
//! as its pace asks and a running print deeper at the toe. The prints are kept in a ring
//! ([`MOST_PRINTS`]) and stamped each frame into the trail map, a square round the eye
//! ([`TRAIL_SIDE`] texels over twice [`TRAIL_REACH`]): how deep the snow is pressed (red) and how
//! much is heaped round the print's edge (green). The lighting reads it on snowy ground alone
//! (`rain.wgsl`'s `land_snowy`), bending the ground into the print, shading its floor and the sun
//! off its far wall. Fresh snow fills a print over [`FILL_SECONDS`] of full snowfall.

use std::collections::VecDeque;

use glam::{Vec2, Vec3};

use crate::gpu::Gpu;

/// The trail map's side, texels, and how far it reaches round the eye, metres.
pub const TRAIL_SIDE: u32 = 3072;
pub const TRAIL_REACH: f32 = 24.0;
/// The trail map's format: the dent, and the heap round it.
pub const TRAIL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;
/// The most prints kept.
pub const MOST_PRINTS: usize = 4096;
/// Seconds of full snowfall that fill a print.
pub const FILL_SECONDS: f64 = 240.0;
/// How far a body's feet may stand off the ground and still tread it, metres.
const ON_GROUND: f32 = 0.35;
/// A body farther than this from where it stood last frame has jumped, metres.
const JUMPED: f32 = 3.0;
/// How far a foot falls beside the body's path, metres.
const STANCE: f32 = 0.11;
/// A print's record: where (x, y), its heading (x, y), its side, depth, running and fill.
const PRINT_BYTES: u64 = 32;

/// One print in the snow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Print {
    /// Where its middle lies, render space.
    pub at: Vec2,
    /// The way its toe points, unit.
    pub heading: Vec2,
    /// -1 a left foot, 1 a right.
    pub side: f32,
    /// How much it ran, 0 to 1: the toe pressed deeper, the heel lighter.
    pub run: f32,
    /// The snowfall's count ([`crate::weather::Weather::snowfall`]) when it was laid.
    pub snowfall: f64,
}

/// A body followed from frame to frame.
#[derive(Debug, Clone, Copy)]
struct Walker {
    at: Vec3,
    heading: Vec2,
    /// How far it has walked since its last print, metres.
    since: f32,
    /// The next print's side.
    side: f32,
    speed: f32,
}

/// The prints and the trail map.
#[derive(Debug)]
pub struct Trails {
    /// Drawn at all (Weather Effects).
    pub on: bool,
    walkers: Vec<Walker>,
    pub prints: VecDeque<Print>,
    last_clock: Option<f64>,
    /// Where the map stands this frame, when it is drawn.
    pub centre: Option<Vec2>,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    buffer: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    params: wgpu::Buffer,
    drawn: u32,
}

impl Trails {
    #[must_use]
    pub fn new(gpu: &Gpu) -> Self {
        let device = &gpu.device;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("trail map"),
            size: wgpu::Extent3d {
                width: TRAIL_SIDE,
                height: TRAIL_SIDE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TRAIL_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("prints"),
            size: PRINT_BYTES * MOST_PRINTS as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("trail map"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("trail map"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
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
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("trail map"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffer.as_entire_binding(),
                },
            ],
        });
        let module = gpu.shader("trail map", include_str!("shaders/trail.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("trail map"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let most = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Max,
        };
        let pipeline = gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
            label: Some("trail map"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_print"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_print"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: TRAIL_FORMAT,
                    blend: Some(wgpu::BlendState {
                        color: most,
                        alpha: most,
                    }),
                    write_mask: wgpu::ColorWrites::RED | wgpu::ColorWrites::GREEN,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            on: true,
            walkers: Vec::new(),
            prints: VecDeque::new(),
            last_clock: None,
            centre: None,
            texture,
            view,
            buffer,
            pipeline,
            group,
            params,
            drawn: 0,
        }
    }

    /// The trail map's view, for the lighting.
    #[must_use]
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// The trail map, for tests.
    #[must_use]
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// Forgets every body and print: a world loaded anew.
    pub fn clear(&mut self) {
        self.walkers.clear();
        self.prints.clear();
        self.last_clock = None;
    }

    /// Moves the prints and bodies by `delta` (render space's origin moved by `-delta`).
    pub fn translate(&mut self, delta: Vec3) {
        for p in &mut self.prints {
            p.at += delta.truncate();
        }
        for w in &mut self.walkers {
            w.at += delta;
        }
    }

    /// Follows `bodies` (their feet) a frame on at `clock` seconds, laying their prints on the
    /// drawn ground of `land` at the snowfall's count `snowfall`.
    pub fn step(
        &mut self,
        clock: f64,
        bodies: &[Vec3],
        land: Option<&crate::land::LandField>,
        snowfall: f64,
    ) {
        #[allow(clippy::cast_possible_truncation)]
        let dt = self.last_clock.map_or(0.0, |c| (clock - c).clamp(0.0, 0.5)) as f32;
        self.last_clock = Some(clock);
        let mut was = std::mem::take(&mut self.walkers);
        for &b in bodies {
            // The body it was: the nearest last frame, close enough.
            let found = was
                .iter()
                .enumerate()
                .map(|(k, w)| (k, w.at.distance(b)))
                .filter(|(_, d)| *d < JUMPED)
                .min_by(|a, c| a.1.total_cmp(&c.1))
                .map(|(k, _)| k);
            let Some(k) = found else {
                self.walkers.push(Walker {
                    at: b,
                    heading: Vec2::Y,
                    since: 0.0,
                    side: 1.0,
                    speed: 0.0,
                });
                continue;
            };
            let mut w = was.swap_remove(k);
            let step = (b - w.at).truncate();
            let moved = step.length();
            if dt > 0.0 {
                let speed = moved / dt;
                w.speed += (speed - w.speed) * (dt * 6.0).min(1.0);
            }
            let ground = land.and_then(|l| l.drawn_height(b.truncate()));
            let treads = ground.is_some_and(|g| (b.z - g).abs() < ON_GROUND);
            if moved > 1e-4 {
                let dir = step / moved;
                w.heading = (w.heading + (dir - w.heading) * 0.5).normalize_or(dir);
                if treads {
                    let stride = (0.55 + 0.12 * w.speed).clamp(0.65, 1.7);
                    w.since += moved;
                    while w.since >= stride {
                        w.since -= stride;
                        let foot = b.truncate() - dir * w.since;
                        let across = Vec2::new(-dir.y, dir.x);
                        self.prints.push_back(Print {
                            at: foot - across * w.side * STANCE,
                            heading: dir,
                            side: w.side,
                            run: ((w.speed - 2.5) / 2.5).clamp(0.0, 1.0),
                            snowfall,
                        });
                        w.side = -w.side;
                    }
                } else {
                    w.since = 0.0;
                }
            }
            w.at = b;
            self.walkers.push(w);
        }
        while self.prints.len() > MOST_PRINTS {
            self.prints.pop_front();
        }
    }

    /// The frame's words for the map: its centre, reach, and the soft snow's terrain types
    /// `soft` (u32 bits; none when it is not drawn).
    #[must_use]
    pub fn frame_words(&self, soft: u32) -> [f32; 4] {
        match (self.centre, self.drawn > 0) {
            (Some(c), true) => [c.x, c.y, TRAIL_REACH, f32::from_bits(soft)],
            _ => [0.0; 4],
        }
    }

    /// Chooses this frame's map round `eye` and uploads the prints in it, filled as far as
    /// `snowfall` has come since each was laid: none when `snowy` is false (no snowy land near).
    pub fn prepare(&mut self, gpu: &Gpu, eye: Vec3, snowfall: f64, snowy: bool) {
        self.drawn = 0;
        self.centre = None;
        if !self.on || !snowy || self.prints.is_empty() {
            return;
        }
        #[allow(clippy::cast_precision_loss)]
        let texel = 2.0 * TRAIL_REACH / TRAIL_SIDE as f32;
        let snap = |v: f32| (v / texel).round() * texel;
        let centre = Vec2::new(snap(eye.x), snap(eye.y));
        let mut bytes =
            Vec::with_capacity(self.prints.len() * usize::try_from(PRINT_BYTES).unwrap_or(32));
        let reach = TRAIL_REACH + 0.5;
        for p in &self.prints {
            let off = p.at - centre;
            if off.x.abs() > reach || off.y.abs() > reach {
                continue;
            }
            #[allow(clippy::cast_possible_truncation)]
            let left = 1.0 - ((snowfall - p.snowfall) / FILL_SECONDS).clamp(0.0, 1.0) as f32;
            if left <= 0.0 {
                continue;
            }
            for v in [
                off.x,
                off.y,
                p.heading.x,
                p.heading.y,
                p.side,
                p.run,
                left,
                0.0,
            ] {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            self.drawn += 1;
        }
        if self.drawn == 0 {
            return;
        }
        gpu.queue.write_buffer(&self.buffer, 0, &bytes);
        gpu.queue.write_buffer(
            &self.params,
            0,
            &[1.0 / TRAIL_REACH, 0.0, 0.0, 0.0]
                .iter()
                .flat_map(|v: &f32| v.to_le_bytes())
                .collect::<Vec<u8>>(),
        );
        self.centre = Some(centre);
    }

    /// How many prints the map holds this frame.
    #[must_use]
    pub fn drawn_count(&self) -> u32 {
        self.drawn
    }

    /// Whether the map holds prints this frame.
    #[must_use]
    pub fn drawn(&self) -> bool {
        self.drawn > 0
    }

    /// Stamps the prints into the map, its pass cleared first.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.drawn == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..6, 0..self.drawn);
    }
}
