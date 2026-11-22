//! The character information sheet's rules: the load section's tokens and variable names, the
//! burden penalty, and the luminance and augmentation quality tables.
//!
//! The section text is composed out of the end-of-retail string tables by the interface, over
//! whichever string service it has; these are the tokens and the names it composes with.

/// The `StringInfo` tokens the load section composes.
pub mod string {
    pub const LOAD_NONE: &str = "ID_CharacterInfo_Load_None";
    /// `%Burden` and `%BurdenPenalty`.
    pub const LOAD_BURDENED: &str = "ID_CharacterInfo_Load_Burdened";
    /// `%NumAugmentations` and `%AdditionalLoad`.
    pub const LOAD_AUGMENTATIONS: &str = "ID_CharacterInfo_Load_Augmentations";
}

/// The **variable names** those rows file their values under — the key each value is added
/// under, which is a string hash of the name and never a slot number.
///
/// The client walks the *row's* own variable list and looks each id up in the caller's table, so
/// these names are the whole of what places a value and the order this file writes them in is
/// invisible. Every one is confirmed against the shipped rows' own variable lists.
pub mod var {
    /// The augmentation-count variable. Every luminance and augmentation row uses this one,
    /// and the 24 of them that ship with no variable at all never look it up.
    pub const NUM_AUGMENTATIONS: &str = "NUM_AUGMENTATIONS";
    /// The burden variable.
    pub const BURDEN: &str = "BURDEN";
    /// The burden-penalty variable.
    pub const PENALTY: &str = "PENALTY";
    /// The additional-load variable.
    pub const ADDITIONAL_LOAD: &str = "ADDITIONAL_LOAD";
}

/// The int quality ids `CharacterInfoPanel` reads that are not already fields of
/// [`pyreal_client_contract::view::CharacterInfo`].
///
/// Defined in [`pyreal_client_contract::panels::characterinfo::prop`], because
/// the runtime's HUD is what asks the qualities.
pub use pyreal_client_contract::panels::characterinfo::prop;

