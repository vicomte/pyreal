//! Typed chat lines and live shell observations shared by the DAT and GPU tests.

use pyreal_client_model::Request;
use {pyreal_client::app::App, pyreal_client_runtime::config::Config};

pub(crate) fn app_with(make: impl FnOnce(Config) -> App) -> App {
    let mut app = make(Config {
        headless: true,
        sound: false,
        ui: true,
        width: 800,
        height: 600,
        dat_dir: pyreal_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir().join("pyreal-chat-commands/preferences.ini"),
        ..Config::default()
    });
    app.start_shell().expect("UI shell");
    let ui = &mut app
        .shell_mut()
        .horizon_mut()
        .expect("the interface is up")
        .ui;
    ui.options.screen = pyreal_horizon::options::StartScreen::Game;
    ui.screen = pyreal_horizon::ui::Screen::Game;
    for _ in 0..4 {
        assert!(app.frame());
    }
    app
}

pub(crate) fn install_player_module(app: &mut App) -> pyreal_protocol::login::PlayerModule {
    let module = pyreal_protocol::login::PlayerModule::default();
    app.probe_mut().hud_mut().player_module = Some(module.clone());
    app.probe_mut()
        .objects_mut()
        .world
        .player_system
        .apply_player_module(&module);
    module
}

pub(crate) fn forced_saved_player_module(app: &mut App) -> pyreal_protocol::login::PlayerModule {
    let mut requests = pyreal_client_model::RecordingRequests::default();
    assert!(
        app.probe_mut()
            .objects_mut()
            .world
            .player_system
            .save_to_server(&mut requests, true),
        "the forced save path has an authoritative module"
    );
    let [Request::CharacterOptionsEvent(saved)] = requests.0.as_slice() else {
        panic!("forced save produced {:?}", requests.0)
    };
    saved.module.clone()
}

/// Submits chat lines as the main chat window does.
#[derive(Default)]
pub(crate) struct Hand;

impl Hand {
    pub(crate) fn new() -> Self {
        Self
    }

    /// Submit one line from the main chat window, as its entry does on `VK_RETURN`.
    pub(crate) fn submit(&mut self, app: &mut App, text: &str) {
        app.submit_requests(vec![pyreal_client_contract::UiRequest::ChatLine {
            text: text.to_owned(),
            window: pyreal_client_contract::chat::interface::window::MAIN,
        }]);
        assert!(app.frame());
    }
}
