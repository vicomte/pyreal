//! Detailed Faces: nearby players' faces drawn with finer skin, hair, beards, brows and eyes,
//! standing out in depth. This client's own check box, a boolean in the option value store.

use crate::view::PrefValue;

/// `[Display] DetailedFaces`: whether nearby faces are drawn in detail.
pub const DETAILED_FACES: &str = crate::options::names::DETAILED_FACES;

/// The check box's caption.
pub const CAPTION: &str = "Detailed Faces";

/// What the check box does, for the row's tooltip.
pub const NOTE: &str = "Draws the faces of players near you with finer skin, hair, beards and \
                        eyes, shaped in depth. Each face shows its detail a moment after it \
                        first comes near.";

/// Register the check box in the option value store, on.
pub fn register() -> usize {
    usize::from(super::store::register_preference(
        DETAILED_FACES,
        PrefValue::Bool(true),
        super::store::DataType::Bool,
    ))
}

/// Whether the store says faces are detailed. On until the store says otherwise.
#[must_use]
pub fn on() -> bool {
    !matches!(
        super::store::inq_value(DETAILED_FACES),
        Some(PrefValue::Bool(false))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_are_detailed_until_turned_off() {
        super::super::store::init();
        assert!(on());
        let _ = super::super::store::set_value(DETAILED_FACES, PrefValue::Bool(false));
        assert!(!on());
        let _ = super::super::store::set_value(DETAILED_FACES, PrefValue::Bool(true));
        assert!(on());
    }
}
