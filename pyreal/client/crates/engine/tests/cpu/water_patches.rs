//! The water's patches: they cover the water once, neighbours differ by a level at most, and
//! where they do the finer one's edge is wholly morphed onto the coarser one's vertices, which
//! are not morphing: no cracks and no T-junctions.

use glam::{Vec2, Vec3};
use pyreal_engine::land::{BlockLand, LandField};
use pyreal_engine::water::{
    finest_level, WaterLayout, WaterNode, WaterTable, FINEST, LEVELS, PATCH,
};

/// Nine blocks a side of deep sea at height 2, but for an island of land in the middle block.
fn sea() -> LandField {
    let blocks = 9usize;
    let mut f = LandField::new((100, 100), (blocks, blocks), Vec2::new(-864.0, -864.0));
    for b in 0..blocks * blocks {
        let block = (
            100 + i32::try_from(b % blocks).unwrap(),
            100 + i32::try_from(b / blocks).unwrap(),
        );
        let island = block == (104, 104);
        let terrain = std::array::from_fn(|k| {
            if island && (2..7).contains(&(k / 9)) && (2..7).contains(&(k % 9)) {
                1 << 2
            } else {
                20 << 2
            }
        });
        f.add_block(&BlockLand {
            block,
            heights: [2.0; 81],
            terrain,
            buildings: Vec::new(),
            footprints: Vec::new(),
            clearings: Vec::new(),
            drawn: None,
        });
    }
    f.finish(&WaterTable::default());
    f
}

/// A node's square in finest steps: its low corner and side.
fn square(n: &WaterNode) -> ([i32; 2], i32) {
    (n.origin, i32::try_from(PATCH << n.level).unwrap())
}

/// How far vertex `grid` of `n` has morphed, as the shader works it out.
fn morphed(f: &LandField, n: &WaterNode, grid: [u32; 2], eye: Vec3) -> f32 {
    let step = (1u32 << n.level) as f32;
    let base = Vec2::new(n.origin[0] as f32, n.origin[1] as f32);
    let at = f.origin + (base + Vec2::new(grid[0] as f32, grid[1] as f32) * step) * FINEST;
    let d = eye.distance(at.extend(2.0));
    ((d - n.morph.0) / (n.morph.1 - n.morph.0)).clamp(0.0, 1.0)
}

fn eyes() -> Vec<Vec3> {
    let mut out = Vec::new();
    for (x, y) in [
        (0.0, 0.0),
        (-300.0, 120.0),
        (37.0, -511.0),
        (700.0, 700.0),
        (-12.3, 5.9),
    ] {
        for z in [3.0, 25.0, 140.0] {
            out.push(Vec3::new(x, y, z));
        }
    }
    out
}

#[test]
fn the_patches_cover_the_water_once() {
    let f = sea();
    let layout = WaterLayout::new(&f);
    for eye in eyes() {
        for finest in [finest_level(0), finest_level(1), finest_level(2)] {
            let nodes = layout.select(eye, &[], finest);
            assert!(nodes.iter().all(|n| n.level >= finest && n.level < LEVELS));
            // Sample every 6 m of the field, inside the island's dry middle or not.
            for sy in 0..288 {
                for sx in 0..288 {
                    let p = [sx * 8 + 4, sy * 8 + 4];
                    let covering = nodes
                        .iter()
                        .filter(|n| {
                            let (o, s) = square(n);
                            (o[0]..o[0] + s).contains(&p[0]) && (o[1]..o[1] + s).contains(&p[1])
                        })
                        .count();
                    // The island's middle cells are more than a cell from any water.
                    let metres = Vec2::new(p[0] as f32, p[1] as f32) * FINEST;
                    let dry = (4.0 * 192.0 + 3.0 * 24.0..4.0 * 192.0 + 5.0 * 24.0)
                        .contains(&metres.x)
                        && (4.0 * 192.0 + 3.0 * 24.0..4.0 * 192.0 + 5.0 * 24.0).contains(&metres.y);
                    if !dry {
                        assert_eq!(covering, 1, "{eye} finest {finest}: {p:?}");
                    } else {
                        assert!(covering <= 1);
                    }
                }
            }
        }
    }
}

#[test]
fn where_levels_meet_the_finer_edge_lies_on_the_coarser_ones_still_vertices() {
    let f = sea();
    let layout = WaterLayout::new(&f);
    let mut meetings = 0;
    for eye in eyes() {
        let nodes = layout.select(eye, &[], 0);
        for a in &nodes {
            for b in &nodes {
                let ((ao, as_), (bo, bs)) = (square(a), square(b));
                // Along each axis, `b` against `a`'s far side.
                for axis in 0..2 {
                    let other = 1 - axis;
                    if ao[axis] + as_ != bo[axis] {
                        continue;
                    }
                    let lo = ao[other].max(bo[other]);
                    let hi = (ao[other] + as_).min(bo[other] + bs);
                    if hi <= lo {
                        continue;
                    }
                    assert!(
                        a.level.abs_diff(b.level) <= 1,
                        "{eye}: levels {} and {}",
                        a.level,
                        b.level
                    );
                    if a.level == b.level {
                        continue;
                    }
                    meetings += 1;
                    let (fine, coarse, fine_side) = if a.level < b.level {
                        (a, b, PATCH)
                    } else {
                        (b, a, 0)
                    };
                    let coarse_side = PATCH - fine_side;
                    // Every odd vertex of the finer edge in the shared stretch is wholly morphed,
                    // and every odd vertex of the coarser edge there not at all.
                    for (n, side, want) in [(fine, fine_side, 1.0), (coarse, coarse_side, 0.0)] {
                        let step = 1i32 << n.level;
                        for t in (1..PATCH).step_by(2) {
                            let along = n.origin[other] + i32::try_from(t).unwrap() * step;
                            if along < lo || along > hi {
                                continue;
                            }
                            let grid = if axis == 0 { [side, t] } else { [t, side] };
                            let k = morphed(&f, n, grid, eye);
                            assert!(
                                k == want,
                                "{eye}: level {} node at {:?}, vertex {grid:?} morphed {k}, not {want}",
                                n.level,
                                n.origin
                            );
                        }
                    }
                }
            }
        }
    }
    assert!(
        meetings > 50,
        "only {meetings} meetings of levels were tried"
    );
}

#[test]
fn low_draws_the_cells_alone_and_high_the_finest_near_the_eye() {
    let f = sea();
    let layout = WaterLayout::new(&f);
    let eye = Vec3::new(0.0, -300.0, 3.0);
    let low = layout.select(eye, &[], finest_level(0));
    assert!(low.iter().all(|n| n.level == LEVELS - 1));
    let high = layout.select(eye, &[], finest_level(2));
    assert!(high.iter().any(|n| n.level == 0));
    let medium = layout.select(eye, &[], finest_level(1));
    assert!(medium.iter().all(|n| n.level >= 1) && medium.iter().any(|n| n.level == 1));
}
