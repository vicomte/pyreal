//! A `GfxObj`'s polygons become sub-meshes by surface and side.

use pyreal_assets::{GfxObj, Polygon, SwVertex, VertexArray};
use pyreal_engine::asset::mesh::{gfxobj_mesh, oct_decode, oct_encode, Sides};
use pyreal_engine::asset::object::setup_parts;
use pyreal_primitives::{DataId, Vec3};

fn vertex(id: i16, x: f32, y: f32) -> SwVertex {
    SwVertex {
        id,
        position: Vec3::new(x, y, 0.0),
        normal: Vec3::new(0.0, 0.0, 1.0),
        uvs: vec![(x, y), (1.0 - x, 1.0 - y)],
    }
}

fn polygon(sides_type: i32, pos: u16, neg: u16, ids: &[u16]) -> Polygon {
    let n = ids.len();
    Polygon {
        poly_id: 0,
        num_pts: u8::try_from(n).unwrap_or(0),
        stippling: 0,
        sides_type,
        pos_surface: pos,
        neg_surface: neg,
        vertex_ids: ids.to_vec(),
        pos_uv_indices: Some(vec![0; n]),
        neg_uv_indices: (sides_type == 2).then(|| vec![1; n]),
    }
}

fn object(polygons: Vec<Polygon>) -> GfxObj {
    GfxObj {
        id: DataId(0x0100_0001),
        flags: 2,
        surfaces: vec![DataId(0x0800_0001), DataId(0x0800_0002)],
        vertex_array: VertexArray {
            vertex_type: 1,
            vertices: vec![
                vertex(0, 0.0, 0.0),
                vertex(1, 1.0, 0.0),
                vertex(2, 1.0, 1.0),
                vertex(3, 0.0, 1.0),
            ],
        },
        physics_polygons: Vec::new(),
        physics_bsp: None,
        sort_center: Vec3::new(0.0, 0.0, 0.0),
        polygons,
        drawing_bsp: None,
        did_degrade: None,
    }
}

/// A quad is a fan of two triangles; a two-sided polygon goes to a sub-mesh drawn without culling;
/// a polygon with a negative surface also draws its back with that surface, wound the other way
/// and with its normal turned round.
#[test]
fn polygons_become_sub_meshes_by_surface_and_side() {
    let mesh = gfxobj_mesh(&object(vec![
        polygon(0, 0, 0, &[0, 1, 2, 3]),
        polygon(1, 1, 1, &[0, 1, 2]),
        polygon(2, 0, 1, &[0, 2, 3]),
    ]));
    let subs: Vec<(u16, Sides, u32)> = mesh
        .submeshes
        .iter()
        .map(|s| (s.surface_index, s.sides, s.index_count))
        .collect();
    // One-sided first: surface 0 (the quad and the front of the third), surface 1 (its back).
    assert_eq!(
        subs,
        vec![(0, Sides::One, 9), (1, Sides::One, 3), (1, Sides::Both, 3)]
    );
    assert_eq!(mesh.submeshes[1].surface, Some(DataId(0x0800_0002)));
    assert_eq!(mesh.triangle_count(), 5);
    // The back of polygon 2 is the front reversed, with flipped normals and its own UVs.
    let back = &mesh.submeshes[1];
    let tri: Vec<u32> = mesh.indices
        [back.first_index as usize..(back.first_index + back.index_count) as usize]
        .to_vec();
    let positions: Vec<[f32; 3]> = tri
        .iter()
        .map(|&i| mesh.vertices[i as usize].position)
        .collect();
    assert_eq!(
        positions,
        vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]]
    );
    for &i in &tri {
        let v = mesh.vertices[i as usize];
        assert_eq!(v.normal, [0.0, 0.0, -1.0]);
        assert_eq!(v.uv, [1.0 - v.position[0], 1.0 - v.position[1]]);
    }
    for &i in &mesh.indices {
        assert!((i as usize) < mesh.vertices.len());
    }
    // The bounding sphere holds every vertex.
    assert_eq!(mesh.center, [0.5, 0.5, 0.0]);
    assert!((mesh.radius - 0.5f32.sqrt()).abs() < 1e-6);
}

/// A corner that recurs with the same UV is one vertex; degenerate polygons are skipped.
#[test]
fn corners_are_shared_and_degenerate_polygons_dropped() {
    let mesh = gfxobj_mesh(&object(vec![
        polygon(0, 0, 0, &[0, 1, 2]),
        polygon(0, 0, 0, &[0, 2, 3]),
        polygon(0, 0, 0, &[0, 1]),
    ]));
    assert_eq!(mesh.vertices.len(), 4);
    assert_eq!(mesh.indices.len(), 6);
}

