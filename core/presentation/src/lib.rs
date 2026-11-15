//! The game's presentation rules: how an appraisal, the character sheet, the journal and the
//! calendar are worded, numbered and placed, for any interface.
//!
//! **Depends on** `pyreal-primitives`, the contract (`pyreal-client-contract`), whose read-only
//! view the rules are functions of, and the shared rules' arithmetic (`pyreal-rules`). **Used by**
//! the runtime (`pyreal-client-runtime`), the interface (`pyreal-horizon`) and the game's words
//! it reads (`pyreal-ui-logic`).
//!
//! **Must never** draw, lay out an element, hold a string table or reach the platform: these are
//! facts about what the game shows, and an interface decides where to show them.
//!
//! - [`appraisal`]: every line, row and colour an identify shows.
//! - [`character`]: the character information sheet's ladders, breakdowns and its augmentation
//!   and luminance section, composed over whatever string service the interface has.
//! - [`numfmt`], [`inforegion`], [`map`]: numeric grouping, information rows and calendar text.
//! - [`journal`]: the journal's pages, its file's text and its captions.

pub mod appraisal;
pub mod character;
pub mod creation;
pub mod inforegion;
pub mod journal;
pub mod map;
pub mod numfmt;
pub mod spell;
pub mod stats;
pub mod vendor;

/// Wording and ordering conventions, independent of the connected world's capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayVariant {
    Classic,
    Modern,
}
