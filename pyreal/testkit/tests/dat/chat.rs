//! Chat: what leaves the client when the player talks -- rooms and channels, emotes and poses
//! -- and the lines the HUD composes from what the shard says, deaths among them.
//!
//! Every scenario here opens the retail dats under `$PYREAL_TEST_DAT_DIR`, and **this binary must run
//! serially**. Incoming lines are packets built here and fed through a real session, or recordings
//! replayed through `Inbound`.
//!
//! `ALL` is this file's own list, so a scenario that is written and not listed shows up as a
//! shortfall rather than as a silent gap.

use pyreal_client_model::chat::TalkFocus;
use pyreal_testkit::{ClientSpec, HeadlessClient, Player};

pyreal_testkit::scenarios! {
    scenario_the_general_channel_carries_a_typed_line_to_its_room => the_general_channel_carries_a_typed_line_to_its_room ["chat.turbine.the-general-channel-carries-a-typed-line-to-its-room"],
    scenario_a_line_typed_before_the_shard_names_a_room_is_refused => a_line_typed_before_the_shard_names_a_room_is_refused ["chat.turbine.a-line-typed-before-the-shard-names-a-room-is-refused"],
    scenario_the_room_service_runs_with_no_interface_at_all => the_room_service_runs_with_no_interface_at_all ["chat.turbine.the-service-runs-with-no-interface-at-all"],
    scenario_the_hosts_own_spelling_is_what_goes_on_the_wire => the_hosts_own_spelling_is_what_goes_on_the_wire ["chat.turbine.the-hosts-own-spelling-is-what-goes-on-the-wire"],
    scenario_a_room_line_reaches_the_log_with_its_rooms_name_and_colour => a_room_line_reaches_the_log_with_its_rooms_name_and_colour ["chat.turbine.a-room-line-reaches-the-log-with-its-rooms-own-name-and-colour"],
    scenario_a_line_in_flight_does_not_follow_the_character_out => a_line_in_flight_does_not_follow_the_character_out ["chat.turbine.a-line-in-flight-does-not-follow-the-character-out"],
    scenario_room_lines_and_ordinary_notices_share_one_ordered_queue => room_lines_and_ordinary_notices_share_one_ordered_queue ["chat.turbine.the-rooms-lines-and-the-ordinary-notices-share-one-queue"],
    scenario_typing_an_emote_sends_it_and_leaves_the_next_line_free => typing_an_emote_sends_it_and_leaves_the_next_line_free ["chat.emote.typing-one-into-the-shipped-entry-sends-it-and-leaves-the-next-line-free"],
    scenario_a_typed_emote_reaches_the_link_on_the_next_frame => a_typed_emote_reaches_the_link_on_the_next_frame ["chat.emote.a-typed-one-reaches-the-link-on-the-next-frame-and-only-once"],
    scenario_a_channel_switched_off_and_on_drops_the_player_back_to_saying_it => a_channel_switched_off_and_on_drops_the_player_back_to_saying_it ["chat.talk-focus.a-channel-switched-off-and-on-again-drops-the-player-back-to-saying-it"],
    scenario_channel_fallback_does_not_require_a_window => channel_fallback_does_not_require_a_window ["chat.talk-focus.channel-fallback-does-not-require-a-window"],
    scenario_every_channel_word_sends_its_own_channel => every_channel_word_sends_its_own_channel ["chat.channel-commands.every-word-sends-its-own-channel-and-says-nothing-locally"],
    scenario_an_empty_channel_command_says_what_to_do_and_sends_nothing => an_empty_channel_command_says_what_to_do_and_sends_nothing ["chat.channel-commands.one-with-no-text-says-what-to-do-and-sends-nothing"],
    scenario_a_squelched_speaker_is_still_heard_on_a_channel => a_squelched_speaker_is_still_heard_on_a_channel ["chat.channel.silencing-the-speaker-does-not-silence-a-channel-line"],
    scenario_every_recorded_kill_notification_is_drawn_verbatim => every_recorded_kill_notification_is_drawn_verbatim ["chat.death.every-recorded-kill-notification-is-drawn-word-for-word-on-the-log"],
    scenario_your_own_death_goes_through_the_same_hand => your_own_death_goes_through_the_same_hand ["chat.death.your-own-death-goes-through-the-same-hand-and-an-empty-one-says-nothing"],
    scenario_a_death_you_were_part_of_reaches_the_log_only_when_you_were_not => a_death_you_were_part_of_reaches_the_log_only_when_you_were_not ["chat.death.a-death-the-player-was-part-of-reaches-the-log-only-when-he-was-not"],
    scenario_no_death_line_is_silenced_where_a_combat_line_is => no_death_line_is_silenced_where_a_combat_line_is ["chat.death.no-death-line-can-be-silenced-where-a-combat-line-can"],
    scenario_a_key_bound_to_a_pose_moves_the_body => a_key_bound_to_a_pose_moves_the_body ["chat.pose.a-key-bound-to-a-pose-moves-the-body-and-one-that-is-not-is-bound-to-nothing"],
    scenario_a_run_between_stars_is_performed_and_the_rest_spoken => a_run_between_stars_is_performed_and_the_rest_spoken ["chat.pose.a-run-between-stars-is-performed-and-the-rest-of-the-line-is-spoken"],
    scenario_a_run_alone_says_nothing_and_an_unknown_one_is_spoken => a_run_alone_says_nothing_and_an_unknown_one_is_spoken ["chat.pose.a-run-alone-says-nothing-and-one-the-client-cannot-place-is-spoken-whole"],
    scenario_the_say_command_goes_through_the_same_extraction => the_say_command_goes_through_the_same_extraction ["chat.pose.the-say-command-goes-through-the-same-extraction"],
    scenario_the_emotes_command_prints_the_shipped_list => the_emotes_command_prints_the_shipped_list ["chat.pose.the-list-command-prints-the-shipped-list-and-sends-nothing"],
    scenario_a_pose_sends_a_different_message_from_the_emote_command => a_pose_sends_a_different_message_from_the_emote_command ["chat.pose.what-a-pose-sends-is-a-different-message-from-what-the-command-sends"],
    scenario_the_possessive_word_is_chosen_by_sex => the_possessive_word_is_chosen_by_sex ["chat.pose.the-possessive-word-is-chosen-by-sex-before-the-message-leaves"],
    scenario_the_players_own_echo_is_never_turned_into_a_noise => the_players_own_echo_is_never_turned_into_a_noise ["chat.pose.the-players-own-echo-is-never-turned-into-a-noise"],
    scenario_every_pose_the_list_advertises_can_be_performed => every_pose_the_list_advertises_can_be_performed ["chat.pose.every-pose-the-list-advertises-can-really-be-performed"],
    scenario_what_a_typed_line_becomes_follows_the_menu => what_a_typed_line_becomes_follows_the_menu ["chat.talk-focus.what-a-typed-line-becomes-follows-the-menu"],
    scenario_a_command_the_client_handles_is_never_spoken => a_command_the_client_handles_is_never_spoken ["chat.commands.a-command-the-client-handles-is-never-spoken-to-the-shard"],
}

