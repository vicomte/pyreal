//! The panels' runtime half: the shipped experience table's costs,
//! the chess game, the contract trackers, and the house messages and commands. Fixture: the retail
//! dats, or a model client, with the shard's answers fed through `Peer` and `Inbound`.
//!
//! The declarations below pair each test wrapper with its behaviour claims and collect the
//! census entries in `ALL`. Implementations and fixtures live in subject modules.
//! **This binary must run serially:** two headless clients in one process share process-wide
//! state.

use pyreal_client_contract::UiRequest;
use pyreal_primitives::ObjectId;
use pyreal_testkit::{ClientSpec, HeadlessClient, Inbound, Peer, Player};

pyreal_testkit::scenarios! {
    scenario_ten_points_cost_the_distance_between_two_entries_of_the_shipped_table => ten_points_cost_the_distance_between_two_entries_of_the_shipped_table ["advancement.cost.the-ten-point-cost-comes-off-the-shipped-experience-table"],
    scenario_every_chess_message_reaches_a_receiver_or_a_sender => every_chess_message_reaches_a_receiver_or_a_sender ["minigame.messages.every-message-the-game-speaks-reaches-a-receiver-or-a-sender"],
    scenario_using_a_board_raises_the_window_and_asks_for_a_seat => using_a_board_raises_the_window_and_asks_for_a_seat ["minigame.board.using-a-board-raises-the-window-and-asks-for-a-seat"],
    scenario_the_seat_deals_the_board_and_the_start_names_whose_turn_it_is => the_seat_deals_the_board_and_the_start_names_whose_turn_it_is ["minigame.board.the-shards-answer-deals-the-board-or-takes-the-window-away"],
    scenario_two_presses_move_a_piece_and_a_refused_move_never_leaves => two_presses_move_a_piece_and_a_refused_move_never_leaves ["minigame.board.two-presses-move-a-piece-and-a-move-the-rules-refuse-never-leaves"],
    scenario_the_opponents_move_is_replayed_on_the_players_own_board => the_opponents_move_is_replayed_on_the_players_own_board ["minigame.board.the-opponents-move-is-replayed-on-the-players-own-board"],
    scenario_the_offer_of_a_draw_and_the_end_of_a_game_are_said_in_the_window => the_offer_of_a_draw_and_the_end_of_a_game_are_said_in_the_window ["minigame.window.the-offer-of-a-stalemate-and-the-end-of-a-game-are-said-in-the-window"],
    scenario_the_window_buttons_offer_a_draw_and_ask_before_resigning => the_window_buttons_offer_a_draw_and_ask_before_resigning ["minigame.window.the-buttons-offer-a-stalemate-and-ask-before-resigning"],
    scenario_a_message_about_another_board_changes_nothing => a_message_about_another_board_changes_nothing ["minigame.board.a-message-about-another-board-changes-nothing"],
    scenario_one_contract_at_a_time_is_added_changed_or_taken_away => one_contract_at_a_time_is_added_changed_or_taken_away ["contracts.list.one-contract-at-a-time-is-added-changed-or-taken-away"],
    scenario_the_client_asks_about_the_house_once_when_it_learns_who_it_is => the_client_asks_about_the_house_once_when_it_learns_who_it_is ["house.query.the-client-asks-about-the-house-once-when-it-learns-who-it-is"],
    scenario_a_new_maintenance_period_clears_every_payment => a_new_maintenance_period_clears_every_payment ["house.rent.a-new-period-moves-both-dates-and-marks-every-payment-unpaid"],
    scenario_a_payment_update_replaces_the_list_rather_than_merging => a_payment_update_replaces_the_list_rather_than_merging ["house.rent.a-payment-update-replaces-the-list-rather-than-merging-into-it"],
    scenario_neither_maintenance_update_touches_a_houseless_character => neither_maintenance_update_touches_a_houseless_character ["house.rent.neither-update-touches-a-character-with-no-house"],
    scenario_a_restriction_update_reaches_the_object_it_names => a_restriction_update_reaches_the_object_it_names ["house.restrictions.an-update-reaches-the-object-it-names-and-never-the-player"],
    scenario_a_transaction_answer_clears_the_pane_as_a_status_does => a_transaction_answer_clears_the_pane_as_a_status_does ["house.data.a-transaction-answer-clears-the-pane-exactly-as-a-status-answer-does"],
    scenario_every_house_sub_command_that_sends_puts_its_own_message_on_the_wire => every_house_sub_command_that_sends_puts_its_own_message_on_the_wire ["house.commands.every-sub-command-that-sends-puts-its-own-message-on-the-wire"],
    scenario_a_house_line_the_ladder_refuses_prints_the_shipped_sentence_and_sends_nothing => a_house_line_the_ladder_refuses_prints_the_shipped_sentence_and_sends_nothing ["house.commands.a-line-the-ladder-refuses-prints-the-shipped-sentence-and-sends-nothing"],
    scenario_the_help_listing_and_the_house_help_are_what_the_client_ships => the_help_listing_and_the_house_help_are_what_the_client_ships ["house.commands.the-help-listing-and-the-house-help-are-what-the-client-ships"],
    scenario_the_guest_list_the_shard_sends_back_is_written_out_on_the_chat_log => the_guest_list_the_shard_sends_back_is_written_out_on_the_chat_log ["house.guests.the-guest-list-the-shard-sends-back-is-written-out-on-the-chat-log"],
    scenario_the_free_houses_the_shard_lists_are_written_out_with_their_locations => the_free_houses_the_shard_lists_are_written_out_with_their_locations ["house.available.the-free-houses-the-shard-lists-are-written-out-with-their-locations"],
}

