//! Each of the settings the client's options page has for the engine changes the frame: the
//! weather (rain, snow, wet ground, fresh snow kept to snowy land far off), the air's light (God Rays), the clouds' detail, the
//! lamps' reach (Light Draw Distance) and their flicker.

use glam::Vec3;
use pyreal_engine::weather::Fall;
use pyreal_engine::world::{
    ground_height, load_area, load_area_smoothed, load_far, Area, AssetLoader,
};
use pyreal_engine::{Camera, Engine, SkyLight};

/// Holtburg loaded round its centre, with the region's sky.
fn holtburg() -> (Engine, pyreal_engine::world::Region, f32) {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = super::engine();
    let mut loader = AssetLoader::default();
    load_area(
        &mut engine,
        &mut loader,
        &store,
        &region,
        Area {
            center: 0xA9B4,
            radius: 1,
        },
    );
    let h = pyreal_engine::heavens::Heavens::load(&mut engine, &mut loader, &store, &region);
    engine.heavens = Some(h);
    let z = ground_height(&store, &region, 0xA9B4, 40.0, 150.0).expect("the ground");
    (engine, region, z)
}

/// `frames` frames from `camera` at time of day `time`, a sixtieth of a second apart; the last.
fn frames(
    engine: &mut Engine,
    region: &pyreal_engine::world::Region,
    camera: &Camera,
    time: f32,
    frames: u32,
) -> Vec<u8> {
    let light = SkyLight::at(region, 0, time);
    let mut rgba = Vec::new();
    for _ in 0..frames {
        engine.renderer.clock += 1.0 / 60.0;
        rgba = engine
            .render_to_rgba(camera, &light, (256, 144))
            .expect("a frame");
    }
    rgba
}

fn mean_difference(a: &[u8], b: &[u8]) -> f64 {
    let sum: u64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let m = sum as f64 / a.len().max(1) as f64;
    m
}

fn keep(name: &str, rgba: &[u8]) {
    let Some(dir) = std::env::var_os("PYREAL_TEST_OUT") else {
        return;
    };
    let mut out = b"P6\n256 144\n255\n".to_vec();
    for p in rgba.as_chunks::<4>().0 {
        out.extend_from_slice(&p[..3]);
    }
    let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{name}.ppm")), out);
}

fn camera(z: f32) -> Camera {
    Camera {
        position: Vec3::new(40.0, 150.0, z + 2.0),
        pitch: -0.12,
        far: 900.0,
        ..Camera::default()
    }
}

#[test]
fn weather_rain_and_snow_change_the_frame() {
    let (mut engine, region, z) = holtburg();
    let cam = camera(z);
    let _ = frames(&mut engine, &region, &cam, 0.5, 30);
    // Two dry frames a frame apart: what the clock alone changes.
    let dry = frames(&mut engine, &region, &cam, 0.5, 1);
    let dry_next = frames(&mut engine, &region, &cam, 0.5, 1);
    engine.renderer.weather.falls(Fall::Rain, false);
    let rain = frames(&mut engine, &region, &cam, 0.5, 300);
    assert!(engine.renderer.weather.wet > 0.4, "the ground soaks");
    // Weather Effects off: on the next frame nothing falls and nothing is wet.
    engine.renderer.weather.on = false;
    let rain_off = frames(&mut engine, &region, &cam, 0.5, 1);
    engine.renderer.weather.on = true;
    engine.renderer.weather.falls(Fall::Snow, false);
    let snow = frames(&mut engine, &region, &cam, 0.5, 900);
    engine.renderer.weather.on = false;
    let snow_off = frames(&mut engine, &region, &cam, 0.5, 1);
    keep("dry", &dry);
    keep("rain", &rain);
    keep("snow", &snow);
    let (floor, r, s) = (
        mean_difference(&dry, &dry_next),
        mean_difference(&rain, &rain_off),
        mean_difference(&snow, &snow_off),
    );
    eprintln!("a frame apart {floor:.2}, rain {r:.2}, snow {s:.2}");
    assert!(
        r > floor + 1.0,
        "rain and wet ground change the frame: {r} against {floor}"
    );
    assert!(
        s > floor + 0.2,
        "snow changes the frame: {s} against {floor}"
    );
}

