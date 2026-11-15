//! The Horizon interface: brought up as the client starts on the art the host hands over, waiting
//! while the art loads and staying down, with the host's reason, while the host cannot get it; and
//! the game as the player plays it there.

use super::*;
use crate::{
    app::CoreApp,
    platform::{
        host::{Host, NullHost},
        window::HostEvent,
    },
};

struct Client {
    app: CoreApp<NullHost>,
    shell: ClientShell<NullHost>,
}

/// The client on host `H` a couple of frames after it starts, its settings kept in `state`.
fn started<H: Host>(state: &std::path::Path) -> (CoreApp<H>, ClientShell<H>) {
    let cfg = pyreal_client_runtime::config::Config {
        headless: true,
        connect: false,
        sound: false,
        world: false,
        width: 800,
        height: 600,
        dat_dir: pyreal_dat::testing::dat_dir(),
        preferences_file: state.join("prefs.ini"),
        ..Default::default()
    };
    let mut app = CoreApp::<H>::bring_up_with_store(
        cfg,
        None,
        |_| Ok(pyreal_client_runtime::app::Platform::headless(800, 600)),
        |_, _, _, _| Ok(Box::new(crate::present::NullPresentation::new(800, 600))),
    )
    .expect("real tables and a device-free presentation");
    let mut shell = ClientShell::with_window_events(app.window.raw_handle(), Default::default());
    app.start_shell(&mut shell).expect("real input and UI");
    for _ in 0..2 {
        assert!(app.frame(&mut shell));
    }
    (app, shell)
}

impl Client {
    /// The client with the Horizon interface at its game screen.
    fn new(state: &std::path::Path) -> Self {
        Self::with(state, pyreal_horizon::options::HorizonOptions::default())
    }

    /// The client at the Horizon interface's character list, as it starts.
    fn before_the_world(state: &std::path::Path) -> Self {
        let (app, shell) = started::<NullHost>(state);
        Self { app, shell }
    }

    /// The client with the Horizon interface at its game screen with `options`.
    fn with(state: &std::path::Path, options: pyreal_horizon::options::HorizonOptions) -> Self {
        let mut c = Self::before_the_world(state);
        c.install_horizon_with(options);
        c
    }

    /// The Horizon interface brought up again at its game screen with `options`, with no art
    /// behind it, and a frame for it.
    fn install_horizon_with(&mut self, options: pyreal_horizon::options::HorizonOptions) {
        let mut cx = self.app.ui_context();
        let mut ui = pyreal_horizon::runtime::HorizonFrontEnd::new(
            std::sync::Arc::new(pyreal_horizon::art::Art::empty()),
            pyreal_horizon::options::HorizonOptions {
                screen: pyreal_horizon::options::StartScreen::Game,
                ..options
            },
            None,
        );
        ui.start(&mut cx);
        self.shell.horizon.ui = Some(ui);
        self.shell.enter_horizon(&mut cx);
        self.frame();
    }

    fn frame(&mut self) {
        assert!(self.app.frame(&mut self.shell));
    }

