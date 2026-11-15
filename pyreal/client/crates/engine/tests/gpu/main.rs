//! The engine draws a place on the hardware GPU from the retail dats ($PYREAL_TEST_DAT_DIR). A
//! missing input fails.

mod bodies;
mod cards;
mod cover;
mod frame;
mod light;
mod marks;
mod portal;
mod preview;
mod settings;
mod stream;
mod surfaces;
mod water;

use glam::Vec3;
use pyreal_engine::world::{ground_height, load_area, Area, AssetLoader};
use pyreal_engine::{Camera, Engine, Gpu, PoolSizes, SkyLight};

/// Holtburg at noon, looking north over the town: the ground and the houses cover most of the
/// frame below the horizon, the depth is reverse Z (near one, sky zero), the cull pass keeps some
/// of the instances and not all, and every pass is timed.
#[test]
fn holtburg_draws_with_ground_objects_and_sane_depth() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let gpu = Gpu::headless().expect("a GPU with the engine's features");
    let timed = gpu.has(wgpu::Features::TIMESTAMP_QUERY);
    let mut engine = Engine::new(
        gpu,
        PoolSizes::default(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    )
    .expect("the engine");
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
    assert_eq!(loader.stats.blocks, 9);
    assert!(loader.stats.buildings > 0 && loader.stats.scenery > 0);
    assert_eq!(loader.stats.texture_failures, 0);

    let z = ground_height(&store, &region, 0xA9B4, 96.0, 60.0).expect("the ground") + 2.0;
    let camera = Camera {
        position: Vec3::new(96.0, 60.0, z),
        pitch: -0.15,
        far: 300.0,
        ..Camera::default()
    };
    let light = SkyLight::at(&region, 0, 0.5);
    let (w, h) = (320, 180);
    let rgba = engine
        .render_to_rgba(&camera, &light, (w, h))
        .expect("a frame");
    assert_eq!(rgba.len(), (w * h * 4) as usize);
    let timings = engine.renderer.collect_timings(&engine.gpu, true).to_vec();

    let depth_tex = engine.renderer.depth_texture().expect("the depth target");
    let depth: Vec<f32> = pyreal_engine::read_texture(&engine.gpu, depth_tex, 4)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    let drawn = depth.iter().filter(|&&d| d > 0.0).count();
    let lower = depth[depth.len() / 2..]
        .iter()
        .filter(|&&d| d > 0.0)
        .count();
    assert!(
        depth.iter().all(|&d| (0.0..=1.0).contains(&d)),
        "depth outside [0, 1]"
    );
    assert!(
        drawn > depth.len() / 3,
        "only {drawn} of {} pixels drawn",
        depth.len()
    );
    assert!(
        lower > depth.len() / 2 * 9 / 10,
        "the ground does not fill the lower half"
    );
    // The sky is not drawn into depth, and the top row is sky.
    assert!(
        depth[..w as usize].iter().all(|&d| d == 0.0),
        "the top row is not sky"
    );

    // The picture is not one colour.
    let distinct: std::collections::BTreeSet<[u8; 4]> =
        rgba.as_chunks::<4>().0.iter().copied().collect();
    assert!(distinct.len() > 500, "{} colours", distinct.len());

    let cull = engine.renderer.read_cull_stats(&engine.gpu);
    let counts = engine.scene.counts();
    assert!(cull.visible_instances > 10, "{cull:?}");
    assert!(
        (cull.visible_instances as usize) < counts.instances,
        "{cull:?} of {counts:?}"
    );
    assert!(u64::from(cull.visible_triangles) < counts.triangles);
    if timed {
        assert_eq!(
            timings.len(),
            pyreal_engine::renderer::graph_decl().passes.len()
        );
        // A pass with nothing to do this frame (no lamp near enough to trace) may read zero.
        assert!(
            timings
                .iter()
                .filter(
                    |t| ["prepass", "gbuffer", "shadows", "lighting", "tonemap"].contains(&t.name)
                )
                .all(|t| t.ms > 0.0),
            "{timings:?}"
        );
    }
}

/// An engine drawing unjittered frames, so two can be compared pixel for pixel.
fn engine() -> Engine {
    let gpu = Gpu::headless().expect("a GPU with the engine's features");
    let mut e = Engine::new(
        gpu,
        PoolSizes::default(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    )
    .expect("the engine");
    e.renderer.anti_aliasing = false;
    e
}

fn words(engine: &Engine, texture: Option<&wgpu::Texture>) -> Vec<u32> {
    pyreal_engine::read_texture(&engine.gpu, texture.expect("the target"), 4)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u32::from_le_bytes(*b))
        .collect()
}

