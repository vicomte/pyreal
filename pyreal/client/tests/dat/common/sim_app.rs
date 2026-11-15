//! The actual client shell and frame loop with a world simulation and no graphics device.

use pyreal_client_runtime::sim_present::SimPresentation;
use pyreal_client_shell::platform::host::Host;
use {
    pyreal_client::app::App, pyreal_client_runtime::app::Platform,
    pyreal_client_runtime::app::StartupError, pyreal_client_runtime::config::Config,
};

#[path = "../../common/app.rs"]
#[allow(dead_code)]
mod shared;

#[allow(unused_imports)]
pub use shared::{
    body, frames, game_screen, key, movement_key, player_description, position, unhide_player,
    unhide_recorded_player,
};

/// Bring up the application at the dimensions selected by its normal startup policy.
/// The caller retains control of shell startup, scene loading and frame advancement.
pub fn new(cfg: Config) -> Result<App, StartupError> {
    assert!(
        cfg.headless,
        "the simulation fixture requires a headless platform"
    );
    pyreal_client::hud::install_platform();
    pyreal_client::Desktop::install_default_output();
    App::bring_up_with_store(
        cfg,
        None,
        Default::default(),
        |cfg| Ok(Platform::headless(cfg.width, cfg.height)),
        |_, width, height, _| Ok(Box::new(SimPresentation::new(width, height))),
    )
}

/// Start the UI and configured static scene, then advance into gameplay.
#[allow(dead_code)]
pub fn app_in_gameplay(frames: u32) -> App {
    shared::app_in_gameplay_with(frames, new)
}

/// Build the recorded player on real terrain and finish its recorded unhide.
pub fn app_with_recorded_body() -> App {
    shared::app_with_recorded_body_with(new)
}

#[test]
fn the_real_shell_frames_and_resizes_its_simulated_world() {
    use {pyreal_client_runtime::config::Preferences, pyreal_client_runtime::scene::SceneConfig};
    let scratch = pyreal_dat::testing::ScratchDir::new("simulated-shell").unwrap();
    let mut cfg = Config {
        headless: true,
        connect: false,
        sound: false,
        ui: true,
        dat_dir: pyreal_dat::testing::dat_dir(),
        preferences_file: scratch.path().join("prefs.ini"),
        ..Default::default()
    };
    cfg.apply_preferences(&Preferences::parse("[Display]\nResolution=1024x768\n"));
    let mut app = new(cfg).expect("real shell with simulation");
    assert_eq!(app.presentation().size(), (800, 600), "late login size");
    assert_eq!(app.window.client_size(), (800, 600));
    app.start_shell().expect("real input and UI");
    app.load_static_scene(SceneConfig {
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..Default::default()
    })
    .expect("real terrain and body");
    shared::game_screen(&mut app);
    frames(&mut app, 8);
    assert_eq!(
        app.presentation().size(),
        (1024, 768),
        "gameplay restores preference"
    );
    let handle = body(&app).handle;
    let sim = app
        .presentation()
        .as_any()
        .downcast_ref::<SimPresentation>()
        .unwrap();
    assert!(sim.steps.updates > 0, "frames drive world physics");
    assert!(sim.resident_blocks().next().is_some());
    let counts = sim.device().counts();
    assert!(counts.draw_overlay > 0, "the interface is drawn");
    assert!(counts.start_frame > 0 && counts.draw_scene > 0);
    assert_eq!(counts.start_frame, counts.end_frame);
    app.force_display_resolution(true, 1280, 720);
    assert_eq!(app.presentation().size(), (1280, 720));
    assert_eq!(
        body(&app).handle,
        handle,
        "resize keeps the same physical body"
    );
    let sim = app
        .presentation()
        .as_any()
        .downcast_ref::<SimPresentation>()
        .unwrap();
    assert!(sim.device().counts().resize > counts.resize);
    app.shutdown();
}

#[test]
fn the_shared_recorded_body_fixture_runs_through_the_simulated_app() {
    let mut app = app_with_recorded_body();
    let before = position(&app);
    let forward = pyreal_client_runtime::actions::names::action_for_enum_name("MovementForward")
        .expect("movement action");
    app.inject_action(pyreal_client_runtime::actions::Action::begin(forward));
    frames(&mut app, 60);
    app.inject_action(pyreal_client_runtime::actions::Action::end(forward));
    frames(&mut app, 1);
    let after = position(&app);
    assert!(pyreal_animation::motion::moveto::distance(&before, &after) > 2.0);
    app.shutdown();
}