#[test]
fn god_rays_and_cloud_detail_change_the_frame() {
    let (mut engine, region, z) = holtburg();
    let mut cam = camera(z);
    // Toward the morning sun.
    cam.yaw = -std::f32::consts::FRAC_PI_2;
    cam.pitch = 0.05;
    let plain = frames(&mut engine, &region, &cam, 0.27, 20);
    engine.renderer.air.level = 3;
    let rays = frames(&mut engine, &region, &cam, 0.27, 20);
    engine.renderer.air.level = 0;
    keep("air-off", &plain);
    keep("air-high", &rays);
    let d = mean_difference(&plain, &rays);
    eprintln!("god rays {d:.2}");
    assert!(d > 2.0, "the air's light changes the frame: {d}");

    cam.pitch = 0.7;
    engine.renderer.atmosphere.cloud_detail = 0;
    let flat = frames(&mut engine, &region, &cam, 0.5, 10);
    engine.renderer.atmosphere.cloud_detail = 3;
    let fine = frames(&mut engine, &region, &cam, 0.5, 10);
    keep("clouds-off", &flat);
    keep("clouds-high", &fine);
    let c = mean_difference(&flat, &fine);
    eprintln!("cloud detail {c:.2}");
    assert!(c > 0.3, "the cloud deck's detail changes the frame: {c}");
}

#[test]
fn each_cloud_detail_draws_its_own_volumetric_clouds() {
    let (mut engine, region, z) = holtburg();
    let mut cam = camera(z);
    cam.yaw = -std::f32::consts::FRAC_PI_2;
    cam.pitch = 0.7;
    let at = |engine: &mut Engine, detail: u32| {
        engine.renderer.atmosphere.cloud_detail = detail;
        frames(engine, &region, &cam, 0.5, 6)
    };
    // A fuller sky than the day's own (`@clouds cover 2`), so most of the view is cloud.
    engine.renderer.atmosphere.cloud_knobs.cover = 2.0;
    let shots: Vec<Vec<u8>> = (0..4).map(|d| at(&mut engine, d)).collect();
    // Medium with no cover: the sky alone, so the march is seen to draw.
    engine.renderer.atmosphere.cloud_knobs.cover = 0.0;
    let clear = at(&mut engine, 2);
    engine.renderer.atmosphere.cloud_knobs.cover = 1.0;
    for (d, shot) in shots.iter().enumerate() {
        keep(&format!("cloud-detail-{d}"), shot);
    }
    keep("cloud-detail-clear", &clear);
    let drawn = mean_difference(&shots[2], &clear);
    eprintln!("the volumetric clouds against a clear sky {drawn:.2}");
    assert!(drawn > 2.0, "the march draws clouds: {drawn}");
    for d in 1..4 {
        let off = mean_difference(&shots[0], &shots[d]);
        eprintln!("cloud detail {d} against the flat deck {off:.2}");
        assert!(off > 3.0, "detail {d} is not the flat deck: {off}");
    }
    for (a, b) in [(1, 2), (2, 3), (1, 3)] {
        let between = mean_difference(&shots[a], &shots[b]);
        eprintln!("cloud detail {a} against {b} {between:.2}");
        assert!(between > 0.15, "detail {a} and {b} differ: {between}");
    }
}

#[test]
fn light_reach_and_flicker_change_the_frame() {
    let (mut engine, region, z) = holtburg();
    let mut cam = camera(z);
    cam.pitch = -0.3;
    // By night the town's lamps light the ground.
    let far = frames(&mut engine, &region, &cam, 0.0, 12);
    engine.renderer.lamp_reach = 20.0;
    let near = frames(&mut engine, &region, &cam, 0.0, 12);
    engine.renderer.lamp_reach = pyreal_engine::lamps::LAMP_DISTANCE;
    keep("lamps-far", &far);
    keep("lamps-near", &near);
    let d = mean_difference(&far, &near);
    eprintln!("light reach {d:.3}");
    assert!(d > 0.05, "a shorter light reach changes the frame: {d}");
    // Flicker: two frames a little apart differ with it, not without.
    let a = frames(&mut engine, &region, &cam, 0.0, 1);
    let b = frames(&mut engine, &region, &cam, 0.0, 7);
    engine.renderer.flicker = false;
    let c = frames(&mut engine, &region, &cam, 0.0, 1);
    let e = frames(&mut engine, &region, &cam, 0.0, 7);
    let (on, off) = (mean_difference(&a, &b), mean_difference(&c, &e));
    eprintln!("flicker on {on:.4} off {off:.4}");
    assert!(
        off < on,
        "steady lamps change less frame to frame: {off} against {on}"
    );
}

