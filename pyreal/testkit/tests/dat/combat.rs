//! Combat: readiness, the swing, the stances, the mode toggle and the power bar.
//!
//! Every scenario here opens the retail dats under `$PYREAL_TEST_DAT_DIR`, and **this binary must run
//! serially**: two headless clients in one process share process-wide state.
//!
//! **Two shapes**, the same two `world.rs` uses. A claim about the combat *model* -- the readiness
//! predicates, the charge, the acknowledgement -- drives the client's own frame slot
//! (`pyreal_client_runtime::interaction::use_time`, which is what `App::frame` calls) at clocks the
//! scenario chooses, opens the dats itself for the body and the region, and books the claim through
//! `assert_behaviour` on a model client. A claim about a gesture the player makes with the shipped
//! keymap builds a whole client with [`pyreal_testkit::ClientSpec::gameplay`] and fires real input
//! events through its own input manager.
//!
//! **What a request counts as here.** A scenario reads the client's own per-frame outbox
//! (`Interaction::last_sent`, which the frame replaces every pass), one layer before the bytes.
//! **The cancel that breaks an automatic attack is the exception**: the abort puts its message
//! straight on the flow queue, so it never reaches that outbox, and `pyreal_testkit::wire` reads it
//! off the datagrams. Nothing here binds a socket or sends a datagram: the endpoint only queues.

use pyreal_testkit::{ClientSpec, HeadlessClient};

use support::{a_melee_bench, cancels, key, melee_attacks, settled, store, world_in, Bench};

// ---------------------------------------------------------------------------------------------
// The `#[test]` beside each scenario.
// ---------------------------------------------------------------------------------------------

