//! The client draws its world through the engine. A reported place, a dungeon and a house's room,
//! each captured by the real binary with no window, cover the frame with a picture; in a headless
//! `App`, the body is drawn as an object the engine picks by its id, the pick read a frame late, and
//! a preview space draws its body into the interface's picture. Fixture: the retail dats
//! (`PYREAL_TEST_DAT_DIR`) and a graphics device; a missing input fails.

#![cfg(gpu)]

use std::path::Path;
use std::process::Command;

use pyreal_client::app::App;
use pyreal_client_contract::overlay::{OverlayItem, PreviewSpace};
use pyreal_client_runtime::config::Config;
use pyreal_client_runtime::present::Presentation as _;
use pyreal_primitives::Viewport;
use {pyreal_client_runtime::scene::SceneConfig, pyreal_world_data::landblock::DEFAULT_LANDBLOCK};

use crate::common::{client_dir, gpu_lock};

/// Holtburg by the road, as a player reported it.
const HOLTBURG: &str = "dereth report 1; loc 0xA9B4002A [142.374573 31.550867 94.005005] \
     -0.996720 0.000000 0.000000 -0.080924; camera 0xA9B4 [146.54408 25.145638 97.8691] yaw \
     -5.7061586 pitch -0.3000001 cell 0xA9B40032; fov 90 vertical 58.343605 aspect 1; time 0.259 \
     Dawnsong, set here; clock 307617752.2452435; weather clear";

/// A dungeon's brick hall, as a player reported it.
const DUNGEON: &str = "dereth report 1; loc 0x019E0114 [10.049774 -40.462921 0.005000] \
     0.993845 0.000000 0.000000 -0.110784; camera 0x019E [10.77995 -44.2 1.9255505] yaw -6.090231 \
     pitch -0.11000007 cell 0x019E0114; fov 90 vertical 58.343605 aspect 1; time 0.690 \
     Warmtide-and-Half, server's clock; clock 307613419.6266459; weather auto";

/// One frame of the real binary, `args` after the dats and a size: its pixels (RGBA) and its log.
fn capture(name: &str, args: &[&str]) -> (u32, u32, Vec<u8>, String) {
    let dat_dir = client_dir();
    assert!(
        pyreal_dat::testing::have_dats(),
        "the retail dats are this test's world and there are none at {} -- set \
         PYREAL_TEST_DAT_DIR",
        dat_dir.display()
    );
    let dir = std::env::temp_dir().join("pyreal-engine-frames");
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let out = dir.join(format!("{name}.png"));
    let _ = std::fs::remove_file(&out);
    let home = dir.join(format!("{name}-home"));
    std::fs::create_dir_all(&home).expect("a scratch home");
    let output = Command::new(env!("CARGO_BIN_EXE_pyreal"))
        .args([
            "--headless",
            "--no-ui",
            "--frames",
            "8",
            "--size",
            "480x270",
        ])
        .args(args)
        .arg("--capture")
        .arg(&out)
        .arg("--dat-dir")
        .arg(&dat_dir)
        .env("PYREAL_SETTINGS_DIR", &home)
        .env("USERPROFILE", &home)
        .env("APPDATA", &home)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &home)
        .output()
        .expect("the client starts");
    let log = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "{name}: exit {:?}\n{log}",
        output.status.code()
    );
    assert!(!log.contains("panicked"), "{name}: {log}");
    assert!(
        log.contains("the engine loaded"),
        "{name}: the engine loaded no world\n{log}"
    );
    let (w, h, rgba) = read_png(&out);
    let _ = std::fs::remove_dir_all(&home);
    (w, h, rgba, log)
}

fn read_png(path: &Path) -> (u32, u32, Vec<u8>) {
    let mut decoder = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(path).expect("the capture was written"),
    ));
    decoder.set_transformations(png::Transformations::ALPHA);
    let mut reader = decoder.read_info().expect("a PNG");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("one frame");
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