/// A half float's value.
fn half(h: u16) -> f32 {
    let exp = i32::from((h >> 10) & 0x1F);
    let frac = f32::from(h & 0x3FF);
    let v = match exp {
        0 => frac * 2f32.powi(-24),
        31 => f32::INFINITY,
        e => (1.0 + frac / 1024.0) * 2f32.powi(e - 15),
    };
    if h & 0x8000 != 0 {
        -v
    } else {
        v
    }
}

/// HDR Output: to a float output (EDR's extended linear sRGB) with headroom, the sun and its
/// glow reach past SDR white; with none, nothing does, and below the knee the two agree.
#[test]
fn an_edr_output_reaches_past_white() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let gpu = pyreal_engine::Gpu::headless().expect("a GPU");
    let float = wgpu::TextureFormat::Rgba16Float;
    let mut engine =
        Engine::new(gpu, pyreal_engine::PoolSizes::default(), float).expect("the engine");
    engine.renderer.anti_aliasing = false;
    let mut loader = AssetLoader::default();
    load_area(
        &mut engine,
        &mut loader,
        &store,
        &region,
        Area {
            center: 0xA9B4,
            radius: 1,
        },
    );
    let h = pyreal_engine::heavens::Heavens::load(&mut engine, &mut loader, &store, &region);
    engine.heavens = Some(h);
    let z = ground_height(&store, &region, 0xA9B4, 40.0, 150.0).expect("the ground");
    let cam = Camera {
        position: Vec3::new(40.0, 150.0, z + 2.0),
        yaw: -std::f32::consts::FRAC_PI_2,
        pitch: 0.12,
        far: 900.0,
        ..Camera::default()
    };
    let light = SkyLight::at(&region, 0, 0.29);
    let size = (256, 144);
    let target = engine.gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("edr output"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let shot = |engine: &mut Engine, headroom: f32| -> Vec<f32> {
        engine.renderer.headroom = headroom;
        for _ in 0..16 {
            engine.renderer.clock += 1.0 / 60.0;
            engine.render(&cam, &light, &view, size).expect("a frame");
        }
        engine.gpu.wait();
        pyreal_engine::read_texture(&engine.gpu, &target, 8)
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| half(u16::from_le_bytes(*b)))
            .collect()
    };
    let sdr = shot(&mut engine, 1.0);
    let edr = shot(&mut engine, 4.0);
    let past = |v: &[f32]| {
        v.as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[..3].iter().any(|c| *c > 1.02))
            .count()
    };
    let brightest = edr.iter().copied().fold(0.0f32, f32::max);
    eprintln!(
        "past white: SDR {} EDR {} (brightest {brightest:.2})",
        past(&sdr),
        past(&edr)
    );
    assert_eq!(past(&sdr), 0, "an SDR output stays within white");
    assert!(past(&edr) > 20, "with headroom the highlights pass white");
    assert!(brightest <= 4.01, "and stop at the headroom: {brightest}");
}

#[test]
fn fog_density_thins_the_haze_and_off_leaves_none() {
    let (mut engine, region, z) = holtburg();
    let mut cam = camera(z);
    // High over the town, down across it to the loaded land's edge, at noon.
    cam.position.z += 60.0;
    cam.pitch = -0.2;
    let mut at = |d: f32| {
        engine.renderer.atmosphere.fog_density = d;
        frames(&mut engine, &region, &cam, 0.5, 6)
    };
    let heavy = at(3.0);
    let retail = at(1.0);
    let off = at(0.0);
    keep("fog-heavy", &heavy);
    keep("fog-off", &off);
    let (h, r) = (
        mean_difference(&off, &heavy),
        mean_difference(&off, &retail),
    );
    eprintln!("fog off against heavy {h:.2}, against retail {r:.2}");
    assert!(
        h > r && h > 1.0,
        "the haze thickens with the density: {h} {r}"
    );
    // Off: no haze begins anywhere; a thinner haze reaches as deep only farther off.
    let light = SkyLight::at(&region, 0, 0.5);
    let (start, _) = pyreal_engine::atmosphere::haze_range(&light, 5000.0, 0.0);
    assert!(start > 1.0e8, "no haze begins anywhere: {start}");
    let (s1, e1) = pyreal_engine::atmosphere::haze_range(&light, 5000.0, 1.0);
    let (s4, e4) = pyreal_engine::atmosphere::haze_range(&light, 5000.0, 0.4);
    assert!(
        (s1 - s4).abs() < 1e-3 && e4 > e1 * 2.0,
        "{s1} {e1} {s4} {e4}"
    );
}

