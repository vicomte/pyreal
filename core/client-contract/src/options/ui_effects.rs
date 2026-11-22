//! UI Blur: the world blurred behind the interface's see-through grounds. This client's own check
//! box, a boolean in the option value store, on at every preset.

use crate::view::PrefValue;

/// `[Display] UIBlur`: whether the world behind the interface's grounds is blurred.
pub const UI_BLUR: &str = crate::options::names::UI_BLUR;

/// The check box's caption.
pub const BLUR_CAPTION: &str = "UI Blur";

/// What the check box does, for the row's tooltip.
pub const BLUR_NOTE: &str = "Blurs the world behind windows, menus and the chat so their text \
                             is easier to read.";

/// Register the check box in the option value store, on.
pub fn register() -> usize {
    usize::from(super::store::register_preference(
        UI_BLUR,
        PrefValue::Bool(true),
        super::store::DataType::Bool,
    ))
}

/// Whether the store says the world behind the interface is blurred. On until it says otherwise.
#[must_use]
pub fn blur_on() -> bool {
    !matches!(
        super::store::inq_value(UI_BLUR),
        Some(PrefValue::Bool(false))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_blur_is_on_until_turned_off() {
        super::super::store::init();
        assert!(blur_on());
        let _ = super::super::store::set_value(UI_BLUR, PrefValue::Bool(false));
        assert!(!blur_on());
        let _ = super::super::store::set_value(UI_BLUR, PrefValue::Bool(true));
        assert!(blur_on());
    }
}
