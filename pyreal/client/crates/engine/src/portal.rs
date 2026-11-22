//! Portal space, enhanced: a bending tube flown through in HDR, with its own bloom and tone map,
//! the world's last picture warped into it on the way in, and on the way out its view collapsed as
//! the original swirl's is. It is drawn no taller than 1080 lines and scaled up sharply under the
//! interface.
//! The teleport's own timing (its legs and their lengths) is the runtime's; this draws a moment
//! of it.

use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use glam::{Vec2, Vec3};
use pyreal_primitives::num::math::{cosf, expf, sinf, tanf};

use crate::gpu::Gpu;

/// The shaders: the tunnel, then its bloom and last step.
#[must_use]
pub fn sources() -> [&'static str; 2] {
    [
        include_str!("shaders/portal.wgsl"),
        include_str!("shaders/portal_post.wgsl"),
    ]
}

/// The teleport's legs inside portal space, as the runtime names them. The world's own fades
/// before and after are the original's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leg {
    /// The tunnel opens after the world has folded away.
    TunnelIn,
    /// Waiting on the server.
    Tunnel,
    /// The destination is known and loaded; the tunnel runs out its time.
    Continue,
    /// The tunnel's last second: its view collapses.
    TunnelOut,
}

/// The tunnel's vertical field of view, degrees.
pub const FOV_Y: f32 = 70.0;
/// The tallest the tunnel is drawn, pixels; a taller output scales it up.
pub const MAX_LINES: u32 = 1080;

/// The tunnel's colours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub deep: Vec3,
    pub streak: Vec3,
    pub spark: Vec3,
    pub glow: Vec3,
    /// Which way the swirl turns, 1 or -1.
    pub spin: f32,
}

impl Palette {
    /// A portal's own, after the original swirl: slate blue walls, pale steel wisps, violet
    /// lights.
    pub const PORTAL: Self = Self {
        deep: Vec3::new(0.006, 0.019, 0.032),
        streak: Vec3::new(0.14, 0.27, 0.42),
        spark: Vec3::new(0.72, 0.16, 1.25),
        glow: Vec3::new(0.65, 0.88, 1.15),
        spin: 1.0,
    };
    /// Entering the world: the portal's colours, turning the other way.
    pub const LOGIN: Self = Self {
        spin: -1.0,
        ..Self::PORTAL
    };
    /// The lifestone's: sea green and pale aqua.
    pub const LIFESTONE: Self = Self {
        deep: Vec3::new(0.003, 0.025, 0.035),
        streak: Vec3::new(0.5, 1.05, 1.05),
        spark: Vec3::new(0.55, 1.4, 0.95),
        glow: Vec3::new(0.85, 1.15, 1.1),
        spin: 1.0,
    };
    /// Home: embers.
    pub const HOUSE: Self = Self {
        deep: Vec3::new(0.045, 0.012, 0.004),
        streak: Vec3::new(1.3, 0.7, 0.3),
        spark: Vec3::new(1.6, 0.45, 0.12),
        glow: Vec3::new(1.2, 0.95, 0.7),
        spin: -1.0,
    };
    /// A tied or summoned portal's: deep magenta.
    pub const RECALL: Self = Self {
        deep: Vec3::new(0.028, 0.005, 0.045),
        streak: Vec3::new(1.0, 0.55, 1.25),
        spark: Vec3::new(1.5, 0.3, 1.0),
        glow: Vec3::new(1.1, 0.8, 1.25),
        spin: 1.0,
    };
    /// Leaving the world: ash and pale steel.
    pub const LOGOFF: Self = Self {
        deep: Vec3::new(0.01, 0.01, 0.014),
        streak: Vec3::new(0.55, 0.6, 0.72),
        spark: Vec3::new(0.65, 0.65, 0.9),
        glow: Vec3::new(0.8, 0.8, 0.9),
        spin: -1.0,
    };

    /// These colours leant toward a portal's `colour` (linear), each keeping its brightest
    /// channel: the walls and wisps most, the lights less.
    #[must_use]
    pub fn toward(self, colour: Vec3) -> Self {
        let hue = colour / colour.max_element().max(1e-4);
        let lean = |c: Vec3, by: f32| c.lerp(hue * c.max_element(), by);
        Self {
            deep: lean(self.deep, LEAN[0]),
            streak: lean(self.streak, LEAN[0]),
            spark: lean(self.spark, LEAN[1]),
            glow: lean(self.glow, LEAN[1]),
            spin: self.spin,
        }
    }
}

/// How far a portal's colour takes the tunnel's walls and wisps, and its lights.
const LEAN: [f32; 2] = [0.6, 0.4];

/// One moment of the teleport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortalFrame {
    pub leg: Leg,
    /// Seconds into the leg.
    pub phase: f32,
    /// Seconds since the effect began.
    pub travelled: f32,
    /// The tunnel's shape.
    pub seed: u32,
    /// Which trip this is: only a new one starts the flight afresh.
    pub trip: u32,
    pub palette: Palette,
    /// The world was on screen as the tunnel opened, so its last picture is warped in.
    pub from_world: bool,
}