/// A roof keeps the rain off. While it rains the shelter map holds the roof of Holtburg's tower
/// high over its hall and nothing over the open grass west of it; and under an open shed's roof
/// (by the map) the ground stays dry, drying it changing nothing, while the open ground round it
/// soaks.
#[test]
fn a_roof_keeps_the_rain_off() {
    let (mut engine, region, _) = holtburg();
    // A steady exposure and no bloom: the drying ground does not move the rest of the picture.
    engine.renderer.exposure = pyreal_engine::renderer::Exposure::Manual(0.5);
    engine.renderer.bloom = 0.0;
    let store = pyreal_dat::testing::open_store_or_fail();
    let ground = |x: f32, y: f32| ground_height(&store, &region, 0xA9B4, x, y).expect("ground");
    let hall = glam::Vec2::new(160.0, 40.0);
    let open = glam::Vec2::new(130.0, 50.0);
    // Before an open shed, looking at the ground under its roof and the grass in front.
    let camera = Camera {
        position: Vec3::new(86.0, 112.0, ground(86.0, 112.0) + 2.5),
        yaw: 0.0,
        pitch: -0.3,
        far: 900.0,
        ..Camera::default()
    };
    engine.renderer.weather.falls(Fall::Rain, false);
    let _ = frames(&mut engine, &region, &camera, 0.5, 600);
    let at = engine
        .renderer
        .shelter
        .expect("the shelter map is drawn while it rains");
    let heights = engine
        .renderer
        .weather
        .shelter_heights(&engine.gpu, &at, &[hall, open]);
    let roof = heights[0].expect("the tower's roof over its hall") - ground(hall.x, hall.y);
    assert!(roof > 5.0, "the roof stands {roof} m over the hall");
    assert!(
        heights[1].is_none_or(|h| h < ground(open.x, open.y)),
        "nothing stands over the open grass: {:?}",
        heights[1]
    );
    // The rain stopped, the ground still wet; then dried at once.
    engine.renderer.weather.falls(Fall::Nothing, false);
    engine.renderer.weather.precip = 0.0;
    engine.renderer.weather.rain = 0.0;
    let wet = frames(&mut engine, &region, &camera, 0.5, 1);
    let depth = super::words(&engine, engine.renderer.depth_texture());
    engine.renderer.weather.on = false;
    let dry = frames(&mut engine, &region, &camera, 0.5, 1);
    let dry_next = frames(&mut engine, &region, &camera, 0.5, 1);
    keep("roof-wet", &wet);
    keep("roof-dry", &dry);
    // Each pixel's point, and whether a roof stands well over it or nothing does.
    let (w, h) = (256usize, 144usize);
    #[allow(clippy::cast_precision_loss)]
    let inverse = camera.view_projection(w as f32 / h as f32).inverse();
    let points: Vec<Option<Vec3>> = (0..w * h)
        .map(|i| {
            let d = f32::from_bits(depth[i]);
            #[allow(clippy::cast_precision_loss)]
            let ndc = glam::Vec2::new(
                ((i % w) as f32 + 0.5) / w as f32 * 2.0 - 1.0,
                1.0 - ((i / w) as f32 + 0.5) / h as f32 * 2.0,
            );
            (d > 0.0).then(|| inverse.project_point3(ndc.extend(d)))
        })
        .collect();
    // The ground's pixels: under a roof a metre in from its edge every way, or open a metre round.
    let round = [
        glam::Vec2::ZERO,
        glam::Vec2::X,
        -glam::Vec2::X,
        glam::Vec2::Y,
        -glam::Vec2::Y,
    ];
    let over = engine.renderer.weather.shelter_heights(
        &engine.gpu,
        &at,
        &points
            .iter()
            .flat_map(|p| round.map(|o| p.unwrap_or_default().truncate() + o))
            .collect::<Vec<_>>(),
    );
    // How much each class changes as it dries, and a frame later dry (the bounce light settling).
    let (mut roofed, mut bare, mut still) = ((0.0, 0u32), (0.0, 0u32), 0.0);
    let change = |a: &[u8], b: &[u8], i: usize| -> f64 {
        (0..3)
            .map(|c| f64::from(a[i * 4 + c].abs_diff(b[i * 4 + c])))
            .sum()
    };
    for (i, p) in points.iter().enumerate() {
        let Some(p) = p else { continue };
        let Some(g) = ground_height(&store, &region, 0xA9B4, p.x, p.y) else {
            continue;
        };
        if (p.z - g).abs() > 0.3 || p.distance(camera.position) > 40.0 {
            continue;
        }
        let tops = &over[i * 5..i * 5 + 5];
        if tops.iter().all(|t| t.is_some_and(|t| t > p.z + 1.5)) {
            roofed = (roofed.0 + change(&wet, &dry, i), roofed.1 + 1);
            still += change(&dry, &dry_next, i);
        } else if tops.iter().all(|t| t.is_none_or(|t| t < p.z + 0.2)) {
            bare = (bare.0 + change(&wet, &dry, i), bare.1 + 1);
        }
    }
    let mean = |(sum, n): (f64, u32)| sum / f64::from(n.max(1));
    let still = mean((still, roofed.1));
    eprintln!(
        "drying: {} roofed pixels change {:.2} (dry, a frame on, {still:.2}), {} open {:.2}",
        roofed.1,
        mean(roofed),
        bare.1,
        mean(bare)
    );
    assert!(
        roofed.1 > 100 && bare.1 > 100,
        "{} roofed and {} open pixels",
        roofed.1,
        bare.1
    );
    assert!(
        mean(bare) > 10.0 && mean(roofed) < still + 0.05 * mean(bare),
        "the roofed ground stays dry ({} against {still}) as the open soaks ({})",
        mean(roofed),
        mean(bare)
    );
}

