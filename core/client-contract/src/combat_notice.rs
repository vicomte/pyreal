//! The skill the combat meter is gated on.
//!
//! One constant shared with the retired retail UI's `hud::combat_notice`, which keeps the notice handler
//! and its element ids. `pyreal_client_runtime::hud` queries the player description for the skill
//! advancement class used by the notice, so both sides have to name the same skill.

/// The skill the combat panel's melee arm asks the player description about:
/// `0x32` is `Recklessness` (`pyreal_client_model::skills::skill::RECKLESSNESS`), which is why the field it
/// gates is the recklessness meter.
pub const RECKLESSNESS_SKILL: u32 = 0x32;
