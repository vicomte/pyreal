//! Explore mode: the world with no server and no body, flown through and left again.

use std::sync::Arc;

use super::*;
use crate::explore::{Ask, ExploreStart, Key};
use crate::scene::SceneConfig;
use crate::sim_present::SimPresentation;

/// Behaviour: none (Pyreal's own: explore mode)
/// Explore mode builds the world round its place with no body, flies the camera at the speed
/// chosen and keeps it above the ground, and its end leaves the scene the next session builds
/// as it was.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail dats: --features retail-dats"
)]
fn explore_flies_over_the_ground_and_its_end_leaves_the_next_sessions_scene() {
    let store = Arc::new(pyreal_dat::testing::open_store().expect("retail dats"));
    let dat_dir = pyreal_dat::testing::dat_dir();
    let mut app = App::<NullShell>::bring_up_with_store(
        Config {
            headless: true,
            connect: false,
            sound: false,
            dat_dir: dat_dir.clone(),
            ..Config::default()
        },
        Some(store),
        |_| Ok(Platform::headless(64, 64)),
        |_, _, _, _| Ok(Box::new(SimPresentation::new(64, 64))),
    )
    .expect("bring-up");
    let mut shell = NullShell;
    app.start_shell(&mut shell).expect("the shell starts");
    // As the screens before the world leave it: the next session's scene, with a body.
    let session_scene = SceneConfig {
        character: true,
        ..SceneConfig::default()
    };
    app.defer_static_scene(session_scene);

    let place = crate::explore::read_place("0xA9B40017 [84 7.1 94] 1 0 0 0").unwrap();
    app.begin_explore(
        &mut shell,
        &ExploreStart {
            dat_dir,
            classic_dat_dir: None,
            era: None,
            place,
        },
    )
    .expect("explore begins");
    assert!(app.exploring());
    let ws = app.world.as_ref().expect("a world");
    assert!(ws.character.is_none(), "no body");
    let start = crate::explore::camera_place(ws).expect("the camera's place");
    assert_eq!(start.cell.0 >> 16, 0xA9B4);

    // North at the fastest speed for a second, then down into the ground.
    app.explore_ask(Ask::Speed(10));
    app.explore_key(Key::Forward, true);
    for _ in 0..30 {
        app.frame(&mut shell);
    }
    app.explore_key(Key::Forward, false);
    app.explore_key(Key::Down, true);
    for _ in 0..60 {
        app.frame(&mut shell);
    }
    let view = app.explore_view().expect("the overlay's view");
    assert_eq!(view.speed, 5000.0);
    let ws = app.world.as_ref().expect("a world");
    let there = crate::explore::camera_place(ws).expect("the camera's place");
    assert!(
        (there.cell.0 >> 16) & 0xFF > 0xB4 + 10,
        "kilometres north: {:#010X}",
        there.cell.0
    );
    assert!(
        (view.height - crate::explore::ROOM).abs() < 0.01,
        "held at its room above the ground: {}",
        view.height
    );

    app.end_explore(&mut shell);
    assert!(!app.exploring());
    assert!(app.world.is_none(), "the world goes");
    assert_eq!(app.scene_config, Some(session_scene));
    assert_eq!(app.pending_scene, Some(session_scene));
}
