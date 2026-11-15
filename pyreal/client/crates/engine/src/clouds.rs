//! The volumetric clouds (Cloud Detail Low to High; render-hifi's, ported): a layer of air over
//! the eye, its cover read from the dats' cloud deck laid over the world and drifting with the
//! game's clock, broken into billows by a tiling noise fixed to the world that turns over slowly,
//! lit by the sun through itself and by the sky about it.
//!
//! They are marched at two thirds of the frame's size before the lighting, each frame laid into a
//! history of their own that follows the camera's turn (`cloud_resolve.wgsl`): premultiplied
//! light in scene units and how much of what lies behind them they hide; the lighting lays it
//! over the heavens where nothing is drawn. Off, the dats' flat deck is drawn as it is. `@clouds`
//! sets [`CloudKnobs`]. A storm (or a wet day's overcast) closes the cover into a low, heavy,
//! darker deck.

use glam::{Vec2, Vec3, Vec4};

use crate::camera::Camera;
use crate::gpu::Gpu;
use crate::heavens::DeckNow;
use crate::sky::SkyLight;

/// The fine detail's texels a side, over one period.
pub const DETAIL_SIZE: u32 = 64;
/// How many periods of the fine detail one period of the deck's picture holds, each way.
pub const DETAIL_PER_PICTURE: f32 = 8.0;
/// The period the fine detail turns over in, seconds of the game's clock.
pub const DETAIL_PERIOD: f64 = 1800.0;
/// How far off, metres, the clouds keep a share 1/e of their own light, the rest the haze's.
pub const CLOUD_FADE: f32 = 40_000.0;
/// The steps through the layer at each Cloud Detail (Off, Low, Medium, High).
pub const STEPS: [u32; 4] = [0, 16, 24, 48];
/// How much the fine detail breaks the cover up at each Cloud Detail.
pub const DETAIL_SHARE: [f32; 4] = [0.0, 0.6, 0.85, 1.0];
/// The sun's light on the clouds and the sky's about them, against the scene's: the dats' deck's
/// calibration, so a noon cloud is white and not blown out.
pub const CLOUD_SUN: f32 = 0.35;
pub const CLOUD_SKY: f32 = 1.4;
/// How much of the night light's colour they lose at full night.
pub const NIGHT_GREY: f32 = 0.6;
/// The share of the sun's light on them that reaches every side, scattered about the deck.
pub const CLOUD_BOUNCE: f32 = 0.1;
/// The sun's height, degrees, over which the clouds' shadows come in: a lower sun's would lie
/// kilometres off along the ground.
pub const SHADOW_SUN: (f32, f32) = (8.0, 20.0);
/// The constant rows the clouds add to the air's block.
pub const ROWS: usize = 8;
/// The clouds' shadow map: its texels a side, and how far it reaches each way, metres.
pub const SHADOW_SIZE: u32 = 512;
pub const SHADOW_REACH: f32 = 6000.0;

/// The share of the dats' deck drift the clouds take: laid as large as they are at their height,
/// the deck's own rate would carry them at about 135 m/s; this keeps them near a breezy 13.
pub const DRIFT: f64 = 0.1;

/// How the clouds are drawn, as `@clouds` may ask.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CloudKnobs {
    /// How much of the sky they cover, against the deck's picture.
    pub cover: f32,
    /// How fast they drift, a share of the deck's drift.
    pub speed: f32,
    /// How high over the eye their base lies, metres.
    pub height: f32,
    /// How thick they are, metres.
    pub depth: f32,
    /// How large the deck's picture is laid over the sky, against its size overhead.
    pub size: f32,
    /// How much fine detail breaks them up, against Cloud Detail's.
    pub detail: f32,
    /// How brightly the sun lights them.
    pub light: f32,
    /// How dark their shaded sides and bases are.
    pub shade: f32,
    /// How much they shade the land under them.
    pub shadows: f32,
}

impl Default for CloudKnobs {
    fn default() -> Self {
        Self {
            cover: 1.0,
            speed: 1.0,
            height: 1500.0,
            depth: 900.0,
            size: 1.0,
            detail: 1.0,
            light: 1.0,
            shade: 1.0,
            shadows: 0.5,
        }
    }
}

