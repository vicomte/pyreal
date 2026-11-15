//! The world in motion: what a moving body reports, what the shard's corrections do to it, what
//! the physics refuses, what the camera slides through, what a click answers, and the radar,
//! door and stance behaviour built on top of them. Fixture: the retail dats (the real land, cells
//! and body setups) and, for the claims about retail's own output, the recorded sessions read
//! through the corpus or replayed. Most scenarios open the dats themselves, drive a body a
//! physics sub-step at a time, and book the claim with `assert_behaviour` on a model client; the
//! rest build a whole client through [`pyreal_testkit::ClientSpec::retail`]. This binary runs
//! serially. `ALL` lists this subject's scenarios. Nothing here binds a socket or sends a datagram.

use pyreal_testkit::{ClientSpec, HeadlessClient};

use support::{
    body_origin, body_position, create_event, create_event_described, create_event_shaped,
    fresh_local_body, give_part_array, position_event, settled_at, store, xy_gap, CREATURE, DT,
    IMPENETRABLE, PK, PK_LITE, PLAYER, STEP,
};

pyreal_testkit::scenarios! {
    scenario_running_reports_walk_with_the_run_hold_key => running_reports_walk_with_the_run_hold_key ["movement.run.reports-walk-with-the-run-hold-key"],
    scenario_enter_world_refuses_a_body_inside_a_building => enter_world_refuses_a_body_inside_a_building ["physics.enter-world.refuses-a-body-inside-a-building"],
    scenario_a_teleport_onto_another_player_is_deflected_only_by_a_creature => a_teleport_onto_another_player_is_deflected_only_by_a_creature ["movement.teleport.arrives-exactly-on-another-player-and-is-pushed-aside-only-by-a-creature"],
    scenario_an_arrival_through_the_login_path_lands_on_the_other_players_spot => an_arrival_through_the_login_path_lands_on_the_other_players_spot ["movement.teleport.a-player-arriving-on-another-player-through-the-shards-own-login-is-not-deflected"],
    scenario_two_player_bodies_collide_only_for_the_pairs_that_are_not_exempt => two_player_bodies_collide_only_for_the_pairs_that_are_not_exempt ["movement.teleport.two-players-pass-through-each-other-only-while-neither-is-marked-for-combat"],
    scenario_an_update_with_no_ground_contact_moves_the_body_nowhere => an_update_with_no_ground_contact_moves_the_body_nowhere ["movement.correction.an-update-that-does-not-say-the-body-is-on-the-ground-moves-it-nowhere"],
    scenario_a_create_onto_an_occupied_spot_is_put_down_beside_it => a_create_onto_an_occupied_spot_is_put_down_beside_it ["movement.remote-body.a-create-onto-an-occupied-spot-is-put-down-beside-it"],
    scenario_a_hook_a_storage_chest_or_a_corpse_is_put_down_exactly_where_sent => a_hook_a_storage_chest_or_a_corpse_is_put_down_exactly_where_sent ["movement.remote-body.a-hook-a-storage-chest-or-a-corpse-is-put-down-exactly-where-sent"],
    scenario_a_re_create_of_a_held_object_moves_it_as_a_position_update_does => a_re_create_of_a_held_object_moves_it_as_a_position_update_does ["movement.remote-body.a-re-create-of-an-object-the-client-holds-moves-it-as-a-position-update-does"],
    scenario_a_nearby_correction_glides_the_body_over_the_following_frames => a_nearby_correction_glides_the_body_over_the_following_frames ["movement.correction.a-nearby-correction-glides-the-body-over-the-following-frames"],
    scenario_a_correction_out_of_reach_is_taken_in_one_step => a_correction_out_of_reach_is_taken_in_one_step ["movement.correction.a-correction-out-of-reach-is-taken-in-one-step-instead-of-a-glide"],
    scenario_a_body_not_yet_simulated_is_put_straight_onto_the_shards_position => a_body_not_yet_simulated_is_put_straight_onto_the_shards_position ["movement.correction.a-body-the-client-has-not-simulated-yet-is-put-straight-onto-the-shards-position"],
    scenario_a_teleport_out_of_the_world_drops_a_glide_in_flight => a_teleport_out_of_the_world_drops_a_glide_in_flight ["movement.correction.a-teleport-out-of-the-world-drops-a-glide-that-was-already-in-flight"],
    scenario_a_destination_in_an_unloaded_block_takes_the_body_out_of_the_world => a_destination_in_an_unloaded_block_takes_the_body_out_of_the_world ["movement.correction.a-destination-in-a-block-the-client-has-not-loaded-takes-the-body-out-of-the-world"],
    scenario_walking_forward_reports_a_walk_with_no_hold_key => walking_forward_reports_a_walk_with_no_hold_key ["movement.walk.reports-walking-forward-with-no-hold-key"],
    scenario_a_running_bodys_report_is_the_retail_clients_own => a_running_bodys_report_is_the_retail_clients_own ["movement.run.the-report-a-running-body-sends-is-the-one-the-retail-client-sent"],
    scenario_turning_run_on_mid_walk_speeds_the_body_and_changes_the_report => turning_run_on_mid_walk_speeds_the_body_and_changes_the_report ["movement.run.turning-run-on-while-already-walking-speeds-the-body-up-and-changes-the-report"],
    scenario_the_camera_slides_through_a_creature_but_not_through_a_solid_twin => the_camera_slides_through_a_creature_but_not_through_a_solid_twin ["camera.view.slides-through-a-creature-standing-between-you-and-the-camera"],
    scenario_a_scene_less_click_reports_nothing_picked => a_scene_less_click_reports_nothing_picked ["world.click.a-click-answered-with-nothing-in-view-reports-nothing-picked"],
    scenario_the_reported_state_is_the_bodys_own => the_reported_state_is_the_bodys_own ["movement.position-report.the-state-the-client-reports-is-the-bodys-own"],
    scenario_a_server_stance_change_is_not_echoed => a_server_stance_change_is_not_echoed ["movement.position-report.a-server-stance-change-is-not-echoed-and-keeps-the-beat"],
    scenario_walking_forward_reports_the_state_the_recordings_carry => walking_forward_reports_the_state_the_recordings_carry ["movement.position-report.walking-forward-reports-the-state-the-recordings-carry"],
    scenario_a_walking_body_reports_where_it_is_and_a_still_one_falls_silent => a_walking_body_reports_where_it_is_and_a_still_one_falls_silent ["movement.position-report.a-walking-body-reports-where-it-is-and-a-still-one-falls-silent"],
    scenario_every_drawn_frame_reaches_the_position_reporter => every_drawn_frame_reaches_the_position_reporter ["movement.position-report.every-drawn-frame-reaches-the-reporter-and-invents-nothing"],
    scenario_a_jump_reports_the_bodys_own_position_and_speed => a_jump_reports_the_bodys_own_position_and_speed ["movement.jump.a-jump-puts-the-bodys-own-position-and-speed-on-the-wire"],
    scenario_a_second_jump_in_mid_air_sends_nothing => a_second_jump_in_mid_air_sends_nothing ["movement.jump.a-second-jump-in-mid-air-is-refused-and-nothing-goes-out"],
    scenario_a_running_jump_carries_its_speed_in_the_bodys_own_frame => a_running_jump_carries_its_speed_in_the_bodys_own_frame ["movement.jump.a-running-jump-carries-its-speed-in-the-bodys-own-frame"],
    scenario_the_clients_jump_height_explains_the_recorded_jumps => the_clients_jump_height_explains_the_recorded_jumps ["movement.jump.the-height-the-client-works-out-explains-the-recorded-jumps"],
    scenario_every_drawn_frame_visits_the_jump_dispatch => every_drawn_frame_visits_the_jump_dispatch ["movement.jump.every-drawn-frame-visits-the-jump-dispatch-and-invents-none"],
    scenario_a_drop_armed_with_nothing_drawn_is_answered => a_drop_armed_with_nothing_drawn_is_answered ["world.scene-less-frame.a-drop-armed-with-nothing-drawn-is-answered-and-the-next-click-still-works"],
    scenario_a_click_with_nothing_drawn_is_answered_too => a_click_with_nothing_drawn_is_answered_too ["world.scene-less-frame.a-click-and-a-double-click-are-answered-so-the-gestures-after-them-still-work"],
    scenario_a_frame_with_nothing_armed_answers_nothing => a_frame_with_nothing_armed_answers_nothing ["world.scene-less-frame.a-frame-with-nothing-armed-answers-nothing-and-clears-nothing"],
    scenario_the_scene_less_answer_names_nothing => the_scene_less_answer_names_nothing ["world.scene-less-frame.the-answer-names-nothing-so-a-dropped-item-takes-the-ground-leg"],
    scenario_a_delayed_script_call_plays_after_its_delay => a_delayed_script_call_plays_after_its_delay ["world.physics-script.a-delayed-call-plays-the-script-it-names-after-the-delay-it-rolled"],
    scenario_an_object_that_leaves_before_the_delay_plays_nothing => an_object_that_leaves_before_the_delay_plays_nothing ["world.physics-script.an-object-that-leaves-its-room-before-the-delay-is-up-plays-nothing"],
    scenario_a_script_call_with_no_delay_plays_on_the_spot => a_script_call_with_no_delay_plays_on_the_spot ["world.physics-script.a-call-with-no-delay-still-plays-on-the-spot"],
    scenario_a_scale_hook_moves_the_collision_radius => a_scale_hook_moves_the_collision_radius ["world.physics-script.a-scale-hook-moves-the-objects-own-collision-radius"],
    scenario_a_body_walking_at_a_scaled_object_stops_further_away => a_body_walking_at_a_scaled_object_stops_further_away ["world.physics-script.a-body-walking-at-a-scaled-object-stops-at-the-scaled-distance"],
    scenario_the_camera_against_a_wall_settles_the_same_at_any_tick_rate => the_camera_against_a_wall_settles_the_same_at_any_tick_rate ["camera.wall.settles-in-the-same-place-however-often-the-client-ticks"],
    scenario_a_key_release_stops_the_body_after_the_shard_has_taken_control => a_key_release_stops_the_body_after_the_shard_has_taken_control ["movement.server-control.a-key-release-stops-the-body-after-the-shard-has-taken-control"],
    scenario_a_turn_release_stops_the_turn_the_same_way => a_turn_release_stops_the_turn_the_same_way ["movement.server-control.a-turn-release-stops-the-turn-the-same-way"],
    scenario_the_player_can_move_again_after_the_shard_teleports_him => the_player_can_move_again_after_the_shard_teleports_him ["movement.teleport.the-player-can-move-again-after-the-shard-teleports-him"],
    scenario_a_press_that_takes_control_back_re_issues_the_key_already_held => a_press_that_takes_control_back_re_issues_the_key_already_held ["movement.server-control.a-press-that-takes-control-back-re-issues-the-key-already-held"],
    scenario_neither_an_approach_nor_a_target_update_leaves_a_motion_behind => neither_an_approach_nor_a_target_update_leaves_a_motion_behind ["movement.approach.neither-an-approach-nor-a-target-update-leaves-a-motion-behind"],
    scenario_a_teleport_with_no_shard_involvement_leaves_a_movable_body => a_teleport_with_no_shard_involvement_leaves_a_movable_body ["movement.teleport.a-teleport-with-no-shard-involvement-leaves-a-body-that-still-moves"],
    scenario_a_shard_authored_swing_holds_the_body_for_its_own_animation => a_shard_authored_swing_holds_the_body_for_its_own_animation ["movement.action.a-shard-authored-swing-holds-every-motion-behind-it-for-its-own-animation"],
    scenario_leaving_the_ground_empties_the_motion_ledger_and_routes_it => leaving_the_ground_empties_the_motion_ledger_and_routes_it ["movement.action.leaving-the-ground-empties-the-motion-ledger-and-routes-what-it-drained"],
    scenario_a_teleport_that_changes_the_ground_frees_the_body => a_teleport_that_changes_the_ground_frees_the_body ["movement.teleport.a-teleport-that-changes-the-ground-empties-the-ledger-and-the-body-moves-again"],
    scenario_a_teleport_onto_the_same_ground_raises_no_edge => a_teleport_onto_the_same_ground_raises_no_edge ["movement.teleport.a-teleport-onto-the-same-kind-of-ground-raises-no-edge-and-empties-nothing"],
    scenario_a_replacement_ends_the_old_approach => a_replacement_ends_the_old_approach ["movement.approach.a-replacement-ends-the-old-approach-before-the-new-one-is-installed"],
    scenario_a_key_press_ends_an_approach_on_the_spot => a_key_press_ends_an_approach_on_the_spot ["movement.approach.a-key-press-ends-an-approach-on-the-spot"],
    scenario_a_jump_ends_an_approach_whether_or_not_it_is_allowed => a_jump_ends_an_approach_whether_or_not_it_is_allowed ["movement.approach.a-jump-ends-an-approach-whether-or-not-the-jump-is-allowed"],
    scenario_the_old_approachs_clean_up_never_outlives_the_next => the_old_approachs_clean_up_never_outlives_the_next ["movement.approach.the-old-approachs-clean-up-never-outlives-the-next-approachs-subject"],
    scenario_every_way_a_follow_ends_reports_it_once => every_way_a_follow_ends_reports_it_once ["movement.follow.every-way-a-follow-ends-reports-it-once-and-leaves-the-body-free"],
    scenario_every_recorded_teleport_is_applied_and_no_other_position_is => every_recorded_teleport_is_applied_and_no_other_position_is ["movement.teleport.every-teleport-the-recordings-carry-is-applied-and-no-other-position-is"],
    scenario_a_recorded_teleport_moves_the_body_and_the_reports_follow => a_recorded_teleport_moves_the_body_and_the_reports_follow ["movement.teleport.a-recorded-teleport-moves-the-body-and-the-reports-name-the-destination"],
    scenario_several_teleports_in_a_row_each_move_the_body => several_teleports_in_a_row_each_move_the_body ["movement.teleport.several-teleports-in-a-row-each-move-the-body"],
    scenario_a_stale_position_leaves_the_tunnel_up => a_stale_position_leaves_the_tunnel_up ["movement.teleport.a-stale-position-leaves-portal-space-up"],
    scenario_a_teleport_into_a_room_lands_the_body_on_its_floor => a_teleport_into_a_room_lands_the_body_on_its_floor ["movement.teleport.a-teleport-into-a-room-lands-the-body-on-its-floor"],
    scenario_every_drawn_frame_applies_a_teleport_before_reporting => every_drawn_frame_applies_a_teleport_before_reporting ["movement.teleport.every-drawn-frame-applies-a-teleport-before-it-reports-a-position"],
    scenario_a_full_ledger_stops_turning_and_walking_alike => a_full_ledger_stops_turning_and_walking_alike ["movement.motion-ledger.gates-turning-and-walking-alike-and-a-fresh-key-press-escapes-it"],
    scenario_a_charged_jump_turns_but_does_not_walk => a_charged_jump_turns_but_does_not_walk ["movement.jump-charge.a-charged-jump-turns-but-does-not-walk-until-the-body-leaves-the-ground"],
    scenario_a_placement_inside_a_building_takes_and_raises_its_footing_change => a_placement_inside_a_building_takes_and_raises_its_footing_change ["movement.placement.a-placement-inside-a-building-takes-and-raises-the-change-of-footing-it-implies"],
    scenario_a_body_that_loses_its_footing_with_no_edge_turns_for_ever => a_body_that_loses_its_footing_with_no_edge_turns_for_ever ["movement.contact.a-body-that-loses-its-footing-with-nothing-to-announce-it-turns-for-ever"],
    scenario_no_door_journey_leaves_a_body_that_turns_but_cannot_walk => no_door_journey_leaves_a_body_that_turns_but_cannot_walk ["movement.door.no-way-of-opening-a-door-leaves-a-body-that-turns-but-cannot-walk"],
    scenario_only_what_the_shard_marks_reaches_the_radar => only_what_the_shard_marks_reaches_the_radar ["radar.filter.only-what-the-shard-marks-for-the-radar-reaches-it"],
    scenario_every_blip_takes_the_colour_its_description_asks_for => every_blip_takes_the_colour_its_description_asks_for ["radar.blips.every-one-takes-the-colour-its-own-description-asks-for"],
    scenario_no_blip_falls_through_to_the_colour_that_means_nothing => no_blip_falls_through_to_the_colour_that_means_nothing ["radar.blips.the-only-things-that-take-no-colour-of-their-own-are-ordinary-players"],
    scenario_there_are_two_radar_ranges_and_one_question => there_are_two_radar_ranges_and_one_question ["radar.range.there-are-two-and-one-question-about-the-players-own-room-decides"],
    scenario_every_recorded_place_takes_the_range_its_room_asks_for => every_recorded_place_takes_the_range_its_room_asks_for ["radar.range.every-place-a-recording-puts-the-player-takes-the-range-its-room-asks-for"],
    scenario_a_dungeon_and_a_building_inside_are_the_same_case => a_dungeon_and_a_building_inside_are_the_same_case ["radar.range.a-dungeon-and-the-inside-of-a-building-are-the-same-case"],
    scenario_the_range_follows_the_player_in_and_out => the_range_follows_the_player_in_and_out ["radar.range.follows-the-player-as-he-steps-inside-and-out-again"],
    scenario_the_chat_sweep_uses_the_same_radius => the_chat_sweep_uses_the_same_radius ["radar.range.the-chat-windows-own-sweep-uses-the-same-radius-as-the-radar"],
    scenario_the_lock_is_one_bit_of_the_players_options => the_lock_is_one_bit_of_the_players_options ["radar.lock.whether-the-interface-is-locked-is-one-bit-of-the-players-own-options"],
}

