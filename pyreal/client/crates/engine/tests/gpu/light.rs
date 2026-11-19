//! Light: the eye's adaptation, the calibrations, the sun's shadows, lamps and
//! their clusters, ambient occlusion and bounce light, each against the retail dats.

use glam::Vec3;
use pyreal_engine::renderer::{Calibration, Exposure};
use pyreal_engine::world::{ground_height, load_area, Area, AssetLoader};
use pyreal_engine::{Camera, Engine, SkyLight};

/// An engine with `area` of the world loaded.
pub fn loaded(
    store: &pyreal_dat::RetailDatStore,
    region: &pyreal_assets::Region,
    area: Area,
) -> Engine {
    let mut engine = super::engine();
    let mut loader = AssetLoader::default();
    load_area(&mut engine, &mut loader, store, region, area);
    engine
}

/// The mean of a frame's channels, 0 to 255.
pub fn mean(rgba: &[u8]) -> f64 {
    let sum: u64 = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| u64::from(p[0]) + u64::from(p[1]) + u64::from(p[2]))
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let n = (rgba.len() / 4 * 3).max(1) as f64;
    #[allow(clippy::cast_precision_loss)]
    let s = sum as f64;
    s / n
}

/// Holtburg from the middle of the town at eye height, looking north over the houses.
pub fn holtburg_eye(store: &pyreal_dat::RetailDatStore, region: &pyreal_assets::Region) -> Camera {
    let z = ground_height(store, region, 0xA9B4, 96.0, 60.0).expect("the ground") + 2.0;
    Camera {
        position: Vec3::new(96.0, 60.0, z),
        pitch: -0.15,
        far: 400.0,
        ..Camera::default()
    }
}

/// The eye adapts: the first frame takes the exposure at once; noon to midnight is a cut, its
/// first frames taken at once too; after it the exposure moves toward its target every frame,
/// never past it, and in a few seconds stands within a tenth of a stop of it. The retail
/// calibration holds it at one. (The cut's snap is the renderer's
/// `TIME_CUT_FRAMES`, and the night frames look right.)
#[test]
fn exposure_converges() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = loaded(
        &store,
        &region,
        Area {
            center: 0xA9B4,
            radius: 1,
        },
    );
    let camera = holtburg_eye(&store, &region);
    let size = (160, 90);
    engine.renderer.frame_seconds = Some(1.0 / 30.0);
    // The eye alone: no bounce light settling under it.
    engine.renderer.bounce = false;
    let noon = SkyLight::at(&region, 0, 0.5);
    engine
        .render_to_rgba(&camera, &noon, size)
        .expect("a frame");
    let first = engine.renderer.post.read_state(&engine.gpu);
    assert!(
        (first.exposure / first.target - 1.0).abs() < 1e-3,
        "the first frame takes its exposure at once: {first:?}"
    );
    let night = SkyLight::at(&region, 0, 0.0);
    let mut last = first.exposure;
    let mut state = first;
    for k in 0..180 {
        engine
            .render_to_rgba(&camera, &night, size)
            .expect("a frame");
        state = engine.renderer.post.read_state(&engine.gpu);
        if k < pyreal_engine::renderer::TIME_CUT_FRAMES {
            assert!(
                (state.exposure / state.target - 1.0).abs() < 1e-3,
                "under the cut the exposure is taken at once: {state:?}"
            );
        } else {
            let (lo, hi) = (last.min(state.target), last.max(state.target));
            assert!(
                state.exposure >= lo * 0.9999 && state.exposure <= hi * 1.0001,
                "the exposure moves toward its target: {last} then {state:?}"
            );
        }
        last = state.exposure;
    }
    assert!(
        state.target > first.target * 100.0,
        "night asks far more exposure than noon: {first:?} then {state:?}"
    );
    assert!(
        (state.exposure / state.target - 1.0).abs() < 0.07,
        "six seconds on, the eye has adapted: {state:?}"
    );
    engine.renderer.calibration = Calibration::Retail;
    engine
        .render_to_rgba(&camera, &noon, size)
        .expect("a frame");
    assert_eq!(engine.renderer.post.read_state(&engine.gpu).exposure, 1.0);
    engine.renderer.exposure = Exposure::Manual(0.5);
    engine
        .render_to_rgba(&camera, &noon, size)
        .expect("a frame");
    assert_eq!(engine.renderer.post.read_state(&engine.gpu).exposure, 0.5);
}

