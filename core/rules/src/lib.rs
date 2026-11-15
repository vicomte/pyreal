//! The pure retail game rules: values and decoded tables in, numbers out.
//!
//! **Depends on** `pyreal-primitives`, the decoded tables of `pyreal-assets` and, behind the
//! default `proto` feature, `pyreal-protocol`. **Used by** the client's object model
//! (`pyreal-client-model`), animation
//! (`pyreal-animation`) and the interface (`pyreal-horizon`).
//!
//! **Must never** hold the object model, read a file or touch the wire. The quality-reading
//! inquiries are written against [`quality::QualityRead`], which the object model implements.
//!
//! It holds the XP curves and attribute raise costs, the skill formula, burden and load, vendor
//! prices, the fellowship split, enchantment stacking ([`enchant`]), the name filter ([`taboo`]),
//! chess, the slot tables, the character-creation credits, and a weenie's look as a server
//! describes it ([`appearance`]).

#![doc(html_no_source)]

pub mod advancement;
pub mod allegiance;
#[cfg(feature = "proto")]
pub mod appearance;
pub mod attributes;
pub mod burden;
/// A container's item and side-pack slot counts.
pub mod capacity;
/// Character-creation arithmetic: skill costs and credits.
pub mod chargen;
pub mod chess;
/// Combat rules: the elemental bonus against players.
pub mod combat;
#[cfg(feature = "proto")]
pub mod enchant;
pub mod fellowship;
/// The spell-formula rules: power levels and the scarab-only (foci) formula.
pub mod magic;
/// The movement system's run rate, jump height and jump stamina cost (one implementation).
pub mod movement;
pub mod names;
/// The property → `PublicWeenieDesc` mirror table.
pub mod pwd_mirror;
#[cfg(feature = "proto")]
pub mod quality;
pub mod skills;
pub mod slots;
pub mod spell_charge;
pub mod taboo;
pub mod vendor;
pub mod weenie;
/// A world's rules as data: the vocabulary, its bounds and the compiled-in profiles.
pub mod world;