/// The 43 augmentation rows, in retail's order: `(quality id, token)`, each behind `if (v > 0)`.
///
/// Every one of them gets an augmentation-count integer variable in retail, including the
/// twenty whose shipped row has **no** variable slot at all (*"You earned the Infused War Magic
/// augmentation…"*). Retail's string resolution builds its value list from the
/// row's own variable list, so a variable the row does not name is never read — and
/// the retired UI framework's `UiSystem::resolve_string_named` does not look it up for the same reason. That is
/// why this table has no "has a variable" column: the client does not have one either. Measured:
/// 24 of the 54 luminance and augmentation rows ship with no
/// variables at all, and `ID_CharacterInfo_Augmentation_JackOfAllTrades` is the pinned control.
///
/// `Resist_Nether` (`0x147`) is the one token with **no row in the shipped
/// `client_local_English.dat`** [measured against the shipped dat]. It stays in
/// the table because the client reads the quality and asks for the string; what a player sees is
/// whatever the missing-string path shows, and inventing a sentence for it would be worse.
pub const AUGMENTATIONS: &[(u32, &str)] = &[
    (0xDA, "ID_CharacterInfo_Augmentation_Attribute_Strength"),
    (0xDB, "ID_CharacterInfo_Augmentation_Attribute_Endurance"),
    (0xDC, "ID_CharacterInfo_Augmentation_Attribute_Coordination"),
    (0xDD, "ID_CharacterInfo_Augmentation_Attribute_Quickness"),
    (0xDE, "ID_CharacterInfo_Augmentation_Attribute_Focus"),
    (0xDF, "ID_CharacterInfo_Augmentation_Attribute_Self"),
    (0xF0, "ID_CharacterInfo_Augmentation_Resist_Slash"),
    (0xF1, "ID_CharacterInfo_Augmentation_Resist_Pierce"),
    (0xF2, "ID_CharacterInfo_Augmentation_Resist_Blunt"),
    (0xF3, "ID_CharacterInfo_Augmentation_Resist_Acid"),
    (0x147, "ID_CharacterInfo_Augmentation_Resist_Nether"),
    (0xF4, "ID_CharacterInfo_Augmentation_Resist_Fire"),
    (0xF5, "ID_CharacterInfo_Augmentation_Resist_Frost"),
    (0xF6, "ID_CharacterInfo_Augmentation_Resist_Lightning"),
    (0xE0, "ID_CharacterInfo_Augmentation_Spec_Salvaging"),
    (0xE1, "ID_CharacterInfo_Augmentation_Spec_ItemTinkering"),
    (0xE2, "ID_CharacterInfo_Augmentation_Spec_ArmorTinkering"),
    (
        0xE3,
        "ID_CharacterInfo_Augmentation_Spec_MagicItemTinkering",
    ),
    (0xE4, "ID_CharacterInfo_Augmentation_Spec_WeaponTinkering"),
    (0x125, "ID_CharacterInfo_Augmentation_Spec_Gearcraft"),
    (0xE5, "ID_CharacterInfo_Augmentation_ExtraPackSlot"),
    (
        0xE6,
        "ID_CharacterInfo_Augmentation_IncreasedCarryingCapacity",
    ),
    (0xE7, "ID_CharacterInfo_Augmentation_LessDeathItemLoss"),
    (0xE8, "ID_CharacterInfo_Augmentation_SpellsRemainPastDeath"),
    (0xE9, "ID_CharacterInfo_Augmentation_CriticalDefense"),
    (0xEA, "ID_CharacterInfo_Augmentation_BonusXP"),
    (0xEB, "ID_CharacterInfo_Augmentation_BonusSalvage"),
    (0xEC, "ID_CharacterInfo_Augmentation_BonusImbueChance"),
    (0xED, "ID_CharacterInfo_Augmentation_FasterRegen"),
    (0xEE, "ID_CharacterInfo_Augmentation_IncreasedSpellDuration"),
    (0x126, "ID_CharacterInfo_Augmentation_Infused_CreatureMagic"),
    (0x127, "ID_CharacterInfo_Augmentation_Infused_ItemMagic"),
    (0x128, "ID_CharacterInfo_Augmentation_Infused_LifeMagic"),
    (0x129, "ID_CharacterInfo_Augmentation_Infused_WarMagic"),
    (0x148, "ID_CharacterInfo_Augmentation_Infused_VoidMagic"),
    (0x12C, "ID_CharacterInfo_Augmentation_SkilledMelee"),
    (0x12D, "ID_CharacterInfo_Augmentation_SkilledMissile"),
    (0x12E, "ID_CharacterInfo_Augmentation_SkilledMagic"),
    (0x135, "ID_CharacterInfo_Augmentation_DamageBonus"),
    (0x136, "ID_CharacterInfo_Augmentation_DamageResist"),
    (0x12A, "ID_CharacterInfo_Augmentation_CriticalExpertise"),
    (0x12B, "ID_CharacterInfo_Augmentation_CriticalPower"),
    (0x146, "ID_CharacterInfo_Augmentation_JackOfAllTrades"),
];

