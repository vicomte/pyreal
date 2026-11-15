//! Landscape Smoothing over the retail dats: the drawn ground is retail's height plus the drawn
//! offset of `pyreal_terrain::land::drawn`, and retail's own triangles with it off.

use glam::{Vec2, Vec3};
use pyreal_engine::drawn::{Around, BLOCK_SIDE, STEP, SUB};
use pyreal_engine::land::LandField;
use pyreal_engine::terrain::build_block;
use pyreal_engine::world::Planner;
use pyreal_terrain::land::drawn::{retail_height, sample, BlockField};
use pyreal_terrain::land::mesh::height_table;
use pyreal_world_data::landblock::load_region;

use super::Requests;

/// Holtburg's block planned and its ground built, with Landscape Smoothing or without: every
/// vertex at retail's height plus the offset the terrain crate works out (nought with it off), the
/// offset rounding the ground somewhere and nought on the town's pins; the land field draws the
/// same heights and lifts a body standing there by about the offset.
#[test]
fn the_drawn_ground_is_retails_height_and_the_offset_and_retails_own_with_it_off() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = load_region(&store).expect("the region");
    let table = height_table(&region);
    let block = (0xA9, 0xB4);
    let started = std::time::Instant::now();
    let plain = Planner::new(&region).block(&store, &region, block);
    let plain_ms = started.elapsed().as_secs_f64() * 1e3;
    let started = std::time::Instant::now();
    let smooth = Planner::new(&region)
        .smoothing(2)
        .block(&store, &region, block);
    let smooth_ms = started.elapsed().as_secs_f64() * 1e3;
    eprintln!("Holtburg planned in {plain_ms:.1} ms, {smooth_ms:.1} ms smoothed");
    assert!(plain.drawn.is_none() && plain.land.as_ref().unwrap().drawn.is_none());
    let field: &BlockField = smooth.drawn.as_deref().expect("its drawn offset");
    let src = Around {
        records: &smooth.around,
        table: &table,
    };
    let lb = smooth.record.as_ref().expect("Holtburg's record");
    let corner = Vec2::new(0xA9 as f32, 0xB4 as f32) * 192.0;
    let retail = |p: Vec2| retail_height(&src, corner.x + p.x, corner.y + p.y);

    let off = build_block(
        lb,
        &region,
        &table,
        0xA9,
        0xB4,
        &|_, _| None,
        0,
        &mut Requests::default(),
        None,
    );
    assert_eq!(off.mesh.triangle_count(), 128);
    for v in &off.mesh.vertices {
        let p = Vec2::new(v.position[0], v.position[1]);
        assert!((v.position[2] - retail(p)).abs() < 5e-3, "{p}");
    }

    let on = build_block(
        lb,
        &region,
        &table,
        0xA9,
        0xB4,
        &|_, _| None,
        0,
        &mut Requests::default(),
        Some(field),
    );
    assert_eq!(on.mesh.triangle_count(), 128 * SUB * SUB);
    let mut most: f32 = 0.0;
    for v in &on.mesh.vertices {
        let p = Vec2::new(v.position[0], v.position[1]);
        let offset = sample(&field.offset, p.x, p.y);
        assert!((v.position[2] - (retail(p) + offset)).abs() < 5e-3, "{p}");
        most = most.max(offset.abs());
    }
    assert!(most > 0.2, "the offset rounds nothing: {most}");
    // The town's buildings keep their ground.
    let info = smooth.info.as_ref().expect("Holtburg's buildings");
    for b in &info.buildings {
        let (x, y) = (b.frame.origin.x, b.frame.origin.y);
        assert!(sample(&field.offset, x, y).abs() < 1e-4, "{x} {y}");
    }

    let mut land = LandField::new(block, (1, 1), Vec2::ZERO);
    land.add_block(smooth.land.as_ref().unwrap());
    land.finish(&pyreal_engine::water::WaterTable::default());
    let k = 37 * BLOCK_SIDE + 21;
    assert!(land.drawn.len() == BLOCK_SIDE * BLOCK_SIDE);
    let p = Vec2::new(21.0 * STEP, 37.0 * STEP) + Vec2::splat(0.5);
    let z = land.drawn_height(p).expect("in the field");
    assert!((z - (retail(p) + land.drawn_offset(p))).abs() < 5e-3);
    assert!((land.drawn_offset(p) - land.drawn[k]).abs() < 0.5);
    let lift = land.lift(p.extend(retail(p)));
    assert!((lift - land.drawn_offset(p)).abs() < 0.3, "{lift}");
    let none = LandField::new(block, (1, 1), Vec2::ZERO);
    assert_eq!(none.lift(Vec3::new(10.0, 10.0, 0.0)), 0.0);
}