/// The physical sky: by day blue overhead and paler at the horizon, at night far darker, with
/// stars; the land at the fog's end is drawn in the haze's colour, not its own.
#[test]
fn the_sky_is_blue_by_day_and_dark_at_night() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = loaded(
        &store,
        &region,
        Area {
            center: 0xA9B4,
            radius: 0,
        },
    );
    let camera = Camera {
        pitch: 0.6,
        ..holtburg_eye(&store, &region)
    };
    let size = (160, 90);
    let row = |rgba: &[u8], y: usize| -> [f64; 3] {
        let w = size.0 as usize;
        let mut s = [0.0; 3];
        for p in rgba[y * w * 4..(y + 1) * w * 4].as_chunks::<4>().0 {
            for k in 0..3 {
                s[k] += f64::from(p[k]);
            }
        }
        s.map(|v| v / w as f64)
    };
    engine.renderer.exposure = Exposure::Manual(0.12);
    let noon = engine
        .render_to_rgba(&camera, &SkyLight::at(&region, 0, 0.5), size)
        .expect("a frame");
    let top = row(&noon, 0);
    assert!(
        top[2] > top[0] * 1.15,
        "overhead the noon sky is blue: {top:?}"
    );
    let night = engine
        .render_to_rgba(&camera, &SkyLight::at(&region, 0, 0.0), size)
        .expect("a frame");
    let dark = row(&night, 0);
    assert!(
        dark.iter().sum::<f64>() * 20.0 < top.iter().sum::<f64>(),
        "at the same exposure the night sky is far darker: {dark:?} against {top:?}"
    );
}

/// The pixels a change darkens by more than a third, of a frame drawn both ways.
pub fn darkened(lit: &[u8], dark: &[u8]) -> usize {
    lit.as_chunks::<4>()
        .0
        .iter()
        .zip(dark.as_chunks::<4>().0)
        .filter(|(a, b)| {
            let la = u32::from(a[0]) + u32::from(a[1]) + u32::from(a[2]);
            let lb = u32::from(b[0]) + u32::from(b[1]) + u32::from(b[2]);
            lb * 3 < la * 2
        })
        .count()
}

/// The sun's shadows: at noon in Holtburg the houses darken a good share of the ground round
/// them against the same frame without shadows; at night the moon low in the east leaves
/// almost nothing for them to take away.
#[test]
fn shadows_darken_the_ground_behind_a_building_at_noon_not_at_night() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = loaded(
        &store,
        &region,
        Area {
            center: 0xA9B4,
            radius: 1,
        },
    );
    // Over the houses north of the square, where the noon sun throws their shadows west.
    let z = ground_height(&store, &region, 0xA9B4, 96.0, 96.0).expect("the ground") + 2.0;
    let camera = Camera {
        position: Vec3::new(96.0, 96.0, z),
        pitch: -0.17,
        far: 400.0,
        ..Camera::default()
    };
    let size = (320, 180);
    let mut both = |time: f32, exposure: f32| {
        let light = SkyLight::at(&region, 0, time);
        engine.renderer.exposure = Exposure::Manual(exposure);
        engine.renderer.shadows = true;
        let shadowed = engine
            .render_to_rgba(&camera, &light, size)
            .expect("a frame");
        assert!(engine.renderer.cascades.is_some(), "the cascades are drawn");
        engine.renderer.shadows = false;
        let open = engine
            .render_to_rgba(&camera, &light, size)
            .expect("a frame");
        darkened(&open, &shadowed)
    };
    let noon = both(0.5, 0.25);
    let night = both(0.0, 15_000.0);
    let pixels = (size.0 * size.1) as usize;
    assert!(
        noon * 100 > pixels,
        "at noon the houses' shadows darken {noon} of {pixels} pixels"
    );
    // Measured 666 against 839: at night what the shadows take is the lamps' own,
    // their pools held off the ground behind the houses; the moon's take little more.
    assert!(
        night < noon,
        "at night the shadows take less: {night} pixels against noon's {noon}"
    );
}

