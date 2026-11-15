//! The Horizon interface: the client's third interface, beside the modern and the classic one, drawn
//! from its own art.
//!
//! The game underneath is the client's: the shared runtime's frame loop, network session, object
//! model and physics, the shared scene that draws the world, and the shared device input with its
//! key maps. What this crate adds is the interface: the title screen, the world select, the lobby
//! and character creation, the HUD and its windows ([`ui`]), drawn as textured quads through the
//! shared overlay ([`draw`]) from the interface's own art ([`art`], [`pieces`]), built in.
//!
//! **Depends on** the shared crates: `pyreal-primitives`, `pyreal-client-contract`,
//! `pyreal-client-runtime`, `pyreal-client-model`, `pyreal-dat`, `pyreal-assets`, `pyreal-chargen`,
//! `pyreal-rules`, `pyreal-animation`, `pyreal-presentation`, `pyreal-client-net`, `pyreal-input`,
//! `pyreal-navigation` (the map's place store) and `pyreal-plugin-api` (the plugins' declared
//! windows); the client's `pyreal-ui-kit` (the widget kit it is drawn with),
//! `pyreal-plugin-ui` (the plugins' windows, drawn with the same kit), `pyreal-scene` (the game's
//! icons), `pyreal-text` (the game's text) and `pyreal-ui-logic` (the game's words, tables and
//! rules it reads). **Used by** the client shell (`pyreal-client-shell`), which shows it.
//!
//! **Must never** reach a window, a device or the platform: the shell hands it events and draws
//! its overlay.
//!
//! The pieces: [`runtime`] is the front end the shell drives ([`runtime::HorizonFrontEnd`]); [`ui`] is
//! the interface; [`state`] reads the game for it; [`dialogs`] shows the game's questions;
//! [`draw`] draws it; [`art`] reads the art; [`pieces`] are the interface's own art's files;
//! [`font`] holds the tables its text is drawn from; [`resample`] makes a picture at the size it is
//! drawn; [`scale`] says which interface scales a window offers and the one it is drawn at.

pub mod art;
pub mod art_packs;
pub mod dialogs;
pub mod draw;
pub mod font;
pub mod front_door;
pub mod looks;
pub mod options;
pub mod pad;
pub mod pieces;
pub mod plugin_shelf;
pub use pyreal_ui_kit::resample;
pub mod retail_ui;
pub mod ring;
pub mod runtime;
pub mod scale;
pub mod state;
pub mod strings;
pub mod ui;
