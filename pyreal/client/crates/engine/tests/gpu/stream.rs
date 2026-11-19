//! The land streamed block by block: a walk across several blocks is never loaded again from
//! nothing, the placed square follows the eye, render space moves under it without a reload, and
//! with eviction on the pools stay bounded however far it goes. What it draws at the end of the
//! walk is what a fresh load of the same place draws.

use std::sync::Arc;

use glam::{Vec2, Vec3};
use pyreal_engine::evict::{Budget, PoolUse};
use pyreal_engine::stream::Stream;
use pyreal_engine::world::{ground_height, AssetLoader};
use pyreal_engine::{Camera, Engine, SkyLight};

const START: (i32, i32) = (0xA9, 0xB4);

fn setup() -> (
    Arc<pyreal_dat::RetailDatStore>,
    Arc<pyreal_engine::world::Region>,
) {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let region = Arc::new(pyreal_world_data::landblock::load_region(&store).expect("the region"));
    (store, region)
}

/// Updates until the square wanted is placed (the planners answer on their own threads).
fn settle(
    stream: &mut Stream,
    engine: &mut Engine,
    loader: &mut AssetLoader,
    viewer: (i32, i32),
    eye: Vec2,
) {
    for _ in 0..20_000 {
        stream.update(engine, loader, viewer, eye);
        if stream.settled() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_micros(200));
    }
    panic!("the square round {viewer:?} never settled");
}

/// The square of `radius` round `c`.
fn square(c: (i32, i32), radius: i32) -> Vec<(i32, i32)> {
    let mut v: Vec<(i32, i32)> = (c.0 - radius..=c.0 + radius)
        .flat_map(|x| (c.1 - radius..=c.1 + radius).map(move |y| (x, y)))
        .collect();
    v.sort_unstable();
    v
}

/// A frame from `eye` (world metres from `block`'s corner) in `stream`'s render space, after a
/// few frames to settle the exposure.
fn frame(
    engine: &mut Engine,
    stream: &Stream,
    region: &pyreal_engine::world::Region,
    store: &pyreal_dat::RetailDatStore,
    block: (i32, i32),
    eye: Vec2,
) -> Vec<u8> {
    let id = u16::try_from((block.0 << 8) | block.1).expect("a block");
    let z = ground_height(store, region, id, eye.x, eye.y).expect("the ground") + 6.0;
    let camera = Camera {
        position: stream.block_origin(block) + Vec3::new(eye.x, eye.y, z),
        pitch: -0.2,
        yaw: -0.8,
        far: 900.0,
        ..Camera::default()
    };
    let light = SkyLight::at(region, 0, 0.5);
    let mut rgba = Vec::new();
    for _ in 0..12 {
        rgba = engine
            .render_to_rgba(&camera, &light, (256, 144))
            .expect("a frame");
    }
    rgba
}

/// Writes `rgba` (256 by 144) as a binary PPM under `$PYREAL_TEST_OUT`, when it is set.
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

fn mean_difference(a: &[u8], b: &[u8]) -> f64 {
    let sum: u64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let mean = sum as f64 / a.len().max(1) as f64;
    mean
}

