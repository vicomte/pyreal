//! The water: where retail lays it, and drawn there.

use glam::{Vec2, Vec3};
use pyreal_engine::world::{ground_height, Area};
use pyreal_engine::{Camera, SkyLight};

/// A lake with no marsh about it (the lake at 0x1252, standing fresh water).
const LAKE: u16 = 0x1252;

/// The water's level is retail's: at every water vertex of the lake's block the water lies at the
/// ground's own height there, and anywhere inside the water it lies on retail's triangles of
/// those heights; the lake's body is a clear lake.
#[test]
fn the_lakes_water_lies_at_retails_height() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let engine = super::light::loaded(
        &store,
        &region,
        Area {
            center: LAKE,
            radius: 0,
        },
    );
    let land = engine.land.as_ref().expect("the land field");
    let mut checked = 0;
    for gy in 0..8 {
        for gx in 0..8 {
            let i = land
                .index(land.first.0 + gx, land.first.1 + gy)
                .expect("in the field");
            if !land.is_water(i) {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let p = Vec2::new(gx as f32 * 24.0 + 7.0, gy as f32 * 24.0 + 11.0);
            let (level, inside) = land.water_at(p).expect("in the field");
            if inside <= 12.0 {
                continue;
            }
            let ground = ground_height(&store, &region, LAKE, p.x, p.y).expect("the ground");
            assert!(
                (level - ground).abs() < 1e-3,
                "at {p}: water {level} against the ground {ground}"
            );
            checked += 1;
        }
    }
    assert!(checked > 10, "only {checked} points well inside the lake");
    let looks = pyreal_engine::water::default_looks();
    assert!(
        land.bodies
            .iter()
            .any(|b| b.vertices > 50 && looks[b.look].name == "clear lake"),
        "{:?}",
        land.bodies
    );
}

/// Drawn, the water covers the lake: looking down on it, most of the frame under the horizon is
/// the water's own colour (blue-green, not the bed's silt), and it is still drawn when the frame is
/// drawn again.
#[test]
fn the_lake_is_drawn_as_water() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = super::light::loaded(
        &store,
        &region,
        Area {
            center: LAKE,
            radius: 1,
        },
    );
    let z = ground_height(&store, &region, LAKE, 20.0, 96.0).expect("the ground") + 2.0;
    let camera = Camera {
        position: Vec3::new(20.0, 96.0, z),
        yaw: -std::f32::consts::FRAC_PI_2,
        pitch: -0.5,
        far: 400.0,
        ..Camera::default()
    };
    let noon = SkyLight::at(&region, 0, 0.5);
    let size = (320, 180);
    let with = engine
        .render_to_rgba(&camera, &noon, size)
        .expect("a frame");
    engine.renderer.water_on = false;
    let without = engine
        .render_to_rgba(&camera, &noon, size)
        .expect("a frame");
    let lower = |rgba: &[u8]| -> (f64, f64) {
        let px = &rgba.as_chunks::<4>().0[(size.0 * size.1 / 2) as usize..];
        #[allow(clippy::cast_precision_loss)]
        let n = px.len() as f64;
        (
            px.iter().map(|p| f64::from(p[2])).sum::<f64>() / n,
            px.iter().map(|p| f64::from(p[0])).sum::<f64>() / n,
        )
    };
    let (blue, red) = lower(&with);
    assert!(
        blue > red,
        "the water is blue-green: blue {blue:.1}, red {red:.1}"
    );
    assert_ne!(with, without, "the water pass draws");
}

/// The river north of Holtburg, whose flat shore lies at the water's level.
const RIVER: u16 = 0xA9B5;

