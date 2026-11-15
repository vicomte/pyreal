//! The house pane's rows: what a house's data and the purchase wait read as, in the client's order
//! and with each row's colour.

use pyreal_client_contract::ctime;
use pyreal_client_contract::view::{HouseDataView, HousePurchaseView};

/// The client's no-house-data arm.
pub const NO_HOUSE: &str = "You do not currently own a house.";

/// The purchase-price display's **other** arm — 41 stored bytes including the
/// trailing newline the composed buy list's first item then follows.
pub const PURCHASE_PRICE: &str = "The purchase price for this dwelling is:\n";
/// The rent-payment prefix, 6 stored bytes.
pub const RENT: &str = "Rent:\n";
/// The buy-time line — 8 bytes, trailing space and no newline.
pub const BOUGHT: &str = "Bought: ";
/// The rent-times display, first row — 30 bytes.
pub const PERIOD_ENDS: &str = "This maintenance period ends: ";
/// The rent-times display, second row — 25 bytes.
pub const NEXT_DUE: &str = "Maintenance is next due: ";
/// The location display's `sprintf` format — 24 bytes.
///
/// See [`location_line`] for the four arguments. Their logical order differs from their stack
/// order.
pub const LOCATION_FORMAT: &str = "Location: %.1f%s, %.1f%s";
/// The house time conversion's zero arm.
pub const TIME_NA: &str = "N/A";
/// The maintenance-warning display's paid arm, 100 stored bytes.
pub const MAINTENANCE_ALREADY_PAID: &str = "The maintenance has already been paid for this \
                                            period. You may not prepay next period's maintenance.";
/// The purchase-time text, the not-yet-expired arm — 39 bytes.
pub const BUY_LANDSCAPE_AT: &str = "You may buy another landscape house at ";
/// …and its suffix — 48 bytes.
pub const APARTMENT_EXEMPTION: &str = ". This restriction does not apply to apartments.";
/// The expired-with-no-house arm, 38 stored bytes.
pub const BUY_IMMEDIATELY: &str = "You may buy another house immediately.";
/// The expired-with-a-house arm, 65 stored bytes.
pub const BUY_AFTER_ABANDON: &str =
    "You may buy another house immediately after you abandon this one.";

/// The client's constant, and the offset
/// the purchase-time section adds to int quality `0xC7` — thirty days.
///
/// Defined in [`pyreal_client_contract::panels::house`], because `pyreal_client_runtime::hud` is
/// what adds it.
pub use pyreal_client_contract::panels::house::PURCHASE_WAIT_SECONDS;

/// `"N/A"` for zero, otherwise `strftime("%c")`.
///
/// Zero gives `"N/A"` without calling `localtime`; otherwise `localtime` runs, and a `NULL`
/// result leaves the string untouched, else `strftime(buf, 0x200, "%c", tm)` fills it.
///
/// It takes the zone offset and renders through [`ctime::strftime_c`] — see the module
/// header.
///
/// The client's third arm matters too: MSVC's
/// `localtime` returns `NULL` for a negative `time_t`, and the client then builds no string,
/// so retail draws **nothing at all** — not `"N/A"`. A negative buy time is not a state the
/// shard produces, but retail has the arm and it costs two lines.
#[must_use]
pub fn convert_time(unix_time: i64, utc_offset_secs: ctime::UtcOffsetSecs) -> String {
    if unix_time == 0 {
        TIME_NA.to_owned()
    } else if unix_time < 0 {
        String::new()
    } else {
        ctime::strftime_c(unix_time, utc_offset_secs)
    }
}

/// The client's location formatting, once the house's landblock coordinates are known.
///
/// Each lcoord at or above `0x400` gets `E` (east-west) or `N` (north-south), otherwise `W` or
/// `S`; its value is `|(lcoord - 1024) * 0.1 + 0.5|`, and the result is
/// `sprintf(buf, fmt, ns_value, ns_letter, ew_value, ew_letter)` — latitude first, which is how
/// retail reads out coordinates. The absolute value is taken *after* the `+ 0.5`, so a
/// coordinate west of 1024 prints its magnitude with a `W`.
///
/// It stays in the `double`s `sprintf`'s varargs require, with the `fabs`.
#[must_use]
pub fn location_line(lx: i32, ly: i32) -> String {
    #[allow(clippy::suboptimal_flops)] // LINT-OK: multiply then add, as retail; two roundings.
    let axis = |v: i32| (f64::from(v - 1024) * 0.1 + 0.5).abs();
    let ew = if lx >= 0x400 { "E" } else { "W" };
    let ns = if ly >= 0x400 { "N" } else { "S" };
    format!("Location: {:.1}{}, {:.1}{}", axis(ly), ns, axis(lx), ew)
}

