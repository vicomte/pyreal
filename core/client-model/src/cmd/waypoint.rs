//! `@way` (also `/way` and `@waypoint`), this client's own waypoint command. Retail has none; the
//! Decal plugins' arrows (GoArrow's) had one, and it reads places the way they did.
//!
//! A place is written as the game writes one, `41.6N 33.9E`, the two halves in either order, with
//! or without the space or a comma between them. Words after it name the waypoint. Nothing is
//! sent: the interface keeps the waypoint and points the way to it. This module holds the
//! command's words.

/// What a typed `@way` asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum WaypointCommand {
    /// Say where the waypoint is, or that there is none.
    Show,
    /// Point the way to `ns`, `ew` (north and east positive), under `name` if one was given.
    Set {
        ns: f64,
        ew: f64,
        name: Option<String>,
    },
    /// Stop pointing the way.
    Clear,
}

/// How far from the middle of the map a place may be, either way, in the game's coordinates.
pub const FURTHEST: f64 = 102.0;

/// The longest name a waypoint keeps, in characters.
pub const LONGEST_NAME: usize = 60;

/// What `@way` takes: the tail of every refusal.
pub const USAGE: &str = "Type @way and a place such as 41.6N 33.9E, with a name after it if you \
                         like. Type @way clear to clear it.";

/// The answer when the interface keeps no waypoints.
pub const NO_INTERFACE: &str = "Waypoints need the interface.";

/// Read a typed `@way`'s words, joined as they were typed. `Err` is the one line that says what
/// was wrong.
///
/// # Errors
///
/// Words that hold no place, or a place off the map.
pub fn parse(line: &str) -> Result<WaypointCommand, String> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(WaypointCommand::Show);
    }
    if ["clear", "off", "none", "reset"]
        .iter()
        .any(|w| line.eq_ignore_ascii_case(w))
    {
        return Ok(WaypointCommand::Clear);
    }
    let Some(((ns, ew), rest)) = read_place(line) else {
        return Err(format!("\"{line}\" is not a place. {USAGE}"));
    };
    if ns.abs() > FURTHEST || ew.abs() > FURTHEST {
        return Err(format!("{} is off the map. {USAGE}", place_text(ns, ew)));
    }
    let name = rest
        .trim_start_matches(|c: char| c == ',' || c.is_whitespace())
        .trim();
    let name = (!name.is_empty()).then(|| name.chars().take(LONGEST_NAME).collect());
    Ok(WaypointCommand::Set { ns, ew, name })
}

/// The place at the start of `text`, `(ns, ew)`, and what follows it; `None` when it starts with
/// no place. Either half may come first, and a comma or spaces may stand between them.
#[must_use]
pub fn read_place(text: &str) -> Option<((f64, f64), &str)> {
    let (first, rest) = read_half(text)?;
    let rest = rest.trim_start_matches(|c: char| c == ',' || c.is_whitespace());
    let (second, rest) = read_half(rest)?;
    match (first, second) {
        (Half::Ns(ns), Half::Ew(ew)) | (Half::Ew(ew), Half::Ns(ns)) => Some(((ns, ew), rest)),
        _ => None,
    }
}

/// One half of a place: how far north (south negative) or east (west negative).
#[derive(Debug, Clone, Copy)]
enum Half {
    Ns(f64),
    Ew(f64),
}

/// The half of a place at the start of `text` (`41.6N`, `33.9 W`) and what follows it.
fn read_half(text: &str) -> Option<(Half, &str)> {
    let text = text.trim_start();
    let end = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(text.len());
    let value: f64 = text[..end].parse().ok().filter(|v: &f64| v.is_finite())?;
    let after = text[end..].trim_start();
    let letter = after.chars().next()?;
    let rest = &after[letter.len_utf8()..];
    // The letter ends the half: `33.9East` is no place.
    if rest.chars().next().is_some_and(char::is_alphabetic) {
        return None;
    }
    let half = match letter.to_ascii_uppercase() {
        'N' => Half::Ns(value),
        'S' => Half::Ns(-value),
        'E' => Half::Ew(value),
        'W' => Half::Ew(-value),
        _ => return None,
    };
    Some((half, rest))
}

/// `ns`, `ew` as the game writes a place: `41.6N 33.9E`.
#[must_use]
pub fn place_text(ns: f64, ew: f64) -> String {
    format!(
        "{:.1}{} {:.1}{}",
        ns.abs(),
        if ns >= 0.0 { "N" } else { "S" },
        ew.abs(),
        if ew >= 0.0 { "E" } else { "W" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(ns: f64, ew: f64, name: Option<&str>) -> WaypointCommand {
        WaypointCommand::Set {
            ns,
            ew,
            name: name.map(str::to_owned),
        }
    }

    #[test]
    fn way_reads_a_place_as_the_game_writes_one_in_either_order_with_a_name_after_it() {
        assert_eq!(parse(""), Ok(WaypointCommand::Show));
        assert_eq!(parse("  CLEAR "), Ok(WaypointCommand::Clear));
        assert_eq!(parse("41.6N 33.9E"), Ok(set(41.6, 33.9, None)));
        assert_eq!(parse("41.6n, 33.9e"), Ok(set(41.6, 33.9, None)));
        assert_eq!(parse("41.6N33.9E"), Ok(set(41.6, 33.9, None)));
        assert_eq!(parse("33.9 W 41.6 S"), Ok(set(-41.6, -33.9, None)));
        assert_eq!(
            parse("42.1N 33.6E Holtburg lifestone"),
            Ok(set(42.1, 33.6, Some("Holtburg lifestone")))
        );
        assert_eq!(
            parse("0N 0E, the middle"),
            Ok(set(0.0, 0.0, Some("the middle")))
        );
        for line in ["Holtburg", "41.6N", "41.6N 33.9N", "41.6North 33.9E", "N E"] {
            let refusal = parse(line).expect_err(line);
            assert!(refusal.ends_with(USAGE), "{line}: {refusal}");
        }
        let off = parse("120N 3E").unwrap_err();
        assert!(off.starts_with("120.0N 3.0E is off the map."), "{off}");
        assert_eq!(place_text(-0.04, -33.95), "0.0S 34.0W");
    }

    #[test]
    fn way_and_waypoint_are_this_clients_own_commands() {
        use crate::cmd::{CommandHandler, CommandInterp, CommandOutcome, TalkFocus};
        let mut c = CommandInterp::new();
        c.add_turbine_chat_commands();
        for line in [
            "/way 41.6N 33.9E",
            "@WAY 41.6N 33.9E",
            "/waypoint 41.6N 33.9E",
        ] {
            match c.on_chat_command(line, 1, TalkFocus::Say) {
                CommandOutcome::Handled { handler, args, .. } => {
                    assert_eq!(handler, CommandHandler::Waypoint, "{line}");
                    assert_eq!(args, vec!["41.6N", "33.9E"], "{line}");
                }
                other => panic!("{line}: {other:?}"),
            }
        }
        let help = c.do_help(&["way".to_owned()]);
        assert!(
            help.iter().any(|(l, _)| l.contains("@way clear")),
            "{help:?}"
        );
    }
}