/// The deepest dark line across a frame `width` wide: each column of `cols` averaged over `rows`,
/// how much darker (in its darkest channel) it is than both columns `apart` either side of it.
fn darkest_line(
    rgba: &[u8],
    width: u32,
    rows: std::ops::Range<u32>,
    cols: std::ops::Range<u32>,
    apart: usize,
) -> f64 {
    let px = rgba.as_chunks::<4>().0;
    #[allow(clippy::cast_precision_loss)]
    let n = rows.len() as f64;
    let profile: Vec<[f64; 3]> = cols
        .map(|x| {
            let mut c = [0.0; 3];
            for y in rows.clone() {
                let p = px[(y * width + x) as usize];
                for (k, v) in c.iter_mut().enumerate() {
                    *v += f64::from(p[k]) / n;
                }
            }
            c
        })
        .collect();
    (apart..profile.len() - apart)
        .flat_map(|i| (0..3).map(move |k| (i, k)))
        .map(|(i, k)| profile[i - apart][k].min(profile[i + apart][k]) - profile[i][k])
        .fold(0.0, f64::max)
}

/// Where the flat shore meets the river (the owner's `seam` report), the water comes in over the
/// wet ground without a seam: across the shore, no dark line runs along the water's edge, wherever
/// the swash has run up or drawn back.
#[test]
fn the_shore_meets_the_river_without_a_seam() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = super::light::loaded(
        &store,
        &region,
        Area {
            center: RIVER,
            radius: 1,
        },
    );
    let camera = Camera {
        position: Vec3::new(105.59, 55.33, 31.91),
        yaw: -1.13,
        pitch: -0.43,
        fov_y: 79.07f32.to_radians(),
        far: 400.0,
        ..Camera::default()
    };
    let noon = SkyLight::at(&region, 0, 0.5);
    let size = (640u32, 360u32);
    for clock in [1000.0, 1002.0, 1004.0, 1006.0] {
        engine.renderer.clock = clock;
        let mut rgba = Vec::new();
        for _ in 0..3 {
            rgba = engine
                .render_to_rgba(&camera, &noon, size)
                .expect("a frame");
        }
        // The band of the frame where the shore runs up it, the river to its left; out on the
        // water the bed's grain and the waves' glints vary of their own.
        let line = darkest_line(&rgba, size.0, 200..250, 160..330, 8);
        assert!(
            line < 8.0,
            "at {clock} s a line {line:.1} darker than either side runs along the shore"
        );
    }
}

/// Terraced pools: an upper pool at 110 m whose falls drop 30 m to pools at 80 m.
const FALLS: u16 = 0x22E6;

/// From the upper pool's beach (the owner's waterfall report), the lower pools behind and below
/// the beach are not drawn over it: the beach is the bed of the water over it, not of water 30 m
/// down beyond the falls.
#[test]
fn a_lower_pool_is_not_drawn_through_the_upper_pools_beach() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = super::light::loaded(
        &store,
        &region,
        Area {
            center: FALLS,
            radius: 1,
        },
    );
    let camera = Camera {
        position: Vec3::new(107.16, 24.45, 113.82),
        yaw: -5.51,
        pitch: -0.415,
        fov_y: 79.07f32.to_radians(),
        far: 400.0,
        ..Camera::default()
    };
    let noon = SkyLight::at(&region, 0, 0.5);
    let size = (280, 226);
    let rgba = engine
        .render_to_rgba(&camera, &noon, size)
        .expect("a frame");
    // The beach between the eye and the falls.
    let px = rgba.as_chunks::<4>().0;
    let (mut blue, mut n) = (0, 0);
    for y in 66..96 {
        for x in 40..140 {
            let p = px[y * size.0 as usize + x];
            n += 1;
            if p[2] > p[0].saturating_add(8) {
                blue += 1;
            }
        }
    }
    assert!(blue * 10 < n, "{blue} of {n} beach pixels show water");
}

/// Under water drawn as water the bed is drawn with the land's mud picture, so it shows a grain
/// and bend of its own rather than one flat colour.
#[test]
fn the_bed_is_drawn_with_the_lands_mud() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let engine = super::light::loaded(
        &store,
        &region,
        Area {
            center: LAKE,
            radius: 0,
        },
    );
    let (slot, per_m) = engine.renderer.bed.expect("the bed's picture");
    assert!(slot != 0 && per_m > 0.0, "{slot} {per_m}");
}
