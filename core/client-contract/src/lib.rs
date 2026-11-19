//! The read-only view onto the game, the request channel back out and the action vocabulary, with
//! no presentation and no platform in it.
//!
//! **Depends on** `pyreal-primitives` (plus `glam` and `raw-window-handle`). **Used by** everything
//! on either side of the seam between the game and what draws it: the object model and the runtime
//! (`pyreal-client-model`, `pyreal-client-runtime`), the drawing and UI crates
//! (`dereth-render`, `pyreal-codecs`, `pyreal-horizon`, `pyreal-ui-logic`), the device input
//! (`pyreal-input`), the client and its test kit.
//!
//! **Must never** reach another `pyreal-*` crate or a presentation or platform crate: its direct
//! production dependencies are exactly `pyreal-primitives`, `glam` and `raw-window-handle`, and
//! nothing in its tree is a UI, a renderer, `winit` or `windows` (`cargo xtask seams`, `seam:
//! pyreal-client-contract deps`; the allowed list is the seam check's own, so adding a dependency
//! is a reviewed change there). It does no file I/O either: the persistence formats are here, the
//! reading and writing is the client's.
//!
//! [`GameView`], [`UiRequest`] and the `…View` value structs ([`view`], glob-re-exported here) are
//! the contract between whoever holds the world and whoever draws it; both sides depend on this
//! crate instead of on each other. [`ids`] holds [`ElementId`]; [`snapshot`] the
//! owned [`GameSnapshot`], for a view that cannot borrow; [`persist`] the UI's persistence types
//! and text formats; and [`actions`] the retail action ids, their phase and their names, which a
//! front end's device input produces and the runtime consumes.

#![forbid(unsafe_code)]

/// The action vocabulary: the retail action ids, their phase and their names. The runtime takes
/// actions; a front end's device pipeline makes them.
pub mod actions;
pub mod book;
/// The projection parameters `ViewParams` carries. `pyreal_codecs::camera` re-exports them.
pub mod camera;
/// The chat seam shared with the retired retail UI's `chat`: the failure table,
/// `ChatMessage`, the window ids and opacity attributes, the reply targets and the auto-target
/// question. `pyreal_client_runtime::hud` fills every one of them.
pub mod chat;
pub mod combat_mode;
/// The gameplay reason a confirmation dialog is on screen, shared by the object model and the
/// dialog engine.
pub mod confirmation;
pub mod ctime;
/// What a disconnect says, whatever UI shows it.
pub mod disconnect;
pub mod era;
pub mod examination;
pub mod floaty;
/// The gameplay screen's 3D viewport element id.
pub mod gameplay;
/// The plugin host's question to the player (a file a plugin asks for), and the answer.
pub mod host_question;
pub mod ids;
pub mod journal;
pub mod linkstatus;
/// The shared UI easing table, which the retired UI framework's `media` re-exports.
pub mod media;
/// The inbound notice queue, shared with the retired retail UI's `notices`. `MagicNotice` is
/// [`view`]'s; the inbox is a plain value
/// with no per-thread state.
pub mod notices;
/// The option *data*: the preference value store, the choice tables and the two const tables it
/// registers from. The retired retail UI's `options` re-exports each item.
pub mod options;
/// The overlay any UI draws over the world, as a presentation draws it.
pub mod overlay;
/// The panel seam shared with the retired retail UI's `panels`: the property
/// keys, tables, notices and formulas the world half of the client has to agree with a panel
/// about. The panels themselves stay in the UI.
pub mod panels;
pub mod persist;
/// The power bar's mode, shared by the combat state and the power-bar widgets.
pub mod powerbar;
/// The pre-game view the host hands the pre-game screens each frame, and the player-session
/// operations they hand back.
pub mod pregame;
/// The radar's two bit vocabularies and its range, shared with
/// the retired retail UI's `mapradar::radar`. The world half and the radar must name the same mask; this
/// is where they meet.
pub mod radar;
/// The outbound request queue, shared with the retired retail UI's `requests`. `UiRequest` is
/// [`view`]'s; the outbox is a plain value
/// with no per-thread state.
pub mod requests;
pub mod research;
/// `STARTUP_FILTERING`, which `pyreal_codecs::sampler` re-exports.
pub mod sampler;
pub mod snapshot;
pub mod social;
pub mod spellbook;
pub mod statmgmt;
/// Where the selected object stands on screen.
pub mod target;
/// The world-view teleport / portal animation model, which
/// the retired retail UI's `screens::teleport` re-exports.
pub mod teleport;
pub mod view;
/// `WindowHandles`, `Rect` and `ScreenMetrics`: the plain data a host window contributes.
/// `dereth_render::{device, window_proc}` re-export each.
pub mod window;
/// The window message table, the frame sleep and the placement arithmetic.
pub mod window_proc;

pub use ctime::UtcOffsetSecs;
pub use ids::ElementId;
pub use journal::{create_journal_path, JournalIdentity, STEM as JOURNAL_STEM};
pub use persist::{CharacterIdentity, CharacterSet, PersistError, UserPreferences};
pub use snapshot::GameSnapshot;
pub use statmgmt::XpHeader;
pub use view::*;

pub mod resolution;

pub mod feedback;