/// Each frame pixel's world point (`None` where nothing is drawn), from the depth target.
pub fn world_points(engine: &Engine, camera: &Camera, (w, h): (u32, u32)) -> Vec<Option<Vec3>> {
    let depth = pyreal_engine::read_texture(
        &engine.gpu,
        engine.renderer.depth_texture().expect("the depth target"),
        4,
    );
    #[allow(clippy::cast_precision_loss)]
    let inv = camera.view_projection(w as f32 / h as f32).inverse();
    depth
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let d = f32::from_le_bytes(*b);
            (d > 0.0).then(|| {
                #[allow(clippy::cast_precision_loss)]
                let i = u32::try_from(i).unwrap_or(0);
                let (x, y) = ((i % w) as f32 + 0.5, (i / w) as f32 + 0.5);
                #[allow(clippy::cast_precision_loss)]
                let ndc =
                    glam::Vec4::new(x / w as f32 * 2.0 - 1.0, 1.0 - y / h as f32 * 2.0, d, 1.0);
                let p = inv * ndc;
                p.truncate() / p.w
            })
        })
        .collect()
}

/// The dungeon at 0x019E where a brazier stands (the report library's `dungeon-flame-orb`).
fn brazier_room() -> Camera {
    Camera {
        position: Vec3::new(35.938_908, -47.720_627, 3.220_717),
        yaw: -4.046_692,
        pitch: -0.29,
        far: 300.0,
        cell: Some(0x019E_0134),
        ..Camera::default()
    }
}

