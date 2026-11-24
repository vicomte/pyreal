//! Hide Interface is on Alt+Z by default, which no shipped key map binds. It fires in the game and
//! in explore from this client's own map, but not while a text box has the keyboard.
//! Fixture: the retail dats' input maps, driven through `InputShell` with synthetic key messages.

use pyreal_primitives::LocalTime;

#[test]
fn hide_interface_is_on_alt_z_in_the_game_and_explore_but_not_in_a_text_box() {
    use winit::keyboard::KeyCode;
    let hide = pyreal_client_contract::actions::pyreal::HIDE_INTERFACE;
    let store = pyreal_dat::testing::open_store().expect("required DATs");
    let mut input = pyreal_client_shell::input::InputShell::new(&store, None).unwrap();
    let keys = input.keys_for_action(hide, pyreal_input::pyreal::INPUT_MAP);
    assert_eq!(keys.len(), 1, "one default key: {keys:?}");
    let key = keys[0];
    let z = pyreal_input::scheme::keyboard_key(&input.manager.keymap, 0x2C, 0x2000_0000)
        .expect("a key");
    assert!(key.is_exactly_equal(&z), "Alt+Z: {key:?}");

    // The shipped maps bind Alt+Z to nothing.
    let shipped = input
        .manager
        .shipped_maps
        .as_ref()
        .expect("the shipped maps");
    for map in [&shipped.0, &shipped.1] {
        for s in &map.sections {
            for (qc, a) in s.bindings() {
                assert!(
                    !qc.is_conflicting(&key) || *a == hide,
                    "Alt+Z is bound to {a:?} in {:?}",
                    s.input_map_id
                );
            }
        }
    }

    let mut pump = pyreal_desktop::pump::Pump::new();
    let mut serial = 0;
    let mut press = |input: &mut pyreal_client_shell::input::InputShell, t: f64| {
        let mut message = |input: &mut pyreal_client_shell::input::InputShell, code, down| {
            serial += 1;
            input.on_message(pump.key_message_for(code, down, serial).unwrap());
        };
        input.use_time(LocalTime(t));
        input.take_events();
        message(input, KeyCode::AltLeft, true);
        message(input, KeyCode::KeyZ, true);
        input.use_time(LocalTime(t + 0.001));
        let started = input
            .take_events()
            .iter()
            .filter(|e| e.action == hide && e.start)
            .count();
        message(input, KeyCode::KeyZ, false);
        message(input, KeyCode::AltLeft, false);
        input.use_time(LocalTime(t + 0.002));
        input.take_events();
        started
    };

    input.set_character_session_input_maps(true);
    assert_eq!(press(&mut input, 0.100), 1, "in the game");
    input.set_character_session_input_maps(false);
    input.set_explore_input_maps(true);
    assert_eq!(press(&mut input, 0.200), 1, "in explore");
    input.set_explore_input_maps(false);

    // A text box with the keyboard keeps it.
    input.set_character_session_input_maps(true);
    input.set_focused_input_maps(&pyreal_client_shell::input::FOCUSED_TEXT_MAP_REGISTRATIONS);
    input.set_text_mode(true);
    assert_eq!(press(&mut input, 0.300), 0, "in a text box");
}
