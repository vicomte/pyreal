//! The DAT shell scenarios: the key maps, the input manager, the window and the settings that
//! survive a relog, driven through physical input against a whole client. Subject modules own
//! their scenarios and fixture helpers. Run this binary serially.

mod bindings;
mod dialog_keys;
mod input;
mod login;
mod options;
mod relog;
mod relogin;
mod text;
mod viewport;

use bindings::each_of_this_clients_actions_works_on_a_key_a_saved_key_map_gives_it;
use dialog_keys::the_dialog_map_binds_only_escape_and_enter;
use input::{
    a_cleared_target_comes_back, a_modified_number_uses_a_quick_slot_and_not_the_chat_window,
    a_rebind_is_written_beside_the_preferences, a_rebound_key_walks_the_body_and_the_old_one_stops,
    an_outsider_starts_at_one_end_and_no_fellowship_moves_nothing,
    every_function_key_does_what_the_shipped_map_says, key,
    the_chat_map_is_walked_first_and_movement_before_the_camera,
    the_desktops_four_are_taken_last_and_do_nothing, the_fellow_cycle_follows_the_panels_order,
    the_key_that_goes_back_is_a_toggle_and_not_a_stack,
    the_left_button_and_the_wheel_each_belong_to_one_shipped_map,
    the_two_keys_walk_the_fellowship_both_ways_and_wrap,
    the_walk_mode_key_follows_the_run_by_default_option,
    the_walk_mode_option_is_read_on_every_press,
    the_window_keeps_the_switcher_and_passes_the_other_two,
    with_nothing_behind_it_the_key_selects_nothing,
};
use login::{
    every_character_set_the_session_decodes_is_an_arrival,
    the_shards_log_off_answer_does_not_end_a_client_with_screens,
};
use options::{
    a_deferred_change_is_flushed_by_the_frame,
    the_settings_it_sends_back_are_byte_for_byte_the_recorded_ones,
    with_no_description_nothing_is_sent,
};
use relog::{
    a_session_that_changed_nothing_deferred_saves_nothing_at_the_log_off,
    every_deferred_change_is_saved_at_the_log_off_and_found_by_the_next_session,
    only_the_shortcut_and_the_auto_saved_option_reach_the_shard_at_the_gesture,
};
use text::every_composition_message_is_handed_to_the_desktop;
use viewport::a_press_in_the_middle_of_a_moved_view_finds_what_is_ahead_of_the_eye;

use pyreal_desktop::platform::window::key_from_key_code;
use pyreal_testkit::adapters_shell::{
    build_app, scratch_preferences, AppSpec, BareKeyboard, Hands,
};
use pyreal_testkit::{ClientSpec, HeadlessClient};
use winit::keyboard::KeyCode;

