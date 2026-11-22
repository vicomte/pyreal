//! The Empyrean lighthouses' beacons, drawn as a giant magical white light.
//!
//! The Empyrean lighthouse (building 0x010011F3; seven stand in Dereth, Tou-Tou's among them) bears
//! on its top a column of white wisps (setup 0x020003C5), and on five of them a white burst
//! (0x020003CB). Retail adds their pictures whole; the particles would take them for snow and
//! steam. Here they are a beacon ([`is_beacon`]):
//! - its heart, a ball of white light hot at its core in a wide halo, swelling slowly, crossed by
//!   slowly turning rays;
//! - its shaft, a soft column of light rising from the heart into the sky, shimmering as it climbs;
//! - retail's burst and wisps drawn as white light round the heart ([`WISP`]);
//! - a white lamp lighting the tower's top ([`lamp`]).
//!
//! The wisps' host carries the heart, the shaft and the lamp ([`HEART`]); the burst's host only
//! its pictures. `shaders/beacons.wgsl` draws it.

use glam::{Affine3A, Vec3, Vec4};

use crate::particles::{Drawn, Look};

/// The lighthouse and its beacon's setups: the burst, and the wisps, which carry the heart.
const LIGHTHOUSE: u32 = 0x0100_11F3;
const BURST: u32 = 0x0200_03CB;
const WISPS: u32 = 0x0200_03C5;

/// A beacon's parts, as [`Look::Beacon`] and a host's `beacon` name them: retail's pictures drawn
/// as light, the heart, the shaft.
pub const WISP: u8 = 0;
pub const HEART: u8 = 1;
pub const SHAFT: u8 = 2;

/// How high over the beacon's foot its heart burns, and its light's reach, metres.
const HEART_HIGH: f32 = 4.0;
const HEART_REACH: f32 = 12.0;
/// The shaft's half width and its height over the heart, metres.
const SHAFT_HALF: f32 = 3.0;
const SHAFT_HIGH: f32 = 90.0;

/// The lamp's colour (a cool white), the client's intensity and falloff (metres) and its gain.
const LAMP_COLOUR: Vec3 = Vec3::new(0.9, 0.95, 1.0);
const LAMP_FALLOFF: f32 = 16.0;
const LAMP_GAIN: f32 = 3.0;

/// Which of a beacon's hosts setup `id` standing at `at` (block-local) is, if it is one: one of
/// its setups over one of the block's `buildings`, the lighthouse ([`HEART`] for its wisps,
/// [`WISP`] for its burst).
#[must_use]
pub fn is_beacon(
    id: u32,
    at: pyreal_primitives::Vec3,
    buildings: &[(u32, pyreal_primitives::Vec3)],
) -> Option<u8> {
    let part = match id {
        WISPS => HEART,
        BURST => WISP,
        _ => return None,
    };
    buildings
        .iter()
        .any(|&(b, o)| {
            b == LIGHTHOUSE && (o.x - at.x).powi(2) + (o.y - at.y).powi(2) < 1.0 && at.z > o.z
        })
        .then_some(part)
}

/// The beacon standing at `foot` (render space): its heart and its shaft as the particles draw
/// them, with picture `sprite`, those `planes` see.
#[must_use]
pub fn drawn(
    foot: Vec3,
    sprite: u32,
    cell: u32,
    eye: Vec3,
    planes: &[Vec4],
    group: u32,
) -> Vec<Drawn> {
    let heart = foot + Vec3::Z * HEART_HIGH;
    let part = |look: u8, transform: Affine3A, at: Vec3, r: f32| {
        planes
            .iter()
            .all(|q| q.truncate().dot(at) + q.w >= -r)
            .then_some(Drawn {
                sprite,
                transform,
                alpha: 1.0,
                cell,
                distance: at.distance(eye),
                light: false,
                look: Look::Beacon(look),
                age: 0.0,
                seed: 0,
                group,
            })
    };
    let mid = heart + Vec3::Z * SHAFT_HIGH * 0.5;
    [
        part(
            HEART,
            Affine3A::from_scale_rotation_translation(
                Vec3::splat(HEART_REACH),
                glam::Quat::IDENTITY,
                heart,
            ),
            heart,
            HEART_REACH,
        ),
        // Its half width and height carried in its scale; the shader stands it up facing the eye.
        part(
            SHAFT,
            Affine3A::from_scale_rotation_translation(
                Vec3::new(SHAFT_HALF, SHAFT_HALF, SHAFT_HIGH),
                glam::Quat::IDENTITY,
                heart,
            ),
            mid,
            SHAFT_HIGH * 0.5,
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The beacon's white lamp at its heart, its foot at `foot`.
#[must_use]
pub fn lamp(foot: Vec3, cell: u32) -> crate::lamps::Lamp {
    let source = crate::lamps::LampSource {
        client_offset: None,
        kind: crate::lamps::LampKind::Portal,
        offset: Vec3::ZERO,
        colour: LAMP_COLOUR,
        intensity: crate::lamps::MAGIC_INTENSITY,
        falloff: LAMP_FALLOFF,
        flicker: crate::lamps::Flicker::Steady,
        shadows: false,
    };
    let at = Affine3A::from_translation(foot + Vec3::Z * HEART_HIGH);
    let mut l = crate::lamps::place(&source, &at, cell, u32::MAX);
    l.colour = LAMP_COLOUR;
    l.candela *= LAMP_GAIN;
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32, y: f32, z: f32) -> pyreal_primitives::Vec3 {
        pyreal_primitives::Vec3::new(x, y, z)
    }

    /// The burst and the wisps over a lighthouse are its beacon; beside it, under it or over
    /// another building they are not.
    #[test]
    fn a_beacon_is_its_setups_over_a_lighthouse() {
        let block = [
            (0x0100_0001, at(10.0, 10.0, 0.0)),
            (LIGHTHOUSE, at(132.0, 84.0, 70.0)),
        ];
        assert_eq!(
            is_beacon(WISPS, at(132.1, 84.2, 141.0), &block),
            Some(HEART)
        );
        assert_eq!(is_beacon(BURST, at(132.1, 84.2, 141.0), &block), Some(WISP));
        assert_eq!(is_beacon(BURST, at(140.0, 84.0, 141.0), &block), None);
        assert_eq!(is_beacon(BURST, at(132.0, 84.0, 60.0), &block), None);
        assert_eq!(is_beacon(BURST, at(10.0, 10.0, 5.0), &block), None);
        assert_eq!(is_beacon(0x0200_0624, at(132.0, 84.0, 141.0), &block), None);
    }
}
