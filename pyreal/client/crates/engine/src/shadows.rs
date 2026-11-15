//! The sun's shadows: four cascades in one depth atlas, each fitted to a slice of the view.
//!
//! The view out to the shadow distance ([`SHADOW_DISTANCE`] unless set) is split between the
//! cascades by the practical scheme (a blend of even and logarithmic splits, [`SPLIT_LAMBDA`]),
//! the shadows fading out over its last [`FADE`]. Each cascade is a sphere round its slice, so it
//! keeps its size as the view turns, looked at from the sun with its centre snapped to its
//! texels, so its shadows hold still as the eye moves (stable cascades). The cull pass runs once
//! per cascade over the same instances (no rooms, no portal rectangles, no depth pyramid, each
//! part at a coarser level than the eye's but never a picture where the eye sees its shape,
//! unless wholly sunward of the cascade's receivers; a picture turns to the sun),
//! writing each cascade's draws into its own list; one indirect draw per bucket and cascade fills
//! the atlas.

use glam::{Mat4, Vec3, Vec4};

use crate::camera::Camera;

/// The cascades.
pub const CASCADES: usize = 4;
/// The atlas's side, texels; each cascade has a quarter, half as wide.
pub const ATLAS: u32 = 4096;
/// How far along the view the sun's shadows reach unless told otherwise, metres.
pub const SHADOW_DISTANCE: f32 = 200.0;
/// The practical split's share of logarithmic splitting.
pub const SPLIT_LAMBDA: f32 = 0.8;
/// How far past each cascade's sphere toward the sun casters are kept, metres.
pub const CASTER_REACH: f32 = 400.0;
/// The share of each cascade blended into the next.
pub const BLEND: f32 = 0.1;
/// The share of the shadow distance the shadows fade out over, ending at it.
pub const FADE: f32 = 0.25;
/// The tangent of the angular radius whose penumbra the shadows take: wider than the sun's own,
/// as the sky's light softens them.
pub const PENUMBRA: f32 = 0.012;
/// The depth atlas's format.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// One cascade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cascade {
    /// World to the cascade's clip space (x and y -1 to 1, depth 0 at the sun's side to 1).
    pub view_proj: Mat4,
    /// The view distance its slice ends at.
    pub far: f32,
    /// A texel's size, metres.
    pub texel: f32,
    /// Its depth range, metres.
    pub depth: f32,
    /// Its planes, `n · p + d >= 0` inside, for the cull pass.
    pub planes: [Vec4; 6],
}

/// Where each cascade's slice of the view ends, out to `distance`.
#[must_use]
pub fn splits(near: f32, distance: f32) -> [f32; CASCADES] {
    let n = near.max(0.05);
    let ratio = distance / n;
    std::array::from_fn(|i| {
        #[allow(clippy::cast_precision_loss)]
        let t = (i + 1) as f32 / CASCADES as f32;
        let log = n * pyreal_primitives::num::math::powf(ratio, t);
        let even = n + (distance - n) * t;
        SPLIT_LAMBDA * log + (1.0 - SPLIT_LAMBDA) * even
    })
}

/// The planes of an orthographic `view_proj`.
pub(crate) fn planes(m: &Mat4) -> [Vec4; 6] {
    let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
    let norm = |p: Vec4| p / p.truncate().length().max(1e-12);
    [
        norm(r3 + r0),
        norm(r3 - r0),
        norm(r3 + r1),
        norm(r3 - r1),
        norm(r2),
        norm(r3 - r2),
    ]
}

/// The cascades for `camera` (at `aspect`) under a sun toward `sun_dir`, reaching `distance`.
#[must_use]
pub fn cascades(camera: &Camera, aspect: f32, sun_dir: Vec3, distance: f32) -> [Cascade; CASCADES] {
    use pyreal_primitives::num::math::tanf;
    let ends = splits(camera.near, distance);
    let fwd = camera.forward();
    let ty = tanf(camera.fov_y * 0.5);
    let tx = ty * aspect;
    let sun = sun_dir.normalize_or(Vec3::Z);
    let light_up = if sun.z.abs() > 0.99 { Vec3::Y } else { Vec3::Z };
    // The sun's view, turned only: x and y across its light.
    let turn = Mat4::look_to_rh(Vec3::ZERO, -sun, light_up);
    let side = f32::from(u16::try_from(ATLAS / 2).unwrap_or(2048));
    let mut start = camera.near;
    std::array::from_fn(|i| {
        let end = ends[i];
        // The slice's bounding sphere: its centre on the view axis where the near and far
        // rectangles' corners are equally far.
        let a2 = (tx * tx + ty * ty) * start * start;
        let b2 = (tx * tx + ty * ty) * end * end;
        let z = ((end * end - start * start + b2 - a2) / (2.0 * (end - start))).clamp(start, end);
        let radius = ((end - z) * (end - z) + b2).sqrt();
        // Rounded up so the cascade's size never shimmers.
        let radius = (radius * 16.0).ceil() / 16.0;
        let centre = camera.position + fwd * z;
        let texel = 2.0 * radius / side;
        // The centre snapped to the cascade's texels, in the sun's turned frame.
        let c = turn.transform_point3(centre);
        let snapped = Vec3::new(
            (c.x / texel).floor() * texel,
            (c.y / texel).floor() * texel,
            c.z,
        );
        let centre = turn.inverse().transform_point3(snapped);
        let eye = centre + sun * (radius + CASTER_REACH);
        let view = Mat4::look_to_rh(eye, -sun, light_up);
        let depth = 2.0 * radius + CASTER_REACH;
        let proj = Mat4::orthographic_rh(-radius, radius, -radius, radius, 0.0, depth);
        let view_proj = proj * view;
        start = end;
        Cascade {
            view_proj,
            far: end,
            texel,
            depth,
            planes: planes(&view_proj),
        }
    })
}