/// What a frame's clouds are drawn from besides the light and the deck.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CloudFrame {
    /// Drawn this frame: the physical calibration, the outside seen, a deck, Cloud Detail on.
    pub on: bool,
    /// Where render space's origin lies in the world, metres.
    pub world_origin: Vec2,
    /// How far a storm has come in (dark, low cloud), 0 to 1, and the lightning.
    pub storm: f32,
    pub flash: f32,
    /// How far the rain has closed the sky over, 0 to 1.
    pub close: f32,
    /// The clouds' picture's height, pixels.
    pub height: u32,
    /// The frames laid into their history so far, 0 to start it afresh.
    pub history: u32,
}

/// `v` brought into `0..period`.
fn wrap(v: f64, period: f64) -> f32 {
    if period <= 0.0 {
        return 0.0;
    }
    #[allow(clippy::cast_possible_truncation)]
    let w = v.rem_euclid(period) as f32;
    w
}

/// The deck's laying as the clouds lay it: its steps shrunk so the picture is as large overhead
/// at the clouds' height as the dats show it at the deck's own, times `size`.
#[must_use]
pub fn laying(deck: &DeckNow, knobs: &CloudKnobs) -> [f32; 4] {
    let k = knobs.height / deck.deck.height.max(50.0) * knobs.size.max(1e-3);
    let [a, b, c, d] = deck.deck.laying;
    [a, b / k, c, d / k]
}

/// The clouds' rows of the air's block (see `air_model.wgsl`): all zero when they are not drawn.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn rows(
    deck: Option<&DeckNow>,
    knobs: &CloudKnobs,
    detail: u32,
    light: &SkyLight,
    camera: &Camera,
    frame: &CloudFrame,
    clock: f64,
) -> [Vec4; ROWS] {
    let mut r = [Vec4::ZERO; ROWS];
    let Some(deck) = deck.filter(|_| frame.on && detail > 0) else {
        return r;
    };
    let level = detail.min(3) as usize;
    let lay = laying(deck, knobs);
    let (b, d) = (
        f64::from(lay[1]).abs().max(1e-9),
        f64::from(lay[3]).abs().max(1e-9),
    );
    let (period_x, period_y) = (1.0 / b, 1.0 / d);
    // The deck's drift in metres, so the billows drift with the cover.
    let speed = f64::from(knobs.speed) * DRIFT;
    let drift = |vel: f32, step: f32| clock * f64::from(vel) * speed / f64::from(step);
    let offset = Vec2::new(
        wrap(
            f64::from(frame.world_origin.x) + drift(deck.velocity.x, lay[1]),
            period_x,
        ),
        wrap(
            f64::from(frame.world_origin.y) + drift(deck.velocity.y, lay[3]),
            period_y,
        ),
    );
    #[allow(clippy::cast_precision_loss)]
    let steps = STEPS[level] as f32;
    #[allow(clippy::cast_precision_loss)]
    let octaves = level as f32;
    // A storm brings the base down; rain thickens the layer.
    let storm = frame.storm.clamp(0.0, 1.0);
    r[0] = Vec4::new(
        steps,
        knobs.cover,
        knobs.height * (1.0 - 0.45 * storm),
        knobs.depth.max(50.0) * (1.0 + 0.5 * frame.close.clamp(0.0, 1.0)),
    );
    r[1] = Vec4::from_array(lay);
    let turned = wrap(clock / DETAIL_PERIOD, 1.0);
    r[2] = Vec4::new(offset.x, offset.y, turned, octaves);
    // The sun's light on them, near white by day, its own colour as the day goes; the light
    // scattered about inside the deck adds to the sky's.
    let peak = light.sun_light.max_element();
    let hue = if peak > 1e-6 {
        light.sun_light / peak
    } else {
        Vec3::ONE
    };
    // By night the dats' purple light reaches them greyed: their own glow is the only purple.
    let grey = 1.0 - NIGHT_GREY * crate::atmosphere::night_share(light);
    let tint = Vec3::ONE.lerp(hue, (0.35 + 0.65 * (1.0 - light.daylight)) * grey);
    // By night the moon lights them more faintly than the sun, so they stay dim.
    let night = 0.4 + 0.6 * light.daylight;
    let sun = tint * peak * CLOUD_SUN * night * knobs.light * deck.brightness;
    r[3] = sun.extend(knobs.shade);
    let luma = |c: Vec3| c.dot(Vec3::new(0.2126, 0.7152, 0.0722)).max(1e-9);
    let ambient = Vec3::splat(luma(light.sky_light)).lerp(light.sky_light, grey);
    // By night no brighter than the sky's light makes them, so they do not outshine the land.
    let sky = ambient * CLOUD_SKY * deck.brightness + sun * CLOUD_BOUNCE;
    r[4] = sky.extend(DETAIL_SHARE[level] * knobs.detail);
    #[allow(clippy::cast_precision_loss)]
    let pixel = camera.fov_y / frame.height.max(1) as f32;
    #[allow(clippy::cast_possible_truncation)]
    let per = (
        (b * f64::from(DETAIL_PER_PICTURE)) as f32,
        (d * f64::from(DETAIL_PER_PICTURE)) as f32,
    );
    r[5] = Vec4::new(pixel, CLOUD_FADE, per.0, per.1);
    let elevation =
        pyreal_primitives::num::math::asinf(light.sun_dir.z.clamp(-1.0, 1.0)).to_degrees();
    let rise = ((elevation - SHADOW_SUN.0) / (SHADOW_SUN.1 - SHADOW_SUN.0)).clamp(0.0, 1.0);
    let shadows = knobs.shadows * light.daylight * rise * rise * (3.0 - 2.0 * rise);
    #[allow(clippy::cast_precision_loss)]
    let history = (frame.history % (1 << 20)) as f32;
    r[6] = Vec4::new(
        frame.storm.clamp(0.0, 1.0),
        frame.flash,
        frame.close.clamp(0.0, 1.0),
        history,
    );
    // The shadow map over the clouds' base where the sun's line from the eye meets it, its texels
    // fixed to the world so the shadows hold still on the land as the eye moves.
    let sun_dir = light.sun_dir;
    let rise_to = (knobs.height / sun_dir.z.max(0.05)).min(SHADOW_REACH * 0.7);
    #[allow(clippy::cast_precision_loss)]
    let texel = 2.0 * SHADOW_REACH / SHADOW_SIZE as f32;
    let world = camera.position.truncate() + sun_dir.truncate() * rise_to + frame.world_origin;
    let centre = (world / texel).round() * texel - frame.world_origin;
    r[7] = Vec4::new(centre.x, centre.y, SHADOW_REACH, shadows);
    r
}

