//! Application-owned DAT facts for part-array initialization and child attachment. The dat walk
//! is `pyreal_world_data::physics_setup`; this wraps its answer into the game
//! model's `PhysicsSetupFacts`.
use pyreal_client_model::objects::PhysicsSetupFacts;
use pyreal_dat::RetailDatStore;

pub(super) fn resolve(store: &RetailDatStore, wire_id: u32) -> PhysicsSetupFacts {
    match pyreal_world_data::physics_setup::resolve(store, wire_id) {
        Some(holding_locations) => PhysicsSetupFacts::Ready { holding_locations },
        None => PhysicsSetupFacts::Failed,
    }
}