/// The share of the frame's pixels that are not black, and how many colours it holds.
fn coverage(rgba: &[u8]) -> (f64, usize) {
    let pixels = rgba.as_chunks::<4>().0;
    let lit = pixels
        .iter()
        .filter(|p| p[0] > 4 || p[1] > 4 || p[2] > 4)
        .count();
    let colours: std::collections::BTreeSet<[u8; 3]> =
        pixels.iter().map(|p| [p[0], p[1], p[2]]).collect();
    #[allow(clippy::cast_precision_loss)]
    let share = lit as f64 / pixels.len().max(1) as f64;
    (share, colours.len())
}

#[test]
fn a_reported_place_drawn_through_the_engine_covers_the_frame() {
    let _gpu = gpu_lock();
    let (w, h, rgba, _) = capture("holtburg", &["--report", HOLTBURG]);
    assert_eq!((w, h), (480, 270));
    let (share, colours) = coverage(&rgba);
    assert!(share > 0.99, "Holtburg by day covers the frame: {share:.3}");
    assert!(
        colours > 2000,
        "a picture, not a cleared target: {colours} colours"
    );
}

#[test]
fn a_dungeon_and_a_room_draw_through_the_engine() {
    let _gpu = gpu_lock();
    let (_, _, dungeon, _) = capture("dungeon", &["--report", DUNGEON]);
    let (share, colours) = coverage(&dungeon);
    assert!(share > 0.5, "the dungeon's hall is drawn: {share:.3} lit");
    assert!(colours > 500, "the dungeon: {colours} colours");
    let (_, _, room, _) = capture(
        "room",
        &["--landblock", "0xA9B4", "--start-cell", "0xA9B40108"],
    );
    let (share, colours) = coverage(&room);
    assert!(share > 0.9, "the house's room is drawn: {share:.3} lit");
    assert!(colours > 1000, "the room: {colours} colours");
}

/// A headless `App` with a body on Holtburg, stepped `frames` times.
fn app_with_body(frames: usize) -> App {
    let mut app = App::new(Config {
        headless: true,
        width: 320,
        height: 240,
        sound: false,
        ui: false,
        dat_dir: pyreal_dat::testing::dat_dir(),
        ..Default::default()
    })
    .expect("a headless App over the retail dats");
    app.start_shell().expect("the input shell starts");
    app.load_static_scene(SceneConfig {
        landblock: DEFAULT_LANDBLOCK,
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        ..Default::default()
    })
    .expect("real terrain and a real body");
    for _ in 0..frames {
        assert!(app.frame());
    }
    app
}

#[test]
fn the_engine_picks_the_body_by_its_id_a_frame_late() {
    let _gpu = gpu_lock();
    let mut app = app_with_body(20);
    let body = app
        .world_state()
        .and_then(|w| w.character.as_ref())
        .map(pyreal_client_runtime::character::Character::object_id)
        .expect("a body");
    // Where the body's middle stands on the screen, from the chase camera.
    let at = {
        let world = app.world_state().expect("a world");
        let c = world.character.as_ref().expect("a body");
        let f = c.render_frame();
        let p = glam::Vec3::new(f.origin.x, f.origin.y, f.origin.z + 0.9);
        let cam = world.camera.frame();
        let fov = app
            .present
            .scene(Some(world))
            .expect("a scene")
            .as_pick_scene()
            .fov_y_rad((320, 240));
        let camera = pyreal_scene::bridge::camera(&cam, glam::Vec3::ZERO, fov, 1000.0, None);
        let clip = glam::Mat4::perspective_rh(fov, 320.0 / 240.0, 0.1, 1000.0)
            * camera.view()
            * p.extend(1.0);
        (
            (clip.x / clip.w * 0.5 + 0.5) * 320.0,
            (0.5 - clip.y / clip.w * 0.5) * 240.0,
        )
    };
    let scene_answer = |app: &App| {
        app.present
            .scene(app.world_state())
            .expect("a scene")
            .as_pick_scene()
            .picked(at)
    };
    assert_eq!(
        scene_answer(&app),
        None,
        "the first ask has no answer and arms a pick of the next frame"
    );
    let mut answer = None;
    for _ in 0..6 {
        assert!(app.frame());
        // The pick's texel is read back once the device is done with the frame.
        app.renderer().gpu.wait();
        answer = scene_answer(&app);
        if answer.is_some() {
            break;
        }
    }
    assert_eq!(
        answer,
        Some(body),
        "the pixel at the body's middle is the body's"
    );
}