/// The no-house or purchase-price row: [`NO_HOUSE`] with no data, else [`PURCHASE_PRICE`] and the
/// composed buy list, as one row.
#[must_use]
pub fn buy_payment_line(data: Option<&HouseDataView>) -> String {
    match data {
        None => NO_HOUSE.to_owned(),
        Some(d) => format!("{PURCHASE_PRICE}{}", d.buy_text),
    }
}

/// `"Rent:\n"` and the composed rent list.
#[must_use]
pub fn rent_payment_line(data: &HouseDataView) -> String {
    format!("{RENT}{}", data.rent_text)
}

/// `"Bought: "` and [`convert_time`] of the buy time.
#[must_use]
pub fn buy_time_line(data: &HouseDataView) -> String {
    format!(
        "{BOUGHT}{}",
        convert_time(data.buy_time, data.utc_offset_secs[0])
    )
}

/// The maintenance period's end, then when it is next due (the period doubled when nothing
/// more is owed).
#[must_use]
pub fn rent_time_lines(data: &HouseDataView) -> [String; 2] {
    [
        format!(
            "{PERIOD_ENDS}{}",
            convert_time(data.maintenance_period_end, data.utc_offset_secs[1])
        ),
        format!(
            "{NEXT_DUE}{}",
            convert_time(data.maintenance_next_due, data.utc_offset_secs[2])
        ),
    ]
}

/// The rent warning while rent is owed, else the already-paid sentence, with the colour each is
/// drawn in.
#[must_use]
pub fn warning_line(data: &HouseDataView) -> (String, HousePanelTextColor) {
    if data.rent_owed {
        (data.rent_warning.clone(), HousePanelTextColor::RentNotPaid)
    } else {
        (
            MAINTENANCE_ALREADY_PAID.to_owned(),
            HousePanelTextColor::RentPaid,
        )
    }
}

/// The purchase-wait line; `None` without a player description.
#[must_use]
pub fn purchase_time_line(
    data: Option<&HouseDataView>,
    purchase: HousePurchaseView,
) -> Option<String> {
    if !purchase.have_player_desc {
        return None;
    }
    Some(if purchase.wait_expired {
        if data.is_some() {
            BUY_AFTER_ABANDON
        } else {
            BUY_IMMEDIATELY
        }
        .to_owned()
    } else {
        // `0x278D00` — thirty days on to the quality, then `localtime` and `strftime("%c")`.
        // Not [`convert_time`]: this arm formats inline and has no `"N/A"` and no NULL check.
        let when = ctime::strftime_c(
            i64::from(purchase.purchase_timestamp) + PURCHASE_WAIT_SECONDS,
            purchase.utc_offset_secs,
        );
        format!("{BUY_LANDSCAPE_AT}{when}{APARTMENT_EXEMPTION}")
    })
}

/// Every row of the house pane, in the client's order, with its colour: what any interface's
/// house pane shows.
#[must_use]
pub fn house_lines(
    data: Option<&HouseDataView>,
    purchase: HousePurchaseView,
) -> Vec<(String, HousePanelTextColor)> {
    let normal = |s: String| (s, HousePanelTextColor::Normal);
    let mut out = vec![normal(buy_payment_line(data))];
    if let Some(d) = data {
        out.push(normal(rent_payment_line(d)));
        out.push(normal(buy_time_line(d)));
        out.extend(rent_time_lines(d).into_iter().map(normal));
        if let Some((lx, ly)) = d.location {
            out.push(normal(location_line(lx, ly)));
        }
        out.push(warning_line(d));
    }
    if let Some(line) = purchase_time_line(data, purchase) {
        out.push(normal(line));
    }
    out
}

