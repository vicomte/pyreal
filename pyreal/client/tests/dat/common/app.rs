//! A headless `App` in gameplay over the retail dats: the shell up, the static scene loaded and
//! the interface on its game screen, with no graphics device (`NullPresentation`) and no network.

use pyreal_client::app::App;
use pyreal_client_runtime::config::Config;
use pyreal_client_runtime::present::NullPresentation;
use pyreal_primitives::ObjectId;

use super::client_dir;

/// A headless app in gameplay after `frames` frames.
///
/// With `player`, that id is seeded as the world's player before the first frame. The player's
/// qualities record is allocated when the player object is made, before any update can arrive;
/// a test that applies HUD events without replaying the creates that build the player needs the
/// same state, so the row is adopted through `set_player`, which allocates the qualities and
/// installs any parked description. A bare `Weenie::new` has no qualities record, and an update
/// arriving at one is refused as unstorable rather than creating it.
///
/// The app answers for the server it does not have (`pyreal_client_runtime::server_stub`), as every
/// headless app with no connection does; [`app_in_gameplay_unanswered`] is the one that does not.
pub fn app_in_gameplay(frames: u32, player: Option<ObjectId>) -> App {
    build(frames, player, true)
}

/// [`app_in_gameplay`] with no stand-in server: nothing the client asks for is answered unless
/// the test answers it, so whatever waits on an answer waits until the test gives one.
pub fn app_in_gameplay_unanswered(frames: u32, player: Option<ObjectId>) -> App {
    build(frames, player, false)
}

fn build(frames: u32, player: Option<ObjectId>, answered: bool) -> App {
    let cfg = Config {
        ui: true,
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::with_presentation(cfg, Box::new(NullPresentation::new(800, 600)))
        .expect("the headless app starts");
    assert!(
        app.server_stub.is_some(),
        "a headless app with no connection answers for its server"
    );
    if !answered {
        app.server_stub = None;
    }
    app.start_shell().expect("the shell comes up");
    if let Some(player) = player {
        let w = &mut app.probe_mut().objects_mut().world;
        w.tables
            .weenies
            .insert(player, pyreal_client_model::weenie::Weenie::new(player));
        assert!(w.set_player(player), "the identity is adopted once");
    }
    let s = pyreal_client_runtime::scene::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..pyreal_client_runtime::scene::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    let ui = &mut app
        .shell_mut()
        .horizon_mut()
        .expect("the interface is up")
        .ui;
    ui.options.screen = pyreal_horizon::options::StartScreen::Game;
    ui.screen = pyreal_horizon::ui::Screen::Game;
    for _ in 0..frames {
        app.frame();
    }
    app
}