#[test]
fn a_preview_space_draws_its_body_into_the_interface() {
    let _gpu = gpu_lock();
    let mut app = app_with_body(2);
    let assets = std::sync::Arc::new(pyreal_world_data::anim_assets::DatAnimAssets::new(
        pyreal_dat::testing::open_store_or_fail().into(),
    ));
    let r = app.renderer_mut();
    assert!(r.ensure_preview(PreviewSpace::CharGen, &assets));
    let store = pyreal_dat::testing::open_store_or_fail();
    // An Aluvian man.
    let index = r
        .add_preview_object(
            PreviewSpace::CharGen,
            &store,
            pyreal_primitives::DataId(0x0200_0001),
        )
        .expect("no device failure");
    assert_eq!(index, Some(0));
    if let Some(s) = r.preview_mut(PreviewSpace::CharGen) {
        s.mode
            .set_camera_position(pyreal_primitives::Vec3::new(0.0, -3.0, 1.0));
    }
    let rect = Viewport {
        x: 0,
        y: 0,
        width: 160,
        height: 240,
    };
    r.start_frame().expect("a frame starts");
    r.draw_overlay(&[OverlayItem::Preview {
        space: PreviewSpace::CharGen,
        rect,
    }])
    .expect("the overlay draws");
    r.end_frame().expect("the frame draws");
    let (w, _, rgba) = r.capture_rgba().expect("a frame");
    let w = usize::try_from(w).expect("a width");
    let lit = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(i, p)| i % w < 160 && (p[0] > 8 || p[1] > 8 || p[2] > 8))
        .count();
    assert!(
        lit > 500,
        "the body is drawn in the preview's rectangle: {lit} pixels"
    );
}

/// HDR Output ticked with a world loaded: offered a float surface in extended linear sRGB, the
/// window takes it and the world is drawn on; unticked, it goes back to sRGB.
#[test]
fn hdr_output_ticked_with_a_world_loaded_takes_the_float_output() {
    use pyreal_client_contract::options::names::fidelity::HDR;
    use pyreal_client_contract::{PrefValue, UiRequest};
    use wgpu::{SurfaceColorSpaces as Cs, TextureFormat as F};
    let _gpu = gpu_lock();
    let mut app = App::new(Config {
        headless: true,
        width: 320,
        height: 240,
        sound: false,
        ui: false,
        dat_dir: pyreal_dat::testing::dat_dir(),
        ..Default::default()
    })
    .expect("a headless App over the retail dats");
    app.load_static_scene(SceneConfig {
        landblock: DEFAULT_LANDBLOCK,
        land_radius: 1,
        scenery_radius: 0,
        ..Default::default()
    })
    .expect("real terrain");
    let offered = [
        (F::Bgra8UnormSrgb, Cs::SRGB),
        (F::Rgba16Float, Cs::SRGB | Cs::EXTENDED_SRGB_LINEAR),
    ];
    app.renderer_mut()
        .offer_surface_capabilities(wgpu::SurfaceCapabilities {
            formats: offered.iter().map(|f| f.0).collect(),
            format_capabilities: offered
                .iter()
                .map(|&(format, color_spaces)| wgpu::SurfaceFormatCapabilities {
                    format,
                    color_spaces,
                })
                .collect(),
            ..wgpu::SurfaceCapabilities::default()
        });
    let tick =
        |app: &mut App, on: bool| {
            let rest = app.renderer_mut().apply_render_preference_requests(vec![
                UiRequest::SetPreference(HDR, PrefValue::Bool(on)),
            ]);
            assert!(rest.is_empty(), "HDR Output is the scene's preference");
            assert!(app.frame(), "a frame is drawn after the switch");
            app.renderer().output_format_in_use()
        };
    assert_eq!(tick(&mut app, true), F::Rgba16Float);
    assert!(app.renderer().bridge.is_some(), "the world is still loaded");
    assert_eq!(tick(&mut app, false), F::Bgra8UnormSrgb);
}