/// In the Holtburg room the ground passes highest through, looking down from inside it with the
/// outdoors drawn whole (the eye named an outdoor cell, as a report standing in a doorway does):
/// the ground covers much of the frame with the cut switched off, and with it on the ground inside
/// the room's volume is gone. Standing in the room itself, its floor is what is under the eye.
#[test]
fn the_ground_is_cut_away_inside_a_holtburg_room() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = engine();
    let mut loader = AssetLoader::default();
    let area = Area {
        center: 0xA9B4,
        radius: 0,
    };
    load_area(&mut engine, &mut loader, &store, &region, area);
    assert!(
        loader.stats.cells > 100 && loader.stats.cut_cells > 0,
        "{:?}",
        loader.stats
    );
    let terrain = loader
        .terrain_material(&mut engine)
        .expect("the ground's material");
    let (_, id, at) = engine
        .cells
        .cells()
        .iter()
        .filter_map(|c| {
            let v = c.volume.as_ref()?;
            let z = ground_height(&store, &region, 0xA9B4, v.center.x, v.center.y)?;
            (z > v.min.z + 0.05 && z < v.max.z - 1.0).then(|| {
                (
                    z - v.min.z,
                    c.id,
                    Vec3::new(v.center.x, v.center.y, z + 0.7),
                )
            })
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .expect("a room the ground passes through");
    let camera = Camera {
        position: at,
        pitch: -1.45,
        far: 300.0,
        cell: Some(0xA9B4_0001),
        ..Camera::default()
    };
    let light = SkyLight::at(&region, 0, 0.5);
    let (w, h) = (64, 64);
    let centre = |ids: &[u32]| ids[(h / 2 * w + w / 2) as usize];
    let ground = |ids: &[u32]| ids.iter().filter(|&&m| m == terrain).count();
    engine.renderer.cut_ground = false;
    engine
        .render_to_rgba(&camera, &light, (w, h))
        .expect("a frame");
    assert!(engine.visibility.outside() && engine.cells.slot_of(id).is_some());
    let whole = words(&engine, engine.renderer.ids_texture());
    assert!(
        ground(&whole) * 4 > whole.len(),
        "uncut, the ground covers the floor of {id:#X}: {} of {}",
        ground(&whole),
        whole.len()
    );
    engine.renderer.cut_ground = true;
    engine
        .render_to_rgba(&camera, &light, (w, h))
        .expect("a frame");
    let cut = words(&engine, engine.renderer.ids_texture());
    assert!(
        ground(&cut) * 10 < ground(&whole),
        "cut, the floor of {id:#X} shows: {} ground pixels of {} uncut",
        ground(&cut),
        ground(&whole)
    );
    let inside = Camera {
        cell: Some(id),
        ..camera
    };
    engine
        .render_to_rgba(&inside, &light, (w, h))
        .expect("a frame");
    assert_eq!(engine.visibility.eye_cell, engine.cells.slot_of(id));
    let ids = words(&engine, engine.renderer.ids_texture());
    let depth = words(&engine, engine.renderer.depth_texture());
    assert!(
        centre(&ids) != terrain && f32::from_bits(centre(&depth)) > 0.0,
        "the floor of {id:#X} is under the eye"
    );
}

/// The training dungeon at 0x019E, from the room a report stood in: walls cover the whole frame,
/// the sky is never drawn, and the portal walk keeps most of the dungeon's rooms out of the draw.
#[test]
fn a_dungeon_draws_with_coverage() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = engine();
    let mut loader = AssetLoader::default();
    load_area(
        &mut engine,
        &mut loader,
        &store,
        &region,
        Area {
            center: 0x019E,
            radius: 0,
        },
    );
    assert!(loader.stats.cells > 50 && loader.stats.cell_statics > 0);
    assert_eq!(loader.stats.cell_failures, 0);
    // dungeon-bricks-1 in the report library.
    let camera = Camera {
        position: Vec3::new(10.78, -44.2, 1.93),
        yaw: -6.090,
        pitch: -0.11,
        far: 300.0,
        cell: Some(0x019E_0114),
        ..Camera::default()
    };
    let light = SkyLight::at(&region, 0, 0.5);
    let (w, h) = (320, 180);
    let rgba = engine
        .render_to_rgba(&camera, &light, (w, h))
        .expect("a frame");
    assert!(!engine.visibility.outside());
    let depth = words(&engine, engine.renderer.depth_texture());
    let drawn = depth.iter().filter(|&&d| f32::from_bits(d) > 0.0).count();
    assert!(
        drawn * 100 >= depth.len() * 99,
        "only {drawn} of {} drawn",
        depth.len()
    );
    let distinct: std::collections::BTreeSet<[u8; 4]> =
        rgba.as_chunks::<4>().0.iter().copied().collect();
    assert!(distinct.len() > 500, "{} colours", distinct.len());
    let cull = engine.renderer.read_cull_stats(&engine.gpu);
    let counts = engine.scene.counts();
    assert!(cull.visible_instances > 5, "{cull:?}");
    assert!(
        (cull.visible_instances as usize) * 4 < counts.instances,
        "{cull:?} of {counts:?}"
    );
}

