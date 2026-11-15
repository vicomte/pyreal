//! The waves' heights: about retail's level on the whole, the deep sea's metre and more, shrinking
//! to nothing before the shore, and none with Water Detail at Low.

use glam::{Vec2, Vec3};
use pyreal_engine::land::{BlockLand, LandField};
use pyreal_engine::shore::ShoreField;
use pyreal_engine::water::{
    frame_bytes, height_at, height_reach, shore_phases, wave_constants, HeightInputs, Heights,
    WaterTable, WAVES_BYTES,
};

/// Nine blocks a side of deep sea at height 2, but for a small island in the middle block.
fn sea() -> LandField {
    island(2.0)
}

/// The same with the island's land `top` metres high (a cliff when high).
fn island(top: f32) -> LandField {
    let blocks = 9usize;
    let mut f = LandField::new((100, 100), (blocks, blocks), Vec2::new(-864.0, -864.0));
    for b in 0..blocks * blocks {
        let block = (
            100 + i32::try_from(b % blocks).unwrap(),
            100 + i32::try_from(b / blocks).unwrap(),
        );
        let island = block == (104, 104);
        let terrain = std::array::from_fn(|k| {
            if island && (3..6).contains(&(k / 9)) && (3..6).contains(&(k % 9)) {
                1 << 2
            } else {
                20 << 2
            }
        });
        f.add_block(&BlockLand {
            block,
            heights: std::array::from_fn(|k| {
                if island && (3..6).contains(&(k / 9)) && (3..6).contains(&(k % 9)) {
                    top
                } else {
                    2.0
                }
            }),
            terrain,
            buildings: Vec::new(),
            footprints: Vec::new(),
            clearings: Vec::new(),
            drawn: None,
        });
    }
    f.finish(&WaterTable::default());
    f.shore = Some(std::sync::Arc::new(ShoreField::build(&f)));
    f
}

/// The surface's height over the level at `p` every tenth of a second for ten minutes.
fn heights(f: &LandField, p: Vec2, reach: f32) -> Vec<f32> {
    heights_for(f, p, reach, 6000)
}

/// The same for `n` tenths of a second.
fn heights_for(f: &LandField, p: Vec2, reach: f32, n: u32) -> Vec<f32> {
    let table = WaterTable::default();
    let gain = table.shore_table();
    (0..n)
        .map(|t| {
            let clock = 1.0e6 + f64::from(t) * 0.1;
            let global = Vec2::new(19_200.0, 19_200.0);
            let w = wave_constants(clock, global);
            let at = HeightInputs {
                waves: &w,
                shore: shore_phases(clock),
                table: &table,
                shore_table: &gain,
                gust: 1.0,
                eye: p.extend(3.0),
                reach,
                band: 1.5,
                global,
            };
            height_at(f, &at, p).expect("water") - 2.0
        })
        .collect()
}

#[test]
fn the_open_sea_rises_and_falls_a_metre_or_so_about_the_level() {
    let f = sea();
    // 500 m from the island.
    let h = heights(&f, Vec2::new(500.0, -300.0), height_reach(1));
    #[allow(clippy::cast_precision_loss)]
    let mean = h.iter().sum::<f32>() / h.len() as f32;
    assert!(mean.abs() < 0.03, "mean {mean}");
    // Crest to trough in each ten seconds.
    let ranges: Vec<f32> = h
        .chunks(100)
        .map(|c| {
            c.iter().copied().fold(f32::MIN, f32::max) - c.iter().copied().fold(f32::MAX, f32::min)
        })
        .collect();
    #[allow(clippy::cast_precision_loss)]
    let typical = ranges.iter().sum::<f32>() / ranges.len() as f32;
    let largest = ranges.iter().copied().fold(0.0, f32::max);
    assert!((0.9..1.6).contains(&typical), "typical {typical}");
    assert!((1.4..2.4).contains(&largest), "largest {largest}");
}

#[test]
fn the_waves_shrink_before_the_shore_and_no_crest_meets_the_waterline() {
    let f = sea();
    let table = WaterTable::default();
    let look = &table.looks[4];
    assert_eq!(look.name, "deep sea");
    // Walking in from the open sea to the island's edge, a metre at a time.
    for step in 0..120 {
        #[allow(clippy::cast_precision_loss)]
        let p = Vec2::new(140.0 - step as f32, 12.0);
        let (_, inside) = f.water_at(p).expect("in the field");
        if inside <= 0.0 {
            continue;
        }
        let t = 1.0 - (inside / look.shelf).clamp(0.0, 1.0);
        let deep = look.depth * (1.0 - t * t);
        let most = heights(&f, p, height_reach(1))
            .iter()
            .fold(0.0f32, |m, h| m.max(h.abs()));
        assert!(
            most <= 0.39 * deep + 1e-3,
            "{inside} m in: {most} m with the bed {deep} m down"
        );
        if deep < 0.5 {
            assert!(most < 0.05, "{inside} m in: {most} m");
        }
    }
}

