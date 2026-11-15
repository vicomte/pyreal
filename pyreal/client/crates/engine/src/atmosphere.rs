//! The atmosphere (ported from render-hifi's): the sky's lookup tables in compute, the
//! constants every pass reads to draw the sky and the air, and the heavens' places.
//!
//! The transmittance and multiple-scattering tables are made when the air changes (the weather);
//! the sky-view table every frame, for the sun as the sky sees it. The dats' sun never sets (at
//! night it is the moon's light), so the sky's sun stands at the dats' bearing, lowered below the
//! horizon as the daylight goes ([`sky_sun`]). The haze over the land is calibrated to the world
//! fog's range and coloured by it, so each region's fog still says how far one sees.

use glam::{Vec2, Vec3, Vec4};

use crate::camera::Camera;
use crate::gpu::Gpu;
use crate::heavens::HeavensNow;
use crate::sky::SkyLight;

/// The planet's radius and the atmosphere's outer radius in the sky model, kilometres.
pub const PLANET_RADIUS_KM: f32 = 6360.0;
pub const TOP_RADIUS_KM: f32 = 6460.0;
/// The tables' sizes.
pub const TRANSMITTANCE_SIZE: (u32, u32) = (256, 64);
pub const MULTIPLE_SIZE: (u32, u32) = (32, 32);
pub const SKY_VIEW_SIZE: (u32, u32) = (192, 108);
/// How many times thicker than the real air the air before the world is drawn.
pub const AERIAL_THICKNESS: f32 = 4.0;
/// The haze's optical depth at the fog's end, by day and by night.
pub const HAZE_DEPTH: (f32, f32) = (2.2, 3.0);
/// The haze's height scale, metres.
pub const HAZE_HEIGHT: f32 = 450.0;
/// How much brighter than the fog the haze glows looking at the sun.
pub const HAZE_GLOW: f32 = 1.5;
/// How far below the horizon the sky's sun stands at night, degrees.
pub const NIGHT_SUN: f32 = -12.0;
/// The sun's disc: its angular radius (degrees, a little over the real sun's so it reads), and
/// the most its radiance is drawn at, scene units.
pub const SUN_RADIUS: f32 = 0.5;
pub const SUN_DISC_MOST: f32 = 4000.0;
/// The moons' radiance, scene units: bright against the night, not a blinding lamp.
pub const MOON_RADIANCE: f32 = 1.2e-3;
/// The stars' brightest, scene units.
pub const STAR_RADIANCE: f32 = 6.0e-4;
/// The constant block's bytes: 24 vectors, the clouds' and their night glow.
pub const AIR_SIZE: u64 = (25 + crate::clouds::ROWS as u64) * 16;
/// The clouds' purple glow at full night, against the night sky's light: a nod to retail's
/// purple night deck, far fainter.
pub const NIGHT_GLOW: f32 = 0.15;
/// The daylight under which the glow comes in.
pub const NIGHT_GLOW_DUSK: f32 = 0.35;

/// The haze's thickness unless asked otherwise: a clear day's distant hills stay in sight.
pub const DEFAULT_FOG: f32 = 0.4;

/// The reach past which the world fog's end stretches with it, metres: the Extreme draw
/// distance's (25 blocks).
pub const HAZE_REACH: f32 = 25.5 * 192.0;

/// The world fog's end `fog_end` for land loaded out to `far` metres: as it is to [`HAZE_REACH`],
/// stretched with the reach past it, so a farther draw distance's land shows as the Extreme's.
#[must_use]
pub fn fog_end_for(fog_end: f32, far: f32) -> f32 {
    fog_end * (far / HAZE_REACH).max(1.0)
}

/// Where the haze begins and where it reaches its full depth, metres: the world fog's range (to
/// `far`, the loaded land's edge, and stretched with it past [`HAZE_REACH`]) stretched by one
/// over `density`, so a thinner haze reaches as deep only farther off; none at all at nought.
#[must_use]
pub fn haze_range(light: &SkyLight, far: f32, density: f32) -> (f32, f32) {
    let end = fog_end_for(light.fog_end, far).min(far).max(50.0);
    let start = light.fog_start.min(end * 0.6);
    if density <= 0.0 {
        return (1.0e9, 2.0e9);
    }
    (start, start + (end - start) / density)
}

