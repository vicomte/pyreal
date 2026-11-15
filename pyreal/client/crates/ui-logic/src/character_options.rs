//! The character options' caption and tooltip tokens: what each player option is called in the
//! string table its page reads.

use pyreal_client_contract::view::PlayerOption;

/// The string table enum writes both of its ids in.
pub const STRING_TABLE_ENUM: u32 = 0x1000_0003;

/// The prefix builds every caption token from.
pub const TOKEN_PREFIX: &str = "ID_PlayerOption_";
/// The suffix that turns a caption token into its tooltip token.
pub const HELP_SUFFIX: &str = "_Help";

/// The name the option initialisation spells into each token, per [`PlayerOption`].
///
/// This is a **transcription of the client's 50 option-name strings**, not a `Debug` rendering:
/// they are the client's own spellings and one of them differs from the obvious one
/// (`ID_PlayerOption_FellowshipShareXP`, capital `XP`). `tests::the_page_names_every_option_the_way_the_client_spells_it`
/// pins a sample of them as literals.
#[must_use]
pub const fn option_name(o: PlayerOption) -> &'static str {
    use PlayerOption as P;
    match o {
        P::ViewCombatTarget => "ViewCombatTarget",
        P::SalvageMultiple => "SalvageMultiple",
        P::MainPackPreferred => "MainPackPreferred",
        P::VividTargetingIndicator => "VividTargetingIndicator",
        P::ShowTooltips => "ShowTooltips",
        P::CoordinatesOnRadar => "CoordinatesOnRadar",
        P::SideBySideVitals => "SideBySideVitals",
        P::SpellDuration => "SpellDuration",
        P::DisableMostWeatherEffects => "DisableMostWeatherEffects",
        P::DisableDistanceFog => "DisableDistanceFog",
        P::PersistentAtDay => "PersistentAtDay",
        P::DisableHouseRestrictionEffects => "DisableHouseRestrictionEffects",
        P::UseCraftSuccessDialog => "UseCraftSuccessDialog",
        P::ConfirmVolatileRareUse => "ConfirmVolatileRareUse",
        P::DisplayTimeStamps => "DisplayTimeStamps",
        P::FilterLanguage => "FilterLanguage",
        P::ShowHelm => "ShowHelm",
        P::ShowCloak => "ShowCloak",
        P::IgnoreAllegianceRequests => "IgnoreAllegianceRequests",
        P::IgnoreFellowshipRequests => "IgnoreFellowshipRequests",
        P::DisplayAllegianceLogonNotifications => "DisplayAllegianceLogonNotifications",
        P::FellowshipShareXP => "FellowshipShareXP",
        P::FellowshipShareLoot => "FellowshipShareLoot",
        P::FellowshipAutoAcceptRequests => "FellowshipAutoAcceptRequests",
        P::AcceptLootPermits => "AcceptLootPermits",
        P::UseDeception => "UseDeception",
        P::AllowGive => "AllowGive",
        P::IgnoreTradeRequests => "IgnoreTradeRequests",
        P::DragItemOnPlayerOpensSecureTrade => "DragItemOnPlayerOpensSecureTrade",
        P::DisplayDateOfBirth => "DisplayDateOfBirth",
        P::DisplayAge => "DisplayAge",
        P::DisplayChessRank => "DisplayChessRank",
        P::DisplayFishingSkill => "DisplayFishingSkill",
        P::DisplayNumberDeaths => "DisplayNumberDeaths",
        P::DisplayNumberCharacterTitles => "DisplayNumberCharacterTitles",
        P::ToggleRun => "ToggleRun",
        P::AdvancedCombatUI => "AdvancedCombatUI",
        P::AutoTarget => "AutoTarget",
        P::AutoRepeatAttack => "AutoRepeatAttack",
        P::UseChargeAttack => "UseChargeAttack",
        P::LeadMissileTargets => "LeadMissileTargets",
        P::UseFastMissiles => "UseFastMissiles",
        P::StayInChatMode => "StayInChatMode",
        P::HearAllegianceChat => "HearAllegianceChat",
        P::HearGeneralChat => "HearGeneralChat",
        P::HearTradeChat => "HearTradeChat",
        P::HearLFGChat => "HearLFGChat",
        P::HearRoleplayChat => "HearRoleplayChat",
        P::HearSocietyChat => "HearSocietyChat",
        P::HearPKDeaths => "HearPKDeaths",
        P::LockUI => "LockUI",
        // Not an option-initialisation row. The name is the client's enum name, and it is carried here so
        // the one producer of option names stays one -- but `ID_PlayerOption_AppearOffline` is
        // **not in string table `0x10000003`**, and the friends panel never hashes
        // it: that box's caption is authored in the layout. See
        // [`super::toggle::Caption::FromLayout`].
        P::AppearOffline => "AppearOffline",
    }
}

/// `"ID_PlayerOption_<Name>"`, hashed for the caption — the caption token.
#[must_use]
pub fn label_token(o: PlayerOption) -> String {
    format!("{TOKEN_PREFIX}{}", option_name(o))
}

/// `"ID_PlayerOption_<Name>_Help"`, hashed for the tooltip — the tooltip token.
#[must_use]
pub fn help_token(o: PlayerOption) -> String {
    format!("{TOKEN_PREFIX}{}{HELP_SUFFIX}", option_name(o))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sample is quoted as literals rather than assembled from the enum, because a token built
    /// by concatenation and checked by concatenation cannot catch a wrong spelling —
    /// the stated testability rule. `FellowshipShareXP` is here specifically because its capitalisation is the
    /// one that is not obvious.
    #[test]
    fn the_page_names_every_option_the_way_the_client_spells_it() {
        use PlayerOption as P;
        assert_eq!(
            label_token(P::ViewCombatTarget),
            "ID_PlayerOption_ViewCombatTarget"
        );
        assert_eq!(
            help_token(P::ViewCombatTarget),
            "ID_PlayerOption_ViewCombatTarget_Help"
        );
        assert_eq!(
            label_token(P::FellowshipShareXP),
            "ID_PlayerOption_FellowshipShareXP"
        );
        assert_eq!(
            help_token(P::DisplayNumberCharacterTitles),
            "ID_PlayerOption_DisplayNumberCharacterTitles_Help"
        );
        assert_eq!(label_token(P::HearLFGChat), "ID_PlayerOption_HearLFGChat");
        assert_eq!(
            label_token(P::AdvancedCombatUI),
            "ID_PlayerOption_AdvancedCombatUI"
        );
        assert_eq!(
            label_token(P::DragItemOnPlayerOpensSecureTrade),
            "ID_PlayerOption_DragItemOnPlayerOpensSecureTrade"
        );
        assert_eq!(TOKEN_PREFIX, "ID_PlayerOption_");
        assert_eq!(HELP_SUFFIX, "_Help");
        //  writes both ids in table enum 0x10000003 — the same one the
        // preference registry uses, and **not** the 0x10000004 the key-binding rows use.
        assert_eq!(STRING_TABLE_ENUM, 0x1000_0003);
    }
}
