//! Spell examination text and renderer-neutral spell icon layers.

/// Metres per yard — the divisor of the range line.
pub const METRES_PER_YARD: f64 = 0.9144;
/// The seconds/minutes fork.
pub const MINUTE_SECONDS: f64 = 60.0;
/// The multiplier used by the minutes-formatting arm, with the client's precision.
pub const RECIPROCAL_MINUTE: f64 = 0.016_666_666_666_666_666;

/// The school names, in school-number order.
///
/// The default arm is `"None"` for anything outside `1..=5`,
/// **not** an empty string.
#[must_use]
pub fn school_name(school: u32) -> &'static str {
    match school {
        1 => "War Magic",
        2 => "Life Magic",
        3 => "Item Enchantment",
        4 => "Creature Enchantment",
        5 => "Void Magic",
        _ => "None",
    }
}

/// The spell's skill lookup, the five fallback skills, and
/// the spell-range determination's arithmetic.
///
/// All three live in [`pyreal_client_contract::panels::spell_examine`], because
/// `pyreal_client_shell::hud` evaluates the range against the player's own skills.
pub use pyreal_client_contract::panels::spell_examine::{
    skill_for_spell, spell_range, MAGIC_SKILLS,
};

/// The mana text write, whole: `"Mana: "` + (`"0"` when the mana is below 1, else its digits,
/// or `"???"` if conversion fails), then `" + %d per target"` when the per-target mod is positive.
///
/// Conversion cannot fail for a value that is already an integer here, so the `"???"` arm is
/// unreachable from this build and is not written; the `"0"` arm is reachable and is.
#[must_use]
pub fn mana_text(base_mana: i32, mana_mod: i32) -> String {
    let n = if base_mana < 1 {
        "0".to_owned()
    } else {
        base_mana.to_string()
    };
    let mut s = format!("Mana: {n}");
    if mana_mod > 0 {
        s.push_str(&format!(" + {mana_mod} per target"));
    }
    s
}

/// The spell examination's duration arm.
///
/// `None` is the client leaving the duration text as the clear left it: a duration of exactly
/// `-1.0` (the duration read's no-meta-spell answer) or `<= 0`. The conversion truncates toward
/// zero, so `59.9 s` reads `59` and `119 s` reads `1 min.`.
#[must_use]
pub fn duration_text(duration: f64) -> Option<String> {
    if duration == -1.0 || duration <= 0.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    if duration >= MINUTE_SECONDS {
        Some(format!(
            "Duration: {} min.",
            (duration * RECIPROCAL_MINUTE) as u32
        ))
    } else {
        Some(format!("Duration: {} sec.", duration as u32))
    }
}

/// The spell examination's duration on a world with a spell economy: the range a cast can land in.
/// A range reaching past a minute and a half reads in minutes.
#[must_use]
pub fn duration_range_text(duration: f64) -> Option<String> {
    if duration == -1.0 || duration <= 0.0 {
        return None;
    }
    let (low, high) = (duration * 0.75, duration * 1.25);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    if high > 90.0 {
        Some(format!(
            "Duration: {}-{} min.",
            crate::numfmt::printf_fixed(low * RECIPROCAL_MINUTE, 1),
            crate::numfmt::printf_fixed(high * RECIPROCAL_MINUTE, 1)
        ))
    } else {
        Some(format!("Duration: {}-{} sec.", low as u32, high as u32))
    }
}

/// The duration a world's spell panels show: a range with a spell economy, else the one number.
#[must_use]
pub fn duration_text_for(
    duration: f64,
    features: pyreal_primitives::EraFeatures,
) -> Option<String> {
    if features.spell_economy {
        duration_range_text(duration)
    } else {
        duration_text(duration)
    }
}

/// The spell examination's range arm. `None` is the exact-zero arm, on which the client clears
/// the text rather than writing `"Range: 0.0 yds."`.
#[must_use]
pub fn range_text(range: f32) -> Option<String> {
    if range == 0.0 {
        return None;
    }
    Some(format!(
        "Range: {:.1} yds.",
        f64::from(range) / METRES_PER_YARD
    ))
}

/// The compositor resolves these semantic mapper keys using its own artwork.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellIconLayers {
    pub power: u32,
    pub reversed: bool,
    /// Fellowship takes precedence over self-targeting.
    pub badge: Option<SpellBadge>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellBadge {
    Fellowship,
    SelfTargeted,
}
impl SpellIconLayers {
    #[must_use]
    pub const fn new(power: u32, bitfield: u32) -> Self {
        Self {
            power,
            reversed: bitfield & 0x10 != 0,
            badge: if bitfield & 0x2000 != 0 {
                Some(SpellBadge::Fellowship)
            } else if bitfield & 8 != 0 {
                Some(SpellBadge::SelfTargeted)
            } else {
                None
            },
        }
    }
    #[must_use]
    pub const fn tint_key(self) -> u32 {
        if self.reversed {
            1
        } else {
            2
        }
    }
    #[must_use]
    pub const fn badge_key(self) -> Option<u32> {
        match self.badge {
            Some(SpellBadge::Fellowship) => Some(4),
            Some(SpellBadge::SelfTargeted) => Some(3),
            None => None,
        }
    }
}

#[cfg(test)]
mod icon_layer_tests {
    use super::*;

    /// The tint row is `2 - (Reversed != 0)`; the badge tests Fellowship `0x2000` (row 4) before
    /// SelfTargeted `0x8` (row 3), so a spell with both flags gets row 4.
    #[test]
    fn the_spell_overlay_rows_are_the_clients_and_fellowship_outranks_self_targeted() {
        let at = |bits| SpellIconLayers::new(0, bits);
        assert_eq!(at(0).tint_key(), 2, "not reversed");
        assert_eq!(at(0x10).tint_key(), 1, "reversed");
        assert_eq!(at(0).badge_key(), None);
        assert_eq!(at(0x8).badge_key(), Some(3));
        assert_eq!(at(0x2000).badge_key(), Some(4));
        assert_eq!(at(0x2000 | 0x8).badge_key(), Some(4));
    }
}

#[cfg(test)]
mod era_text_tests {
    use super::*;

    #[test]
    fn a_duration_reads_as_its_range() {
        assert_eq!(
            duration_range_text(1800.0).as_deref(),
            Some("Duration: 22.5-37.5 min.")
        );
        assert_eq!(
            duration_range_text(60.0).as_deref(),
            Some("Duration: 45-75 sec."),
            "the top of the range is under a minute and a half"
        );
        assert_eq!(duration_range_text(0.0), None);
        // 75 s is exactly 1.25 min, which retail's printf rounds up.
        assert_eq!(
            duration_range_text(100.0).as_deref(),
            Some("Duration: 1.3-2.1 min.")
        );
        let with = pyreal_primitives::EraFeatures::LAUNCH;
        let without = pyreal_primitives::EraFeatures::END_OF_RETAIL;
        assert!(duration_text_for(1800.0, with).unwrap().contains('-'));
        assert_eq!(
            duration_text_for(1800.0, without).as_deref(),
            Some("Duration: 30 min.")
        );
    }
}