/// The direction toward the sun as the sky sees it: the dats' bearing, at their elevation by
/// day, lowered toward [`NIGHT_SUN`] as the daylight goes.
#[must_use]
pub fn sky_sun(light: &SkyLight) -> Vec3 {
    use pyreal_primitives::num::math::{asinf, cosf, sinf};
    let d = light.sun_dir;
    let flat = Vec2::new(d.x, d.y).normalize_or(Vec2::X);
    let up = asinf(d.z.clamp(-1.0, 1.0));
    let el = NIGHT_SUN.to_radians() + (up - NIGHT_SUN.to_radians()) * light.daylight;
    (flat * cosf(el)).extend(sinf(el))
}

/// The night clouds' hue, of unit luma: the dats' ambient, deepened toward magenta.
#[must_use]
pub fn night_hue(light: &SkyLight) -> Vec3 {
    let luma = |c: Vec3| c.dot(Vec3::new(0.2126, 0.7152, 0.0722)).max(1e-9);
    let hue = light.ambient / luma(light.ambient);
    let hue = (Vec3::ONE + (hue - Vec3::ONE) * 2.0).max(Vec3::ZERO) * Vec3::new(1.3, 0.95, 0.85);
    hue / luma(hue)
}

/// How far the night has come in for the clouds' glow, 0 to 1.
#[must_use]
pub fn night_share(light: &SkyLight) -> f32 {
    let t = (light.daylight / NIGHT_GLOW_DUSK).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// The clouds' night glow, scene units: [`night_hue`] as bright as [`NIGHT_GLOW`] of the night
/// sky's light, coming in as the daylight goes; w the night's share.
#[must_use]
pub fn night_glow(light: &SkyLight) -> Vec4 {
    let night = night_share(light);
    let luma = light.sky_light.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    (night_hue(light) * luma * NIGHT_GLOW * night).extend(night)
}

/// The tables and the constant block.
#[derive(Debug)]
pub struct Atmosphere {
    pub constants: wgpu::Buffer,
    pub transmittance: wgpu::TextureView,
    pub sky_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pipelines: [wgpu::ComputePipeline; 3],
    groups: [wgpu::BindGroup; 3],
    /// The air the transmittance and multiple tables were made for.
    made_for: Option<[u32; 2]>,
    /// This frame's air wants them made again.
    stale: bool,
    _textures: [wgpu::Texture; 3],
    /// How thick the haze over the land is (Fog Density): 1 the world fog's own range, 0 none,
    /// [`DEFAULT_FOG`] unless asked.
    pub fog_density: f32,
    /// How finely the clouds are drawn (Cloud Detail): 0 the dats' flat deck as it is, 1 to 3
    /// volumetric clouds, marched in more steps with finer detail at each.
    pub cloud_detail: u32,
    /// How the volumetric clouds are drawn, as `@clouds` asks.
    pub cloud_knobs: crate::clouds::CloudKnobs,
    /// How far a storm has come in for the clouds, 0 to 1, over what the weather gives.
    pub storm: f32,
    /// How much of each moon's face is lit, by its graphics id; a moon not named is full.
    pub moon_phases: [(u32, f32); 2],
    /// How thickly snow falls round the eye, 0 to 1 and more in a blizzard: the haze closes in.
    pub snowfall: f32,
}

fn f32s(out: &mut Vec<u8>, v: &[f32]) {
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
}

impl Atmosphere {
    #[must_use]
    pub fn new(gpu: &Gpu) -> Self {
        use wgpu::ShaderStages as S;
        let device = &gpu.device;
        let constants = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("air"),
            size: AIR_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let table = |label, (w, h): (u32, u32)| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let t = table("transmittance table", TRANSMITTANCE_SIZE);
        let m = table("multiple scattering table", MULTIPLE_SIZE);
        let v = table("sky-view table", SKY_VIEW_SIZE);
        let tv = t.create_view(&Default::default());
        let mv = m.create_view(&Default::default());
        let vv = v.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("air"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: S::COMPUTE,
            ty,
            count: None,
        };
        let sampled = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("air tables"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(1, sampled),
                entry(2, sampled),
                entry(
                    3,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(
                    4,
                    wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                ),
            ],
        });
        // A stand-in for the table a dispatch writes, where it would also be read.
        let blank = table("air blank", (1, 1));
        let bv = blank.create_view(&Default::default());
        let group = |a: &wgpu::TextureView, b: &wgpu::TextureView, out: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("air tables"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: constants.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(a),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(b),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(out),
                    },
                ],
            })
        };
        let groups = [
            group(&bv, &bv, &tv),
            group(&tv, &bv, &mv),
            group(&tv, &mv, &vv),
        ];
        let module = gpu.shader("air tables", &crate::renderer::source::air_tables());
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("air tables"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |e: &str| {
            gpu.compute_pipeline(wgpu::ComputePipelineDescriptor {
                label: Some(e),
                layout: Some(&pl),
                module: &module,
                entry_point: Some(e),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            constants,
            transmittance: tv,
            sky_view: vv,
            sampler,
            pipelines: [
                pipeline("transmittance"),
                pipeline("multiple"),
                pipeline("sky_view"),
            ],
            groups,
            made_for: None,
            stale: true,
            _textures: [t, m, v],
            cloud_detail: 2,
            cloud_knobs: crate::clouds::CloudKnobs::default(),
            storm: 0.0,
            moon_phases: [(0, 1.0); 2],
            fog_density: DEFAULT_FOG,
            snowfall: 0.0,
        }
    }

    /// Writes this frame's constants: the air of `light`'s weather, its sun, the haze to its fog,
    /// the heavens' places and the clouds, seen from `camera`.
    pub fn write(
        &mut self,
        gpu: &Gpu,
        light: &SkyLight,
        camera: &Camera,
        heavens: Option<&HeavensNow>,
        clock: f64,
        clouds: &crate::clouds::CloudFrame,
    ) {
        use pyreal_primitives::num::math::cosf;
        let (mie, overcast) = light.weather.haze();
        let key = [mie.to_bits(), 0];
        self.stale = self.made_for != Some(key);
        self.made_for = Some(key);
        let sun = sky_sun(light);
        let sun_top = crate::sky::SUN_LUX / crate::sky::SCENE_LUX;
        let day = light.daylight;
        let mut c = Vec::with_capacity(usize::try_from(AIR_SIZE).unwrap_or(0));
        let rayleigh = Vec3::new(5.802e-3, 13.558e-3, 33.1e-3) * (1.0 - 0.25 * overcast);
        f32s(&mut c, &rayleigh.extend(-1.0 / 8.0).to_array());
        f32s(&mut c, &[3.996e-3 * mie, 4.44e-3 * mie, -1.0 / 1.2, 0.8]);
        f32s(&mut c, &[0.650e-3, 1.881e-3, 0.085e-3, PLANET_RADIUS_KM]);
        f32s(&mut c, &[0.25, 0.27, 0.2, TOP_RADIUS_KM]);
        f32s(&mut c, &sun.extend(sun_top).to_array());
        f32s(
            &mut c,
            &[
                camera.position.z.max(0.0) * 0.001 + 0.05,
                AERIAL_THICKNESS * (self.fog_density / DEFAULT_FOG).min(1.5),
                day,
                overcast,
            ],
        );
        let (fog_start, fog_end) = haze_range(light, camera.far, self.fog_density);
        // Falling snow hides the distance far sooner than rain.
        let thick = 1.0 + 1.6 * self.snowfall.max(0.0);
        let (fog_start, fog_end) = (fog_start / thick, fog_end / thick);
        let depth = if self.fog_density > 0.0 {
            HAZE_DEPTH.1 + (HAZE_DEPTH.0 - HAZE_DEPTH.1) * day
        } else {
            0.0
        };
        f32s(&mut c, &[fog_start, fog_end, depth, HAZE_HEIGHT]);
        f32s(
            &mut c,
            &(light.fog * light.sky_scale()).extend(HAZE_GLOW).to_array(),
        );
        // The moons.
        let none = crate::asset::material::NO_TEXTURE;
        let moons = heavens.map_or(&[][..], |h| &h.moons[..]);
        let moon = |k: usize| moons.get(k);
        let phase = |gfx: u32| {
            self.moon_phases
                .iter()
                .find(|(g, _)| *g == gfx)
                .map_or(1.0, |(_, lit)| lit.clamp(0.0, 1.0))
        };
        for k in 0..2 {
            let d = moon(k).map_or(Vec4::ZERO, |m| m.dir.extend(phase(m.gfx)));
            f32s(&mut c, &d.to_array());
        }
        for k in 0..2 {
            let u = moon(k).map_or(Vec4::ZERO, |m| m.u.extend(m.uv0.x));
            f32s(&mut c, &u.to_array());
        }
        for k in 0..2 {
            let v = moon(k).map_or(Vec4::ZERO, |m| m.v.extend(m.uv0.y));
            f32s(&mut c, &v.to_array());
        }
        for k in 0..2 {
            let w = moon(k).map_or([none, 0, none, none], |m| {
                [m.texture, if m.ball { 2 } else { 1 }, m.lights, m.clouds]
            });
            for x in w {
                c.extend_from_slice(&x.to_le_bytes());
            }
        }
        for k in 0..2 {
            let l = moon(k).map_or(Vec4::ZERO, |m| {
                Vec4::new(
                    MOON_RADIANCE,
                    MOON_RADIANCE,
                    MOON_RADIANCE * 1.05,
                    m.opacity,
                )
            });
            f32s(&mut c, &l.to_array());
        }
        // The deck.
        let deck = heavens.and_then(|h| h.deck);
        f32s(&mut c, &deck.map_or([0.0; 4], |d| d.deck.laying));
        f32s(
            &mut c,
            &deck.map_or([0.0; 4], |d| {
                #[allow(clippy::cast_precision_loss)]
                [
                    d.deck.height.max(200.0),
                    d.opacity,
                    d.brightness,
                    self.cloud_detail as f32,
                ]
            }),
        );
        for x in [deck.map_or(none, |d| d.texture), 0, 0, 0] {
            c.extend_from_slice(&x.to_le_bytes());
        }
        f32s(
            &mut c,
            &deck.map_or([0.0; 4], |d| [d.scroll.x, d.scroll.y, 0.0, 0.0]),
        );
        // The stars, hidden where the deck covers the sky, and the sun's disc.
        #[allow(clippy::cast_possible_truncation)]
        let twinkle = (clock % 600.0) as f32;
        let turn = heavens.map_or(0.0, |h| h.time) * std::f32::consts::TAU;
        let cover = deck.map_or(0.0, |d| d.opacity * 0.5);
        f32s(&mut c, &[STAR_RADIANCE * (1.0 - cover), twinkle, turn, 0.0]);
        let cos_r = cosf(SUN_RADIUS.to_radians());
        let solid = std::f32::consts::TAU * (1.0 - cos_r);
        let disc = (sun_top / solid).min(SUN_DISC_MOST) * crate::sky::sun_share(day);
        f32s(&mut c, &[disc, disc * 0.98, disc * 0.94, cos_r]);
        for row in crate::clouds::rows(
            deck.as_ref(),
            &self.cloud_knobs,
            self.cloud_detail,
            light,
            camera,
            clouds,
            clock,
        ) {
            f32s(&mut c, &row.to_array());
        }
        f32s(&mut c, &night_glow(light).to_array());
        gpu.queue.write_buffer(&self.constants, 0, &c);
    }

    /// The tables: the transmittance and multiple scattering when the air changed, the sky
    /// from the eye every frame.
    pub fn encode(&self, pass: &mut wgpu::ComputePass<'_>) {
        let sizes = [TRANSMITTANCE_SIZE, MULTIPLE_SIZE, SKY_VIEW_SIZE];
        for (k, ((pipeline, group), size)) in self
            .pipelines
            .iter()
            .zip(&self.groups)
            .zip(sizes)
            .enumerate()
        {
            if k < 2 && !self.stale {
                continue;
            }
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups(size.0.div_ceil(8), size.1.div_ceil(8), 1);
        }
    }
}
