//! The far terrain at the world's edge.

use glam::{Vec2, Vec3};
use pyreal_engine::far::FarField;
use pyreal_engine::land::bits;
use pyreal_terrain::land::mesh::height_table;
use pyreal_world_data::landblock::load_region;

/// Round block 0xF618, near the world's east edge, the far field runs on past the last block as
/// open sea at the lowest height, its water records reaching the horizon as the near sea's do.
#[test]
fn the_sea_runs_on_past_the_worlds_edge() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = load_region(&store).expect("the region");
    let radius = 12;
    let near = (Vec2::splat(-768.0), Vec2::splat(960.0));
    let f = FarField::build(
        &store,
        &region,
        0xF618,
        Vec3::ZERO,
        radius,
        near,
        &mut |_| [0, 0, 255, 255],
    );
    let sea = height_table(&region)[0];
    let side = f.water_side as usize;
    assert!(side > 0, "the field has water");
    let first = 0xF6 - radius;
    let edge = usize::try_from((0xFF - first) * 8).unwrap();
    let word = |i: usize| u32::from_le_bytes(f.water[i * 16 + 12..i * 16 + 16].try_into().unwrap());
    let float = |i: usize, k: usize| {
        f32::from_bits(u32::from_le_bytes(
            f.water[i * 16 + k * 4..i * 16 + k * 4 + 4]
                .try_into()
                .unwrap(),
        ))
    };
    for y in 0..side {
        for x in edge + 1..side {
            let i = y * side + x;
            assert!(
                word(i) & bits::WATER != 0,
                "vertex ({x}, {y}) past the edge is water"
            );
            assert!((float(i, 2) - sea).abs() < 1e-3, "its level is the sea's");
            assert!(
                (f.heights[y * f.size as usize + x] - sea).abs() < 1e-3,
                "its ground is the sea's"
            );
        }
    }
    // Well out past the edge it is open water, as far inside as the records reach.
    let i = 100 * side + side - 1;
    assert!(float(i, 1) > 100.0, "{} m inside", float(i, 1));
}
