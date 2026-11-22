//! Skin a worn model keeps is shaded by the cloth round it.

use pyreal_engine::asset::mesh::{shade_inside, MeshData, Sides, SubMesh, Vertex, SHADE_REACH};
use pyreal_primitives::DataId;

const SKIN: DataId = DataId(0x0800_0B03);
const CLOTH: DataId = DataId(0x0800_0BAC);

fn quad(mesh: &mut MeshData, surface: DataId, corners: [[f32; 3]; 4], normal: [f32; 3]) {
    let base = u32::try_from(mesh.vertices.len()).unwrap();
    for position in corners {
        mesh.vertices.push(Vertex {
            position,
            normal,
            uv: [0.0, 0.0],
            extra: [0, 0],
        });
    }
    let first = u32::try_from(mesh.indices.len()).unwrap();
    mesh.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
    mesh.submeshes.push(SubMesh {
        surface_index: u16::try_from(mesh.submeshes.len()).unwrap(),
        surface: Some(surface),
        sides: Sides::One,
        first_index: first,
        index_count: 6,
    });
}

/// A small face looking up, and far off a second one.
fn faces() -> MeshData {
    let mut m = MeshData::default();
    let s = 0.01;
    quad(
        &mut m,
        SKIN,
        [[-s, -s, 0.0], [s, -s, 0.0], [s, s, 0.0], [-s, s, 0.0]],
        [0.0, 0.0, 1.0],
    );
    quad(
        &mut m,
        SKIN,
        [
            [1.0 - s, -s, 0.0],
            [1.0 + s, -s, 0.0],
            [1.0 + s, s, 0.0],
            [1.0 - s, s, 0.0],
        ],
        [0.0, 0.0, 1.0],
    );
    m
}

#[test]
fn skin_under_cloth_is_shaded_and_open_skin_is_not() {
    let mut m = faces();
    // A roof of cloth over the first face only, closer than the reach.
    let (h, w) = (SHADE_REACH * 0.3, 0.3);
    quad(
        &mut m,
        CLOTH,
        [[-w, -w, h], [-w, w, h], [w, w, h], [w, -w, h]],
        [0.0, 0.0, -1.0],
    );
    assert!(shade_inside(&mut m, &[SKIN], SHADE_REACH));
    let shade: Vec<u32> = m.vertices.iter().map(|v| v.extra[1]).collect();
    assert!(
        shade[..4].iter().all(|&k| k > 200),
        "covered skin {shade:?}"
    );
    assert!(shade[4..8].iter().all(|&k| k == 0), "open skin {shade:?}");
    assert!(
        shade[8..].iter().all(|&k| k == 0),
        "the cloth keeps no shade {shade:?}"
    );
}

#[test]
fn a_model_with_no_cloth_is_not_shaded() {
    let mut m = faces();
    assert!(!shade_inside(&mut m, &[SKIN], SHADE_REACH));
    assert!(m.vertices.iter().all(|v| v.extra[1] == 0));
}