/// The clouds' pass: its pipeline, the fine detail and the sampler that wraps it.
#[derive(Debug)]
pub struct Clouds {
    pub layout: wgpu::BindGroupLayout,
    pub pipeline: wgpu::ComputePipeline,
    /// The shadow map's pass, and the map.
    pub shadow_pipeline: wgpu::ComputePipeline,
    pub shadow: wgpu::TextureView,
    _shadow: wgpu::Texture,
    /// Their history's passes: each frame's march laid into it, and the result kept.
    pub resolve_layout: wgpu::BindGroupLayout,
    pub resolve_pipeline: wgpu::ComputePipeline,
    pub keep_pipeline: wgpu::ComputePipeline,
    clamp_sampler: wgpu::Sampler,
    detail: wgpu::TextureView,
    sampler: wgpu::Sampler,
    _texture: wgpu::Texture,
}

/// The clouds' picture's format.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

impl Clouds {
    /// The pipeline (the frame's group first, the material table's third) and the detail.
    #[must_use]
    pub fn new(
        gpu: &Gpu,
        frame_layout: &wgpu::BindGroupLayout,
        materials_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        use wgpu::ShaderStages as S;
        let device = &gpu.device;
        let levels = detail_noise();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cloud detail"),
            size: wgpu::Extent3d {
                width: DETAIL_SIZE,
                height: DETAIL_SIZE,
                depth_or_array_layers: DETAIL_SIZE,
            },
            mip_level_count: u32::try_from(levels.len()).unwrap_or(1),
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut side = DETAIL_SIZE;
        for (k, level) in levels.iter().enumerate() {
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: u32::try_from(k).unwrap_or(0),
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                level,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(side),
                    rows_per_image: Some(side),
                },
                wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: side,
                },
            );
            side = (side / 2).max(1);
        }
        let detail = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cloud detail"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("clouds"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: S::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: S::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: FORMAT,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: S::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: S::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: S::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: FORMAT,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
        let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cloud shadows"),
            size: wgpu::Extent3d {
                width: SHADOW_SIZE,
                height: SHADOW_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow = shadow_texture.create_view(&Default::default());
        let module = gpu.shader("clouds", &crate::renderer::source::clouds());
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("clouds"),
            bind_group_layouts: &[Some(frame_layout), Some(&layout), Some(materials_layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str| {
            gpu.compute_pipeline(wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: S::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let resolve_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cloud history"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: S::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: S::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: FORMAT,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
        let resolve_module = gpu.shader("cloud history", &crate::renderer::source::cloud_resolve());
        let resolve_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("cloud history"),
                bind_group_layouts: &[Some(frame_layout), Some(&resolve_layout)],
                immediate_size: 0,
            });
        let resolve_pipeline = |entry: &str| {
            gpu.compute_pipeline(wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&resolve_pipeline_layout),
                module: &resolve_module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let clamp_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cloud history"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            layout,
            pipeline: pipeline("clouds"),
            shadow_pipeline: pipeline("cloud_shadows"),
            shadow,
            _shadow: shadow_texture,
            resolve_layout,
            resolve_pipeline: resolve_pipeline("cloud_resolve"),
            keep_pipeline: resolve_pipeline("cloud_keep"),
            clamp_sampler,
            detail,
            sampler,
            _texture: texture,
        }
    }

    /// The pass's group, reading `depth` and writing `out` and the shadow map.
    #[must_use]
    pub fn group(
        &self,
        gpu: &Gpu,
        depth: &wgpu::TextureView,
        out: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("clouds"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(out),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.detail),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&self.shadow),
                },
            ],
        })
    }
}