/// How long the world takes to be swallowed on the way in, seconds.
pub const SWALLOW: f32 = 0.9;
/// The tunnel's speed while cruising, meters a second.
const CRUISE: f32 = 55.0;
/// How far the light at the end stands at first, and once the destination is loaded.
const LIGHT_FAR: f32 = 560.0;
const LIGHT_LOADED: f32 = 260.0;

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much of the tunnel shows `phase` seconds into its last second.
fn fade_out(phase: f32) -> f32 {
    1.0 - ease((phase - 0.35) / 0.65)
}

/// The spine's shape for `seed`: amplitudes, frequencies and phases of its two waves across and
/// two up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spine {
    pub a: [f32; 4],
    pub f: [f32; 4],
    pub p: [f32; 4],
}

impl Spine {
    #[must_use]
    pub fn new(seed: u32) -> Self {
        let mut s = seed.wrapping_mul(0x9E37_79B9) ^ 0x5bd1_e995;
        let mut roll = |lo: f32, hi: f32| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            #[allow(clippy::cast_precision_loss)]
            let u = (s >> 8) as f32 / 16_777_216.0;
            lo + (hi - lo) * u
        };
        let a = [
            roll(10.0, 18.0),
            roll(3.0, 7.0),
            roll(10.0, 18.0),
            roll(3.0, 7.0),
        ];
        let f = [
            roll(0.01, 0.018),
            roll(0.022, 0.035),
            roll(0.01, 0.018),
            roll(0.022, 0.035),
        ];
        let tau = std::f32::consts::TAU;
        let p = [
            roll(0.0, tau),
            roll(0.0, tau),
            roll(0.0, tau),
            roll(0.0, tau),
        ];
        Self { a, f, p }
    }

    /// Where the spine stands across the tube at `z`.
    #[must_use]
    pub fn at(&self, z: f32) -> Vec2 {
        Vec2::new(
            self.a[0] * sinf(z * self.f[0] + self.p[0])
                + self.a[1] * sinf(z * self.f[1] + self.p[1]),
            self.a[2] * sinf(z * self.f[2] + self.p[2])
                + self.a[3] * sinf(z * self.f[3] + self.p[3]),
        )
    }

    /// The point on the spine at `z`.
    #[must_use]
    pub fn point(&self, z: f32) -> Vec3 {
        self.at(z).extend(z)
    }
}

/// The lights that fly through the tunnel with the traveller.
pub const MOTES: usize = 12;
/// How far ahead of the eye the lights fly, metres.
const MOTE_REACH: f32 = 80.0;

/// One light's own course.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Mote {
    /// Metres ahead of the eye at the start, and how fast it falls back (negative: pulls ahead).
    ahead: f32,
    drift: f32,
    /// Its angle round the tube, how fast it turns, and how far out it flies (share of the radius).
    angle: f32,
    turn: f32,
    out: f32,
    size: f32,
    /// Its pulse's rate and phase.
    pulse: f32,
    phase: f32,
}

fn motes(seed: u32) -> [Mote; MOTES] {
    let mut s = seed.wrapping_mul(0x85EB_CA6B) ^ 0xC2B2_AE35;
    let mut roll = |lo: f32, hi: f32| {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        #[allow(clippy::cast_precision_loss)]
        let u = (s >> 8) as f32 / 16_777_216.0;
        lo + (hi - lo) * u
    };
    std::array::from_fn(|_| Mote {
        ahead: roll(0.0, MOTE_REACH),
        drift: roll(-3.0, 7.0),
        angle: roll(0.0, std::f32::consts::TAU),
        turn: roll(0.3, 1.1) * if roll(0.0, 1.0) < 0.5 { -1.0 } else { 1.0 },
        out: roll(0.15, 0.7),
        size: roll(0.1, 0.28),
        pulse: roll(1.5, 4.0),
        phase: roll(0.0, std::f32::consts::TAU),
    })
}

/// What the tunnel's shader is given, worked out from a moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shot {
    pub eye: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub fwd: Vec3,
    pub visible: f32,
    pub light: Vec3,
    pub reach: f32,
    pub glow: f32,
    /// The hole's centre (NDC), radius and side (+1 the world outside, -1 inside, 0 none).
    pub hole: [f32; 4],
    pub twist: f32,
    pub flash: f32,
    pub speed: f32,
    pub streaks: f32,
    pub rim: f32,
    /// Each light: where it is (xyz) and its size (w), then where it was a moment ago (xyz) and
    /// its brightness (w).
    pub motes: [[f32; 4]; MOTES],
    pub trails: [[f32; 4]; MOTES],
}

/// The flight: where the camera has got to, kept from frame to frame.
#[derive(Debug, Clone, Default)]
pub struct Flight {
    z: f32,
    last: Option<f32>,
    trip: Option<u32>,
    light: f32,
    roll: f32,
}

