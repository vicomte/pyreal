//! The November 1999 client's wire (version `4.35752`), spoken in process so Pyreal can play a
//! server that speaks it, such as the AC1999 project's.
//!
//! **Depends on** `pyreal-primitives`, `pyreal-transport` (the checksum, the ISAAC key stream and
//! the fragment header, which the 1999 wire shares), `pyreal-protocol` (the later messages it
//! translates to) and `pyreal-dat` (the zlib inflate and the dat id ranges, for the dat push).
//! **Used by** the client runtime's 1999 driver.
//!
//! **Must never** open a socket or read a clock: the caller feeds it datagrams and the time.
//!
//! The header, flags, checksum and fragments are [`packet`], the login, credential and grant
//! [`login`], one connection's reliability [`conn`], the 1999 strings and account field
//! [`strings`], the login-area messages [`messages`], and the session's
//! [`pyreal_primitives::Transport`] over it [`link`]. [`probe`] tells a world of this wire from a
//! later one.
//!
//! The translation is split by area: each module keeps its own state and implements
//! [`translate::Translate`], and [`link::modules`] lists them. The login screens are
//! [`login_area`], the dat push [`dats`], character creation and deletion [`chargen`], and world
//! entry [`entry`]. The object stream is [`world`], with creates read in [`objects`]. [`stamps`]
//! keeps the event stamps and property bytes the 1999 wire lacks, and [`movement`] names the
//! player's moves the outbound module sends. The game actions and other requests sent from the
//! world are [`outbound`]. The player description is [`player`], quality updates [`qualities`]
//! and the other game events [`events`].
//!
//! The 1999 wire differs from the later one throughout: the checksum is seeded `0x03291975`, the
//! keys come from a grant rather than a connect request, only data packets are keyed, delivery is
//! in packet order, and the server sends on queues 3, 4 and 7.

pub mod chargen;
pub mod conn;
pub mod dats;
pub mod entry;
pub mod events;
pub mod link;
pub mod login;
pub mod login_area;
pub mod messages;
pub mod movement;
pub mod objects;
pub mod outbound;
pub mod packet;
pub mod player;
pub mod probe;
pub mod qualities;
pub mod stamps;
pub mod strings;
pub mod translate;
pub mod world;

pub use conn::{Close, Connection, Rejected, State};
pub use link::Link1999;
pub use login::{Credential, CredentialError, CD_ITERATIONS, CLIENT_VERSION, CREDENTIAL_LEN};
pub use translate::{Context, Shared, Translate, Wrapped};