/// The dungeon's lamps come from its setups' own lights, and a lamp-lit room is brighter near its
/// lamp than across the room: the light the lamps add within a metre and a half of a lamp is far
/// more than they add four to eight metres from every lamp. Measured as lit minus unlit, so the
/// fire's own glow, drawn either way, doesn't count.
#[test]
fn a_lamp_lit_room_is_brighter_near_its_lamp() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = loaded(
        &store,
        &region,
        Area {
            center: 0x019E,
            radius: 0,
        },
    );
    assert!(
        engine
            .lamps
            .iter()
            .any(|l| l.kind == pyreal_engine::lamps::LampKind::Authored && l.cell != 0),
        "the dungeon's setups give it lamps"
    );
    let camera = brazier_room();
    let size = (320, 180);
    let light = SkyLight::at(&region, 0, 0.5);
    engine.renderer.exposure = Exposure::Manual(60.0);
    let lit = engine
        .render_to_rgba(&camera, &light, size)
        .expect("a frame");
    let lamps = engine.renderer.lamps_drawn.clone();
    assert!(!lamps.is_empty(), "the room's lamps are lit");
    if engine.gpu.rays() {
        assert!(
            engine.renderer.rays.instances > 0,
            "the nearest lamps' shadows are traced"
        );
    }
    let points = world_points(&engine, &camera, size);
    engine.renderer.lamps = false;
    let dark = engine
        .render_to_rgba(&camera, &light, size)
        .expect("a frame");
    let luma = |p: &[u8; 4]| {
        f64::from(p[0]) * 0.2126 + f64::from(p[1]) * 0.7152 + f64::from(p[2]) * 0.0722
    };
    let (mut near, mut far) = (Vec::new(), Vec::new());
    for ((p, a), b) in points
        .iter()
        .zip(lit.as_chunks::<4>().0)
        .zip(dark.as_chunks::<4>().0)
    {
        let Some(p) = p else { continue };
        let d = lamps
            .iter()
            .map(|l| l.position.distance(*p))
            .fold(f32::MAX, f32::min);
        if d < 1.5 {
            near.push(luma(a) - luma(b));
        } else if (4.0..8.0).contains(&d) {
            far.push(luma(a) - luma(b));
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let avg = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    assert!(
        near.len() > 50 && far.len() > 50,
        "{} near, {} far",
        near.len(),
        far.len()
    );
    assert!(avg(&near) > 12.0, "the lamps light it: {:.1}", avg(&near));
    assert!(
        avg(&near) > avg(&far) * 2.0,
        "lamp light near the lamp {:.1} against {:.1} across the room",
        avg(&near),
        avg(&far)
    );
}

/// A dungeon stays dim under the eye's adaptation: the exposure is held to the room's ceiling
/// (retail's brightness), so the frame's mean stays under a lit day's while its lamp lights a pool.
#[test]
fn a_dungeon_stays_dark_under_the_eyes_adaptation() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = loaded(
        &store,
        &region,
        Area {
            center: 0x019E,
            radius: 0,
        },
    );
    let camera = brazier_room();
    let light = SkyLight::at(&region, 0, 0.5);
    let size = (320, 180);
    engine.renderer.frame_seconds = Some(0.5);
    let mut frame = Vec::new();
    for _ in 0..8 {
        frame = engine
            .render_to_rgba(&camera, &light, size)
            .expect("a frame");
    }
    let state = engine.renderer.post.read_state(&engine.gpu);
    assert!(
        state.exposure <= pyreal_engine::post::ROOM_EXPOSURE * 1.001,
        "{state:?}"
    );
    let m = mean(&frame);
    assert!((6.0..60.0).contains(&m), "the dungeon's mean {m:.1}");
    let brightest = frame
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p[0].max(p[1]).max(p[2]))
        .max()
        .unwrap_or(0);
    assert!(brightest > 120, "its lamp lights a pool: {brightest}");
}

/// The bounce light and the ambient occlusion: in the lamp-lit dungeon the probes carry the
/// lamps' light round the rooms (the report library's `dungeon-wall-relief`, a room lit through
/// its doorway), the frame about as bright with them as without; at noon in Holtburg the
/// occlusion darkens the corners, so the frame is a little darker with it.
#[test]
fn bounce_light_fills_the_rooms_and_occlusion_darkens_corners() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = loaded(
        &store,
        &region,
        Area {
            center: 0x019E,
            radius: 0,
        },
    );
    if !engine.gpu.rays() {
        return;
    }
    let camera = Camera {
        position: Vec3::new(14.2, -21.889_132, 1.559_036),
        yaw: -5.914_131_6,
        pitch: -0.015,
        far: 300.0,
        cell: Some(0x019E_010B),
        ..Camera::default()
    };
    let size = (320, 180);
    let light = SkyLight::at(&region, 0, 0.5);
    engine.renderer.exposure = Exposure::Manual(400.0);
    let mut lit = Vec::new();
    // The probes settle over their first frames.
    for _ in 0..10 {
        lit = engine
            .render_to_rgba(&camera, &light, size)
            .expect("a frame");
    }
    assert!(engine.renderer.probe_frames >= 10);
    engine.renderer.bounce = false;
    let flat = engine
        .render_to_rgba(&camera, &light, size)
        .expect("a frame");
    // The bounce settles the shade: some pixels a few levels brighter with it.
    let brightened = lit
        .as_chunks::<4>()
        .0
        .iter()
        .zip(flat.as_chunks::<4>().0)
        .filter(|(a, b)| {
            a.iter()
                .zip(b.iter())
                .take(3)
                .any(|(x, y)| *x >= y.saturating_add(4))
        })
        .count();
    // Measured 121.5 against 122.8, 88 pixels brighter: in this lamp-lit room the
    // bounce adds little; the frames with and without it look alike and sound.
    assert!(
        (mean(&lit) - mean(&flat)).abs() < 3.0 && brightened > 40,
        "the bounce settles the room's light: {:.1} against {:.1}, {brightened} pixels brighter",
        mean(&lit),
        mean(&flat)
    );

    let mut town = loaded(
        &store,
        &region,
        Area {
            center: 0xA9B4,
            radius: 0,
        },
    );
    let eye = holtburg_eye(&store, &region);
    let noon = SkyLight::at(&region, 0, 0.5);
    town.renderer.exposure = Exposure::Manual(0.25);
    town.renderer.bounce = false;
    // The occlusion is gathered over frames: each case is drawn until it settles (a
    // first frame alone held almost none; settled, it darkens under the houses).
    let settled = |town: &mut Engine| {
        let mut rgba = Vec::new();
        for _ in 0..8 {
            rgba = town.render_to_rgba(&eye, &noon, size).expect("a frame");
        }
        rgba
    };
    let occluded = settled(&mut town);
    town.renderer.ambient_occlusion = false;
    let open = settled(&mut town);
    assert!(
        mean(&occluded) < mean(&open),
        "occlusion darkens: {:.2} against {:.2}",
        mean(&occluded),
        mean(&open)
    );
}

