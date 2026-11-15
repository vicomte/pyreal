//! The retail dats, as the gpu tier's stations open them.
//!
//! A station that cannot get them fails rather than returning early: a test that printed
//! "skipping" and passed would read as a pass on a machine that tested nothing.

use std::sync::Arc;

use pyreal_dat::RetailDatStore;

/// The retail dats, shared, or a failed test that says where they were looked for.
pub fn dats() -> Arc<RetailDatStore> {
    Arc::new(pyreal_dat::testing::open_store_or_fail())
}
