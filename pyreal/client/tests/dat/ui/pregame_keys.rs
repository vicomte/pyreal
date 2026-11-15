//! Before the player is in the world, no key reaches an in-world command: on the character screen
//! `G` does not start an examine, `F` does not start a use, `W` does not walk, and no gameplay input
//! map is registered at all, while Escape still reaches the screen's own map. Entering the game
//! screen brings every one of them back, and leaving it takes them away again.
//! Fixture: the retail dats' keymaps; a headless `App` with its UI up and no network, driven with
//! synthetic key messages.

use crate::common::client_dir;

use pyreal_client::app::App;
use pyreal_client_runtime::config::Config;
use pyreal_client_runtime::interaction::TargetMode;
use pyreal_client_runtime::present::NullPresentation;
use pyreal_input::host::HostEvent;
use pyreal_input::keys::Key;
use pyreal_input::{ActionId, InputMapId};
use {
    pyreal_client_shell::input::InputShell, pyreal_client_shell::input::BASE_MAP_REGISTRATIONS,
    pyreal_client_shell::input::WHOLE_RUN_INPUT_MAPS,
};

/// `SelectionExamine` and `UseSelected`, both in the UI-commands map.
const SELECTION_EXAMINE: ActionId = ActionId(0x1000_002B);
const USE: ActionId = ActionId(0x1000_0025);
const UI_COMMANDS_MAP: InputMapId = InputMapId(0x1000_0009);
/// The pre-game screens' own map and its Escape action.
const DIALOG_BOXES_MAP: InputMapId = InputMapId(9);
const ESCAPE_KEY: ActionId = ActionId(0x27);

const KEY_G: Key = Key::new(0x47, 0x22);
const KEY_F: Key = Key::new(0x46, 0x21);

/// The maps a character session registers: every base row the whole run does not keep.
fn session_maps() -> Vec<u32> {
    BASE_MAP_REGISTRATIONS
        .iter()
        .map(|(_, map, _)| *map)
        .filter(|m| !WHOLE_RUN_INPUT_MAPS.contains(m))
        .collect()
}

fn registered_maps(input: &InputShell) -> Vec<u32> {
    input
        .manager
        .maps
        .entries()
        .iter()
        .map(|e| e.map.0)
        .collect()
}

/// Put the interface on its game screen or on character select, and run a frame.
fn show(app: &mut App, game: bool) {
    let ui = &mut app
        .shell_mut()
        .horizon_mut()
        .expect("the interface is up")
        .ui;
    let (start, screen) = if game {
        (
            pyreal_horizon::options::StartScreen::Game,
            pyreal_horizon::ui::Screen::Game,
        )
    } else {
        (
            pyreal_horizon::options::StartScreen::Lobby,
            pyreal_horizon::ui::Screen::Lobby,
        )
    };
    ui.options.screen = start;
    ui.screen = screen;
    app.frame();
}

/// Feed one key press and release straight into the input manager and return what it resolved to.
fn resolve(app: &mut App, key: Key, t: &mut u32) -> Vec<(u32, u32, bool)> {
    let mut pump = pyreal_desktop::pump::Pump::new();
    let input = app.input_manager_mut().expect("the input manager is up");
    input.manager.take_events();
    for pressed in [true, false] {
        *t += 10;
        input.on_message(pump.key_message_for_key(key, pressed, *t));
    }
    input
        .manager
        .take_events()
        .iter()
        .map(|e| (e.input_map.0, e.action.0, e.start))
        .collect()
}

/// Press and release a key through the window, as the event loop hands it on, and run a frame.
fn press_through_the_window(app: &mut App, key: Key, text: &str, t: &mut u32) {
    for pressed in [true, false] {
        *t += 10;
        app.handle_window_event(
            &HostEvent::KeyboardInput {
                key,
                pressed,
                text: pressed.then(|| text.to_owned()),
            },
            *t,
        );
        app.frame();
    }
}