pyreal_testkit::scenarios! {
    scenario_every_function_key_does_what_the_shipped_map_says => every_function_key_does_what_the_shipped_map_says ["keymap.function-keys.each-one-does-what-the-shipped-map-binds-it-to"],
    scenario_a_rebound_key_walks_the_body_and_the_old_one_stops => a_rebound_key_walks_the_body_and_the_old_one_stops ["keymap.rebind.a-rebound-key-moves-the-body-and-the-old-one-stops"],
    scenario_a_rebind_is_written_beside_the_preferences => a_rebind_is_written_beside_the_preferences ["keymap.rebind.is-written-beside-the-preferences-on-a-clean-exit"],
    scenario_the_desktops_four_are_taken_last_and_do_nothing => the_desktops_four_are_taken_last_and_do_nothing ["keymap.system-keys.the-four-the-desktop-owns-are-taken-last-and-do-nothing"],
    scenario_the_window_keeps_the_switcher_and_passes_the_other_two => the_window_keeps_the_switcher_and_passes_the_other_two ["window.system-keys.the-desktop-keeps-the-two-it-must-and-the-client-eats-the-third"],
    scenario_the_chat_map_is_walked_first_and_movement_before_the_camera => the_chat_map_is_walked_first_and_movement_before_the_camera ["keymap.walk-order.the-chat-map-is-walked-first-and-movement-before-the-camera"],
    scenario_a_modified_number_uses_a_quick_slot_and_not_the_chat_window => a_modified_number_uses_a_quick_slot_and_not_the_chat_window ["keymap.modified-digits.a-number-with-the-modifier-uses-a-quick-slot-and-not-the-chat-window"],
    scenario_the_walk_mode_key_follows_the_run_by_default_option => the_walk_mode_key_follows_the_run_by_default_option ["movement.walk-mode-key.holding-it-follows-the-run-by-default-option-both-ways"],
    scenario_the_walk_mode_option_is_read_on_every_press => the_walk_mode_option_is_read_on_every_press ["movement.walk-mode-key.the-option-is-read-on-every-press-and-not-at-start-up"],
    scenario_the_two_keys_walk_the_fellowship_both_ways_and_wrap => the_two_keys_walk_the_fellowship_both_ways_and_wrap ["selection.fellow.one-key-walks-the-fellowship-forward-and-the-other-back-both-wrapping"],
    scenario_an_outsider_starts_at_one_end_and_no_fellowship_moves_nothing => an_outsider_starts_at_one_end_and_no_fellowship_moves_nothing ["selection.fellow.a-selection-outside-the-fellowship-starts-at-one-end-and-with-no-fellowship-nothing-moves"],
    scenario_the_fellow_cycle_follows_the_panels_order => the_fellow_cycle_follows_the_panels_order ["selection.fellow.the-cycle-follows-the-order-the-panel-shows"],
    scenario_the_key_that_goes_back_is_a_toggle_and_not_a_stack => the_key_that_goes_back_is_a_toggle_and_not_a_stack ["selection.previous.the-key-goes-back-one-and-is-a-toggle-rather-than-a-stack"],
    scenario_with_nothing_behind_it_the_key_selects_nothing => with_nothing_behind_it_the_key_selects_nothing ["selection.previous.with-nothing-behind-it-the-key-selects-nothing"],
    scenario_a_cleared_target_comes_back => a_cleared_target_comes_back ["selection.previous.a-target-that-was-cleared-comes-back"],
    scenario_the_settings_it_sends_back_are_byte_for_byte_the_recorded_ones => the_settings_it_sends_back_are_byte_for_byte_the_recorded_ones ["options.character-page.the-settings-it-sends-back-are-byte-for-byte-the-ones-a-real-client-sent"],
    scenario_a_deferred_change_is_flushed_by_the_frame => a_deferred_change_is_flushed_by_the_frame ["options.character-page.a-deferred-change-reaches-the-shard-eight-minutes-later-and-once"],
    scenario_with_no_description_nothing_is_sent => with_no_description_nothing_is_sent ["options.character-page.with-no-description-nothing-is-sent-and-nothing-is-invented"],
    scenario_the_dialog_map_binds_only_escape_and_enter => the_dialog_map_binds_only_escape_and_enter ["keymap.dialog-keys.the-shipped-map-binds-only-escape-and-enter-and-each-once"],
    scenario_every_composition_message_is_handed_to_the_desktop => every_composition_message_is_handed_to_the_desktop ["text-entry.ime.every-composition-message-is-handed-to-the-desktop-and-an-ordinary-character-is-not"],
    scenario_the_left_button_and_the_wheel_each_belong_to_one_shipped_map => the_left_button_and_the_wheel_each_belong_to_one_shipped_map ["pointer.the-left-button-and-the-wheel-each-belong-to-one-shipped-map"],
    scenario_a_press_in_the_middle_of_a_moved_view_finds_what_is_ahead_of_the_eye => a_press_in_the_middle_of_a_moved_view_finds_what_is_ahead_of_the_eye ["viewport.a-press-in-the-middle-of-a-moved-view-finds-what-is-ahead-of-the-eye"],
    scenario_the_shards_log_off_answer_does_not_end_a_client_with_screens => the_shards_log_off_answer_does_not_end_a_client_with_screens ["shell-only.log-off.the-shards-answer-does-not-end-a-client-that-has-a-screen-to-go-back-to"],
    scenario_every_character_set_the_session_decodes_is_an_arrival => every_character_set_the_session_decodes_is_an_arrival ["shell-only.character-set.every-arrival-is-a-notice-even-when-the-list-has-not-changed"],
    scenario_only_the_shortcut_and_the_auto_saved_option_reach_the_shard_at_the_gesture => only_the_shortcut_and_the_auto_saved_option_reach_the_shard_at_the_gesture ["relog.state.only-the-shortcut-and-the-auto-saved-option-reach-the-shard-at-the-gesture"],
    scenario_every_deferred_change_is_saved_at_the_log_off_and_found_by_the_next_session => every_deferred_change_is_saved_at_the_log_off_and_found_by_the_next_session ["relog.state.every-deferred-change-is-saved-at-the-log-off-and-found-by-the-next-session"],
    scenario_a_session_that_changed_nothing_deferred_saves_nothing_at_the_log_off => a_session_that_changed_nothing_deferred_saves_nothing_at_the_log_off ["relog.state.a-session-that-changed-nothing-deferred-saves-nothing-at-the-log-off"],
    scenario_each_of_this_clients_actions_works_on_a_key_a_saved_key_map_gives_it => each_of_this_clients_actions_works_on_a_key_a_saved_key_map_gives_it ["keys.own.each-of-this-clients-actions-works-on-a-key-a-saved-key-map-gives-it"],
}

