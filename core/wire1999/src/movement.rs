//! The player's movement going out. The outbound module sends these in the ordered envelope with
//! its own order number on the 1999 weenie queue; their bodies are the later ones, which is what
//! AC1999's server reads.
//!
//! Motion commands stay in the world's numbering, which for the 1999 dats is the pre-2015 one
//! Pyreal already speaks; the stamps the bodies echo are the server's, passed through on the way in.

/// Jump, move to state, do and stop a movement command, turn to, autonomy level, autonomous
/// position and the non-autonomous jump.
pub const MOVEMENT_ACTIONS: [u32; 8] = [
    0xF61B, 0xF61C, 0xF61E, 0xF649, 0xF661, 0xF752, 0xF753, 0xF7C9,
];

/// Whether `action` is one of the player's movement actions.
#[must_use]
pub fn is_movement(action: u32) -> bool {
    MOVEMENT_ACTIONS.contains(&action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyreal_protocol::movement::{
        AutonomousPosition, JumpPack, MoveTimestamps, MovementAutonomousPosition, MovementJump,
    };
    use pyreal_protocol::types::{Frame, PositionWire, Quat, Vec3};
    use pyreal_protocol::{write_blob, Message};

    fn here() -> PositionWire {
        PositionWire {
            objcell_id: 0xA9B4_0031,
            frame: Frame {
                origin: Vec3 {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                },
                orientation: Quat {
                    w: 1.0,
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
            },
        }
    }

    fn word(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    /// AC1999 reads `0xF753` as position, then instance, server control, teleport and force
    /// position, then the contact byte (`ClientEvents.cpp`, release form).
    #[test]
    fn the_autonomous_position_is_laid_out_as_the_server_reads_it() {
        let m = MovementAutonomousPosition(AutonomousPosition {
            position: here(),
            timestamps: MoveTimestamps {
                instance: 3,
                server_control: 4,
                teleport: 5,
                force_position: 6,
            },
            contact: 1,
        });
        let b = write_blob(&m).unwrap();
        assert!(is_movement(MovementAutonomousPosition::OPCODE.0));
        assert_eq!(word(&b, 4), 0xA9B4_0031);
        let stamps: Vec<u16> = (0..4)
            .map(|i| u16::from_le_bytes([b[36 + 2 * i], b[37 + 2 * i]]))
            .collect();
        assert_eq!(stamps, [3, 4, 5, 6]);
        assert_eq!(b[44], 1);
        assert_eq!(b.len(), 48);
    }

    /// AC1999 reads `0xF61B` as extent, velocity, then a position, and ignores what follows.
    #[test]
    fn the_jump_starts_as_the_server_reads_it() {
        let m = MovementJump(JumpPack {
            extent: 0.5,
            velocity: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 9.0,
            },
            position: here(),
            timestamps: MoveTimestamps::default(),
        });
        let b = write_blob(&m).unwrap();
        assert!(is_movement(MovementJump::OPCODE.0));
        assert_eq!(f32::from_le_bytes([b[4], b[5], b[6], b[7]]), 0.5);
        assert_eq!(f32::from_le_bytes([b[16], b[17], b[18], b[19]]), 9.0);
        assert_eq!(word(&b, 20), 0xA9B4_0031);
        assert!(!is_movement(0x0015));
    }
}
