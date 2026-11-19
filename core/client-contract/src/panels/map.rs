//! A press on the world map.

/// Which of the pointer's buttons pressed the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapButton {
    Left,
    Right,
    Middle,
}

impl MapButton {
    /// The button's name: `left`, `right` or `middle`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Middle => "middle",
        }
    }
}

/// A press on the world map: the map coordinates under the pointer (north and east positive, as
/// the radar shows them), the button, and which of Ctrl, Shift and Alt were held.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapClick {
    pub ns: f64,
    pub ew: f64,
    pub button: MapButton,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}
