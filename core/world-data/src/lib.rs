//! The retail data files turned into the shapes physics and animation consume.
//!
//! **Depends on** `pyreal-primitives`, the container and decoders (`pyreal-dat`, `pyreal-assets`)
//! and the two engines it feeds (`pyreal-physics`, `pyreal-animation`). **Used by** the client
//! runtime (`pyreal-client-runtime`).
//!
//! **Must never** draw, read input, own a frame loop or know about a network session: every module
//! is an adapter, data-file records in and physics or animation structures out.

/// The decoded dat records turned into `pyreal-animation`'s runtime shapes: the only place the decoders
/// and the animation layer meet.
pub mod anim_convert;

/// A `pyreal_animation::AnimAssets` over the retail portal dat, with its record memo.
pub mod anim_assets;

/// The numbering of the data files' motion commands, and their records translated into the final
/// numbering as they are loaded.
pub mod command_numbering;

/// The data files' physics script and sound numbers against the later files'.
pub mod effect_numbering;

/// The interior cells of a landblock, converted for physics, and the static objects baked into them.
pub mod env_cells;

/// A `pyreal_physics::LandSource` over the retail cell dat.
pub mod land_source;

/// The landblock and region identities the landscape path is indexed by, and `WorldError`.
pub mod landblock;

/// Part-array initialization's answer for a setup id: whether it succeeds, and the holding locations.
pub mod physics_setup;

/// A setup record's collision half: spheres and heights, and the parts with their BSPs and bounds.
pub mod setup;