/// Fresh snow past the loaded land lies only on the far field's snowy ground: from a snowy hilltop
/// over green land (the developer's report, 0xB5BF), the far ground does not take the cover of the
/// loaded land's snowy edge, which once ran on to the horizon in bands.
#[test]
fn fresh_snow_past_the_loaded_land_keeps_to_snowy_ground() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = super::engine();
    let mut loader = AssetLoader::default();
    let area = Area {
        center: 0xB5BF,
        radius: 4,
    };
    load_area_smoothed(&mut engine, &mut loader, &store, &region, area, 2);
    load_far(
        &mut engine,
        &mut loader,
        &store,
        &region,
        area,
        12,
        1344.0,
        true,
    );
    let camera = Camera {
        position: Vec3::new(190.87, 77.34, 422.58),
        yaw: -3.038,
        pitch: -0.195,
        fov_y: 58.34f32.to_radians(),
        far: 12.5 * 192.0,
        ..Camera::default()
    };
    // Fog Density off, as the developer had it.
    engine.renderer.atmosphere.fog_density = 0.0;
    let w = &mut engine.renderer.weather;
    w.snowy_types = pyreal_engine::weather::snowy_types(&region);
    w.falls(Fall::Nothing, false);
    let frame = |engine: &mut Engine, cover: f32| {
        engine.renderer.weather.cover = cover;
        frames(engine, &region, &camera, 0.518, 2)
    };
    let bare = frame(&mut engine, 0.0);
    let covered = frame(&mut engine, 1.0);
    let depth = super::words(&engine, engine.renderer.depth_texture());
    keep("far-snow-bare", &bare);
    keep("far-snow-covered", &covered);
    // The far ground's pixels: two cells or more past the loaded square, nine blocks across.
    let (w, h) = (256usize, 144usize);
    #[allow(clippy::cast_precision_loss)]
    let inverse = camera.view_projection(w as f32 / h as f32).inverse();
    let (mut far, mut whitened) = (0u32, 0u32);
    for i in 0..w * h {
        let d = f32::from_bits(depth[i]);
        if d <= 0.0 {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let ndc = glam::Vec2::new(
            ((i % w) as f32 + 0.5) / w as f32 * 2.0 - 1.0,
            1.0 - ((i / w) as f32 + 0.5) / h as f32 * 2.0,
        );
        let p = inverse.project_point3(ndc.extend(d));
        if (p.x - 96.0).abs().max((p.y - 96.0).abs()) < 864.0 + 48.0 {
            continue;
        }
        far += 1;
        let luma = |f: &[u8]| {
            let c = &f[i * 4..];
            0.2126 * f32::from(c[0]) + 0.7152 * f32::from(c[1]) + 0.0722 * f32::from(c[2])
        };
        if luma(&covered) > luma(&bare) + 25.0 {
            whitened += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let share = f64::from(whitened) / f64::from(far.max(1));
    eprintln!("{far} far pixels, {share:.3} of them whitened");
    assert!(far > 2000, "the far ground fills the view: {far} pixels");
    assert!(
        share < 0.05,
        "fresh snow whitened {share:.3} of the far ground"
    );
}