/// The base pack opens on its own thread at start-up, and the first world waits for it: loaded
/// before any frame, it is drawn with the pack.
#[test]
fn the_first_world_waits_for_the_base_pack_opening_on_its_thread() {
    let _gpu = gpu_lock();
    let pack = std::env::temp_dir().join(format!("pyreal-base-pack-wait-{}", std::process::id()));
    std::fs::create_dir_all(&pack).expect("a pack folder");
    std::fs::write(
        pack.join("manifest.toml"),
        "[pack]\nname = \"tiny\"\nformat = 1\n",
    )
    .expect("a manifest");
    let mut app = App::new(Config {
        headless: true,
        width: 320,
        height: 240,
        sound: false,
        ui: false,
        dat_dir: pyreal_dat::testing::dat_dir(),
        base_pack: Some(pack.clone()),
        ..Default::default()
    })
    .expect("a headless App over the retail dats");
    app.load_static_scene(SceneConfig {
        landblock: DEFAULT_LANDBLOCK,
        land_radius: 1,
        scenery_radius: 0,
        ..Default::default()
    })
    .expect("real terrain");
    let drawn = app.renderer().art_pack().map(|(name, _)| name);
    let _ = std::fs::remove_dir_all(&pack);
    assert_eq!(
        drawn.as_deref(),
        Some("tiny"),
        "the world is drawn with the base pack"
    );
}

/// The meshes a preview space holds are kept through an eviction that frees everything else, and
/// let go once the space holds them no more.
#[test]
fn a_preview_keeps_its_meshes_from_eviction_until_it_lets_them_go() {
    let _gpu = gpu_lock();
    let mut app = app_with_body(2);
    let assets = std::sync::Arc::new(pyreal_world_data::anim_assets::DatAnimAssets::new(
        pyreal_dat::testing::open_store_or_fail().into(),
    ));
    let store = pyreal_dat::testing::open_store_or_fail();
    let r = app.renderer_mut();
    assert!(r.ensure_preview(PreviewSpace::CharGen, &assets));
    // An Aluvian man.
    r.add_preview_object(
        PreviewSpace::CharGen,
        &store,
        pyreal_primitives::DataId(0x0200_0001),
    )
    .expect("no device failure");
    if let Some(s) = r.preview_mut(PreviewSpace::CharGen) {
        s.mode
            .set_camera_position(pyreal_primitives::Vec3::new(0.0, -3.0, 1.0));
    }
    let rect = Viewport {
        x: 0,
        y: 0,
        width: 160,
        height: 240,
    };
    let draw = |r: &mut pyreal_client_shell::gpu::Renderer| {
        r.start_frame().expect("a frame starts");
        r.draw_overlay(&[OverlayItem::Preview {
            space: PreviewSpace::CharGen,
            rect,
        }])
        .expect("the overlay draws");
        r.end_frame().expect("the frame draws");
        let (w, _, rgba) = r.capture_rgba().expect("a frame");
        let w = usize::try_from(w).expect("a width");
        rgba.as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(i, p)| i % w < 160 && (p[0] > 8 || p[1] > 8 || p[2] > 8))
            .count()
    };
    assert!(draw(r) > 500, "the body is drawn");
    let held: Vec<_> = r.previews.pinned().collect();
    assert!(!held.is_empty(), "the space's meshes are pinned");
    let scene: &mut pyreal_client_shell::gpu::SceneRenderer = r;
    pyreal_engine::evict::evict(&mut scene.engine, &mut scene.loader, Vec::new());
    let live: std::collections::BTreeSet<_> = r.engine.scene.live_meshes().collect();
    assert!(
        held.iter().all(|m| live.contains(m)),
        "no held mesh was freed"
    );
    assert!(draw(r) > 500, "the body is drawn after the eviction");
    if let Some(s) = r.preview_mut(PreviewSpace::CharGen) {
        s.remove_all_objects();
    }
    let _ = draw(r);
    assert_eq!(
        r.previews.pinned().count(),
        0,
        "let go with the space's objects"
    );
    let pinned: std::collections::BTreeSet<_> = r.engine.scene.pinned().collect();
    assert!(
        held.iter().all(|m| !pinned.contains(m)),
        "unpinned in the scene"
    );
}

