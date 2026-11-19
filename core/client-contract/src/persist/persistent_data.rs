//! The account's character list, as the login reply fills it.

use pyreal_primitives::ObjectId;

/// One character in the account's list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterIdentity {
    pub id: ObjectId,
    pub name: String,
    /// Seconds until the pending deletion completes, 0 when not pending.
    pub seconds_grace_period: u32,
}

/// `CharacterSet` — a packable object the login reply fills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterSet {
    /// The live characters.
    pub set: Vec<CharacterIdentity>,
    /// Characters pending deletion.
    pub del_set: Vec<CharacterIdentity>,
    /// Account status flags.
    pub status: u32,
    /// The account's character-slot count, **initialised to 5** and overwritten by the server.
    pub num_allowed_characters: u32,
    /// The account name.
    pub account: String,
    pub is_dark_majesty: bool,
    pub is_throne_of_destiny: bool,
    pub pre_ordered_throne_of_destiny: bool,
}

impl Default for CharacterSet {
    fn default() -> Self {
        Self {
            set: Vec::new(),
            del_set: Vec::new(),
            status: 0,
            // The documented initialiser. The server overwrites it, but a client that has not
            // heard from the server yet shows five slots.
            num_allowed_characters: 5,
            account: String::new(),
            is_dark_majesty: false,
            is_throne_of_destiny: false,
            pre_ordered_throne_of_destiny: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the allowed-character count starts at five.
    #[test]
    fn the_character_set_starts_with_five_slots_and_no_data() {
        let d = CharacterSet::default();
        assert_eq!(d.num_allowed_characters, 5);
        assert!(d.set.is_empty());
    }
}
