//! Who the current selection lets the player swear allegiance to.

use crate::{AllegianceRoster, GameView};
use pyreal_primitives::ObjectId;

#[must_use]
pub fn swear_target(view: &dyn GameView, roster: &AllegianceRoster) -> Option<ObjectId> {
    if roster.patron.is_some()
        || view.available_experience().max(0) < i64::from(view.oath_xp_cost().unwrap_or(0))
    {
        return None;
    }
    view.selected_object().filter(|id| {
        Some(*id) != view.player()
            && !view.allegiance_has_member(*id)
            && view
                .selection_query_facts(*id)
                .is_some_and(|facts| facts.is_player)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AllegianceEntry, SelectionQueryFacts};

    #[derive(Debug)]
    struct View {
        selection: Option<ObjectId>,
        known_player: bool,
        cost: Option<u32>,
        experience: i64,
        allegiance_member: bool,
    }
    impl Default for View {
        fn default() -> Self {
            Self {
                selection: Some(ObjectId(100)),
                known_player: true,
                cost: None,
                experience: 0,
                allegiance_member: false,
            }
        }
    }
    impl GameView for View {
        fn player(&self) -> Option<ObjectId> {
            Some(ObjectId(1))
        }
        fn selected_object(&self) -> Option<ObjectId> {
            self.selection
        }
        fn selection_query_facts(&self, _: ObjectId) -> Option<SelectionQueryFacts> {
            self.known_player.then_some(SelectionQueryFacts {
                is_player: true,
                ..Default::default()
            })
        }
        fn oath_xp_cost(&self) -> Option<u32> {
            self.cost
        }
        fn available_experience(&self) -> i64 {
            self.experience
        }
        fn allegiance_has_member(&self, _: ObjectId) -> bool {
            self.allegiance_member
        }
    }
    #[test]
    fn an_oath_requires_a_valid_outside_player_and_enough_unassigned_experience() {
        let mut view = View {
            cost: Some(10),
            experience: 9,
            ..Default::default()
        };
        let mut roster = AllegianceRoster::default();
        assert_eq!(swear_target(&view, &roster), None);
        view.experience = 10;
        assert_eq!(swear_target(&view, &roster), Some(ObjectId(100)));
        roster.patron = Some(AllegianceEntry::default());
        assert_eq!(swear_target(&view, &roster), None);
        roster.patron = None;
        view.allegiance_member = true;
        assert_eq!(swear_target(&view, &roster), None);
        view.allegiance_member = false;
        view.selection = Some(ObjectId(1));
        assert_eq!(swear_target(&view, &roster), None);
        view.selection = Some(ObjectId(100));
        view.known_player = false;
        assert_eq!(swear_target(&view, &roster), None);
        view.known_player = true;
        view.cost = None;
        view.experience = -1;
        assert_eq!(swear_target(&view, &roster), Some(ObjectId(100)));
    }
}