// -------------------------------------------------------------------------------------------
// Harness self-proofs. **Not census rows**: they make no claim about the client, they prove that
// one of the harness's `ClientSpec` options and one input step do what they say. `goldens.rs` is
// the precedent for a `dat` test that is not a behaviour.
// -------------------------------------------------------------------------------------------

/// `ClientSpec::with_scratch_settings` makes its directory, the client is pointed at it, and
/// dropping the client removes it. A scenario about what the client *wrote* gets a fresh one every
/// time and leaves nothing behind -- and nothing of it is the player's real settings folder.
#[test]
fn a_scratch_settings_directory_is_made_and_removed_with_the_client() {
    let dir;
    {
        let c = HeadlessClient::new(ClientSpec::retail().with_scratch_settings("selftest"));
        dir = c
            .scratch_settings()
            .expect("the spec named one")
            .dir()
            .to_path_buf();
        assert!(
            dir.is_dir(),
            "the settings directory was not made at {}",
            dir.display()
        );
        assert!(
            c.view()
                .expect_app()
                .config()
                .preferences_file
                .starts_with(&dir),
            "the client's preferences file is not under the directory the scenario named"
        );
        c.shutdown();
    }
    assert!(
        !dir.exists(),
        "the settings directory outlived the client at {}",
        dir.display()
    );
}

/// `input_steps::press_use` presses the key the client's **keymap** binds, and the interface's
/// layout binds it to `F`.
///
/// It is the whole of what the step claims: the binding is read out of the client's own keymap,
/// the press is asserted against it, and a wrong key would fail here rather than press something
/// else.
#[test]
fn the_keymap_binds_use_to_f_and_the_step_presses_it() {
    use pyreal_testkit::input_steps;

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let bound = input_steps::bound_scan_code(&mut c, input_steps::USE, input_steps::UI_COMMANDS);
    let f = pyreal_desktop::platform::window::key_from_key_code(winit::keyboard::KeyCode::KeyF)
        .expect("the host names F");
    assert_eq!(
        bound & 0x7F,
        f.scan_code & 0x7F,
        "the interface's layout binds USE to DIK_F; it binds {bound:#06X}"
    );
    input_steps::press_use(&mut c, f);
    c.shutdown();
}
