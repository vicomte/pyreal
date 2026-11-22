//! The one `panels::linkstatus` constant `GameView` names.
//!
//! `GameView::packet_loss_percent`'s default body is this number, so it lives beside `GameView`.

/// The packet-loss meter's compiled-in initial value, `1.0f`.
///
/// The same number as `pyreal_client_net::linkstatus::INITIAL_PACKET_LOSS`, which is the definition that
/// belongs to the transport; this crate cannot see that one and
/// a client test asserts the two agree.
pub const INITIAL_PACKET_LOSS: f32 = 1.0;
