//! The external-container panel's two subscribed notices.
//!
//! The panel lives in the retired retail UI's `panels::external_container`; this enum is the transit
//! `pyreal_client_runtime::{hud, interaction}` fill and the panel drains -- `Hud::pending_external_container`
//! is a `Vec` of them -- so it is the contract and not the drawing. Two `ObjectId`s.

use pyreal_primitives::ObjectId;

/// The two subscribed item notices, retained in their original relative order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalContainerNotice {
    SetGroundObject(ObjectId),
    ItemMoved {
        object: ObjectId,
        container: ObjectId,
    },
}