/// Walking east twelve blocks: one load at the start and none after; each time the eye goes a
/// cell past the centre block the square follows it, placed in the background and switched to
/// at once; render space's origin moves on past [`pyreal_engine::stream::REBASE`] blocks with
/// nothing loaded again; and the frame at the end is a fresh load's.
#[test]
fn walking_streams_the_land_with_no_reload() {
    let (store, region) = setup();
    let mut engine = super::engine();
    let mut loader = AssetLoader::default();
    let mut stream = Stream::new(Arc::clone(&store), Arc::clone(&region), 1, 3, &loader);
    let eye = Vec2::new(96.0, 96.0);
    settle(&mut stream, &mut engine, &mut loader, START, eye);
    assert_eq!(stream.stats.full_loads, 1);
    let mut viewer = START;
    for _ in 0..12 {
        // A cell into the next block east: inside the margin, nothing moves yet.
        viewer.0 += 1;
        settle(
            &mut stream,
            &mut engine,
            &mut loader,
            viewer,
            Vec2::new(10.0, 96.0),
        );
        assert_eq!(
            stream.centre(),
            Some((viewer.0 - 1, viewer.1)),
            "within the margin"
        );
        // Past it, the square follows.
        settle(
            &mut stream,
            &mut engine,
            &mut loader,
            viewer,
            Vec2::new(40.0, 96.0),
        );
        assert_eq!(stream.centre(), Some(viewer));
        let mut placed: Vec<(i32, i32)> = stream.blocks().collect();
        placed.sort_unstable();
        assert_eq!(
            placed,
            square(viewer, 1),
            "the placed square follows the eye"
        );
    }
    assert_eq!(stream.stats.full_loads, 1, "no reload while walking");
    assert_eq!(stream.stats.switches, 12);
    assert!(stream.stats.rebases >= 1, "render space moved on");
    assert!(
        pyreal_engine::stream::REBASE >= (viewer.0 - stream.origin().0).abs(),
        "the origin stays near the eye"
    );
    let walked = frame(
        &mut engine,
        &stream,
        &region,
        &store,
        viewer,
        Vec2::new(40.0, 96.0),
    );

    let mut fresh_engine = super::engine();
    let mut fresh_loader = AssetLoader::default();
    let mut fresh = Stream::new(Arc::clone(&store), Arc::clone(&region), 1, 3, &fresh_loader);
    settle(
        &mut fresh,
        &mut fresh_engine,
        &mut fresh_loader,
        viewer,
        Vec2::new(40.0, 96.0),
    );
    let loaded = frame(
        &mut fresh_engine,
        &fresh,
        &region,
        &store,
        viewer,
        Vec2::new(40.0, 96.0),
    );
    keep("walked", &walked);
    keep("loaded", &loaded);
    let d = mean_difference(&walked, &loaded);
    assert!(
        d < 1.0,
        "the walked frame differs from a fresh load's by {d:.2} a channel"
    );
}

/// An eye flying faster than the square is placed: the square being placed is kept while the eye
/// stays two blocks inside it, rather than started over at every block, and given up for one round
/// the eye when it goes further. Nothing is loaded again from nothing.
#[test]
fn a_fast_eye_keeps_the_square_being_placed_until_it_nears_its_edge() {
    let (store, region) = setup();
    let mut engine = super::engine();
    let mut loader = AssetLoader::default();
    let mut stream = Stream::new(Arc::clone(&store), Arc::clone(&region), 4, 4, &loader);
    settle(
        &mut stream,
        &mut engine,
        &mut loader,
        START,
        Vec2::new(96.0, 96.0),
    );
    let north = |n: i32| (START.0, START.1 + n);
    let past = Vec2::new(96.0, 40.0);
    stream.update(&mut engine, &mut loader, north(1), past);
    assert_eq!(stream.target(), Some(north(1)));
    for n in 2..=3 {
        stream.update(&mut engine, &mut loader, north(n), past);
        assert_eq!(stream.centre(), Some(START), "not switched yet");
        assert_eq!(stream.target(), Some(north(1)), "kept {n} blocks on");
    }
    stream.update(&mut engine, &mut loader, north(4), past);
    assert_eq!(stream.target(), Some(north(4)), "given up near its edge");
    settle(&mut stream, &mut engine, &mut loader, north(4), past);
    assert_eq!(stream.centre(), Some(north(4)));
    assert_eq!(stream.stats.full_loads, 1, "no reload");
}

