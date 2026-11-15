//! The game's words, tables and rules the interface reads, kept apart from how it draws them.
//!
//! **Depends on** `pyreal-primitives`, the contract (`pyreal-client-contract`) through which it
//! reads the game, the shared presentation rules (`pyreal-presentation`), character creation's
//! model (`pyreal-chargen`) and the dat records and files it reads the creation tables from
//! (`pyreal-assets`, `pyreal-dat`). **Used by** the Horizon interface (`pyreal-horizon`) and the
//! client shell (`pyreal-client-shell`).
//!
//! **Must never** draw, or name the object model or the runtime: it reads the game through the
//! contract only.
//!
//! - [`chargen`]: the creation tables and the turntable's model; [`barber`] the barber's edit.
//! - [`radar`]: the blips' colours and shapes. [`paper_doll`]: the doll's camera and pose.
//! - [`icon`] and [`items`]: an object's or a spell's icon, composited from the dat.
//! - [`house`]: the house pane's rows. [`panels`]: the panels' and dialogs' words and ids.
//! - [`shortcuts`]: the shortcut bar's actions. [`character_options`]: the option tokens.

#![forbid(unsafe_code)]

pub mod barber;
pub mod character_options;
pub mod chargen;
pub mod house;
pub mod icon;
pub mod items;
pub mod panels;
pub mod paper_doll;
pub mod radar;
pub mod shortcuts;
