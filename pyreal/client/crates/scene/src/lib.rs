//! The client's drawn world: the runtime's world handed to the engine, the presentation the frame draws through, and the preview spaces.
//!
//! It also holds the plain pictures the interface reads: the dats' textures and the world map.
//!
//! **Depends on** the runtime whose world it draws (`pyreal-client-runtime`), the engine that draws
//! it (`pyreal-engine`, on `wgpu`), and the crates the world is read from: `pyreal-terrain`,
//! `pyreal-primitives`, `pyreal-dat`, `pyreal-assets`, `pyreal-physics`, `pyreal-animation`,
//! `pyreal-audio`, `pyreal-protocol`, `pyreal-world-data`, `pyreal-world-render` (the sky's day groups and the preview
//! spaces' model), the picture codecs (`pyreal-codecs`), the contract's camera and overlay types
//! (`pyreal-client-contract`), character creation's model (`pyreal-chargen`) and the art pack
//! (`pyreal-art-pack`, re-exported as `art_pack`). **Used by** the client shell
//! (`pyreal-client-shell`) and the desktop client (`pyreal-client`).
//!
//! **Must never** reach a platform: no window, no event loop, no sound device and no operating
//! system call (`cargo xtask seams`, `seam: application halves`). The window arrives as raw
//! handles; the drawing is gated on the `wgpu` feature, and without it the crate holds only the
//! plain types.
//!
//! | Module | What it owns |
//! |---|---|
//! | [`gpu`] | the presentation: the device and its window, the frame, the UI's picture, captures |
//! | [`bridge`] | the world handed to the engine: the land round a centre, the objects, the player |
//! | [`settings`] | the graphics options as the engine's switches |
//! | [`previews`], [`preview`] | the preview spaces on the engine, and their bodies |
//! | [`first_pixel`] | the picture drawn before any world |
//! | [`marks`] | lines and dots a front end places in the world |
//! | [`textures`] | the dat texture lookup |
//! | [`render_prefs`] | the `Render.*` owners on the scene |
//! | [`world_map`] | the world map's picture, drawn from the landblocks off the frame |
//! | `art_pack` | the art pack's reader |

pub use pyreal_art_pack as art_pack;
/// The world handed to the engine.
#[cfg(gpu)]
pub mod bridge;
/// The first-pixel picture.
#[cfg(gpu)]
pub mod first_pixel;
/// The presentation on the engine.
#[cfg(gpu)]
pub mod gpu;
/// Marks a front end places in the world, and their geometry.
pub mod marks;
/// The preview spaces' bodies.
pub mod preview;
/// The preview spaces on the engine.
#[cfg(gpu)]
pub mod previews;
/// The scene-owned half of the `Render.*` preferences, over a glob re-export of
/// [`pyreal_client_runtime::render_prefs`].
pub mod render_prefs;
/// The graphics options as the engine's switches.
#[cfg(gpu)]
pub mod settings;
pub mod textures;
pub mod world_map;
