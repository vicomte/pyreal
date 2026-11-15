//! The shore field: its distance exact against a brute force, and its way landward on an island,
//! a channel and a bay.

use glam::Vec2;
use pyreal_engine::shore::{half, ShoreField, MOST, TEXEL};

/// A mask from a picture: `#` land, anything else water; the first row is the south.
fn mask(rows: &[&str]) -> (Vec<bool>, usize, usize) {
    let (w, h) = (rows[0].len(), rows.len());
    let water = rows
        .iter()
        .flat_map(|r| r.bytes().map(|b| b != b'#'))
        .collect();
    (water, w, h)
}

/// A mask drawn by `land(x, y)`.
fn drawn(w: usize, h: usize, land: impl Fn(i32, i32) -> bool) -> Vec<bool> {
    (0..w * h)
        .map(|i| {
            let (x, y) = (i32::try_from(i % w).unwrap(), i32::try_from(i / w).unwrap());
            !land(x, y)
        })
        .collect()
}

#[test]
fn the_distance_is_the_brute_forces_to_the_nearest_texel_of_the_other_kind() {
    let (w, h) = (37usize, 29usize);
    // A scatter of land, the same every run.
    let mut seed = 0x2545_f491_u32;
    let water: Vec<bool> = (0..w * h)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 24) > 40
        })
        .collect();
    let s = ShoreField::from_mask(&water, w, h);
    for i in 0..w * h {
        let (x, y) = ((i % w) as f32, (i / w) as f32);
        let near = (0..w * h)
            .filter(|&j| water[j] != water[i])
            .map(|j| Vec2::new((j % w) as f32, (j / w) as f32).distance(Vec2::new(x, y)))
            .fold(f32::MAX, f32::min);
        let want = (near * TEXEL - TEXEL * 0.5).clamp(0.0, MOST);
        let want = if water[i] { want } else { -want };
        assert!(
            (s.distance[i] - want).abs() < 1e-3,
            "texel ({x}, {y}): {} against {want}",
            s.distance[i]
        );
    }
}

#[test]
fn a_field_with_no_shore_is_open_water_every_way() {
    let (w, h) = (9, 7);
    let s = ShoreField::from_mask(&vec![true; w * h], w, h);
    assert!(s.distance.iter().all(|&d| d == MOST));
    assert!(s.width.iter().all(|&d| d == MOST));
    assert!(s.toward.iter().all(|v| *v == Vec2::ZERO));
}

#[test]
fn round_an_island_the_way_landward_points_in_from_every_side() {
    let (w, h) = (101, 101);
    let water = drawn(w, h, |x, y| (x - 50) * (x - 50) + (y - 50) * (y - 50) <= 64);
    let s = ShoreField::from_mask(&water, w, h);
    for (dx, dy) in [(14, 0), (-14, 0), (0, 14), (0, -14), (10, 10), (-10, -10)] {
        let i = usize::try_from((50 + dy) * 101 + 50 + dx).unwrap();
        let v = s.toward[i];
        let inward = -Vec2::new(dx as f32, dy as f32).normalize();
        assert!(
            v.normalize().dot(inward) > 0.97 && v.length() > 0.8,
            "({dx}, {dy}): {v} should point in"
        );
    }
}

#[test]
fn in_a_channel_the_shores_disagree_down_its_middle_and_its_width_is_measured() {
    // Water between two banks, twenty texels apart.
    let (w, h) = (60, 40);
    let water = drawn(w, h, |_, y| !(10..30).contains(&y));
    let s = ShoreField::from_mask(&water, w, h);
    let at = |x: usize, y: usize| y * w + x;
    // Near each bank it points to that bank, and the shores agree.
    let south = s.toward[at(30, 11)];
    let north = s.toward[at(30, 28)];
    assert!(south.y < -0.9 && north.y > 0.9, "{south} {north}");
    // Down the middle they cancel.
    let middle = s.toward[at(30, 19)]
        .length()
        .min(s.toward[at(30, 20)].length());
    assert!(middle < 0.3, "agreement down the middle {middle}");
    // Twenty texels of water, 60 m, give or take a texel.
    let width = s.width[at(30, 15)];
    assert!((width - 60.0).abs() <= TEXEL + 1e-3, "width {width}");
    // On land the way still points landward, so it runs on across the edge.
    assert!(s.toward[at(30, 5)].y < -0.9);
}

#[test]
fn in_a_bay_the_way_landward_turns_to_its_head() {
    let rows = [
        "##############################",
        "##############################",
        "##############################",
        "#######################.......",
        "#####..............###........",
        "####..................#.......",
        "###...........................",
        "##............................",
        "##............................",
        "#.............................",
    ];
    // Drawn north first: the first row of the mask is the south.
    let rows: Vec<&str> = rows.iter().rev().copied().collect();
    let (water, w, h) = mask(&rows);
    let s = ShoreField::from_mask(&water, w, h);
    // Below the bay's head, the way runs north to it.
    let v = s.toward[3 * w + 10];
    assert!(v.normalize().y > 0.8, "{v}");
}

#[test]
fn half_floats_round_to_the_nearest() {
    assert_eq!(half(0.0), 0);
    assert_eq!(half(1.0), 0x3C00);
    assert_eq!(half(-2.0), 0xC000);
    assert_eq!(half(512.0), 0x6000);
    assert_eq!(half(0.333_333), 0x3555);
    assert_eq!(half(1.0e6), 0x7C00);
    assert_eq!(half(6.0e-8), 0x0001);
}