#[test]
fn g_and_f_start_no_examine_or_use_on_the_character_screen_and_do_in_the_world() {
    assert!(
        pyreal_dat::testing::have_dats(),
        "the shipped keymaps are this test's oracle: no dats at {}",
        client_dir().display()
    );
    let cfg = Config {
        headless: true,
        ui: true,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::with_presentation(cfg, Box::new(NullPresentation::new(800, 600)))
        .expect("a headless app");
    app.start_shell().expect("the UI shell comes up");

    // The two keys are the key map's examine and use keys.
    let input = app.input_manager().expect("the input manager is up");
    let offsets = |a| {
        input
            .keys_for_action(a, UI_COMMANDS_MAP)
            .iter()
            .filter(|k| k.meta_mode == 0)
            .map(|k| k.control.offset())
            .collect::<Vec<_>>()
    };
    assert!(
        offsets(SELECTION_EXAMINE).contains(&0x22),
        "G examines: {:x?} {:?}",
        offsets(SELECTION_EXAMINE),
        input.keys_for_action(SELECTION_EXAMINE, UI_COMMANDS_MAP)
    );
    assert!(offsets(USE).contains(&0x21), "F uses: {:x?}", offsets(USE));

    // ---- the character screen ----
    show(&mut app, false);
    app.frame();
    let registered = registered_maps(app.input_manager().unwrap());
    for m in session_maps() {
        assert!(
            !registered.contains(&m),
            "map {m:#x} is an in-world map and must not be registered on the character screen: \
             {registered:x?}"
        );
    }
    assert!(registered.contains(&DIALOG_BOXES_MAP.0), "{registered:x?}");

    let mut t = 1_000;
    assert_eq!(
        resolve(&mut app, KEY_G, &mut t),
        vec![],
        "G resolves to nothing"
    );
    assert_eq!(
        resolve(&mut app, KEY_F, &mut t),
        vec![],
        "F resolves to nothing"
    );
    assert_eq!(
        resolve(&mut app, Key::KEY_W, &mut t),
        vec![],
        "W resolves to nothing"
    );
    // A pre-game key still reaches the screen's own map. Escape there is a click: one start.
    assert_eq!(
        resolve(&mut app, Key::ESCAPE, &mut t),
        vec![(DIALOG_BOXES_MAP.0, ESCAPE_KEY.0, true)],
        "Escape is the character screen's"
    );

    // The whole path, window to cursor: neither key leaves a target mode or a changed cursor.
    let cursor = app.current_cursor_did();
    assert!(cursor.is_some(), "a cursor is installed");
    press_through_the_window(&mut app, KEY_G, "g", &mut t);
    assert_eq!(app.interaction().target_mode(), TargetMode::None, "after G");
    assert_eq!(app.current_cursor_did(), cursor, "after G");
    press_through_the_window(&mut app, KEY_F, "f", &mut t);
    assert_eq!(app.interaction().target_mode(), TargetMode::None, "after F");
    assert_eq!(app.current_cursor_did(), cursor, "after F");

    // ---- the game screen: every in-world map is back, and G examines ----
    show(&mut app, true);
    app.frame();
    let registered = registered_maps(app.input_manager().unwrap());
    for m in session_maps() {
        assert!(
            registered.contains(&m),
            "map {m:#x} is registered in the world: {registered:x?}"
        );
    }
    assert!(
        resolve(&mut app, KEY_G, &mut t).contains(&(UI_COMMANDS_MAP.0, SELECTION_EXAMINE.0, true)),
        "G examines in the world"
    );

    // ---- and back to the character screen, as a log-off does ----
    show(&mut app, false);
    app.frame();
    assert_eq!(resolve(&mut app, KEY_G, &mut t), vec![], "G after leaving");
    assert_eq!(resolve(&mut app, KEY_F, &mut t), vec![], "F after leaving");

    app.shutdown();
}