impl Flight {
    /// Whether the world's last picture is still being swallowed.
    #[must_use]
    pub fn swallowing(f: &PortalFrame) -> bool {
        f.from_world && f.travelled < SWALLOW + 0.3
    }

    /// The shot at `f`, the picture's aspect `aspect` and its vertical half-view tangent `tan`.
    pub fn shot(&mut self, f: &PortalFrame, aspect: f32, tan: f32) -> Shot {
        let spine = Spine::new(f.seed);
        // Only a new trip starts afresh; a moment handed over late is two clocks disagreeing.
        if self.trip != Some(f.trip) {
            *self = Self {
                z: 0.0,
                last: Some(f.travelled),
                trip: Some(f.trip),
                light: LIGHT_FAR,
                roll: 0.0,
            };
        }
        let last = self.last.unwrap_or(f.travelled);
        let dt = (f.travelled - last).clamp(0.0, 0.25);
        self.last = Some(f.travelled.max(last));
        let out = if f.leg == Leg::TunnelOut {
            ease(f.phase)
        } else {
            0.0
        };
        let start = ease(f.travelled / 1.5);
        let speed = CRUISE * (0.35 + 0.65 * start) + 260.0 * out * out;
        self.z += speed * dt;
        // The light comes nearer as the trip goes on, and nearer still once the destination is
        // loaded.
        let target = match f.leg {
            Leg::Tunnel | Leg::TunnelIn => {
                LIGHT_FAR - (LIGHT_FAR - 300.0) * (1.0 - expf(-f.travelled / 5.0))
            }
            Leg::Continue | Leg::TunnelOut => LIGHT_LOADED,
        };
        let k = 1.2;
        self.light += (target - self.light) * (1.0 - expf(-k * dt));
        let z = self.z;
        let eye = spine.point(z);
        let ahead = spine.point(z + 12.0);
        let light_z = z + self.light.max(2.0);
        let light = spine.point(light_z);
        let fwd = (ahead - eye).normalize();
        self.roll += dt * f.palette.spin * (0.25 + 0.6 * out);
        let up0 = Vec3::new(sinf(self.roll), cosf(self.roll), 0.0);
        let right = fwd.cross(up0).normalize();
        let up = right.cross(fwd).normalize();
        // Where the light stands on the screen.
        let rel = light - eye;
        let depth = rel.dot(fwd).max(0.5);
        let vp = Vec2::new(
            rel.dot(right) / (depth * tan * aspect),
            rel.dot(up) / (depth * tan),
        )
        .clamp(Vec2::splat(-0.9), Vec2::splat(0.9));
        let loaded = matches!(f.leg, Leg::Continue | Leg::TunnelOut);
        let (lights, trails) = self.lights(f, &spine, speed, out);
        let mut shot = Shot {
            eye,
            right,
            up,
            fwd,
            visible: 1.0,
            light,
            reach: if loaded { 3.0 } else { 1.6 },
            glow: if loaded { 1.6 } else { 0.9 },
            hole: [vp.x, vp.y, 0.0, 0.0],
            twist: 0.0,
            flash: 0.0,
            speed,
            streaks: 0.6 * start + 2.5 * out,
            rim: 0.0,
            motes: lights,
            trails,
        };
        if f.leg == Leg::TunnelOut {
            // The view collapses (the caller widens it, as the swirl's is) and the tunnel goes
            // dark before the world opens out.
            shot.visible = fade_out(f.phase);
        } else if f.from_world && f.travelled < SWALLOW + 0.2 {
            // On the way in, the world's last picture is swallowed.
            let t = (f.travelled / SWALLOW).clamp(0.0, 1.0);
            shot.hole = [0.0, 0.0, t * t * 1.2, 1.0];
            shot.twist = 5.0 * t;
            shot.rim = 1.0 - ease((f.travelled - SWALLOW) / 0.2);
        } else if !f.from_world {
            shot.visible = ease(f.travelled / 0.8);
            shot.flash = 0.6 * expf(-f.travelled * 4.0);
        }
        shot
    }

