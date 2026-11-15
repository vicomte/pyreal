//! The performance panel's toggle is on Ctrl+Shift+P by default, which no shipped key map binds. It
//! fires on every screen from this client's own map, which is live for the whole run, but not while
//! a text box has the keyboard, and a player's own binding of the key wins.
//! Fixture: the retail dats' input maps, driven through `InputShell` with synthetic key messages.

use pyreal_primitives::LocalTime;

#[test]
fn the_performance_panel_is_on_ctrl_shift_p_on_every_screen_but_not_in_a_text_box() {
    use winit::keyboard::KeyCode;
    let perf = pyreal_client_contract::actions::pyreal::TOGGLE_PERFORMANCE_PANEL;
    let store = pyreal_dat::testing::open_store().expect("required DATs");
    let mut input = pyreal_client_shell::input::InputShell::new(&store, None).unwrap();
    let keys = input.keys_for_action(perf, pyreal_input::pyreal::INPUT_MAP);
    assert_eq!(keys.len(), 1, "one default key: {keys:?}");
    let key = keys[0];
    let p = pyreal_input::scheme::keyboard_key(&input.manager.keymap, 0x19, 0xC000_0000)
        .expect("a key");
    assert!(key.is_exactly_equal(&p), "Ctrl+Shift+P: {key:?}");

    // The shipped maps bind Ctrl+Shift+P to nothing but this.
    let shipped = input
        .manager
        .shipped_maps
        .as_ref()
        .expect("the shipped maps");
    for map in [&shipped.0, &shipped.1] {
        for s in &map.sections {
            for (qc, a) in s.bindings() {
                assert!(
                    !qc.is_conflicting(&key) || *a == perf,
                    "Ctrl+Shift+P is bound to {a:?} in {:?}",
                    s.input_map_id
                );
            }
        }
    }

    let mut pump = pyreal_desktop::pump::Pump::new();
    let mut serial = 0;
    let mut press = |input: &mut pyreal_client_shell::input::InputShell, t: f64, action| {
        let mut message = |input: &mut pyreal_client_shell::input::InputShell, code, down| {
            serial += 1;
            input.on_message(pump.key_message_for(code, down, serial).unwrap());
        };
        input.use_time(LocalTime(t));
        input.take_events();
        message(input, KeyCode::ControlLeft, true);
        message(input, KeyCode::ShiftLeft, true);
        message(input, KeyCode::KeyP, true);
        input.use_time(LocalTime(t + 0.001));
        let started = input
            .take_events()
            .iter()
            .filter(|e| e.action == action && e.start)
            .count();
        message(input, KeyCode::KeyP, false);
        message(input, KeyCode::ShiftLeft, false);
        message(input, KeyCode::ControlLeft, false);
        input.use_time(LocalTime(t + 0.002));
        input.take_events();
        started
    };

    // The screens before the world, the game, and explore.
    input.set_character_session_input_maps(false);
    assert_eq!(press(&mut input, 0.100, perf), 1, "before the world");
    input.set_character_session_input_maps(true);
    assert_eq!(press(&mut input, 0.200, perf), 1, "in the game");
    input.set_character_session_input_maps(false);
    input.set_explore_input_maps(true);
    assert_eq!(press(&mut input, 0.300, perf), 1, "in explore");

    // A text box with the keyboard keeps it.
    input.set_focused_input_maps(&pyreal_client_shell::input::FOCUSED_TEXT_MAP_REGISTRATIONS);
    input.set_text_mode(true);
    assert_eq!(press(&mut input, 0.400, perf), 0, "in a text box");
    input.set_focused_input_maps(&[]);
    input.set_text_mode(false);
    input.set_explore_input_maps(false);

    // The player gives the key to the map panel: it opens the map, and the panel stays.
    let map_panel = pyreal_client_contract::actions::names::action_for_enum_name("ToggleMapPanel")
        .expect("the map panel's action");
    input.set_character_session_input_maps(true);
    assert!(input.set_binding(pyreal_input::maps::UI, map_panel, None, key));
    assert_eq!(press(&mut input, 0.500, map_panel), 1, "the player's key");
    assert_eq!(press(&mut input, 0.600, perf), 0, "not the panel's");
}
