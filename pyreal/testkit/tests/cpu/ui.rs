//! The interface layer with no game model behind it: a line the client writes for itself when the
//! character logs off, and the shape and zone a date is drawn in.

use pyreal_testkit::HeadlessClient;

pyreal_testkit::scenarios! {
    scenario_a_line_still_waiting_when_the_character_logs_off_goes_with_the_windows => a_line_still_waiting_when_the_character_logs_off_goes_with_the_windows ["notice.a-line-still-waiting-when-the-character-logs-off-goes-with-the-windows"],
    scenario_the_shape_is_the_shipped_runtimes_own_and_the_zone_is_handed_in => the_shape_is_the_shipped_runtimes_own_and_the_zone_is_handed_in ["dates.the-shape-is-the-shipped-runtimes-own-and-the-zone-is-something-handed-in"],
    scenario_a_pane_draws_in_whatever_zone_it_is_handed_row_by_row => a_pane_draws_in_whatever_zone_it_is_handed_row_by_row ["dates.a-pane-draws-in-whatever-zone-it-is-handed-row-by-row"],
}

/// The channel the client talks to itself on.
const BUBBLE_CHAT_TYPE: u8 = 0x1A;

// =============================================================================================
// notice.a-line-still-waiting-when-the-character-logs-off-goes-with-the-windows
// =============================================================================================

/// The line is drained and *then* discarded, which is the client's own order -- so the counter and
/// the queue disagree here on purpose.
pub fn a_line_still_waiting_when_the_character_logs_off_goes_with_the_windows() {
    let mut c = HeadlessClient::model();
    c.world_mut()
        .scroll
        .add_text_to_scroll("a refusal", u32::from(BUBBLE_CHAT_TYPE), true, 0);
    let one_is_waiting = c.view().world().scroll.pending().len() == 1;

    c.when(pyreal_testkit::Inbound::event(
        pyreal_client_net::client_session::SessionEvent::LoggedOff,
    ));

    let the_queue_is_empty = c.view().world().scroll.pending().is_empty();
    // It was counted on the way past: the drain runs before the log-off arm.
    let it_was_counted = c.chat_lines().len() == 1;

    c.assert_behaviour(
        "notice.a-line-still-waiting-when-the-character-logs-off-goes-with-the-windows",
        move |_| one_is_waiting && the_queue_is_empty && it_was_counted,
    );
}

// =============================================================================================
// dates.* -- the shape a date is drawn in, and the zone it is drawn for
//
// The same house at the same instant can read seven hours apart and in two different shapes.
// These two claims are arithmetic over the client's own formatter and over the view a pane is
// handed, and need no data file; the surfaces that draw a date are in the `dat` tier.
//
// **No assertion here reads the machine's own zone.** Every zone below is a number this file
// writes out, so these say the same thing wherever they are run.
// =============================================================================================

use pyreal_client_contract::ctime::{asctime, strftime_c};
use pyreal_ui_logic::house;

/// The instant a pair of retail screenshots shows.
const THE_INSTANT: i64 = 1_789_404_178;
/// The zone that machine was in, written out rather than read.
const WEST: i32 = -7 * 3600;
/// A zone on the far side of the date line, so a sign error cannot cancel out.
const EAST: i32 = 9 * 3600;
/// What the shipped runtime writes for that instant in that zone, and what this client used to
/// write instead.
const WHAT_IT_SHOULD_SAY: &str = "9/14/2026 9:42:58 AM";
const WHAT_IT_USED_TO_SAY: &str = "Mon Sep 14 16:42:58 2026";

// =============================================================================================
// dates.the-shape-is-the-shipped-runtimes-own-and-the-zone-is-something-handed-in
// =============================================================================================

