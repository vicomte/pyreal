//! The DAT UI scenarios: the run lock's notices, the link's
//! status through a real failure, and the chat stamp's zone.
//! Run serially because whole headless clients share process-wide state.

mod autorun;
mod connection;
mod dates;

use autorun::{
    a_key_that_changes_nothing_says_nothing, the_run_lock_says_so_in_the_message_window,
};
use connection::{
    a_real_failure_takes_the_player_off_the_world, setting_the_clock_does_not_change_the_lamp,
};
use dates::the_chat_stamp_carries_the_machines_own_offset;

use pyreal_testkit::{ClientSpec, HeadlessClient};

pyreal_testkit::scenarios! {
    scenario_the_run_lock_says_so_in_the_message_window => the_run_lock_says_so_in_the_message_window ["notice.autorun.turning-the-run-lock-on-or-off-says-so-in-the-message-window"],
    scenario_a_key_that_changes_nothing_says_nothing => a_key_that_changes_nothing_says_nothing ["notice.autorun.a-key-that-does-not-change-the-run-lock-says-nothing"],
    scenario_setting_the_clock_does_not_change_the_lamp => setting_the_clock_does_not_change_the_lamp ["hud.link-lamp.the-shard-setting-the-clock-does-not-change-it-and-nor-does-a-quiet-link"],
    scenario_a_real_failure_takes_the_player_off_the_world => a_real_failure_takes_the_player_off_the_world ["hud.link-lamp.a-real-failure-puts-the-player-off-the-world-rather-than-reddening-a-lamp"],
    scenario_the_chat_stamp_carries_the_machines_own_offset => the_chat_stamp_carries_the_machines_own_offset ["dates.the-chat-stamp-carries-the-machines-own-offset-and-not-a-constant"],
}

use pyreal_desktop::platform::local_utc_offset_secs;