pyreal_testkit::scenarios! {
    scenario_ready_is_the_bodys_own_motion_queue => ready_is_the_bodys_own_motion_queue ["combat.readiness.is-the-bodys-own-motion-queue-and-not-whether-it-is-on-the-ground"],
    scenario_a_stance_change_the_body_refuses_is_queued => a_stance_change_the_body_refuses_is_queued ["combat.mode.a-change-the-body-is-not-ready-for-is-queued-and-nothing-is-sent"],
    scenario_the_two_flavours_disagree_on_one_body => the_two_flavours_disagree_on_one_body ["combat.readiness.a-swing-and-a-stance-change-ask-different-questions-of-one-body"],
    scenario_a_melee_swing_needs_the_weapons_combat_table => a_melee_swing_needs_the_weapons_combat_table ["combat.readiness.a-melee-swing-needs-the-weapons-own-combat-table"],
    scenario_a_missile_swing_needs_the_stance_and_the_ready_command => a_missile_swing_needs_the_stance_and_the_ready_command ["combat.readiness.a-missile-swing-needs-the-stance-up-and-the-body-standing-ready"],
    scenario_all_three_attack_arms_consume_the_produced_flavour => all_three_attack_arms_consume_the_produced_flavour ["combat.readiness.the-window-the-keyboard-and-the-shards-reply-all-ask-the-body"],
    scenario_one_click_charges_the_bar_and_swings_when_it_fills => one_click_charges_the_bar_and_swings_when_it_fills ["combat.attack.a-click-charges-the-bar-and-the-swing-comes-when-it-fills"],
    scenario_a_held_control_charges_to_full_and_swings_on_release => a_held_control_charges_to_full_and_swings_on_release ["combat.attack.a-held-control-charges-to-full-and-swings-only-on-release"],
    scenario_the_button_and_the_key_start_the_same_charge => the_button_and_the_key_start_the_same_charge ["combat.attack.the-window-button-and-the-key-start-the-same-charge"],
    scenario_a_clean_acknowledgement_rearms_the_loop => a_clean_acknowledgement_rearms_the_loop ["combat.attack-done.a-clean-acknowledgement-rearms-the-loop-and-sends-nothing"],
    scenario_any_other_acknowledgement_cancels_the_repeat => any_other_acknowledgement_cancels_the_repeat ["combat.attack-done.any-other-value-cancels-the-repeat-and-empties-the-bar"],
    scenario_a_moved_slider_refires_at_its_new_power => a_moved_slider_refires_at_its_new_power ["combat.attack-done.a-moved-slider-refires-the-swing-at-its-new-power"],
    scenario_the_acknowledgement_writes_no_line => the_acknowledgement_writes_no_line ["combat.attack-done.the-acknowledgement-writes-no-line-whatever-it-carries"],
    scenario_walking_backwards_breaks_a_repeating_swing => walking_backwards_breaks_a_repeating_swing ["combat.auto-attack.walking-backwards-breaks-a-repeating-swing"],
    scenario_a_release_a_turn_and_an_idle_walk_cancel_nothing => a_release_a_turn_and_an_idle_walk_cancel_nothing ["combat.auto-attack.a-release-a-turn-and-an-idle-walk-cancel-nothing"],
    scenario_a_damage_line_says_what_was_hit_how_hard_and_with_what => a_damage_line_says_what_was_hit_how_hard_and_with_what ["combat.damage-line.says-what-was-hit-how-hard-and-with-what"],
    scenario_the_verb_steps_at_the_thresholds => the_verb_steps_at_the_thresholds ["combat.damage-line.the-verb-steps-at-the-thresholds-and-an-unranked-hit-just-hits"],
    scenario_a_critical_a_sneak_and_a_reckless_swing_each_say_so => a_critical_a_sneak_and_a_reckless_swing_each_say_so ["combat.damage-line.a-critical-a-sneak-and-a-reckless-swing-each-say-so"],
    scenario_both_lines_reach_the_window_and_a_squelch_stops_them => both_lines_reach_the_window_and_a_squelch_stops_them ["combat.damage-line.both-lines-reach-the-window-and-a-squelch-stops-them"],
    scenario_exactly_one_combat_map_is_registered_and_it_follows_the_mode => exactly_one_combat_map_is_registered_and_it_follows_the_mode ["combat.input-map.exactly-one-is-registered-and-it-follows-the-mode"],
    scenario_the_mode_decides_which_map_takes_the_contested_keys => the_mode_decides_which_map_takes_the_contested_keys ["combat.input-map.the-mode-decides-which-map-takes-the-contested-keys"],
    scenario_every_shipped_quickslot_key_reaches_the_toolbar => every_shipped_quickslot_key_reaches_the_toolbar ["combat.input-map.every-shipped-quickslot-key-reaches-the-toolbar-except-the-nine-magic-takes"],
    scenario_the_combat_map_itself_is_always_up_and_never_moves => the_combat_map_itself_is_always_up_and_never_moves ["combat.input-map.the-combat-map-itself-is-always-up-and-never-moves"],
    scenario_a_running_client_swaps_the_keys_the_frame_after_the_mode_changes => a_running_client_swaps_the_keys_the_frame_after_the_mode_changes ["combat.input-map.a-running-client-swaps-the-keys-the-frame-after-the-mode-changes"],
    scenario_the_shards_own_word_opens_the_window_and_the_client_sends_nothing => the_shards_own_word_opens_the_window_and_the_client_sends_nothing ["combat.mode.the-shards-own-word-opens-the-window-and-the-client-sends-nothing"],
    scenario_an_update_that_is_not_this_players_or_is_older_is_refused => an_update_that_is_not_this_players_or_is_older_is_refused ["combat.mode.an-update-that-is-not-this-players-or-is-older-than-the-last-is-refused"],
    scenario_the_recorded_combat_mode_changes_reach_the_window_the_buttons_and_the_keys => the_recorded_combat_mode_changes_reach_the_window_the_buttons_and_the_keys ["combat.mode.the-recorded-changes-reach-the-window-the-buttons-and-the-keys"],
    scenario_the_power_bar_follows_the_clock_at_one_full_charge_a_second => the_power_bar_follows_the_clock_at_one_full_charge_a_second ["combat.power-bar.it-follows-the-clock-at-one-full-charge-a-second"],
    scenario_a_two_handed_style_charges_a_quarter_faster => a_two_handed_style_charges_a_quarter_faster ["combat.power-bar.a-two-handed-style-charges-a-quarter-faster-over-the-same-frames"],
    scenario_the_keyboard_gauge_has_seven_notches_and_starts_half_way => the_keyboard_gauge_has_seven_notches_and_starts_half_way ["combat.power-bar.the-keyboard-gauge-has-seven-notches-and-starts-half-way"],
    scenario_the_power_drag_is_continuous_and_lands_between_the_notches => the_power_drag_is_continuous_and_lands_between_the_notches ["combat.power-bar.the-drag-is-continuous-and-lands-between-the-notches"],
    scenario_the_advanced_option_is_off_at_login_and_the_toggle_is_one_bit => the_advanced_option_is_off_at_login_and_the_toggle_is_one_bit ["combat.advanced.the-option-is-off-at-login-and-the-toggle-is-one-bit-on-the-wire"],
    scenario_the_recorded_attacks_are_charges_after_the_toggle_and_notches_before => the_recorded_attacks_are_charges_after_the_toggle_and_notches_before ["combat.advanced.the-recorded-attacks-are-charges-after-the-toggle-and-notches-before"],
    scenario_one_set_of_keys_means_three_different_things => one_set_of_keys_means_three_different_things ["combat.input-map.one-set-of-keys-means-three-different-things-and-the-mode-picks-which"],
    scenario_every_height_key_moves_the_height_in_both_modes => every_height_key_moves_the_height_in_both_modes ["combat.attack.every-height-key-moves-the-height-in-both-modes-and-the-swing-carries-it"],
    scenario_the_gauge_keys_step_by_a_sixth_in_both_modes => the_gauge_keys_step_by_a_sixth_in_both_modes ["combat.power-bar.the-gauge-keys-step-by-a-sixth-in-both-modes-and-saturate"],
    scenario_a_value_between_notches_rounds_to_the_nearest_before_stepping => a_value_between_notches_rounds_to_the_nearest_before_stepping ["combat.power-bar.a-value-between-notches-rounds-to-the-nearest-before-stepping"],
    scenario_holding_a_height_key_refuses_once_and_not_once_per_repeat => holding_a_height_key_refuses_once_and_not_once_per_repeat ["combat.attack.holding-a-height-key-refuses-once-and-not-once-per-repeat"],
    scenario_at_peace_no_combat_key_reaches_an_arm => at_peace_no_combat_key_reaches_an_arm ["combat.mode.at-peace-no-combat-key-reaches-an-arm"],
}