// -------------------------------------------------------------------------------------------
// movement.teleport.a-player-arriving-on-another-player-through-the-shards-own-login-is-not-deflected
// movement.teleport.two-players-pass-through-each-other-only-while-neither-is-marked-for-combat
// -------------------------------------------------------------------------------------------

/// The measurement and the pairings of player descriptions it is taken over.
///
/// Both halves of the pair arrive the way a connected client gets them -- the login names the
/// player, the player's own create carries his description, the client adopts the shard's id and
/// the object stream pushes that description on to the collision body -- so this is a claim about
/// the whole client and not about the collision walk alone.
mod teleloc;

// -------------------------------------------------------------------------------------------
// movement.walk.reports-walking-forward-with-no-hold-key
// movement.run.the-report-a-running-body-sends-is-the-one-the-retail-client-sent
// movement.run.turning-run-on-while-already-walking-speeds-the-body-up-and-changes-the-report
// -------------------------------------------------------------------------------------------

/// The walk and run reports a live body sends, driven over real terrain; no corpus count is pinned.
mod run_forward;

// -------------------------------------------------------------------------------------------
// The fixtures the body scenarios share
// -------------------------------------------------------------------------------------------

/// What a scenario in this subject needs before it can measure anything: the retail dats, a settled
/// local body over real terrain, and a create or a position update that has really been through the
/// wire.
///
/// It is shared so that the scenarios that need the same four helpers do not each build their own.
pub(super) mod support;