/// A long reach: the square switches beside the far terrain already drawn, its hole moved to the
/// new square, rather than waiting on a far field each time, and a new far field round the eye
/// takes its place once the square has gone far enough. Nothing is loaded again from nothing.
#[test]
fn a_long_reach_keeps_its_far_terrain_beside_the_square_until_a_new_one_is_made() {
    let (store, region) = setup();
    let mut engine = super::engine();
    let mut loader = AssetLoader::default();
    let mut stream = Stream::new(Arc::clone(&store), Arc::clone(&region), 1, 24, &loader);
    settle(
        &mut stream,
        &mut engine,
        &mut loader,
        START,
        Vec2::new(96.0, 96.0),
    );
    assert_eq!(stream.far_centre(), Some(START));
    let east = |n: i32| (START.0 + n, START.1);
    let past = Vec2::new(40.0, 96.0);
    settle(&mut stream, &mut engine, &mut loader, east(1), past);
    assert_eq!(stream.centre(), Some(east(1)));
    assert_eq!(
        stream.far_centre(),
        Some(START),
        "the drawn far terrain is kept"
    );
    let field = engine.far.as_ref().expect("far terrain").field();
    let hole = (
        stream
            .block_origin((east(1).0 - 1, east(1).1 - 1))
            .truncate(),
        stream
            .block_origin((east(1).0 + 2, east(1).1 + 2))
            .truncate(),
    );
    assert_eq!(
        (field.near_min, field.near_max),
        hole,
        "its hole is the new square"
    );
    for n in 2..=8 {
        settle(&mut stream, &mut engine, &mut loader, east(n), past);
    }
    let started = std::time::Instant::now();
    while stream.far_centre() == Some(START) && started.elapsed().as_secs() < 30 {
        stream.update(&mut engine, &mut loader, east(8), past);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let made = stream.far_centre().expect("far terrain");
    assert!(
        (made.0 - east(8).0).abs() <= 6,
        "a far field round the way the eye went: {made:?}"
    );
    assert_eq!(stream.stats.full_loads, 1, "no reload");
}

/// A long walk with the pools' budget at nothing: what the blocks left behind drew is freed at
/// every switch, so the pools hold about what one square needs however far the walk goes, freed
/// slots are reused, and the frame at the end is still a fresh load's.
#[test]
fn eviction_bounds_the_pools_over_a_journey() {
    let (store, region) = setup();
    let mut engine = super::engine();
    let mut loader = AssetLoader::default();
    let mut stream = Stream::new(Arc::clone(&store), Arc::clone(&region), 1, 1, &loader);
    stream.budget = Budget {
        vertices: 0.0,
        indices: 0.0,
        meshes: 0.0,
        materials: 0.0,
        textures: 0.0,
        texture_bytes: 0,
    };
    let eye = Vec2::new(150.0, 96.0);
    settle(&mut stream, &mut engine, &mut loader, START, eye);
    let mut viewer = START;
    let mut uses: Vec<PoolUse> = vec![PoolUse::of(&engine)];
    for _ in 0..16 {
        viewer.0 += 1;
        settle(&mut stream, &mut engine, &mut loader, viewer, eye);
        uses.push(PoolUse::of(&engine));
    }
    assert_eq!(stream.stats.full_loads, 1);
    assert!(stream.stats.evictions >= 16, "{:?}", stream.stats);
    eprintln!(
        "vertices at each switch: {:?}",
        uses.iter().map(|u| u.vertices).collect::<Vec<_>>()
    );
    // Past Holtburg's town (two blocks), what the walk holds.
    let most = uses[3..].iter().map(|u| u.vertices).max().unwrap_or(0);
    let most_meshes = uses[3..].iter().map(|u| u.meshes).max().unwrap_or(0);
    let most_textures = uses[3..].iter().map(|u| u.textures).max().unwrap_or(0);

    // The same walk with nothing freed, for scale.
    let mut keep_engine = super::engine();
    let mut keep_loader = AssetLoader::default();
    let mut keep = Stream::new(Arc::clone(&store), Arc::clone(&region), 1, 1, &keep_loader);
    keep.budget = Budget {
        vertices: 1.0,
        indices: 1.0,
        meshes: 1.0,
        materials: 1.0,
        textures: 1.0,
        texture_bytes: u64::MAX,
    };
    let mut v = START;
    settle(&mut keep, &mut keep_engine, &mut keep_loader, v, eye);
    for _ in 0..16 {
        v.0 += 1;
        settle(&mut keep, &mut keep_engine, &mut keep_loader, v, eye);
    }
    let kept = PoolUse::of(&keep_engine);
    assert_eq!(keep.stats.evictions, 0);
    eprintln!(
        "pool use over the walk, evicting: vertices at most {most}, meshes {most_meshes}, textures {most_textures}; keeping: {kept:?}; evicting at the end: {:?}",
        uses.last()
    );
    assert!(
        most * 2 < kept.vertices,
        "evicting holds the vertices to {most} against {} kept",
        kept.vertices
    );
    assert!(most_meshes * 2 < kept.meshes);
    assert!(most_textures * 2 < kept.textures);
    let end = uses.last().copied().unwrap_or_default();
    assert!(
        end.largest_vertex_run * 2 > end.vertex_capacity - end.vertices,
        "the free vertices stay in long runs: {end:?}"
    );

    let walked = frame(&mut engine, &stream, &region, &store, viewer, eye);
    let mut fresh_engine = super::engine();
    let mut fresh_loader = AssetLoader::default();
    let mut fresh = Stream::new(Arc::clone(&store), Arc::clone(&region), 1, 1, &fresh_loader);
    settle(
        &mut fresh,
        &mut fresh_engine,
        &mut fresh_loader,
        viewer,
        eye,
    );
    let loaded = frame(&mut fresh_engine, &fresh, &region, &store, viewer, eye);
    // As much as a fresh load of the place holds (the walk's cache of the last switch aside).
    let fresh_use = PoolUse::of(&fresh_engine);
    assert!(
        end.vertices <= fresh_use.vertices * 3 / 2 && end.meshes <= fresh_use.meshes * 3 / 2,
        "the walk holds {end:?} where a fresh load holds {fresh_use:?}"
    );
    let d = mean_difference(&walked, &loaded);
    // Two runs of the same frame differ by 0.7 to 1.0 a channel; past that is real.
    assert!(
        d < 1.5,
        "after eviction the frame differs from a fresh load's by {d:.2}"
    );
}

/// Updates until the far trees fill the rings round the drawn square: as many blocks as the rings
/// hold, none outside them.
fn fill_trees(
    stream: &mut Stream,
    engine: &mut Engine,
    loader: &mut AssetLoader,
    viewer: (i32, i32),
    eye: Vec2,
    rings: i32,
) {
    let outer = stream.radius + rings;
    let want = square(viewer, outer).len() - square(viewer, stream.radius).len();
    for _ in 0..200_000 {
        stream.update(engine, loader, viewer, eye);
        let blocks: Vec<(i32, i32)> = stream.tree_blocks().collect();
        let within = blocks.iter().all(|b| {
            let d = (b.0 - viewer.0).abs().max((b.1 - viewer.1).abs());
            d > stream.radius && d <= outer
        });
        if within && blocks.len() == want && stream.stats.pending == 0 {
            return;
        }
        std::thread::sleep(std::time::Duration::from_micros(100));
    }
    panic!("the far trees round {viewer:?} never filled their rings");
}

/// At the Horizon draw distance (127 blocks, 24 km) the far terrain holds the dats' heights to
/// its edge and the far trees fill 24 rings past the square, thinning past the eighth; walking a
/// block on, both follow the eye.
#[test]
fn the_far_land_and_trees_stream_at_the_horizon_reach() {
    let (store, region) = setup();
    // The commonest scenery object round the start stands for a pack tree.
    let mut planner = pyreal_engine::world::Planner::new(&region);
    let mut counts: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    for b in square(START, 2) {
        for (id, _) in planner.scenery(&store, &region, b) {
            *counts.entry(id.0).or_default() += 1;
        }
    }
    let tree = counts
        .iter()
        .max_by_key(|(_, n)| **n)
        .map(|(id, _)| pyreal_primitives::DataId(*id))
        .expect("scenery round the start");
    let pack = pyreal_engine::pack::EnginePack::new(Arc::new(super::cards::tree_pack(tree)));
    let mut loader = AssetLoader::with_pack(pack);
    let mut engine = super::engine();
    let far = 127;
    let rings = pyreal_engine::world::far_tree_rings(1, far);
    assert_eq!(rings, pyreal_engine::world::FAR_TREE_MOST);
    let mut stream = Stream::new(Arc::clone(&store), Arc::clone(&region), 1, far, &loader);
    let eye = Vec2::new(96.0, 96.0);
    settle(&mut stream, &mut engine, &mut loader, START, eye);
    // The field reaches 127 blocks each way, the dats' heights a hundred blocks west.
    let height_at = |engine: &Engine, stream: &Stream, b: (i32, i32)| {
        let f = engine.far.as_ref().expect("the far terrain").field();
        assert_eq!(f.extent, u32::try_from((2 * far + 1) * 8 + 1).unwrap_or(0));
        let corner = stream.block_origin(b).truncate();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (tx, ty) = (
            ((corner.x - f.origin.x) / 24.0).round() as usize,
            ((corner.y - f.origin.y) / 24.0).round() as usize,
        );
        f.heights[ty * f.size as usize + tx]
    };
    let west = (START.0 - 100, START.1);
    let id = u16::try_from((west.0 << 8) | west.1).expect("a block");
    let z = ground_height(&store, &region, id, 0.0, 0.0).expect("the ground");
    let h = height_at(&engine, &stream, west);
    assert!(
        (h - z).abs() < 0.05,
        "the far field's {h} against the dats' {z}"
    );
    // The far trees fill every ring, thinning past the eighth.
    fill_trees(&mut stream, &mut engine, &mut loader, START, eye, rings);
    let per_block = |engine: &Engine, stream: &Stream, c: (i32, i32), lo: i32, hi: i32| {
        let o = stream.block_origin(c).truncate();
        let ring = |p: Vec2| {
            let d = ((p - o) / 192.0).floor();
            #[allow(clippy::cast_possible_truncation)]
            let (x, y) = (d.x as i32, d.y as i32);
            x.abs().max(y.abs()) - 1
        };
        let trees = engine
            .scene
            .instances()
            .filter(|(_, i)| (lo..=hi).contains(&ring(i.transform.translation.truncate())))
            .count();
        let blocks = square(c, 1 + hi).len() - square(c, lo).len();
        #[allow(clippy::cast_precision_loss)]
        let mean = trees as f64 / blocks as f64;
        mean
    };
    let near = per_block(&engine, &stream, START, 2, 8);
    let outer = per_block(&engine, &stream, START, 20, 24);
    eprintln!("far trees a block: {near:.1} within eight rings, {outer:.1} past twenty");
    assert!(near > 5.0, "{near} trees a block near");
    assert!(
        outer > 0.0 && outer < near * 0.3,
        "{outer} trees a block past twenty rings against {near}"
    );
    // A block east: the far terrain and the trees round the new square.
    let next = (START.0 + 1, START.1);
    let walked = std::time::Instant::now();
    settle(
        &mut stream,
        &mut engine,
        &mut loader,
        next,
        Vec2::new(40.0, 96.0),
    );
    eprintln!(
        "first load {:.0} ms; the next square with its far field ready in {:.0} ms",
        stream.stats.full_load_ms,
        walked.elapsed().as_secs_f64() * 1e3
    );
    assert_eq!(stream.centre(), Some(next));
    let west = (next.0 - 100, next.1);
    let id = u16::try_from((west.0 << 8) | west.1).expect("a block");
    let z = ground_height(&store, &region, id, 0.0, 0.0).expect("the ground");
    let h = height_at(&engine, &stream, west);
    assert!((h - z).abs() < 0.05, "after the switch, {h} against {z}");
    fill_trees(
        &mut stream,
        &mut engine,
        &mut loader,
        next,
        Vec2::new(40.0, 96.0),
        rings,
    );
}

/// A jump behind portal space, from Holtburg to Yaraq: the square round the new place is placed
/// over many frames, none of them long, and is drawn only once it is all in; what it draws then is
/// a fresh load's.
#[test]
fn a_jump_behind_portal_space_is_placed_over_frames() {
    let (store, region) = setup();
    let mut engine = super::engine();
    let mut loader = AssetLoader::default();
    let mut stream = Stream::new(Arc::clone(&store), Arc::clone(&region), 2, 2, &loader);
    let eye = Vec2::new(96.0, 96.0);
    settle(&mut stream, &mut engine, &mut loader, START, eye);
    let whole = stream.stats.full_load_ms;
    let yaraq = (0x7D, 0x64);
    stream.staged = true;
    let mut frames = 0;
    let mut longest = 0.0_f64;
    loop {
        let t = std::time::Instant::now();
        let changed = stream.update(&mut engine, &mut loader, yaraq, eye);
        longest = longest.max(t.elapsed().as_secs_f64() * 1e3);
        assert_eq!(
            changed.loaded,
            frames == 0,
            "the world is cleared once, at the jump"
        );
        frames += 1;
        if stream.settled() {
            break;
        }
        // Landed, the square is in whole; what the old square drew is freed a step later.
        if !stream.jumping() {
            assert_eq!(
                stream.centre(),
                Some(yaraq),
                "a jump until the square is in"
            );
            continue;
        }
        assert!(stream.centre().is_none(), "nothing is drawn half placed");
        assert!(frames < 20_000, "the jump never landed");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    eprintln!("placed in {frames} frames, the longest {longest:.1} ms");
    assert!(frames > 3, "placed over frames: {frames}");
    // Against the whole load at the start, timed alike, so a busy machine slows both: the
    // longest step is the new place's first ground, about 20 ms in the test profile.
    assert!(
        longest * 10.0 < whole,
        "the longest frame of the jump: {longest:.1} ms against a whole load's {whole:.1} ms"
    );
    assert_eq!(stream.centre(), Some(yaraq));
    assert_eq!(stream.stats.full_loads, 2);
    let mut placed: Vec<(i32, i32)> = stream.blocks().collect();
    placed.sort_unstable();
    assert_eq!(placed, square(yaraq, 2));
    let jumped = frame(&mut engine, &stream, &region, &store, yaraq, eye);

    let mut fresh_engine = super::engine();
    let mut fresh_loader = AssetLoader::default();
    let mut fresh = Stream::new(Arc::clone(&store), Arc::clone(&region), 2, 2, &fresh_loader);
    settle(&mut fresh, &mut fresh_engine, &mut fresh_loader, yaraq, eye);
    let loaded = frame(&mut fresh_engine, &fresh, &region, &store, yaraq, eye);
    keep("jumped", &jumped);
    keep("jump-loaded", &loaded);
    let d = mean_difference(&jumped, &loaded);
    assert!(
        d < 1.0,
        "the jumped frame differs from a fresh load's by {d:.2} a channel"
    );
}

/// Walking across block seams with the rays on: every mesh the rays hold a bottom level for is
/// still live, so a ground left behind is forgotten with its mesh and its slot, when reused, is
/// never traced as the old ground.
#[test]
fn rays_forget_the_ground_a_walk_frees() {
    let (store, region) = setup();
    let mut engine = super::engine();
    assert!(engine.gpu.rays(), "this test needs ray queries");
    let mut loader = AssetLoader::default();
    let mut stream = Stream::new(Arc::clone(&store), Arc::clone(&region), 1, 3, &loader);
    let eye = Vec2::new(40.0, 96.0);
    settle(&mut stream, &mut engine, &mut loader, START, eye);
    let mut viewer = START;
    for _ in 0..3 {
        let _ = frame(&mut engine, &stream, &region, &store, viewer, eye);
        viewer.0 += 1;
        settle(&mut stream, &mut engine, &mut loader, viewer, eye);
        assert_eq!(stream.centre(), Some(viewer));
        let freed: Vec<_> = engine
            .renderer
            .rays
            .meshes()
            .filter(|&m| engine.scene.mesh(m).is_none_or(|m| !m.live))
            .collect();
        assert!(freed.is_empty(), "the rays hold freed meshes {freed:?}");
    }
}

/// Render space moved by a rebase takes the placed flames with it, as it does their lamps.
#[test]
fn translate_moves_the_flames_with_their_lamps() {
    let mut engine = super::engine();
    let mut loader = AssetLoader::default();
    let at = Vec3::new(10.0, 20.0, 1.0);
    engine.flames.placed.push(pyreal_engine::flames::Flame {
        base: at,
        kind: pyreal_engine::flames::FlameKind::Torch,
        height: 0.4,
        seed: 1,
        reach: 5.0,
        cell: 0,
        object: 7,
        colour: 0,
        count: 0,
        spin: 0,
    });
    engine.lamps.push(pyreal_engine::lamps::Lamp {
        position: at,
        colour: Vec3::ONE,
        candela: 1.0,
        range: 5.0,
        retail: glam::Vec4::ONE,
        flicker: pyreal_engine::lamps::Flicker::Flame,
        shadows: false,
        cell: 0,
        kind: pyreal_engine::lamps::LampKind::Flame,
        object: 7,
        flash: false,
        held: false,
        seed: 0,
    });
    let seed = |l: &pyreal_engine::lamps::Lamp| {
        if l.seed == 0 {
            pyreal_engine::flames::seed_of(l.position)
        } else {
            l.seed
        }
    };
    let before = seed(&engine.lamps[0]);
    let delta = Vec3::new(192.0, -384.0, 0.0);
    engine.translate(&mut loader, delta);
    assert_eq!(engine.flames.placed[0].base, at + delta);
    assert_eq!(
        seed(&engine.lamps[0]),
        before,
        "the lamp flickers on as its flame does"
    );
}

/// A world cleared for a reload keeps none of the last place's water: its ripples' square and
/// their simulation start again, as after a rebase.
#[test]
fn clear_world_forgets_the_last_places_ripples() {
    let mut engine = super::engine();
    let mut loader = AssetLoader::default();
    engine.renderer.water.area = Some([-96.0, -96.0, 192.0]);
    engine.clear_world(&mut loader);
    assert_eq!(engine.renderer.water.area, None);
    assert_eq!(
        engine.renderer.water.sim,
        pyreal_engine::water::Sim::default()
    );
}

/// Bodies dressed in place (no block crossed) let the stream free what nothing draws when over
/// its budget, at most once each `DRESSED_EVICT_EVERY`.
#[test]
fn dressing_bodies_in_place_evicts_when_over_budget() {
    let (store, region) = setup();
    let mut engine = super::engine();
    let mut loader = AssetLoader::default();
    let mut stream = Stream::new(Arc::clone(&store), Arc::clone(&region), 1, 1, &loader);
    stream.budget = Budget {
        vertices: 0.0,
        indices: 0.0,
        meshes: 0.0,
        materials: 0.0,
        textures: 0.0,
        texture_bytes: 0,
    };
    let eye = Vec2::new(150.0, 96.0);
    settle(&mut stream, &mut engine, &mut loader, START, eye);
    let before = stream.stats.evictions;
    // A body dressed: the count the loader keeps of the variants it made.
    loader.dressed_made += 1;
    settle(&mut stream, &mut engine, &mut loader, START, eye);
    assert_eq!(
        stream.stats.evictions,
        before + 1,
        "freed after the dressing"
    );
    loader.dressed_made += 1;
    settle(&mut stream, &mut engine, &mut loader, START, eye);
    assert_eq!(
        stream.stats.evictions,
        before + 1,
        "not again within {:?}",
        pyreal_engine::stream::DRESSED_EVICT_EVERY
    );
}

/// A pack whose obsidian plain (terrain type 6, drawn in every era) is a flat violet ground.
fn obsidian_pack() -> pyreal_art_pack::ArtPack {
    let image = |srgb: bool, texel: [u8; 4]| pyreal_art_pack::Image {
        width: 4,
        height: 4,
        srgb,
        levels: vec![Arc::new(texel.repeat(16))],
        format: None,
    };
    pyreal_art_pack::ArtPack {
        grounds: vec![pyreal_art_pack::PackGround {
            grounds: vec![pyreal_art_pack::Ground::Type(6)],
            albedo: image(true, [70, 50, 90, 128]),
            normal: image(false, [128, 128, 0, 255]),
            surface: image(false, [200, 255, 0, 255]),
            size: 4.0,
            normal_scale: 1.0,
            steep: false,
            fingerprint: None,
            detail: None,
            detail_size: 1.0,
            detail_strength: 1.0,
            pebbles: true,
            credit: Default::default(),
        }],
        ..Default::default()
    }
}

/// A jump from Holtburg to the obsidian plain at 0x322E behind portal space, with the pools
/// freed of what nothing draws while the new square is half placed and not yet drawn: the
/// placed ground keeps its meshes, so the plain lands as a fresh load draws it.
#[test]
fn an_eviction_during_a_jump_keeps_the_waiting_ground() {
    let (store, region) = setup();
    let pack = || {
        AssetLoader::with_pack(pyreal_engine::pack::EnginePack::new(Arc::new(
            obsidian_pack(),
        )))
    };
    let mut engine = super::engine();
    let mut loader = pack();
    let mut stream = Stream::new(Arc::clone(&store), Arc::clone(&region), 2, 2, &loader);
    let eye = Vec2::new(180.0, 150.0);
    settle(&mut stream, &mut engine, &mut loader, START, eye);
    let plain = (0x32, 0x2E);
    stream.staged = true;
    let mut evicted = false;
    for _ in 0..20_000 {
        stream.update(&mut engine, &mut loader, plain, eye);
        if !evicted && stream.jumping() && stream.blocks().count() >= 9 {
            stream.evict(&mut engine, &mut loader);
            evicted = true;
        }
        if stream.settled() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(evicted, "the jump was never caught half placed");
    assert_eq!(stream.centre(), Some(plain));
    let jumped = frame(&mut engine, &stream, &region, &store, plain, eye);
    let mut fresh_engine = super::engine();
    let mut fresh_loader = pack();
    let mut fresh = Stream::new(Arc::clone(&store), Arc::clone(&region), 2, 2, &fresh_loader);
    settle(&mut fresh, &mut fresh_engine, &mut fresh_loader, plain, eye);
    let loaded = frame(&mut fresh_engine, &fresh, &region, &store, plain, eye);
    keep("obsidian-jumped", &jumped);
    keep("obsidian-loaded", &loaded);
    let d = mean_difference(&jumped, &loaded);
    assert!(
        d < 1.0,
        "the plain after the jump differs from a fresh load's by {d:.2} a channel"
    );
}
