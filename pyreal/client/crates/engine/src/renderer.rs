//! The passes and their pipelines: the ground's depth prepass and pyramid, the cull pass, the
//! G-buffer, deferred lighting, translucent surfaces drawn forward, and the tone map, run through
//! the [`FrameGraph`].

use glam::Mat4;

use crate::camera::Camera;
use crate::gpu::Gpu;
use crate::graph::{FrameGraph, GraphDecl, PassDecl, PassKind, PassTiming};
use crate::materials::MaterialTable;
use crate::scene::{PipelineKind, Scene, ARGS_SIZE};
use crate::sky::SkyLight;
use crate::EngineError;

pub const ALBEDO: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
pub const NORMAL: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;
pub const MATERIAL: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const MOTION: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;
pub const IDS: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;
pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The order-independent translucency's weighted sum and revealage.
pub const OIT_ACCUM: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const OIT_REVEAL: wgpu::TextureFormat = wgpu::TextureFormat::R16Float;

/// A target drawn into and read, `width` by `height`.
fn plain_target(
    gpu: &Gpu,
    label: &str,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

const COMMON: &str = concat!(
    include_str!("shaders/common.wgsl"),
    "\n",
    include_str!("shaders/land.wgsl"),
    "\n",
    include_str!("shaders/air_model.wgsl"),
    "\n",
    include_str!("shaders/air_sky.wgsl")
);
const SCENE: &str = include_str!("shaders/scene.wgsl");
const MATERIALS: &str = include_str!("shaders/materials.wgsl");
const FULLSCREEN: &str = include_str!("shaders/fullscreen.wgsl");

/// The WGSL of each module, assembled from the shared files.
pub mod source {
    use super::{COMMON, FULLSCREEN, MATERIALS, SCENE};

    #[must_use]
    pub fn hiz_first() -> String {
        format!(
            "{}\n{}",
            include_str!("shaders/hiz.wgsl"),
            include_str!("shaders/hiz_first.wgsl")
        )
    }

    #[must_use]
    pub fn hiz_down() -> String {
        format!(
            "{}\n{}",
            include_str!("shaders/hiz.wgsl"),
            include_str!("shaders/hiz_down.wgsl")
        )
    }

    #[must_use]
    pub fn far() -> String {
        crate::far::source()
    }

    #[must_use]
    pub fn cull() -> String {
        include_str!("shaders/cull.wgsl").to_owned()
    }

    #[must_use]
    pub fn gbuffer() -> String {
        format!(
            "enable wgpu_binding_array;\n{COMMON}\n{SCENE}\n{MATERIALS}\n{}",
            include_str!("shaders/gbuffer.wgsl")
        )
    }

    #[must_use]
    pub fn shadow_cast() -> String {
        format!(
            "{}\n{}",
            gbuffer(),
            include_str!("shaders/shadow_cast.wgsl")
        )
    }

    #[must_use]
    pub fn forward() -> String {
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            gbuffer(),
            include_str!("shaders/sun_shadow.wgsl"),
            include_str!("shaders/lamp_types.wgsl"),
            include_str!("shaders/lamp_light.wgsl"),
            include_str!("shaders/probe_types.wgsl"),
            include_str!("shaders/gi_light.wgsl"),
            include_str!("shaders/forward.wgsl"),
            include_str!("shaders/particles.wgsl"),
            include_str!("shaders/webs.wgsl"),
            include_str!("shaders/flames.wgsl"),
            include_str!("shaders/body_fire.wgsl"),
            include_str!("shaders/blobs.wgsl"),
            include_str!("shaders/puffs.wgsl"),
            include_str!("shaders/smoke.wgsl"),
            include_str!("shaders/bolts.wgsl")
        )
    }

    #[must_use]
    pub fn oit() -> String {
        format!(
            "{COMMON}\n{FULLSCREEN}\n{}",
            include_str!("shaders/oit.wgsl")
        )
    }

    #[must_use]
    pub fn lighting() -> String {
        format!(
            "enable wgpu_binding_array;\n{COMMON}\n{FULLSCREEN}\n{MATERIALS}\n{}\n{}\n{}\n{}\n{}\n{}",
            include_str!("shaders/heavens.wgsl"),
            include_str!("shaders/rain.wgsl"),
            include_str!("shaders/lighting.wgsl"),
            include_str!("shaders/sun_shadow.wgsl"),
            include_str!("shaders/lamp_types.wgsl"),
            include_str!("shaders/lamp_light.wgsl")
        ) + "\n"
            + include_str!("shaders/probe_types.wgsl")
            + "\n"
            + include_str!("shaders/gi_light.wgsl")
    }

    #[must_use]
    pub fn clusters() -> String {
        format!(
            "{COMMON}\n{}\n{}",
            include_str!("shaders/lamp_types.wgsl"),
            include_str!("shaders/clusters.wgsl")
        )
    }

    #[must_use]
    pub fn lamp_rays() -> String {
        format!(
            "enable wgpu_ray_query;\n{COMMON}\n{}\n{}",
            include_str!("shaders/lamp_types.wgsl"),
            include_str!("shaders/lamp_rays.wgsl")
        )
    }

    #[must_use]
    pub fn clouds() -> String {
        format!(
            "enable wgpu_binding_array;\n{COMMON}\n{MATERIALS}\n{}\n{}",
            include_str!("shaders/clouds.wgsl"),
            include_str!("shaders/cloud_shape.wgsl")
        )
    }

    #[must_use]
    pub fn cloud_resolve() -> String {
        format!("{COMMON}\n{}", include_str!("shaders/cloud_resolve.wgsl"))
    }

    #[must_use]
    pub fn gtao() -> String {
        format!("{COMMON}\n{}", include_str!("shaders/gtao.wgsl"))
    }

    #[must_use]
    pub fn ao_filter() -> String {
        format!("{COMMON}\n{}", include_str!("shaders/ao_filter.wgsl"))
    }

    #[must_use]
    pub fn ao_blur() -> String {
        format!("{COMMON}\n{}", include_str!("shaders/ao_blur.wgsl"))
    }

    #[must_use]
    pub fn probes() -> String {
        format!(
            "enable wgpu_ray_query;\nenable wgpu_binding_array;\n{COMMON}\n{MATERIALS}\n{}\n{}\n{}",
            include_str!("shaders/lamp_types.wgsl"),
            include_str!("shaders/probe_types.wgsl"),
            include_str!("shaders/probes.wgsl")
        )
    }

    #[must_use]
    pub fn air_tables() -> String {
        format!(
            "{}\n{}",
            include_str!("shaders/air_model.wgsl"),
            include_str!("shaders/air_tables.wgsl")
        )
    }

    #[must_use]
    pub fn exposure() -> String {
        format!("{COMMON}\n{}", include_str!("shaders/exposure.wgsl"))
    }

    #[must_use]
    pub fn bloom() -> String {
        include_str!("shaders/bloom.wgsl").to_owned()
    }

    /// The air's froxels and their apply pass.
    #[must_use]
    pub fn air_froxels() -> String {
        format!("{COMMON}\n{}", crate::air::sources()[0])
    }

    #[must_use]
    pub fn air_apply() -> String {
        format!("{COMMON}\n{FULLSCREEN}\n{}", crate::air::sources()[1])
    }

    /// The rain and snow.
    #[must_use]
    pub fn precipitation() -> String {
        format!("{COMMON}\n{}", crate::weather::precip_wgsl())
    }

    #[must_use]
    pub fn water() -> String {
        format!(
            "enable wgpu_binding_array;\n{COMMON}\n{FULLSCREEN}\n{MATERIALS}\n{}\n{}\n{}\n{}\n{}\n{}",
            include_str!("shaders/heavens.wgsl"),
            include_str!("shaders/sun_shadow.wgsl"),
            include_str!("shaders/probe_types.wgsl"),
            include_str!("shaders/gi_light.wgsl"),
            include_str!("shaders/rain.wgsl"),
            include_str!("shaders/water.wgsl")
        )
    }

    /// The water's depth laid into the world's.
    #[must_use]
    pub fn water_merge() -> String {
        format!("{FULLSCREEN}\n{}", include_str!("shaders/water_merge.wgsl"))
    }

    #[must_use]
    pub fn cover() -> String {
        format!(
            "{COMMON}\n{}\n{}\n{}\n{}",
            include_str!("shaders/cover_types.wgsl"),
            include_str!("shaders/cover.wgsl"),
            include_str!("shaders/blades.wgsl"),
            include_str!("shaders/scatter.wgsl")
        )
    }

    #[must_use]
    pub fn cover_draw() -> String {
        format!(
            "{}\n{}\n{}\n{}",
            gbuffer(),
            include_str!("shaders/cover_types.wgsl"),
            include_str!("shaders/cover_draw.wgsl"),
            include_str!("shaders/scatter_draw.wgsl")
        )
    }

    #[must_use]
    pub fn water_sim() -> String {
        format!(
            "{COMMON}\n{}\n{}",
            crate::water::kernel_wgsl(),
            include_str!("shaders/water_sim.wgsl")
        )
    }

    #[must_use]
    pub fn tonemap() -> String {
        format!(
            "{COMMON}\n{FULLSCREEN}\n{}",
            include_str!("shaders/tonemap.wgsl")
        )
    }
}

/// The frame's passes, in order, with what each reads and writes.
#[must_use]
pub fn graph_decl() -> GraphDecl {
    GraphDecl {
        passes: vec![
            PassDecl::new("atmosphere", PassKind::Compute).writes(&["sky-tables"]),
            PassDecl::new("prepass", PassKind::Render)
                .reads(&["instances", "vertices", "far-terrain"])
                .writes(&["ground-depth"]),
            PassDecl::new("hiz", PassKind::Compute)
                .reads(&["ground-depth"])
                .writes(&["hiz"]),
            PassDecl::new("cull", PassKind::Compute)
                .reads(&["instances", "meshes", "hiz"])
                .writes(&["draw-args", "visible"]),
            PassDecl::new("cover", PassKind::Compute)
                .reads(&["land"])
                .writes(&["cover-draws"]),
            PassDecl::new("gbuffer", PassKind::Render)
                .reads(&[
                    "draw-args",
                    "cover-draws",
                    "visible",
                    "instances",
                    "vertices",
                    "materials",
                    "far-terrain",
                ])
                .writes(&[
                    "albedo", "normal", "surface", "motion", "ids", "objects", "depth",
                ]),
            PassDecl::new("shadow-cull", PassKind::Compute)
                .reads(&["instances", "meshes"])
                .writes(&["shadow-args", "shadow-visible"]),
            PassDecl::new("shadows", PassKind::Render)
                .reads(&[
                    "shadow-args",
                    "shadow-visible",
                    "instances",
                    "vertices",
                    "materials",
                ])
                .writes(&["shadow-atlas"]),
            PassDecl::new("shelter", PassKind::Render)
                .reads(&[
                    "shadow-args",
                    "shadow-visible",
                    "instances",
                    "vertices",
                    "materials",
                ])
                .writes(&["shelter"]),
            PassDecl::new("trails", PassKind::Render).writes(&["trails"]),
            PassDecl::new("clusters", PassKind::Compute)
                .reads(&["lamps"])
                .writes(&["clusters"]),
            PassDecl::new("lamp-shadows", PassKind::Compute)
                .reads(&["depth", "normal", "lamps", "rays"])
                .writes(&["lamp-shadows"]),
            PassDecl::new("gtao", PassKind::Compute)
                .reads(&["depth"])
                .writes(&["ao-raw"]),
            PassDecl::new("ao gather", PassKind::Compute)
                .reads(&["ao-raw", "motion", "depth"])
                .writes(&["ao-gathered"]),
            PassDecl::new("ao blur", PassKind::Compute)
                .reads(&["ao-gathered"])
                .writes(&["ao"]),
            PassDecl::new("probes", PassKind::Compute)
                .reads(&["rays", "lamps", "materials", "vertices"])
                .writes(&["probes"]),
            PassDecl::new("ripples", PassKind::Compute)
                .reads(&["land"])
                .writes(&["ripples"]),
            PassDecl::new("clouds", PassKind::Compute)
                .reads(&["depth", "sky-tables", "materials"])
                .writes(&["clouds"]),
            PassDecl::new("lighting", PassKind::Render)
                .reads(&[
                    "albedo",
                    "normal",
                    "surface",
                    "depth",
                    "sky-tables",
                    "materials",
                    "shadow-atlas",
                    "lamps",
                    "clusters",
                    "lamp-shadows",
                    "ao",
                    "probes",
                    "land",
                    "clouds",
                    "shelter",
                    "trails",
                ])
                .writes(&["hdr"]),
            PassDecl::new("water depth", PassKind::Render)
                .reads(&["depth", "surface", "ids", "land"])
                .writes(&["water-depth", "motion"]),
            PassDecl::new("water", PassKind::Render)
                .reads(&[
                    "water-depth",
                    "hdr",
                    "depth",
                    "surface",
                    "ripples",
                    "land",
                    "sky-tables",
                    "shadow-atlas",
                    "probes",
                    "materials",
                ])
                .writes(&["hdr", "reactive"]),
            PassDecl::new("water merge", PassKind::Render)
                .reads(&["water-depth"])
                .writes(&["depth"]),
            PassDecl::new("puddles", PassKind::Render)
                .reads(&["hdr", "depth", "land", "shelter", "sky-tables", "probes"])
                .writes(&["hdr"]),
            PassDecl::new("ice", PassKind::Render)
                .reads(&[
                    "draw-args",
                    "visible",
                    "hdr",
                    "depth",
                    "materials",
                    "sky-tables",
                    "shadow-atlas",
                    "probes",
                ])
                .writes(&["hdr", "depth"]),
            PassDecl::new("translucent", PassKind::Render)
                .reads(&[
                    "draw-args",
                    "visible",
                    "depth",
                    "hdr",
                    "materials",
                    "sky-tables",
                    "particles",
                    "shadow-atlas",
                    "lamps",
                    "clusters",
                    "probes",
                    "reactive",
                ])
                .writes(&["hdr", "oit", "reactive"]),
            PassDecl::new("oit", PassKind::Render)
                .reads(&["oit", "hdr"])
                .writes(&["hdr"]),
            PassDecl::new("weather", PassKind::Render)
                .reads(&["depth", "hdr", "land", "shelter", "reactive"])
                .writes(&["hdr", "reactive"]),
            PassDecl::new("air", PassKind::Compute)
                .reads(&["shadow-atlas"])
                .writes(&["air"]),
            PassDecl::new("air apply", PassKind::Render)
                .reads(&["air", "depth", "hdr"])
                .writes(&["hdr"]),
            PassDecl::new("exposure", PassKind::Compute)
                .reads(&["hdr"])
                .writes(&["exposure"]),
            PassDecl::new("bloom", PassKind::Compute)
                .reads(&["hdr", "depth"])
                .writes(&["bloom"]),
            PassDecl::new("tonemap", PassKind::Render)
                .reads(&["hdr", "bloom", "exposure", "depth", "objects"])
                .writes(&["ldr"]),
            PassDecl::new("world marks", PassKind::Render)
                .reads(&["depth", "ui"])
                .writes(&["ui"]),
            PassDecl::new("resolve", PassKind::Render)
                .reads(&["ldr", "depth", "motion", "ids", "reactive"])
                .writes(&["picture"]),
            PassDecl::new("composite", PassKind::Render)
                .reads(&["picture", "ui"])
                .writes(&["output"]),
        ],
        imported: vec![
            "instances",
            "meshes",
            "vertices",
            "materials",
            "far-terrain",
            "lamps",
            "rays",
            "land",
            "ui",
            "particles",
        ],
        exported: vec!["motion", "ids", "output"],
    }
}

/// The curve the HDR image is tone mapped through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToneCurve {
    /// Khronos PBR Neutral: the low and middle range as they are, the highlights rolled off.
    #[default]
    Neutral,
    /// ACES, render-hifi's fit: filmic, with a toe that deepens the dark.
    Aces,
    /// AgX: filmic, keeping hue as highlights go to white.
    Agx,
}

impl ToneCurve {
    /// The frame uniform's word for it.
    #[must_use]
    pub fn word(self) -> f32 {
        match self {
            Self::Neutral => 0.0,
            Self::Aces => 1.0,
            Self::Agx => 2.0,
        }
    }
}

pub use crate::post::{Calibration, Exposure};

/// The light rooms closed to the sky take, lux per unit of the client's indoor light (its 0.2
/// ambient and the lamps' stand-in): a lamp-lit room's few hundred lux.
pub const ROOM_LUX: f32 = 400.0;

/// What one frame looks at: the camera, the light, and whether the portal walk reached the
/// outside (the sky is drawn only then).
#[derive(Debug, Clone, Copy)]
pub struct View<'a> {
    pub camera: &'a Camera,
    pub light: &'a SkyLight,
    pub outside: bool,
    /// The flat light rooms take (the sun never reaches them): see [`indoor_light`].
    pub indoor: glam::Vec3,
    /// The same in scene units, for the physical calibration: see [`indoor_physical`].
    pub indoor_physical: glam::Vec3,
    /// Whether the eye is in a room sealed from the sky: the exposure is held to
    /// [`crate::post::ROOM_EXPOSURE`] there.
    pub sealed: bool,
    /// The far terrain, drawn after the instances when the outside is seen.
    pub far: Option<&'a crate::far::FarTerrain>,
    /// Each cell slot's screen rectangle this frame.
    pub rects: &'a [crate::cells::Rect],
    /// The moons and the cloud deck, when the region's sky objects are loaded.
    pub heavens: Option<&'a crate::heavens::HeavensNow>,
    /// The lamps that can reach the view, nearest first ([`crate::lamps::frame_lamps`]).
    pub lamps: &'a [crate::lamps::Lamp],
    /// The lamps that light the probes' rays ([`crate::lamps::probe_lamps`]).
    pub probe_lamps: &'a [crate::lamps::Lamp],
    /// The flames seen, nearest first ([`crate::flames::frame_flames`]).
    pub flames: &'a [crate::flames::Flame],
    /// Where the bodies near the eye stand (their feet): they stir the water.
    pub bodies: &'a [glam::Vec3],
    /// How far the waves lift the water over its level where each of `bodies` stands.
    pub body_water: &'a [f32],
    /// Where the shadows stand (their feet), nearest first: each pools darkness round it.
    pub umbrae: &'a [glam::Vec3],
    /// What the objects standing on the land clear of cover (x, y and radius).
    pub clearings: &'a [glam::Vec3],
    /// This frame's particles, sorted and batched ([`crate::particles::Particles::collect`]).
    pub particles: Option<&'a crate::particles::Particles>,
    /// The interface's picture, when it has triangles standing in the world to draw against the
    /// world's depth.
    pub overlay: Option<&'a crate::overlay::Overlay>,
    /// The room the eye stands in, when it does: no rain falls in it.
    pub eye_room: Option<&'a crate::cells::Volume>,
}

/// What a distance is multiplied by before the degrade switches: this view's screen-size scale
/// against [`DEGRADE_FOV_Y`]'s, over `bias` (zero when `bias` keeps every near level).
#[must_use]
pub fn lod_scale(camera: &Camera, bias: f32) -> f32 {
    if bias > 0.0 {
        pyreal_primitives::num::math::tanf(camera.fov_y * 0.5)
            / pyreal_primitives::num::math::tanf(DEGRADE_FOV_Y.to_radians() * 0.5)
            / bias
    } else {
        0.0
    }
}

/// The world ambient a room is lit by with no lamp near: the client's 0.2 white.
pub const INDOOR_AMBIENT: f32 = 0.2;
/// What stands in for the lamps' light burned into rooms when the lamps are not lit per pixel.
pub const LAMP_STAND_IN: f32 = 0.4;

/// The light rooms take: the client's world ambient, which is the outdoor objects' ambient for an
/// eye outdoors or in a room open to the sky and a dim white for any other, and the lamps'
/// stand-in there when the lamps are not lit per pixel (`lamps_lit` false).
#[must_use]
pub fn indoor_light(light: &SkyLight, open_to_sky: bool, lamps_lit: bool) -> glam::Vec3 {
    if open_to_sky {
        light.ambient + light.sun_strength * 0.2
    } else if lamps_lit {
        glam::Vec3::splat(INDOOR_AMBIENT)
    } else {
        glam::Vec3::splat(INDOOR_AMBIENT + LAMP_STAND_IN)
    }
}

/// [`indoor_light`] in scene units: a room open to the sky takes the sky's scale, any other
/// [`ROOM_LUX`].
#[must_use]
pub fn indoor_physical(light: &SkyLight, open_to_sky: bool, lamps_lit: bool) -> glam::Vec3 {
    let retail = indoor_light(light, open_to_sky, lamps_lit);
    let lin = |v: f32| pyreal_primitives::num::math::powf(v.clamp(0.0, 1.0), 2.2);
    let lin = glam::Vec3::new(lin(retail.x), lin(retail.y), lin(retail.z));
    if open_to_sky {
        lin * crate::sky::sky_lux(light.daylight) / crate::sky::SCENE_LUX
    } else {
        lin * ROOM_LUX / crate::sky::SCENE_LUX
    }
}

