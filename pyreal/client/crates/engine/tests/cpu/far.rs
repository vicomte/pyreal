//! The far terrain's quadtree: levels coarsen with distance, the near square is left to the exact
//! cells, and the field's bounds hold its heights.

use glam::{Vec2, Vec3};
use pyreal_engine::camera::Camera;
use pyreal_engine::far::{levels_for, ranges, select, FarField, PATCH, TEXEL};
use pyreal_primitives::num::math::{cosf, sinf};

/// A 512-texel field of rolling ground with a 400 m near square in its middle.
fn field() -> FarField {
    let size = 512u32;
    let n = size as usize;
    let heights = (0..n * n)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = ((i % n) as f32, (i / n) as f32);
            sinf(x * 0.05) * 40.0 + cosf(y * 0.03) * 25.0
        })
        .collect();
    let half = size as f32 * TEXEL * 0.5;
    FarField {
        origin: Vec2::splat(-half),
        size,
        extent: size,
        heights,
        colours: vec![[90, 110, 60, 255]; n * n],
        water: Vec::new(),
        water_side: 0,
        near_min: Vec2::splat(-200.0),
        near_max: Vec2::splat(200.0),
        smooth: false,
        prepared: None,
    }
}

#[test]
fn bounds_hold_every_height_of_their_node() {
    let f = field();
    let levels = levels_for(f.size);
    assert_eq!(PATCH << (levels - 1), f.size);
    let b = f.bounds(levels);
    let n = f.size as usize;
    let (lo, hi) = f
        .heights
        .iter()
        .fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
    assert_eq!(b[levels as usize - 1], vec![(lo, hi)], "the root holds all");
    // A level-0 node's box holds its own corner heights.
    let span = PATCH as usize;
    let count = n / span;
    let (i, j) = (5, 7);
    let (l, h) = b[0][j * count + i];
    for (x, y) in [(i * span, j * span), ((i + 1) * span, (j + 1) * span)] {
        let v = f.heights[y * n + x];
        assert!(l <= v && v <= h);
    }
}

#[test]
fn levels_coarsen_with_distance_and_leave_the_near_square_alone() {
    let f = field();
    let levels = levels_for(f.size);
    let b = f.bounds(levels);
    let r = ranges(800.0, levels);
    let cam = Camera {
        position: Vec3::new(0.0, 0.0, 120.0),
        far: 1.0e5,
        ..Camera::default()
    };
    // Looking every way at once: no frustum.
    let nodes = select(&f, &b, &r, cam.position, &[]);
    assert!(!nodes.is_empty());
    for node in &nodes {
        let span = (PATCH << node.level) as f32 * TEXEL;
        let (min, max) = (node.origin, node.origin + Vec2::splat(span));
        assert!(
            !(min.cmpge(f.near_min).all() && max.cmple(f.near_max).all()),
            "{node:?} lies wholly in the near square"
        );
        // A node is never coarser than its distance allows, nor finer than one level below.
        let d = (cam.position.truncate().clamp(min, max) - cam.position.truncate()).length();
        if node.level > 0 {
            assert!(d + 130.0 >= r[node.level as usize - 1], "{node:?} at {d}");
        }
        assert_eq!(node.morph.1, r[node.level as usize]);
    }
    let finest = nodes.iter().filter(|n| n.level == 0).count();
    let coarse = nodes.iter().filter(|n| n.level >= 2).count();
    assert!(finest > 0 && coarse > 0, "{finest} fine, {coarse} coarse");

    // Facing north, the nodes behind the eye are left out.
    let ahead = select(&f, &b, &r, cam.position, &cam.frustum(16.0 / 9.0));
    assert!(ahead.len() < nodes.len());
    assert!(ahead
        .iter()
        .all(|n| n.origin.y + (PATCH << n.level) as f32 * TEXEL > -1.0));
}

/// The levels about a far tree are those its distance picks with the eye's slack, and the next
/// coarser one; the far trees reach farther with the far terrain, thinning past eight rings, and
/// the haze stretches with a reach past Extreme's.
#[test]
fn far_trees_follow_the_far_terrain_and_the_haze_its_reach() {
    use pyreal_engine::far::levels_near;
    use pyreal_engine::world::{far_tree_keep, far_tree_rings, FAR_TREE_BLOCKS, FAR_TREE_MOST};
    assert_eq!(levels_near(1000.0, 500.0, 0.0), 0..=1);
    assert_eq!(levels_near(1000.0, 2500.0, 0.0), 2..=3);
    assert_eq!(levels_near(1000.0, 1900.0, 200.0), 1..=3);
    assert_eq!(far_tree_rings(4, 25), FAR_TREE_BLOCKS);
    assert_eq!(far_tree_rings(4, 6), 2);
    assert_eq!(far_tree_rings(4, 52), 16);
    assert_eq!(far_tree_rings(4, 127), FAR_TREE_MOST);
    assert!((far_tree_keep(FAR_TREE_BLOCKS) - 1.0).abs() < 1e-6);
    assert!((far_tree_keep(16) - 0.25).abs() < 1e-6);
    use pyreal_engine::atmosphere::{fog_end_for, HAZE_REACH};
    assert!((fog_end_for(2400.0, 3000.0) - 2400.0).abs() < 1e-3);
    assert!((fog_end_for(2400.0, HAZE_REACH * 4.0) - 9600.0).abs() < 1e-2);
}
