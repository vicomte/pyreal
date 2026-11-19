//! The land field and where it says the water lies.

use glam::Vec2;
use pyreal_engine::land::{BlockLand, LandField};
use pyreal_engine::water::WaterTable;

/// One block of flat land at height 10 with a pond of standing water (type 17) over the vertices
/// `lo..=hi` each way, a marsh (type 4) round it when `marsh`.
fn pond(lo: usize, hi: usize, marsh: bool) -> LandField {
    let mut heights = [10.0; 81];
    let mut terrain = [1u16 << 2; 81];
    for x in 0..9 {
        for y in 0..9 {
            let k = x * 9 + y;
            if (lo..=hi).contains(&x) && (lo..=hi).contains(&y) {
                terrain[k] = 17 << 2;
                heights[k] = 9.0;
            } else if marsh {
                terrain[k] = 4 << 2;
            }
        }
    }
    let mut f = LandField::new((10, 10), (1, 1), Vec2::ZERO);
    f.add_block(&BlockLand {
        block: (10, 10),
        heights,
        terrain,
        buildings: Vec::new(),
        footprints: Vec::new(),
        clearings: Vec::new(),
        drawn: None,
    });
    f.finish(&WaterTable::default());
    f
}

/// The water's edge runs halfway between a water vertex and the land beside it; inside, the water
/// lies at its vertices' own height, and on the bank beside it at the water's level.
#[test]
fn the_edge_runs_halfway_to_the_land_and_the_water_lies_level() {
    let f = pond(3, 5, false);
    let (level, inside) = f.water_at(Vec2::new(96.0, 96.0)).expect("in the field");
    assert!(
        (level - 9.0).abs() < 1e-5 && inside > 30.0,
        "{level} {inside}"
    );
    // Halfway from the pond's last vertex (x 5) to the land (x 6).
    let (_, edge) = f
        .water_at(Vec2::new(5.5 * 24.0, 96.0))
        .expect("in the field");
    assert!(edge.abs() < 1e-3, "{edge}");
    let (bank, out) = f
        .water_at(Vec2::new(5.9 * 24.0, 96.0))
        .expect("in the field");
    assert!(out < 0.0 && (bank - 9.0).abs() < 1e-5, "{bank} {out}");
    assert_eq!(f.bodies.len(), 1);
}

/// A bank between a pool at 0 and one at 30 above it keeps the upper pool level at the top of the
/// fall, rather than dipping into the ground toward the lower one.
#[test]
fn the_upper_pool_stays_level_at_the_top_of_a_fall() {
    let mut heights = [32.0; 81];
    let mut terrain = [1u16 << 2; 81];
    for x in 0..9 {
        for y in 0..=5 {
            let k = x * 9 + y;
            terrain[k] = 17 << 2;
            heights[k] = if x <= 2 { 0.0 } else { 30.0 };
        }
    }
    let mut f = LandField::new((10, 10), (1, 1), Vec2::ZERO);
    f.add_block(&BlockLand {
        block: (10, 10),
        heights,
        terrain,
        buildings: Vec::new(),
        footprints: Vec::new(),
        clearings: Vec::new(),
        drawn: None,
    });
    f.finish(&WaterTable::default());
    let (level, inside) = f
        .water_at(Vec2::new(3.2 * 24.0, 5.45 * 24.0))
        .expect("in the field");
    assert!(
        inside > 0.0 && (level - 30.0).abs() < 1e-4,
        "{level} {inside}"
    );
}

/// A pool at 60 whose ground falls away to 0 past its last vertex (a fall's brink): its edge stops
/// at that vertex, as against a cliff, rather than halfway out over the drop.
#[test]
fn a_pools_edge_stops_at_the_brink_of_a_fall() {
    let mut heights = [0.0; 81];
    let mut terrain = [1u16 << 2; 81];
    for x in 0..9 {
        for y in 0..=4 {
            let k = x * 9 + y;
            terrain[k] = 17 << 2;
            heights[k] = 60.0;
        }
    }
    let mut f = LandField::new((10, 10), (1, 1), Vec2::ZERO);
    f.add_block(&BlockLand {
        block: (10, 10),
        heights,
        terrain,
        buildings: Vec::new(),
        footprints: Vec::new(),
        clearings: Vec::new(),
        drawn: None,
    });
    f.finish(&WaterTable::default());
    let drawn = |y: f32| {
        let p = Vec2::new(4.0 * 24.0, y * 24.0);
        f.water_at(p).expect("in the field").1 - f.bank_at(p).1
    };
    assert!(drawn(3.5) > 0.0, "{}", drawn(3.5));
    assert!(drawn(4.1) <= 0.0, "{}", drawn(4.1));
}

/// A body's look comes from its water's terrain type and its shore: the same pond is a clear lake
/// in grass and a swamp in a marsh.
#[test]
fn a_ponds_look_comes_from_its_shore() {
    let looks = pyreal_engine::water::default_looks();
    let lake = pond(2, 6, false);
    assert_eq!(looks[lake.bodies[0].look].name, "clear lake");
    let swamp = pond(2, 6, true);
    assert_eq!(looks[swamp.bodies[0].look].name, "swamp");
}

