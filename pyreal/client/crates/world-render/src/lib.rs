//! The retail client's world rules with no GPU code: the sky through the day and the preview spaces' camera.
//!
//! The modules: the sky's day groups, light and fog through the day (`sky`) and the preview
//! spaces' camera and light (`creature_mode`).
//!
//! **Depends on** `pyreal-primitives`, the decoded world records of `pyreal-assets` and the
//! terrain's arithmetic in `pyreal-terrain`. **Used by** the engine (`pyreal-engine`), the scene
//! (`pyreal-scene`) and the client shell.
//!
//! **Must never** hold GPU code, link a renderer crate or decode anything, and it never reaches the
//! client runtime (`cargo xtask seams`, `seam: client crates`).
//!
//! | Module | What it owns |
//! |---|---|
//! | [`sky`] | the sky lookup, the day cycle, fog |
//! | [`creature_mode`] | the preview spaces' camera, field of view and light |

#![doc(html_no_source)]
#![forbid(unsafe_code)]

pub mod creature_mode;
pub mod sky;

/// A plane in the client's form: a **unit** normal and the signed distance `d` such that
/// `N·p + d == 0` on the plane.
///
/// The same type the dat decoders produce and physics collides with; here it is constructed at run
/// time from polygon geometry and is the one terrain, scenery placement and picking all read.
pub use pyreal_primitives::shape::Plane;
