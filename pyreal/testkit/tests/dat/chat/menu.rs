use super::*;
// -------------------------------------------------------------------------------------------
// chat.talk-focus: what the player talks on when a channel goes away
// -------------------------------------------------------------------------------------------

/// A client with the chat-room service on, the player described, and talking on the general
/// channel.
fn a_client_on_the_general_channel() -> HeadlessClient {
    let mut c = a_bare_client(true);
    c.tick(1);
    c.world_mut().chat.using_turbine_chat = true;
    describe(&mut c, true, false);
    c.tick(1);
    pick_general(&mut c);
    c
}

/// Put the player on the general channel, as picking it from the talk-to menu does.
fn pick_general(c: &mut HeadlessClient) {
    c.when(Player::ui(
        pyreal_client_contract::UiRequest::SetTalkFocus {
            focus: GENERAL_FOCUS,
        },
    ))
    .tick(1);
    assert_eq!(
        c.view().world().chat.talk_focus,
        TalkFocus::General,
        "picking the channel must move the talk focus"
    );
}

/// The number the chat system knows the general channel by.
const GENERAL_FOCUS: u32 = 8;

/// Whether the last frame said `text` aloud.
fn spoke(c: &HeadlessClient, text: &str) -> bool {
    c.view()
        .expect_app()
        .interaction()
        .last_sent
        .iter()
        .any(|r| matches!(r, pyreal_client_model::Request::Talk(m) if m.message == text))
}

/// A channel the player was talking on, switched off and straight back on again, drops him back to
/// saying it aloud -- and the line he sends next goes out that way.
pub fn a_channel_switched_off_and_on_drops_the_player_back_to_saying_it() {
    let mut c = a_client_on_the_general_channel();

    // A line typed on the channel is not misrouted into an ordinary spoken one just because the
    // room is not there to take it.
    let lines = c.view().expect_app().interaction().stats.chat_lines_sent;
    say(&mut c, "general intent");
    let routed = c.view().expect_app().interaction().stats.chat_lines_sent == lines + 1
        && c.view().world().chat.talk_focus == TalkFocus::General
        && !spoke(&c, "general intent");

    // Switching the channel off and on again is two changes, not none: the player is put back on
    // saying it aloud by the first and left there by the second.
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, false);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, true);
    say(&mut c, "after the fallback");
    let fell_back =
        c.view().world().chat.talk_focus == TalkFocus::All && spoke(&c, "after the fallback");

    // The same edge from the option the player himself sets, with a line between the two halves:
    // the line has to see the fallback that has already happened rather than the final answer.
    pick_general(&mut c);
    c.when(Player::Ui(vec![
        pyreal_client_contract::UiRequest::SetPlayerOption(
            pyreal_client_contract::PlayerOption::HearGeneralChat,
            false,
        ),
        pyreal_client_contract::UiRequest::ChatLine {
            text: "between the options".to_owned(),
            window: 1,
        },
        pyreal_client_contract::UiRequest::SetPlayerOption(
            pyreal_client_contract::PlayerOption::HearGeneralChat,
            true,
        ),
    ]))
    .tick(1);
    let in_between = c.view().world().chat.talk_focus == TalkFocus::All
        && spoke(&c, "between the options")
        && c.view().world().chat.enabled_focuses()[GENERAL_FOCUS as usize];

    // And the same edge when it comes from the shard's own description of the player.
    pick_general(&mut c);
    describe(&mut c, false, false);
    describe(&mut c, true, false);
    c.tick(1);
    let from_the_shard = c.view().world().chat.talk_focus == TalkFocus::All;

    // And when it comes from the service itself going away.
    pick_general(&mut c);
    c.world_mut().chat.using_turbine_chat = false;
    c.world_mut().enable_chat_talk_focuses(false);
    c.world_mut().chat.using_turbine_chat = true;
    c.world_mut().enable_chat_talk_focuses(false);
    c.tick(1);
    let from_the_service = c.view().world().chat.talk_focus == TalkFocus::All
        && c.world_mut().chat.take_talk_focus_notices().is_empty();

    c.assert_behaviour(
        "chat.talk-focus.a-channel-switched-off-and-on-again-drops-the-player-back-to-saying-it",
        move |_| routed && fell_back && in_between && from_the_shard && from_the_service,
    );
    c.shutdown();
}

/// Shared channel fallback runs without an interface, and notices are not kept for a future one.
pub fn channel_fallback_does_not_require_a_window() {
    let mut c = a_bare_client(false);
    let had_none = !has_ui(&c);
    c.world_mut().chat.using_turbine_chat = true;
    describe(&mut c, true, false);
    c.world_mut().chat.set_talk_focus(TalkFocus::General);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, false);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, true);
    c.tick(1);
    let kept = c.view().world().chat.talk_focus == TalkFocus::All
        && c.world_mut().chat.take_talk_focus_notices().is_empty();
    c.tick(1);
    let still = c.view().world().chat.talk_focus == TalkFocus::All;

    c.assert_behaviour(
        "chat.talk-focus.channel-fallback-does-not-require-a-window",
        move |_| had_none && kept && still,
    );
    c.shutdown();
}
