//! A saved key map's bindings for this client's own actions.

use super::*;
use pyreal_input::ActionId;

// ---------------------------------------------------------------------------------------------
// keys.own.each-of-this-clients-actions-works-on-a-key-a-saved-key-map-gives-it
// ---------------------------------------------------------------------------------------------

fn pref_on(name: &str) -> bool {
    matches!(
        pyreal_client_contract::options::store::inq_value(name),
        Some(pyreal_client_contract::PrefValue::Bool(true))
    )
}

/// This client's own actions the runtime answers, given a free key by the key map the client saved
/// last time, answer that key: the performance panel flips its setting, and hold sidestep is held
/// for as long as its key is.
pub(super) fn each_of_this_clients_actions_works_on_a_key_a_saved_key_map_gives_it() {
    use pyreal_client_contract::actions::pyreal as own;
    // Keys neither the shipped map nor the interface's layout binds, one for each action.
    let free = [
        (own::TOGGLE_PERFORMANCE_PANEL.0, KeyCode::F7),
        (own::MOVEMENT_HOLD_SIDESTEP.0, KeyCode::Numpad9),
    ];
    let dir = std::env::temp_dir().join(format!("dere-scenario-own-keys-{}", std::process::id()));
    let spec = || ClientSpec::gameplay_in_world(6).with_settings_dir(dir.clone());

    // One run gives the keys and saves its key map, the file the client writes on the way out.
    // Its settings directory goes with it, so what it wrote is carried to the next run's.
    let (file, saved) = {
        let mut first = HeadlessClient::new(spec());
        let input = first
            .app_mut()
            .input_manager_mut()
            .expect("the input shell");
        for (action, code) in free {
            // None of these keys is an extended one, so the scan code is the key map's number.
            let chord =
                pyreal_input::pyreal::keyboard_chord(&input.manager.keymap, key(code).scan_code)
                    .expect("a keyboard key");
            assert!(input.set_binding(
                pyreal_input::pyreal::INPUT_MAP,
                ActionId(action),
                None,
                chord
            ));
        }
        // The interface's own key map is the one that holds this client's actions.
        assert!(input.save_horizon_keymap().expect("the key map is written"));
        let file = dir.join(pyreal_client_shell::input::horizon_scheme::HORIZON_KEYMAP_FILE);
        let saved = std::fs::read(&file).expect("the saved key map");
        first.shutdown();
        (file, saved)
    };

    // The next run reads it back.
    std::fs::create_dir_all(&dir).expect("the settings directory");
    std::fs::write(&file, saved).expect("the saved key map is in place");
    let mut c = HeadlessClient::new(spec());
    let mut hands = Hands::new();
    let bound: Vec<(u32, usize)> = free
        .iter()
        .map(|(action, _)| {
            let keys = c
                .app_mut()
                .input_manager_mut()
                .expect("the input shell")
                .keys_for_action(ActionId(*action), pyreal_input::pyreal::INPUT_MAP);
            (*action, keys.len())
        })
        .collect();
    let all_bound = bound.iter().all(|(_, n)| *n == 1);

    let flips = |c: &mut HeadlessClient, hands: &mut Hands, code, name: &str| {
        let was = pref_on(name);
        hands.tap(c, key(code));
        c.tick(3);
        pref_on(name) != was
    };
    let perf = flips(
        &mut c,
        &mut hands,
        KeyCode::F7,
        pyreal_client_contract::options::performance::PERFORMANCE_PANEL,
    );
    hands.key(&mut c, key(KeyCode::Numpad9), true);
    c.tick(3);
    let held = c.app_mut().movement.lists.hold_sidestep;
    hands.key(&mut c, key(KeyCode::Numpad9), false);
    c.tick(3);
    let let_go = !c.app_mut().movement.lists.hold_sidestep;

    c.assert_behaviour(
        "keys.own.each-of-this-clients-actions-works-on-a-key-a-saved-key-map-gives-it",
        move |_| all_bound && perf && held && let_go,
    );
    c.shutdown();
}