pub(super) use support as world_support;

// ===========================================================================================
// Movement driven through the animation data: sub-steps, cached speeds, approaches, the walk
// bit, doors, stances and the host's effect queue.
//
// These need `pyreal-animation` and `pyreal-assets` as well as the dats. Like the scenarios above
// them, they open the dats themselves, drive the structure directly, and book the claim through
// `assert_behaviour` on a model client.
//
// The `movement.stance.*` claims sit here with the rest of the motion vocabulary although the
// census files their subject under `combat`.
// ===========================================================================================

pub(super) mod motion;

mod bodies;
use bodies::{
    a_body_not_yet_simulated_is_put_straight_onto_the_shards_position,
    a_correction_out_of_reach_is_taken_in_one_step,
    a_create_onto_an_occupied_spot_is_put_down_beside_it,
    a_destination_in_an_unloaded_block_takes_the_body_out_of_the_world,
    a_hook_a_storage_chest_or_a_corpse_is_put_down_exactly_where_sent,
    a_nearby_correction_glides_the_body_over_the_following_frames,
    a_re_create_of_a_held_object_moves_it_as_a_position_update_does,
    a_running_bodys_report_is_the_retail_clients_own,
    a_teleport_onto_another_player_is_deflected_only_by_a_creature,
    a_teleport_out_of_the_world_drops_a_glide_in_flight,
    an_arrival_through_the_login_path_lands_on_the_other_players_spot,
    an_update_with_no_ground_contact_moves_the_body_nowhere,
    enter_world_refuses_a_body_inside_a_building, running_reports_walk_with_the_run_hold_key,
    turning_run_on_mid_walk_speeds_the_body_and_changes_the_report,
    two_player_bodies_collide_only_for_the_pairs_that_are_not_exempt,
    walking_forward_reports_a_walk_with_no_hold_key,
};

