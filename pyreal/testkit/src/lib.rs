//! Test support for the client: the harness a behaviour scenario is written in.
//!
//! **Depends on** the products it tests (`pyreal-client`) and the crates a
//! scenario reaches through them (`pyreal-client-shell`, `pyreal-desktop`,
//! `pyreal-client-runtime`, `pyreal-client-model`, `pyreal-rules`,
//! `pyreal-client-contract`, `pyreal-client-net`, `pyreal-input`,
//! `pyreal-primitives`, `pyreal-dat`, `pyreal-physics`, `pyreal-transport`, `pyreal-protocol`), and
//! the plugin host a conformance case runs (`pyreal-plugin-api`, `pyreal-plugin-host`,
//! `pyreal-plugin-lua` and `pyreal-plugin-wasm`, with `toml` for the cases' expectations, and `pyreal-assets` for the scripted
//! world's spell table, and `pyreal-navigation` for its journeys).
//! **Used by** nothing: it is a workspace member only so its tests are built and run.
//!
//! **Must never** be depended on from any crate's `src/` or from `tools/`, and it holds no oracle
//! of its own: every fact a scenario asserts over comes from the recorded corpus, the shipped data
//! or the client's own production code.
//!
//! | piece | what it is |
//! |---|---|
//! | [`HeadlessClient`] | the client under test, set up in one call |
//! | [`claims`] | the ids a scenario declares and asserts |
//! | [`golden`] | one output file per run instead of a dozen counters |
//! | [`Inbound::from_corpus`] | messages from the recorded corpus, whose bytes are proved once |
//! | [`input_steps`] | the action and input driver scenarios share |
//! | [`plugins`], [`conformance`] | the plugin host over a replayed recording, and the spec's conformance cases run on it |
//! | [`plugin_reference`] | the plugin reference pages of the spec, generated from the plugin interface's code |
//!
//! ```no_run
//! use pyreal_testkit::{HeadlessClient, Given, Inbound, Player};
//! use pyreal_primitives::ObjectId;
//!
//! let mut c = HeadlessClient::model();
//! c.given(Given::APlayer(ObjectId(0x5000_0001)))
//!     .when(Player::DoubleClick(ObjectId(0x5000_0002)))
//!     .assert_behaviour("use.progress-notice.names-the-object", |v| {
//!         v.notice_text() == ["Using the Chest"]
//!     });
//! ```
//!
//! `given` sets the client up, `when` does something to it, `tick` runs frames, and the closure
//! handed to `assert_behaviour` reads a [`ScenarioView`], which can see everything and write
//! nothing.
//!
//! `tests/cpu` opens no retail data file; `tests/dat` opens the data under `$PYREAL_TEST_DAT_DIR` and
//! **must run serially**, because two headless clients in one process share process-wide state.

pub mod adapters_chat;
pub mod adapters_inventory;
pub mod adapters_shell;
pub mod adapters_social;
pub mod claims;
pub mod client;
pub mod conformance;
pub mod corpus_steps;
pub mod golden;
pub mod inbound;
pub mod input_steps;
mod login;
pub mod outbound;
pub mod player;
pub mod plugin_reference;
pub mod plugins;
pub mod replay;
mod scenarios;
pub mod scripted_travel;
pub mod view;
pub mod wire;

pub use client::{Assets, ClientSpec, Given, HeadlessClient, ScratchSettings, Step};
pub use inbound::Inbound;
pub use outbound::Outbound;
pub use player::{Direction, Player, ScreenPoint, Target};
pub use replay::Peer;
pub use view::ScenarioView;
pub use wire::Wire;
