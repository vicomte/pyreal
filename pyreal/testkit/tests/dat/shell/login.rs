//! The client's answer to the shard's log-off and character-list messages.

use super::*;
use pyreal_client_net::client_session::SessionEvent;
use pyreal_primitives::ObjectId;
use pyreal_testkit::replay::Peer;

/// The shard's own list of three.
fn wire_character_set() -> pyreal_protocol::login::LoginCharacterSet {
    let named = |gid: u32, name: &str| pyreal_protocol::login::CharacterIdentity {
        gid: ObjectId(gid),
        name: name.into(),
        seconds_greyed_out: 0,
    };
    pyreal_protocol::login::LoginCharacterSet {
        status: 0,
        characters: vec![
            named(0x5000_0003, "Zeddish"),
            named(0x5000_0001, "Larktest"),
            named(0x5000_0002, "Tarinell"),
        ],
        deleted: vec![],
        num_allowed_characters: 5,
        account: "acct0001".into(),
        use_turbine_chat: 0,
        has_throne_of_destiny: 0,
    }
}

// ---------------------------------------------------------------------------------------------
// shell-only.log-off.the-shards-answer-does-not-end-a-client-that-has-a-screen-to-go-back-to
// ---------------------------------------------------------------------------------------------

/// The defect that made re-entry impossible in the most literal way there is: the process was
/// gone. Both directions, because a client that always keeps running and one that never does read
/// alike from one of them.
pub(super) fn the_shards_log_off_answer_does_not_end_a_client_with_screens() {
    // With a screen to go back to: thirty frames of still running.
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let _shard = Peer::attach(&mut c, ObjectId(0x5000_0001));
    c.app_mut()
        .process_logon_event_queue(vec![SessionEvent::LoggedOff]);
    let mut still_running = true;
    for _ in 0..30 {
        still_running &= c.app_mut().frame();
    }
    let not_shutting_down =
        c.view().expect_app().state() != pyreal_client_runtime::app::AppState::ShuttingDown;
    c.shutdown();

    // With none: the same answer ends the run, because there is nothing to return to and nothing
    // to take the player's next press.
    let mut c = HeadlessClient::new(ClientSpec::retail());
    let no_screens = {
        use pyreal_client_runtime::shell::Shell as _;
        !c.view().expect_app().shell().has_ui()
    };
    let _shard = Peer::attach(&mut c, ObjectId(0x5000_0001));
    let running_before = c.app_mut().frame();
    c.app_mut()
        .process_logon_event_queue(vec![SessionEvent::LoggedOff]);
    let ended = !c.app_mut().frame();

    c.assert_behaviour(
        "shell-only.log-off.the-shards-answer-does-not-end-a-client-that-has-a-screen-to-go-back-to",
        move |_| still_running && not_shutting_down && no_screens && running_before && ended,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// shell-only.character-set.every-arrival-is-a-notice-even-when-the-list-has-not-changed
// ---------------------------------------------------------------------------------------------

/// A notice is a count, not a value: the shard re-sends the *same* list after a log-off, and a
/// client comparing the two would see nothing happen at the one moment something did.
pub(super) fn every_character_set_the_session_decodes_is_an_arrival() {
    let mut c = HeadlessClient::new(ClientSpec::retail());
    let _shard = Peer::attach(&mut c, ObjectId(0x5000_0001));
    let nothing_yet = c.view().expect_app().host_state().character_set_notices == 0;

    let set = wire_character_set();
    c.app_mut()
        .process_logon_event_queue(vec![SessionEvent::CharacterSet(Box::new(set.clone()))]);
    let first = c.view().expect_app().host_state().character_set_notices == 1;

    // The second carries the **same** characters, which is what the shard sends after a log-off.
    let before = c.view().expect_app().host_state().character_set.clone();
    c.app_mut()
        .process_logon_event_queue(vec![SessionEvent::CharacterSet(Box::new(set))]);
    let unchanged = c.view().expect_app().host_state().character_set == before;
    let second = c.view().expect_app().host_state().character_set_notices == 2;

    c.assert_behaviour(
        "shell-only.character-set.every-arrival-is-a-notice-even-when-the-list-has-not-changed",
        move |_| nothing_yet && first && unchanged && second,
    );
    c.shutdown();
}
