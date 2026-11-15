//! The words and ids the game's panels and dialogs are written with: string-table tokens, the
//! table enums they are read from, and the literal texts.

/// The end-character-session question.
pub mod logout {
    /// The table enum the end-character-session and log-off handlers build their
    /// `StringInfo` in — **`0x10000001`**, not the `0x10000002` every pre-game dialog uses.
    pub const STRING_TABLE_ENUM: u32 = 0x1000_0001;
    /// The client's string, verified against the shipped
    /// table: *"This will exit your character from the game world."* then *"Are you sure?"*,
    /// which is the sentence in the paired retail frame.
    pub const END_SESSION_CONFIRM: &str = "ID_Client_EndCharacterSessionConfirm";
}

/// The abuse report's answers.
pub mod abuse {
    /// The string-table enum the page-three text is written with.
    pub const STRING_TABLE_ENUM: u32 = 0x1000_0001;

    /// The three error codes handled by the abuse-report-response notice.
    pub mod response {
        pub const NO_SUCH_CHARACTER: u32 = 0x04B8;
        pub const SELF_REPORT: u32 = 0x04B9;
        pub const SUCCESS: u32 = 0x04BA;
    }

    /// The hashed string-id names read by the abuse-report-response notice's three arms.
    #[must_use]
    pub const fn token(code: u32) -> Option<&'static str> {
        match code {
            response::NO_SUCH_CHARACTER => Some("ID_Abuse_Response_NoSuchCharacter"),
            response::SELF_REPORT => Some("ID_Abuse_Response_Self"),
            response::SUCCESS => Some("ID_Abuse_Response_Success"),
            _ => None,
        }
    }
}

/// The allegiance's confirmations, and what an oath costs.
pub mod allegiance {
    use pyreal_client_contract::view::GameView;

    /// `ID_Allegiance_SwearConfirmation`, one `PLAYER` variable — the selected object's name.
    pub const ID_SWEAR_CONFIRMATION: &str = "ID_Allegiance_SwearConfirmation";
    /// `ID_Allegiance_BreakConfirmation`, one `PLAYER` variable — the **patron's** name.
    pub const ID_BREAK_CONFIRMATION: &str = "ID_Allegiance_BreakConfirmation";
    /// `ID_Allegiance_KickConfirmation`, one `PLAYER` variable — the selected **vassal's** name.
    pub const ID_KICK_CONFIRMATION: &str = "ID_Allegiance_KickConfirmation";
    /// `ID_Allegiance_AcceptSwearConfirmation`, one `PLAYER` variable — the name the **server**
    /// sent in `0x0274`'s text field.
    pub const ID_ACCEPT_SWEAR_CONFIRMATION: &str = "ID_Allegiance_AcceptSwearConfirmation";

    /// What an oath costs the player on a world that charges experience for one: nothing before a
    /// first break from a patron, otherwise the shared rule over the player's next-level step and
    /// its break count. `None` on a world without the charge.
    #[must_use]
    pub fn oath_xp_cost(view: &dyn GameView) -> Option<u32> {
        view.oath_xp_cost()
    }

    /// What the prompt to swear adds on a world that charges experience for an oath; the end-of-retail
    /// prompt is unchanged.
    #[must_use]
    pub fn oath_cost_line(view: &dyn GameView) -> Option<String> {
        oath_xp_cost(view).filter(|&c| c > 0).map(|cost| {
            format!(
                "\n\nThis oath costs {} unassigned experience.",
                pyreal_presentation::appraisal::insert_commas(
                    i32::try_from(cost).unwrap_or(i32::MAX)
                )
            )
        })
    }
}

/// The fellowship invitation.
pub mod fellowship {
    /// `ID_Fellowship_FellowshipRequest`, one `PLAYER` variable — the prompt the fellow request
    /// dialog builds for a `0x0274` of type **4**.
    pub const ID_FELLOWSHIP_REQUEST: &str = "ID_Fellowship_FellowshipRequest";
}

/// The housing vendor's words.
pub mod slumlord {
    /// The house refresh's owner line, narrow, 7 characters including the
    /// trailing space.
    pub const OWNER_PREFIX: &str = "Owner: ";
    /// …and its empty-name arm, narrow, 4 characters. The test is that the profile's owner name
    /// has length 1 counting the terminator, i.e. the string is empty.
    pub const OWNER_NONE: &str = "None";

    /// The house-purchase confirmation prompt, wide, 138 characters.
    pub const BUY_CONFIRMATION: &str = "When you buy a landscape house like this one, you are \
                                        restricted from buying another for 30 days. Are you sure you \
                                        want to buy this house?";
    /// The rent payment by proxy confirmation prompt, wide, 86
    /// characters. Note the apostrophe in *"else's"*.
    pub const RENT_BY_PROXY_CONFIRMATION: &str =
        "You are paying maintenance on someone else's house. Are you sure you wish to continue?";
}

/// The magic window's spell-research tab.
pub mod research {
    /// The tab's caption.
    pub const TAB_TEXT: &str = "Create Spell";
}

/// The chess board.
pub mod minigame {
    /// The board's squares.
    pub const CELLS: usize = 64;
}

/// The world map.
pub mod map {
    use pyreal_primitives::DataId;
    /// The player-location icon, the green circle, 17x16.
    pub const PLAYER_ICON: DataId = DataId(0x0600_4D10);
}

/// The chat channel the client's own notices about mouse turning are written to.
pub const MOUSE_TURNING_CHANNEL: u32 = 7;

/// What the mouse-turning preset says of the two keys it rebinds.
pub const MOUSE_TURNING_KEY_MESSAGES: [&str; 2] = [
    "The key for Camera Zoom In has been changed to the mouse wheel up.",
    "The key for Camera Zoom Out has been changed to the mouse wheel down.",
];