#[test]
fn low_keeps_the_water_at_the_level() {
    let f = sea();
    assert_eq!(height_reach(pyreal_engine::water::finest_level(0)), 0.0);
    assert!(heights(&f, Vec2::new(500.0, -300.0), 0.0)
        .iter()
        .all(|h| h.abs() < 1e-6));
}

#[test]
fn the_frames_constants_carry_the_heights_reach_and_the_bodies() {
    let flat = frame_bytes(100.0, Vec2::ZERO, &Heights::default());
    let waving = frame_bytes(
        100.0,
        Vec2::ZERO,
        &Heights {
            reach: 192.0,
            bodies: vec![Vec3::new(1.0, 2.0, 3.0)],
        },
    );
    assert_eq!(flat.len() as u64, WAVES_BYTES);
    assert_eq!(waving.len() as u64, WAVES_BYTES);
    assert_ne!(flat, waving);
}

fn range(h: &[f32]) -> f32 {
    h.iter().copied().fold(f32::MIN, f32::max) - h.iter().copied().fold(f32::MAX, f32::min)
}

#[test]
fn the_shore_waves_close_up_as_the_bed_rises_and_run_at_deep_waters_pace_offshore() {
    use pyreal_engine::water::{sea_shore_wave, wavenumber, SHORE_ROW, SHORE_STEP, SHORE_STEPS};
    let table = WaterTable::default();
    let gain = table.shore_table();
    let deep = 4;
    assert_eq!(table.looks[deep].name, "deep sea");
    let row = &gain[deep * SHORE_ROW..(deep + 1) * SHORE_ROW];
    let k = |i: usize| (row[i + 1] - row[i]) / SHORE_STEP;
    // Shorter toward the edge, all the way in.
    for i in 1..SHORE_STEPS - 1 {
        assert!(k(i - 1) >= k(i) - 1e-4, "{i}: {} then {}", k(i - 1), k(i));
    }
    // Past the shelf, the long swell's wave over the bed's deepest (12 m: a little shorter than
    // in deep water).
    let wave = sea_shore_wave();
    let open = row[SHORE_STEPS];
    assert!((k(200) - open).abs() < 1e-3 * open, "{} {open}", k(200));
    assert!((f64::from(open) - wavenumber(wave.omega(), 12.0)).abs() < 1e-4);
    assert!(open > wave.wavenumber() && open < 1.2 * wave.wavenumber());
    // A metre in (the bed about 0.4 m down) it is the shallow water's, √(ω² / g h).
    let h = f64::from(pyreal_engine::water::bed_depth(1.0, 12.0, 60.0));
    let shallow = wave.omega() / (9.81 * h).sqrt();
    assert!(
        (f64::from(k(2)) / shallow - 1.0).abs() < 0.1,
        "{} {shallow}",
        k(2)
    );
    assert!((wavenumber(wave.omega(), 1.0e4) - f64::from(wave.wavenumber())).abs() < 1e-4);
}

#[test]
fn the_swash_starts_up_as_each_shore_waves_foam_reaches_the_edge() {
    use pyreal_engine::water::{surf_phase, SHORE_FRONT, SURF_PERIOD};
    let tau = std::f64::consts::TAU;
    for t in 0..50 {
        let clock = 1.0e6 + f64::from(t) * 0.37;
        let at = f64::from(shore_phases(clock)[0]);
        let share = ((SHORE_FRONT - at) / tau).rem_euclid(1.0);
        assert!((share - f64::from(surf_phase(clock))).abs() < 1e-5);
    }
    // The sea's shore wave keeps the long swell's period.
    let wave = pyreal_engine::water::sea_shore_wave();
    assert!((tau / wave.omega() - SURF_PERIOD).abs() < 1e-3);
}

#[test]
fn against_a_cliff_the_waves_keep_their_height_to_a_few_metres_out_and_none_at_its_foot() {
    let cliff = island(30.0);
    let beach = sea();
    let reach = height_reach(1);
    // The island's last water vertices lie 48 m from its middle; on the beach the water reaches
    // halfway to the land, 36 m out, and on the cliff the ground rises from the vertex itself.
    let foot = Vec2::new(48.0, 0.0);
    assert!(cliff.height_at(foot).expect("ground") - 2.0 < 0.01);
    assert!(range(&heights_for(&cliff, foot, reach, 600)) < 0.01);
    // Half a metre out from the foot, a few centimetres.
    assert!(range(&heights_for(&cliff, foot + Vec2::new(0.5, 0.0), reach, 600)) < 0.08);
    // Six metres out, as high as the beach's waves eighteen metres from its edge.
    let off_cliff = range(&heights_for(&cliff, foot + Vec2::new(6.0, 0.0), reach, 600));
    let off_beach = range(&heights_for(&beach, foot + Vec2::new(6.0, 0.0), reach, 600));
    assert!(off_cliff > 0.6 * off_beach, "{off_cliff} {off_beach}");
    // And six metres out from the beach's edge, a gentle bed's, far less.
    let near_beach = range(&heights_for(&beach, Vec2::new(42.0, 0.0), reach, 600));
    assert!(off_cliff > 1.5 * near_beach, "{off_cliff} {near_beach}");
}