/// One message, delivered the way the session layer delivers it.
fn deliver<M: pyreal_protocol::Message>(c: &mut HeadlessClient, m: &M) {
    c.when(Inbound::message(m));
    c.tick(1);
}

use pyreal_client_model::chat_cmd as house_cmd;
use pyreal_client_model::minigame::GameBoard as ChessBoard;
use {
    pyreal_rules::chess::move_result as chess_mr, pyreal_rules::chess::Coord as ChessCoord,
    pyreal_rules::chess::GameState, pyreal_rules::chess::PieceType,
};

mod character_stats;
use character_stats::ten_points_cost_the_distance_between_two_entries_of_the_shipped_table;

mod chess_board;
use chess_board::{
    a_message_about_another_board_changes_nothing,
    every_chess_message_reaches_a_receiver_or_a_sender,
    the_offer_of_a_draw_and_the_end_of_a_game_are_said_in_the_window,
    the_opponents_move_is_replayed_on_the_players_own_board,
    the_seat_deals_the_board_and_the_start_names_whose_turn_it_is,
    the_window_buttons_offer_a_draw_and_ask_before_resigning,
    two_presses_move_a_piece_and_a_refused_move_never_leaves,
    using_a_board_raises_the_window_and_asks_for_a_seat,
};

mod contracts_list;
use contracts_list::one_contract_at_a_time_is_added_changed_or_taken_away;

mod housing;
use housing::{
    a_new_maintenance_period_clears_every_payment,
    a_payment_update_replaces_the_list_rather_than_merging,
    a_restriction_update_reaches_the_object_it_names,
    a_transaction_answer_clears_the_pane_as_a_status_does, house_event,
    neither_maintenance_update_touches_a_houseless_character,
    the_client_asks_about_the_house_once_when_it_learns_who_it_is, HOUSE_PLAYER,
};

mod housing_commands;
use housing_commands::{
    a_house_line_the_ladder_refuses_prints_the_shipped_sentence_and_sends_nothing,
    every_house_sub_command_that_sends_puts_its_own_message_on_the_wire,
    the_free_houses_the_shard_lists_are_written_out_with_their_locations,
    the_guest_list_the_shard_sends_back_is_written_out_on_the_chat_log,
    the_help_listing_and_the_house_help_are_what_the_client_ships,
};