    /// The lights flying with the eye: each drifts slowly along the tube and round it, and in the
    /// last second they rush past.
    fn lights(
        &self,
        f: &PortalFrame,
        spine: &Spine,
        speed: f32,
        out: f32,
    ) -> ([[f32; 4]; MOTES], [[f32; 4]; MOTES]) {
        /// The trail's length, seconds.
        const BLUR: f32 = 0.05;
        let t = f.travelled;
        let shown = match f.leg {
            Leg::TunnelOut => fade_out(f.phase),
            _ if f.from_world => ease((t - 0.3) / 0.8),
            _ => ease(t / 0.8),
        };
        let rush = 120.0 * out * out;
        let eye_was = self.z - speed * BLUR;
        let mut now = [[0.0; 4]; MOTES];
        let mut was = [[0.0; 4]; MOTES];
        for (i, m) in motes(f.seed).iter().enumerate() {
            let at = |time: f32, eye_z: f32, extra: f32| {
                let ahead = (m.ahead - m.drift * time - extra).rem_euclid(MOTE_REACH) - 8.0;
                let a = m.angle + m.turn * time * f.palette.spin;
                let z = eye_z + ahead;
                let r = 10.0 * m.out * (1.0 + 0.15 * sinf(time * 0.7 + m.phase));
                (
                    (spine.at(z) + Vec2::new(cosf(a), sinf(a)) * r).extend(z),
                    ahead,
                )
            };
            let (p, ahead) = at(t, self.z, rush * f.phase.min(1.0));
            let (q, _) = at(
                t - BLUR,
                eye_was,
                (rush * (f.phase - BLUR).max(0.0)).min(rush),
            );
            // From the haze far ahead they fade in; wrapping round they are behind the eye.
            let fade = 1.0 - ease((ahead - (MOTE_REACH - 28.0)) / 20.0);
            let glow = (0.7 + 0.3 * sinf(t * m.pulse + m.phase)) * shown * fade;
            // Where it was, seen from where the eye is now.
            let q = spine.point(self.z) + (q - spine.point(eye_was));
            now[i] = [p.x, p.y, p.z, m.size];
            was[i] = [q.x, q.y, q.z, glow];
        }
        (now, was)
    }
}

const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const LEVELS: usize = 5;

/// The size the tunnel is drawn at for an output of `size`: no taller than `lines`.
fn inner_size(size: (u32, u32), lines: u32) -> (u32, u32) {
    if size.1 <= lines {
        return size;
    }
    let w = u64::from(size.0) * u64::from(lines) / u64::from(size.1);
    (u32::try_from(w).unwrap_or(size.0).max(1), lines)
}
/// How many floats the tunnel's uniform holds.
const UNIFORM_FLOATS: usize = (16 + 2 * MOTES) * 4;

struct Targets {
    size: (u32, u32),
    hdr: wgpu::Texture,
    levels: Vec<wgpu::Texture>,
    down: Vec<wgpu::BindGroup>,
    up: Vec<wgpu::BindGroup>,
}

/// The enhanced portal space's pipelines and pictures.
pub struct PortalSpace {
    tunnel: wgpu::RenderPipeline,
    tunnel_layout: wgpu::BindGroupLayout,
    down_first: wgpu::RenderPipeline,
    down: wgpu::RenderPipeline,
    up: wgpu::RenderPipeline,
    sample_layout: wgpu::BindGroupLayout,
    finish: Option<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
    finish_layout: wgpu::BindGroupLayout,
    finish_module: wgpu::ShaderModule,
    uniform: wgpu::Buffer,
    finish_uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    black: wgpu::TextureView,
    clear_ui: wgpu::TextureView,
    targets: Option<Targets>,
    pub flight: Flight,
    /// The vertical field of view the tunnel is seen with, degrees.
    pub fov_y: f32,
    /// The tallest the tunnel is drawn, pixels.
    pub lines: u32,
}

impl std::fmt::Debug for PortalSpace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PortalSpace").finish_non_exhaustive()
    }
}

fn texture_entry(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
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

fn pipeline(
    gpu: &Gpu,
    label: &str,
    module: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
    entry: &str,
    format: wgpu::TextureFormat,
    add: bool,
) -> wgpu::RenderPipeline {
    let pl = gpu
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &[Some(layout)],
            immediate_size: 0,
        });
    let blend = add.then_some(wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent::REPLACE,
    });
    gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_full"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn solid(gpu: &Gpu, label: &str, rgba: [u8; 4]) -> wgpu::TextureView {
    let t = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        t.as_image_copy(),
        &rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    t.create_view(&Default::default())
}

fn bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_ne_bytes()).collect()
}