/// A body's swash (how far it runs up its shore) is its look's, carried by its water vertices and
/// the land beside them, so the water and the wet ground beside it move their edge together; a
/// swamp barely stirs, a lake runs up a fraction of a metre.
#[test]
fn the_land_beside_a_pond_carries_its_swash() {
    use pyreal_engine::land::{bits, SWASH_STEP};
    let looks = pyreal_engine::water::default_looks();
    let swash = |f: &LandField, x: i32, y: i32| {
        let i = f.index(80 + x, 80 + y).expect("in the field");
        #[allow(clippy::cast_precision_loss)]
        let s = ((f.word[i] >> bits::SWASH_SHIFT) & 63) as f32 * SWASH_STEP;
        s
    };
    for (f, look) in [
        (pond(3, 5, false), "clear lake"),
        (pond(3, 5, true), "swamp"),
    ] {
        let want = looks
            .iter()
            .find(|l| l.name == look)
            .expect("the look")
            .swash;
        // A water vertex, and the bank vertex beside it.
        for (x, y) in [(4, 4), (6, 4)] {
            assert!(
                (swash(&f, x, y) - want).abs() < SWASH_STEP * 0.5,
                "{look} at {x},{y}"
            );
        }
    }
    let lake = looks
        .iter()
        .find(|l| l.name == "clear lake")
        .expect("the lake")
        .swash;
    let sea = looks
        .iter()
        .find(|l| l.name == "shallow sea")
        .expect("the sea")
        .swash;
    assert!(
        (0.2..1.0).contains(&lake) && sea > 2.0 * lake,
        "{lake} {sea}"
    );
}

/// The ripple simulation steps at a fixed rate by the game clock: a held clock runs none, a long
/// pause only a few; its square follows the body nearest the eye.
#[test]
fn the_ripples_step_with_the_clock_and_follow_the_nearest_body() {
    use glam::Vec3;
    use pyreal_engine::water::{Sim, SIM_MOST_STEPS, SIM_STEP};
    let mut sim = Sim::default();
    let eye = Vec3::new(0.0, -5.0, 3.0);
    let body = [Vec3::new(4.0, 3.0, 0.0)];
    assert!(sim.plan(100.0, eye, &body, &[], 0.0).steps.is_empty());
    let two = sim.plan(100.0 + 2.0 * SIM_STEP + 1e-6, eye, &body, &[], 0.0);
    assert_eq!(two.steps.len(), 2);
    let held = sim.plan(100.0 + 2.0 * SIM_STEP + 1e-6, eye, &body, &[], 0.0);
    assert!(held.steps.is_empty());
    let plan = sim.plan(110.0, eye, &body, &[], 0.0);
    assert_eq!(plan.steps.len(), SIM_MOST_STEPS);
    let [x0, y0, side] = plan.area;
    assert!(
        x0 < 4.0 && 4.0 < x0 + side && y0 < 3.0 && 3.0 < y0 + side,
        "{:?}",
        plan.area
    );
}

/// Landscape Smoothing's offset in the land field: the cover reads it at the drawn ground's own
/// spacing, the drawn ground stands that far over retail's, and a body on it is lifted by it, one
/// high in the air not; without it nothing moves.
#[test]
fn the_drawn_offset_lifts_the_ground_and_what_stands_on_it() {
    use glam::Vec3;
    use pyreal_engine::drawn::{BLOCK_SIDE, SUB};
    assert!(pyreal_engine::renderer::source::cover()
        .contains(&format!("const DRAWN_SUB: i32 = {SUB};")));
    let mut f = LandField::new((10, 10), (1, 1), Vec2::ZERO);
    let block = BlockLand {
        block: (10, 10),
        heights: [10.0; 81],
        terrain: [1u16 << 2; 81],
        buildings: Vec::new(),
        footprints: Vec::new(),
        clearings: Vec::new(),
        drawn: None,
    };
    f.add_block(&block);
    f.finish(&WaterTable::default());
    let p = Vec2::new(50.0, 70.0);
    assert_eq!(f.drawn_offset(p), 0.0);
    assert_eq!(f.lift(Vec3::new(50.0, 70.0, 10.0)), 0.0);
    let mut g = LandField::new((10, 10), (1, 1), Vec2::ZERO);
    g.add_block(&BlockLand {
        drawn: Some(std::sync::Arc::new(vec![0.75; BLOCK_SIDE * BLOCK_SIDE])),
        ..block
    });
    g.finish(&WaterTable::default());
    assert_eq!(g.drawn.len(), BLOCK_SIDE * BLOCK_SIDE);
    assert!((g.drawn_offset(p) - 0.75).abs() < 1e-5);
    assert!((g.drawn_height(p).unwrap() - 10.75).abs() < 1e-5);
    assert!((g.lift(Vec3::new(50.0, 70.0, 10.0)) - 0.75).abs() < 1e-5);
    assert_eq!(g.lift(Vec3::new(50.0, 70.0, 30.0)), 0.0);
}
