//! The retail UI element id the contract carries.
//!
//! `ElementId`'s hand-written `Debug` prints the hex form every layout id is written in.

/// Unique **inside a layout**, and the key of the whole message system.
///
/// The element lookup is a **linear** search of the layout's element list that returns the
/// *first* match, so ids must stay unique across simultaneously loaded layouts. The client
/// achieves that by partitioning the id space per screen into `0x10000xxx` blocks.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ElementId(pub u32);

impl std::fmt::Debug for ElementId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, concat!(stringify!(ElementId), "(0x{:X})"), self.0)
    }
}