/// The values are the shipped runtime's own answers, taken from it once and written down here, so
/// this is a claim about the client and not a restatement of it.
pub fn the_shape_is_the_shipped_runtimes_own_and_the_zone_is_handed_in() {
    // The pair from the screenshot, on one line each.
    let the_pair = strftime_c(THE_INSTANT, WEST) == WHAT_IT_SHOULD_SAY
        && asctime(THE_INSTANT, 0) == WHAT_IT_USED_TO_SAY
        && strftime_c(THE_INSTANT, WEST) != asctime(THE_INSTANT, WEST);

    // Eight instants, including both ends of the twelve-hour clock, a leap day and the far end of
    // a thirty-two-bit clock.
    let the_shape = [
        (0_i64, "1/1/1970 12:00:00 AM"),
        (43_200, "1/1/1970 12:00:00 PM"),
        (46_800, "1/1/1970 1:00:00 PM"),
        (86_399, "1/1/1970 11:59:59 PM"),
        (951_782_400, "2/29/2000 12:00:00 AM"),
        (1_789_404_178, "9/14/2026 4:42:58 PM"),
        (1_788_565_581, "9/4/2026 11:46:21 PM"),
        (2_147_483_647, "1/19/2038 3:14:07 AM"),
    ]
    .into_iter()
    .all(|(t, want)| strftime_c(t, 0) == want);

    // The zone is a shift and not a label: east and west, across midnight in both directions.
    let a_real_shift = strftime_c(0, -3600) == "12/31/1969 11:00:00 PM"
        && strftime_c(0, EAST) == "1/1/1970 9:00:00 AM"
        && strftime_c(THE_INSTANT, EAST) == "9/15/2026 1:42:58 AM";

    // The other shape pads the day with a zero. The one character that separates the runtime the
    // client ships with from the one this workspace is built on.
    let the_other_shape =
        asctime(0, 0) == "Thu Jan 01 00:00:00 1970" && !asctime(0, 0).contains("Jan  1");

    // ...and no date in this shape ever carries a weekday or a month's name, at any zone and any
    // instant, which is what catches a surface reaching for the other one.
    let mut never_the_other = true;
    for off in [0, WEST, EAST] {
        for t in [0_i64, THE_INSTANT, 1_788_565_581, 2_147_483_647] {
            let c = strftime_c(t, off);
            never_the_other &= ![
                "Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun", "Jan", "Sep", "Dec",
            ]
            .iter()
            .any(|w| c.contains(w))
                && (c.ends_with(" AM") || c.ends_with(" PM"));
        }
    }

    // The two answers that are not a date at all: nothing recorded, and nothing to say.
    let the_two_non_dates =
        house::convert_time(0, WEST) == "N/A" && house::convert_time(-1, WEST).is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "dates.the-shape-is-the-shipped-runtimes-own-and-the-zone-is-something-handed-in",
        move |_| {
            the_pair
                && the_shape
                && a_real_shift
                && the_other_shape
                && never_the_other
                && the_two_non_dates
        },
    );
}

// =============================================================================================
// dates.a-pane-draws-in-whatever-zone-it-is-handed-row-by-row
// =============================================================================================

/// Driven at zones this machine is not in, which is the point: a host on the far side of the
/// world gets that side's sheet.
pub fn a_pane_draws_in_whatever_zone_it_is_handed_row_by_row() {
    use pyreal_client_contract::view::{HouseDataView, HousePurchaseView};

    let mut every_row_follows = true;
    for (off, want) in [
        (0, "9/14/2026 4:42:58 PM"),
        (WEST, WHAT_IT_SHOULD_SAY),
        (EAST, "9/15/2026 1:42:58 AM"),
    ] {
        let d = HouseDataView {
            buy_time: THE_INSTANT,
            maintenance_period_end: THE_INSTANT,
            maintenance_next_due: THE_INSTANT,
            utc_offset_secs: [off; 3],
            ..HouseDataView::default()
        };
        every_row_follows &= house::convert_time(d.buy_time, d.utc_offset_secs[0]) == want
            && house::convert_time(d.maintenance_period_end, d.utc_offset_secs[1]) == want
            && house::convert_time(d.maintenance_next_due, d.utc_offset_secs[2]) == want;
        let p = HousePurchaseView {
            utc_offset_secs: off,
            ..HousePurchaseView::default()
        };
        every_row_follows &= strftime_c(THE_INSTANT, p.utc_offset_secs) == want;
    }

    // Three rows, three zones at once -- which is why the zone is a row's and not a pane's: each
    // row is worked out on its own.
    let d = HouseDataView {
        buy_time: THE_INSTANT,
        maintenance_period_end: THE_INSTANT,
        maintenance_next_due: THE_INSTANT,
        utc_offset_secs: [0, WEST, EAST],
        ..HouseDataView::default()
    };
    let three: Vec<String> = [0_usize, 1, 2]
        .map(|i| {
            house::convert_time(
                [d.buy_time, d.maintenance_period_end, d.maintenance_next_due][i],
                d.utc_offset_secs[i],
            )
        })
        .to_vec();
    let three_at_once = three
        == vec![
            "9/14/2026 4:42:58 PM".to_owned(),
            WHAT_IT_SHOULD_SAY.to_owned(),
            "9/15/2026 1:42:58 AM".to_owned(),
        ];

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "dates.a-pane-draws-in-whatever-zone-it-is-handed-row-by-row",
        move |_| every_row_follows && three_at_once,
    );
}
