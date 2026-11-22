//! Weather: rain and snow on the game's rainy days, wet ground and puddles, ripples on the water,
//! and the flashes of a storm's lightning (render-hifi's weather, cut down to the engine's frame).
//!
//! The client says what falls ([`Weather::falls`]: the day's weather group is a rainy one, or the
//! player asked for rain or snow) and how much of the land round the eye is snowy
//! ([`Weather::snow_share`]); the engine eases rain, snow and the ground's wetness in and out over
//! [`EASE_SECONDS`] by its clock, so nothing switches in one frame. On the day's own weather
//! ([`Fall::ByLand`]) each drop falls as rain or snow by the land under it ([`snowy_types`]): snow
//! over snowy terrain, rain elsewhere, mixed in a band where they meet. While it falls:
//!
//! - drops fixed in the world fill a box round the eye, drawn into the lit picture as streaks a
//!   shutter blurs, hidden behind what stands nearer, none under a roof (the [`Shelter`] map), in
//!   the room the eye stands in, or within a metre of the eye;
//! - flakes fill a near box and a wide one, each its own size, pace and flutter, carried by the
//!   wind and swirled by its eddies, blurred by the shutter, lit through from behind by the sun; a
//!   storm's snow is a blizzard, driven near flat;
//! - fresh snow gathers on the open snowy ground and the tops of what stands on it while it snows
//!   ([`Weather::cover`]), and melts back over a quarter of an hour once it stops;
//! - the ground outdoors darkens and grows a little glossier, more where it faces up, and gathers
//!   puddles in its hollows and on the flat (a noise laid on the world, the land's lie) that grow
//!   while it rains, mirror the sky and their surroundings and ring with the drops; it dries over a
//!   minute or two once the rain stops; none of it under a roof or a porch;
//! - the water's ripples take the drops;
//! - on a stormy day lightning flashes now and then, lighting the sky and the land for a moment.
//!
//! Indoors with no view out, nothing is drawn. Weather Effects off, none of it.

use crate::gpu::Gpu;

/// How long rain or snow takes to come or go, seconds.
pub const EASE_SECONDS: f32 = 3.0;
/// How long the ground takes to dry once the rain stops, seconds.
pub const DRY_SECONDS: f32 = 60.0;
/// How long the puddles take to fill while it rains, and to dry once it stops, seconds.
pub const PUDDLE_SECONDS: f32 = 30.0;
pub const PUDDLE_DRY_SECONDS: f32 = 90.0;
/// The shelter map: its side, texels; how far it reaches round the eye, and above and below it,
/// metres.
pub const SHELTER_SIDE: u32 = 2048;
pub const SHELTER_REACH: f32 = 128.0;
pub const SHELTER_DEPTH: f32 = 300.0;
/// The upscaler's reactive mask's format, the pass's second target.
pub const REACTIVE: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
/// The shelter map's format.
pub const SHELTER_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// The clock's wrap for the ground's ripples and splashes, seconds.
pub const RIPPLE_WRAP: f64 = 600.0;
/// The box of drops round the eye, metres, and how many it holds.
pub const RAIN_BOX: f32 = 36.0;
pub const RAIN_DROPS: u32 = 14_000;
pub const SNOW_BOX: f32 = 40.0;
pub const SNOW_FLAKES: u32 = 30_000;
/// The near box of flakes round the eye, metres, and how many it holds.
pub const SNOW_NEAR_BOX: f32 = 9.0;
pub const SNOW_NEAR_FLAKES: u32 = 7_000;
/// How fast they fall, metres a second.
pub const RAIN_SPEED: f32 = 9.0;
pub const SNOW_SPEED: f32 = 1.0;
/// How hard a storm's gale drives the rain sideways, metres a second: about 26 degrees off the
/// fall.
pub const STORM_PUSH: f32 = 5.6;
/// How hard a light rain drifts with the breeze, metres a second per unit of breeze: about 13
/// degrees off the fall in a fresh breeze.
pub const RAIN_DRIFT: f32 = 3.8;
/// How hard a blizzard drives the snow, metres a second.
pub const BLIZZARD_PUSH: f32 = 8.0;
/// How long fresh snow takes to cover the ground while it snows, and to melt once it stops,
/// seconds.
pub const COVER_SECONDS: f32 = 240.0;
pub const MELT_SECONDS: f32 = 900.0;
/// The uniform's bytes.
const PARAMS_SIZE: u64 = 64 + 32 + 16 * 16 + 16;
/// The most planes of the eye's room the drops are kept out of.
const ROOM_PLANES: usize = 16;
/// The clock's wrap for the flakes' wobble, seconds.
const WRAP: f64 = 1200.0;
/// The drops' fall speeds are this many twentieths of the box's and over (`precip.wgsl`): what
/// they have fallen wraps at twenty boxes, a whole number of boxes for each.
const FALL_WRAP: f64 = 20.0;