impl PortalSpace {
    #[must_use]
    pub fn new(gpu: &Gpu) -> Self {
        let [tunnel_src, post_src] = sources();
        let tunnel_module = gpu.shader("portal space", tunnel_src);
        let post_module = gpu.shader("portal space post", post_src);
        let device = &gpu.device;
        let tunnel_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("portal space"),
            entries: &[uniform_entry(0), texture_entry(1, true), sampler_entry(2)],
        });
        let sample_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("portal bloom"),
            entries: &[texture_entry(0, true), sampler_entry(1)],
        });
        let finish_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("portal finish"),
            entries: &[
                uniform_entry(0),
                texture_entry(1, true),
                texture_entry(2, true),
                texture_entry(3, false),
                sampler_entry(4),
            ],
        });
        let tunnel = pipeline(
            gpu,
            "portal tunnel",
            &tunnel_module,
            &tunnel_layout,
            "fs_portal",
            HDR,
            false,
        );
        let down_first = pipeline(
            gpu,
            "portal bloom down",
            &post_module,
            &sample_layout,
            "fs_down_first",
            HDR,
            false,
        );
        let down = pipeline(
            gpu,
            "portal bloom down",
            &post_module,
            &sample_layout,
            "fs_down",
            HDR,
            false,
        );
        let up = pipeline(
            gpu,
            "portal bloom up",
            &post_module,
            &sample_layout,
            "fs_up",
            HDR,
            true,
        );
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("portal space"),
            size: (UNIFORM_FLOATS * 4) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let finish_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("portal finish"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("portal space"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            tunnel,
            tunnel_layout,
            down_first,
            down,
            up,
            sample_layout,
            finish: None,
            finish_layout,
            finish_module: post_module,
            uniform,
            finish_uniform,
            sampler,
            black: solid(gpu, "portal no world", [0, 0, 0, 255]),
            clear_ui: solid(gpu, "portal no interface", [0, 0, 0, 0]),
            targets: None,
            flight: Flight::default(),
            fov_y: FOV_Y,
            lines: MAX_LINES,
        }
    }

    fn targets(&mut self, gpu: &Gpu, size: (u32, u32)) {
        let size = inner_size(size, self.lines);
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let make = |label: &str, w: u32, h: u32| {
            gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w.max(1),
                    height: h.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HDR,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let hdr = make("portal space", size.0, size.1);
        let levels: Vec<_> = (1..=LEVELS)
            .map(|k| make("portal bloom", size.0 >> k, size.1 >> k))
            .collect();
        let group = |t: &wgpu::Texture| {
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("portal bloom"),
                layout: &self.sample_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &t.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        };
        // Down: level k is made from the one before it (the picture for the first).
        let down = std::iter::once(&hdr)
            .chain(levels.iter().take(LEVELS - 1))
            .map(group)
            .collect();
        // Up: level k gains the one after it.
        let up = levels.iter().skip(1).map(group).collect();
        self.targets = Some(Targets {
            size,
            hdr,
            levels,
            down,
            up,
        });
    }

    /// Builds the last pass for `format` ahead of the first trip, so its first frame has no wait.
    pub fn prepare(&mut self, gpu: &Gpu, format: wgpu::TextureFormat) {
        let _ = self.finish_pipeline(gpu, format);
    }

    fn finish_pipeline(&mut self, gpu: &Gpu, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        if let Some((f, p)) = self.finish.as_ref() {
            if *f == format {
                return p.clone();
            }
        }
        let p = pipeline(
            gpu,
            "portal finish",
            &self.finish_module,
            &self.finish_layout,
            "fs_finish",
            format,
            false,
        );
        self.finish = Some((format, p.clone()));
        p
    }

    /// Draws the moment `f` over the whole of `target` (`format`, `size`): the tunnel, the world
    /// picture `world` where it shows, bloom, the tone map and the interface `ui` over it.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        gpu: &Gpu,
        f: &PortalFrame,
        world: Option<&wgpu::TextureView>,
        ui: Option<&crate::temporal::Ui>,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        size: (u32, u32),
    ) {
        self.targets(gpu, size);
        #[allow(clippy::cast_precision_loss)]
        let aspect = size.0 as f32 / size.1.max(1) as f32;
        let tan = tanf(self.fov_y.to_radians() * 0.5);
        let s = self.flight.shot(f, aspect, tan);
        let spine = Spine::new(f.seed);
        let p = &f.palette;
        let head: [f32; 64] = [
            s.eye.x,
            s.eye.y,
            s.eye.z,
            f.travelled,
            s.right.x,
            s.right.y,
            s.right.z,
            tan * aspect,
            s.up.x,
            s.up.y,
            s.up.z,
            tan,
            s.fwd.x,
            s.fwd.y,
            s.fwd.z,
            s.visible,
            spine.a[0],
            spine.a[1],
            spine.a[2],
            spine.a[3],
            spine.f[0],
            spine.f[1],
            spine.f[2],
            spine.f[3],
            spine.p[0],
            spine.p[1],
            spine.p[2],
            spine.p[3],
            10.0,
            0.3,
            0.011,
            spine.p[0],
            p.deep.x,
            p.deep.y,
            p.deep.z,
            p.spin,
            p.streak.x,
            p.streak.y,
            p.streak.z,
            2.4,
            p.spark.x,
            p.spark.y,
            p.spark.z,
            0.02,
            p.glow.x,
            p.glow.y,
            p.glow.z,
            s.glow,
            s.light.x,
            s.light.y,
            s.light.z,
            s.reach,
            s.hole[0],
            s.hole[1],
            s.hole[2],
            s.hole[3],
            s.twist,
            s.flash,
            s.speed,
            s.streaks,
            aspect,
            if world.is_some() { 1.0 } else { 0.0 },
            s.rim,
            // How wide a pixel of the tunnel's picture is, at a metre.
            #[allow(clippy::cast_precision_loss)]
            {
                2.0 * tan / inner_size(size, self.lines).1 as f32
            },
        ];
        let mut u: Vec<f32> = Vec::with_capacity(UNIFORM_FLOATS);
        u.extend_from_slice(&head);
        u.extend(s.motes.iter().flatten());
        u.extend(s.trails.iter().flatten());
        gpu.queue.write_buffer(&self.uniform, 0, &bytes(&u));
        let gamma = ui.is_some_and(|u| u.gamma);
        let fin: [f32; 8] = [
            if f.leg == Leg::TunnelOut {
                0.04 * (s.streaks / 2.5).min(1.0)
            } else {
                0.0
            },
            0.35,
            0.045,
            1.0,
            if gamma { 1.0 } else { 0.0 },
            if ui.is_some() { 1.0 } else { 0.0 },
            0.0,
            0.0,
        ];
        gpu.queue
            .write_buffer(&self.finish_uniform, 0, &bytes(&fin));
        let finish = self.finish_pipeline(gpu, format);
        let Some(t) = self.targets.as_ref() else {
            return;
        };
        let device = &gpu.device;
        let tunnel_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("portal space"),
            layout: &self.tunnel_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(world.unwrap_or(&self.black)),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let hdr_view = t.hdr.create_view(&Default::default());
        let level_views: Vec<_> = t
            .levels
            .iter()
            .map(|l| l.create_view(&Default::default()))
            .collect();
        let finish_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("portal finish"),
            layout: &self.finish_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.finish_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&hdr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&level_views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(
                        ui.map_or(&self.clear_ui, |u| &u.view),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("portal space"),
        });
        let pass = |enc: &mut wgpu::CommandEncoder,
                    view: &wgpu::TextureView,
                    load: wgpu::LoadOp<wgpu::Color>,
                    pipe: &wgpu::RenderPipeline,
                    group: &wgpu::BindGroup| {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("portal space"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipe);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        };
        let clear = wgpu::LoadOp::Clear(wgpu::Color::BLACK);
        pass(&mut enc, &hdr_view, clear, &self.tunnel, &tunnel_group);
        for (k, group) in t.down.iter().enumerate() {
            let pipe = if k == 0 { &self.down_first } else { &self.down };
            pass(&mut enc, &level_views[k], clear, pipe, group);
        }
        for k in (0..LEVELS - 1).rev() {
            pass(
                &mut enc,
                &level_views[k],
                wgpu::LoadOp::Load,
                &self.up,
                &t.up[k],
            );
        }
        pass(&mut enc, target, clear, &finish, &finish_group);
        gpu.queue.submit([enc.finish()]);
    }
}

