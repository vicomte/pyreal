//! Vitae wording shared by interfaces.
use crate::numfmt::exact_number;
use crate::DisplayVariant;
use pyreal_client_contract::VitaeDisplay;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VitaeContent {
    pub penalty: i32,
    pub experience: i64,
}
pub fn vitae_content(display: VitaeDisplay, variant: DisplayVariant) -> VitaeContent {
    let strength = f64::from(display.multiplier) * 100.0;
    let penalty = match variant {
        DisplayVariant::Modern => {
            100i32.saturating_sub(pyreal_primitives::num::to_i32_f64(strength + 0.5))
        }
        DisplayVariant::Classic => {
            let loss = 100.0 - strength;
            if loss > 0.0 {
                pyreal_primitives::num::to_i32_f64(loss.max(1.0) + 0.5)
            } else {
                0
            }
        }
    };
    VitaeContent {
        penalty,
        experience: i64::from(display.threshold) - i64::from(display.cp_pool),
    }
}
impl VitaeContent {
    pub fn classic_text(self) -> String {
        let Self {
            penalty,
            experience,
        } = self;
        format!("\n\nDue to your recent death, you have temporarily lost {penalty}% of your Vitae, or life force.\n\nThis means that your health, stamina, mana, and skills are temporarily reduced by {penalty}%.  A reduction of less than 15% will not hinder you much, but beware losing much more than that.\n\nYou will regain 1% of your Vitae once you earn {} more experience point{}.\n", exact_number(experience), if experience == 1 { "" } else { "s" })
    }
}

#[cfg(test)]
mod tests;

/// The classic artwork clips the ratio after computing it in double precision.
pub fn classic_meter_ratio(current: u32, maximum: u32) -> f64 {
    if maximum == 0 {
        0.0
    } else {
        (f64::from(current) / f64::from(maximum)).clamp(0.0, 1.0)
    }
}