/// What falls from the sky.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fall {
    #[default]
    Nothing,
    /// Rain everywhere.
    Rain,
    /// Snow everywhere.
    Snow,
    /// Snow over snowy terrain and rain elsewhere, drop by drop.
    ByLand,
}

/// The terrain types (bits 0 to 31) whose ground is snow or ice, by their names.
#[must_use]
pub fn snowy_types(region: &crate::world::Region) -> u32 {
    types_named(region, &["snow", "ice"])
}

/// The terrain types whose ground is snow a foot sinks into (not ice).
#[must_use]
pub fn soft_types(region: &crate::world::Region) -> u32 {
    types_named(region, &["snow"])
}

fn types_named(region: &crate::world::Region, words: &[&str]) -> u32 {
    region
        .terrain_types
        .iter()
        .take(32)
        .enumerate()
        .filter(|(_, t)| {
            let n = t.terrain_name.to_ascii_lowercase();
            words.iter().any(|w| n.contains(w))
        })
        .fold(0, |m, (k, _)| m | (1 << k))
}

/// The weather's state, and its pass.
#[derive(Debug)]
pub struct Weather {
    /// Weather Effects: drawn at all.
    pub on: bool,
    /// What the day sends ([`Self::falls`]).
    pub wanted: Fall,
    /// A stormy day: lightning, and heavier rain.
    pub storm: bool,
    /// How much rain and snow fall now round the eye, 0 to 1, and how wet the ground is.
    pub rain: f32,
    pub snow: f32,
    pub wet: f32,
    /// How much falls now, whatever its form, 0 to 1.
    pub precip: f32,
    /// The terrain types whose ground is snowy ([`snowy_types`]), and of them the soft snow
    /// that takes prints ([`soft_types`]).
    pub snowy_types: u32,
    pub soft_types: u32,
    /// The share of the land round the eye that is snowy, 0 to 1, which the client works out.
    pub snow_share: f32,
    /// How much fresh snow covers the open snowy ground, 0 to 1.
    pub cover: f32,
    /// Seconds of snowfall since the start, each second weighed by how hard it snowed: how far
    /// fresh snow has filled what was trodden.
    pub snowfall: f64,
    /// How full the puddles are, 0 to 1.
    pub puddles: f32,
    /// This frame's lightning, 0 to 1.
    pub flash: f32,
    /// The wind on the drops, metres a second east and north, and how far they have been carried
    /// and have fallen since the start, metres: moved by each frame's step, never jumping.
    pub push: glam::Vec2,
    drift: glam::DVec3,
    /// The flakes' wind, and how far they have been carried and fallen.
    pub snow_push: glam::Vec2,
    snow_drift: glam::DVec3,
    /// A storm's gust now, -1 to 1: how much thicker the rain falls.
    gust: f32,
    last_clock: Option<f64>,
    params: [wgpu::Buffer; 2],
    group: [wgpu::BindGroup; 2],
    pipeline: wgpu::RenderPipeline,
    shelter: wgpu::Texture,
    shelter_view: wgpu::TextureView,
    /// Bound in the map's place on a frame it is not drawn.
    no_shelter: wgpu::TextureView,
}

/// Where the shelter map stands: what stands over the ground round the eye, the depth of the
/// world's static geometry seen straight down (render-hifi's cover map), so a roof or a porch
/// keeps the rain off what stands under it. Centred on the eye and snapped to its texels, so it
/// holds still as the eye walks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shelter {
    /// Its centre, render space.
    pub centre: glam::Vec2,
    /// The height its depth starts at: depth 0 is this high, depth 1 twice [`SHELTER_DEPTH`] below.
    pub top: f32,
}