/// The frame uniform's bytes.
pub const FRAME_SIZE: u64 = 1216;
/// The most shadows' pools of darkness a frame lays.
pub const UMBRAE: usize = 8;
/// A shadow's pool of darkness's radius, metres.
pub const UMBRA_RADIUS: f32 = 0.9;
/// The light clusters: a tile's side in target pixels, the tiles the buffers first hold (a
/// 4096x2304 target; a larger one grows them), and the depth slices out to the lamps' reach.
pub const CLUSTER_TILE: u32 = 64;
pub const CLUSTER_TILES: (u32, u32) = (64, 36);
pub const CLUSTER_SLICES: u32 = 24;

/// The cluster tiles across and down a `width` by `height` target: every pixel in one.
#[must_use]
pub fn cluster_tiles_for(width: u32, height: u32) -> (u32, u32) {
    (
        width.div_ceil(CLUSTER_TILE).max(1),
        height.div_ceil(CLUSTER_TILE).max(1),
    )
}

/// The cluster buffers' bytes for `tiles` tiles: a count and the lamp list of each cluster.
#[must_use]
pub fn cluster_bytes(tiles: u32) -> (u64, u64) {
    let clusters = u64::from(tiles) * u64::from(CLUSTER_SLICES);
    (clusters * 4, clusters * 4 * CLUSTER_MOST)
}
/// The most lamps a cluster holds (`CLUSTER_MOST` in the shaders).
pub const CLUSTER_MOST: u64 = 48;
/// Where the clusters' first slice ends, metres.
pub const CLUSTER_NEAR: f32 = 0.5;
/// How near the eye a lamp's light must reach for its shadows to be ray traced, metres.
pub const RAY_SHADOW_REACH: f32 = 20.0;
/// The bounce light's probes round the eye: across, along and up, and their spacing in metres.
pub const PROBE_DIMS: (u32, u32, u32) = (32, 32, 8);
pub const PROBE_SPACING: (f32, f32, f32) = (4.0, 4.0, 3.0);
/// The probes traced each frame once the grid has settled: one in this many.
pub const PROBE_SHARE: u32 = 4;
/// The probes round the eye traced every frame (`NEAR` in the shader).
const PROBE_NEAR: u32 = 12 * 12 * 4;
/// The least of a new trace a probe takes, once its first traces are averaged.
pub const PROBE_BLEND: f32 = 0.05;
/// The most the time of day moves in a frame as it runs (a hundredth of a day); past it the
/// time was set (`@tod`, `@day`) and the eye and the bounce light start again.
pub const TIME_CUT: f32 = 0.01;
/// The frames the eye takes the new light at once after the time is set, while the bounce
/// light's first traces settle.
pub const TIME_CUT_FRAMES: u32 = 12;
/// A probe's bytes (`Probe` in the shaders).
const PROBE_BYTES: u64 = 80;
/// Where the frame uniform's feature flags (`switches.w`) lie, bytes.
const FRAME_WORD_FLAGS: u64 = 316;

/// The frame uniform's feature flags (`switches.w`).
pub mod flags {
    pub const PHYSICAL: u32 = 1;
    pub const SHADOWS: u32 = 2;
    pub const LAMPS: u32 = 4;
    pub const AO: u32 = 8;
    pub const GI: u32 = 16;
    pub const ATMOSPHERE: u32 = 32;
    pub const LAMP_RAYS: u32 = 64;
    pub const WATER: u32 = 128;
    pub const COVER: u32 = 256;
    pub const RELIEF: u32 = 512;
    pub const HEIGHT_BLEND: u32 = 1024;
    pub const TRIPLANAR: u32 = 2048;
    pub const DETAIL: u32 = 4096;
    pub const MACRO: u32 = 8192;
    pub const WEBS: u32 = 16384;
}

/// The surfaces' depth, each on its own so its cost and its look can be told
/// apart; drawn under the physical calibration only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Surfaces {
    /// Depth mapping's level, 0 (Off) to [`RELIEF_HIGH`]: a row of [`RELIEF_LEVELS`].
    pub relief: u8,
    /// Ground layers blended by their heights, where they have them.
    pub height_blend: bool,
    /// Steep ground's pictures projected along the slope.
    pub triplanar: bool,
    /// Fine bends on the ground close to the eye.
    pub detail: bool,
    /// Tint and a second, turned placement of the ground's pictures across large areas.
    pub macro_variation: bool,
}

impl Surfaces {
    pub const ALL: Self = Self {
        relief: RELIEF_HIGH,
        height_blend: true,
        triplanar: true,
        detail: true,
        macro_variation: true,
    };
    pub const NONE: Self = Self {
        relief: 0,
        height_blend: false,
        triplanar: false,
        detail: false,
        macro_variation: false,
    };

    /// The frame's feature flags these turn on.
    #[must_use]
    pub fn flags(self) -> u32 {
        [
            (self.relief_level()[0] > 0.0, flags::RELIEF),
            (self.height_blend, flags::HEIGHT_BLEND),
            (self.triplanar, flags::TRIPLANAR),
            (self.detail, flags::DETAIL),
            (self.macro_variation, flags::MACRO),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .fold(0, |f, (_, bit)| f | bit)
    }

    /// What [`Self::relief`] draws, a row of [`RELIEF_LEVELS`].
    #[must_use]
    pub fn relief_level(self) -> [f32; 4] {
        RELIEF_LEVELS[usize::from(self.relief).min(RELIEF_LEVELS.len() - 1)]
    }
}

impl Default for Surfaces {
    fn default() -> Self {
        Self::ALL
    }
}

/// Steep ground takes its pictures stood up along the slope from this up-facing (a triangle's
/// normal's height), and wholly by `TRIPLANAR_WHOLE` (`gbuffer.wgsl` keeps the same).
pub const TRIPLANAR_FROM: f32 = 0.8;
pub const TRIPLANAR_WHOLE: f32 = 0.6;
/// Depth mapping's levels (render-hifi's, ported), Off to High: how deep a material's height
/// reaches, metres (none: no parallax and no self-shadow); the share of its steps searched; how
/// strongly a pack material's normal bends a retail picture's surface; how far off it shows, metres.
pub const RELIEF_LEVELS: [[f32; 4]; 4] = [
    [0.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 0.7, 0.0],
    [0.06, 0.5, 0.85, 20.0],
    [0.12, 1.0, 1.0, 35.0],
];
/// Depth mapping's greatest level.
pub const RELIEF_HIGH: u8 = 3;
/// Where the ground's close detail has faded out, metres from the eye.
pub const DETAIL_REACH: f32 = 18.0;
/// The lamps whose light a surface's relief shades: the nearest this many.
pub const KEY_LAMPS: usize = 4;
/// The cull parameters' bytes: six planes, the view-projection, the instance count, the
/// pyramid's description, the degrade scale and the eye.
pub const CULL_PARAMS_SIZE: u64 = 224;
/// How much farther each cascade's casters seem to the degrade than to the eye: the shadow of a
/// coarser level is as good, and the cascades hold far more of the world than the view.
pub const CASCADE_DEGRADE: [f32; 4] = [3.0, 6.0, 12.0, 24.0];

/// How much farther cascade `c`'s casters seem to the degrade at the degrade bias `bias`: past 1
/// the casters' distances grow as its square root, not with it, as the eye's do.
#[must_use]
pub fn caster_degrade(c: usize, bias: f32) -> f32 {
    CASCADE_DEGRADE[c.min(CASCADE_DEGRADE.len() - 1)] * bias.max(1.0).sqrt()
}
/// The stride of the cascades' cull parameters in their buffer.
const SHADOW_PARAMS_STRIDE: u64 = 256;

/// The vertical field of view the dats' degrade distances are kept at: the client's 90 degrees
/// across a 16:9 view. A narrower view keeps detail further out, as screen size asks.
pub const DEGRADE_FOV_Y: f32 = 58.7;

/// What is highlighted, by object number: the selected object (outlined, with its target ring
/// round its feet) and the hovered one (outlined).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Selection {
    pub selected: Option<u32>,
    pub hovered: Option<u32>,
    /// The target ring under the selected object: where its feet are and its radius, metres.
    pub ring: Option<(glam::Vec3, f32)>,
    /// The ring's and the selected outline's colour, packed `0xAABBGGRR`; zero for the default.
    pub colour: u32,
}

/// What the cull pass kept in the last frame read back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CullStats {
    pub visible_instances: u32,
    pub visible_triangles: u32,
    /// Instances in the frustum and their cell's rectangle that the ground hid.
    pub occluded: u32,
    /// Instances drawn at a coarser degrade level than their near one.
    pub degraded: u32,
}

/// The render targets of one size.
#[derive(Debug)]
struct Targets {
    width: u32,
    height: u32,
    views: [wgpu::TextureView; 8],
    lighting_group: wgpu::BindGroup,
    tonemap_group: wgpu::BindGroup,
    /// The depth pyramid: its level-0 size, its levels, one bind group per level it is built in,
    /// and the cull pass's view of the whole.
    hiz_size: (u32, u32),
    hiz_levels: u32,
    hiz_groups: Vec<wgpu::BindGroup>,
    cull_hiz_group: wgpu::BindGroup,
    /// The lighting's lights: the sun's atlas, the lamps, their clusters and ray-traced shadows.
    light_group: wgpu::BindGroup,
    /// The lamps' ray-traced shadows, half size.
    lamp_shadow: wgpu::TextureView,
    /// The ambient occlusion's search, half size.
    gtao_group: wgpu::BindGroup,
    /// Its gathering over the frames and its blur, by which history is last frame's.
    ao_gather_groups: [wgpu::BindGroup; 2],
    ao_blur_groups: [wgpu::BindGroup; 2],
    /// The volumetric clouds' march into their picture, two thirds of the frame's size.
    clouds_group: wgpu::BindGroup,
    /// Their march laid into their history, and the result kept for the next frame.
    clouds_resolve_group: wgpu::BindGroup,
    clouds_keep_group: wgpu::BindGroup,
    clouds_size: (u32, u32),
    /// The lit frame before the water, which the water reads, and the water's bind group.
    scene: wgpu::Texture,
    water_groups: [wgpu::BindGroup; 2],
    /// The same for the water's depth pass, which draws into the depth the others read.
    water_depth_groups: [wgpu::BindGroup; 2],
    /// The water's own depth, then the world's copied before the ice writes its own, and the
    /// ice's lights.
    depth_behind: wgpu::Texture,
    /// Its view, and the group the water merge reads it by.
    behind_view: wgpu::TextureView,
    water_merge_group: wgpu::BindGroup,
    ice_group: wgpu::BindGroup,
    /// The order-independent translucency's sum and revealage, and the pass laying it over.
    oit_views: [wgpu::TextureView; 3],
    oit_group: wgpu::BindGroup,
    /// The tone-mapped picture, last frame's materials, and MetalFX's reactive mask (none).
    ldr: wgpu::Texture,
    prev_ids: wgpu::Texture,
    reactive: wgpu::Texture,
    reactive_view: wgpu::TextureView,
    // Kept alive with their views.
    textures: Vec<wgpu::Texture>,
}

const T_ALBEDO: usize = 0;
const T_NORMAL: usize = 1;
const T_SURFACE: usize = 2;
const T_MOTION: usize = 3;
const T_IDS: usize = 4;
const T_DEPTH: usize = 5;
const T_HDR: usize = 6;
/// Each pixel's object plus one (0 none), `IDS`'s format.
const T_OBJECTS: usize = 7;

/// Everything the passes need besides the scene.
#[derive(Debug)]
pub struct Renderer {
    pub output_format: wgpu::TextureFormat,
    /// The game clock, seconds: twinkles, drifting clouds and flickering flames follow it.
    pub clock: f64,
    /// How the light is scaled: physical, or the client's own.
    pub calibration: Calibration,
    /// What the HDR image is multiplied by before the tone curve; the retail calibration fixes
    /// it at 1 unless it is given.
    pub exposure: Exposure,
    /// The bloom's strength (none under the retail calibration).
    pub bloom: f32,
    /// The seconds between frames the eye adapts by; measured when `None`.
    pub frame_seconds: Option<f32>,
    last_frame: Option<std::time::Instant>,
    /// The clock last frame, for the wind's motion.
    last_clock: Option<f64>,
    /// The time of day last frame, to see it set.
    last_time: Option<f32>,
    /// The frames left that the exposure snaps to its target, after the time was set.
    time_cut: u32,
    /// The exposure, bloom and tone map's resources.
    pub post: crate::post::Post,
    /// The sky's tables and the air's constants.
    pub atmosphere: crate::atmosphere::Atmosphere,
    /// Whether the sun casts shadows (the physical calibration only).
    pub shadows: bool,
    /// How far along the view the sun's shadows reach, metres.
    pub shadow_distance: f32,
    /// The cascades drawn last frame, when the sun's shadows were.
    pub cascades: Option<[crate::shadows::Cascade; crate::shadows::CASCADES]>,
    shadow_atlas: wgpu::Texture,
    shadow_params: wgpu::Buffer,
    shadow_cull_groups: Vec<wgpu::BindGroup>,
    shadow_pipelines: [wgpu::RenderPipeline; 2],
    shadow_cmp: wgpu::Sampler,
    /// Where the weather's shelter map stands this frame, when it is drawn.
    pub shelter: Option<crate::weather::Shelter>,
    shelter_pipeline: wgpu::RenderPipeline,
    light_layout: wgpu::BindGroupLayout,
    /// The world's ray-tracing structures, on a device with ray queries.
    pub rays: crate::rt::RayScene,
    /// Whether the nearest lamps' shadows are ray traced (where the device can).
    pub lamp_rays: bool,
    ray_pipeline: Option<(wgpu::ComputePipeline, wgpu::BindGroupLayout)>,
    ray_slots: wgpu::Buffer,
    /// Whether the ambient occlusion and the probes' bounce light are drawn (the physical
    /// calibration only; the probes where the device has ray queries).
    pub ambient_occlusion: bool,
    pub bounce: bool,
    gtao_pipeline: wgpu::ComputePipeline,
    gtao_layout: wgpu::BindGroupLayout,
    ao_gather_pipeline: wgpu::ComputePipeline,
    ao_gather_layout: wgpu::BindGroupLayout,
    ao_blur_pipeline: wgpu::ComputePipeline,
    ao_blur_layout: wgpu::BindGroupLayout,
    /// The occlusion's frame index and whether it has history.
    ao_params: wgpu::Buffer,
    /// The occlusion's frames gathered since its history was last good.
    pub(crate) ao_frames: u32,
    /// The volumetric clouds' pass.
    clouds: crate::clouds::Clouds,
    probe_buffer: wgpu::Buffer,
    probe_grid: wgpu::Buffer,
    /// The lamps the probes' rays are lit by ([`crate::lamps::probe_lamps`]).
    probe_lamps: wgpu::Buffer,
    probe_pipeline: Option<(wgpu::ComputePipeline, wgpu::BindGroupLayout)>,
    /// The probes' frames traced so far, and the grid's first cell.
    pub probe_frames: u32,
    /// Whether the lamps are lit per pixel.
    pub lamps: bool,
    /// The lamps lit last frame, in the order the clusters index them.
    pub lamps_lit: usize,
    pub lamps_drawn: Vec<crate::lamps::Lamp>,
    /// The clusters' tiles across and down last frame.
    pub cluster_tiles: (u32, u32),
    lamp_buffer: wgpu::Buffer,
    /// This frame's flames, and how many.
    flame_buffer: wgpu::Buffer,
    flames_drawn: u32,
    cluster_counts: wgpu::Buffer,
    cluster_lamps: wgpu::Buffer,
    cluster_layout: wgpu::BindGroupLayout,
    /// The tiles the cluster buffers hold.
    cluster_capacity: u32,
    cluster_pipeline: wgpu::ComputePipeline,
    cluster_group: wgpu::BindGroup,
    pub tone: ToneCurve,
    /// The land field ([`crate::land`]): its texture and where it lies (first vertex, vertices
    /// across and along).
    land_texture: wgpu::Texture,
    pub land: [f32; 4],
    /// The land field's shore field ([`crate::shore`]) and which it is, so a field moved with
    /// render space keeps its texture.
    shore_texture: wgpu::Texture,
    shore_of: Option<std::sync::Arc<crate::shore::ShoreField>>,
    /// The far terrain's water records and parameters the frame's group holds (with no far
    /// terrain, a stand-in), and which far terrain they are (0: none).
    far_water: (wgpu::TextureView, wgpu::Buffer),
    no_far: (wgpu::TextureView, wgpu::Buffer),
    far_bound: u64,
    /// The ground's material, which the water reads its bed by.
    pub terrain_material: u32,
    /// The water's surface, looks and simulation, and its pass.
    pub water: crate::water::WaterGpu,
    /// The air's light: volumetric fog and the sun's shafts.
    pub air: crate::air::Air,
    /// Rain, snow, wet ground and lightning.
    pub weather: crate::weather::Weather,
    /// The prints in the snow.
    pub trails: crate::snow::Trails,
    /// How far from the eye lamps light the world, metres (the Light Draw Distance).
    pub lamp_reach: f32,
    /// The output's headroom over SDR white (EDR on the Mac; 1 for an SDR output).
    pub headroom: f32,
    /// Whether flames' light flickers (Flame Flicker).
    pub flicker: bool,
    /// How much the wind sways foliage and cover (Foliage Wind): 1, or 0 to hold them still.
    pub foliage_wind: f32,
    /// Cobwebs drawn as silk, and the bodies that stir them (Silk Cobwebs).
    pub webs: crate::webs::Stir,
    /// Whether the water is drawn as water (the physical calibration only).
    pub water_on: bool,
    /// The ground cover, when the pack has some, and whether it is drawn.
    pub cover: Option<crate::cover::Cover>,
    /// The land field last given, whose footprints a cover made later takes.
    land_footprints: Option<crate::land::LandField>,
    pub cover_on: bool,
    /// The cover's reach, a share of the pack's (Ground Cover); its levels' ends also follow
    /// [`Self::lod_bias`].
    pub cover_reach: f32,
    /// How tall the cover grows, 1 the pack's own height (Grass Height).
    pub cover_height: f32,
    /// The surfaces' depth (5h), each part on or off.
    pub surfaces: Surfaces,
    pub cover_pipelines: crate::cover::CoverPipelines,
    cover_draw_pipeline: wgpu::RenderPipeline,
    /// The grass under the cover, and the flowers and stones among it.
    blade_draw_pipeline: wgpu::RenderPipeline,
    flower_draw_pipeline: wgpu::RenderPipeline,
    stone_draw_pipeline: wgpu::RenderPipeline,
    /// The camera's motion where the G-buffer drew nothing (the sky), for MetalFX.
    sky_motion_pipeline: wgpu::RenderPipeline,
    /// Where render space's origin lies in the world, metres (the cover's grid is the world's).
    pub world_origin: glam::Vec2,
    water_layout: wgpu::BindGroupLayout,
    water_depth_pipeline: wgpu::RenderPipeline,
    water_pipeline: wgpu::RenderPipeline,
    water_merge_layout: wgpu::BindGroupLayout,
    water_merge_pipeline: wgpu::RenderPipeline,
    far_water_pipeline: wgpu::RenderPipeline,
    puddle_pipeline: wgpu::RenderPipeline,
    /// Whether the ground is cut away inside rooms (off only to see what the cut does).
    pub cut_ground: bool,
    graph: FrameGraph,
    /// The frame uniform's layout, group 0 of every pass that draws.
    pub frame_layout: wgpu::BindGroupLayout,
    frame_buffer: wgpu::Buffer,
    frame_group: wgpu::BindGroup,
    scene_group: wgpu::BindGroup,
    cull_pipeline: wgpu::ComputePipeline,
    cull_group: wgpu::BindGroup,
    cull_hiz_layout: wgpu::BindGroupLayout,
    depth_pipeline: wgpu::RenderPipeline,
    hiz_first_pipeline: wgpu::ComputePipeline,
    hiz_down_pipeline: wgpu::ComputePipeline,
    hiz_first_layout: wgpu::BindGroupLayout,
    hiz_down_layout: wgpu::BindGroupLayout,
    /// Whether the cull pass tests instances against the ground's depth pyramid.
    pub hiz: bool,
    /// Multiplies the distance each degrade level is kept to; zero keeps every mesh at its near
    /// level.
    pub lod_bias: f32,
    /// How far from the eye a tree is its card, as the far rings past the loaded land draw it,
    /// metres (0: no far trees).
    pub card_edge: f32,
    /// Translucent draws sorted and drawn last frame.
    pub sorted_draws: usize,
    cull_params: wgpu::Buffer,
    stats: wgpu::Buffer,
    pipelines: Vec<(PipelineKind, wgpu::RenderPipeline)>,
    lighting_layout: wgpu::BindGroupLayout,
    lighting_pipeline: wgpu::RenderPipeline,
    tonemap_layout: wgpu::BindGroupLayout,
    tonemap_pipeline: wgpu::RenderPipeline,
    targets: Option<Targets>,
    prev_view_proj: Option<Mat4>,
    /// The frames laid into the clouds' history since it was last started afresh.
    cloud_history: u32,
    /// The timed frames read back when the dynamic resolution last looked.
    timed_seen: u64,
    /// The temporal resolve, the upscalers and the composite ([`crate::temporal`]).
    pub temporal: crate::temporal::Temporal,
    /// Whether frames are jittered and resolved over time (the temporal anti-aliasing).
    pub anti_aliasing: bool,
    /// The upscaler tried first; the first that can run is used ([`crate::temporal::choose`]).
    pub upscaler: crate::temporal::Upscaler,
    /// The share of the output's size the world is drawn at, when not dynamic.
    pub render_scale: f32,
    /// A render scale chosen each frame to hold a frame budget, in place of `render_scale`.
    pub dynamic: Option<crate::temporal::DynamicResolution>,
    /// The UI's picture, laid over the output at its own size.
    pub ui: Option<crate::temporal::Ui>,
    /// The size the world was drawn at last frame, and the output's.
    pub internal: (u32, u32),
    pub output_size: (u32, u32),
    /// What is highlighted.
    pub selection: Selection,
    /// Translucent surfaces and particles drawn order-independently (weighted, blended) rather
    /// than sorted.
    pub oit: bool,
    /// The particles drawn last frame and their draws.
    pub particles_drawn: usize,
    pub particle_draws: usize,
    particle_buffer: wgpu::Buffer,
    /// Sorted particles' pipelines (blended, additive), and the order-independent ones'.
    particle_pipelines: [wgpu::RenderPipeline; 2],
    particle_oit_pipelines: [wgpu::RenderPipeline; 2],
    /// Cobwebs' silk, sorted and order-independent.
    web_pipelines: [wgpu::RenderPipeline; 2],
    /// Flames, sorted and order-independent.
    flame_pipelines: [wgpu::RenderPipeline; 2],
    /// Lightning's channels in a creature's cloud ([`crate::bolts`]), added.
    bolt_pipelines: [wgpu::RenderPipeline; 2],
    /// Liquid balls ([`crate::particles::Look::Blob`]).
    blob_pipeline: wgpu::RenderPipeline,
    /// Vapour, a volume a body ([`crate::particles::Look::Puff`]).
    puff_pipeline: wgpu::RenderPipeline,
    /// Glacier ice ([`PipelineKind::Ice`]).
    ice_pipeline: wgpu::RenderPipeline,
    /// Translucent objects' order-independent pipelines (blended, additive).
    oit_pipelines: [wgpu::RenderPipeline; 2],
    oit_layout: wgpu::BindGroupLayout,
    oit_pipeline: wgpu::RenderPipeline,
    /// A pick on its way back: the buffer its texel is copied into and whether it is mapped.
    pick_pending: Option<(wgpu::Buffer, std::sync::Arc<std::sync::atomic::AtomicBool>)>,
}

