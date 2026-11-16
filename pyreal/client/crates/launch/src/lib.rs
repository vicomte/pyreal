//! What Pyreal knows about worlds, accounts and data files before it connects, with no window, no
//! renderer and no operating system in it.
//!
//! **Depends on** the primitives crate (`pyreal-primitives`), for the client's era table, the
//! data-file crate (`pyreal-dat`), for a dat's identity and iterations in either layout, the plugin
//! packages crate (`pyreal-plugin-package`), for the plugins it installs and the lists of the
//! sources it installs them from, the plugin interface (`pyreal-plugin-api`), for the client's
//! plugin API version and plugins' versions, and the plugin template (`pyreal-plugin-template`),
//! for which folders of a plugin repository hold its plugins. **Used by** the client
//! (`pyreal-client`), whose screens before the world are a front end over it.
//!
//! **Must never** reach for a window, a GPU or the network, or name a platform API: it reads and
//! writes files (a dat, its own state) and nothing else.
//!
//! | module | what it knows |
//! |---|---|
//! | [`world`] | a world: its address and what it says of itself, read tolerantly |
//! | [`serverlist`] | the community's world list, read from its file |
//! | [`direct`] | the rule for a world's status document address |
//! | [`known`] | the worlds that run a client of their own, by list id: logon, files, rules |
//! | [`eras`] | the eras and systems a world can play; a world that does not say plays the end of retail |
//! | [`dat`] | a dat's iteration number, read straight off disk in a few small reads |
//! | [`datset`] | a Modern set (the four later dats, identified by iterations) or a Classic one (`portal.dat` and `cell.dat`) |
//! | [`install`] | which client build a folder holds, by the executable's hash and a manifest |
//! | [`choices`] | which dat sets a world may be played with |
//! | [`mod@check`] | the check before connecting: what the world would refuse, and the fix for each |
//! | [`launch`] | the [`LaunchPlan`] the client starts a session from, in process |
//! | [`state`] | the player's worlds, accounts, dat sets and per-world choices, and the old launcher's file |
//! | [`vault`] | the seam to the operating system's secret store, where passwords live and nowhere else |
//! | [`plugins`] | the plugins it installs for the client: the sources it reads them from, installing and updating them |
//! | [`join`] | a join link (`pyreal://join?...`): a world to add |
//! | [`update`] | whether a newer Pyreal is out: the latest release's index, compared and kept a day |

#![forbid(unsafe_code)]

pub mod check;
pub mod choices;
pub mod dat;
pub mod datset;
pub mod direct;
pub mod eras;
pub mod install;
pub mod join;
pub mod known;
pub mod launch;
pub mod plugins;
pub mod serverlist;
pub mod state;
pub mod update;
pub mod vault;
pub mod world;

#[cfg(any(test, feature = "testing"))]
#[doc(hidden)]
pub mod testing;

pub use check::{check, CheckId, CheckInput, Finding, Fix, Report, RunningClient, Verdict};
pub use datset::{DatOrigin, DatRole, DatSet, Iterations, SetKind};
pub use install::{ClientKind, Installation};
pub use launch::{plan, LaunchPlan, LaunchRequest};
pub use state::{Account, Favourite, LauncherState, WorldPrefs};
pub use vault::Vault;
pub use world::{Emulator, World, WorldState};