mod camera;
use camera::{
    the_camera_against_a_wall_settles_the_same_at_any_tick_rate,
    the_camera_slides_through_a_creature_but_not_through_a_solid_twin,
};

mod picking;
use picking::{
    a_click_with_nothing_drawn_is_answered_too, a_drop_armed_with_nothing_drawn_is_answered,
    a_frame_with_nothing_armed_answers_nothing, a_scene_less_click_reports_nothing_picked,
    the_scene_less_answer_names_nothing,
};

mod reports;
use reports::{
    a_jump_reports_the_bodys_own_position_and_speed,
    a_running_jump_carries_its_speed_in_the_bodys_own_frame,
    a_second_jump_in_mid_air_sends_nothing, a_server_stance_change_is_not_echoed,
    a_walking_body_reports_where_it_is_and_a_still_one_falls_silent,
    every_drawn_frame_reaches_the_position_reporter, every_drawn_frame_visits_the_jump_dispatch,
    the_clients_jump_height_explains_the_recorded_jumps, the_reported_state_is_the_bodys_own,
    walking_forward_reports_the_state_the_recordings_carry,
};

mod scripts;
use scripts::{
    a_body_walking_at_a_scaled_object_stops_further_away,
    a_delayed_script_call_plays_after_its_delay, a_scale_hook_moves_the_collision_radius,
    a_script_call_with_no_delay_plays_on_the_spot,
    an_object_that_leaves_before_the_delay_plays_nothing,
};