fn storage(binding: u32, read_only: bool, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn uniform(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn filtered(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn entire(binding: u32, b: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: b.as_entire_binding(),
    }
}

fn view(binding: u32, v: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(v),
    }
}

/// The pool vertex: position, octahedral normal, texture coordinate and two words.
const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
    0 => Float32x3,
    1 => Snorm16x2,
    2 => Float32x2,
    3 => Uint32,
    4 => Uint32,
];

/// The land field's texture, `width` by `height` vertices of four words each.
fn land_texture(gpu: &Gpu, width: u32, height: u32, texels: &[u8]) -> wgpu::Texture {
    let t = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("land field"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Uint,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        t.as_image_copy(),
        texels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 16),
            rows_per_image: Some(height),
        },
        t.size(),
    );
    t
}

/// The shore field's texture, `width` by `height` texels of four half floats each.
fn shore_texture(gpu: &Gpu, width: u32, height: u32, texels: &[u8]) -> wgpu::Texture {
    let t = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shore field"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        t.as_image_copy(),
        texels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 8),
            rows_per_image: Some(height),
        },
        t.size(),
    );
    t
}

/// The frame's bind group: its uniform, the pre-exposure, the sky's tables, the land field and
/// its shore field, and the far terrain's water.
#[allow(clippy::too_many_arguments)]
fn frame_group(
    gpu: &Gpu,
    layout: &wgpu::BindGroupLayout,
    frame_buffer: &wgpu::Buffer,
    post: &crate::post::Post,
    atmosphere: &crate::atmosphere::Atmosphere,
    land: &wgpu::Texture,
    shore: &wgpu::Texture,
    far: (&wgpu::TextureView, &wgpu::Buffer),
) -> wgpu::BindGroup {
    gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("frame"),
        layout,
        entries: &[
            entire(0, frame_buffer),
            entire(1, &post.pre),
            view(2, &atmosphere.sky_view),
            view(3, &atmosphere.transmittance),
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&atmosphere.sampler),
            },
            entire(5, &atmosphere.constants),
            view(6, &land.create_view(&Default::default())),
            view(7, far.0),
            entire(8, far.1),
            view(9, &shore.create_view(&Default::default())),
        ],
    })
}

fn f32s(out: &mut Vec<u8>, v: &[f32]) {
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
}

/// Runs `$body` on `$jobs`' pool with its own copies of the named handles (each a reference count:
/// the device, layouts, modules), so the body reads as it would here and runs side by side with
/// the others ([`crate::jobs`]).
macro_rules! job {
    ($jobs:expr, [$($c:ident),* $(,)?], $body:expr) => {{
        $(let $c = $c.clone();)*
        $jobs.run(move || $body)
    }};
}

impl Renderer {
    /// What animated effects' reaches (particles, flames) are scaled by: Object Detail's share
    /// ([`Self::lod_bias`]), never under the game's own; every near level kept counts as the most.
    #[must_use]
    pub fn effect_detail(&self) -> f32 {
        if self.lod_bias > 0.0 {
            self.lod_bias.max(1.0)
        } else {
            4.0
        }
    }