/// `HousePanelTextColor` — the colour each row is given, passed through to the row text's
/// colour argument.
///
/// The three values are **indices into the row template's own font-colour array**, element
/// property `0x1B`, and the shipped `0x100001E6` template carries exactly three entries
/// [measured on the shipped layout]:
///
/// | index | variant | colour | drawn by |
/// |---|---|---|---|
/// | 0 | [`Self::Normal`] | `0xFFFFFFFF` white | every row but the warning |
/// | 1 | [`Self::RentPaid`] | **`0xFF00FF00` green** | the already-paid line |
/// | 2 | [`Self::RentNotPaid`] | `0xFFFFFF00` yellow | the rent warning |
///
/// The index must reach the drawn row: otherwise *"The maintenance
/// has already been paid for this period"* draws in the element's default white where retail
/// draws it green. `as u8` on this enum is the index the client passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HousePanelTextColor {
    #[default]
    Normal = 0,
    RentPaid = 1,
    RentNotPaid = 2,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the retail string quoted in the module header. A paraphrase is the failure this
    /// asserts against — the rule that a formatted/concatenated string is reproduced exactly,
    /// never paraphrased.
    #[test]
    fn the_houseless_line_is_the_string_in_the_image() {
        assert_eq!(NO_HOUSE, "You do not currently own a house.");
        assert_eq!(NO_HOUSE.len(), 33, "33 bytes before the NUL ");
        assert!(
            !NO_HOUSE.ends_with('\n'),
            "the literal has no newline; `Rent:\\n` next door does"
        );
    }

    /// Every literal is the string in the image.
    #[test]
    fn every_literal_is_the_string_in_the_image() {
        for (s, n) in [
            (PURCHASE_PRICE, 41),
            (RENT, 6),
            (BOUGHT, 8),
            (NEXT_DUE, 25),
            (PERIOD_ENDS, 30),
            (LOCATION_FORMAT, 24),
            (MAINTENANCE_ALREADY_PAID, 100),
            (BUY_LANDSCAPE_AT, 39),
            (APARTMENT_EXEMPTION, 48),
            (BUY_IMMEDIATELY, 38),
            (BUY_AFTER_ABANDON, 65),
            (TIME_NA, 3),
        ] {
            assert_eq!(s.len(), n, "{s:?}");
        }
        assert!(
            PURCHASE_PRICE.ends_with('\n'),
            "the buy list follows straight after it"
        );
        assert!(BOUGHT.ends_with(' ') && !BOUGHT.ends_with('\n'));
        assert_eq!(PURCHASE_WAIT_SECONDS, 2_592_000);
    }

    /// The location line prints latitude first and takes the magnitude.
    #[test]
    fn the_location_line_prints_latitude_first_and_takes_the_magnitude() {
        // 1024 is the origin. `(1024-1024)*0.1 + 0.5 = 0.5`, and `>= 0x400` is N and E.
        assert_eq!(location_line(1024, 1024), "Location: 0.5N, 0.5E");
        // A cell south and west of it: both letters flip and both numbers come back positive.
        assert_eq!(location_line(0, 0), "Location: 101.9S, 101.9W");
        // Latitude is the FIRST number: this pair is unambiguous.
        assert_eq!(location_line(1224, 1524), "Location: 50.5N, 20.5E");
        assert_eq!(
            location_line(1023, 1023),
            "Location: 0.4S, 0.4W",
            "1023 is west, not east"
        );
    }

    /// Convert time has a sentinel for zero.
    #[test]
    fn convert_time_has_a_sentinel_for_zero() {
        assert_eq!(convert_time(0, 0), "N/A");
        // `strftime("%c")` under `English_United States.1252`, which is what `setlocale(LC_ALL,
        // "English")` leaves the CRT in. Measured in the retail install's `msvcr70.dll`.
        assert_eq!(convert_time(1_600_000_000, 0), "9/13/2020 12:26:40 PM");
        // The zone is a real parameter and not decoration: seven hours west moves the clock.
        assert_eq!(
            convert_time(1_600_000_000, -7 * 3600),
            "9/13/2020 5:26:40 AM"
        );
        // The client's NULL-`tm` arm: a negative instant draws nothing at all, not "N/A".
        assert_eq!(convert_time(-1, 0), "");
    }
}