impl Shelter {
    /// The map round `eye`.
    #[must_use]
    pub fn around(eye: glam::Vec3) -> Self {
        #[allow(clippy::cast_precision_loss)]
        let texel = 2.0 * SHELTER_REACH / SHELTER_SIDE as f32;
        let snap = |v: f32| (v / texel).round() * texel;
        Self {
            centre: glam::Vec2::new(snap(eye.x), snap(eye.y)),
            top: eye.z + SHELTER_DEPTH,
        }
    }

    /// World to the map's clip space: x and y -1 to 1 across it (north up), depth 0 at its top.
    #[must_use]
    pub fn view_proj(&self) -> glam::Mat4 {
        let s = 1.0 / SHELTER_REACH;
        let d = 1.0 / (2.0 * SHELTER_DEPTH);
        glam::Mat4::from_cols(
            glam::Vec4::new(s, 0.0, 0.0, 0.0),
            glam::Vec4::new(0.0, s, 0.0, 0.0),
            glam::Vec4::new(0.0, 0.0, -d, 0.0),
            glam::Vec4::new(-self.centre.x * s, -self.centre.y * s, self.top * d, 1.0),
        )
    }

    /// The height of the map's depth `d` (0 to 1).
    #[must_use]
    pub fn height(&self, d: f32) -> f32 {
        self.top - d * 2.0 * SHELTER_DEPTH
    }

    /// The texel over render-space `p`, if the map reaches it.
    #[must_use]
    pub fn texel(&self, p: glam::Vec2) -> Option<(u32, u32)> {
        let s = (p - self.centre) / SHELTER_REACH;
        if s.x.abs() >= 1.0 || s.y.abs() >= 1.0 {
            return None;
        }
        #[allow(clippy::cast_precision_loss)]
        let side = SHELTER_SIDE as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let at = |v: f32| ((v * side) as u32).min(SHELTER_SIDE - 1);
        Some((at(s.x * 0.5 + 0.5), at(0.5 - s.y * 0.5)))
    }
}

/// A hash of `x`, 0..1.
fn hash(x: u64) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    #[allow(clippy::cast_precision_loss)]
    let v = (h >> 40) as f32 / (1u64 << 24) as f32;
    v
}

/// The lightning at `clock` seconds: a strike every eight to twenty-eight seconds, each two or three
/// flickers over a quarter of a second, 0 to 1.
#[must_use]
pub fn lightning(clock: f64) -> f32 {
    // Strikes fall in windows of twenty-eight seconds, one at a time somewhere in each.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let window = (clock / 28.0).floor().max(0.0) as u64;
    let at = f64::from(hash(window)) * 20.0 + 4.0;
    let t = clock - window as f64 * 28.0 - at;
    if !(0.0..0.35).contains(&t) {
        return 0.0;
    }
    #[allow(clippy::cast_possible_truncation)]
    let t = t as f32;
    let pulses = pyreal_primitives::num::math::sinf(t * 18.0 * std::f32::consts::PI).abs();
    let envelope = (1.0 - t / 0.35).max(0.0);
    pulses * envelope * (0.6 + 0.4 * hash(window ^ 0xABCD))
}