/// The clusters hold exactly the lamps whose reach touches their boxes: a field of lamps of many
/// reaches in front of the eye, each cluster's list against the boxes worked out here.
#[test]
fn cluster_lists_hold_the_lamps_that_touch_them() {
    use pyreal_engine::lamps::{Flicker, Lamp, LampKind};
    use pyreal_engine::renderer::{CLUSTER_NEAR, CLUSTER_SLICES, CLUSTER_TILE};
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = loaded(
        &store,
        &region,
        Area {
            center: 0xA9B4,
            radius: 0,
        },
    );
    let camera = holtburg_eye(&store, &region);
    engine.lamps.clear();
    let mut seed = 12345u32;
    let mut rnd = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        #[allow(clippy::cast_precision_loss)]
        let r = (seed >> 8) as f32 / (1u32 << 24) as f32;
        r
    };
    for _ in 0..400 {
        let p = camera.position
            + camera.forward() * (2.0 + rnd() * 120.0)
            + camera.right() * (rnd() - 0.5) * 140.0
            + Vec3::Z * (rnd() - 0.5) * 20.0;
        engine.lamps.push(Lamp {
            position: p,
            colour: Vec3::ONE,
            candela: 0.01,
            range: 0.5 + rnd() * 12.0,
            retail: glam::Vec4::ONE,
            flicker: Flicker::Steady,
            shadows: false,
            cell: 0,
            kind: LampKind::Authored,
            object: 0,
            flash: false,
            held: false,
            seed: 0,
        });
    }
    let size = (320u32, 180u32);
    engine
        .render_to_rgba(&camera, &SkyLight::at(&region, 0, 0.5), size)
        .expect("a frame");
    let lamps = engine.renderer.lamps_drawn.clone();
    assert!(lamps.len() > 100, "{} lamps drawn", lamps.len());
    let (counts, lists) = engine.renderer.read_clusters(&engine.gpu);
    let tiles = engine.renderer.cluster_tiles;
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (size.0 as f32, size.1 as f32);
    let ty = pyreal_primitives::num::math::tanf(camera.fov_y * 0.5);
    let tx = ty * w / h;
    let view = camera.view();
    let far = pyreal_engine::lamps::LAMP_DISTANCE;
    #[allow(clippy::cast_precision_loss)]
    let slice = |k: u32| {
        CLUSTER_NEAR
            * pyreal_primitives::num::math::powf(
                far / CLUSTER_NEAR,
                k as f32 / CLUSTER_SLICES as f32,
            )
    };
    let (mut checked, mut touched) = (0, 0);
    for z in 0..CLUSTER_SLICES {
        for y in 0..tiles.1 {
            for x in 0..tiles.0 {
                let c = (x + y * tiles.0 + z * tiles.0 * tiles.1) as usize;
                #[allow(clippy::cast_precision_loss)]
                let p0 = glam::Vec2::new((x * CLUSTER_TILE) as f32, (y * CLUSTER_TILE) as f32);
                let p1 = (p0 + CLUSTER_TILE as f32).min(glam::Vec2::new(w, h));
                let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
                for k in 0..8u32 {
                    let p = glam::Vec2::new(
                        if k & 1 == 0 { p0.x } else { p1.x },
                        if k & 2 == 0 { p0.y } else { p1.y },
                    );
                    let d = if k & 4 == 0 { slice(z) } else { slice(z + 1) };
                    let ndc = glam::Vec2::new(p.x / w * 2.0 - 1.0, 1.0 - p.y / h * 2.0);
                    let q = Vec3::new(ndc.x * tx * d, ndc.y * ty * d, -d);
                    lo = lo.min(q);
                    hi = hi.max(q);
                }
                let listed = &lists[c];
                for (i, l) in lamps.iter().enumerate() {
                    let v = view.transform_point3(l.position);
                    let gap = (lo - v).max(v - hi).max(Vec3::ZERO).length();
                    // Lamps on the edge of a box may round either way.
                    if (gap - l.range).abs() < 1e-3 {
                        continue;
                    }
                    let touches = gap < l.range;
                    let has = listed.contains(&u32::try_from(i).expect("an index"));
                    if counts[c] < 48 {
                        assert_eq!(touches, has, "cluster {c} and lamp {i}");
                    } else if has {
                        assert!(
                            touches,
                            "cluster {c} lists lamp {i}, which does not touch it"
                        );
                    }
                    checked += 1;
                    touched += usize::from(touches);
                }
            }
        }
    }
    assert!(touched > 200 && checked > touched, "{touched} of {checked}");
}