    fn finish(self) {
        let Self { app, mut shell } = self;
        app.shutdown(&mut shell);
    }
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_interface_draws_its_hud_through_the_shared_overlay() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-draw-{}", std::process::id()));
    // With no server, the HUD over a sample character.
    let mut c = Client::new(&state);
    for _ in 0..3 {
        c.frame();
    }
    let horizon = c.shell.horizon.ui.as_ref().expect("the Horizon interface");
    assert!(
        !horizon.overlay_items().is_empty(),
        "the Horizon interface composes an overlay"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_interface_comes_up_as_the_client_starts_on_its_own_art() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-own-art-{}", std::process::id()));
    let mut c = Client::before_the_world(&state);
    let ui = c.shell.horizon.ui.as_ref().expect("brought up");
    assert!(
        ui.ui.art.has_piece("window.tl"),
        "its own pieces are behind it"
    );
    for _ in 0..3 {
        c.frame();
    }
    assert!(c.shell.horizon.ui.is_some(), "it stays up");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

/// The version the Horizon interface's pre-game screens show is the one the host names the
/// client by, as `@version` prints it.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_interface_reads_the_client_s_version_the_host_names() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-version-{}", std::process::id()));
    let mut c = Client::before_the_world(&state);
    c.app.interaction.client_build_id = "pyreal-client 9.8.7";
    let read = pyreal_horizon::state::snapshot(&c.app.ui_context(), Vec::new());
    assert_eq!(read.client_version, "9.8.7");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_line_the_horizon_interface_sends_reaches_the_game_and_its_answer_reaches_the_chat() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-line-{}", std::process::id()));
    let mut c = Client::new(&state);
    let ui = c.shell.horizon.ui.as_mut().unwrap();
    ui.outbox.emit(pyreal_client_contract::UiRequest::ChatLine {
        text: "@version".into(),
        window: 0,
    });
    // Handed on between frames: with no server the game screen's chat is the sample player's.
    ui.hand_on_requests(&mut c.app.ui_context(), pyreal_primitives::LocalTime(0.0));
    assert!(ui.outbox.take().is_empty(), "every request was handed on");
    assert!(
        c.app
            .objects
            .world
            .scroll
            .pending()
            .iter()
            .any(|l| l.body.contains("Client version")),
        "the game answered the line in the chat"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_spellbook_filter_the_horizon_interface_sets_is_the_game_s_filter() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-filter-{}", std::process::id()));
    let mut c = Client::new(&state);
    let mask = pyreal_client_contract::spellbook::DEFAULT_SPELL_FILTERS
        & !pyreal_client_contract::spellbook::level_mask(1);
    c.shell
        .horizon
        .ui
        .as_mut()
        .unwrap()
        .outbox
        .emit(pyreal_client_contract::UiRequest::SetSpellbookFilter { mask });
    for _ in 0..3 {
        c.frame();
    }
    assert_eq!(c.app.objects.world.player_system.spell_filters, mask);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

/// A key `vk` (scan code `scan`) pressed and released over two frames, typing `text` if any.
fn tap(c: &mut Client, vk: usize, scan: u16, text: Option<&str>) {
    use pyreal_input::keys::Key;
    c.shell.queue_window_event(HostEvent::KeyboardInput {
        key: Key::new(vk, scan),
        pressed: true,
        text: text.map(str::to_owned),
    });
    c.frame();
    c.shell.queue_window_event(HostEvent::KeyboardInput {
        key: Key::new(vk, scan),
        pressed: false,
        text: None,
    });
    c.frame();
}

/// How many actions the device input has handed the game while W is held for a few frames.
fn actions_while_walking(c: &mut Client) -> u64 {
    use pyreal_input::keys::Key;
    let before = c.app.actions.stats().submitted;
    c.shell.queue_window_event(HostEvent::KeyboardInput {
        key: Key::new(0x57, 0x11),
        pressed: true,
        text: Some("w".into()),
    });
    for _ in 0..5 {
        c.frame();
    }
    c.shell.queue_window_event(HostEvent::KeyboardInput {
        key: Key::new(0x57, 0x11),
        pressed: false,
        text: None,
    });
    for _ in 0..2 {
        c.frame();
    }
    c.app.actions.stats().submitted - before
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn after_a_line_is_typed_and_sent_in_the_horizon_chat_the_movement_keys_reach_the_game_again() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-walk-{}", std::process::id()));
    let mut c = Client::new(&state);
    for _ in 0..2 {
        c.frame();
    }
    let walking = actions_while_walking(&mut c);
    assert!(walking > 0, "W walks before the chat is touched");
    // Enter opens the line, a letter is typed, Enter sends it.
    tap(&mut c, 0x0D, 0x1C, None);
    tap(&mut c, 0x48, 0x23, Some("h"));
    tap(&mut c, 0x49, 0x17, Some("i"));
    tap(&mut c, 0x0D, 0x1C, None);
    for _ in 0..2 {
        c.frame();
    }
    assert!(
        !c.shell.horizon.ui.as_ref().unwrap().ui.text_focus,
        "the chat line gave the keyboard back"
    );
    let after = actions_while_walking(&mut c);
    assert!(after > 0, "W walks after a line was sent");
    // The line opened and typed in, then a click in the world: the keyboard goes back.
    tap(&mut c, 0x0D, 0x1C, None);
    tap(&mut c, 0x48, 0x23, Some("h"));
    assert!(
        c.shell.horizon.ui.as_ref().unwrap().ui.text_focus,
        "the line has the keyboard"
    );
    c.shell
        .queue_window_event(HostEvent::CursorMoved { x: 400.0, y: 250.0 });
    c.frame();
    for pressed in [true, false] {
        c.shell.queue_window_event(HostEvent::MouseInput {
            button: crate::platform::keys::MouseButton::Left,
            pressed,
        });
        c.frame();
    }
    c.frame();
    assert!(
        !c.shell.horizon.ui.as_ref().unwrap().ui.text_focus,
        "a click in the world let the keyboard go"
    );
    assert!(
        actions_while_walking(&mut c) > 0,
        "W walks after a click away"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_game_screen_takes_the_game_s_keys() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-keys-{}", std::process::id()));
    let mut c = Client::new(&state);
    for _ in 0..2 {
        c.frame();
    }
    // I, the Horizon layout's inventory key.
    tap(&mut c, 0x49, 0x17, None);
    assert!(
        c.shell
            .horizon
            .ui
            .as_ref()
            .unwrap()
            .ui
            .windows
            .is_open(pyreal_horizon::ui::panels::WindowId::Inventory),
        "the inventory key opens the inventory"
    );
    assert!(actions_while_walking(&mut c) > 0, "W walks");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_power_bar_fills_as_an_attack_builds_and_empties_when_it_ends() {
    use pyreal_client_model::combat::PowerBarMode;
    let state = std::env::temp_dir().join(format!("pyreal-horizon-power-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.frame();
    let combat = &mut c.app.objects.world.combat;
    combat.begin_power_bar(PowerBarMode::Combat, true, 0);
    combat.set_power_bar_level(0.4);
    c.frame();
    let power = || c.shell.horizon.ui.as_ref().unwrap().power();
    assert_eq!(power(), Some(0.4), "the attack's level fills the bar");
    c.app.objects.world.combat.hide_power_bar();
    c.frame();
    assert_eq!(
        c.shell.horizon.ui.as_ref().unwrap().power(),
        None,
        "the attack's end empties it"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_bare_channel_command_sent_from_the_horizon_chat_switches_the_talk_focus_and_it_stays() {
    use pyreal_client_model::chat::TalkFocus;
    let state = std::env::temp_dir().join(format!("pyreal-horizon-cg-{}", std::process::id()));
    let mut c = Client::new(&state);
    // The player is in the General channel.
    c.app
        .objects
        .world
        .chat
        .set_talk_focus_enabled(TalkFocus::General, true);
    for _ in 0..2 {
        c.frame();
    }
    let type_cg = |c: &mut Client, space: bool| {
        tap(c, 0x0D, 0x1C, None);
        tap(c, 0xBF, 0x35, Some("/"));
        tap(c, 0x43, 0x2E, Some("c"));
        tap(c, 0x47, 0x22, Some("g"));
        if space {
            tap(c, 0x20, 0x39, Some(" "));
        }
        tap(c, 0x0D, 0x1C, None);
        for _ in 0..3 {
            c.frame();
        }
    };
    for space in [true, false] {
        c.app.objects.world.chat.talk_focus = TalkFocus::All;
        type_cg(&mut c, space);
        assert_eq!(
            c.app.objects.world.chat.talk_focus,
            TalkFocus::General,
            "/cg{} and Enter",
            if space { " " } else { "" }
        );
        assert_eq!(
            c.app
                .objects
                .world
                .chat
                .entries
                .get(&pyreal_client_contract::chat::interface::window::MAIN)
                .map(|e| e.text.as_str()),
            Some(""),
            "the command is not left in the line"
        );
    }
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_screens_before_the_world_keep_the_player_s_size() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-size-{}", std::process::id()));
    let mut c = Client::before_the_world(&state);
    for _ in 0..2 {
        c.frame();
    }
    assert!(
        !c.app.uses_forced_resolution(),
        "the Horizon character list runs at the player's size"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn an_item_the_horizon_interface_drops_on_the_world_is_picked_for_where_it_was_let_go() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-drop-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.frame();
    let horizon = c.shell.horizon.ui.as_mut().unwrap();
    horizon.frame_input.mouse = (412.0, 233.0);
    horizon
        .outbox
        .emit(pyreal_client_contract::UiRequest::DragDrop {
            item: pyreal_primitives::ObjectId(0x8000_0042),
            target: pyreal_client_contract::view::DropTarget::World,
        });
    c.frame();
    assert_eq!(c.app.interaction.cursor(), (412, 233));
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn with_nothing_selected_horizon_lays_no_ring_on_the_ground() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-ring-{}", std::process::id()));
    let mut c = Client::new(&state);
    let marker = |c: &Client| {
        c.app
            .present
            .as_any()
            .downcast_ref::<crate::present::NullPresentation>()
            .expect("the headless presentation")
            .ground_markers()
            .first()
            .copied()
    };
    for _ in 0..2 {
        c.frame();
    }
    assert_eq!(
        marker(&c),
        None,
        "the Horizon game screen with no selection"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

/// Whether `scan` (a key's scan code, unmodified) does `name` in the input map the key pages list
/// it in, in the key map in use.
fn key_does(c: &Client, scan: u16, name: &str) -> bool {
    let input = c.shell.shared.input.as_ref().expect("input");
    let action = pyreal_client_contract::actions::names::action_for_enum_name(name).expect(name);
    let map = pyreal_input::presentation::rows_of(action)
        .next()
        .expect("a row")
        .input_map();
    let key = pyreal_input::scheme::keyboard_key(&input.manager.keymap, scan, 0).expect("a key");
    input
        .keys_for_action(action, map)
        .iter()
        .any(|k| k.is_exactly_equal(&key))
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn in_horizon_a_press_is_the_whole_attack() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-attacks-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.frame();
    assert!(c.app.press_attacks);
    // And every attack repeats, the player's own option left as it is.
    let combat = &c.app.objects.world.combat;
    assert!(combat.always_repeat);
    assert!(combat.advanced_combat_refused);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn under_horizon_escape_is_still_the_games_own_escape_for_what_the_interface_hands_on() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-escape-{}", std::process::id()));
    // Escape, unmodified, in the game's interface map: the game's own Escape.
    let escapes = |c: &Client| {
        let input = c.shell.shared.input.as_ref().expect("input");
        let key = pyreal_input::scheme::keyboard_key(&input.manager.keymap, 0x01, 0)
            .expect("the Escape key");
        input
            .manager
            .keymap
            .section(pyreal_input::maps::UI)
            .is_some_and(|s| {
                s.bindings().iter().any(|(k, a)| {
                    k.is_exactly_equal(&key)
                        && *a == pyreal_client_contract::actions::mapped::ESCAPE_KEY
                })
            })
    };
    let mut c = Client::new(&state);
    c.frame();
    assert!(
        escapes(&c),
        "Escape stops an attack charging or repeating, which the interface hands on"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_interface_plays_with_its_own_keys_and_camera() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-keys2-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.frame();
    // The Horizon layout: Q and E step sideways, R locks the run, I is the inventory, V the front view.
    assert!(key_does(&c, 0x10, "MovementStrafeLeft"));
    assert!(key_does(&c, 0x12, "MovementStrafeRight"));
    assert!(key_does(&c, 0x13, "MovementRunLock"));
    assert!(key_does(&c, 0x17, "ToggleInventoryPanel"));
    assert!(key_does(&c, 0x2F, "LookAtFront"));
    assert!(!key_does(&c, 0x10, "MovementRunLock"));
    assert!(c.app.orbit.is_some(), "the interface's own camera");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn under_camera_based_movement_back_runs_forward_and_under_character_based_it_backs_up() {
    use pyreal_client_contract::actions::{movement as a, Action};
    let state = std::env::temp_dir().join(format!("pyreal-horizon-back-{}", std::process::id()));
    let mut c = Client::new(&state);
    // Camera-based: chosen, the keyboard's own default being character-based.
    let movement = |c: &mut Client, m| {
        c.shell
            .horizon
            .ui
            .as_mut()
            .unwrap()
            .ui
            .options
            .orbit
            .movement = m;
    };
    movement(&mut c, pyreal_client_runtime::orbit::MovementMode::Camera);
    c.frame();
    assert_eq!(
        c.app.orbit.map(|o| o.movement),
        Some(pyreal_client_runtime::orbit::MovementMode::Camera)
    );
    c.app.inject_action(Action::begin(a::MOVE_BACKWARD));
    c.frame();
    assert!(c.app.char_input.forward && !c.app.char_input.back);
    assert!(c.app.orbit_keys.held.away(), "faced away from the camera");
    c.app.inject_action(Action::end(a::MOVE_BACKWARD));
    c.frame();
    assert!(!c.app.char_input.forward);
    // Character-based: the game's own.
    movement(
        &mut c,
        pyreal_client_runtime::orbit::MovementMode::Character,
    );
    c.frame();
    c.app.inject_action(Action::begin(a::MOVE_BACKWARD));
    c.frame();
    assert!(c.app.char_input.back && !c.app.char_input.forward);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_slash_key_opens_the_horizon_chat_line_with_one_slash() {
    use pyreal_client_model::chat::TalkFocus;
    let state = std::env::temp_dir().join(format!("pyreal-horizon-slash-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.app
        .objects
        .world
        .chat
        .set_talk_focus_enabled(TalkFocus::General, true);
    for _ in 0..2 {
        c.frame();
    }
    // "/" opens the line; "cg " after it is the General channel's command only with one slash.
    tap(&mut c, 0xBF, 0x35, Some("/"));
    tap(&mut c, 0x43, 0x2E, Some("c"));
    tap(&mut c, 0x47, 0x22, Some("g"));
    tap(&mut c, 0x20, 0x39, Some(" "));
    assert_eq!(c.app.objects.world.chat.talk_focus, TalkFocus::General);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_target_the_horizon_interface_gives_ends_the_use_cursor() {
    use pyreal_client_runtime::interaction::TargetMode;
    let state = std::env::temp_dir().join(format!("pyreal-horizon-target-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.frame();
    let horizon = c.shell.horizon.ui.as_mut().unwrap();
    horizon
        .outbox
        .emit(pyreal_client_contract::UiRequest::SetTargetMode(
            pyreal_client_contract::TargetMode::Use,
        ));
    c.frame();
    assert_eq!(c.app.interaction.target_mode(), TargetMode::Use);
    c.shell.horizon.ui.as_mut().unwrap().outbox.emit(
        pyreal_client_contract::UiRequest::ExecuteTargetItem(pyreal_primitives::ObjectId(
            0x5000_0001,
        )),
    );
    c.frame();
    assert_eq!(c.app.interaction.target_mode(), TargetMode::None);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_interface_keeps_the_game_view_s_clock_running_between_messages() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-clock-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.frame();
    let before = c.app.hud.now.0;
    std::thread::sleep(std::time::Duration::from_millis(30));
    c.frame();
    assert!(
        c.app.hud.now.0 > before,
        "{} then {}",
        before,
        c.app.hud.now.0
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_layout_turns_the_camera_with_the_arrows_and_uses_with_f_and_the_full_stop() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-keys2-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.frame();
    assert!(key_does(&c, 0xCB, "CameraRotateLeft"));
    assert!(key_does(&c, 0xCD, "CameraRotateRight"));
    assert!(key_does(&c, 0xC8, "CameraRotateUp"));
    assert!(key_does(&c, 0xD0, "CameraRotateDown"));
    assert!(key_does(&c, 0x21, "USE"));
    assert!(key_does(&c, 0x34, "USE"));
    // T and F1 to F9 are the interface's own: the game's map gives them nothing.
    let input = c.shell.shared.input.as_ref().expect("input");
    for scan in [0x14, 0x3B, 0x43] {
        let key =
            pyreal_input::scheme::keyboard_key(&input.manager.keymap, scan, 0).expect("a key");
        assert!(
            pyreal_input::scheme::keyboard_bindings(&input.manager.keymap)
                .all(|(_, k, _)| !k.is_exactly_equal(&key)),
            "scan {scan:#x} is unbound"
        );
    }
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn both_mouse_buttons_held_over_the_world_run_forward_in_the_horizon_interface() {
    use crate::platform::keys::MouseButton;
    let state = std::env::temp_dir().join(format!("pyreal-horizon-steer-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.frame();
    c.shell
        .queue_window_event(HostEvent::CursorMoved { x: 400.0, y: 250.0 });
    c.frame();
    let press = |c: &mut Client, button, pressed| {
        c.shell
            .queue_window_event(HostEvent::MouseInput { button, pressed });
        c.frame();
        c.frame();
    };
    press(&mut c, MouseButton::Left, true);
    assert!(
        !c.app.char_input.forward,
        "one button turns the camera only"
    );
    press(&mut c, MouseButton::Right, true);
    assert!(c.app.char_input.forward, "both run");
    assert!(c.app.mouse_look, "and still turn the camera");
    press(&mut c, MouseButton::Left, false);
    assert!(!c.app.char_input.forward, "one let go stops the run");
    assert!(c.app.mouse_look, "the other still turns the camera");
    press(&mut c, MouseButton::Right, false);
    assert!(!c.app.mouse_look);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn under_character_based_movement_the_turning_keys_turn_with_the_left_button_and_step_with_the_right(
) {
    use crate::platform::keys::MouseButton;
    use pyreal_client_contract::actions::{movement as a, Action};
    let state =
        std::env::temp_dir().join(format!("pyreal-horizon-charturn-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.shell
        .horizon
        .ui
        .as_mut()
        .unwrap()
        .ui
        .options
        .orbit
        .movement = pyreal_client_runtime::orbit::MovementMode::Character;
    c.frame();
    c.shell
        .queue_window_event(HostEvent::CursorMoved { x: 400.0, y: 250.0 });
    c.frame();
    let press = |c: &mut Client, button, pressed| {
        c.shell
            .queue_window_event(HostEvent::MouseInput { button, pressed });
        c.frame();
        c.frame();
    };
    // The left button held: the turning key still turns the player.
    press(&mut c, MouseButton::Left, true);
    c.app.inject_action(Action::begin(a::TURN_LEFT));
    c.frame();
    assert!(c.app.char_input.turn_left && !c.app.char_input.step_left);
    press(&mut c, MouseButton::Left, false);
    // The right button steers: the same key held now steps sideways.
    press(&mut c, MouseButton::Right, true);
    assert!(
        c.app.char_input.step_left && !c.app.char_input.turn_left,
        "a step while the mouse steers"
    );
    press(&mut c, MouseButton::Right, false);
    assert!(
        c.app.char_input.turn_left && !c.app.char_input.step_left,
        "a turn again once it is let go"
    );
    c.app.inject_action(Action::end(a::TURN_LEFT));
    c.frame();
    assert!(!c.app.char_input.turn_left && !c.app.char_input.step_left);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_layout_keeps_the_game_s_spell_and_shortcut_keys_and_inherits_none_of_its_others() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-own-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.frame();
    let input = c.shell.shared.input.as_mut().expect("input");
    let defaults = input.horizon_defaults();
    let actions_of = |scan: u16| -> Vec<u32> {
        let Some(key) = pyreal_input::scheme::keyboard_key(&defaults, scan, 0) else {
            return Vec::new();
        };
        pyreal_input::scheme::keyboard_bindings(&defaults)
            .filter(|(_, k, _)| k.is_exactly_equal(&key))
            .map(|(_, _, a)| a.0)
            .collect()
    };
    let named = |name: &str| {
        pyreal_client_contract::actions::names::action_for_enum_name(name)
            .unwrap()
            .0
    };
    // Kept: the spell keys, the shortcut keys.
    assert!(
        actions_of(0xD2).contains(&named("CombatPrevSpellTab")),
        "Insert"
    );
    assert!(
        actions_of(0xC9).contains(&named("CombatNextSpellTab")),
        "Page Up"
    );
    assert!(actions_of(0x02).contains(&named("UseQuickSlot_1")), "1");
    // The left arrow turns the camera, and still moves the cursor in a text box.
    assert!(
        actions_of(0xCB).contains(&named("CameraRotateLeft")),
        "left arrow"
    );
    assert!(
        actions_of(0xCB).contains(&named("CursorCharLeft")),
        "cursor left"
    );
    // Not inherited: F12 (the game's inventory key, the layout's screenshot), Z (the game's
    // sidestep).
    assert_eq!(actions_of(0x58), [named("CaptureScreenshot")], "F12");
    assert!(actions_of(0x2C).is_empty(), "Z: {:?}", actions_of(0x2C));
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_saved_horizon_key_map_loses_the_game_s_keys_it_inherited_and_keeps_the_player_s_own() {
    use crate::input::horizon_scheme::drop_inherited;
    let state = std::env::temp_dir().join(format!("pyreal-horizon-drop-{}", std::process::id()));
    let mut c = Client::new(&state);
    let input = c.shell.shared.input.as_mut().expect("input");
    let shipped_maps = input.manager.shipped_maps.clone().expect("shipped");
    let shipped = [&shipped_maps.0, &shipped_maps.1];
    let template = input.manager.keymap.clone();
    let defaults = input.horizon_defaults();
    let game = pyreal_input::InputManager::load_over_defaults(None, &shipped, None);
    let key = |scan| pyreal_input::scheme::keyboard_key(&template, scan, 0).expect("a key");
    let actions = |map: &pyreal_input::MasterInputMap, scan| -> Vec<u32> {
        pyreal_input::scheme::keyboard_bindings(map)
            .filter(|(_, k, _)| k.is_exactly_equal(&key(scan)))
            .map(|(_, _, a)| a.0)
            .collect()
    };
    // A map saved when the layout still inherited the game's F12, with the player's own Z.
    let mut saved = defaults.clone();
    for (m, k, a) in pyreal_input::scheme::keyboard_bindings(&game)
        .filter(|(_, k, _)| k.is_exactly_equal(&key(0x58)))
        .collect::<Vec<_>>()
    {
        saved.create_input_map(m).add_mapping(k, a);
    }
    let inventory =
        pyreal_client_contract::actions::names::action_for_enum_name("ToggleInventoryPanel")
            .expect("an action");
    saved
        .create_input_map(pyreal_input::InputMapId(0x1000_0009))
        .add_mapping(key(0x2C), inventory);
    // Saved when the layout's arrows took the text box's cursor keys too.
    for section in &mut saved.sections {
        if section.input_map_id == pyreal_input::InputMapId(7) {
            section.unbind_by_key(&key(0xCB));
        }
    }
    let cursor_left =
        pyreal_client_contract::actions::names::action_for_enum_name("CursorCharLeft")
            .expect("an action");
    assert!(!actions(&saved, 0xCB).contains(&cursor_left.0));
    assert!(!actions(&saved, 0x58).is_empty());
    drop_inherited(&mut saved, &shipped, &defaults);
    assert!(
        actions(&saved, 0xCB).contains(&cursor_left.0),
        "the cursor key is back"
    );
    assert!(
        actions(&saved, 0x58).is_empty(),
        "the inherited F12 is gone"
    );
    assert_eq!(actions(&saved, 0x2C), [inventory.0], "the player's Z stays");
    assert_eq!(
        actions(&saved, 0xD2),
        actions(&defaults, 0xD2),
        "a kept key of the game's stays"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn in_the_horizon_interface_ctrl_c_is_not_c() {
    use pyreal_horizon::ui::panels::WindowId;
    use pyreal_input::keys::Key;
    let state = std::env::temp_dir().join(format!("pyreal-horizon-ctrlc-{}", std::process::id()));
    let mut c = Client::new(&state);
    for _ in 0..2 {
        c.frame();
    }
    let open = |c: &Client| {
        c.shell
            .horizon
            .ui
            .as_ref()
            .unwrap()
            .ui
            .windows
            .is_open(WindowId::Character)
    };
    let ctrl = |c: &mut Client, pressed: bool| {
        c.shell.queue_window_event(HostEvent::KeyboardInput {
            key: Key::new(0x11, 0x1D),
            pressed,
            text: None,
        });
        c.frame();
    };
    ctrl(&mut c, true);
    tap(&mut c, 0x43, 0x2E, None);
    ctrl(&mut c, false);
    assert!(!open(&c), "Ctrl+C opens nothing");
    tap(&mut c, 0x43, 0x2E, Some("c"));
    assert!(open(&c), "C opens the character window");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_saved_horizon_key_map_older_than_its_layout_takes_the_layout_s_new_keys_but_keeps_a_rebinding()
{
    use crate::input::horizon_scheme::bring_up_to_date;
    let state = std::env::temp_dir().join(format!("pyreal-horizon-older-{}", std::process::id()));
    let mut c = Client::new(&state);
    let input = c.shell.shared.input.as_mut().expect("input");
    let shipped_maps = input.manager.shipped_maps.clone().expect("shipped");
    let shipped = [&shipped_maps.0, &shipped_maps.1];
    let template = input.manager.keymap.clone();
    let defaults = input.horizon_defaults();
    let game = pyreal_input::InputManager::load_over_defaults(None, &shipped, None);
    let key = |scan| pyreal_input::scheme::keyboard_key(&template, scan, 0).expect("a key");
    let actions = |map: &pyreal_input::MasterInputMap, scan| -> Vec<u32> {
        pyreal_input::scheme::keyboard_bindings(map)
            .filter(|(_, k, _)| k.is_exactly_equal(&key(scan)))
            .map(|(_, _, a)| a.0)
            .collect()
    };
    let rotate_left =
        pyreal_client_contract::actions::names::action_for_enum_name("CameraRotateLeft")
            .expect("an action")
            .0;
    // A map saved before the layout named the arrows: the left arrow as the game binds it, and
    // the player's own choice for Q.
    let mut saved = defaults.clone();
    for scan in [0xCB_u16, 0x10] {
        for section in &mut saved.sections {
            section.unbind_by_key(&key(scan));
        }
    }
    for (m, k, a) in pyreal_input::scheme::keyboard_bindings(&game)
        .filter(|(_, k, _)| k.is_exactly_equal(&key(0xCB)))
        .collect::<Vec<_>>()
    {
        saved.create_input_map(m).add_mapping(k, a);
    }
    let jump = pyreal_client_contract::actions::names::action_for_enum_name("MovementJump")
        .expect("an action");
    saved
        .create_input_map(pyreal_input::InputMapId(0x1000_0001))
        .add_mapping(key(0x10), jump);
    let turn_left =
        pyreal_client_contract::actions::names::action_for_enum_name("MovementTurnLeft")
            .expect("an action")
            .0;
    assert!(
        actions(&saved, 0xCB).contains(&turn_left),
        "the old map: the arrow turns the player too"
    );
    bring_up_to_date(&mut saved, &template, &shipped, &defaults);
    assert_eq!(
        actions(&saved, 0xCB),
        actions(&defaults, 0xCB),
        "the arrow turns the camera, not the player"
    );
    assert!(actions(&saved, 0xCB).contains(&rotate_left));
    assert!(!actions(&saved, 0xCB).contains(&turn_left));
    assert_eq!(
        actions(&saved, 0x10),
        [jump.0],
        "the player's own binding stays"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_wheel_over_the_world_zooms_the_horizon_camera() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-wheel-{}", std::process::id()));
    let mut c = Client::new(&state);
    for _ in 0..2 {
        c.frame();
    }
    // The pointer over the world, clear of every window.
    c.shell
        .queue_window_event(HostEvent::CursorMoved { x: 400.0, y: 250.0 });
    c.frame();
    let before = c.app.actions.stats().submitted;
    c.shell
        .queue_window_event(HostEvent::MouseWheel { notches: 1.0 });
    for _ in 0..2 {
        c.frame();
    }
    // A notch of zoom: the game's zoom begun and let go.
    assert_eq!(c.app.actions.stats().submitted - before, 2);
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn in_the_horizon_layout_x_stops_and_g_examines_and_nothing_else() {
    let state = std::env::temp_dir().join(format!("pyreal-horizon-xg-{}", std::process::id()));
    let mut c = Client::new(&state);
    let input = c.shell.shared.input.as_mut().expect("input");
    let template = input.manager.keymap.clone();
    let defaults = input.horizon_defaults();
    assert_eq!(
        bindings_of(&defaults, &template, 0x2D, 0),
        ["MovementStop"],
        "X"
    );
    assert_eq!(
        bindings_of(&defaults, &template, 0x22, 0),
        ["SelectionExamine"],
        "G"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

/// What the key `scan` held with `meta` does in `map`: the action in each input map it is bound
/// in, by name, in order.
fn bindings_of(
    map: &pyreal_input::MasterInputMap,
    template: &pyreal_input::MasterInputMap,
    scan: u16,
    meta: u32,
) -> Vec<String> {
    let key = pyreal_input::scheme::keyboard_key(template, scan, meta).expect("a key");
    let mut out: Vec<String> = pyreal_input::scheme::keyboard_bindings(map)
        .filter(|(_, k, _)| k.is_exactly_equal(&key))
        .map(|(_, _, a)| pyreal_client_contract::actions::names::enum_name_for_action(a))
        .collect();
    out.sort();
    out
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_layout_binds_the_chat_keys_the_last_slots_the_item_keys_and_the_screenshot() {
    use crate::input::horizon_scheme::{CTRL, SHIFT};
    let state = std::env::temp_dir().join(format!("pyreal-horizon-layout-{}", std::process::id()));
    let mut c = Client::new(&state);
    let input = c.shell.shared.input.as_mut().expect("input");
    let template = input.manager.keymap.clone();
    let defaults = input.horizon_defaults();
    let does = |scan, meta| bindings_of(&defaults, &template, scan, meta);
    // Enter and `/` open the chat line as the game's own actions, so the page shows them; Enter
    // still accepts a text box's line.
    assert!(
        does(0x1C, 0).contains(&"EnterChatMode".to_owned()),
        "{:?}",
        does(0x1C, 0)
    );
    assert!(
        does(0x1C, 0).contains(&"AcceptInput".to_owned()),
        "{:?}",
        does(0x1C, 0)
    );
    assert_eq!(does(0x35, 0), ["START_COMMAND"]);
    // 0, - and =: the tenth to twelfth shortcuts, and the spell bar's in a spell stance.
    for (scan, n) in [(0x0B, 10), (0x0C, 11), (0x0D, 12)] {
        assert_eq!(
            does(scan, 0),
            [format!("UseQuickSlot_{n}"), format!("UseSpellSlot_{n}")],
            "{scan:#x}"
        );
        // With Ctrl, the shortcut's, as Ctrl with 1 to 9 is.
        assert_eq!(
            does(scan, CTRL),
            [format!("UseQuickSlot_{n}")],
            "Ctrl {scan:#x}"
        );
    }
    // The number row with Alt, Shift, Ctrl+Alt and Alt+Shift: the spell bar's second to fifth rows.
    let alt = 0x2000_0000;
    for (i, scan) in (0x02..=0x0D).enumerate() {
        for (meta, first) in [(alt, 13), (SHIFT, 25), (CTRL | alt, 37), (alt | SHIFT, 49)] {
            assert_eq!(
                does(scan, meta),
                [format!("UseSpellSlot_{}", first + i)],
                "{meta:#x} {scan:#x}"
            );
        }
    }
    // The layout's Ctrl is the key map's own: the left Ctrl key's bit.
    let left_ctrl =
        pyreal_input::spec::ControlCode::new(0, pyreal_input::spec::SubControlIndex::None, 0x1D);
    assert_eq!(template.meta_mode_from_key(left_ctrl), CTRL);
    // Alt with 1 to 9 reach no shortcut: not the tenth to twelfth, which 0, - and = are, nor the
    // thirteenth to eighteenth, which keep no key.
    let alt = 0x2000_0000;
    for scan in 0x02..=0x0A {
        assert!(
            !does(scan, alt)
                .iter()
                .any(|a| a.starts_with("UseQuickSlot_1")),
            "Alt {scan:#x}: {:?}",
            does(scan, alt)
        );
    }
    // Backslash, [ and ]: the closest, the previous and the next item.
    assert_eq!(does(0x2B, 0), ["SelectionClosestItem"]);
    assert_eq!(does(0x1A, 0), ["SelectionPreviousItem"]);
    assert_eq!(does(0x1B, 0), ["SelectionNextItem"]);
    // Tab and Shift+Tab: the next and the previous creature.
    assert_eq!(does(0x0F, 0), ["SelectionNextMonster"]);
    assert_eq!(does(0x0F, SHIFT), ["SelectionPreviousMonster"]);
    // Shift+R switches between walking and running.
    assert_eq!(does(0x13, SHIFT), ["MovementWalkMode"]);
    assert_eq!(does(0x58, 0), ["CaptureScreenshot"]);
    // Dropped: the numpad's zoom, Shift+Escape's end of the session.
    assert!(does(0x4A, 0).is_empty(), "numpad -: {:?}", does(0x4A, 0));
    assert!(does(0x4E, 0).is_empty(), "numpad +: {:?}", does(0x4E, 0));
    assert!(
        !does(0x01, 1).contains(&"LOGOUT".to_owned()),
        "Shift+Escape: {:?}",
        does(0x01, 1)
    );
    let nowhere = |name: &str| {
        let action = pyreal_client_contract::actions::names::action_for_enum_name(name).unwrap();
        defaults
            .sections
            .iter()
            .all(|s| s.bindings().iter().all(|(_, a)| *a != action))
    };
    for name in [
        "LOGOUT",
        "CreateShortcut",
        "ToggleFriendsPanel",
        "MovementHoldSidestep",
        "UseQuickSlot_13",
        "UseQuickSlot_14",
        "UseQuickSlot_15",
        "UseQuickSlot_16",
        "UseQuickSlot_17",
        "UseQuickSlot_18",
    ] {
        assert!(nowhere(name), "{name} is bound");
    }
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_wheel_is_the_horizon_camera_s_zoom_in_its_key_map() {
    let state =
        std::env::temp_dir().join(format!("pyreal-horizon-wheelmap-{}", std::process::id()));
    let mut c = Client::new(&state);
    let input = c.shell.shared.input.as_mut().expect("input");
    let defaults = input.horizon_defaults();
    let camera = pyreal_input::InputMapId(5);
    for (name, wheel) in [
        ("CameraMoveToward", 0x0008_0101),
        ("CameraMoveAway", 0x0008_0201),
    ] {
        let action = pyreal_client_contract::actions::names::action_for_enum_name(name).unwrap();
        let keys = defaults
            .section(camera)
            .map(|s| s.keys_for_action(action))
            .unwrap_or_default();
        assert_eq!(keys.len(), 1, "{name}: {keys:?}");
        assert_eq!(
            keys[0].control,
            pyreal_input::spec::ControlCode(wheel),
            "{name}"
        );
    }
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_saved_horizon_key_map_gives_up_the_earlier_layout_s_x_o_and_numpad_zoom_but_keeps_the_player_s(
) {
    use crate::input::horizon_scheme::{bring_up_to_date, drop_inherited};
    let state = std::env::temp_dir().join(format!("pyreal-horizon-retired-{}", std::process::id()));
    let mut c = Client::new(&state);
    let input = c.shell.shared.input.as_mut().expect("input");
    let shipped_maps = input.manager.shipped_maps.clone().expect("shipped");
    let shipped = [&shipped_maps.0, &shipped_maps.1];
    let template = input.manager.keymap.clone();
    let defaults = input.horizon_defaults();
    let game = pyreal_input::InputManager::load_over_defaults(None, &shipped, None);
    let key = |scan| pyreal_input::scheme::keyboard_key(&template, scan, 0).expect("a key");
    let named = |n| pyreal_client_contract::actions::names::action_for_enum_name(n).unwrap();
    let ui = pyreal_input::InputMapId(0x1000_0009);
    let camera = pyreal_input::InputMapId(5);
    // Written by an earlier layout: X examined, O opened the friends list, the numpad zoomed (as
    // the game binds it) and the wheel was not in the map; the player put J on the inventory.
    let mut saved = defaults.clone();
    for scan in [0x2D_u16, 0x18, 0x24] {
        for section in &mut saved.sections {
            section.unbind_by_key(&key(scan));
        }
    }
    saved
        .create_input_map(ui)
        .add_mapping(key(0x2D), named("SelectionExamine"));
    saved
        .create_input_map(ui)
        .add_mapping(key(0x18), named("ToggleFriendsPanel"));
    saved
        .create_input_map(ui)
        .add_mapping(key(0x24), named("ToggleInventoryPanel"));
    for (m, k, a) in pyreal_input::scheme::keyboard_bindings(&game)
        .filter(|(_, k, _)| k.is_exactly_equal(&key(0x4A)))
        .collect::<Vec<_>>()
    {
        saved.create_input_map(m).add_mapping(k, a);
    }
    let wheel = |s: &pyreal_input::MasterInputMap| {
        s.section(camera)
            .map(|s| s.keys_for_action(named("CameraMoveToward")))
            .unwrap_or_default()
            .iter()
            .any(|k| k.control == pyreal_input::spec::ControlCode(0x0008_0101))
    };
    if let Some(s) = saved.sections.iter_mut().find(|s| s.input_map_id == camera) {
        s.retain(|k, _| k.control != pyreal_input::spec::ControlCode(0x0008_0101));
    }
    assert!(!wheel(&saved));
    // Read back as the client reads a saved file: over the layout, then brought up to date.
    let mut loaded = pyreal_input::InputManager::load_over_defaults(
        Some(&saved.to_keymap_text()),
        &[&defaults],
        Some(&input.manager.action_map),
    );
    bring_up_to_date(&mut loaded, &template, &shipped, &defaults);
    drop_inherited(&mut loaded, &shipped, &defaults);
    let does = |scan| bindings_of(&loaded, &template, scan, 0);
    assert_eq!(does(0x2D), ["MovementStop"], "X");
    assert_eq!(does(0x18), ["ToggleSocialPanel"], "O");
    assert!(does(0x4A).is_empty(), "numpad -: {:?}", does(0x4A));
    assert_eq!(does(0x24), ["ToggleInventoryPanel"], "the player's J stays");
    assert!(wheel(&loaded), "the wheel zooms");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn in_the_world_the_horizon_interface_listens_to_every_chat_channel_once_its_options_have_come() {
    use pyreal_client_model::player::options::option;
    let state =
        std::env::temp_dir().join(format!("pyreal-horizon-channels-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.app.host_state.in_world = true;
    let channels = [
        option::HEAR_GENERAL_CHAT,
        option::HEAR_TRADE_CHAT,
        option::HEAR_LFG_CHAT,
        option::HEAR_ROLEPLAY_CHAT,
        option::HEAR_SOCIETY_CHAT,
    ];
    let listening = |c: &Client| channels.map(|o| c.app.objects.world.player_system.options.get(o));
    for o in channels {
        c.app.objects.world.player_system.options.set(o, false);
    }
    for _ in 0..2 {
        c.frame();
    }
    assert_eq!(listening(&c), [false; 5], "the options have not come yet");
    // The player's description: listening to General only.
    c.app.objects.world.player_system.module = Some(Default::default());
    c.app
        .objects
        .world
        .player_system
        .options
        .set(option::HEAR_GENERAL_CHAT, true);
    for _ in 0..2 {
        c.frame();
    }
    assert_eq!(listening(&c), [true; 5]);
    // A channel left stays left while the character is in the world.
    c.app
        .objects
        .world
        .player_system
        .options
        .set(option::HEAR_TRADE_CHAT, false);
    for _ in 0..2 {
        c.frame();
    }
    assert!(!listening(&c)[1], "Trade stays left");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn while_the_hud_is_laid_out_no_drag_wheel_or_key_reaches_the_game_and_escape_closes_the_layout() {
    use crate::platform::keys::MouseButton;
    use pyreal_horizon::ui::panels::WindowId;
    let state = std::env::temp_dir().join(format!("pyreal-horizon-laying-{}", std::process::id()));
    let mut c = Client::new(&state);
    for _ in 0..2 {
        c.frame();
    }
    let laying_out = |c: &Client| {
        c.shell
            .horizon
            .ui
            .as_ref()
            .unwrap()
            .ui
            .windows
            .is_open(WindowId::Layout)
    };
    c.shell
        .horizon
        .ui
        .as_mut()
        .unwrap()
        .ui
        .windows
        .open(WindowId::Layout, 0.0);
    c.frame();
    assert!(laying_out(&c));
    // On the minimap's outline, to drag it.
    let r = c
        .shell
        .horizon
        .ui
        .as_ref()
        .unwrap()
        .ui
        .hud
        .layout
        .outlines
        .iter()
        .find(|(name, _)| *name == "minimap")
        .map(|(_, r)| *r)
        .expect("the minimap's outline");
    let (x, y) = (f64::from(r.x + r.w / 2.0), f64::from(r.y + r.h / 2.0));
    c.shell.queue_window_event(HostEvent::CursorMoved { x, y });
    c.frame();
    let before = c.app.actions.stats().submitted;
    // Both buttons held and dragged: no camera turn, no run.
    for (button, pressed) in [(MouseButton::Left, true), (MouseButton::Right, true)] {
        c.shell
            .queue_window_event(HostEvent::MouseInput { button, pressed });
        c.frame();
    }
    c.shell.queue_window_event(HostEvent::CursorMoved {
        x: x - 60.0,
        y: y + 40.0,
    });
    c.frame();
    assert!(!c.app.mouse_look, "a drag does not turn the camera");
    assert!(!c.app.char_input.forward, "both buttons do not run");
    for button in [MouseButton::Left, MouseButton::Right] {
        c.shell.queue_window_event(HostEvent::MouseInput {
            button,
            pressed: false,
        });
        c.frame();
    }
    // The wheel: no zoom.
    c.shell
        .queue_window_event(HostEvent::MouseWheel { notches: 1.0 });
    c.frame();
    c.frame();
    assert_eq!(
        c.app.actions.stats().submitted - before,
        0,
        "the buttons and the wheel reach nothing of the game"
    );
    assert_eq!(actions_while_walking(&mut c), 0, "W does not walk");
    tap(&mut c, 0x1B, 0x01, None);
    assert!(!laying_out(&c), "Escape closes the layout");
    assert!(actions_while_walking(&mut c) > 0, "W walks again");
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn in_the_horizon_interface_o_opens_the_social_window() {
    use pyreal_horizon::ui::panels::WindowId;
    let state = std::env::temp_dir().join(format!("pyreal-horizon-social-{}", std::process::id()));
    let mut c = Client::new(&state);
    for _ in 0..2 {
        c.frame();
    }
    tap(&mut c, 0x4F, 0x18, Some("o"));
    assert!(c
        .shell
        .horizon
        .ui
        .as_ref()
        .unwrap()
        .ui
        .windows
        .is_open(WindowId::Social));
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_saved_horizon_key_map_loses_the_game_s_alt_keys_for_the_last_shortcuts_gains_ctrl_and_keeps_the_player_s_own(
) {
    use crate::input::horizon_scheme::{CTRL, HORIZON_KEYMAP_FILE};
    let state = std::env::temp_dir().join(format!("pyreal-horizon-alt-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&state);
    let mut c = Client::new(&state);
    let named = |n| pyreal_client_contract::actions::names::action_for_enum_name(n).expect(n);
    let alt = 0x2000_0000;
    let input = c.shell.shared.input.as_mut().expect("input");
    let template = input.manager.keymap.clone();
    let key =
        |scan, meta| pyreal_input::scheme::keyboard_key(&template, scan, meta).expect("a key");
    // A map saved by the earlier layout: Alt+1, Alt+2 and Alt+4 as the game gave them, no
    // Ctrl+0, and Alt+3 the player's own, on the fifth shortcut.
    let mut saved = input.horizon_defaults();
    // The shortcuts' keys' own map, where the layout binds them.
    let quick = saved
        .sections
        .iter()
        .find(|s| {
            s.bindings()
                .iter()
                .any(|(_, a)| *a == named("UseQuickSlot_10"))
        })
        .expect("the shortcuts' map")
        .input_map_id;
    for section in &mut saved.sections {
        section.unbind_by_key(&key(0x0B, CTRL));
    }
    saved
        .create_input_map(quick)
        .add_mapping(key(0x02, alt), named("UseQuickSlot_10"));
    saved
        .create_input_map(quick)
        .add_mapping(key(0x03, alt), named("UseQuickSlot_11"));
    saved
        .create_input_map(quick)
        .add_mapping(key(0x04, alt), named("UseQuickSlot_5"));
    saved
        .create_input_map(quick)
        .add_mapping(key(0x05, alt), named("UseQuickSlot_13"));
    c.finish();
    std::fs::write(state.join(HORIZON_KEYMAP_FILE), saved.to_keymap_text()).expect("written");
    // The client started next reads it.
    let c = Client::new(&state);
    let does = |c: &Client, k: &pyreal_input::ControlChord, n| {
        c.shell
            .shared
            .input
            .as_ref()
            .expect("input")
            .keys_for_action(named(n), quick)
            .iter()
            .any(|x| x.is_exactly_equal(k))
    };
    assert!(!does(&c, &key(0x02, alt), "UseQuickSlot_10"));
    assert!(!does(&c, &key(0x03, alt), "UseQuickSlot_11"));
    assert!(!does(&c, &key(0x05, alt), "UseQuickSlot_13"));
    assert!(does(&c, &key(0x0B, CTRL), "UseQuickSlot_10"));
    assert!(
        does(&c, &key(0x04, alt), "UseQuickSlot_5"),
        "the player's own key"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_saved_horizon_key_map_with_freed_movement_arrows_and_an_earlier_shift_still_turns_the_camera_and_shifts(
) {
    use crate::input::horizon_scheme::{HORIZON_KEYMAP_FILE, SHIFT};
    use pyreal_input::binding::DO_NOTHING;
    let state = std::env::temp_dir().join(format!("pyreal-horizon-mend-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&state);
    let mut c = Client::new(&state);
    let named = |n| pyreal_client_contract::actions::names::action_for_enum_name(n).expect(n);
    let home = |n| {
        pyreal_input::presentation::rows_of(named(n))
            .next()
            .expect("a row")
            .input_map()
    };
    let input = c.shell.shared.input.as_mut().expect("input");
    let template = input.manager.keymap.clone();
    let key =
        |scan, meta| pyreal_input::scheme::keyboard_key(&template, scan, meta).expect("a key");
    // The layout's Shift is the key map's own: the left Shift key's bit.
    let left_shift =
        pyreal_input::spec::ControlCode::new(0, pyreal_input::spec::SubControlIndex::None, 0x2A);
    assert_eq!(template.meta_mode_from_key(left_shift), SHIFT);
    // The left arrow freed in the movement keys, as a Restore Defaults left it, and Shift+Tab
    // and Shift+R given with the bottom bit an earlier layout took for Shift.
    let mut saved = input.horizon_defaults();
    for section in &mut saved.sections {
        for scan in [0x0F, 0x13] {
            section.unbind_by_key(&key(scan, SHIFT));
        }
    }
    saved
        .create_input_map(home("MovementTurnLeft"))
        .add_mapping(key(0xCB, 0), DO_NOTHING);
    saved
        .create_input_map(home("SelectionPreviousMonster"))
        .add_mapping(key(0x0F, 1), named("SelectionPreviousMonster"));
    saved
        .create_input_map(home("MovementWalkMode"))
        .add_mapping(key(0x13, 1), DO_NOTHING);
    saved
        .create_input_map(home("MovementWalkMode"))
        .add_mapping(key(0x13, SHIFT), named("MovementWalkMode"));
    c.finish();
    std::fs::write(state.join(HORIZON_KEYMAP_FILE), saved.to_keymap_text()).expect("written");
    // The client started next reads it.
    let mut c = Client::new(&state);
    let does = |c: &Client, k: &pyreal_input::ControlChord, n| {
        c.shell
            .shared
            .input
            .as_ref()
            .expect("input")
            .keys_for_action(named(n), home(n))
            .iter()
            .any(|x| x.is_exactly_equal(k))
    };
    assert!(does(&c, &key(0x0F, SHIFT), "SelectionPreviousMonster"));
    assert!(does(&c, &key(0x13, SHIFT), "MovementWalkMode"));
    assert!(!does(&c, &key(0x0F, 1), "SelectionPreviousMonster"));
    // The left arrow, held, turns the camera.
    c.shell.queue_window_event(HostEvent::KeyboardInput {
        key: pyreal_input::keys::Key::new(0x25, 0xE04B),
        pressed: true,
        text: None,
    });
    c.frame();
    assert!(c.app.input.look_left, "the left arrow turns the camera");
    c.shell.queue_window_event(HostEvent::KeyboardInput {
        key: pyreal_input::keys::Key::new(0x25, 0xE04B),
        pressed: false,
        text: None,
    });
    c.frame();
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_layout_gives_b_and_l_their_windows_and_ctrl_tab_the_radar_and_a_saved_map_takes_them(
) {
    use crate::input::horizon_scheme::{bring_up_to_date, drop_inherited, CTRL, SHIFT};
    let state = std::env::temp_dir().join(format!("pyreal-horizon-bltab-{}", std::process::id()));
    let mut c = Client::new(&state);
    let input = c.shell.shared.input.as_mut().expect("input");
    let shipped_maps = input.manager.shipped_maps.clone().expect("shipped");
    let shipped = [&shipped_maps.0, &shipped_maps.1];
    let template = input.manager.keymap.clone();
    let defaults = input.horizon_defaults();
    let does =
        |map: &pyreal_input::MasterInputMap, scan, meta| bindings_of(map, &template, scan, meta);
    assert_eq!(does(&defaults, 0x30, 0), ["ToggleInventoryPanel"], "B");
    assert_eq!(does(&defaults, 0x26, 0), ["ToggleJournalPanel"], "L");
    assert_eq!(
        does(&defaults, 0x0F, CTRL),
        ["SelectionNextCompassItem"],
        "Ctrl+Tab"
    );
    assert_eq!(
        does(&defaults, 0x0F, CTRL | SHIFT),
        ["SelectionPreviousCompassItem"],
        "Ctrl+Shift+Tab"
    );
    assert_eq!(does(&defaults, 0x0F, 0), ["SelectionNextMonster"], "Tab");
    assert_eq!(
        does(&defaults, 0x0F, SHIFT),
        ["SelectionPreviousMonster"],
        "Shift+Tab"
    );
    // Saved before the layout gave these keys, with the player's own L on the map.
    let key =
        |scan, meta| pyreal_input::scheme::keyboard_key(&template, scan, meta).expect("a key");
    let mut saved = defaults.clone();
    for (scan, meta) in [(0x30, 0), (0x26, 0), (0x0F, CTRL), (0x0F, CTRL | SHIFT)] {
        for section in &mut saved.sections {
            section.unbind_by_key(&key(scan, meta));
        }
    }
    let map = pyreal_client_contract::actions::names::action_for_enum_name("ToggleMapPanel")
        .expect("an action");
    saved
        .create_input_map(pyreal_input::InputMapId(0x1000_0009))
        .add_mapping(key(0x26, 0), map);
    let mut loaded = pyreal_input::InputManager::load_over_defaults(
        Some(&saved.to_keymap_text()),
        &[&defaults],
        Some(&input.manager.action_map),
    );
    bring_up_to_date(&mut loaded, &template, &shipped, &defaults);
    drop_inherited(&mut loaded, &shipped, &defaults);
    assert_eq!(does(&loaded, 0x30, 0), ["ToggleInventoryPanel"], "B, saved");
    assert_eq!(
        does(&loaded, 0x26, 0),
        ["ToggleMapPanel"],
        "the player's L stays"
    );
    assert_eq!(
        does(&loaded, 0x0F, CTRL),
        ["SelectionNextCompassItem"],
        "Ctrl+Tab, saved"
    );
    assert_eq!(
        does(&loaded, 0x0F, CTRL | SHIFT),
        ["SelectionPreviousCompassItem"],
        "Ctrl+Shift+Tab, saved"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn in_the_horizon_interface_tab_with_each_of_its_modifiers_does_only_its_own_action() {
    use pyreal_input::fire::ControlType;
    let state = std::env::temp_dir().join(format!("pyreal-horizon-tabs-{}", std::process::id()));
    let mut c = Client::new(&state);
    for _ in 0..2 {
        c.frame();
    }
    let input = c.shell.shared.input.as_mut().expect("input");
    let control = |input: &crate::input::InputShell, scan| {
        pyreal_input::scheme::keyboard_key(&input.manager.keymap, scan, 0)
            .expect("a key")
            .control
    };
    let mut time = 1000_u32;
    let mut press = |input: &mut crate::input::InputShell, scans: &[u16]| -> Vec<String> {
        let _ = input.manager.take_events();
        for scan in scans {
            time += 300;
            let cs = control(input, *scan);
            input
                .manager
                .fire_input_event(cs, ControlType::Button, 0x80, time);
        }
        let fired: Vec<String> = input
            .manager
            .take_events()
            .iter()
            .filter(|e| e.start)
            .map(|e| pyreal_client_contract::actions::names::enum_name_for_action(e.action))
            .collect();
        for scan in scans.iter().rev() {
            time += 300;
            let cs = control(input, *scan);
            input
                .manager
                .fire_input_event(cs, ControlType::Button, 0, time);
        }
        let _ = input.manager.take_events();
        fired
    };
    assert_eq!(press(input, &[0x0F]), ["SelectionNextMonster"], "Tab");
    assert_eq!(
        press(input, &[0x2A, 0x0F]),
        ["SelectionPreviousMonster"],
        "Shift+Tab"
    );
    assert_eq!(
        press(input, &[0x1D, 0x0F]),
        ["SelectionNextCompassItem"],
        "Ctrl+Tab"
    );
    assert_eq!(
        press(input, &[0x1D, 0x2A, 0x0F]),
        ["SelectionPreviousCompassItem"],
        "Ctrl+Shift+Tab"
    );
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn in_the_horizon_interface_s_and_a_back_up_and_step_left_through_a_cast_in_either_movement() {
    use pyreal_client_contract::actions::{movement as a, Action};
    use pyreal_client_runtime::orbit::MovementMode;
    let state = std::env::temp_dir().join(format!("pyreal-horizon-slide-{}", std::process::id()));
    let mut c = Client::new(&state);
    c.frame();
    for movement in [MovementMode::Camera, MovementMode::Character] {
        c.shell
            .horizon
            .ui
            .as_mut()
            .unwrap()
            .ui
            .options
            .orbit
            .movement = movement;
        c.frame();
        let input = |c: &Client| {
            let i = c.app.char_input;
            (i.forward, i.back, i.turn_left, i.step_left)
        };
        // Not casting: S and A run the way they point (camera-based) or back up and turn.
        c.app.inject_action(Action::begin(a::MOVE_BACKWARD));
        c.app.inject_action(Action::begin(a::TURN_LEFT));
        c.frame();
        assert!(
            !input(&c).3,
            "{movement:?}: not casting, A does not step: {:?}",
            input(&c)
        );
        c.app.inject_action(Action::end(a::TURN_LEFT));
        c.app.inject_action(Action::end(a::MOVE_BACKWARD));
        c.frame();
        // Casting: they back up and step left, and keep doing so after the cast while held.
        c.app.objects.world.magic.casting = true;
        c.frame();
        c.app.inject_action(Action::begin(a::MOVE_BACKWARD));
        c.app.inject_action(Action::begin(a::TURN_LEFT));
        c.frame();
        assert_eq!(
            input(&c),
            (false, true, false, true),
            "{movement:?}: casting, back and a step left, no run and no turn"
        );
        c.app.objects.world.magic.casting = false;
        c.frame();
        assert_eq!(
            input(&c),
            (false, true, false, true),
            "{movement:?}: the cast over, the keys held go on as they were"
        );
        c.app.inject_action(Action::end(a::TURN_LEFT));
        c.app.inject_action(Action::end(a::MOVE_BACKWARD));
        c.frame();
        assert_eq!(input(&c), (false, false, false, false), "{movement:?}");
        // Casting, S alone backs up.
        c.app.objects.world.magic.casting = true;
        c.frame();
        c.app.inject_action(Action::begin(a::MOVE_BACKWARD));
        c.frame();
        assert_eq!(
            input(&c),
            (false, true, false, false),
            "{movement:?}: S backs up"
        );
        c.app.inject_action(Action::end(a::MOVE_BACKWARD));
        c.app.objects.world.magic.casting = false;
        c.frame();
        c.frame();
    }
    c.finish();
    let _ = std::fs::remove_dir_all(&state);
}

/// A frame hook that notes whether each of its frames saw the player move the character, and asks
/// once for the run lock when told to.
#[derive(Debug, Default)]
struct MovementWatch {
    seen: std::rc::Rc<std::cell::RefCell<Vec<bool>>>,
    run: std::rc::Rc<std::cell::Cell<bool>>,
}

impl pyreal_client_runtime::frame_hook::FrameHook for MovementWatch {
    fn step(&mut self, frame: &mut pyreal_client_runtime::frame_hook::HookFrame<'_>) {
        self.seen.borrow_mut().push(frame.player_input);
        if self.run.take() {
            frame.nav(pyreal_client_runtime::frame_hook::NavRequest::Autorun(true));
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// What a [`MovementWatch`] saw, and its switch for the run lock.
type Watched = (
    std::rc::Rc<std::cell::RefCell<Vec<bool>>>,
    std::rc::Rc<std::cell::Cell<bool>>,
);

impl Client {
    /// The Horizon interface shown on its game screen under `movement`, with a [`MovementWatch`]
    /// installed.
    fn horizon_with_watch(
        &mut self,
        movement: pyreal_client_runtime::orbit::MovementMode,
    ) -> Watched {
        self.shell
            .horizon
            .ui
            .as_mut()
            .unwrap()
            .ui
            .options
            .orbit
            .movement = movement;
        let watch = MovementWatch::default();
        let seen = std::rc::Rc::clone(&watch.seen);
        let run = std::rc::Rc::clone(&watch.run);
        self.app.install_frame_hook(Box::new(watch));
        self.frame();
        self.frame();
        seen.borrow_mut().clear();
        (seen, run)
    }
}

/// Under either of Horizon's movements, a pad's stick and both mouse buttons held are the player's
/// own movement: the frame hook hears of them as it hears of a movement key, and its own run lock
/// is not the player's.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_stick_and_mouse_run_are_the_players_own_movement_to_a_frame_hook() {
    use crate::platform::keys::MouseButton;
    use pyreal_client_runtime::orbit::MovementMode;
    for movement in [MovementMode::Camera, MovementMode::Character] {
        let state = std::env::temp_dir().join(format!(
            "pyreal-horizon-watch-{movement:?}-{}",
            std::process::id()
        ));
        let mut c = Client::new(&state);
        let (seen, run) = c.horizon_with_watch(movement);
        c.app
            .ui_context()
            .orbit_sticks(Some((0.0, 1.0)), (0.0, 0.0));
        c.frame();
        c.frame();
        assert!(
            seen.borrow().iter().any(|s| *s),
            "{movement:?}: the stick pushed is the player's movement: {:?}",
            seen.borrow()
        );
        c.app.ui_context().orbit_sticks(None, (0.0, 0.0));
        c.frame();
        c.frame();
        seen.borrow_mut().clear();
        // The hook's own run lock is the client's steering, not the player's movement.
        run.set(true);
        for _ in 0..4 {
            c.frame();
        }
        assert!(
            c.app.char_input.auto_run,
            "{movement:?}: the run lock is on"
        );
        assert!(
            seen.borrow().iter().all(|s| !*s),
            "{movement:?}: the hook's own run lock is not the player's: {:?}",
            seen.borrow()
        );
        // Both mouse buttons held over the world.
        c.shell
            .queue_window_event(HostEvent::CursorMoved { x: 400.0, y: 250.0 });
        c.frame();
        seen.borrow_mut().clear();
        for button in [MouseButton::Left, MouseButton::Right] {
            c.shell.queue_window_event(HostEvent::MouseInput {
                button,
                pressed: true,
            });
            c.frame();
        }
        c.frame();
        assert!(
            seen.borrow().iter().any(|s| *s),
            "{movement:?}: both buttons held run the player: {:?}",
            seen.borrow()
        );
        c.finish();
        let _ = std::fs::remove_dir_all(&state);
    }
}

/// A journey's steering under Horizon is the game's own movement:
/// its turning key turns the player (camera-based movement would make it a run to the side), its
/// forward key runs the way the player faces, and neither is the player's own movement.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_journeys_steering_under_horizon_is_the_games_own_movement() {
    use pyreal_client_contract::actions::{movement as a, Action};
    use pyreal_client_runtime::orbit::MovementMode;
    for movement in [MovementMode::Camera, MovementMode::Character] {
        let state = std::env::temp_dir().join(format!(
            "pyreal-horizon-steer2-{movement:?}-{}",
            std::process::id()
        ));
        let mut c = Client::new(&state);
        let (seen, _) = c.horizon_with_watch(movement);
        c.app.actions.steer(Action::begin(a::TURN_RIGHT));
        c.frame();
        let input = c.app.char_input;
        assert!(
            input.turn_right && !input.step_right && !input.forward,
            "{movement:?}: a turn, not a run or a step: {input:?}"
        );
        c.app.actions.steer(Action::end(a::TURN_RIGHT));
        c.app.actions.steer(Action::begin(a::MOVE_FORWARD));
        c.frame();
        assert!(c.app.char_input.forward && !c.app.char_input.turn_right);
        assert!(
            !c.app.orbit_keys.held.forward,
            "{movement:?}: not read as the player's key"
        );
        c.app.actions.steer(Action::end(a::MOVE_FORWARD));
        c.frame();
        c.frame();
        assert!(!c.app.char_input.forward);
        assert!(
            seen.borrow().iter().all(|s| !*s),
            "{movement:?}: the journey's keys are not the player's: {:?}",
            seen.borrow()
        );
        c.finish();
        let _ = std::fs::remove_dir_all(&state);
    }
}