// =============================================================================================
// The chat-room service: starting it, typing into it, and what it composes
// =============================================================================================
//
// **No socket is opened anywhere here.** An outgoing line is read out of the client's own outbox
// and, where the bytes are the claim, handed to the production sender over a mock transport; an
// incoming one is a packet this file builds and feeds through a real session, which is what decides
// that a packet on that queue is a chat-room line at all.

use pyreal_client_net::client_session::testing::{Corpus, Direction, MockTransport};
use pyreal_client_net::client_session::{Session, SessionEvent};
use pyreal_primitives::{IncomingMessage, LocalTime, NetBlobId, NetQueue, ObjectId, RecipientId};
use pyreal_protocol::login::LoginCharacterSet;
use pyreal_testkit::Inbound;

mod menu;
use menu::{
    a_channel_switched_off_and_on_drops_the_player_back_to_saying_it,
    channel_fallback_does_not_require_a_window,
};

mod rooms;
use rooms::{
    a_line_in_flight_does_not_follow_the_character_out,
    a_line_typed_before_the_shard_names_a_room_is_refused,
    a_room_line_reaches_the_log_with_its_rooms_name_and_colour,
    room_lines_and_ordinary_notices_share_one_ordered_queue,
    the_general_channel_carries_a_typed_line_to_its_room,
    the_hosts_own_spelling_is_what_goes_on_the_wire,
    the_room_service_runs_with_no_interface_at_all, ROOMS_SESSION,
};

mod support;
use support::{
    a_bare_client, a_client_listening, composed_text, describe, has_ui, on_the_wire, recorded, say,
    scroll_lines, word, MAIN_WINDOW,
};

mod emote_entry;
use emote_entry::{
    a_typed_emote_reaches_the_link_on_the_next_frame,
    typing_an_emote_sends_it_and_leaves_the_next_line_free,
};

mod channel_commands;
use channel_commands::{
    an_empty_channel_command_says_what_to_do_and_sends_nothing,
    every_channel_word_sends_its_own_channel,
};

mod channel_display;
use channel_display::a_squelched_speaker_is_still_heard_on_a_channel;

mod deaths;
use deaths::{
    a_death_you_were_part_of_reaches_the_log_only_when_you_were_not,
    every_recorded_kill_notification_is_drawn_verbatim,
    no_death_line_is_silenced_where_a_combat_line_is, your_own_death_goes_through_the_same_hand,
};

mod poses;
use poses::{
    a_key_bound_to_a_pose_moves_the_body, a_pose_sends_a_different_message_from_the_emote_command,
    a_run_alone_says_nothing_and_an_unknown_one_is_spoken,
    a_run_between_stars_is_performed_and_the_rest_spoken,
    every_pose_the_list_advertises_can_be_performed, the_emotes_command_prints_the_shipped_list,
    the_players_own_echo_is_never_turned_into_a_noise, the_possessive_word_is_chosen_by_sex,
    the_say_command_goes_through_the_same_extraction,
};

mod surface;
use surface::{
    a_command_the_client_handles_is_never_spoken, what_a_typed_line_becomes_follows_the_menu,
};