/// Holtburg's door behind the hill (the report library's `door-behind-hill`): the ground's depth
/// pyramid hides most of the town's instances behind the crest, and the frame is the frame
/// drawn without it, pixel for pixel but for the odd coplanar tie.
#[test]
fn the_ground_hides_what_is_behind_a_hill_and_nothing_else() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = engine();
    let mut loader = AssetLoader::default();
    load_area(
        &mut engine,
        &mut loader,
        &store,
        &region,
        Area {
            center: 0xA9B4,
            radius: 2,
        },
    );
    let camera = Camera {
        position: Vec3::new(79.319_176, 177.736_65, 63.949_776),
        yaw: -2.383_948,
        pitch: -0.005,
        far: 500.0,
        cell: Some(0xA9B4_0020),
        ..Camera::default()
    };
    let light = SkyLight::at(&region, 0, 0.5);
    let size = (320, 180);
    let hidden = engine
        .render_to_rgba(&camera, &light, size)
        .expect("a frame");
    let with = engine.renderer.read_cull_stats(&engine.gpu);
    engine.renderer.hiz = false;
    let plain = engine
        .render_to_rgba(&camera, &light, size)
        .expect("a frame");
    let without = engine.renderer.read_cull_stats(&engine.gpu);
    assert_eq!(without.occluded, 0);
    assert!(
        with.occluded > 500 && with.visible_instances * 2 < without.visible_instances,
        "{with:?} against {without:?}"
    );
    let differ = hidden
        .as_chunks::<4>()
        .0
        .iter()
        .zip(plain.as_chunks::<4>().0)
        .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 16))
        .count();
    // The two frames' probes and shadows' noise differ a little; the tone curve's toe steepens it.
    assert!(differ * 1000 < hidden.len() / 4, "{differ} pixels differ");
}

/// The same view of Holtburg with the dats' degrade levels and without: far parts draw coarser
/// meshes, or none past their last level, and the visible triangles fall.
#[test]
fn degrade_levels_thin_the_distance() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = engine();
    let mut loader = AssetLoader::default();
    load_area(
        &mut engine,
        &mut loader,
        &store,
        &region,
        Area {
            center: 0xA9B4,
            radius: 2,
        },
    );
    // doors-through-foliage-1 in the report library: over the town to the lake.
    let camera = Camera {
        position: Vec3::new(100.481_93, 45.935_85, 96.153_54),
        yaw: -5.819_972,
        pitch: -0.415,
        far: 500.0,
        ..Camera::default()
    };
    let light = SkyLight::at(&region, 0, 0.5);
    engine.renderer.lod_bias = 0.0;
    engine
        .render_to_rgba(&camera, &light, (320, 180))
        .expect("a frame");
    let near = engine.renderer.read_cull_stats(&engine.gpu);
    engine.renderer.lod_bias = 1.0;
    engine
        .render_to_rgba(&camera, &light, (320, 180))
        .expect("a frame");
    let degraded = engine.renderer.read_cull_stats(&engine.gpu);
    assert_eq!(near.degraded, 0);
    assert!(degraded.degraded > 100, "{degraded:?}");
    assert!(
        degraded.visible_triangles * 2 < near.visible_triangles,
        "{degraded:?} against {near:?}"
    );
}

/// The high-performance adapter is fit, so the engine takes it as it is asked for and never
/// looks further.
#[test]
fn the_high_performance_adapter_is_fit_and_taken() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let asked = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        apply_limit_buckets: false,
    }))
    .expect("an adapter");
    assert_eq!(
        pyreal_engine::gpu::unfit(asked.features(), &asked.limits()),
        None,
        "binding array room {}",
        asked.limits().max_binding_array_elements_per_shader_stage
    );
    let gpu = Gpu::headless().expect("a GPU with the engine's features");
    assert_eq!(gpu.adapter.get_info(), asked.get_info());
}
