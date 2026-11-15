//! The sun's cascades: their splits, their reach, and how they hold still.

use glam::{Vec3, Vec4};
use pyreal_engine::shadows::{cascades, splits, ATLAS, CASCADES, SHADOW_DISTANCE};
use pyreal_engine::Camera;

/// The splits grow, the last at the shadows' reach, the first a few tens of metres out.
#[test]
fn splits_grow_to_the_shadows_reach() {
    let s = splits(0.1, SHADOW_DISTANCE);
    assert!(s.windows(2).all(|w| w[0] < w[1]), "{s:?}");
    assert!((s[CASCADES - 1] - SHADOW_DISTANCE).abs() < 1e-3);
    assert!(s[0] > 5.0 && s[0] < 40.0, "{s:?}");
}

/// Every point of a cascade's slice of the view lies inside its box, and the cascades' texels
/// coarsen outward.
#[test]
fn each_cascade_holds_its_slice() {
    let camera = Camera {
        position: Vec3::new(10.0, 20.0, 30.0),
        yaw: 0.7,
        pitch: -0.3,
        ..Camera::default()
    };
    let sun = Vec3::new(0.4, 0.2, 0.9).normalize();
    let cs = cascades(&camera, 16.0 / 9.0, sun, SHADOW_DISTANCE);
    let mut start = camera.near;
    for c in &cs {
        // Points along the slice's middle and toward its far corners.
        for t in [0.0f32, 0.5, 1.0] {
            let d = start + (c.far - start) * t;
            let p = camera.position + camera.forward() * d + camera.right() * d * 0.5;
            let q = c.view_proj * p.extend(1.0);
            assert!(
                q.x.abs() <= 1.0 && q.y.abs() <= 1.0 && (0.0..=1.0).contains(&q.z),
                "{p} outside its cascade: {q}"
            );
            assert!(c
                .planes
                .iter()
                .all(|pl: &Vec4| pl.dot(p.extend(1.0)) >= -1e-3));
        }
        start = c.far;
    }
    assert!(cs.windows(2).all(|w| w[0].texel < w[1].texel));
}

/// Moving the eye a little moves each cascade by whole texels, so its shadows hold still.
#[test]
fn cascades_move_by_whole_texels() {
    let sun = Vec3::new(0.3, -0.5, 0.8).normalize();
    let a = Camera {
        position: Vec3::new(100.0, 50.0, 20.0),
        ..Camera::default()
    };
    let b = Camera {
        position: a.position + Vec3::new(0.37, -0.21, 0.0),
        ..a
    };
    let (ca, cb) = (
        cascades(&a, 1.5, sun, SHADOW_DISTANCE),
        cascades(&b, 1.5, sun, SHADOW_DISTANCE),
    );
    #[allow(clippy::cast_precision_loss)]
    let half = (ATLAS / 2) as f32;
    for (x, y) in ca.iter().zip(&cb) {
        // A fixed world point's place in both, in texels.
        let p = Vec3::new(90.0, 60.0, 15.0).extend(1.0);
        let (qa, qb) = (x.view_proj * p, y.view_proj * p);
        let shift = (qa - qb).truncate().truncate() * half * 0.5;
        assert!(
            (shift.x - shift.x.round()).abs() < 0.02 && (shift.y - shift.y.round()).abs() < 0.02,
            "a shift of {shift} texels"
        );
    }
}
