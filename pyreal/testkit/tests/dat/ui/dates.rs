//! UI fixtures and scenarios for dates.

use super::*;
// ---------------------------------------------------------------------------------------------
// dates.the-chat-stamp-carries-the-machines-own-offset-and-not-a-constant
// ---------------------------------------------------------------------------------------------

/// The one surface whose shape was always right, because that shape says nothing about where in
/// the world it is. What is claimed here is the number behind it.
pub(super) fn the_chat_stamp_carries_the_machines_own_offset() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.tick(6);

    let (stamped_at, offset) = {
        let world = c.view().world();
        (world.scroll.now_unix, world.scroll.utc_offset_secs)
    };
    let the_clock_was_written = stamped_at > 1_700_000_000;
    let it_is_the_machines = offset == local_utc_offset_secs(stamped_at);

    let prefix = pyreal_client_model::scroll::timestamp_prefix(stamped_at, offset);
    let hour = prefix.split(':').next().unwrap_or("");
    let the_stamp_is_shaped_right =
        prefix.ends_with(' ') && !hour.is_empty() && (hour.len() == 1 || !hour.starts_with('0'));
    // ...and it is not the shard's time, unless this machine keeps that time.
    let not_a_constant =
        offset == 0 || prefix != pyreal_client_model::scroll::timestamp_prefix(stamped_at, 0);

    c.assert_behaviour(
        "dates.the-chat-stamp-carries-the-machines-own-offset-and-not-a-constant",
        move |_| {
            the_clock_was_written
                && it_is_the_machines
                && the_stamp_is_shaped_right
                && not_a_constant
        },
    );
    c.shutdown();
}