impl Weather {
    /// The pass, drawing into `hdr` with the depth of `depth` format, the frame's group first.
    #[must_use]
    pub fn new(
        gpu: &Gpu,
        frame_layout: &wgpu::BindGroupLayout,
        common: &str,
        hdr: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
    ) -> Self {
        let device = &gpu.device;
        // Rain's and snow's, drawn one after the other in a frame.
        let params = ["rain", "snow"].map(|label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: PARAMS_SIZE,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let map = |label, side| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: SHELTER_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let shelter = map("shelter", SHELTER_SIDE);
        let shelter_view = shelter.create_view(&Default::default());
        let no_shelter = map("no shelter", 1).create_view(&Default::default());
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("precipitation"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let group = params.each_ref().map(|params| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("precipitation"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&shelter_view),
                    },
                ],
            })
        });
        let module = gpu.shader("precipitation", &format!("{common}\n{}", precip_wgsl()));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("precipitation"),
            bind_group_layouts: &[Some(frame_layout), Some(&layout)],
            immediate_size: 0,
        });
        let most = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Max,
        };
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
            label: Some("precipitation"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_precip"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_precip"),
                compilation_options: Default::default(),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: hdr,
                        blend: Some(wgpu::BlendState {
                            color: premultiplied,
                            alpha: premultiplied,
                        }),
                        write_mask: wgpu::ColorWrites::COLOR,
                    }),
                    // The reactive mask keeps the most any drop changed a pixel by.
                    Some(wgpu::ColorTargetState {
                        format: REACTIVE,
                        blend: Some(wgpu::BlendState {
                            color: most,
                            alpha: most,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            on: true,
            wanted: Fall::Nothing,
            storm: false,
            rain: 0.0,
            snow: 0.0,
            wet: 0.0,
            precip: 0.0,
            snowy_types: 0,
            soft_types: 0,
            snow_share: 0.0,
            cover: 0.0,
            snowfall: 0.0,
            puddles: 0.0,
            flash: 0.0,
            push: glam::Vec2::ZERO,
            drift: glam::DVec3::ZERO,
            snow_push: glam::Vec2::ZERO,
            snow_drift: glam::DVec3::ZERO,
            gust: 0.0,
            last_clock: None,
            params,
            group,
            pipeline,
            shelter,
            shelter_view,
            no_shelter,
        }
    }

    /// The shelter map, its view, and the stand-in drawn in its place on a frame it is not.
    #[must_use]
    pub fn shelter_map(&self) -> (&wgpu::Texture, &wgpu::TextureView, &wgpu::TextureView) {
        (&self.shelter, &self.shelter_view, &self.no_shelter)
    }

    /// The shelter map's heights over render-space `points` as drawn at `at`: none where nothing
    /// stands over a point or the map does not reach it (the whole map read back, for tests).
    #[must_use]
    pub fn shelter_heights(
        &self,
        gpu: &Gpu,
        at: &Shelter,
        points: &[glam::Vec2],
    ) -> Vec<Option<f32>> {
        let bytes = crate::read_texture(gpu, &self.shelter, 4);
        let side = SHELTER_SIDE as usize;
        points
            .iter()
            .map(|p| {
                let (x, y) = at.texel(*p)?;
                let i = (y as usize * side + x as usize) * 4;
                let d = f32::from_le_bytes(bytes.get(i..i + 4)?.try_into().ok()?);
                (d < 1.0).then(|| at.height(d))
            })
            .collect()
    }

    /// Whether the shelter map is wanted: something falls, or the ground is wet.
    #[must_use]
    pub fn sheltering(&self) -> bool {
        self.on
            && (self.falling()
                || self.wet > 0.001
                || self.puddles > 0.001
                || (self.cover > 0.001 && self.snow_share > 0.001))
    }

    /// The day asks for `fall`, in a storm or not.
    pub fn falls(&mut self, fall: Fall, storm: bool) {
        self.wanted = fall;
        self.storm = storm;
    }

    /// Eases rain, snow and the ground's wetness toward what the day asks, by the engine's
    /// `clock` (seconds), takes this frame's lightning, and carries the drops on the wind from
    /// `breeze` ([`crate::wind::wind`]).
    pub fn step(&mut self, clock: f64, breeze: glam::Vec3) {
        #[allow(clippy::cast_possible_truncation)]
        let dt = self.last_clock.map_or(1.0, |c| (clock - c).clamp(0.0, 1.0)) as f32;
        let first = self.last_clock.is_none();
        self.last_clock = Some(clock);
        let on = self.on;
        let toward = |v: f32, target: f32, seconds: f32| {
            let step = dt / seconds;
            if v < target {
                (v + step).min(target)
            } else {
                (v - step).max(target)
            }
        };
        let share = self.snow_share.clamp(0.0, 1.0);
        let (precip, snow_part) = match (on, self.wanted) {
            (true, Fall::Rain) => (1.0, 0.0),
            (true, Fall::Snow) => (1.0, 1.0),
            (true, Fall::ByLand) => (1.0, share),
            _ => (0.0, 0.0),
        };
        // A world entered while it falls has it falling from the first frame.
        self.precip = if first {
            precip
        } else {
            toward(self.precip, precip, EASE_SECONDS)
        };
        self.rain = self.precip * (1.0 - snow_part);
        self.snow = self.precip * snow_part;
        self.wet = if self.rain > self.wet {
            toward(self.wet, self.rain, EASE_SECONDS * 3.0)
        } else {
            toward(self.wet, self.rain.max(0.0), DRY_SECONDS)
        };
        self.puddles = if self.rain > 0.001 {
            toward(self.puddles, self.rain, PUDDLE_SECONDS)
        } else {
            toward(self.puddles, 0.0, PUDDLE_DRY_SECONDS)
        };
        // Fresh snow gathers wherever it would snow, the land deciding where it lies; a world
        // entered while it snows is already white.
        let snows = on && matches!(self.wanted, Fall::Snow | Fall::ByLand);
        self.cover = match (snows, first) {
            (true, true) => 1.0,
            (true, false) => toward(self.cover, 1.0, COVER_SECONDS),
            (false, _) => toward(self.cover, 0.0, MELT_SECONDS),
        };
        if snows {
            self.snowfall += f64::from(self.precip * dt);
        }
        if !on {
            self.wet = 0.0;
            self.puddles = 0.0;
            self.cover = 0.0;
        }
        self.flash = if on && self.storm && self.rain > 0.5 {
            lightning(clock) * self.rain
        } else {
            0.0
        };
        let (push, gust) = rain_wind(clock, breeze, self.storminess());
        self.push = push;
        self.gust = gust;
        self.snow_push = snow_wind(clock, breeze, if self.storm { self.precip } else { 0.0 });
        let heavy = 1.0 + 0.6 * self.storminess();
        self.drift += glam::DVec3::new(
            f64::from(push.x),
            f64::from(push.y),
            f64::from(RAIN_SPEED * heavy.sqrt()),
        ) * f64::from(dt);
        self.snow_drift += glam::DVec3::new(
            f64::from(self.snow_push.x),
            f64::from(self.snow_push.y),
            f64::from(SNOW_SPEED),
        ) * f64::from(dt);
    }

    /// The way the rain or snow is driven now, east and north, whichever falls more.
    #[must_use]
    pub fn blown_way(&self) -> Option<glam::Vec2> {
        let push = if self.snow > self.rain {
            self.snow_push
        } else {
            self.push
        };
        push.try_normalize()
    }

    /// A storm's gust now, -1 to 1.
    #[must_use]
    pub fn gust(&self) -> f32 {
        self.gust
    }

    /// How far a storm has come in, 0 to 1: its rain on a stormy day.
    #[must_use]
    pub fn storminess(&self) -> f32 {
        if self.storm {
            self.precip.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Whether anything falls now.
    #[must_use]
    pub fn falling(&self) -> bool {
        self.precip > 0.001
    }

    /// The drops and flakes into the lit picture: the pass's targets are the HDR picture `size`
    /// big and the upscaler's reactive mask, the depth read-only. None fall under the shelter
    /// map's roofs or inside `room`, the one the eye stands in.
    pub fn encode(
        &self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        frame: &wgpu::BindGroup,
        clock: f64,
        size: (u32, u32),
        room: Option<&crate::cells::Volume>,
    ) {
        #[allow(clippy::cast_possible_truncation)]
        let t = (clock % WRAP) as f32;
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (size.0 as f32, size.1 as f32);
        let amount = self.precip.min(1.0);
        if amount <= 0.001 {
            return;
        }
        let storm = self.storminess();
        #[allow(clippy::cast_possible_truncation)]
        let wrap = |d: f64, side: f32| d.rem_euclid(f64::from(side)) as f32;
        let mut draw = |snow: bool, gate: f32| {
            let k = usize::from(snow);
            let heavy = 1.0 + 0.6 * storm;
            let (side, count, near_side, near_count, width, speed, push, drift) = if snow {
                (
                    SNOW_BOX,
                    SNOW_FLAKES,
                    SNOW_NEAR_BOX,
                    SNOW_NEAR_FLAKES,
                    0.012,
                    SNOW_SPEED,
                    self.snow_push,
                    self.snow_drift,
                )
            } else {
                (
                    RAIN_BOX,
                    RAIN_DROPS,
                    1.0,
                    0,
                    0.006,
                    RAIN_SPEED * heavy.sqrt(),
                    self.push,
                    self.drift,
                )
            };
            // A storm's rain is heavier, thickening in its gusts; a blizzard's snow thicker.
            let thicker = if snow {
                1.0 + 1.2 * storm
            } else {
                heavy * heavy * (1.0 + 0.2 * storm * self.gust)
            };
            #[allow(clippy::cast_precision_loss)]
            let drawn = (count + near_count) as f32 * amount * thicker;
            #[allow(clippy::cast_possible_truncation)]
            let words = [
                side,
                count as f32,
                speed,
                if snow { 1.0 } else { 0.0 },
                push.x,
                push.y,
                t,
                amount,
                if snow { 1.0 / 120.0 } else { 1.0 / 30.0 },
                width,
                w,
                h,
                wrap(drift.x, side),
                wrap(drift.y, side),
                drift.z.rem_euclid(f64::from(side) * FALL_WRAP) as f32,
                drawn,
                near_side,
                // Every this many flakes one is in the near box.
                (count + near_count).checked_div(near_count).unwrap_or(0) as f32,
                wrap(drift.x, near_side),
                wrap(drift.y, near_side),
                drift.z.rem_euclid(f64::from(near_side) * FALL_WRAP) as f32,
                gate,
                storm,
                self.gust,
            ];
            let mut bytes: Vec<u8> = words.iter().flat_map(|v| v.to_le_bytes()).collect();
            let planes = room.map_or(&[][..], |r| &r.planes[..r.planes.len().min(ROOM_PLANES)]);
            for k in 0..ROOM_PLANES {
                let p = planes.get(k).copied().unwrap_or(glam::Vec4::ZERO);
                bytes.extend(p.to_array().iter().flat_map(|v| v.to_le_bytes()));
            }
            let n = u32::try_from(planes.len()).unwrap_or(0);
            bytes.extend([n, 0, 0, 0].iter().flat_map(|v| v.to_le_bytes()));
            gpu.queue.write_buffer(&self.params[k], 0, &bytes);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, frame, &[]);
            pass.set_bind_group(1, &self.group[k], &[]);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let n = drawn.ceil() as u32;
            pass.draw(0..6, 0..n.max(1));
        };
        // Gate 0 draws every drop; 1 only rain over land that is not snowy, 2 only snow over land
        // that is.
        match self.wanted {
            Fall::Rain => draw(false, 0.0),
            Fall::Snow => draw(true, 0.0),
            // Fading out after the day's weather changed: as it last fell.
            Fall::ByLand | Fall::Nothing => {
                if self.snow_share < 0.999 || self.snowy_types == 0 {
                    draw(false, 1.0);
                }
                if self.snow_share > 0.001 && self.snowy_types != 0 {
                    draw(true, 2.0);
                }
            }
        }
    }
}

/// How far round the eye the land's snow is counted, metres.
pub const SNOW_REACH: f32 = 288.0;

/// The share of `land`'s vertices within [`SNOW_REACH`] of `eye` whose terrain is one of the
/// snowy types `mask`, each counting less the farther it is.
#[must_use]
pub fn snow_share(land: &crate::land::LandField, eye: glam::Vec3, mask: u32) -> f32 {
    let (mut sum, mut all) = (0.0f32, 0.0f32);
    let (w, h) = land.side;
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let span = |o: f32, n: usize| {
        let lo = ((o - SNOW_REACH) / 24.0).floor().max(0.0) as usize;
        let hi = (((o + SNOW_REACH) / 24.0).ceil().max(0.0) as usize + 1).min(n);
        lo..hi
    };
    let rel = eye.truncate() - land.origin;
    for y in span(rel.y, h) {
        for x in span(rel.x, w) {
            #[allow(clippy::cast_precision_loss)]
            let p = land.origin + glam::Vec2::new(x as f32, y as f32) * 24.0;
            let d = p.distance(eye.truncate());
            if d > SNOW_REACH {
                continue;
            }
            let k = 1.0 - d / SNOW_REACH;
            all += k;
            let t = land.word[y * w + x] & crate::land::bits::TYPE;
            if (mask >> t) & 1 != 0 {
                sum += k;
            }
        }
    }
    if all > 0.0 {
        sum / all
    } else {
        0.0
    }
}

/// [`snow_share`], worked out again once the eye has moved eight metres from where `at` last
/// had it, or a second's frames on (the land streams in round a still eye).
pub fn share_near(
    land: &crate::land::LandField,
    eye: glam::Vec3,
    mask: u32,
    at: &mut Option<(glam::Vec3, u32, f32)>,
) -> f32 {
    match at {
        Some((p, frames, share)) if p.distance(eye) < 8.0 && *frames < 60 => {
            *frames += 1;
            *share
        }
        _ => {
            let share = snow_share(land, eye, mask);
            *at = Some((eye, 0, share));
            share
        }
    }
}

/// The wind on the drops at `clock` seconds, metres a second east and north, from the breeze
/// `breeze` ([`crate::wind::wind`]) as far as a storm (`storm`, 0 to 1) has come in, and the
/// storm's gust, -1 to 1. A light rain drifts with the breeze. A storm's gale keeps the breeze's
/// heading, wandering a few degrees over minutes, at a steady strength that swells a little: its
/// gusts thicken the rain rather than turn it, so the drops never swing about.
#[must_use]
pub fn rain_wind(clock: f64, breeze: glam::Vec3, storm: f32) -> (glam::Vec2, f32) {
    use pyreal_primitives::num::math::{atan2f, cosf, sinf};
    #[allow(clippy::cast_possible_truncation)]
    let t = (clock % 100_000.0) as f32;
    let calm = glam::Vec2::new(breeze.x, breeze.y) * RAIN_DRIFT;
    let heading =
        atan2f(breeze.y, breeze.x) + 0.08 * sinf(t * 0.021) + 0.04 * sinf(t * 0.053 + 2.0);
    let swell = 0.6 * sinf(t * 0.11 + 0.7) + 0.4 * sinf(t * 0.047 + 1.9);
    let gale = glam::Vec2::new(cosf(heading), sinf(heading)) * STORM_PUSH * (1.0 + 0.12 * swell);
    let gust = 0.6 * sinf(t * 0.37 + 0.3) + 0.4 * sinf(t * 0.83 + 1.1);
    (calm.lerp(gale, storm.clamp(0.0, 1.0)), gust)
}

/// The wind on the flakes at `clock` seconds, metres a second east and north: they drift with
/// the breeze `breeze` far more than rain does, and a blizzard (`blizzard`, 0 to 1) drives them
/// near flat, its heading wandering and its strength swelling.
#[must_use]
pub fn snow_wind(clock: f64, breeze: glam::Vec3, blizzard: f32) -> glam::Vec2 {
    use pyreal_primitives::num::math::{atan2f, cosf, sinf};
    #[allow(clippy::cast_possible_truncation)]
    let t = (clock % 100_000.0) as f32;
    let calm = glam::Vec2::new(breeze.x, breeze.y) * 1.6;
    let heading =
        atan2f(breeze.y, breeze.x) + 0.12 * sinf(t * 0.031) + 0.06 * sinf(t * 0.077 + 1.3);
    let swell = 0.6 * sinf(t * 0.13 + 0.4) + 0.4 * sinf(t * 0.059 + 2.3);
    let gale = glam::Vec2::new(cosf(heading), sinf(heading)) * BLIZZARD_PUSH * (1.0 + 0.15 * swell);
    calm.lerp(gale, blizzard.clamp(0.0, 1.0))
}

/// The rain and snow's shader, after the shelter map's and the wet ground's helpers.
#[must_use]
pub fn precip_wgsl() -> String {
    format!(
        "{}\n{}",
        include_str!("shaders/rain.wgsl"),
        include_str!("shaders/precip.wgsl")
    )
}
