//! The landscape's geometry and static contents: landblock meshes, terrain merge keys, stitching,
//! water, roads, scenery placement and the per-cell building registry, as pure functions of the
//! retail records.
//!
//! **Depends on** `pyreal-primitives` (the vocabulary and the arithmetic policy), the decoded
//! world records of `pyreal-assets` and the block window of `pyreal-landscape`. **Used by**
//! the world drawing (`pyreal-world-render`)
//! and the client runtime's world builder (`pyreal-client-runtime`), which places the same scenery
//! and statics with no device.
//!
//! **Must never** draw, hold a device or decode a record: what leaves here is geometry, placements
//! and lists.
//!
//! | Module | What it owns |
//! |---|---|
//! | [`land::mesh`] | vertices, the diagonal split, planes, UVs, per-cell rotation |
//! | [`land::merge`] | the terrain merge key, road codes, alpha maps, the ground plan |
//! | [`land::stitch`] | the transition adjustment between detail rings |
//! | [`land::water`] | water classification |
//! | [`scenery`], [`road`] | deterministic scenery, its hashes and filters |
//! | [`buildings`] | the building each land cell owns and its static-object lists |
//! | [`math`], [`consts`] | the frame arithmetic and the landscape's constants |
//!
//! **Specified in** `docs/formats/15-region.md` (the world grid and terrain texturing) and
//! `docs/formats/17-scene-and-particles.md` (scenery placement).

pub mod buildings;
pub mod consts;
pub mod ground_cover;
pub mod land;
pub mod math;
pub mod narrow;
pub mod road;
pub mod scenery;

/// A plane in the client's form: a **unit** normal and the signed distance `d` such that
/// `N·p + d == 0` on the plane.
pub use pyreal_primitives::shape::Plane;
