//! The shared real application fixtures, as the device-backed modules use them.

#[path = "../../common/app.rs"]
#[allow(dead_code)] // shared with the dat binary, which uses the rest
mod shared;

pub use shared::{frames, game_screen, position, unhide_recorded_player};