/// How many evenly spaced samples of the tunnel's last second [`Show::collapse`] holds.
pub const COLLAPSE_SAMPLES: usize = 33;

/// One moment of portal space handed to the [`PortalThread`], and what it is drawn with.
#[derive(Debug, Clone)]
pub struct Show {
    pub frame: PortalFrame,
    /// When `frame` was the moment (when the runtime read its clock, not when it was handed
    /// over): the thread carries it on from here at its own pace.
    pub at: Instant,
    /// The world's last picture, while it is swallowed.
    pub world: Option<wgpu::TextureView>,
    pub ui: Option<crate::temporal::Ui>,
    pub format: wgpu::TextureFormat,
    /// The tunnel's vertical field of view over its last second, degrees, as the teleport's own
    /// collapse has it.
    pub collapse: [f32; COLLAPSE_SAMPLES],
}

impl Show {
    /// The moment `since` seconds after the hand-over, never earlier than `drawn` on its trip.
    #[must_use]
    pub fn carried(&self, since: f32, drawn: Option<PortalFrame>) -> PortalFrame {
        let mut f = self.frame;
        f.phase += since;
        f.travelled += since;
        if let Some(d) = drawn.filter(|d| d.trip == f.trip) {
            f.travelled = f.travelled.max(d.travelled);
            if d.leg == f.leg {
                f.phase = f.phase.max(d.phase);
            }
        }
        f
    }

    /// The tunnel's field of view at moment `f`, degrees.
    #[must_use]
    pub fn fov_at(&self, f: &PortalFrame) -> f32 {
        if f.leg != Leg::TunnelOut {
            return FOV_Y;
        }
        #[allow(clippy::cast_precision_loss)]
        let x = f.phase.clamp(0.0, 1.0) * (COLLAPSE_SAMPLES - 1) as f32;
        let i = usize::try_from(pyreal_primitives::num::floor_to_i32(x))
            .unwrap_or(0)
            .min(COLLAPSE_SAMPLES - 2);
        #[allow(clippy::cast_precision_loss)]
        let t = x - i as f32;
        self.collapse[i] + (self.collapse[i + 1] - self.collapse[i]) * t
    }
}

#[derive(Debug, Default)]
struct Shared {
    show: Option<Show>,
    quit: bool,
    /// Frames the thread has put on the screen.
    shown: u64,
    /// The surface wants configuring again, which is the frame thread's to do.
    outdated: bool,
}