/// A corner whose normal the dats bent toward a neighbouring face takes its own face's normal,
/// unshared; one near its face keeps its own.
#[test]
fn bent_corner_normals_take_the_face_normal() {
    let mut obj = object(vec![
        polygon(0, 0, 0, &[0, 1, 2]),
        polygon(0, 0, 0, &[0, 2, 3]),
    ]);
    // Corner 2 leans 55 degrees off the face, as a room's smoothed corner does; corner 1 a little.
    obj.vertex_array.vertices[2].normal = Vec3::new(0.577, 0.577, 0.577);
    obj.vertex_array.vertices[1].normal = Vec3::new(0.0, 0.2, 0.98);
    let mesh = gfxobj_mesh(&obj);
    assert_eq!(mesh.vertices.len(), 5, "corner 2 is one vertex per face");
    for &i in &mesh.indices {
        let v = mesh.vertices[i as usize];
        if v.position == [1.0, 1.0, 0.0] {
            assert_eq!(v.normal, [0.0, 0.0, 1.0]);
        }
        if v.position == [1.0, 0.0, 0.0] {
            assert_eq!(v.normal, [0.0, 0.2, 0.98]);
        }
    }
}

/// On a shape smaller than a metre the bent corner keeps its smoothing (a crystal's facets).
#[test]
fn a_small_shapes_bent_corners_keep_their_normals() {
    let mut obj = object(vec![
        polygon(0, 0, 0, &[0, 1, 2]),
        polygon(0, 0, 0, &[0, 2, 3]),
    ]);
    for v in &mut obj.vertex_array.vertices {
        v.position = Vec3::new(v.position.x * 0.2, v.position.y * 0.2, 0.0);
    }
    obj.vertex_array.vertices[2].normal = Vec3::new(0.577, 0.577, 0.577);
    let mesh = gfxobj_mesh(&obj);
    assert_eq!(mesh.vertices.len(), 4, "corner 2 is shared");
    assert!(mesh
        .vertices
        .iter()
        .any(|v| v.normal == [0.577, 0.577, 0.577]));
}

/// An object with no drawing polygons has nothing to draw.
#[test]
fn a_physics_only_object_converts_to_nothing() {
    let mesh = gfxobj_mesh(&object(Vec::new()));
    assert!(mesh.indices.is_empty() && mesh.submeshes.is_empty());
}

/// Octahedral normals survive the round trip, the lower hemisphere included.
#[test]
fn octahedral_normals_round_trip() {
    for n in [
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
        [1.0, 0.0, 0.0],
        [0.6, -0.48, -0.64],
        [-0.36, 0.48, 0.8],
    ] {
        let d = oct_decode(oct_encode(n));
        for k in 0..3 {
            assert!((d[k] - n[k]).abs() < 1e-5, "{n:?} came back {d:?}");
        }
    }
}

/// A setup's parts take the resting placement (0x65) over the default (0), and its default
/// scale scales each part's mesh.
#[test]
fn setup_parts_rest_at_placement_0x65_with_their_default_scale() {
    use pyreal_assets::geometry::{Placement, Setup, SetupOrder};
    use pyreal_primitives::{Frame, Quat};
    let frame = |x: f32| Frame::new(Vec3::new(x, 0.0, 0.0), Quat::new(1.0, 0.0, 0.0, 0.0));
    let placements = [(0u32, 1.0f32), (0x65, 5.0)]
        .into_iter()
        .map(|(k, x)| {
            (
                k,
                Placement {
                    frames: vec![frame(x)],
                    hooks: Vec::new(),
                },
            )
        })
        .collect();
    let empty = pyreal_assets::Sphere {
        center: Vec3::new(0.0, 0.0, 0.0),
        radius: 0.0,
    };
    let setup = Setup {
        id: DataId(0x0200_0001),
        flags: 2,
        parts: vec![DataId(0x0100_0001)],
        parent_index: None,
        default_scale: Some(vec![Vec3::new(2.0, 2.0, 2.0)]),
        allow_free_heading: false,
        has_physics_bsp: false,
        holding_locations: Default::default(),
        connection_points: Default::default(),
        placement_frames: placements,
        cylspheres: Vec::new(),
        spheres: Vec::new(),
        height: 0.0,
        radius: 0.0,
        step_up_height: 0.0,
        step_down_height: 0.0,
        sorting_sphere: empty,
        selection_sphere: empty,
        lights: Default::default(),
        default_anim_id: DataId(0),
        default_script_id: DataId(0),
        default_mtable_id: DataId(0),
        default_stable_id: DataId(0),
        default_phstable_id: DataId(0),
        written_order: SetupOrder::default(),
    };
    let parts = setup_parts(&setup);
    assert_eq!(parts.len(), 1);
    let p = parts[0]
        .transform
        .transform_point3(glam::Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(p, glam::Vec3::new(7.0, 0.0, 0.0));
}