impl Clouds {
    /// The history's group, laying `marched` into `history` to write `out`.
    #[must_use]
    pub fn resolve_group(
        &self,
        gpu: &Gpu,
        marched: &wgpu::TextureView,
        history: &wgpu::TextureView,
        out: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cloud history"),
            layout: &self.resolve_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(marched),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(history),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.clamp_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(out),
                },
            ],
        })
    }
}

/// The clouds' fine detail: noise of billows, the same in every direction, repeating over its
/// cube [`DETAIL_SIZE`] texels a side, one byte a texel, and every smaller level of it after it.
#[must_use]
pub fn detail_noise() -> Vec<Vec<u8>> {
    let n = DETAIL_SIZE as usize;
    let mut values = vec![0.0f32; n * n * n];
    #[allow(clippy::cast_precision_loss)]
    let inv = 1.0 / n as f32;
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                #[allow(clippy::cast_precision_loss)]
                let p = [x as f32 * inv, y as f32 * inv, z as f32 * inv];
                let w1 = 1.0 - worley(p, 6, 0x1a2b);
                let w2 = 1.0 - worley(p, 12, 0x3c4d);
                let g = gradient(p, 6, 0x5e6f) * 0.5 + 0.5;
                values[(z * n + y) * n + x] = 0.5 * w1 + 0.25 * w2 + 0.25 * g;
            }
        }
    }
    let lo = values.iter().copied().fold(f32::MAX, f32::min);
    let hi = values.iter().copied().fold(f32::MIN, f32::max);
    let span = (hi - lo).max(1e-6);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let level0: Vec<u8> = values
        .iter()
        .map(|v| ((v - lo) / span * 255.0).round().clamp(0.0, 255.0) as u8)
        .collect();
    let mut levels = vec![level0];
    let mut side = n;
    while side > 1 {
        let prev = levels.last().map_or(&[][..], Vec::as_slice);
        let half = side / 2;
        let mut next = vec![0u8; half * half * half];
        for z in 0..half {
            for y in 0..half {
                for x in 0..half {
                    let mut sum = 0u32;
                    for k in 0..8 {
                        let (dx, dy, dz) = (k & 1, (k >> 1) & 1, k >> 2);
                        sum +=
                            u32::from(prev[((2 * z + dz) * side + 2 * y + dy) * side + 2 * x + dx]);
                    }
                    #[allow(clippy::cast_possible_truncation)]
                    let mean = ((sum + 4) / 8) as u8;
                    next[(z * half + y) * half + x] = mean;
                }
            }
        }
        levels.push(next);
        side = half;
    }
    levels
}

/// A well-mixed 32-bit value from `v`.
#[must_use]
pub fn pcg(v: u32) -> u32 {
    let state = v.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let word = ((state >> ((state >> 28) + 4)) ^ state).wrapping_mul(277_803_737);
    (word >> 22) ^ word
}