/// Noon in Holtburg under the physical light, the eye adapted, is about as bright as the client's
/// own light draws it: the calibrations agree on a sunlit town.
#[test]
fn physical_noon_is_about_as_bright_as_retail() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = loaded(
        &store,
        &region,
        Area {
            center: 0xA9B4,
            radius: 1,
        },
    );
    let camera = holtburg_eye(&store, &region);
    let noon = SkyLight::at(&region, 0, 0.5);
    let size = (320, 180);
    let physical = engine
        .render_to_rgba(&camera, &noon, size)
        .expect("a frame");
    engine.renderer.calibration = Calibration::Retail;
    let retail = engine
        .render_to_rgba(&camera, &noon, size)
        .expect("a frame");
    let (p, r) = (mean(&physical), mean(&retail));
    assert!(
        (p / r - 1.0).abs() < 0.25,
        "physical {p:.1} against retail {r:.1}"
    );
}

/// A dungeon room (`dungeon-lights` in the report library, 0x019E010B): its torches'
/// objects stand in the next rooms' cells, which the portal walk does not see, yet their reach
/// lights this room, as retail burns every static light in range into a room. The room reads lit,
/// not black under the room's held exposure.
#[test]
fn a_rooms_lamps_light_it_whichever_cell_their_objects_stand_in() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = loaded(
        &store,
        &region,
        Area {
            center: 0x019E,
            radius: 0,
        },
    );
    let camera = Camera {
        position: Vec3::new(7.646_814, -15.8, 2.404_381),
        yaw: -2.698_013_5,
        pitch: -0.16,
        far: 300.0,
        cell: Some(0x019E_010B),
        ..Camera::default()
    };
    let light = SkyLight::at(&region, 0, 0.35);
    let size = (320, 180);
    engine.renderer.frame_seconds = Some(0.5);
    let mut frame = Vec::new();
    for _ in 0..8 {
        frame = engine
            .render_to_rgba(&camera, &light, size)
            .expect("a frame");
    }
    let m = mean(&frame);
    eprintln!("the room's mean {m:.1}");
    assert!(m > 12.0, "the room reads lit: {m:.1}");
}