/// The eleven luminance ratings, in retail's order: `(quality id, base token, specialised token)`.
///
/// The four rows carrying a `Some(spec)` are the compare-against-5 pairs: the first five points are the
/// base aura and everything past five is the Seer's, and **both** lines can appear. The seven
/// with `None` are a single `if (v > 0)`.
///
/// `ID_CharacterInfo_Luminance_Base_Mana_Gain` is the shipped row's own spelling of *"The mana
/// provide by Mana Stones"*; it is quoted, not corrected.
pub const LUMINANCE: &[(u32, &str, Option<&str>)] = &[
    (
        0x14D,
        "ID_CharacterInfo_Luminance_Base_Damage",
        Some("ID_CharacterInfo_Luminance_Spec_Damage"),
    ),
    (
        0x14E,
        "ID_CharacterInfo_Luminance_Base_Reduction",
        Some("ID_CharacterInfo_Luminance_Spec_Reduction"),
    ),
    (
        0x14F,
        "ID_CharacterInfo_Luminance_Base_Crit_Damage",
        Some("ID_CharacterInfo_Luminance_Spec_Crit_Damage"),
    ),
    (
        0x150,
        "ID_CharacterInfo_Luminance_Base_Crit_Reduction",
        Some("ID_CharacterInfo_Luminance_Spec_Crit_Reduction"),
    ),
    (0x152, "ID_CharacterInfo_Luminance_Base_Surge_Chance", None),
    (0x153, "ID_CharacterInfo_Luminance_Base_Mana_Use", None),
    (0x154, "ID_CharacterInfo_Luminance_Base_Mana_Gain", None),
    (0x156, "ID_CharacterInfo_Luminance_Base_Healing", None),
    (0x157, "ID_CharacterInfo_Luminance_Base_Skilled_Craft", None),
    (0x158, "ID_CharacterInfo_Luminance_Spec_Skilled_Spec", None),
    (0x16D, "ID_CharacterInfo_Luminance_Base_All_Skills", None),
];

/// The encumbrance system's load mod, then `(10 - (int)(mod * 10)) * 10`.
///
/// The percentage `ID_CharacterInfo_Load_Burdened` shows as *"reducing your Run, Jump, Melee
/// Defense and Missile Defense skills by N%"*. A load of exactly `1.0` gives `0`, `1.5` gives
/// `50`, and anything at or over `2.0` gives `100`.
///
/// The load mod is [`pyreal_rules::burden::load_mod`], the one the run rate uses. The client
/// multiplies it by ten in double precision and truncates the product without storing it as a
/// float first; the end-of-retail client and the early ones do the same.
#[must_use]
pub fn burden_penalty_percent(load: f32) -> i32 {
    let load_mod = pyreal_rules::burden::load_mod(load);
    let tenths = pyreal_primitives::num::to_i32_f64(f64::from(load_mod) * 10.0);
    (10 - tenths) * 10
}

/// A number as every integer variable on the sheet renders it: the shipped digit grouping.
#[must_use]
pub fn num(v: impl Into<i128>) -> String {
    crate::numfmt::language_number(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the client's `(10 - trunc(load_mod(load) * 10.0)) * 10`, the product in double
    /// precision, and the load modifier's three branches.
    #[test]
    fn the_burden_penalty_is_ten_minus_the_load_mod_in_tenths_times_ten() {
        assert_eq!(
            burden_penalty_percent(0.5),
            0,
            "under capacity: the load mod is 1.0"
        );
        assert_eq!(
            burden_penalty_percent(1.0),
            0,
            "exactly at capacity, and still 0%"
        );
        assert_eq!(burden_penalty_percent(1.5), 50, "0.5 is exact in f32");
        // **The truncation is the client's and it is visible.** `2.0f - 1.2f` is `0.79999995`,
        // `* 10.0f` is `7.9999995`, and the truncation takes **7** -- so a load of 1.2 costs 30%, not
        // the 20% an `f64` implementation would print. Pinned because it is the one place this
        // function can silently disagree with retail.
        assert_eq!(burden_penalty_percent(1.2), 30);
        // **The product is a double.** A load of 110% is `1.1f`, its mod `0.9f` is `0.89999998`,
        // and ten of those is `8.9999998` in double precision: the truncation takes **8**, so the
        // sheet says 20%. Rounded to a float first, the product would be `9.0` and the sheet 10%.
        // Of every float load from 1 to 2 this is the one where the two widths disagree.
        assert_eq!(
            burden_penalty_percent(pyreal_rules::burden::load(100, 110)),
            20
        );
        assert_eq!(
            burden_penalty_percent(1.9),
            90,
            "2.0f - 1.9f is 0.100000024, so (int)1"
        );
        assert_eq!(
            burden_penalty_percent(2.0),
            100,
            "the load mod clamps to 0 at twice capacity"
        );
        assert_eq!(burden_penalty_percent(5.0), 100);
    }
}
