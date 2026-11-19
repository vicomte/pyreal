//! The interface colours: the kit's table of rows ([`pyreal_ui_kit::colours`]), and the log
//! window's colour for each of the game's chat types.

use crate::draw::Argb;

pub use pyreal_ui_kit::colours::{row, Colours};

/// The health bar's colour: retail's red.
pub const HEALTH: Argb = 0xFFC8_1E22;
/// The stamina bar's colour: retail's amber.
pub const STAMINA: Argb = 0xFFE0_9E1C;
/// The mana bar's colour: retail's blue.
pub const MANA: Argb = 0xFF1A_70B4;

/// The health, stamina and mana bars' colours: retail's red, amber and blue.
pub const VITALS: [Argb; 3] = [HEALTH, STAMINA, MANA];

/// The log window's colour for one of the game's chat types, after the defaults players of other
/// games know: say white, tells pink, the fellowship as a party's blue, the allegiance as a guild's
/// green, the chat rooms a channel's pale rose, emotes orange and the game's own notices yellow.
#[must_use]
pub fn chat_colour(chat_type: u8) -> Argb {
    match chat_type {
        // Say.
        2 => 0xFFFF_FFFF,
        // Tells, both ways, and an admin's.
        3 | 4 | 31 => 0xFFFF_80FF,
        // Combat: the battle log's colours.
        6 | 21 => 0xFFFF_7B7B,
        22 => 0xFFFF_A0A0,
        // Magic and spellcasting.
        7 | 17 => 0xFFA0_C8FF,
        // Channels and the chat rooms.
        8 | 9 | 14 | 15 | 27..=30 | 32 => 0xFFFF_C0C0,
        // Patron, monarch, vassals and co-vassals: an officer's green.
        10 | 11 => 0xFF40_C040,
        // Emotes.
        12 => 0xFFFF_8040,
        // Experience and levels.
        13 => 0xFF6F_6FFF,
        // Appraisal.
        16 => 0xFFCC_CCCC,
        // The allegiance: a guild's green.
        18 | 33 => 0xFF40_FF40,
        // The fellowship: a party's blue.
        19 => 0xFFAA_AAFF,
        // Crafting and salvaging.
        24 | 25 => 0xFF55_55FF,
        // Client feedback and errors.
        26 => 0xFFFF_2020,
        // System lines, broadcasts and recalls.
        _ => 0xFFFF_FF00,
    }
}

/// The shade of `colour` the player's own lines take: 43% of the way to grey `#969696`, the step
/// the original game took from its tell yellow (`#FFFF3F`) to its sent-tell tan (`#D2D264`).
#[must_use]
pub fn sent_colour(colour: Argb) -> Argb {
    let shade = |shift: u32| {
        let c = f64::from((colour >> shift) & 0xFF);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let v = (c + (150.0 - c) * 0.43).round() as u32;
        v << shift
    };
    (colour & 0xFF00_0000) | shade(16) | shade(8) | shade(0)
}

/// The chat types people talk on, where a line may be the player's own.
const TALK: [u8; 13] = [2, 3, 4, 8, 9, 10, 11, 18, 19, 27, 28, 29, 30];

/// Whether a talk line is the player's own: a sent type, or one beginning "You say", "You tell",
/// "You think" or `me` saying, after its time stamp and any `[Channel]` tag.
#[must_use]
pub fn is_own_line(chat_type: u8, text: &str, me: &str) -> bool {
    if matches!(chat_type, 4 | 9 | 11) {
        return true;
    }
    if !TALK.contains(&chat_type) && chat_type != 32 {
        return false;
    }
    let mut rest = text;
    if let Some((stamp, after)) = rest.split_once(' ') {
        if !stamp.is_empty() && stamp.chars().all(|c| c.is_ascii_digit() || c == ':') {
            rest = after;
        }
    }
    if let Some(after) = rest.strip_prefix('[').and_then(|r| r.split_once("] ")) {
        rest = after.1;
    }
    let me = me.trim_start_matches(['+', '~']);
    ["You say", "You tell", "You think"]
        .iter()
        .any(|p| rest.starts_with(p))
        || (!me.is_empty()
            && rest
                .trim_start_matches(['+', '~'])
                .strip_prefix(me)
                .is_some_and(|r| r.starts_with(" says")))
}

/// A log line's colour: its chat type's, in the sent shade for the player's own lines.
#[must_use]
pub fn line_colour(chat_type: u8, text: &str, me: &str) -> Argb {
    let colour = chat_colour(chat_type);
    if is_own_line(chat_type, text, me) {
        sent_colour(colour)
    } else {
        colour
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sent_shade_is_the_original_games_step_from_tell_yellow_to_sent_tan() {
        assert_eq!(sent_colour(0xFFFF_FF3F), 0xFFD2_D264);
        assert_eq!(sent_colour(0xFFFF_FFFF), 0xFFD2_D2D2);
        assert_eq!(sent_colour(chat_colour(3)), 0xFFD2_89D2);
    }

    #[test]
    fn the_log_takes_the_familiar_colours_and_both_sides_of_a_tell_share_one() {
        for (ty, colour) in [
            (2, 0xFFFF_FFFF),
            (3, 0xFFFF_80FF),
            (5, 0xFFFF_FF00),
            (12, 0xFFFF_8040),
            (18, 0xFF40_FF40),
            (19, 0xFFAA_AAFF),
            (27, 0xFFFF_C0C0),
        ] {
            assert_eq!(chat_colour(ty), colour, "type {ty}");
        }
        assert_eq!(chat_colour(4), chat_colour(3));
        assert_eq!(chat_colour(9), chat_colour(8));
        assert_eq!(chat_colour(11), chat_colour(10));
    }

    #[test]
    fn own_lines_are_told_from_other_peoples_lines() {
        let me = "+Tester";
        for (ty, text) in [
            (2, "You say, \"hi\""),
            (2, "12:04:09 You say, \"hi\""),
            (4, "You tell Alba, \"hi\""),
            (3, "You think, \"hmm\""),
            (19, "[Fellowship] You say, \"ready\""),
            (11, "You say to your Patron, \"hi\""),
            (9, "You say on the Help channel, \"hi\""),
            (27, "[General] Tester says, \"lfg\""),
            (32, "1:02:03 [Society] +Tester says, \"hi\""),
        ] {
            assert!(is_own_line(ty, text, me), "{ty} {text}");
            assert_eq!(
                line_colour(ty, text, me),
                sent_colour(chat_colour(ty)),
                "{ty} {text}"
            );
        }
        for (ty, text) in [
            (2, "Hob says, \"You say what?\""),
            (3, "Alba tells you, \"hi\""),
            (19, "[Fellowship] Ivo says, \"You say\""),
            (27, "[General] Testers says, \"hi\""),
            (5, "You say nothing; this is a system line"),
            (12, "You wave."),
        ] {
            assert!(!is_own_line(ty, text, me), "{ty} {text}");
            assert_eq!(line_colour(ty, text, me), chat_colour(ty), "{ty} {text}");
        }
    }
}