mod support;

/// The damage-type bits the shard sends.
mod dt;

/// The conditions a swing can carry.
mod cond;

/// The input shell, the registration band, and one key pressed through it.
mod maps;

/// A whole client in the world, and the shard's word on its combat mode.
mod window;

/// The combat system and its charge notices, driven at clocks the scenario chooses.
mod bars;

/// The recorded session the two wire claims read.
mod advanced;

/// One key press into the client's own interaction slot.
pub mod keys;

mod advanced_display;
use advanced_display::{
    the_advanced_option_is_off_at_login_and_the_toggle_is_one_bit,
    the_recorded_attacks_are_charges_after_the_toggle_and_notches_before,
};

mod charge;
use charge::{
    a_clean_acknowledgement_rearms_the_loop, a_held_control_charges_to_full_and_swings_on_release,
    a_moved_slider_refires_at_its_new_power, a_release_a_turn_and_an_idle_walk_cancel_nothing,
    any_other_acknowledgement_cancels_the_repeat,
    one_click_charges_the_bar_and_swings_when_it_fills, the_acknowledgement_writes_no_line,
    the_button_and_the_key_start_the_same_charge, walking_backwards_breaks_a_repeating_swing,
};

mod damage_lines;
use damage_lines::{
    a_critical_a_sneak_and_a_reckless_swing_each_say_so,
    a_damage_line_says_what_was_hit_how_hard_and_with_what,
    both_lines_reach_the_window_and_a_squelch_stops_them, the_verb_steps_at_the_thresholds,
};

mod input_maps;
use input_maps::{
    a_running_client_swaps_the_keys_the_frame_after_the_mode_changes,
    every_shipped_quickslot_key_reaches_the_toolbar,
    exactly_one_combat_map_is_registered_and_it_follows_the_mode,
    the_combat_map_itself_is_always_up_and_never_moves,
    the_mode_decides_which_map_takes_the_contested_keys,
};

mod keyboard_controls;
use keyboard_controls::{
    a_value_between_notches_rounds_to_the_nearest_before_stepping,
    at_peace_no_combat_key_reaches_an_arm, every_height_key_moves_the_height_in_both_modes,
    holding_a_height_key_refuses_once_and_not_once_per_repeat,
    one_set_of_keys_means_three_different_things, the_gauge_keys_step_by_a_sixth_in_both_modes,
};

mod power_displays;
use power_displays::{
    a_two_handed_style_charges_a_quarter_faster,
    the_keyboard_gauge_has_seven_notches_and_starts_half_way,
    the_power_bar_follows_the_clock_at_one_full_charge_a_second,
    the_power_drag_is_continuous_and_lands_between_the_notches,
};

mod readiness;
use readiness::{
    a_melee_swing_needs_the_weapons_combat_table,
    a_missile_swing_needs_the_stance_and_the_ready_command,
    a_stance_change_the_body_refuses_is_queued, all_three_attack_arms_consume_the_produced_flavour,
    ready_is_the_bodys_own_motion_queue, the_two_flavours_disagree_on_one_body,
};

mod window_events;
use window_events::{
    an_update_that_is_not_this_players_or_is_older_is_refused,
    the_recorded_combat_mode_changes_reach_the_window_the_buttons_and_the_keys,
    the_shards_own_word_opens_the_window_and_the_client_sends_nothing,
};