/// A hash's low bits as a share of one.
fn unit(h: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let u = (h & 0x00ff_ffff) as f32 / 16_777_216.0;
    u
}

/// A well-mixed value for lattice cell `c`.
fn cell_hash(c: [i32; 3], seed: u32) -> u32 {
    pcg(c[0].cast_unsigned() ^ pcg(c[1].cast_unsigned() ^ pcg(c[2].cast_unsigned() ^ seed)))
}

/// The cell of `q` on a lattice, and where in it `q` lies.
fn cell_of(q: [f32; 3]) -> ([i32; 3], [f32; 3]) {
    let c = q.map(pyreal_primitives::num::floor_to_i32);
    #[allow(clippy::cast_precision_loss)]
    let f = [q[0] - c[0] as f32, q[1] - c[1] as f32, q[2] - c[2] as f32];
    (c, f)
}

/// The distance from `p` (each axis 0..1) to the nearest of a point in each cell of a `cells`
/// sided grid that wraps, as a share of a cell, at most 1.
fn worley(p: [f32; 3], cells: i32, seed: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let g = cells as f32;
    let (c, f) = cell_of(p.map(|v| v * g));
    let mut best = f32::MAX;
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cell = [c[0] + dx, c[1] + dy, c[2] + dz];
                let h = cell_hash(cell.map(|v| v.rem_euclid(cells)), seed);
                #[allow(clippy::cast_precision_loss)]
                let at = [
                    dx as f32 + unit(h),
                    dy as f32 + unit(pcg(h)),
                    dz as f32 + unit(pcg(h ^ 0x9e37)),
                ];
                let d = (at[0] - f[0]).powi(2) + (at[1] - f[1]).powi(2) + (at[2] - f[2]).powi(2);
                best = best.min(d);
            }
        }
    }
    best.sqrt().min(1.0)
}

/// Gradient noise at `p` (each axis 0..1) over a lattice `cells` a side that wraps, about -1..1.
fn gradient(p: [f32; 3], cells: i32, seed: u32) -> f32 {
    // One of the twelve edges of a cube.
    const DIRS: [[f32; 3]; 12] = [
        [1.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0],
        [1.0, -1.0, 0.0],
        [-1.0, -1.0, 0.0],
        [1.0, 0.0, 1.0],
        [-1.0, 0.0, 1.0],
        [1.0, 0.0, -1.0],
        [-1.0, 0.0, -1.0],
        [0.0, 1.0, 1.0],
        [0.0, -1.0, 1.0],
        [0.0, 1.0, -1.0],
        [0.0, -1.0, -1.0],
    ];
    #[allow(clippy::cast_precision_loss)]
    let g = cells as f32;
    let (c, f) = cell_of(p.map(|v| v * g));
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let u = f.map(fade);
    let corner = |dx: i32, dy: i32, dz: i32| {
        let w = [
            (c[0] + dx).rem_euclid(cells),
            (c[1] + dy).rem_euclid(cells),
            (c[2] + dz).rem_euclid(cells),
        ];
        let dir = DIRS[(cell_hash(w, seed) % 12) as usize];
        #[allow(clippy::cast_precision_loss)]
        let off = [f[0] - dx as f32, f[1] - dy as f32, f[2] - dz as f32];
        dir[0] * off[0] + dir[1] * off[1] + dir[2] * off[2]
    };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(corner(0, 0, 0), corner(1, 0, 0), u[0]);
    let x10 = lerp(corner(0, 1, 0), corner(1, 1, 0), u[0]);
    let x01 = lerp(corner(0, 0, 1), corner(1, 0, 1), u[0]);
    let x11 = lerp(corner(0, 1, 1), corner(1, 1, 1), u[0]);
    lerp(lerp(x00, x10, u[1]), lerp(x01, x11, u[1]), u[2])
}

/// How far a storm has come in for the clouds, 0 to 1: the rain's share of a stormy day, a
/// lighter share of a plain rainy one, a wet day's overcast, or `asked` if more.
#[must_use]
pub fn storm(rain: f32, stormy: bool, overcast: f32, asked: f32) -> f32 {
    let wet = rain * if stormy { 1.0 } else { 0.35 };
    wet.max(overcast * 0.5).max(asked).clamp(0.0, 1.0)
}
