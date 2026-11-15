//! The paper doll's model: its camera, light and pose, and the heritages that move its camera.

/// The paper doll's camera position —
/// `(0.12, -2.4, 0.88)` looking at the origin.
///
/// Initialization sets the position and zeroes the target.
pub const CAMERA_POSITION: (f32, f32, f32) = (0.12, -2.4, 0.88);

/// The second `Vector3` handed to the camera is `(0, 0, 0)`, the same target
/// re-issued for every known heritage.
pub const CAMERA_TARGET: (f32, f32, f32) = (0.0, 0.0, 0.0);

/// The paper doll's light: a distant light of intensity 2.0 in this direction.
///
/// The direction is `(0.3, **+1.9**, 0.65)`.
/// Positive y, like the character preview's and unlike the portal space's.
///
pub const LIGHT_DIRECTION: (f32, f32, f32) = (0.3, 1.9, 0.65);

/// The light's intensity.
pub const LIGHT_INTENSITY: f32 = 2.0;

/// The doll object's heading, set when the creature is redressed — the fixed three-quarter
/// pose, and the reason the doll is not seen edge-on.
pub const HEADING_DEGREES: f32 = 191.3679;

/// The doll's animation is set by clearing the
/// sequence, then starting at frame **1**, at **0 fps**.
///
/// Zero is not a mistake and not "unset". The paper doll is a *pose*, not a loop:
/// update advances `framerate * dt`, so a zero framerate holds frame 1 for ever.
/// A rebuild that "helpfully" ran it at 30 fps would animate a figure retail keeps still.
pub const LOW_FRAME: i32 = 1;
/// See [`LOW_FRAME`].
pub const FRAMERATE: f32 = 0.0;

/// The heritages that move the camera, keyed by
/// `PropertyInt 0xBC HeritageGroup`.
///
/// The `switch` has **no default arm that sets the camera**: heritages 1–5, 10 and 11 fall
/// through to the `return` and keep the post-init's [`CAMERA_POSITION`]. That is why an Aluvian
/// looks right with the constant above and why this table is short.
///
/// Cases 12 (`0x10000011`) and 13 (`0x10000013`) also swap the doll's animation for their own
/// `UIASSET` enum before moving the camera — the Olthoi heritages, which have no humanoid
/// idle. Those two enums are the second element of each row; `None` keeps
/// [`ANIMATION_ENUM`].
/// One row of [`RACE_CAMERA`]: the heritage, the camera position, and the `UIASSET` enum that
/// replaces the doll's animation (only the two Olthoi heritages have one).
pub type RaceCamera = (u32, (f32, f32, f32), Option<u32>);

/// See [`RaceCamera`].
pub const RACE_CAMERA: [RaceCamera; 6] = [
    (6, (0.12, -3.0, 0.88), None),
    (7, (0.12, -3.0, 0.88), None),
    (8, (0.12, -3.4, 1.0), None),
    (9, (0.12, -3.4, 0.88), None),
    (12, (0.12, -3.4, 0.88), Some(0x1000_0011)),
    (13, (0.12, -3.4, 0.88), Some(0x1000_0013)),
];

/// [`RACE_CAMERA`] as the per-race update asks it: the camera and the animation enum for a
/// heritage, or `None` when the `switch` falls through and nothing is re-issued.
#[must_use]
pub fn for_race(heritage: u32) -> Option<((f32, f32, f32), Option<u32>)> {
    RACE_CAMERA
        .iter()
        .find(|(h, _, _)| *h == heritage)
        .map(|(_, c, a)| (*c, *a))
}

/// The doll's animation: enum
/// `0x10000005`, the `UIASSET` group's
/// `PaperDollAnimation`. Resolved by the host through `pyreal_client_runtime::assets::enum_did`; this crate
/// carries the enum because the client does.
pub const ANIMATION_ENUM: u32 = 0x1000_0005;