/// Portal space drawn and put on the screen by a thread of its own, at the display's pace, so
/// nothing the frame thread loads meanwhile holds a frame of it
/// back. The window's surface is shared under `presenting`, which whoever puts a frame on the
/// screen (or configures the surface) holds.
#[derive(Debug)]
pub struct PortalThread {
    shared: Arc<(Mutex<Shared>, Condvar)>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl PortalThread {
    /// Starts the thread, idle until [`Self::show`].
    ///
    /// # Errors
    /// The system would not start a thread.
    pub fn spawn(
        gpu: Gpu,
        surface: Arc<wgpu::Surface<'static>>,
        presenting: Arc<Mutex<()>>,
        space: Arc<Mutex<PortalSpace>>,
    ) -> std::io::Result<Self> {
        let shared: Arc<(Mutex<Shared>, Condvar)> = Arc::default();
        let theirs = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("portal space".to_owned())
            .spawn(move || run(&gpu, &surface, &presenting, &space, &theirs))?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }

    fn state(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Portal space at this moment, carried on by the thread until the next.
    pub fn show(&self, show: Show) {
        self.state().show = Some(show);
        self.shared.1.notify_all();
    }

    /// Nothing more of portal space: the thread puts no further frame on the screen once this
    /// returns and the caller holds `presenting`.
    pub fn hide(&self) {
        self.state().show = None;
    }

    /// Whether portal space is being shown.
    #[must_use]
    pub fn showing(&self) -> bool {
        self.state().show.is_some()
    }

    /// Frames put on the screen so far.
    #[must_use]
    pub fn shown(&self) -> u64 {
        self.state().shown
    }

    /// Whether the surface wants configuring again, once.
    pub fn take_outdated(&self) -> bool {
        std::mem::take(&mut self.state().outdated)
    }
}

impl Drop for PortalThread {
    fn drop(&mut self) {
        self.state().quit = true;
        self.shared.1.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The thread: waits for a moment to show, then draws and shows frames until it is hidden.
fn run(
    gpu: &Gpu,
    surface: &wgpu::Surface<'static>,
    presenting: &Mutex<()>,
    space: &Mutex<PortalSpace>,
    shared: &(Mutex<Shared>, Condvar),
) {
    let (lock, wake) = shared;
    let state = || lock.lock().unwrap_or_else(PoisonError::into_inner);
    let mut last: Option<PortalFrame> = None;
    loop {
        let show = {
            let mut s = state();
            loop {
                if s.quit {
                    return;
                }
                if let Some(show) = s.show.clone() {
                    break show;
                }
                last = None;
                s = wake.wait(s).unwrap_or_else(PoisonError::into_inner);
            }
        };
        let held = presenting.lock().unwrap_or_else(PoisonError::into_inner);
        // Hidden while the surface was busy: the frame thread has the screen again.
        if state().show.is_none() {
            continue;
        }
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                state().outdated = true;
                drop(held);
                std::thread::sleep(Duration::from_millis(4));
                continue;
            }
            _ => {
                drop(held);
                std::thread::sleep(Duration::from_millis(16));
                continue;
            }
        };
        let f = show.carried(show.at.elapsed().as_secs_f32().min(30.0), last);
        last = Some(f);
        let size = (frame.texture.width(), frame.texture.height());
        let view = frame.texture.create_view(&Default::default());
        {
            let mut space = space.lock().unwrap_or_else(PoisonError::into_inner);
            space.fov_y = show.fov_at(&f);
            space.render(
                gpu,
                &f,
                show.world.as_ref(),
                show.ui.as_ref(),
                &view,
                show.format,
                size,
            );
        }
        gpu.queue.present(frame);
        drop(held);
        state().shown += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(leg: Leg, phase: f32, travelled: f32) -> PortalFrame {
        PortalFrame {
            leg,
            phase,
            travelled,
            seed: 7,
            trip: 1,
            palette: Palette::PORTAL,
            from_world: true,
        }
    }

    #[test]
    fn a_portals_colour_leans_the_tunnel_toward_it_without_brightening_it() {
        let p = Palette::PORTAL;
        let green = p.toward(Vec3::new(0.03, 0.78, 0.02));
        assert!(green.streak.y > green.streak.z && green.streak.y > green.streak.x);
        assert!(green.deep.y > p.deep.y && green.deep.z < p.deep.z);
        // Not all the way: the walls keep some of the tunnel's own blue.
        assert!(green.streak.z > 0.05);
        for (a, b) in [
            (p.deep, green.deep),
            (p.streak, green.streak),
            (p.spark, green.spark),
        ] {
            assert!(b.max_element() <= a.max_element() + 1e-6);
        }
        assert!((green.spin - p.spin).abs() < 1e-6);
    }

    #[test]
    fn the_same_seed_makes_the_same_spine() {
        assert_eq!(Spine::new(42), Spine::new(42));
        assert_ne!(Spine::new(42), Spine::new(43));
    }

    #[test]
    fn the_hole_swallows_the_world_and_the_tunnel_goes_dark_at_the_end() {
        let mut f = Flight::default();
        let a = f.shot(&frame(Leg::Tunnel, 0.1, 0.1), 1.6, 0.7);
        let b = f.shot(&frame(Leg::Tunnel, 0.8, 0.8), 1.6, 0.7);
        assert!(a.hole[3] > 0.5 && b.hole[2] > a.hole[2]);
        let c = f.shot(&frame(Leg::Continue, 1.0, 3.0), 1.6, 0.7);
        assert!(
            c.hole[3].abs() < 0.5,
            "no window onto the destination: {c:?}"
        );
        let d = f.shot(&frame(Leg::TunnelOut, 0.2, 4.2), 1.6, 0.7);
        let e = f.shot(&frame(Leg::TunnelOut, 1.0, 5.0), 1.6, 0.7);
        assert!(d.visible > 0.9 && e.visible < 0.01, "{d:?} {e:?}");
        assert!(
            e.trails.iter().all(|t| t[3] < 0.01),
            "the lights go with it"
        );
    }

    #[test]
    fn the_thread_reads_the_collapse_through_the_last_second_only() {
        #[allow(clippy::cast_precision_loss)]
        let show = Show {
            frame: frame(Leg::TunnelOut, 0.0, 4.0),
            at: Instant::now(),
            world: None,
            ui: None,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            collapse: std::array::from_fn(|i| FOV_Y + i as f32),
        };
        assert!((show.fov_at(&frame(Leg::TunnelOut, 0.5, 4.5)) - (FOV_Y + 16.0)).abs() < 1e-3);
        assert!((show.fov_at(&frame(Leg::TunnelOut, 2.0, 6.0)) - (FOV_Y + 32.0)).abs() < 1e-3);
        assert!((show.fov_at(&frame(Leg::Tunnel, 0.5, 2.0)) - FOV_Y).abs() < 1e-6);
    }

    fn show(f: PortalFrame) -> Show {
        Show {
            frame: f,
            at: Instant::now(),
            world: None,
            ui: None,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            collapse: [FOV_Y; COLLAPSE_SAMPLES],
        }
    }

    #[test]
    fn the_thread_never_draws_a_moment_earlier_than_it_has_on_the_same_trip() {
        let late = show(frame(Leg::Continue, 1.0, 3.0));
        let drawn = frame(Leg::Continue, 1.58, 3.58);
        let f = late.carried(0.01, Some(drawn));
        assert!((f.travelled - 3.58).abs() < 1e-6 && (f.phase - 1.58).abs() < 1e-6);
        // A new leg's phase starts again; another trip's moment is taken as it is.
        let f = late.carried(0.01, Some(frame(Leg::Tunnel, 2.5, 3.58)));
        assert!((f.phase - 1.01).abs() < 1e-6 && (f.travelled - 3.58).abs() < 1e-6);
        let f = late.carried(0.01, Some(PortalFrame { trip: 0, ..drawn }));
        assert!((f.travelled - 3.01).abs() < 1e-6);
    }

    /// The 0.2.0 login: the frame thread stalled 580 ms placing the world, then handed over the
    /// moment it read before the stall, stamped after it.
    #[test]
    fn a_stalled_frame_thread_never_restarts_the_tunnel() {
        let mut flight = Flight::default();
        let mut drawn = None;
        let mut eye = 0.0;
        let mut light = f32::NAN;
        let mut draw = |s: &Show, since: f32, drawn: &mut Option<PortalFrame>| {
            let f = s.carried(since, *drawn);
            *drawn = Some(f);
            let shot = flight.shot(&f, 1.6, 0.7);
            assert!(
                flight.z >= eye,
                "the eye went back at {}: {eye} to {}",
                f.travelled,
                flight.z
            );
            if light.is_finite() {
                assert!(
                    (flight.light - light).abs() < 30.0,
                    "the light jumped at {}",
                    f.travelled
                );
            }
            eye = flight.z;
            light = flight.light;
            shot
        };
        let step = 1.0 / 60.0;
        let before = show(frame(Leg::Continue, 1.0, 3.0));
        let mut since = 0.0;
        while since < 0.59 {
            draw(&before, since, &mut drawn);
            since += step;
        }
        let late = show(frame(Leg::Continue, 1.0, 3.0));
        for since in [0.0, step, 2.0 * step] {
            draw(&late, since, &mut drawn);
        }
        let caught_up = show(frame(Leg::Continue, 1.6, 3.6));
        draw(&caught_up, 0.0, &mut drawn);
        assert!(eye > 20.0, "the flight went on: {eye}");
    }

    #[test]
    fn a_new_trip_starts_the_flight_afresh() {
        let mut flight = Flight::default();
        for i in 0..60 {
            #[allow(clippy::cast_precision_loss)]
            let t = 2.0 + i as f32 / 60.0;
            flight.shot(&frame(Leg::Tunnel, t, t), 1.6, 0.7);
        }
        assert!(flight.z > 20.0);
        flight.shot(
            &PortalFrame {
                trip: 2,
                ..frame(Leg::TunnelIn, 0.5, 1.5)
            },
            1.6,
            0.7,
        );
        assert!(flight.z.abs() < 1e-6 && (flight.light - LIGHT_FAR).abs() < 1e-3);
    }

    #[test]
    fn a_tall_output_draws_the_tunnel_at_its_cap() {
        assert_eq!(inner_size((3840, 2160), 1080), (1920, 1080));
        assert_eq!(inner_size((800, 450), 1080), (800, 450));
    }
}
