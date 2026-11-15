//! Inventory and use: what the client sends and holds when an item is picked up, given,
//! moved, equipped, bought or used, and what the model keeps after the shard's own answers.
//!
//! Fixture: the retail dats under `$PYREAL_TEST_DAT_DIR` read by headless clients, with recordings
//! replayed through `Inbound` where a claim needs the shard's own answers. **This binary must run
//! serially.** The dats are needed even where nothing reads a table: an attachment is validated
//! against the holder's part array, which comes from the shipped data.

use pyreal_primitives::ObjectId;
use pyreal_protocol::types::PublicWeenieDesc;
use pyreal_testkit::{ClientSpec, HeadlessClient, Inbound};

pyreal_testkit::scenarios! {
    scenario_a_stowed_item_leaves_the_hand_and_the_world => a_stowed_item_leaves_the_hand_and_the_world ["inventory.pickup.an-item-the-shard-says-was-stowed-leaves-the-hand-and-the-world"],
    scenario_a_stale_or_unknown_stow_changes_nothing => a_stale_or_unknown_stow_changes_nothing ["inventory.pickup.a-stale-or-unknown-stow-changes-nothing"],
    scenario_a_holders_own_list_decides_what_it_holds => a_holders_own_list_decides_what_it_holds ["inventory.held-item.a-holders-own-list-decides-what-hangs-off-it"],
    scenario_a_world_drop_is_refused_in_mid_air => a_world_drop_is_refused_in_mid_air ["inventory.world-drop.a-player-in-mid-air-is-refused-and-one-on-the-ground-is-not"],
    scenario_a_give_in_mid_air_is_not_refused => a_give_in_mid_air_is_not_refused ["inventory.give.handing-something-over-in-mid-air-is-not-refused"],
    scenario_the_ground_answer_is_read_again_every_frame => the_ground_answer_is_read_again_every_frame ["inventory.world-drop.whether-the-player-is-on-the-ground-is-read-again-every-frame"],
    scenario_the_purse_is_filled_from_the_players_own_coin => the_purse_is_filled_from_the_players_own_coin ["vendor.purse.is-filled-from-the-players-own-coin-when-the-shop-opens"],
    scenario_every_recorded_give_is_answered_by_the_move_and_nothing_else => every_recorded_give_is_answered_by_the_move_and_nothing_else ["inventory.give.every-recorded-one-is-answered-by-the-move-and-by-nothing-else"],
    scenario_a_give_takes_the_hold_and_the_next_gesture_is_refused_in_words => a_give_takes_the_hold_and_the_next_gesture_is_refused_in_words ["inventory.give.the-hold-it-takes-refuses-the-next-gesture-in-the-clients-own-words"],
    scenario_the_shards_move_releases_the_give_and_the_next_one_goes_out => the_shards_move_releases_the_give_and_the_next_one_goes_out ["inventory.give.the-shards-move-releases-the-hold-and-the-next-one-goes-out"],
    scenario_the_shards_refusal_releases_the_give_and_says_which_thing => the_shards_refusal_releases_the_give_and_says_which_thing ["inventory.give.the-shards-refusal-releases-the-hold-and-says-which-thing"],
    scenario_part_of_a_stack_is_released_by_the_stacks_new_count => part_of_a_stack_is_released_by_the_stacks_new_count ["inventory.give.part-of-a-stack-is-released-by-the-stacks-new-count"],
    scenario_a_container_being_viewed_does_not_release_a_give => a_container_being_viewed_does_not_release_a_give ["inventory.give.the-contents-of-a-container-being-viewed-do-not-release-one"],
    scenario_no_cursor_the_client_ships_is_chosen_by_a_drag => no_cursor_the_client_ships_is_chosen_by_a_drag ["inventory.drag-hint.no-cursor-the-client-ships-is-chosen-by-a-drag"],
    scenario_the_shards_unlock_reaches_the_chest_and_the_relock_follows_it => the_shards_unlock_reaches_the_chest_and_the_relock_follows_it ["container.ground.the-shards-unlock-reaches-the-chest-and-the-relock-follows-it"],
    scenario_the_shipped_key_puts_what_is_picked_into_the_players_own_pack => the_shipped_key_puts_what_is_picked_into_the_players_own_pack ["inventory.pickup.the-shipped-key-puts-what-is-picked-into-the-players-own-pack"],
    scenario_the_shipped_key_carries_the_picked_stacks_own_count_into_the_merge => the_shipped_key_carries_the_picked_stacks_own_count_into_the_merge ["inventory.pickup.the-shipped-key-carries-the-picked-stacks-own-count-into-the-merge"],
    scenario_the_shipped_key_with_nothing_picked_sends_nothing => the_shipped_key_with_nothing_picked_sends_nothing ["inventory.pickup.the-shipped-key-with-nothing-picked-sends-nothing"],
    scenario_a_stale_coin_update_for_the_player_is_dropped_by_the_same_counter => a_stale_coin_update_for_the_player_is_dropped_by_the_same_counter ["vendor.purse.a-stale-coin-update-for-the-player-is-dropped-by-the-same-counter"],
}

