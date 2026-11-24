//! A pad as the host reads it each frame: its buttons held, its two sticks and its two triggers,
//! named by where they sit on the pad rather than by what any one maker prints on them.
//!
//! The host fills a [`PadState`] from whatever pad library it has; an interface reads it and turns
//! it into actions and the orbit camera's sticks. Nothing here decides what a button does.

/// A pad's button, by its place: the four face buttons by compass point, the shoulder buttons,
/// the two small buttons in the middle, the d-pad and the sticks pressed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PadButton {
    /// The bottom face button (A on an Xbox pad, Cross on a PlayStation one).
    South,
    /// The right face button (B, Circle).
    East,
    /// The left face button (X, Square).
    West,
    /// The top face button (Y, Triangle).
    North,
    LeftBumper,
    RightBumper,
    /// The left of the two middle buttons (Back, View, Select, Share).
    Back,
    /// The right of the two middle buttons (Start, Menu, Options).
    Start,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    LeftStick,
    RightStick,
}

impl PadButton {
    /// Every button, in the order of their bits in [`PadState::buttons`].
    pub const ALL: [Self; 14] = [
        Self::South,
        Self::East,
        Self::West,
        Self::North,
        Self::LeftBumper,
        Self::RightBumper,
        Self::Back,
        Self::Start,
        Self::DPadUp,
        Self::DPadDown,
        Self::DPadLeft,
        Self::DPadRight,
        Self::LeftStick,
        Self::RightStick,
    ];

    /// Its bit in [`PadState::buttons`].
    #[must_use]
    pub const fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

/// Whose buttons a pad has, for marking them as they are printed on it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PadFamily {
    /// An Xbox pad, and any pad not known to be another's: A, B, X, Y, LB, RB, LT, RT.
    #[default]
    Xbox,
    /// A PlayStation pad (a DualShock or a DualSense): cross, circle, square and triangle, with
    /// its shoulder buttons and triggers.
    PlayStation,
}

impl PadFamily {
    /// Sony's USB vendor id.
    pub const SONY: u16 = 0x054C;

    /// The family of a pad with USB vendor id `vendor` (when the pad library knows it) and the
    /// name `name`.
    #[must_use]
    pub fn of(vendor: Option<u16>, name: &str) -> Self {
        let name = name.to_ascii_lowercase();
        if vendor == Some(Self::SONY)
            || ["dualsense", "dualshock", "ps4 controller", "ps5 controller"]
                .iter()
                .any(|n| name.contains(n))
        {
            Self::PlayStation
        } else {
            Self::Xbox
        }
    }
}

/// One pad's state at one moment.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PadState {
    /// The buttons held, one bit each ([`PadButton::bit`]).
    pub buttons: u16,
    /// The left stick: right and up, each from -1 to 1.
    pub left: (f32, f32),
    /// The right stick: right and up, each from -1 to 1.
    pub right: (f32, f32),
    /// The triggers, each from 0 (let go) to 1 (pulled all the way).
    pub left_trigger: f32,
    pub right_trigger: f32,
    /// Whose buttons it has.
    pub family: PadFamily,
}

impl PadState {
    /// Whether `button` is held.
    #[must_use]
    pub const fn held(&self, button: PadButton) -> bool {
        self.buttons & button.bit() != 0
    }

    /// Hold `button`, or let it go.
    pub fn set(&mut self, button: PadButton, held: bool) {
        if held {
            self.buttons |= button.bit();
        } else {
            self.buttons &= !button.bit();
        }
    }

    /// Whether `button` went down between `before` and this.
    #[must_use]
    pub const fn pressed_since(&self, before: &Self, button: PadButton) -> bool {
        self.held(button) && !before.held(button)
    }

    /// Whether `button` came up between `before` and this.
    #[must_use]
    pub const fn released_since(&self, before: &Self, button: PadButton) -> bool {
        !self.held(button) && before.held(button)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_button_has_a_bit_of_its_own_and_its_edges_are_read_against_the_last_state() {
        let mut seen = 0u16;
        for b in PadButton::ALL {
            assert_eq!(seen & b.bit(), 0, "{b:?} shares a bit");
            seen |= b.bit();
        }
        let before = PadState::default();
        let mut now = before;
        now.set(PadButton::South, true);
        assert!(now.held(PadButton::South) && !now.held(PadButton::East));
        assert!(now.pressed_since(&before, PadButton::South));
        assert!(!now.pressed_since(&now, PadButton::South));
        let mut later = now;
        later.set(PadButton::South, false);
        assert!(later.released_since(&now, PadButton::South));
        assert_eq!(later, before);
    }

    #[test]
    fn a_sony_pad_is_a_playstation_pad_and_any_other_is_marked_as_an_xbox_pad() {
        assert_eq!(
            PadFamily::of(Some(0x054C), "DualSense Wireless Controller"),
            PadFamily::PlayStation
        );
        assert_eq!(
            PadFamily::of(None, "PS4 Controller"),
            PadFamily::PlayStation
        );
        assert_eq!(
            PadFamily::of(Some(0x045E), "Xbox Wireless Controller"),
            PadFamily::Xbox
        );
        assert_eq!(
            PadFamily::of(Some(0x057E), "Pro Controller"),
            PadFamily::Xbox
        );
        assert_eq!(PadFamily::of(None, "Xbox Controller"), PadFamily::Xbox);
    }
}
