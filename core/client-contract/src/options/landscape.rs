//! The presentation options from another era: which era's ground, which era's sky, and which
//! era's look the world's objects draw with.
//!
//! These were this client's own options, not retail ones. No player chooses any more: every world
//! draws with its own look. An older profile's [`GROUND`], [`SKY`] and [`OBJECTS`] are read as
//! nothing.

/// `[Render] Ground`: the land surface the ground is drawn with.
pub const GROUND: &str = crate::options::names::GROUND;
/// `[Render] Sky`: the sky, and the light and fog that come with it.
pub const SKY: &str = crate::options::names::SKY;
/// `[Render] Objects`: the era whose models, surfaces, pictures and palettes the world's objects
/// are drawn with.
pub const OBJECTS: &str = crate::options::names::OBJECTS;

/// Which of the three preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Landscape {
    /// [`GROUND`].
    Ground,
    /// [`SKY`].
    Sky,
    /// [`OBJECTS`].
    Objects,
}

impl Landscape {
    /// The preference this is, by name, in any case.
    #[must_use]
    pub fn of(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case(GROUND) {
            Some(Self::Ground)
        } else if name.eq_ignore_ascii_case(SKY) {
            Some(Self::Sky)
        } else if name.eq_ignore_ascii_case(OBJECTS) {
            Some(Self::Objects)
        } else {
            None
        }
    }
}
