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

/// The log window's colour for one of the game's chat types: speech, tells, the fellowship, the
/// allegiance, the chat channels and the game's own messages each in a colour of their own.
#[must_use]
pub fn chat_colour(chat_type: u8) -> Argb {
    match chat_type {
        // Speech: say.
        2 => 0xFFF7_F7F7,
        // Tells, both ways.
        3 | 4 | 31 => 0xFFFF_B8DE,
        // System messages.
        5 => 0xFFCC_CCCC,
        // Combat: the battle log's colours.
        6 | 21 => 0xFFFF_7B7B,
        22 => 0xFFFF_A0A0,
        // Magic and spellcasting.
        7 | 17 => 0xFFA0_C8FF,
        // The chat channels: green.
        0 | 1 | 8 | 9 | 20 => 0xFFD4_FF7D,
        // Social: shout.
        10 | 11 => 0xFFFF_A666,
        // Emotes.
        12 => 0xFFBA_FFF0,
        // Advancement: the level-up gold.
        13 => 0xFFFF_DE73,
        // Allegiance.
        18 | 33 => 0xFFAB_DBE5,
        // Fellowship: party.
        19 => 0xFF66_E5FF,
        // Client feedback and errors.
        15 | 26 => 0xFFFF_4A4A,
        _ => 0xFFCC_CCCC,
    }
}
