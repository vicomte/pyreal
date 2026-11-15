//! The records the shaders read are the sizes the shaders declare; the pools' allocator; the
//! camera's conventions.

use glam::{Affine3A, Vec3};
use pyreal_engine::asset::mesh::{Vertex, VERTEX_SIZE};
use pyreal_engine::camera::Camera;
use pyreal_engine::pool::RangeAllocator;
use pyreal_engine::scene::{Instance, MeshId, INSTANCE_SIZE};
use pyreal_engine::terrain::{CellRecord, Layer, CELL_RECORD_SIZE};

#[test]
fn records_are_the_sizes_the_shaders_read() {
    let mut b = Vec::new();
    Vertex::default().write(&mut b);
    assert_eq!(b.len() as u64, VERTEX_SIZE);
    b.clear();
    Instance {
        mesh: MeshId(3),
        transform: Affine3A::from_translation(Vec3::new(1.0, 2.0, 3.0)),
        previous: Affine3A::IDENTITY,
        object: 9,
        cell: 4,
        sway: 0,
    }
    .write(&mut b);
    assert_eq!(b.len() as u64, INSTANCE_SIZE);
    // Row 0 of the transform is (1, 0, 0, tx).
    let f = |i: usize| f32::from_le_bytes([b[i * 4], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]]);
    assert_eq!([f(0), f(1), f(2), f(3)], [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(f(11), 3.0);
    // The mesh, the object and the cell slot follow the two transforms.
    assert_eq!(&b[96..108], &[3, 0, 0, 0, 9, 0, 0, 0, 4, 0, 0, 0]);
    b.clear();
    CellRecord {
        layers: vec![Layer {
            texture: 1,
            alpha: 2,
            rotation: 3,
            kind: 4,
            ground: 9,
            tiling: 2.0,
        }],
        water: true,
        cut_first: 5,
        cut_count: 2,
    }
    .write(&mut b);
    assert_eq!(b.len() as u64, CELL_RECORD_SIZE);
    assert_eq!(&b[0..16], &[1, 0, 0, 0, 1, 0, 0, 0, 5, 0, 0, 0, 2, 0, 0, 0]);
    assert_eq!(&b[24..28], &(3u32 | (4 << 8) | (9 << 16)).to_le_bytes());
}

/// First fit, aligned, and freed ranges merge with their neighbours.
#[test]
fn the_range_allocator_reuses_and_merges() {
    let mut a = RangeAllocator::new(100);
    assert_eq!(a.alloc(10, 1), Some(0));
    assert_eq!(a.alloc(10, 16), Some(16));
    assert_eq!(a.alloc(30, 1), Some(26));
    assert_eq!(a.used_units(), 50);
    assert_eq!(a.alloc(60, 1), None);
    a.free(16, 10);
    a.free(0, 10);
    assert_eq!(
        a.alloc(16, 1),
        Some(0),
        "the freed ranges and the gap merged"
    );
    a.free(0, 16);
    a.free(26, 30);
    assert_eq!(a.free_units(), 100);
    assert_eq!(a.alloc(100, 1), Some(0));
}

/// The client's yaw: zero faces north (+y), a quarter turn faces west; things ahead are inside
/// the frustum and things behind or past the far distance are not.
#[test]
fn the_camera_faces_as_the_client_does() {
    let mut c = Camera::default();
    assert!((c.forward() - Vec3::Y).length() < 1e-6);
    c.yaw = std::f32::consts::FRAC_PI_2;
    assert!((c.forward() - Vec3::NEG_X).length() < 1e-6);
    c.yaw = 0.0;
    c.far = 100.0;
    let inside = |p: Vec3| {
        c.frustum(1.0)
            .iter()
            .all(|pl| pl.truncate().dot(p) + pl.w >= 0.0)
    };
    assert!(inside(Vec3::new(0.0, 10.0, 0.0)));
    assert!(!inside(Vec3::new(0.0, -10.0, 0.0)));
    assert!(!inside(Vec3::new(0.0, 150.0, 0.0)));
    assert!(!inside(Vec3::new(50.0, 10.0, 0.0)));
}
