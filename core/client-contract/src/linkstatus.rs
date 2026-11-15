//! The one `panels::linkstatus` constant `GameView` names.
//!
//! `GameView::packet_loss_percent`'s default body is this number, so it has to be reachable from
//! a crate that may not depend on `dereth-ui-screens`. The rest of the panel — the captions, the
//! ping interval, the formatting — stays where it is.
//! the retired retail UI's `panels::linkstatus::INITIAL_PACKET_LOSS` is a `pub use` of this constant.

/// The packet-loss meter's compiled-in initial value, `1.0f`.
///
/// The same number as `pyreal_client_net::linkstatus::INITIAL_PACKET_LOSS`, which is the definition that
/// belongs to the transport; this crate cannot see that one and
/// a client test asserts the two agree.
pub const INITIAL_PACKET_LOSS: f32 = 1.0;
