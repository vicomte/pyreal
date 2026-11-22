//! The stat-management panel's inputs.
//!
//! `GameView::experience_header` returns [`XpHeader`], so the type is contract: it is what the
//! host computed (`pyreal_client_model::advancement::experience_header`) and handed the panel, not anything
//! the panel draws. Its one inherent method is the meter fraction, which is arithmetic on its own
//! fields, so it comes with the type.

/// The client's inputs, already joined by the host.
///
/// `pyreal_client_model::advancement::experience_header` computes all of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct XpHeader {
    /// Integer64 quality 1, `TotalExperience`, already saturated at 2³²−1 for a
    /// non-Throne-of-Destiny account.
    pub total: u64,
    /// `total - xp_to_reach(level)`, where `xp_to_reach` is the experience table's total for a
    /// level.
    pub into_level: u64,
    /// `xp_to_reach(level + 1) - xp_to_reach(level)`; 0 makes the meter 0.
    pub level_span: u64,
    /// `xp_to_reach(level + 1) - total`, forced to 0 at the non-ToD level cap.
    pub to_level: u64,
    /// Integer quality `0x19`, the character level.
    pub level: i32,
    /// True at the level cap of an account without Throne of Destiny, where the number is
    /// replaced whatever the experience table says.
    pub at_cap: bool,
}

impl XpHeader {
    /// Whether the panel shows its "infinity" word in place of the experience to the next level:
    /// whenever nothing more is owed. That is the last level the experience table has (the next
    /// level's total is the last one's, which the character has reached), and the capped
    /// account's level 126.
    #[must_use]
    pub fn shows_infinity(&self) -> bool {
        self.at_cap || self.to_level == 0
    }
}