    /// The pipelines and bind groups for `scene` and `materials`, drawing to `output_format`.
    ///
    /// # Errors
    /// The frame graph does not validate.
    pub fn new(
        gpu: &Gpu,
        scene: &Scene,
        materials: &MaterialTable,
        output_format: wgpu::TextureFormat,
    ) -> Result<Self, EngineError> {
        use wgpu::ShaderStages as S;
        let device = &gpu.device;
        // Every pipeline is made on the pool, side by side, and waited for where `Self` is put
        // together ([`crate::jobs`]): a cold driver compiles them on all the cores at once.
        let jobs = crate::jobs::Jobs::new();
        let graph = FrameGraph::new(gpu, graph_decl()).map_err(EngineError::Graph)?;

        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[
                uniform(0, S::VERTEX_FRAGMENT | S::COMPUTE),
                storage(1, true, S::VERTEX_FRAGMENT | S::COMPUTE),
                filtered(2, S::VERTEX_FRAGMENT | S::COMPUTE),
                filtered(3, S::VERTEX_FRAGMENT | S::COMPUTE),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: S::VERTEX_FRAGMENT | S::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                uniform(5, S::VERTEX_FRAGMENT | S::COMPUTE),
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: S::VERTEX_FRAGMENT | S::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: S::VERTEX_FRAGMENT | S::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                uniform(8, S::VERTEX_FRAGMENT | S::COMPUTE),
                wgpu::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: S::VERTEX_FRAGMENT | S::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        // The parts with pipelines of their own, each made whole on the pool.
        let materials_layout = materials.layout.clone();
        let post = job!(
            jobs,
            [gpu, frame_layout],
            crate::post::Post::new(&gpu, &frame_layout)
        );
        let atmosphere = job!(jobs, [gpu], crate::atmosphere::Atmosphere::new(&gpu));
        let clouds = job!(jobs, [gpu, frame_layout, materials_layout], {
            crate::clouds::Clouds::new(&gpu, &frame_layout, &materials_layout)
        });
        let air = job!(jobs, [gpu, frame_layout], {
            crate::air::Air::new(&gpu, &frame_layout, COMMON, FULLSCREEN, HDR)
        });
        let weather = job!(jobs, [gpu, frame_layout], {
            crate::weather::Weather::new(&gpu, &frame_layout, COMMON, HDR, DEPTH)
        });
        let trails = job!(jobs, [gpu], crate::snow::Trails::new(&gpu));
        let water = job!(jobs, [gpu, frame_layout], {
            crate::water::WaterGpu::new(&gpu, &frame_layout, &crate::water::WaterTable::default())
        });
        let cover_pipelines = job!(jobs, [gpu, frame_layout], {
            crate::cover::CoverPipelines::new(&gpu, &frame_layout)
        });
        // Made here while the pool works: MetalFX's scaler is not Send.
        let temporal = crate::temporal::Temporal::new(gpu, output_format);
        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame"),
            size: FRAME_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let land_texture = land_texture(gpu, 1, 1, &[0u8; 16]);
        let shore_texture = shore_texture(gpu, 1, 1, &[0u8; 8]);
        let no_far = (
            land_texture.create_view(&Default::default()),
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("no far terrain"),
                size: 64,
                usage: wgpu::BufferUsages::UNIFORM,
                mapped_at_creation: false,
            }),
        );
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene"),
            entries: &[
                storage(0, true, S::VERTEX_FRAGMENT),
                storage(1, true, S::VERTEX_FRAGMENT),
                storage(2, true, S::VERTEX_FRAGMENT),
                storage(3, true, S::VERTEX_FRAGMENT),
                storage(4, true, S::VERTEX_FRAGMENT),
                storage(5, true, S::VERTEX_FRAGMENT),
            ],
        });
        let particle_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particles"),
            size: crate::particles::PARTICLE_SIZE * crate::particles::MAX_PARTICLES as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scene_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene"),
            layout: &scene_layout,
            entries: &[
                entire(0, &scene.instances.buffer),
                entire(1, &scene.visible),
                entire(2, &scene.terrain_cells.buffer),
                entire(3, &scene.cuts.buffer),
                entire(4, &scene.cut_lists.buffer),
                entire(5, &particle_buffer),
            ],
        });

        // The cull pass.
        let cull_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cull"),
            entries: &[
                uniform(0, S::COMPUTE),
                storage(1, true, S::COMPUTE),
                storage(2, true, S::COMPUTE),
                storage(3, true, S::COMPUTE),
                storage(4, true, S::COMPUTE),
                storage(5, false, S::COMPUTE),
                storage(6, false, S::COMPUTE),
                storage(7, false, S::COMPUTE),
                storage(8, true, S::COMPUTE),
            ],
        });
        let cull_params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cull params"),
            size: CULL_PARAMS_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let stats = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cull stats"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cull_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cull"),
            layout: &cull_layout,
            entries: &[
                entire(0, &cull_params),
                entire(1, &scene.instances.buffer),
                entire(2, &scene.mesh_table),
                entire(3, &scene.mesh_buckets),
                entire(4, &scene.bucket_table),
                entire(5, &scene.args),
                entire(6, &scene.visible),
                entire(7, &stats),
                entire(8, &scene.cell_rects),
            ],
        });
        // The cascades' cull: the same pass, each cascade's parameters at its own offset, its
        // draws into the cascades' arguments and the visible list past the frame's.
        let shadow_params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow cull params"),
            size: SHADOW_PARAMS_STRIDE * crate::scene::SHADOW_LISTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shadow_cull_groups = (0..crate::scene::SHADOW_LISTS)
            .map(|c| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("shadow cull"),
                    layout: &cull_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &shadow_params,
                                offset: c * SHADOW_PARAMS_STRIDE,
                                size: wgpu::BufferSize::new(CULL_PARAMS_SIZE),
                            }),
                        },
                        entire(1, &scene.instances.buffer),
                        entire(2, &scene.mesh_table),
                        entire(3, &scene.mesh_buckets),
                        entire(4, &scene.bucket_table),
                        entire(5, &scene.shadow_args),
                        entire(6, &scene.visible),
                        entire(7, &stats),
                        entire(8, &scene.cell_rects),
                    ],
                })
            })
            .collect();
        let cull_hiz_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cull pyramid"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: S::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let storage_out = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: S::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: wgpu::TextureFormat::R32Float,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        };
        let compute_texture = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: S::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let hiz_first_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pyramid level 0"),
            entries: &[
                compute_texture(0, wgpu::TextureSampleType::Depth),
                storage_out(1),
            ],
        });
        let hiz_down_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pyramid level"),
            entries: &[
                compute_texture(0, wgpu::TextureSampleType::Float { filterable: false }),
                storage_out(1),
            ],
        });
        // A compute pipeline of one group's layout, made on the pool.
        let compute = |label: &'static str,
                       src: String,
                       entry: &'static str,
                       layouts: &[&wgpu::BindGroupLayout]| {
            let g = gpu.clone();
            let layouts: Vec<wgpu::BindGroupLayout> = layouts.iter().map(|&l| l.clone()).collect();
            jobs.run(move || {
                let module = g.shader(label, &src);
                let groups: Vec<Option<&wgpu::BindGroupLayout>> =
                    layouts.iter().map(Some).collect();
                let layout = g
                    .device
                    .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some(label),
                        bind_group_layouts: &groups,
                        immediate_size: 0,
                    });
                g.compute_pipeline(wgpu::ComputePipelineDescriptor {
                    label: Some(label),
                    layout: Some(&layout),
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    cache: None,
                })
            })
        };
        let hiz_first_pipeline = compute(
            "pyramid level 0",
            source::hiz_first(),
            "hiz_first",
            &[&hiz_first_layout],
        );
        let hiz_down_pipeline = compute(
            "pyramid level",
            source::hiz_down(),
            "hiz_down",
            &[&hiz_down_layout],
        );
        let cull_pipeline = compute(
            "cull",
            source::cull(),
            "cull",
            &[&cull_layout, &cull_hiz_layout],
        );

        // The mesh pipelines.
        let mesh_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[
                Some(&frame_layout),
                Some(&scene_layout),
                Some(&materials.layout),
            ],
            immediate_size: 0,
        });
        // The modules most pipelines share, read side by side before the pipelines are asked for.
        let module = |label: &'static str, src: String| {
            let g = gpu.clone();
            jobs.run(move || g.shader(label, &src))
        };
        let gbuffer_module = module("gbuffer", source::gbuffer());
        let forward_module = module("forward", source::forward());
        let shadow_module = module("shadow casters", source::shadow_cast());
        let (gbuffer_module, forward_module, shadow_module) = (
            gbuffer_module.wait(),
            forward_module.wait(),
            shadow_module.wait(),
        );
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: crate::asset::mesh::VERTEX_SIZE,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRIBUTES,
        };
        let gbuffer_targets = [
            Some(ALBEDO.into()),
            Some(NORMAL.into()),
            Some(MATERIAL.into()),
            Some(MOTION.into()),
            Some(IDS.into()),
            Some(IDS.into()),
        ];
        let blend_alpha = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };
        let blend_add = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };
        let mut pipelines: Vec<(PipelineKind, crate::jobs::Pending<wgpu::RenderPipeline>)> =
            PipelineKind::ALL
                .iter()
                .filter(|k| k.deferred())
                .map(|&kind| {
                    let (module, fs, cull, blend) = match kind {
                        PipelineKind::Opaque => {
                            (&gbuffer_module, "fs_object", Some(wgpu::Face::Back), None)
                        }
                        PipelineKind::TwoSided => (&gbuffer_module, "fs_object", None, None),
                        PipelineKind::Terrain => {
                            (&gbuffer_module, "fs_terrain", Some(wgpu::Face::Back), None)
                        }
                        PipelineKind::Translucent
                        | PipelineKind::Additive
                        | PipelineKind::Web
                        | PipelineKind::Ice => {
                            unreachable!("forward pipelines are made with the light's layout")
                        }
                    };
                    let (g, mesh_layout, module, vertex_layout, gbuffer_targets) = (
                        gpu.clone(),
                        mesh_layout.clone(),
                        module.clone(),
                        vertex_layout.clone(),
                        gbuffer_targets.clone(),
                    );
                    let pipeline = jobs.run(move || {
                        let forward_target = [Some(wgpu::ColorTargetState {
                            format: HDR,
                            blend,
                            write_mask: wgpu::ColorWrites::ALL,
                        })];
                        let targets: &[Option<wgpu::ColorTargetState>] = if kind.deferred() {
                            &gbuffer_targets
                        } else {
                            &forward_target
                        };
                        g.render_pipeline(wgpu::RenderPipelineDescriptor {
                            label: Some("mesh"),
                            layout: Some(&mesh_layout),
                            vertex: wgpu::VertexState {
                                module: &module,
                                entry_point: Some("vs_main"),
                                compilation_options: Default::default(),
                                buffers: &[Some(vertex_layout)],
                            },
                            primitive: wgpu::PrimitiveState {
                                front_face: wgpu::FrontFace::Ccw,
                                cull_mode: cull,
                                ..Default::default()
                            },
                            depth_stencil: Some(wgpu::DepthStencilState {
                                format: DEPTH,
                                depth_write_enabled: Some(kind.deferred()),
                                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                                stencil: Default::default(),
                                bias: Default::default(),
                            }),
                            multisample: Default::default(),
                            fragment: Some(wgpu::FragmentState {
                                module: &module,
                                entry_point: Some(fs),
                                compilation_options: Default::default(),
                                targets,
                            }),
                            multiview_mask: None,
                            cache: None,
                        })
                    });
                    (kind, pipeline)
                })
                .collect();

        // The sun's shadow casters: depth alone, both faces, cut-outs cut.
        let shadow_pipeline = |fs: Option<&'static str>| {
            job!(
                jobs,
                [gpu, mesh_layout, shadow_module, vertex_layout],
                gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                    label: Some("shadow casters"),
                    layout: Some(&mesh_layout),
                    vertex: wgpu::VertexState {
                        module: &shadow_module,
                        entry_point: Some("vs_shadow"),
                        compilation_options: Default::default(),
                        buffers: &[Some(vertex_layout.clone())],
                    },
                    primitive: wgpu::PrimitiveState {
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: crate::shadows::FORMAT,
                        depth_write_enabled: Some(true),
                        depth_compare: Some(wgpu::CompareFunction::Less),
                        stencil: Default::default(),
                        bias: wgpu::DepthBiasState {
                            constant: 2,
                            slope_scale: 2.0,
                            clamp: 0.0,
                        },
                    }),
                    multisample: Default::default(),
                    fragment: fs.map(|entry| wgpu::FragmentState {
                        module: &shadow_module,
                        entry_point: Some(entry),
                        compilation_options: Default::default(),
                        targets: &[],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            )
        };
        let shadow_pipelines = [shadow_pipeline(Some("fs_shadow")), shadow_pipeline(None)];
        // The weather's shelter map: depth alone, straight down, both faces.
        let shelter_pipeline = job!(
            jobs,
            [gpu, mesh_layout, shadow_module, vertex_layout],
            gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                label: Some("shelter"),
                layout: Some(&mesh_layout),
                vertex: wgpu::VertexState {
                    module: &shadow_module,
                    entry_point: Some("vs_shelter"),
                    compilation_options: Default::default(),
                    buffers: &[Some(vertex_layout.clone())],
                },
                primitive: wgpu::PrimitiveState {
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::weather::SHELTER_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: None,
                multiview_mask: None,
                cache: None,
            })
        );
        let shadow_atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow atlas"),
            size: wgpu::Extent3d {
                width: crate::shadows::ATLAS,
                height: crate::shadows::ATLAS,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::shadows::FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_cmp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow compare"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        // The lamps and their clusters.
        let lamp_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame lamps"),
            size: crate::lamps::LAMP_SIZE * crate::lamps::MAX_FRAME_LAMPS as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let flame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame flames"),
            size: crate::flames::FLAME_SIZE * crate::flames::MAX_FRAME_FLAMES as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cluster_capacity = CLUSTER_TILES.0 * CLUSTER_TILES.1;
        let (cluster_counts, cluster_lamps) = cluster_buffers(device, cluster_capacity);
        let cluster_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("clusters"),
            entries: &[
                storage(0, true, S::COMPUTE),
                storage(1, false, S::COMPUTE),
                storage(2, false, S::COMPUTE),
            ],
        });
        let cluster_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("clusters"),
            layout: &cluster_layout,
            entries: &[
                entire(0, &lamp_buffer),
                entire(1, &cluster_counts),
                entire(2, &cluster_lamps),
            ],
        });
        let cluster_pipeline = compute(
            "clusters",
            source::clusters(),
            "build",
            &[&frame_layout, &cluster_layout],
        );
        // The ambient occlusion's search.
        let storage_tex = |binding, format| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: S::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        };
        let unfiltered = wgpu::TextureSampleType::Float { filterable: false };
        let gtao_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gtao"),
            entries: &[
                compute_texture(0, wgpu::TextureSampleType::Depth),
                storage_tex(1, wgpu::TextureFormat::Rgba16Float),
                uniform(2, S::COMPUTE),
            ],
        });
        let ao_gather_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ao gather"),
            entries: &[
                compute_texture(0, unfiltered),
                compute_texture(1, unfiltered),
                compute_texture(2, unfiltered),
                compute_texture(3, wgpu::TextureSampleType::Depth),
                storage_tex(4, wgpu::TextureFormat::Rgba16Float),
                uniform(5, S::COMPUTE),
            ],
        });
        let ao_blur_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ao blur"),
            entries: &[
                compute_texture(0, unfiltered),
                storage_tex(1, wgpu::TextureFormat::Rgba8Unorm),
            ],
        });
        let ao_params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ao params"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ao_gather_pipeline = compute(
            "ao gather",
            source::ao_filter(),
            "gather",
            &[&frame_layout, &ao_gather_layout],
        );
        let ao_blur_pipeline = compute(
            "ao blur",
            source::ao_blur(),
            "blur",
            &[&frame_layout, &ao_blur_layout],
        );
        let gtao_pipeline = compute(
            "gtao",
            source::gtao(),
            "gtao",
            &[&frame_layout, &gtao_layout],
        );
        // The probes and their update.
        let probe_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("probes"),
            size: u64::from(PROBE_DIMS.0 * PROBE_DIMS.1 * PROBE_DIMS.2) * PROBE_BYTES,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let probe_grid = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("probe grid"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let probe_lamps = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("probe lamps"),
            size: crate::lamps::PROBE_LAMPS as u64 * 64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let probe_pipeline = gpu.rays().then(|| {
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("probes"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: S::COMPUTE,
                        ty: wgpu::BindingType::AccelerationStructure {
                            vertex_return: false,
                        },
                        count: None,
                    },
                    storage(1, false, S::COMPUTE),
                    storage(2, true, S::COMPUTE),
                    storage(3, true, S::COMPUTE),
                    storage(4, true, S::COMPUTE),
                    storage(5, true, S::COMPUTE),
                    uniform(6, S::COMPUTE),
                    storage(7, true, S::COMPUTE),
                ],
            });
            let pipeline = compute(
                "probes",
                source::probes(),
                "update",
                &[&frame_layout, &layout, &materials.layout],
            );
            (pipeline, layout)
        });
        let light_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("light"),
            entries: &[
                texture(0, wgpu::TextureSampleType::Depth),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: S::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                storage(2, true, S::FRAGMENT),
                storage(3, true, S::FRAGMENT),
                storage(4, true, S::FRAGMENT),
                texture(5, wgpu::TextureSampleType::Float { filterable: false }),
                texture(6, wgpu::TextureSampleType::Float { filterable: false }),
                storage(7, true, S::FRAGMENT),
                uniform(8, S::FRAGMENT),
                texture(9, wgpu::TextureSampleType::Depth),
                texture(10, wgpu::TextureSampleType::Depth),
                uniform(11, S::VERTEX),
                storage(12, true, S::VERTEX_FRAGMENT),
                texture(13, wgpu::TextureSampleType::Float { filterable: true }),
                texture(14, wgpu::TextureSampleType::Float { filterable: true }),
            ],
        });
        // The translucent surfaces' and particles' pipelines: lit as the opaque world is, their
        // light's group fourth; sorted (one target) or order-independent (three).
        let forward_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("forward"),
            bind_group_layouts: &[
                Some(&frame_layout),
                Some(&scene_layout),
                Some(&materials.layout),
                Some(&light_layout),
            ],
            immediate_size: 0,
        });
        let oit_add = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let oit_reveal = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::OneMinusSrc,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::REPLACE,
        };
        let forward =
            |label: &'static str, vs: &'static str, fs: &'static str, additive: bool, oit: bool| {
                job!(
                    jobs,
                    [gpu, forward_layout, forward_module, vertex_layout],
                    {
                        let target = |format, blend, on: bool| {
                            Some(wgpu::ColorTargetState {
                                format,
                                blend: Some(blend),
                                write_mask: if on {
                                    wgpu::ColorWrites::ALL
                                } else {
                                    wgpu::ColorWrites::empty()
                                },
                            })
                        };
                        // The reactive mask keeps the most any draw changed a pixel by.
                        let most = wgpu::BlendState {
                            color: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::One,
                                dst_factor: wgpu::BlendFactor::One,
                                operation: wgpu::BlendOperation::Max,
                            },
                            alpha: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::One,
                                dst_factor: wgpu::BlendFactor::One,
                                operation: wgpu::BlendOperation::Max,
                            },
                        };
                        let sorted = [
                            target(HDR, if additive { blend_add } else { blend_alpha }, true),
                            target(wgpu::TextureFormat::R8Unorm, most, true),
                        ];
                        // Under `oit` additive light goes to a target of its own, laid on top afterwards.
                        let three = [
                            target(HDR, blend_add, true),
                            target(OIT_ACCUM, oit_add, true),
                            target(OIT_REVEAL, oit_reveal, true),
                        ];
                        let targets: &[Option<wgpu::ColorTargetState>] =
                            if oit { &three } else { &sorted };
                        gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                            label: Some(label),
                            layout: Some(&forward_layout),
                            vertex: wgpu::VertexState {
                                module: &forward_module,
                                entry_point: Some(vs),
                                compilation_options: Default::default(),
                                buffers: &[Some(vertex_layout.clone())],
                            },
                            primitive: wgpu::PrimitiveState {
                                front_face: wgpu::FrontFace::Ccw,
                                cull_mode: None,
                                ..Default::default()
                            },
                            depth_stencil: Some(wgpu::DepthStencilState {
                                format: DEPTH,
                                depth_write_enabled: Some(false),
                                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                                stencil: Default::default(),
                                bias: Default::default(),
                            }),
                            multisample: Default::default(),
                            fragment: Some(wgpu::FragmentState {
                                module: &forward_module,
                                entry_point: Some(fs),
                                compilation_options: Default::default(),
                                targets,
                            }),
                            multiview_mask: None,
                            cache: None,
                        })
                    }
                )
            };
        pipelines.push((
            PipelineKind::Translucent,
            forward("translucent", "vs_main", "fs_translucent", false, false),
        ));
        pipelines.push((
            PipelineKind::Additive,
            forward("additive", "vs_main", "fs_translucent", true, false),
        ));
        let oit_pipelines = [
            forward(
                "translucent oit",
                "vs_main",
                "fs_translucent_oit",
                false,
                true,
            ),
            forward("additive oit", "vs_main", "fs_translucent_oit", true, true),
        ];
        let particle_pipelines = [
            forward("particles", "vs_particle", "fs_particle", false, false),
            forward(
                "particles additive",
                "vs_particle",
                "fs_particle",
                true,
                false,
            ),
        ];
        let particle_oit_pipelines = [
            forward(
                "particles oit",
                "vs_particle",
                "fs_particle_oit",
                false,
                true,
            ),
            forward(
                "particles additive oit",
                "vs_particle",
                "fs_particle_oit",
                true,
                true,
            ),
        ];
        let web_pipelines = [
            forward("webs", "vs_web", "fs_web", false, false),
            forward("webs oit", "vs_web", "fs_web_oit", false, true),
        ];
        let flame_pipelines = [
            forward("flames", "vs_flame", "fs_flame", true, false),
            forward("flames oit", "vs_flame", "fs_flame_oit", true, true),
        ];
        let bolt_pipelines = [
            forward("bolts", "vs_bolt", "fs_bolt", true, false),
            forward("bolts oit", "vs_bolt", "fs_bolt_oit", true, true),
        ];
        // Glacier ice: over the frame it reads behind it, writing its depth.
        let ice_pipeline = job!(
            jobs,
            [gpu, forward_layout, forward_module, vertex_layout],
            gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                label: Some("ice"),
                layout: Some(&forward_layout),
                vertex: wgpu::VertexState {
                    module: &forward_module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(vertex_layout.clone())],
                },
                primitive: wgpu::PrimitiveState {
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &forward_module,
                    entry_point: Some("fs_ice"),
                    compilation_options: Default::default(),
                    targets: &[
                        Some(wgpu::ColorTargetState {
                            format: HDR,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                        // The reactive mask, which only the vapour marks.
                        Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::R8Unorm,
                            blend: None,
                            write_mask: wgpu::ColorWrites::empty(),
                        }),
                    ],
                }),
                multiview_mask: None,
                cache: None,
            })
        );
        // Liquid balls, drawn with the ice ([`crate::particles::Look::Blob`]).
        let blob_pipeline = job!(
            jobs,
            [gpu, forward_layout, forward_module],
            gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                label: Some("blobs"),
                layout: Some(&forward_layout),
                vertex: wgpu::VertexState {
                    module: &forward_module,
                    entry_point: Some("vs_blob"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &forward_module,
                    entry_point: Some("fs_blob"),
                    compilation_options: Default::default(),
                    targets: &[
                        Some(wgpu::ColorTargetState {
                            format: HDR,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                        // The reactive mask, which only the vapour marks.
                        Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::R8Unorm,
                            blend: None,
                            write_mask: wgpu::ColorWrites::empty(),
                        }),
                    ],
                }),
                multiview_mask: None,
                cache: None,
            })
        );
        // Vapour, drawn with the ice over what is behind it ([`crate::particles::Look::Puff`]).
        let puff_pipeline = job!(
            jobs,
            [gpu, forward_layout, forward_module],
            gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                label: Some("puffs"),
                layout: Some(&forward_layout),
                vertex: wgpu::VertexState {
                    module: &forward_module,
                    entry_point: Some("vs_puff"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &forward_module,
                    entry_point: Some("fs_puff"),
                    compilation_options: Default::default(),
                    targets: &[
                        Some(wgpu::ColorTargetState {
                            format: HDR,
                            blend: Some(wgpu::BlendState {
                                color: wgpu::BlendComponent {
                                    src_factor: wgpu::BlendFactor::One,
                                    dst_factor: wgpu::BlendFactor::SrcAlpha,
                                    operation: wgpu::BlendOperation::Add,
                                },
                                alpha: wgpu::BlendComponent {
                                    src_factor: wgpu::BlendFactor::Zero,
                                    dst_factor: wgpu::BlendFactor::One,
                                    operation: wgpu::BlendOperation::Add,
                                },
                            }),
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                        // How much it changes the pixel, for the upscaler: the most of any draw.
                        Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::R8Unorm,
                            blend: Some(wgpu::BlendState {
                                color: wgpu::BlendComponent {
                                    src_factor: wgpu::BlendFactor::One,
                                    dst_factor: wgpu::BlendFactor::One,
                                    operation: wgpu::BlendOperation::Max,
                                },
                                alpha: wgpu::BlendComponent {
                                    src_factor: wgpu::BlendFactor::One,
                                    dst_factor: wgpu::BlendFactor::One,
                                    operation: wgpu::BlendOperation::Max,
                                },
                            }),
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                    ],
                }),
                multiview_mask: None,
                cache: None,
            })
        );
        // The lamps' rays: the G-buffer's depth and normals, the world's top level, the
        // shadows out, the lamps and the slots they are traced for.
        let ray_slots = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lamp ray slots"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ray_pipeline = gpu.rays().then(|| {
            let compute_tex = |binding, sample_type| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: S::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            };
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("lamp rays"),
                entries: &[
                    compute_tex(0, wgpu::TextureSampleType::Depth),
                    compute_tex(1, wgpu::TextureSampleType::Float { filterable: false }),
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: S::COMPUTE,
                        ty: wgpu::BindingType::AccelerationStructure {
                            vertex_return: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: S::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                    storage(4, true, S::COMPUTE),
                    uniform(5, S::COMPUTE),
                ],
            });
            let pipeline = compute(
                "lamp rays",
                source::lamp_rays(),
                "trace",
                &[&frame_layout, &layout],
            );
            (pipeline, layout)
        });

        // The ground's depth prepass: the terrain's own vertices, depth alone.
        let depth_pipeline = job!(
            jobs,
            [gpu, vertex_layout, mesh_layout, gbuffer_module],
            gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                label: Some("ground depth"),
                layout: Some(&mesh_layout),
                vertex: wgpu::VertexState {
                    module: &gbuffer_module,
                    entry_point: Some("vs_depth"),
                    compilation_options: Default::default(),
                    buffers: &[Some(vertex_layout.clone())],
                },
                primitive: wgpu::PrimitiveState {
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &gbuffer_module,
                    entry_point: Some("fs_depth"),
                    compilation_options: Default::default(),
                    targets: &[],
                }),
                multiview_mask: None,
                cache: None,
            })
        );

        // The full-screen passes.
        let unfilterable = wgpu::TextureSampleType::Float { filterable: false };
        let lighting_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lighting"),
            entries: &[
                texture(0, unfilterable),
                texture(1, unfilterable),
                texture(2, unfilterable),
                texture(3, wgpu::TextureSampleType::Depth),
                texture(4, wgpu::TextureSampleType::Float { filterable: true }),
                texture(5, wgpu::TextureSampleType::Float { filterable: true }),
            ],
        });
        let tonemap_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tonemap"),
            entries: &[
                texture(0, unfilterable),
                texture(1, wgpu::TextureSampleType::Float { filterable: true }),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: S::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                storage(3, true, S::FRAGMENT),
                texture(4, wgpu::TextureSampleType::Depth),
                texture(5, wgpu::TextureSampleType::Uint),
            ],
        });
        // A full-screen pass of its own module, made on the pool.
        let fullscreen = |label: &'static str,
                          src: String,
                          fs: &'static str,
                          layouts: &[&wgpu::BindGroupLayout],
                          format: wgpu::TextureFormat| {
            let layouts: Vec<wgpu::BindGroupLayout> = std::iter::once(&frame_layout)
                .chain(layouts.iter().copied())
                .cloned()
                .collect();
            job!(jobs, [gpu], {
                let module = gpu.shader(label, &src);
                let groups: Vec<Option<&wgpu::BindGroupLayout>> =
                    layouts.iter().map(Some).collect();
                gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&gpu.device.create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some(label),
                            bind_group_layouts: &groups,
                            immediate_size: 0,
                        },
                    )),
                    vertex: wgpu::VertexState {
                        module: &module,
                        entry_point: Some("vs_fullscreen"),
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
                        targets: &[Some(format.into())],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            })
        };
        let lighting_pipeline = fullscreen(
            "lighting",
            source::lighting(),
            "fs_lighting",
            &[&lighting_layout, &materials.layout, &light_layout],
            HDR,
        );
        let tonemap_pipeline = fullscreen(
            "tonemap",
            source::tonemap(),
            "fs_tonemap",
            &[&tonemap_layout],
            crate::temporal::LDR,
        );
        let oit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("oit"),
            entries: &[
                texture(0, unfilterable),
                texture(1, unfilterable),
                texture(2, unfilterable),
            ],
        });
        let oit_module = gpu.shader("oit", &source::oit());
        let oit_pipeline = job!(
            jobs,
            [gpu, frame_layout, oit_module, oit_layout, device],
            gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                label: Some("oit"),
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("oit"),
                        bind_group_layouts: &[Some(&frame_layout), Some(&oit_layout)],
                        immediate_size: 0,
                    }),
                ),
                vertex: wgpu::VertexState {
                    module: &oit_module,
                    entry_point: Some("vs_fullscreen"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &oit_module,
                    entry_point: Some("fs_oit"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: HDR,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        );
        let sky_motion_pipeline = job!(
            jobs,
            [gpu, device, frame_layout, gbuffer_targets, gbuffer_module],
            {
                let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("sky motion"),
                    bind_group_layouts: &[Some(&frame_layout)],
                    immediate_size: 0,
                });
                let targets = gbuffer_targets.clone().map(|t| {
                    t.map(|mut t| {
                        if t.format != MOTION {
                            t.write_mask = wgpu::ColorWrites::empty();
                        }
                        t
                    })
                });
                gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                    label: Some("sky motion"),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        module: &gbuffer_module,
                        entry_point: Some("vs_sky_motion"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    primitive: Default::default(),
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: DEPTH,
                        depth_write_enabled: Some(false),
                        depth_compare: Some(wgpu::CompareFunction::Equal),
                        stencil: Default::default(),
                        bias: Default::default(),
                    }),
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &gbuffer_module,
                        entry_point: Some("fs_sky_motion"),
                        compilation_options: Default::default(),
                        targets: &targets,
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            }
        );
        let cover_pipelines = cover_pipelines.wait();
        let cover_module = gpu.shader("cover draws", &source::cover_draw());
        let cover_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cover draws"),
            bind_group_layouts: &[
                Some(&frame_layout),
                Some(&scene_layout),
                Some(&materials.layout),
                Some(&cover_pipelines.draw_layout),
            ],
            immediate_size: 0,
        });
        // The cover's plants from the pool's vertices; the grass under it from nothing.
        let cover_pipeline =
            |label: &'static str,
             vs: &'static str,
             fs: &'static str,
             buffers: Vec<Option<wgpu::VertexBufferLayout<'static>>>| {
                job!(
                    jobs,
                    [gpu, cover_layout, cover_module, gbuffer_targets],
                    gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                        label: Some(label),
                        layout: Some(&cover_layout),
                        vertex: wgpu::VertexState {
                            module: &cover_module,
                            entry_point: Some(vs),
                            compilation_options: Default::default(),
                            buffers: &buffers,
                        },
                        primitive: wgpu::PrimitiveState {
                            front_face: wgpu::FrontFace::Ccw,
                            cull_mode: None,
                            ..Default::default()
                        },
                        depth_stencil: Some(wgpu::DepthStencilState {
                            format: DEPTH,
                            depth_write_enabled: Some(true),
                            depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                            stencil: Default::default(),
                            bias: Default::default(),
                        }),
                        multisample: Default::default(),
                        fragment: Some(wgpu::FragmentState {
                            module: &cover_module,
                            entry_point: Some(fs),
                            compilation_options: Default::default(),
                            targets: &gbuffer_targets,
                        }),
                        multiview_mask: None,
                        cache: None,
                    })
                )
            };
        let cover_draw_pipeline = cover_pipeline(
            "cover draws",
            "vs_cover",
            "fs_cover",
            vec![Some(vertex_layout.clone())],
        );
        let blade_draw_pipeline = cover_pipeline("cover grass", "vs_blade", "fs_blade", Vec::new());
        let flower_draw_pipeline =
            cover_pipeline("cover flowers", "vs_flower", "fs_flower", Vec::new());
        let stone_draw_pipeline =
            cover_pipeline("cover stones", "vs_stone", "fs_stone", Vec::new());
        let filtered_frag = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: S::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler_frag = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: S::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let water_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("water"),
            entries: &[
                filtered_frag(0),
                sampler_frag(1),
                texture(2, unfilterable),
                texture(3, wgpu::TextureSampleType::Depth),
                filtered_frag(4),
                sampler_frag(5),
                storage(6, true, S::VERTEX_FRAGMENT),
                texture(7, wgpu::TextureSampleType::Uint),
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: S::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage(9, true, S::VERTEX),
                texture(10, wgpu::TextureSampleType::Depth),
                storage(11, true, S::VERTEX_FRAGMENT),
            ],
        });
        let water_module = gpu.shader("water", &source::water());
        let water_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("water"),
                bind_group_layouts: &[
                    Some(&frame_layout),
                    Some(&water_layout),
                    Some(&materials.layout),
                    Some(&light_layout),
                ],
                immediate_size: 0,
            });
        // The water's colour and reactive mask; the far water's colour alone.
        let water_targets = |all: bool| {
            [
                Some(HDR.into()),
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R8Unorm,
                    blend: None,
                    write_mask: if all {
                        wgpu::ColorWrites::ALL
                    } else {
                        wgpu::ColorWrites::empty()
                    },
                }),
            ]
        };
        // The patches into the water's own depth (tested by hand against the world's, the bed
        // let through) and their motion.
        let water_depth_pipeline = job!(
            jobs,
            [gpu, water_pipeline_layout, water_module],
            gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                label: Some("water depth"),
                layout: Some(&water_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &water_module,
                    entry_point: Some("vs_water"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState {
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &water_module,
                    entry_point: Some("fs_water_depth"),
                    compilation_options: Default::default(),
                    targets: &[Some(MOTION.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        );
        // The water shaded from that depth, once a pixel however fine its triangles.
        let water_pipeline = job!(
            jobs,
            [gpu, water_pipeline_layout, water_module],
            gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                label: Some("water"),
                layout: Some(&water_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &water_module,
                    entry_point: Some("vs_fullscreen"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &water_module,
                    entry_point: Some("fs_water"),
                    compilation_options: Default::default(),
                    targets: &water_targets(true),
                }),
                multiview_mask: None,
                cache: None,
            })
        );
        // The water's depth laid into the world's.
        let water_merge_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("water merge"),
                entries: &[texture(0, wgpu::TextureSampleType::Depth)],
            });
        let merge_module = gpu.shader("water merge", &source::water_merge());
        let water_merge_pipeline = job!(
            jobs,
            [gpu, merge_module, water_merge_layout, device],
            gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                label: Some("water merge"),
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("water merge"),
                        bind_group_layouts: &[Some(&water_merge_layout)],
                        immediate_size: 0,
                    }),
                ),
                vertex: wgpu::VertexState {
                    module: &merge_module,
                    entry_point: Some("vs_fullscreen"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &merge_module,
                    entry_point: Some("fs_water_merge"),
                    compilation_options: Default::default(),
                    targets: &[],
                }),
                multiview_mask: None,
                cache: None,
            })
        );
        // The far rings' water, shaded over the lit far ground as the near water is.
        let far_water_pipeline = job!(
            jobs,
            [
                gpu,
                water_module,
                water_layout,
                frame_layout,
                light_layout,
                device,
                materials_layout
            ],
            gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                label: Some("far water"),
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("far water"),
                        bind_group_layouts: &[
                            Some(&frame_layout),
                            Some(&water_layout),
                            Some(&materials_layout),
                            Some(&light_layout),
                        ],
                        immediate_size: 0,
                    }),
                ),
                vertex: wgpu::VertexState {
                    module: &water_module,
                    entry_point: Some("vs_fullscreen"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &water_module,
                    entry_point: Some("fs_far_water"),
                    compilation_options: Default::default(),
                    targets: &water_targets(false),
                }),
                multiview_mask: None,
                cache: None,
            })
        );
        // The rain's puddles: their mirror laid over the lit ground, as far as each mirrors.
        let puddle_pipeline = job!(
            jobs,
            [
                gpu,
                water_module,
                water_layout,
                frame_layout,
                light_layout,
                device,
                materials_layout
            ],
            gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                label: Some("puddles"),
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("puddles"),
                        bind_group_layouts: &[
                            Some(&frame_layout),
                            Some(&water_layout),
                            Some(&materials_layout),
                            Some(&light_layout),
                        ],
                        immediate_size: 0,
                    }),
                ),
                vertex: wgpu::VertexState {
                    module: &water_module,
                    entry_point: Some("vs_fullscreen"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &water_module,
                    entry_point: Some("fs_puddles"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: HDR,
                        blend: Some(wgpu::BlendState {
                            color: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::SrcAlpha,
                                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                                operation: wgpu::BlendOperation::Add,
                            },
                            alpha: wgpu::BlendComponent::OVER,
                        }),
                        write_mask: wgpu::ColorWrites::COLOR,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        );
        // Everything made on the pool, waited for: the frame's group, which reads the exposure's
        // and the air's resources, last.
        let (post, atmosphere) = (post.wait(), atmosphere.wait());
        let frame_group = frame_group(
            gpu,
            &frame_layout,
            &frame_buffer,
            &post,
            &atmosphere,
            &land_texture,
            &shore_texture,
            (&no_far.0, &no_far.1),
        );
        let pipelines: Vec<(PipelineKind, wgpu::RenderPipeline)> =
            pipelines.into_iter().map(|(k, p)| (k, p.wait())).collect();
        let shadow_pipelines = shadow_pipelines.map(crate::jobs::Pending::wait);
        let oit_pipelines = oit_pipelines.map(crate::jobs::Pending::wait);
        let particle_pipelines = particle_pipelines.map(crate::jobs::Pending::wait);
        let particle_oit_pipelines = particle_oit_pipelines.map(crate::jobs::Pending::wait);
        let web_pipelines = web_pipelines.map(crate::jobs::Pending::wait);
        let flame_pipelines = flame_pipelines.map(crate::jobs::Pending::wait);
        let bolt_pipelines = bolt_pipelines.map(crate::jobs::Pending::wait);
        let probe_pipeline = probe_pipeline.map(|(p, l)| (p.wait(), l));
        let ray_pipeline = ray_pipeline.map(|(p, l)| (p.wait(), l));
        Ok(Self {
            output_format,
            clock: 0.0,
            calibration: Calibration::Physical,
            exposure: Exposure::default(),
            bloom: crate::post::BLOOM_STRENGTH,
            frame_seconds: None,
            last_frame: None,
            last_clock: None,
            last_time: None,
            time_cut: 0,
            post,
            atmosphere,
            shadows: true,
            shadow_distance: crate::shadows::SHADOW_DISTANCE,
            cascades: None,
            shadow_atlas,
            shadow_params,
            shadow_cull_groups,
            shadow_pipelines,
            shadow_cmp,
            shelter: None,
            shelter_pipeline: shelter_pipeline.wait(),
            light_layout,
            rays: crate::rt::RayScene::default(),
            lamp_rays: true,
            ray_pipeline,
            ray_slots,
            ambient_occlusion: true,
            bounce: true,
            gtao_pipeline: gtao_pipeline.wait(),
            gtao_layout,
            ao_gather_pipeline: ao_gather_pipeline.wait(),
            ao_gather_layout,
            ao_blur_pipeline: ao_blur_pipeline.wait(),
            ao_blur_layout,
            ao_params,
            ao_frames: 0,
            clouds: clouds.wait(),
            probe_buffer,
            probe_grid,
            probe_lamps,
            probe_pipeline,
            probe_frames: 0,
            lamps: true,
            lamps_lit: 0,
            lamps_drawn: Vec::new(),
            cluster_tiles: (0, 0),
            lamp_buffer,
            flame_buffer,
            flames_drawn: 0,
            cluster_counts,
            cluster_lamps,
            cluster_layout,
            cluster_capacity,
            cluster_pipeline: cluster_pipeline.wait(),
            cluster_group,
            tone: ToneCurve::Neutral,
            land_texture,
            land: [0.0; 4],
            shore_texture,
            shore_of: None,
            far_water: no_far.clone(),
            no_far,
            far_bound: 0,
            terrain_material: u32::MAX,
            air: air.wait(),
            weather: weather.wait(),
            trails: trails.wait(),
            lamp_reach: crate::lamps::LAMP_DISTANCE,
            flicker: true,
            foliage_wind: 1.0,
            webs: crate::webs::Stir::new(device),
            headroom: 1.0,
            water: water.wait(),
            water_on: true,
            cover: None,
            land_footprints: None,
            cover_on: true,
            cover_reach: 1.0,
            cover_height: 1.0,
            surfaces: Surfaces::ALL,
            cover_pipelines,
            cover_draw_pipeline: cover_draw_pipeline.wait(),
            blade_draw_pipeline: blade_draw_pipeline.wait(),
            flower_draw_pipeline: flower_draw_pipeline.wait(),
            stone_draw_pipeline: stone_draw_pipeline.wait(),
            sky_motion_pipeline: sky_motion_pipeline.wait(),
            world_origin: glam::Vec2::ZERO,
            water_layout,
            water_depth_pipeline: water_depth_pipeline.wait(),
            water_pipeline: water_pipeline.wait(),
            water_merge_layout,
            water_merge_pipeline: water_merge_pipeline.wait(),
            far_water_pipeline: far_water_pipeline.wait(),
            puddle_pipeline: puddle_pipeline.wait(),
            cut_ground: true,
            graph,
            frame_layout,
            frame_buffer,
            frame_group,
            scene_group,
            cull_pipeline: cull_pipeline.wait(),
            cull_group,
            cull_hiz_layout,
            depth_pipeline: depth_pipeline.wait(),
            hiz_first_pipeline: hiz_first_pipeline.wait(),
            hiz_down_pipeline: hiz_down_pipeline.wait(),
            hiz_first_layout,
            hiz_down_layout,
            hiz: true,
            lod_bias: 1.0,
            card_edge: 0.0,
            sorted_draws: 0,
            cull_params,
            stats,
            pipelines,
            lighting_layout,
            lighting_pipeline: lighting_pipeline.wait(),
            tonemap_layout,
            tonemap_pipeline: tonemap_pipeline.wait(),
            targets: None,
            prev_view_proj: None,
            cloud_history: 0,
            timed_seen: 0,
            temporal,
            anti_aliasing: true,
            upscaler: crate::temporal::Upscaler::MetalFx,
            render_scale: 1.0,
            dynamic: None,
            ui: None,
            internal: (0, 0),
            output_size: (0, 0),
            selection: Selection::default(),
            oit: false,
            particles_drawn: 0,
            particle_draws: 0,
            particle_buffer,
            particle_pipelines,
            particle_oit_pipelines,
            web_pipelines,
            flame_pipelines,
            bolt_pipelines,
            ice_pipeline: ice_pipeline.wait(),
            blob_pipeline: blob_pipeline.wait(),
            puff_pipeline: puff_pipeline.wait(),
            oit_pipelines,
            oit_layout,
            oit_pipeline: oit_pipeline.wait(),
            pick_pending: None,
        })
    }

    /// Puts the land field on the GPU: the frame's group takes it, and the water its surface.
    /// Takes `cover`, which keeps off the buildings of the land field last given, however late
    /// it comes (a stream's first land can come before its cover, as after a pack is switched).
    pub fn set_cover(&mut self, gpu: &Gpu, mut cover: crate::cover::Cover) {
        if let Some(field) = self.land_footprints.as_ref() {
            cover.set_footprints(gpu, field);
        }
        self.cover = Some(cover);
    }

    pub fn set_land(&mut self, gpu: &Gpu, field: &crate::land::LandField) {
        let (w, h) = (
            u32::try_from(field.side.0).unwrap_or(1),
            u32::try_from(field.side.1).unwrap_or(1),
        );
        self.land_texture = land_texture(gpu, w, h, &field.texels());
        let same = match (&field.shore, &self.shore_of) {
            (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.shore_of.clone_from(&field.shore);
            self.shore_texture = match field.shore.as_deref() {
                Some(s) => shore_texture(
                    gpu,
                    u32::try_from(s.side.0).unwrap_or(1),
                    u32::try_from(s.side.1).unwrap_or(1),
                    &s.texels(),
                ),
                None => shore_texture(gpu, 1, 1, &[0u8; 8]),
            };
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.land = [field.origin.x, field.origin.y, w as f32, h as f32];
        }
        self.frame_group = frame_group(
            gpu,
            &self.frame_layout,
            &self.frame_buffer,
            &self.post,
            &self.atmosphere,
            &self.land_texture,
            &self.shore_texture,
            (&self.far_water.0, &self.far_water.1),
        );
        self.water.set_field(field);
        if let Some(c) = self.cover.as_mut() {
            c.set_footprints(gpu, field);
        }
        self.land_footprints = Some(field.clone());
    }

    /// Binds `far`'s water to the frame's group, when it is not the far terrain bound already.
    fn bind_far(&mut self, gpu: &Gpu, far: Option<&crate::far::FarTerrain>) {
        let id = far.map_or(0, |f| f.id);
        if id == self.far_bound {
            return;
        }
        self.far_bound = id;
        self.far_water = far.map_or_else(
            || self.no_far.clone(),
            |f| {
                let (v, b) = f.water_bindings();
                (v.clone(), b.clone())
            },
        );
        self.frame_group = frame_group(
            gpu,
            &self.frame_layout,
            &self.frame_buffer,
            &self.post,
            &self.atmosphere,
            &self.land_texture,
            &self.shore_texture,
            (&self.far_water.0, &self.far_water.1),
        );
    }

    /// The cluster buffers grown to hold a `width` by `height` target's tiles, before the light
    /// groups that read them are made.
    fn fit_clusters(&mut self, gpu: &Gpu, width: u32, height: u32) {
        let (x, y) = cluster_tiles_for(width, height);
        let tiles = x * y;
        if tiles <= self.cluster_capacity {
            return;
        }
        let (counts, lamps) = cluster_buffers(&gpu.device, tiles);
        self.cluster_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("clusters"),
            layout: &self.cluster_layout,
            entries: &[
                entire(0, &self.lamp_buffer),
                entire(1, &counts),
                entire(2, &lamps),
            ],
        });
        self.cluster_counts = counts;
        self.cluster_lamps = lamps;
        self.cluster_capacity = tiles;
    }

    fn targets(&mut self, gpu: &Gpu, width: u32, height: u32) -> &Targets {
        let stale = self
            .targets
            .as_ref()
            .is_none_or(|t| t.width != width || t.height != height);
        if stale {
            self.fit_clusters(gpu, width, height);
            let make = |label, format| {
                gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                })
            };
            let textures = vec![
                make("albedo", ALBEDO),
                make("normal", NORMAL),
                make("surface", MATERIAL),
                make("motion", MOTION),
                make("ids", IDS),
                make("depth", DEPTH),
                make("hdr", HDR),
                make("objects", IDS),
            ];
            let views: [wgpu::TextureView; 8] =
                std::array::from_fn(|i| textures[i].create_view(&Default::default()));
            let clouds_size = (
                (width * 2).div_ceil(3).max(1),
                (height * 2).div_ceil(3).max(1),
            );
            let cloud_picture = |label| {
                gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: clouds_size.0,
                        height: clouds_size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: crate::clouds::FORMAT,
                    usage: wgpu::TextureUsages::STORAGE_BINDING
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
            };
            let clouds = cloud_picture("clouds");
            let clouds_marched = cloud_picture("clouds marched");
            let clouds_history = cloud_picture("clouds history");
            let clouds_view = clouds.create_view(&Default::default());
            let marched_view = clouds_marched.create_view(&Default::default());
            let history_view = clouds_history.create_view(&Default::default());
            let clouds_group = self.clouds.group(gpu, &views[T_DEPTH], &marched_view);
            let clouds_resolve_group =
                self.clouds
                    .resolve_group(gpu, &marched_view, &history_view, &clouds_view);
            let clouds_keep_group =
                self.clouds
                    .resolve_group(gpu, &clouds_view, &clouds_view, &history_view);
            self.cloud_history = 0;
            let lighting_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("lighting"),
                layout: &self.lighting_layout,
                entries: &[
                    view(0, &views[T_ALBEDO]),
                    view(1, &views[T_NORMAL]),
                    view(2, &views[T_SURFACE]),
                    view(3, &views[T_DEPTH]),
                    view(4, &clouds_view),
                    view(5, &self.clouds.shadow),
                ],
            });
            self.post.resize(
                gpu,
                &views[T_HDR],
                &views[T_DEPTH],
                &views[T_SURFACE],
                width,
                height,
            );
            let bloom = self
                .post
                .bloom_view()
                .unwrap_or_else(|| unreachable!("made by resize"));
            let tonemap_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("tonemap"),
                layout: &self.tonemap_layout,
                entries: &[
                    view(0, &views[T_HDR]),
                    view(1, bloom),
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.post.sampler),
                    },
                    entire(3, &self.post.state),
                    view(4, &views[T_DEPTH]),
                    view(5, &views[T_OBJECTS]),
                ],
            });
            let hiz_size = (width.div_ceil(2).max(1), height.div_ceil(2).max(1));
            let hiz_levels = 32 - hiz_size.0.max(hiz_size.1).leading_zeros();
            let hiz = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth pyramid"),
                size: wgpu::Extent3d {
                    width: hiz_size.0,
                    height: hiz_size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: hiz_levels,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R32Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let level = |k| {
                hiz.create_view(&wgpu::TextureViewDescriptor {
                    base_mip_level: k,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            };
            let levels: Vec<wgpu::TextureView> = (0..hiz_levels).map(level).collect();
            let mut hiz_groups = vec![gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("pyramid level 0"),
                layout: &self.hiz_first_layout,
                entries: &[view(0, &views[T_DEPTH]), view(1, &levels[0])],
            })];
            for k in 1..levels.len() {
                hiz_groups.push(gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("pyramid level"),
                    layout: &self.hiz_down_layout,
                    entries: &[view(0, &levels[k - 1]), view(1, &levels[k])],
                }));
            }
            let whole = hiz.create_view(&Default::default());
            let cull_hiz_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cull pyramid"),
                layout: &self.cull_hiz_layout,
                entries: &[view(0, &whole)],
            });
            let lamp_shadow = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("lamp shadows"),
                size: wgpu::Extent3d {
                    width: width.div_ceil(2).max(1),
                    height: (height * 2).div_ceil(3).max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let lamp_shadow_view = lamp_shadow.create_view(&Default::default());
            let half = |label, format| {
                gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: width.div_ceil(2).max(1),
                        height: height.div_ceil(2).max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::STORAGE_BINDING
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
            };
            let ao = half("ambient occlusion", wgpu::TextureFormat::Rgba8Unorm);
            let ao_raw = half("ambient occlusion search", wgpu::TextureFormat::Rgba16Float);
            let ao_gathered = [
                half(
                    "ambient occlusion gathered",
                    wgpu::TextureFormat::Rgba16Float,
                ),
                half(
                    "ambient occlusion gathered",
                    wgpu::TextureFormat::Rgba16Float,
                ),
            ];
            let ao_view = ao.create_view(&Default::default());
            let ao_raw_view = ao_raw.create_view(&Default::default());
            let gathered_views = ao_gathered
                .each_ref()
                .map(|t| t.create_view(&Default::default()));
            let gtao_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gtao"),
                layout: &self.gtao_layout,
                entries: &[
                    view(0, &views[T_DEPTH]),
                    view(1, &ao_raw_view),
                    entire(2, &self.ao_params),
                ],
            });
            let ao_gather_groups = [0, 1].map(|k| {
                gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("ao gather"),
                    layout: &self.ao_gather_layout,
                    entries: &[
                        view(0, &ao_raw_view),
                        view(1, &gathered_views[1 - k]),
                        view(2, &views[T_MOTION]),
                        view(3, &views[T_DEPTH]),
                        view(4, &gathered_views[k]),
                        entire(5, &self.ao_params),
                    ],
                })
            });
            let ao_blur_groups = [0, 1].map(|k| {
                gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("ao blur"),
                    layout: &self.ao_blur_layout,
                    entries: &[view(0, &gathered_views[k]), view(1, &ao_view)],
                })
            });
            self.ao_frames = 0;
            let scene = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("scene before the water"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HDR,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let scene_view = scene.create_view(&Default::default());
            // The water's own depth, then the depth behind the ice.
            let depth_behind = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth behind the ice"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let behind_view = depth_behind.create_view(&Default::default());
            let water_merge_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("water merge"),
                layout: &self.water_merge_layout,
                entries: &[view(0, &behind_view)],
            });
            // The ice's lights read the depth behind it, as it writes its own.
            let light_groups = [&views[T_DEPTH], &behind_view].map(|depth| {
                gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("light"),
                    layout: &self.light_layout,
                    entries: &[
                        view(0, &self.shadow_atlas.create_view(&Default::default())),
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.shadow_cmp),
                        },
                        entire(2, &self.lamp_buffer),
                        entire(3, &self.cluster_counts),
                        entire(4, &self.cluster_lamps),
                        view(5, &lamp_shadow_view),
                        view(6, &ao_view),
                        entire(7, &self.probe_buffer),
                        entire(8, &self.probe_grid),
                        view(9, depth),
                        view(10, self.weather.shelter_map().1),
                        entire(11, self.webs.buffer()),
                        entire(12, &self.flame_buffer),
                        view(13, &scene_view),
                        view(14, self.trails.view()),
                    ],
                })
            });
            let [light_group, ice_group] = light_groups;
            let oit_targets = [
                plain_target(gpu, "oit sum", OIT_ACCUM, width, height),
                plain_target(gpu, "oit revealage", OIT_REVEAL, width, height),
                plain_target(gpu, "oit additive", HDR, width, height),
            ];
            let oit_views = oit_targets
                .each_ref()
                .map(|t| t.create_view(&Default::default()));
            let oit_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("oit"),
                layout: &self.oit_layout,
                entries: &[
                    view(0, &oit_views[0]),
                    view(1, &oit_views[1]),
                    view(2, &oit_views[2]),
                ],
            });
            let clamp = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("water clamp"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let repeat = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("water repeat"),
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            // The depth pass's groups hold a stand-in for the water's depth it draws into.
            let no_depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("no water depth"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let no_depth_view = no_depth.create_view(&Default::default());
            let water_group = |k: usize, water_depth: &wgpu::TextureView| {
                gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("water"),
                    layout: &self.water_layout,
                    entries: &[
                        view(0, &scene_view),
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&clamp),
                        },
                        view(2, &views[T_SURFACE]),
                        view(3, &views[T_DEPTH]),
                        view(4, &self.water.sim_views[k]),
                        wgpu::BindGroupEntry {
                            binding: 5,
                            resource: wgpu::BindingResource::Sampler(&repeat),
                        },
                        entire(6, &self.water.looks),
                        view(7, &views[T_IDS]),
                        entire(8, &self.water.waves),
                        entire(9, &self.water.nodes),
                        view(10, water_depth),
                        entire(11, &self.water.shore_gain),
                    ],
                })
            };
            let water_groups = [0, 1].map(|k| water_group(k, &behind_view));
            let water_depth_groups = [0, 1].map(|k| water_group(k, &no_depth_view));
            let plain = |label, format, usage| {
                gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
            };
            let ldr = plain(
                "tone mapped",
                crate::temporal::LDR,
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
            );
            let prev_ids = plain(
                "last frame's materials",
                IDS,
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            );
            let reactive = plain(
                "reactive mask",
                wgpu::TextureFormat::R8Unorm,
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            );
            let mut textures = textures;
            textures.push(hiz);
            textures.push(lamp_shadow);
            textures.push(ao);
            textures.push(ao_raw);
            textures.extend(ao_gathered);
            textures.push(clouds);
            textures.push(clouds_marched);
            textures.push(clouds_history);
            self.targets = Some(Targets {
                width,
                height,
                views,
                lighting_group,
                tonemap_group,
                hiz_size,
                hiz_levels,
                hiz_groups,
                cull_hiz_group,
                light_group,
                lamp_shadow: lamp_shadow_view,
                gtao_group,
                ao_gather_groups,
                ao_blur_groups,
                clouds_group,
                clouds_resolve_group,
                clouds_keep_group,
                clouds_size,
                scene,
                water_groups,
                water_depth_groups,
                depth_behind,
                behind_view,
                water_merge_group,
                ice_group,
                oit_views,
                oit_group,
                ldr,
                prev_ids,
                reactive_view: reactive.create_view(&Default::default()),
                reactive,
                textures,
            });
            self.temporal.reset();
        }
        self.targets
            .as_ref()
            .unwrap_or_else(|| unreachable!("made above"))
    }

    /// Draws one frame of `scene` seen by `camera` in `light` to `output`, `width` by `height`.
    ///
    /// # Errors
    /// The scene's layout does not fit its pools, or the material table has no bind group.
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        gpu: &Gpu,
        scene: &Scene,
        materials: &MaterialTable,
        view: &View<'_>,
        output: &wgpu::TextureView,
        output_size: (u32, u32),
    ) -> Result<(), EngineError> {
        let (camera, light) = (view.camera, view.light);
        if self.last_time.is_some_and(|t| {
            let d = (light.time - t).abs();
            d.min(1.0 - d) > TIME_CUT
        }) {
            self.time_cut = TIME_CUT_FRAMES;
            self.probe_frames = 0;
            self.cloud_history = 0;
            self.temporal.reset();
        }
        self.last_time = Some(light.time);
        if self.time_cut > 0 {
            self.time_cut -= 1;
            self.post.snap = true;
        }
        // The size the world is drawn at: the render scale's, or the budget's.
        if let Some(d) = self.dynamic.as_mut() {
            self.graph.collect(gpu, false);
            if self.graph.timed() != self.timed_seen {
                self.timed_seen = self.graph.timed();
                d.observe(self.graph.span_ms());
            }
        }
        let scale = self.dynamic.map_or(self.render_scale, |d| d.scale());
        let (width, height) = crate::temporal::scaled(output_size, scale);
        self.internal = (width, height);
        self.output_size = output_size;
        let how = self.temporal.choose(
            gpu,
            self.upscaler,
            self.anti_aliasing,
            (width, height),
            output_size,
        );
        let jitter = if how.temporal() {
            crate::temporal::jitter(self.temporal.frame, (width, height))
        } else {
            glam::Vec2::ZERO
        };
        let material_group = materials
            .group()
            .ok_or_else(|| EngineError::Device("the material table is not uploaded".into()))?
            .clone();
        #[allow(clippy::cast_precision_loss)]
        let aspect = output_size.0 as f32 / output_size.1.max(1) as f32;
        // Drawn through the jittered projection; culled, sorted and followed through the plain one.
        let plain = camera.view_projection(aspect);
        let view_proj = crate::temporal::jitter_matrix(jitter) * plain;
        let prev = self.prev_view_proj.unwrap_or(plain);
        self.prev_view_proj = Some(plain);
        let reproject = prev * plain.inverse();

        self.weather
            .step(self.clock, crate::wind::wind(light.time, self.clock));
        self.atmosphere.snowfall = self.weather.snow * (0.7 + 1.3 * self.weather.storminess());
        self.trails.on = self.weather.on;
        self.trails.prepare(
            gpu,
            camera.position,
            self.weather.snowfall,
            view.outside && self.weather.snow_share > 0.0 && self.weather.snowy_types != 0,
        );
        let mut frame = Vec::with_capacity(320);
        f32s(&mut frame, &view_proj.to_cols_array());
        f32s(&mut frame, &prev.to_cols_array());
        f32s(&mut frame, &view_proj.inverse().to_cols_array());
        let p = camera.position;
        #[allow(clippy::cast_precision_loss)]
        let size = [width as f32, height as f32];
        let (fog_start, fog_end) = if self.atmosphere.fog_density > 0.0 {
            let end = crate::atmosphere::fog_end_for(light.fog_end, camera.far).min(camera.far);
            let start = light.fog_start.min(end * 0.6);
            (
                start,
                start + (end - start) / self.atmosphere.fog_density.max(1.0),
            )
        } else {
            (1.0e9, 2.0e9)
        };
        let physical = self.calibration == Calibration::Physical;
        let bloom = if physical { self.bloom } else { 0.0 };
        self.bind_far(gpu, view.far);
        self.targets(gpu, width, height);
        for v in [
            [p.x, p.y, p.z, self.tone.word()],
            light.sun_dir.extend(light.daylight).to_array(),
            light.sun.extend(0.0).to_array(),
            light.ambient.extend(light.sun_strength).to_array(),
            light.fog.extend(fog_start).to_array(),
            [fog_end, bloom, size[0], size[1]],
            view.indoor
                .extend(if view.sealed { 1.0 } else { 0.0 })
                .to_array(),
        ] {
            f32s(&mut frame, &v);
        }
        let mut features = 0;
        // The volumetric clouds: the physical sky seen with a deck over it, Cloud Detail on.
        let clouds_on = physical
            && view.outside
            && self.atmosphere.cloud_detail > 0
            && view.heavens.is_some_and(|h| {
                h.deck
                    .is_some_and(|d| d.texture != crate::asset::material::NO_TEXTURE)
            });
        if physical {
            features |= flags::PHYSICAL | flags::ATMOSPHERE;
            let clouds = crate::clouds::CloudFrame {
                on: clouds_on,
                world_origin: self.world_origin,
                storm: crate::clouds::storm(
                    self.weather.rain,
                    self.weather.storm,
                    light.weather.haze().1,
                    self.atmosphere.storm,
                ),
                flash: self.weather.flash,
                close: self.weather.rain.max(self.weather.snow),
                height: (height * 2).div_ceil(3).max(1),
                history: if clouds_on { self.cloud_history } else { 0 },
            };
            self.cloud_history = if clouds_on {
                self.cloud_history.wrapping_add(1).max(1)
            } else {
                0
            };
            self.atmosphere
                .write(gpu, light, camera, view.heavens, self.clock, &clouds);
        }
        let cascades = (physical && self.shadows && view.outside).then(|| {
            crate::shadows::cascades(
                camera,
                aspect,
                light.sun_dir,
                self.shadow_distance.min(camera.far),
            )
        });
        self.cascades = cascades;
        if cascades.is_some() {
            features |= flags::SHADOWS;
        }
        // What stands over the ground round the eye, while anything falls or the ground is wet.
        let shelter = (view.outside && self.weather.sheltering())
            .then(|| crate::weather::Shelter::around(camera.position));
        self.shelter = shelter;
        let puddles_on = shelter.is_some() && self.weather.puddles > 0.001;
        let near_water = self.water.layout.is_some() && self.land[2] > 0.0;
        let far_water = view.far.is_some_and(crate::far::FarTerrain::has_water);
        if physical {
            features |= self.surfaces.flags();
        }
        if physical && self.water_on && view.outside && (near_water || far_water) {
            features |= flags::WATER;
        }
        // The nearest few shadowed lamps whose light reaches near the eye get ray-traced slots.
        let mut slots = [u32::MAX; 4];
        if self.lamps && self.lamp_rays && self.ray_pipeline.is_some() {
            let mut k = 0;
            for (i, l) in view
                .lamps
                .iter()
                .enumerate()
                .take(crate::lamps::MAX_FRAME_LAMPS)
            {
                if k < 4
                    && l.shadows
                    && l.position.distance(camera.position) - l.range < RAY_SHADOW_REACH
                {
                    slots[k] = u32::try_from(i).unwrap_or(u32::MAX);
                    k += 1;
                }
            }
        }
        if self.lamps && !view.lamps.is_empty() {
            features |= flags::LAMPS;
            let bytes: Vec<u8> = view
                .lamps
                .iter()
                .take(crate::lamps::MAX_FRAME_LAMPS)
                .enumerate()
                .flat_map(|(k, l)| {
                    let i = u32::try_from(k).unwrap_or(0);
                    let slot = slots
                        .iter()
                        .position(|&s| s == i)
                        .map_or(255, |s| u32::try_from(s).unwrap_or(255));
                    let seed = if l.seed == 0 {
                        crate::flames::seed_of(l.position)
                    } else {
                        l.seed
                    };
                    crate::lamps::lamp_bytes(l, slot, seed)
                })
                .collect();
            gpu.queue.write_buffer(&self.lamp_buffer, 0, &bytes);
            if slots[0] != u32::MAX {
                features |= flags::LAMP_RAYS;
                let b: Vec<u8> = slots.iter().flat_map(|s| s.to_le_bytes()).collect();
                gpu.queue.write_buffer(&self.ray_slots, 0, &b);
            }
        }
        self.flames_drawn = 0;
        if self.lamps && !view.flames.is_empty() {
            let bytes: Vec<u8> = view
                .flames
                .iter()
                .take(crate::flames::MAX_FRAME_FLAMES)
                .flat_map(|f| crate::flames::flame_bytes(f, self.flicker))
                .collect();
            gpu.queue.write_buffer(&self.flame_buffer, 0, &bytes);
            self.flames_drawn =
                u32::try_from(bytes.len() as u64 / crate::flames::FLAME_SIZE).unwrap_or(0);
        }
        if self.webs.on {
            features |= flags::WEBS;
            self.webs
                .update(&gpu.queue, view.bodies, camera.position, self.clock);
        }
        for v in [
            u32::from(view.outside),
            u32::from(self.cut_ground),
            lod_scale(camera, self.lod_bias).to_bits(),
            features,
        ] {
            frame.extend_from_slice(&v.to_le_bytes());
        }
        #[allow(clippy::cast_possible_truncation)]
        let clock = (self.clock % 86_400.0) as f32;
        // Under the rain's closed sky the sun is mostly hidden; a storm's darkens the sky too.
        let storm = self.weather.storminess();
        let close = self.weather.rain.max(self.weather.snow);
        for v in [
            // Falling snow's cloud is thicker still: the sun's shadows all but go.
            (light.sun_light
                * (1.0 - 0.55 * close)
                * (1.0 - 0.35 * storm)
                * (1.0 - 0.35 * self.weather.snow))
                .extend(clock)
                .to_array(),
            (light.sky_light
                * (1.0 - 0.2 * close)
                * (1.0 - 0.25 * storm)
                * (1.0 + 12.0 * self.weather.flash))
                .extend(light.sky_scale())
                .to_array(),
            view.indoor_physical.extend(0.0).to_array(),
        ] {
            f32s(&mut frame, &v);
        }
        f32s(&mut frame, &camera.view().to_cols_array());
        let cs = cascades.unwrap_or(
            [crate::shadows::Cascade {
                view_proj: Mat4::IDENTITY,
                far: 0.0,
                texel: 1.0,
                depth: 1.0,
                planes: [glam::Vec4::ZERO; 6],
            }; crate::shadows::CASCADES],
        );
        for c in &cs {
            f32s(&mut frame, &c.view_proj.to_cols_array());
        }
        f32s(&mut frame, &cs.map(|c| c.far));
        #[allow(clippy::cast_precision_loss)]
        let texel_uv = 1.0 / crate::shadows::ATLAS as f32;
        f32s(
            &mut frame,
            &[
                texel_uv,
                crate::shadows::FADE,
                crate::shadows::BLEND,
                crate::shadows::PENUMBRA,
            ],
        );
        // The clusters: tiles, slices and lamps; their depths and the view's tangents.
        let tiles = cluster_tiles_for(width, height);
        let lamp_count = if self.lamps {
            view.lamps.len().min(crate::lamps::MAX_FRAME_LAMPS)
        } else {
            0
        };
        self.lamps_lit = lamp_count;
        self.cluster_tiles = tiles;
        self.lamps_drawn.clear();
        self.lamps_drawn
            .extend_from_slice(&view.lamps[..lamp_count]);
        for v in [
            tiles.0,
            tiles.1,
            CLUSTER_SLICES,
            u32::try_from(lamp_count).unwrap_or(0),
        ] {
            frame.extend_from_slice(&v.to_le_bytes());
        }
        let ty = pyreal_primitives::num::math::tanf(camera.fov_y * 0.5);
        f32s(
            &mut frame,
            &[
                CLUSTER_NEAR,
                self.lamp_reach.max(CLUSTER_NEAR * 2.0),
                ty * aspect,
                ty,
            ],
        );
        f32s(&mut frame, &cs.map(|c| c.texel));
        f32s(&mut frame, &cs.map(|c| c.depth));
        let stride = u32::try_from(scene.sizes.visible).unwrap_or(u32::MAX);
        for v in [
            stride,
            CLUSTER_TILE,
            self.terrain_material,
            self.headroom.max(1.0).to_bits(),
        ] {
            frame.extend_from_slice(&v.to_le_bytes());
        }
        f32s(&mut frame, &self.land);
        // A storm's gale sways the trees and cover twice as hard and more, gusting high.
        let storm = self.weather.storminess();
        let wind = crate::wind::wind(light.time, self.clock);
        let sway = self.foliage_wind * (1.0 + 1.3 * storm);
        #[allow(clippy::cast_possible_truncation)]
        let last_clock = (self.last_clock.unwrap_or(self.clock) % 86_400.0) as f32;
        #[allow(clippy::cast_possible_truncation)]
        let last_lap = (self.last_clock.unwrap_or(self.clock) % 600.0) as f32;
        let gust = wind.z + (1.0 - wind.z) * storm * 0.6;
        self.water.gust = gust;
        f32s(
            &mut frame,
            &[wind.x * sway, wind.y * sway, gust, last_clock],
        );
        self.last_clock = Some(self.clock);
        let water_on = physical && self.water_on && view.outside && (near_water || far_water);
        if water_on && near_water {
            self.water
                .update(gpu, camera.position, &camera.frustum(aspect));
        } else {
            self.water.node_count = 0;
        }
        let ripple_steps = if water_on {
            self.water.prepare(
                gpu,
                self.clock,
                self.world_origin,
                camera.position,
                view.bodies,
                view.body_water,
                self.weather.rain,
                near_water,
            )
        } else {
            0
        };
        let ripples = match (water_on, self.water.area) {
            (true, Some(a)) => [a[0], a[1], a[2], 1.0],
            _ => [0.0; 4],
        };
        f32s(&mut frame, &ripples);
        // The surfaces' depth, and the nearest lamps their relief is shaded toward.
        // Render space's origin block, which what is laid on the world is laid from.
        let block =
            |m: f32| f32::from_bits(pyreal_primitives::num::to_i32(m / 192.0).cast_unsigned());
        f32s(
            &mut frame,
            &[
                self.surfaces.relief_level()[0],
                DETAIL_REACH,
                block(self.world_origin.x),
                block(self.world_origin.y),
            ],
        );
        let mut power = [0.0; KEY_LAMPS];
        let mut keys = [[0.0f32; 4]; KEY_LAMPS];
        if self.lamps {
            for (k, l) in view
                .lamps
                .iter()
                .filter(|l| l.range > 0.0 && l.candela > 0.0)
                .take(KEY_LAMPS)
                .enumerate()
            {
                keys[k] = l.position.extend(l.range).to_array();
                power[k] = l.candela;
            }
        }
        for k in &keys {
            f32s(&mut frame, k);
        }
        f32s(&mut frame, &power);
        // zw: how wet the ground is, and the lightning.
        f32s(
            &mut frame,
            &[jitter.x, jitter.y, self.weather.wet, self.weather.flash],
        );
        let sel = self.selection;
        let plus_one = |o: Option<u32>| o.map_or(0, |o| o.wrapping_add(1));
        for v in [
            plus_one(sel.selected),
            plus_one(sel.hovered),
            u32::from(sel.ring.is_some() && sel.selected.is_some()),
            sel.colour,
        ] {
            frame.extend_from_slice(&v.to_le_bytes());
        }
        let (at, r) = sel.ring.unwrap_or((glam::Vec3::ZERO, 0.0));
        f32s(&mut frame, &at.extend(r).to_array());
        #[allow(clippy::cast_possible_truncation)] // a fraction in [0, 1), and seconds
        let (lap, lap_seconds) = (
            ((self.clock % 600.0) / 600.0) as f32,
            (self.clock % 600.0) as f32,
        );
        f32s(
            &mut frame,
            &[
                crate::water::surf_phase(self.clock),
                lap,
                lap_seconds,
                last_lap,
            ],
        );
        // The shelter map's place, and the rain on the ground.
        let (shelter_at, span) = shelter.map_or(([0.0; 4], 0.0), |s| {
            (
                [s.centre.x, s.centre.y, crate::weather::SHELTER_REACH, s.top],
                2.0 * crate::weather::SHELTER_DEPTH,
            )
        });
        f32s(&mut frame, &shelter_at);
        #[allow(clippy::cast_possible_truncation)]
        let ripple_clock = (self.clock % crate::weather::RIPPLE_WRAP) as f32;
        f32s(
            &mut frame,
            &[span, ripple_clock, self.weather.rain, self.weather.puddles],
        );
        let relief = self.surfaces.relief_level();
        // w: the terrain types the drawn cover grows on, whose ground the relief only bends.
        let covered = match &self.cover {
            Some(c) if self.cover_on => c.kinds.iter().fold(0, |m, k| m | k.grounds),
            _ => 0,
        };
        f32s(
            &mut frame,
            &[relief[1], relief[2], relief[3], f32::from_bits(covered)],
        );
        // The snow: its terrain types, the fresh cover, how hard it snows, and where the
        // instance pool's dynamic slots start (what moves takes no snow).
        f32s(
            &mut frame,
            &[
                // None while no snowy land is near: the lighting asks nothing of the land then.
                f32::from_bits(if self.weather.on && self.weather.snow_share > 0.0 {
                    self.weather.snowy_types
                } else {
                    0
                }),
                self.weather.cover,
                self.weather.snow,
                f32::from_bits(scene.dynamic_base()),
            ],
        );
        f32s(
            &mut frame,
            &self.trails.frame_words(self.weather.soft_types),
        );
        // The shadows' pools of darkness, under the enhanced lighting.
        if physical {
            for u in view.umbrae.iter().take(UMBRAE) {
                f32s(&mut frame, &u.extend(UMBRA_RADIUS).to_array());
            }
        }
        frame.resize(usize::try_from(FRAME_SIZE).unwrap_or(0), 0);
        gpu.queue.write_buffer(&self.frame_buffer, 0, &frame);

        let dt = self.frame_seconds.unwrap_or_else(|| {
            self.last_frame
                .map_or(0.0, |t| t.elapsed().as_secs_f32().min(0.25))
        });
        self.last_frame = Some(std::time::Instant::now());
        let exposure = match (physical, self.exposure) {
            (false, Exposure::Auto { .. }) => Exposure::Manual(1.0),
            // The eye does not wholly adapt to a storm's gloom: it reads darker than rain.
            // Falling snow's bright overcast reads white, not grey.
            (true, Exposure::Auto { compensation }) if view.outside => Exposure::Auto {
                compensation: compensation - 0.6 * self.weather.storminess()
                    + 0.5 * self.weather.snow,
            },
            (_, e) => e,
        };
        let Some(t) = self.targets.as_ref() else {
            return Ok(());
        };
        // The ground hides things only when the outside is seen and drawn first.
        let ground = view.outside && self.hiz;
        let mut params = Vec::with_capacity(208);
        for plane in camera.frustum(aspect) {
            f32s(&mut params, &plane.to_array());
        }
        f32s(&mut params, &plain.to_cols_array());
        let range = scene.cull_range();
        for v in [
            range.static_count,
            u32::from(ground),
            t.hiz_levels,
            range.dynamic_count,
        ] {
            params.extend_from_slice(&v.to_le_bytes());
        }
        // Screen size: the same object at the same distance looks larger through a narrower view.
        let lod_scale = lod_scale(camera, self.lod_bias);
        #[allow(clippy::cast_precision_loss)]
        f32s(
            &mut params,
            &[
                t.hiz_size.0 as f32,
                t.hiz_size.1 as f32,
                lod_scale,
                f32::from_bits(range.dynamic_base),
            ],
        );
        f32s(
            &mut params,
            &camera.position.extend(self.card_edge).to_array(),
        );
        params.extend_from_slice(&[0u8; 16]);
        gpu.queue.write_buffer(&self.cull_params, 0, &params);
        // Each cascade's: its box, the eye's degrade, its own arguments and list.
        let bucket_count = u32::try_from(scene.buckets().len()).unwrap_or(0);
        let list = u32::try_from(scene.sizes.visible).unwrap_or(u32::MAX);
        if let Some(cs) = self.cascades.as_ref() {
            let mut bytes =
                vec![0u8; usize::try_from(SHADOW_PARAMS_STRIDE).unwrap_or(256) * cs.len()];
            for (c, cascade) in cs.iter().enumerate() {
                let mut p = Vec::with_capacity(224);
                for plane in cascade.planes {
                    f32s(&mut p, &plane.to_array());
                }
                f32s(&mut p, &cascade.view_proj.to_cols_array());
                for v in [range.static_count, 0, t.hiz_levels, range.dynamic_count] {
                    p.extend_from_slice(&v.to_le_bytes());
                }
                #[allow(clippy::cast_precision_loss)]
                f32s(
                    &mut p,
                    &[
                        t.hiz_size.0 as f32,
                        t.hiz_size.1 as f32,
                        lod_scale,
                        f32::from_bits(range.dynamic_base),
                    ],
                );
                // A caster wholly past its receivers, toward the sun, may be a picture.
                let reach = crate::shadows::CASTER_REACH;
                f32s(&mut p, &camera.position.extend(reach).to_array());
                let k = u32::try_from(c).unwrap_or(0);
                let coarser = caster_degrade(c, self.lod_bias).to_bits();
                for v in [1, k * bucket_count, (k + 1) * list, coarser] {
                    p.extend_from_slice(&v.to_le_bytes());
                }
                let at = c * usize::try_from(SHADOW_PARAMS_STRIDE).unwrap_or(256);
                bytes[at..at + p.len()].copy_from_slice(&p);
            }
            gpu.queue.write_buffer(&self.shadow_params, 0, &bytes);
        }
        // The shelter map's: straight down over its square, the world's statics alone (what walks
        // about shelters nothing), the rooms too, each at the eye's degrade.
        if let Some(s) = shelter {
            let view_proj = s.view_proj();
            let mut p = Vec::with_capacity(224);
            for plane in crate::shadows::planes(&view_proj) {
                f32s(&mut p, &plane.to_array());
            }
            f32s(&mut p, &view_proj.to_cols_array());
            for v in [range.static_count, 0, t.hiz_levels, 0] {
                p.extend_from_slice(&v.to_le_bytes());
            }
            #[allow(clippy::cast_precision_loss)]
            f32s(
                &mut p,
                &[
                    t.hiz_size.0 as f32,
                    t.hiz_size.1 as f32,
                    lod_scale,
                    f32::from_bits(range.dynamic_base),
                ],
            );
            f32s(&mut p, &camera.position.extend(0.0).to_array());
            let k = u32::try_from(crate::scene::SHADOW_LISTS - 1).unwrap_or(0);
            for v in [2, k * bucket_count, (k + 1) * list, 1.0f32.to_bits()] {
                p.extend_from_slice(&v.to_le_bytes());
            }
            gpu.queue
                .write_buffer(&self.shadow_params, u64::from(k) * SHADOW_PARAMS_STRIDE, &p);
        }
        // Translucent draws far to near, each object whole, from the visible list's tail.
        let sorted = scene.sorted_translucent(
            camera.position,
            &camera.frustum(aspect),
            view.rects,
            &plain,
            lod_scale,
        );
        let sorted_base = scene.sorted_base();
        self.sorted_draws = sorted.len();
        let blobs = view.particles.map_or((0, 0), |p| p.blobs);
        let puffs = view.particles.map_or((0, 0), |p| p.puffs);
        let bolts = view.particles.map_or((0, 0), |p| p.bolts);
        let ice_drawn = blobs.1 > 0
            || puffs.1 > 0
            || sorted
                .iter()
                .any(|(_, d, _)| d.pipeline == PipelineKind::Ice);
        if !sorted.is_empty() {
            let bytes: Vec<u8> = sorted
                .iter()
                .flat_map(|(i, d, _)| [*i, d.material])
                .flat_map(u32::to_le_bytes)
                .collect();
            gpu.queue
                .write_buffer(&scene.visible, u64::from(sorted_base) * 8, &bytes);
        }
        let batches: &[crate::particles::Batch] = view.particles.map_or(&[], |p| &p.batches);
        self.particle_draws = batches.len();
        self.particles_drawn = view.particles.map_or(0, |p| p.drawn.len());
        if let Some(p) = view
            .particles
            .filter(|_| !batches.is_empty() || blobs.1 > 0 || puffs.1 > 0 || bolts.1 > 0)
        {
            gpu.queue.write_buffer(&self.particle_buffer, 0, &p.bytes());
        }
        let buckets = scene.buckets();
        let mut enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        enc.clear_buffer(&self.stats, 0, None);
        if let Some(c) = self.cover.as_mut() {
            let detail = if self.lod_bias > 0.0 {
                self.lod_bias
            } else {
                4.0
            };
            c.fit(gpu, &self.cover_pipelines, self.cover_reach, detail);
            c.height = self.cover_height;
            c.set_objects(gpu, view.clearings);
        }
        let cover = self
            .cover
            .as_ref()
            .filter(|_| self.cover_on && view.outside && self.land[2] > 0.0);
        if let Some(c) = cover {
            c.prepare(
                gpu,
                &mut enc,
                &camera.frustum(aspect),
                camera.position,
                self.world_origin,
                view.bodies,
            );
        }
        let most = if view.sealed {
            crate::post::ROOM_EXPOSURE
        } else {
            f32::MAX
        };
        self.post.prepare(gpu, &mut enc, exposure, most, dt);
        let wants_bounce = physical && self.bounce && self.probe_pipeline.is_some();
        let rays_ready = (features & flags::LAMP_RAYS != 0 || wants_bounce)
            && self
                .rays
                .update(gpu, scene, materials, camera.position, &mut enc);
        if physical && self.ambient_occlusion {
            features |= flags::AO;
        }
        let ao_slot = (self.ao_frames % 2) as usize;
        if features & flags::AO != 0 {
            let words = [self.ao_frames, u32::from(self.ao_frames == 0), 0, 0];
            let b: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
            gpu.queue.write_buffer(&self.ao_params, 0, &b);
            self.ao_frames = self.ao_frames.wrapping_add(1).max(1);
        } else {
            self.ao_frames = 0;
        }
        let bouncing = wants_bounce && rays_ready;
        if bouncing {
            // The grid round the eye, a cell at a time; the first frames trace every probe.
            let cell = |v: f32, s: f32| pyreal_primitives::num::floor_to_i32(v / s);
            let half = |d: u32| i32::try_from(d / 2).unwrap_or(0);
            let lit: Vec<u8> = view
                .probe_lamps
                .iter()
                .take(crate::lamps::PROBE_LAMPS)
                .flat_map(|l| crate::lamps::lamp_bytes(l, 255, 0))
                .collect();
            if self.lamps && !lit.is_empty() {
                gpu.queue.write_buffer(&self.probe_lamps, 0, &lit);
            }
            let base = [
                cell(camera.position.x, PROBE_SPACING.0) - half(PROBE_DIMS.0),
                cell(camera.position.y, PROBE_SPACING.1) - half(PROBE_DIMS.1),
                cell(camera.position.z, PROBE_SPACING.2) - half(PROBE_DIMS.2),
                if self.lamps {
                    i32::try_from(lit.len() / 64).unwrap_or(0)
                } else {
                    0
                },
            ];
            let share = if self.probe_frames < 8 {
                1
            } else {
                PROBE_SHARE
            };
            let mut g = Vec::with_capacity(48);
            for v in base {
                g.extend_from_slice(&v.to_le_bytes());
            }
            for v in [
                PROBE_DIMS.0,
                PROBE_DIMS.1,
                PROBE_DIMS.2,
                share | (self.probe_frames << 8),
            ] {
                g.extend_from_slice(&v.to_le_bytes());
            }
            f32s(
                &mut g,
                &[
                    PROBE_SPACING.0,
                    PROBE_SPACING.1,
                    PROBE_SPACING.2,
                    PROBE_BLEND,
                ],
            );
            gpu.queue.write_buffer(&self.probe_grid, 0, &g);
            // Traced before the lighting: the first frame's every probe is ready for it.
            features |= flags::GI;
        }
        let mut probe_share = 0;
        if bouncing {
            probe_share = if self.probe_frames < 8 {
                1
            } else {
                PROBE_SHARE
            };
            self.probe_frames += 1;
        }
        let lamp_rays_ready = rays_ready && features & flags::LAMP_RAYS != 0;
        {
            let w = usize::try_from(FRAME_WORD_FLAGS).unwrap_or(0);
            if !lamp_rays_ready {
                features &= !flags::LAMP_RAYS;
            }
            frame[w..w + 4].copy_from_slice(&features.to_le_bytes());
            gpu.queue.write_buffer(&self.frame_buffer, 0, &frame);
        }
        if !buckets.is_empty() {
            enc.copy_buffer_to_buffer(
                &scene.args_template,
                0,
                &scene.args,
                0,
                buckets.len() as u64 * ARGS_SIZE,
            );
            if self.cascades.is_some() || shelter.is_some() {
                enc.copy_buffer_to_buffer(
                    &scene.shadow_args_template,
                    0,
                    &scene.shadow_args,
                    0,
                    buckets.len() as u64 * ARGS_SIZE * crate::scene::SHADOW_LISTS,
                );
            }
        }
        let mut rec = self.graph.begin(&mut enc);
        {
            let mut pass = rec.compute("atmosphere");
            if physical {
                self.atmosphere.encode(&mut pass);
            }
        }
        {
            // The ground alone into depth: the near cells (rooms cut out) and the far rings.
            let mut pass = rec.render(
                "prepass",
                &[],
                Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.views[T_DEPTH],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
            );
            if ground {
                pass.set_pipeline(&self.depth_pipeline);
                pass.set_bind_group(0, &self.frame_group, &[]);
                pass.set_bind_group(1, &self.scene_group, &[]);
                pass.set_bind_group(2, &material_group, &[]);
                pass.set_vertex_buffer(0, scene.vertices.buffer.slice(..));
                pass.set_index_buffer(scene.indices.buffer.slice(..), wgpu::IndexFormat::Uint32);
                for g in scene.ground() {
                    pass.draw_indexed(
                        g.first_index..g.first_index + g.index_count,
                        0,
                        g.instance..g.instance + 1,
                    );
                }
                if let Some(far) = view.far {
                    far.draw_depth(&mut pass, &self.frame_group);
                }
            }
        }
        {
            let mut pass = rec.compute("hiz");
            if ground {
                for (k, group) in t.hiz_groups.iter().enumerate() {
                    let pipeline = if k == 0 {
                        &self.hiz_first_pipeline
                    } else {
                        &self.hiz_down_pipeline
                    };
                    let shift = u32::try_from(k).unwrap_or(0);
                    let (w, h) = (
                        (t.hiz_size.0 >> shift).max(1),
                        (t.hiz_size.1 >> shift).max(1),
                    );
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, group, &[]);
                    pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
                }
            }
        }
        {
            let mut pass = rec.compute("cull");
            pass.set_pipeline(&self.cull_pipeline);
            pass.set_bind_group(0, &self.cull_group, &[]);
            pass.set_bind_group(1, &t.cull_hiz_group, &[]);
            pass.dispatch_workgroups(range.total().div_ceil(64), 1, 1);
        }
        {
            let mut pass = rec.compute("cover");
            if let Some(c) = cover {
                c.encode(&mut pass, &self.cover_pipelines, &self.frame_group);
            }
        }
        let draw = |pass: &mut wgpu::RenderPass<'_>, deferred: bool| {
            pass.set_bind_group(0, &self.frame_group, &[]);
            pass.set_bind_group(1, &self.scene_group, &[]);
            pass.set_bind_group(2, &material_group, &[]);
            pass.set_vertex_buffer(0, scene.vertices.buffer.slice(..));
            pass.set_index_buffer(scene.indices.buffer.slice(..), wgpu::IndexFormat::Uint32);
            let mut current = None;
            for (k, b) in buckets.iter().enumerate() {
                // Translucent buckets are filled but drawn from the sorted list instead.
                if !deferred || !b.draw.pipeline.deferred() {
                    continue;
                }
                if current != Some(b.draw.pipeline) {
                    current = Some(b.draw.pipeline);
                    if let Some((_, p)) = self
                        .pipelines
                        .iter()
                        .find(|(kind, _)| *kind == b.draw.pipeline)
                    {
                        pass.set_pipeline(p);
                    }
                }
                pass.draw_indexed_indirect(&scene.args, k as u64 * ARGS_SIZE);
            }
        };
        fn clear(
            v: &wgpu::TextureView,
            c: wgpu::Color,
        ) -> Option<wgpu::RenderPassColorAttachment<'_>> {
            Some(wgpu::RenderPassColorAttachment {
                view: v,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(c),
                    store: wgpu::StoreOp::Store,
                },
            })
        }
        let black = wgpu::Color::TRANSPARENT;
        {
            let mut pass = rec.render(
                "gbuffer",
                &[
                    clear(&t.views[T_ALBEDO], black),
                    clear(&t.views[T_NORMAL], black),
                    clear(&t.views[T_SURFACE], black),
                    clear(&t.views[T_MOTION], black),
                    clear(&t.views[T_IDS], black),
                    clear(&t.views[T_OBJECTS], black),
                ],
                Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.views[T_DEPTH],
                    // Cleared: the prepass drew all the ground, some of which the cull set
                    // leaves out (outside a doorway's rectangle).
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
            );
            draw(&mut pass, true);
            if let Some(c) = cover {
                pass.set_pipeline(&self.cover_draw_pipeline);
                pass.set_bind_group(3, &c.draw_group, &[]);
                for k in 0..c.draws.len() as u64 {
                    pass.draw_indexed_indirect(&c.args, k * ARGS_SIZE);
                }
                pass.set_pipeline(&self.blade_draw_pipeline);
                pass.draw_indirect(&c.blade_args, 0);
                pass.draw_indirect(&c.blade_args, 16);
                pass.set_pipeline(&self.flower_draw_pipeline);
                pass.draw_indirect(&c.blade_args, 32);
                pass.set_pipeline(&self.stone_draw_pipeline);
                pass.draw_indirect(&c.blade_args, 48);
            }
            if let (Some(far), true) = (view.far, view.outside) {
                far.draw(&mut pass, &self.frame_group);
            }
            pass.set_pipeline(&self.sky_motion_pipeline);
            pass.set_bind_group(0, &self.frame_group, &[]);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = rec.compute("shadow-cull");
            if self.cascades.is_some() || shelter.is_some() {
                pass.set_pipeline(&self.cull_pipeline);
                pass.set_bind_group(1, &t.cull_hiz_group, &[]);
                let (suns, last) = self.shadow_cull_groups.split_at(crate::shadows::CASCADES);
                if self.cascades.is_some() {
                    for group in suns {
                        pass.set_bind_group(0, group, &[]);
                        pass.dispatch_workgroups(range.total().div_ceil(64), 1, 1);
                    }
                }
                if let (Some(group), true) = (last.first(), shelter.is_some()) {
                    pass.set_bind_group(0, group, &[]);
                    pass.dispatch_workgroups(range.static_count.div_ceil(64), 1, 1);
                }
            }
        }
        {
            let atlas = self.shadow_atlas.create_view(&Default::default());
            let mut pass = rec.render(
                "shadows",
                &[],
                Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &atlas,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
            );
            if self.cascades.is_some() {
                pass.set_bind_group(0, &self.frame_group, &[]);
                pass.set_bind_group(1, &self.scene_group, &[]);
                pass.set_bind_group(2, &material_group, &[]);
                pass.set_vertex_buffer(0, scene.vertices.buffer.slice(..));
                pass.set_index_buffer(scene.indices.buffer.slice(..), wgpu::IndexFormat::Uint32);
                #[allow(clippy::cast_precision_loss)]
                let half = (crate::shadows::ATLAS / 2) as f32;
                let n = buckets.len() as u64;
                for c in 0..crate::shadows::CASCADES as u64 {
                    #[allow(clippy::cast_precision_loss)]
                    let (x, y) = ((c & 1) as f32 * half, (c >> 1) as f32 * half);
                    pass.set_viewport(x, y, half, half, 0.0, 1.0);
                    let mut current = None;
                    for (k, b) in buckets.iter().enumerate() {
                        if !b.draw.pipeline.deferred() && b.draw.pipeline != PipelineKind::Ice {
                            continue;
                        }
                        let which = usize::from(b.draw.pipeline == PipelineKind::Terrain);
                        if current != Some(which) {
                            current = Some(which);
                            pass.set_pipeline(&self.shadow_pipelines[which]);
                        }
                        pass.draw_indexed_indirect(
                            &scene.shadow_args,
                            (c * n + k as u64) * ARGS_SIZE,
                        );
                    }
                }
            }
        }
        {
            // The shelter map; on a frame without it, a texel stood in its place.
            let (_, map, none) = self.weather.shelter_map();
            let mut pass = rec.render(
                "shelter",
                &[],
                Some(wgpu::RenderPassDepthStencilAttachment {
                    view: if shelter.is_some() { map } else { none },
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: if shelter.is_some() {
                            wgpu::StoreOp::Store
                        } else {
                            wgpu::StoreOp::Discard
                        },
                    }),
                    stencil_ops: None,
                }),
            );
            if shelter.is_some() {
                pass.set_pipeline(&self.shelter_pipeline);
                pass.set_bind_group(0, &self.frame_group, &[]);
                pass.set_bind_group(1, &self.scene_group, &[]);
                pass.set_bind_group(2, &material_group, &[]);
                pass.set_vertex_buffer(0, scene.vertices.buffer.slice(..));
                pass.set_index_buffer(scene.indices.buffer.slice(..), wgpu::IndexFormat::Uint32);
                let n = buckets.len() as u64;
                let c = crate::scene::SHADOW_LISTS - 1;
                for (k, b) in buckets.iter().enumerate() {
                    // The ground is what the rain falls on, not what shelters it.
                    if !b.draw.pipeline.deferred() || b.draw.pipeline == PipelineKind::Terrain {
                        continue;
                    }
                    pass.draw_indexed_indirect(&scene.shadow_args, (c * n + k as u64) * ARGS_SIZE);
                }
            }
        }
        {
            // The prints in the snow, stamped afresh.
            let drawn = self.trails.drawn();
            let mut pass = rec.render(
                "trails",
                &[Some(wgpu::RenderPassColorAttachment {
                    view: self.trails.view(),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: if drawn {
                            wgpu::StoreOp::Store
                        } else {
                            wgpu::StoreOp::Discard
                        },
                    },
                })],
                None,
            );
            self.trails.draw(&mut pass);
        }
        {
            let mut pass = rec.compute("clusters");
            if features & flags::LAMPS != 0 {
                pass.set_pipeline(&self.cluster_pipeline);
                pass.set_bind_group(0, &self.frame_group, &[]);
                pass.set_bind_group(1, &self.cluster_group, &[]);
                let tiles = cluster_tiles_for(width, height);
                pass.dispatch_workgroups(tiles.0, tiles.1, CLUSTER_SLICES);
            }
        }
        {
            let mut pass = rec.compute("lamp-shadows");
            if let (true, Some((pipeline, layout)), Some(tlas)) = (
                lamp_rays_ready,
                self.ray_pipeline.as_ref(),
                self.rays.tlas(),
            ) {
                let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("lamp rays"),
                    layout,
                    entries: &[
                        self::view(0, &t.views[T_DEPTH]),
                        self::view(1, &t.views[T_NORMAL]),
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::AccelerationStructure(tlas),
                        },
                        self::view(3, &t.lamp_shadow),
                        entire(4, &self.lamp_buffer),
                        entire(5, &self.ray_slots),
                    ],
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &self.frame_group, &[]);
                pass.set_bind_group(1, &group, &[]);
                pass.dispatch_workgroups(
                    width.div_ceil(2).div_ceil(8),
                    height.div_ceil(2).div_ceil(8),
                    1,
                );
            }
        }
        {
            let mut pass = rec.compute("gtao");
            if features & flags::AO != 0 {
                pass.set_pipeline(&self.gtao_pipeline);
                pass.set_bind_group(0, &self.frame_group, &[]);
                pass.set_bind_group(1, &t.gtao_group, &[]);
                pass.dispatch_workgroups(
                    width.div_ceil(2).div_ceil(8),
                    height.div_ceil(2).div_ceil(8),
                    1,
                );
            }
        }
        for (name, pipeline, groups) in [
            ("ao gather", &self.ao_gather_pipeline, &t.ao_gather_groups),
            ("ao blur", &self.ao_blur_pipeline, &t.ao_blur_groups),
        ] {
            let mut pass = rec.compute(name);
            if features & flags::AO != 0 {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &self.frame_group, &[]);
                pass.set_bind_group(1, &groups[ao_slot], &[]);
                pass.dispatch_workgroups(
                    width.div_ceil(2).div_ceil(8),
                    height.div_ceil(2).div_ceil(8),
                    1,
                );
            }
        }
        {
            let mut pass = rec.compute("probes");
            if let (true, Some((pipeline, layout)), Some(tlas), Some(table), Some(draws)) = (
                probe_share > 0,
                self.probe_pipeline.as_ref(),
                self.rays.tlas(),
                self.rays.instance_table.as_ref(),
                self.rays.draws.as_ref(),
            ) {
                let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("probes"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::AccelerationStructure(tlas),
                        },
                        entire(1, &self.probe_buffer),
                        entire(2, &scene.vertices.buffer),
                        entire(3, &scene.indices.buffer),
                        entire(4, table),
                        entire(5, draws),
                        entire(6, &self.probe_grid),
                        entire(7, &self.probe_lamps),
                    ],
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &self.frame_group, &[]);
                pass.set_bind_group(1, &group, &[]);
                pass.set_bind_group(2, &material_group, &[]);
                let total = PROBE_DIMS.0 * PROBE_DIMS.1 * PROBE_DIMS.2;
                pass.dispatch_workgroups(total.div_ceil(probe_share) + PROBE_NEAR, 1, 1);
            }
        }
        {
            let mut pass = rec.compute("ripples");
            if ripple_steps > 0 {
                self.water
                    .encode(&mut pass, &self.frame_group, ripple_steps);
            }
        }
        {
            let mut pass = rec.compute("clouds");
            if clouds_on {
                pass.set_pipeline(&self.clouds.pipeline);
                pass.set_bind_group(0, &self.frame_group, &[]);
                pass.set_bind_group(1, &t.clouds_group, &[]);
                pass.set_bind_group(2, &material_group, &[]);
                let (x, y) = (t.clouds_size.0.div_ceil(8), t.clouds_size.1.div_ceil(8));
                pass.dispatch_workgroups(x, y, 1);
                pass.set_pipeline(&self.clouds.resolve_pipeline);
                pass.set_bind_group(1, &t.clouds_resolve_group, &[]);
                pass.dispatch_workgroups(x, y, 1);
                pass.set_pipeline(&self.clouds.keep_pipeline);
                pass.set_bind_group(1, &t.clouds_keep_group, &[]);
                pass.dispatch_workgroups(x, y, 1);
                pass.set_pipeline(&self.clouds.shadow_pipeline);
                pass.set_bind_group(1, &t.clouds_group, &[]);
                let side = crate::clouds::SHADOW_SIZE.div_ceil(8);
                pass.dispatch_workgroups(side, side, 1);
            }
        }
        {
            let mut pass = rec.render("lighting", &[clear(&t.views[T_HDR], black)], None);
            pass.set_pipeline(&self.lighting_pipeline);
            pass.set_bind_group(0, &self.frame_group, &[]);
            pass.set_bind_group(1, &t.lighting_group, &[]);
            pass.set_bind_group(2, &material_group, &[]);
            pass.set_bind_group(3, &t.light_group, &[]);
            pass.draw(0..3, 0..1);
        }
        if features & flags::WATER != 0 || puddles_on {
            rec.encoder().copy_texture_to_texture(
                t.textures[T_HDR].as_image_copy(),
                t.scene.as_image_copy(),
                t.scene.size(),
            );
        }
        let water_patches = features & flags::WATER != 0 && self.water.node_count > 0;
        {
            // The water's own depth and its motion, the world's depth read beside it.
            let mut pass = rec.render(
                "water depth",
                &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.views[T_MOTION],
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.behind_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
            );
            if water_patches {
                pass.set_pipeline(&self.water_depth_pipeline);
                pass.set_bind_group(0, &self.frame_group, &[]);
                pass.set_bind_group(1, &t.water_depth_groups[self.water.current], &[]);
                pass.set_bind_group(2, &material_group, &[]);
                pass.set_bind_group(3, &t.light_group, &[]);
                self.water.draw(&mut pass);
            }
        }
        {
            let keep = |v| {
                Some(wgpu::RenderPassColorAttachment {
                    view: v,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })
            };
            // The reactive mask begins here, the water marking it first.
            let mut pass = rec.render(
                "water",
                &[
                    keep(&t.views[T_HDR]),
                    clear(&t.reactive_view, wgpu::Color::TRANSPARENT),
                ],
                None,
            );
            if water_patches {
                pass.set_pipeline(&self.water_pipeline);
                pass.set_bind_group(0, &self.frame_group, &[]);
                pass.set_bind_group(1, &t.water_groups[self.water.current], &[]);
                pass.set_bind_group(2, &material_group, &[]);
                pass.set_bind_group(3, &t.light_group, &[]);
                pass.draw(0..3, 0..1);
            }
            if features & flags::WATER != 0
                && view.far.is_some_and(crate::far::FarTerrain::has_water)
            {
                pass.set_pipeline(&self.far_water_pipeline);
                pass.set_bind_group(0, &self.frame_group, &[]);
                pass.set_bind_group(1, &t.water_groups[self.water.current], &[]);
                pass.set_bind_group(2, &material_group, &[]);
                pass.set_bind_group(3, &t.light_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        {
            let mut pass = rec.render(
                "water merge",
                &[],
                Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.views[T_DEPTH],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
            );
            if water_patches {
                pass.set_pipeline(&self.water_merge_pipeline);
                pass.set_bind_group(0, &t.water_merge_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        {
            let mut pass = rec.render(
                "puddles",
                &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.views[T_HDR],
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                None,
            );
            if puddles_on {
                pass.set_pipeline(&self.puddle_pipeline);
                pass.set_bind_group(0, &self.frame_group, &[]);
                pass.set_bind_group(1, &t.water_groups[self.water.current], &[]);
                pass.set_bind_group(2, &material_group, &[]);
                pass.set_bind_group(3, &t.light_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        {
            // The ice reads the frame and the depth behind it, then writes its own depth.
            if ice_drawn {
                rec.encoder().copy_texture_to_texture(
                    t.textures[T_HDR].as_image_copy(),
                    t.scene.as_image_copy(),
                    t.scene.size(),
                );
                rec.encoder().copy_texture_to_texture(
                    t.textures[T_DEPTH].as_image_copy(),
                    t.depth_behind.as_image_copy(),
                    t.depth_behind.size(),
                );
            }
            let mut pass = rec.render(
                "ice",
                &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &t.views[T_HDR],
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    // The vapour marks the reactive mask the water began.
                    Some(wgpu::RenderPassColorAttachment {
                        view: &t.reactive_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.views[T_DEPTH],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
            );
            if ice_drawn {
                draw(&mut pass, false);
                pass.set_bind_group(3, &t.ice_group, &[]);
                pass.set_pipeline(&self.ice_pipeline);
                for (k, (_, d, _)) in sorted.iter().enumerate() {
                    if d.pipeline == PipelineKind::Ice {
                        let entry = sorted_base + u32::try_from(k).unwrap_or(0);
                        pass.draw_indexed(
                            d.first_index..d.first_index + d.index_count,
                            0,
                            entry..entry + 1,
                        );
                    }
                }
                // Liquid: each ball a card the merged surface is found on.
                if blobs.1 > 0 {
                    pass.set_pipeline(&self.blob_pipeline);
                    pass.draw(0..6, blobs.0..blobs.0 + blobs.1);
                }
                // Vapour: each ball a card, each body marched by the first ball the view crosses.
                if puffs.1 > 0 {
                    pass.set_pipeline(&self.puff_pipeline);
                    pass.draw(0..6, puffs.0..puffs.0 + puffs.1);
                }
            }
        }
        {
            // The picture (and the order-independent sum and revealage); the depth read-only, so
            // the lamps and the particles' soft edges read it beside the test.
            let oit = self.oit;
            let keep = |v| {
                Some(wgpu::RenderPassColorAttachment {
                    view: v,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })
            };
            let color = if oit {
                vec![
                    clear(&t.oit_views[2], wgpu::Color::TRANSPARENT),
                    clear(&t.oit_views[0], wgpu::Color::TRANSPARENT),
                    clear(&t.oit_views[1], wgpu::Color::WHITE),
                ]
            } else {
                vec![keep(&t.views[T_HDR]), keep(&t.reactive_view)]
            };
            let mut pass = rec.render(
                "translucent",
                &color,
                Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.views[T_DEPTH],
                    depth_ops: None,
                    stencil_ops: None,
                }),
            );
            draw(&mut pass, false);
            pass.set_bind_group(3, &t.light_group, &[]);
            // Objects and runs of particles merged far to near (in any order under `oit`).
            let mut current: Option<(bool, bool, bool)> = None;
            let (mut i, mut j) = (0, 0);
            while i < sorted.len() || j < batches.len() {
                let object =
                    j >= batches.len() || (i < sorted.len() && sorted[i].2 >= batches[j].distance);
                let (d, instances) = if object {
                    let entry = sorted_base + u32::try_from(i).unwrap_or(0);
                    i += 1;
                    (sorted[i - 1].1, entry..entry + 1)
                } else {
                    let b = batches[j];
                    j += 1;
                    (b.draw, b.first..b.first + b.count)
                };
                if object && d.pipeline == PipelineKind::Ice {
                    continue;
                }
                let additive = d.pipeline == PipelineKind::Additive;
                let web = d.pipeline == PipelineKind::Web;
                if web && !self.webs.on {
                    continue;
                }
                if current != Some((object, additive, web)) {
                    current = Some((object, additive, web));
                    let k = usize::from(additive);
                    let p = match (object, oit) {
                        _ if web => Some(&self.web_pipelines[usize::from(oit)]),
                        (true, false) => self
                            .pipelines
                            .iter()
                            .find(|(kind, _)| *kind == d.pipeline)
                            .map(|(_, p)| p),
                        (true, true) => Some(&self.oit_pipelines[k]),
                        (false, false) => Some(&self.particle_pipelines[k]),
                        (false, true) => Some(&self.particle_oit_pipelines[k]),
                    };
                    if let Some(p) = p {
                        pass.set_pipeline(p);
                    }
                }
                pass.draw_indexed(d.first_index..d.first_index + d.index_count, 0, instances);
            }
            // The flames, added over all of it.
            if self.flames_drawn > 0 {
                pass.set_pipeline(&self.flame_pipelines[usize::from(oit)]);
                pass.draw(0..6, 0..self.flames_drawn);
            }
            // The lightning's channels, added over it all ([`crate::bolts`]).
            if bolts.1 > 0 {
                pass.set_pipeline(&self.bolt_pipelines[usize::from(oit)]);
                pass.draw(0..6, bolts.0..bolts.0 + bolts.1);
            }
        }
        {
            let mut pass = rec.render(
                "oit",
                &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.views[T_HDR],
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                None,
            );
            if self.oit {
                pass.set_pipeline(&self.oit_pipeline);
                pass.set_bind_group(0, &self.frame_group, &[]);
                pass.set_bind_group(1, &t.oit_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        {
            // The drops mark the reactive mask, which the water began.
            let reactive = wgpu::LoadOp::Load;
            let mut pass = rec.render(
                "weather",
                &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &t.views[T_HDR],
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &t.reactive_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: reactive,
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.views[T_DEPTH],
                    depth_ops: None,
                    stencil_ops: None,
                }),
            );
            if view.outside && !view.sealed && self.weather.falling() {
                self.weather.encode(
                    gpu,
                    &mut pass,
                    &self.frame_group,
                    self.clock,
                    (width, height),
                    view.eye_room,
                );
            }
        }
        self.air.weather = self
            .weather
            .rain
            .max(self.weather.snow * (0.6 + 0.4 * self.weather.storminess()));
        let air = self.air.drawn(physical, view.sealed);
        if air {
            self.air.prepare(gpu, camera.near, camera.position.z);
        }
        {
            let mut pass = rec.compute("air");
            if air {
                let atlas = self.shadow_atlas.create_view(&Default::default());
                self.air.encode_froxels(
                    gpu,
                    &mut pass,
                    &self.frame_group,
                    &atlas,
                    &self.shadow_cmp,
                );
            }
        }
        {
            let mut pass = rec.render(
                "air apply",
                &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.views[T_HDR],
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                None,
            );
            if air {
                self.air
                    .encode_apply(gpu, &mut pass, &self.frame_group, &t.views[T_DEPTH]);
            }
        }
        {
            let mut pass = rec.compute("exposure");
            self.post.encode_exposure(&mut pass, &self.frame_group);
        }
        {
            let mut pass = rec.compute("bloom");
            if bloom > 0.0 {
                self.post.encode_bloom(&mut pass);
            }
        }
        {
            let ldr = t.ldr.create_view(&Default::default());
            let mut pass = rec.render("tonemap", &[clear(&ldr, wgpu::Color::BLACK)], None);
            pass.set_pipeline(&self.tonemap_pipeline);
            pass.set_bind_group(0, &self.frame_group, &[]);
            pass.set_bind_group(1, &t.tonemap_group, &[]);
            pass.draw(0..3, 0..1);
        }
        {
            // The interface's marks in the world, under it, against the world's depth.
            let marks = view
                .overlay
                .filter(|o| o.has_world() && self.ui.is_some())
                .and_then(|o| Some((o, o.view()?)));
            let target = match marks {
                Some((_, v)) => v.clone(),
                None => t.ldr.create_view(&Default::default()),
            };
            let mut pass = rec.render(
                "world marks",
                &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                None,
            );
            if let Some((o, _)) = marks {
                #[allow(clippy::cast_precision_loss)]
                let scale = (
                    width as f32 / output_size.0.max(1) as f32,
                    height as f32 / output_size.1.max(1) as f32,
                );
                let depth = t.textures[T_DEPTH].create_view(&wgpu::TextureViewDescriptor {
                    aspect: wgpu::TextureAspect::DepthOnly,
                    ..Default::default()
                });
                o.draw_world(gpu, &mut pass, &depth, scale, camera.near);
            }
        }
        self.temporal.record(
            gpu,
            &mut rec,
            how,
            &crate::temporal::Drawn {
                size: (width, height),
                ldr: &t.ldr,
                depth: &t.textures[T_DEPTH],
                motion: &t.textures[T_MOTION],
                ids: &t.textures[T_IDS],
                prev_ids: &t.prev_ids,
                reactive: &t.reactive,
                jitter,
                reproject,
            },
            output,
            output_size,
            self.ui.as_ref(),
        );
        rec.finish();
        gpu.queue.submit([enc.finish()]);
        self.graph.submitted(gpu);
        Ok(())
    }

    /// Reads back the last frame's pass timings; `wait` blocks until they are in.
    pub fn collect_timings(&mut self, gpu: &Gpu, wait: bool) -> &[PassTiming] {
        self.graph.collect(gpu, wait);
        self.graph.timings()
    }

    /// The last timed frame's GPU time, first pass's start to last pass's end, ms.
    #[must_use]
    pub fn frame_span_ms(&self) -> f64 {
        self.graph.span_ms()
    }

    /// The frame graph's declaration.
    #[must_use]
    pub fn graph(&self) -> &GraphDecl {
        self.graph.decl()
    }

    /// What the cull pass kept last frame, read back (blocking).
    pub fn read_cull_stats(&self, gpu: &Gpu) -> CullStats {
        let b = gpu.read_buffer(&self.stats, 0, 16);
        let w = |i: usize| {
            b.get(i * 4..i * 4 + 4)
                .map_or(0, |c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        };
        CullStats {
            visible_instances: w(0),
            visible_triangles: w(1),
            occluded: w(2),
            degraded: w(3),
        }
    }

    /// Each cluster's lamp count and lamp list (indices into [`Self::lamps_drawn`]), read back
    /// (blocking): `x + y * tiles.0 + slice * tiles.0 * tiles.1`.
    pub fn read_clusters(&self, gpu: &Gpu) -> (Vec<u32>, Vec<Vec<u32>>) {
        let n = u64::from(self.cluster_tiles.0 * self.cluster_tiles.1 * CLUSTER_SLICES);
        let words = |b: Vec<u8>| -> Vec<u32> {
            b.as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect()
        };
        let counts = words(gpu.read_buffer(&self.cluster_counts, 0, n * 4));
        let all = words(gpu.read_buffer(&self.cluster_lamps, 0, n * 4 * CLUSTER_MOST));
        let most = usize::try_from(CLUSTER_MOST).unwrap_or(48);
        let lists = counts
            .iter()
            .enumerate()
            .map(|(c, &k)| all[c * most..c * most + k as usize].to_vec())
            .collect();
        (counts, lists)
    }

    /// The texel of the objects target under output pixel `at`, copied into a fresh readback
    /// buffer by a submission of its own.
    fn copy_pick(&self, gpu: &Gpu, at: (u32, u32)) -> Option<wgpu::Buffer> {
        let t = self.targets.as_ref()?;
        let (ow, oh) = (self.output_size.0.max(1), self.output_size.1.max(1));
        let map = |v: u32, out: u32, size: u32| {
            u32::try_from(u64::from(v) * u64::from(size) / u64::from(out))
                .unwrap_or(0)
                .min(size.saturating_sub(1))
        };
        let (x, y) = (map(at.0, ow, t.width), map(at.1, oh, t.height));
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pick"),
            size: 256,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &t.textures[T_OBJECTS],
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        gpu.queue.submit([enc.finish()]);
        Some(buffer)
    }

    /// The object number in a mapped pick buffer, `None` for no object.
    fn read_pick(buffer: &wgpu::Buffer) -> Option<u32> {
        let slice = buffer.slice(..);
        let word = slice
            .get_mapped_range()
            .ok()
            .map(|v| u32::from_le_bytes([v[0], v[1], v[2], v[3]]));
        buffer.unmap();
        word.and_then(|w| w.checked_sub(1))
    }

    /// The object (its number) under output pixel `at` last frame, `None` for the ground, the sky
    /// or a draw with no object. Reads one texel back (blocking).
    #[must_use]
    pub fn pick(&self, gpu: &Gpu, at: (u32, u32)) -> Option<u32> {
        let buffer = self.copy_pick(gpu, at)?;
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        gpu.wait();
        Self::read_pick(&buffer)
    }

    /// Whether a pick asked for is still on its way back.
    #[must_use]
    pub fn pick_in_flight(&self) -> bool {
        self.pick_pending.is_some()
    }

    /// Asks for the object under output pixel `at` in the last frame drawn, without waiting:
    /// [`Self::take_pick`] answers once the texel is back (a frame or so later). A request
    /// replaces one still on its way, which is then never answered ([`Self::pick_in_flight`]).
    pub fn request_pick(&mut self, gpu: &Gpu, at: (u32, u32)) {
        let Some(buffer) = self.copy_pick(gpu, at) else {
            return;
        };
        let ready = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = std::sync::Arc::clone(&ready);
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            if r.is_ok() {
                flag.store(true, std::sync::atomic::Ordering::Release);
            }
        });
        self.pick_pending = Some((buffer, ready));
    }

    /// The answer to the last [`Self::request_pick`] once it is back: `Some(object)`, the inner
    /// `None` for no object. `None` while it is on its way or none was asked for.
    #[allow(clippy::option_option)]
    pub fn take_pick(&mut self, gpu: &Gpu) -> Option<Option<u32>> {
        let ready = self.pick_pending.as_ref()?.1.clone();
        if !ready.load(std::sync::atomic::Ordering::Acquire) {
            let _ = gpu.device.poll(wgpu::PollType::Poll);
        }
        if !ready.load(std::sync::atomic::Ordering::Acquire) {
            return None;
        }
        let (buffer, _) = self.pick_pending.take()?;
        Some(Self::read_pick(&buffer))
    }

    /// The world's last picture, tone mapped and without the interface, at the drawn size.
    #[must_use]
    pub fn world_picture(&self) -> Option<&wgpu::Texture> {
        self.targets.as_ref().map(|t| &t.ldr)
    }

    /// The depth target, for a test that checks what was drawn.
    #[must_use]
    pub fn depth_texture(&self) -> Option<&wgpu::Texture> {
        self.targets.as_ref().and_then(|t| t.textures.get(T_DEPTH))
    }

    /// The material id of each pixel (`u32`), for a test that checks what was drawn.
    #[must_use]
    pub fn ids_texture(&self) -> Option<&wgpu::Texture> {
        self.targets.as_ref().and_then(|t| t.textures.get(T_IDS))
    }

    /// Each pixel's motion since last frame (two halves: x and y, in texture coordinates), for a
    /// test that checks what moved.
    #[must_use]
    pub fn motion_texture(&self) -> Option<&wgpu::Texture> {
        self.targets.as_ref().and_then(|t| t.textures.get(T_MOTION))
    }

    /// Each pixel's normal (two halves: octahedral x and y), for a test that checks how a
    /// surface faces.
    #[must_use]
    pub fn normal_texture(&self) -> Option<&wgpu::Texture> {
        self.targets.as_ref().and_then(|t| t.textures.get(T_NORMAL))
    }
}

/// The cluster counts and lamp lists for `tiles` tiles.
fn cluster_buffers(device: &wgpu::Device, tiles: u32) -> (wgpu::Buffer, wgpu::Buffer) {
    let (counts, lamps) = cluster_bytes(tiles);
    let make = |label, size| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    };
    (make("cluster counts", counts), make("cluster lamps", lamps))
}
