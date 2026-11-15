//! The camera: the client's yaw and pitch (`R_z(yaw) · R_x(pitch)` applied to north), a
//! reverse-Z projection with no far plane, and the frustum the cull pass tests against.

use glam::{Mat4, Vec3, Vec4};
use pyreal_primitives::num::math::{cosf, sinf};

/// A viewpoint, in the engine's render space (metres, z up, relative to the scene origin).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub position: Vec3,
    /// Radians counter-clockwise from north (+y).
    pub yaw: f32,
    /// Radians above the horizon.
    pub pitch: f32,
    /// Vertical field of view, radians.
    pub fov_y: f32,
    pub near: f32,
    /// How far the cull pass keeps anything.
    pub far: f32,
    /// The dat cell the eye stands in, when known (a report names it); otherwise the rooms'
    /// volumes decide.
    pub cell: Option<u32>,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            fov_y: 60f32.to_radians(),
            near: 0.1,
            far: 2000.0,
            cell: None,
        }
    }
}

impl Camera {
    /// The way the camera looks.
    #[must_use]
    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = (sinf(self.yaw), cosf(self.yaw));
        let (sp, cp) = (sinf(self.pitch), cosf(self.pitch));
        Vec3::new(-sy * cp, cy * cp, sp)
    }

    /// Level and to the right of [`Self::forward`].
    #[must_use]
    pub fn right(&self) -> Vec3 {
        let (sy, cy) = (sinf(self.yaw), cosf(self.yaw));
        Vec3::new(cy, sy, 0.0)
    }

    #[must_use]
    pub fn view(&self) -> Mat4 {
        Mat4::look_to_rh(self.position, self.forward(), Vec3::Z)
    }

    /// Reverse Z: the near plane at depth 1, infinity at 0.
    #[must_use]
    pub fn projection(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_infinite_reverse_rh(self.fov_y, aspect, self.near)
    }

    #[must_use]
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.projection(aspect) * self.view()
    }

    /// The frustum's planes, `n · p + d >= 0` inside: left, right, bottom, top, near, and a far
    /// plane at [`Self::far`] along the view direction.
    #[must_use]
    pub fn frustum(&self, aspect: f32) -> [Vec4; 6] {
        let m = self.view_projection(aspect);
        let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
        let norm = |p: Vec4| p / p.truncate().length().max(1e-12);
        let f = self.forward();
        [
            norm(r3 + r0),
            norm(r3 - r0),
            norm(r3 + r1),
            norm(r3 - r1),
            // Reverse Z: inside is depth <= 1, which is z_clip <= w_clip.
            norm(r3 - r2),
            Vec4::new(-f.x, -f.y, -f.z, f.dot(self.position) + self.far),
        ]
    }
}