use pyreal_client_contract::UiRequest;
use pyreal_client_model::inventory::requests::InventoryRequest;
use pyreal_client_model::{RecordingRequests, RecordingSink, Request, World};
use pyreal_client_net::client_session::testing::Corpus;
use pyreal_client_runtime::objects::ObjectStream;
use pyreal_primitives::ServerTime;
use pyreal_testkit::Outbound;
use std::sync::Arc;
use {
    pyreal_client_runtime::character::Character, pyreal_client_runtime::character::CharacterInput,
};
use {pyreal_client_runtime::interaction, pyreal_client_runtime::interaction::Interaction};
use {pyreal_world_data::landblock::load_region, pyreal_world_data::landblock::DEFAULT_LANDBLOCK};

/// The shop: where a row's kind and its stack are read from.
pub(super) mod shop;

/// **Letting something go over the world: the gift, the sale, the trade, the chest and the
/// ground.** One release over the viewport arms the client's own pick and sends nothing; what the
/// pick then finds decides everything.
pub(super) mod give;
/// **Picking something up off the ground.** There is no pickup message: the client picks a thing
/// up with the same `PutItemInContainer` a move into the pack sends.
pub(super) mod pickup;
/// **The lines a use prints when it is refused.**
pub(super) mod use_refusal;

/// Equip: the gate a place makes against the thing let go on it, what happens when the place is
/// already full, and the same asked for by a double-click.
pub(super) mod equip;

mod client_support;
use client_support::{a_client, a_gameplay_client, a_retail_client};

mod container_moves;
use container_moves::the_shards_unlock_reaches_the_chest_and_the_relock_follows_it;

mod containment;
use containment::{
    a_holders_own_list_decides_what_it_holds, a_stale_or_unknown_stow_changes_nothing,
    a_stowed_item_leaves_the_hand_and_the_world,
};

mod drop_readiness;
use drop_readiness::{
    a_give_in_mid_air_is_not_refused, a_world_drop_is_refused_in_mid_air,
    the_ground_answer_is_read_again_every_frame, BUSY, FEEDBACK_CHANNEL,
};

mod give_receipts;
use give_receipts::{
    a_container_being_viewed_does_not_release_a_give,
    a_give_takes_the_hold_and_the_next_gesture_is_refused_in_words, every_recorded_give,
    every_recorded_give_is_answered_by_the_move_and_nothing_else,
    part_of_a_stack_is_released_by_the_stacks_new_count,
    the_shards_move_releases_the_give_and_the_next_one_goes_out,
    the_shards_refusal_releases_the_give_and_says_which_thing, EVERY_SESSION,
};

mod input_hits;
use input_hits::no_cursor_the_client_ships_is_chosen_by_a_drag;

mod request_support;
use request_support::{a_host_with_a_recipient, a_settled_body, is_drop, is_give, jump, DropHost};

mod target_use;
use target_use::{
    the_shipped_key_carries_the_picked_stacks_own_count_into_the_merge,
    the_shipped_key_puts_what_is_picked_into_the_players_own_pack,
    the_shipped_key_with_nothing_picked_sends_nothing,
};

mod vendor_activity;
use vendor_activity::{
    a_stale_coin_update_for_the_player_is_dropped_by_the_same_counter, dword,
    the_purse_is_filled_from_the_players_own_coin, the_recorded_grocer,
};