mod control;
use control::{
    a_jump_ends_an_approach_whether_or_not_it_is_allowed, a_key_press_ends_an_approach_on_the_spot,
    a_key_release_stops_the_body_after_the_shard_has_taken_control,
    a_press_that_takes_control_back_re_issues_the_key_already_held,
    a_replacement_ends_the_old_approach,
    a_shard_authored_swing_holds_the_body_for_its_own_animation,
    a_teleport_onto_the_same_ground_raises_no_edge,
    a_teleport_that_changes_the_ground_frees_the_body,
    a_teleport_with_no_shard_involvement_leaves_a_movable_body,
    a_turn_release_stops_the_turn_the_same_way, every_way_a_follow_ends_reports_it_once,
    leaving_the_ground_empties_the_motion_ledger_and_routes_it,
    neither_an_approach_nor_a_target_update_leaves_a_motion_behind,
    the_old_approachs_clean_up_never_outlives_the_next,
    the_player_can_move_again_after_the_shard_teleports_him,
};

mod teleports;
use teleports::{
    a_body_that_loses_its_footing_with_no_edge_turns_for_ever,
    a_charged_jump_turns_but_does_not_walk, a_full_ledger_stops_turning_and_walking_alike,
    a_placement_inside_a_building_takes_and_raises_its_footing_change,
    a_recorded_teleport_moves_the_body_and_the_reports_follow,
    a_stale_position_leaves_the_tunnel_up, a_teleport_into_a_room_lands_the_body_on_its_floor,
    every_drawn_frame_applies_a_teleport_before_reporting,
    every_recorded_teleport_is_applied_and_no_other_position_is,
    no_door_journey_leaves_a_body_that_turns_but_cannot_walk,
    several_teleports_in_a_row_each_move_the_body,
};

mod radar;
use radar::{
    a_dungeon_and_a_building_inside_are_the_same_case,
    every_blip_takes_the_colour_its_description_asks_for,
    every_recorded_place_takes_the_range_its_room_asks_for,
    no_blip_falls_through_to_the_colour_that_means_nothing,
    only_what_the_shard_marks_reaches_the_radar, the_chat_sweep_uses_the_same_radius,
    the_lock_is_one_bit_of_the_players_options, the_range_follows_the_player_in_and_out,
    there_are_two_radar_ranges_and_one_question,
};