/// HDR Output ticked while the base pack is still opening on its thread, before any frame: the
/// engine made again on the float format, the first world is drawn with the pack and the float
/// output both.
#[test]
fn hdr_ticked_while_the_base_pack_opens_keeps_both() {
    use pyreal_client_contract::options::names::fidelity::HDR;
    use pyreal_client_contract::{PrefValue, UiRequest};
    use wgpu::{SurfaceColorSpaces as Cs, TextureFormat as F};
    let _gpu = gpu_lock();
    let pack = std::env::temp_dir().join(format!("pyreal-hdr-pack-{}", std::process::id()));
    std::fs::create_dir_all(&pack).expect("a pack folder");
    std::fs::write(
        pack.join("manifest.toml"),
        "[pack]\nname = \"tiny\"\nformat = 1\n",
    )
    .expect("a manifest");
    let mut app = App::new(Config {
        headless: true,
        width: 320,
        height: 240,
        sound: false,
        ui: false,
        dat_dir: pyreal_dat::testing::dat_dir(),
        base_pack: Some(pack.clone()),
        ..Default::default()
    })
    .expect("a headless App over the retail dats");
    let offered = [
        (F::Bgra8UnormSrgb, Cs::SRGB),
        (F::Rgba16Float, Cs::SRGB | Cs::EXTENDED_SRGB_LINEAR),
    ];
    let r = app.renderer_mut();
    r.offer_surface_capabilities(wgpu::SurfaceCapabilities {
        formats: offered.iter().map(|f| f.0).collect(),
        format_capabilities: offered
            .iter()
            .map(|&(format, color_spaces)| wgpu::SurfaceFormatCapabilities {
                format,
                color_spaces,
            })
            .collect(),
        ..wgpu::SurfaceCapabilities::default()
    });
    let rest = r.apply_render_preference_requests(vec![UiRequest::SetPreference(
        HDR,
        PrefValue::Bool(true),
    )]);
    assert!(rest.is_empty(), "HDR Output is the scene's preference");
    assert_eq!(r.output_format_in_use(), F::Rgba16Float);
    // The world is loaded with the same preference, as the saved one would bring it.
    let mut render = pyreal_client_runtime::render_prefs::RenderPreferences::default();
    assert!(render.set_named(HDR, &PrefValue::Bool(true)));
    app.load_static_scene(SceneConfig {
        landblock: DEFAULT_LANDBLOCK,
        land_radius: 1,
        scenery_radius: 0,
        render,
        ..Default::default()
    })
    .expect("real terrain");
    assert!(app.frame(), "a frame is drawn");
    let drawn = app.renderer().art_pack().map(|(name, _)| name);
    let format = app.renderer().output_format_in_use();
    let _ = std::fs::remove_dir_all(&pack);
    assert_eq!(
        drawn.as_deref(),
        Some("tiny"),
        "the world is drawn with the base pack"
    );
    assert_eq!(format, F::Rgba16Float, "and to the float output");
}
