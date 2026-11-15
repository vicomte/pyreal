//! UI fixtures and scenarios for autorun.

use super::*;
// ---------------------------------------------------------------------------------------------
// notice.autorun.*
//
// The words themselves are literals in the scenarios below rather than the symbols the client
// writes them through.
// ---------------------------------------------------------------------------------------------

/// The two lines the client writes, and the channel it writes them on. Literals, not the symbols
/// the client writes them through: a scenario that read a value back through the symbol the
/// production code writes it through could not detect a wrong value.
const AUTORUN_ON: &str = "AutoRun ON";
const AUTORUN_OFF: &str = "AutoRun OFF";
const CLIENT_FEEDBACK_CHANNEL: u32 = 0x1A;

/// What the client has queued to say and not yet said.
fn queued_to_say(c: &HeadlessClient) -> Vec<(u32, String)> {
    c.view()
        .expect_app()
        .objects()
        .world
        .scroll
        .pending()
        .iter()
        .map(|f| (f.chat_type, f.body.clone()))
        .collect()
}

/// How many lines the client has ever written -- a count and not a queue length, because the queue
/// is drained every frame and "nothing was said" and "something was said and consumed" are the
/// same reading without it.
fn lines_ever_written(c: &HeadlessClient) -> u64 {
    c.view().expect_app().objects().world.scroll.added
}

/// A client in the world.
fn a_client_in_the_world() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay_in_world(4))
}

/// One movement action going down or up, as the input manager hands it on, and the frame that
/// acts on it. Injected rather than keyed: Horizon keeps the keyboard off the world until the
/// player has entered it, and these clients never log in.
fn act(c: &mut HeadlessClient, id: pyreal_client_runtime::actions::ActionId, start: bool) {
    use pyreal_client_runtime::actions::Action;
    c.app_mut().inject_action(if start {
        Action::begin(id)
    } else {
        Action::end(id)
    });
    c.tick(1);
}

/// The run lock's key: one-shot, so only its press is an action.
fn toggle_the_run_lock(c: &mut HeadlessClient) {
    act(
        c,
        pyreal_client_runtime::actions::movement::action::AUTORUN,
        true,
    );
}

/// Turning the run lock on or off says so in the message window.
pub(super) fn the_run_lock_says_so_in_the_message_window() {
    let mut c = a_client_in_the_world();
    let starts_off =
        !c.view().expect_app().probe().char_input().auto_run && queued_to_say(&c).is_empty();
    let base = lines_ever_written(&c);

    toggle_the_run_lock(&mut c);
    let on = c.view().expect_app().probe().char_input().auto_run
        && queued_to_say(&c) == vec![(CLIENT_FEEDBACK_CHANNEL, AUTORUN_ON.to_owned())]
        && lines_ever_written(&c) == base + 1
        && c.view()
            .expect_app()
            .probe()
            .movement_commands()
            .auto_run_notices_sent
            == 1;
    c.tick(1);
    let drained = queued_to_say(&c).is_empty();

    toggle_the_run_lock(&mut c);
    let off = !c.view().expect_app().probe().char_input().auto_run
        && queued_to_say(&c) == vec![(CLIENT_FEEDBACK_CHANNEL, AUTORUN_OFF.to_owned())]
        && lines_ever_written(&c) == base + 2
        && c.view()
            .expect_app()
            .probe()
            .movement_commands()
            .auto_run_notices_sent
            == 2;
    c.tick(1);

    c.assert_behaviour(
        "notice.autorun.turning-the-run-lock-on-or-off-says-so-in-the-message-window",
        move |_| starts_off && on && drained && off,
    );
    c.shutdown();
}

/// An action that does not change the run lock says nothing.
pub(super) fn a_key_that_changes_nothing_says_nothing() {
    use pyreal_client_runtime::actions::movement::action::MOVE_FORWARD;

    let mut c = a_client_in_the_world();
    let base = lines_ever_written(&c);

    // The lock is already off, so moving does not change it -- and the client says nothing.
    // Without this direction the claim above would hold on a client that wrote the line on every
    // press.
    act(&mut c, MOVE_FORWARD, true);
    let walked = c.view().expect_app().probe().char_input().forward;
    let silent = lines_ever_written(&c) == base
        && c.view()
            .expect_app()
            .probe()
            .movement_commands()
            .auto_run_notices_sent
            == 0;
    act(&mut c, MOVE_FORWARD, false);

    // ...and the same action when it *does* change it -- cancelling the lock -- tells the player.
    toggle_the_run_lock(&mut c);
    let locked = c.view().expect_app().probe().char_input().auto_run;
    c.tick(1);
    let after_on = lines_ever_written(&c);
    // Forward starting is where the input layer releases the run lock, in the same frame and
    // ahead of the forward itself.
    c.app_mut()
        .inject_action(pyreal_client_runtime::actions::Action::end(
            pyreal_client_runtime::actions::movement::action::AUTORUN,
        ));
    act(&mut c, MOVE_FORWARD, true);
    let cancelled = !c.view().expect_app().probe().char_input().auto_run
        && queued_to_say(&c) == vec![(CLIENT_FEEDBACK_CHANNEL, AUTORUN_OFF.to_owned())]
        && lines_ever_written(&c) == after_on + 1;
    act(&mut c, MOVE_FORWARD, false);

    c.assert_behaviour(
        "notice.autorun.a-key-that-does-not-change-the-run-lock-says-nothing",
        move |_| walked && silent && locked && cancelled,
    );
    c.shutdown();
}
