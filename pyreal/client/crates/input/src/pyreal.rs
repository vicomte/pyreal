//! This client's own actions in the input layer ([`pyreal_client_contract::actions::pyreal`]):
//! their map, toggle types, classes and names.
//!
//! The shipped action map has no row for these actions, so the manager registers them beside
//! the shipped rows in [`INPUT_MAP`]. Only the performance panel has a default key
//! ([`DEFAULT_KEYS`]). The map is live for the whole run at the lowest priority, and a focused
//! text box's barrier still stops its bindings.

use crate::actionmap::{ActionMap, ActionMapValue, ToggleType};
use crate::keymap::MasterInputMap;
use crate::spec::{ControlChord, ControlCode};
use crate::{ActionId, InputMapId};

/// The input map this client's own actions are bound in.
pub use crate::maps::OWN as INPUT_MAP;

/// One of this client's actions: its id, toggle type, action class (the key page's tab:
/// 1 movement, 3 interface, 7 character settings), and name. The shipped string table has no
/// name for it, so the name is this client's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerethAction {
    pub action: ActionId,
    pub toggle: ToggleType,
    pub class: u32,
    pub name: &'static str,
}

/// The action-class tabs of the key page these actions are listed on.
pub mod class {
    pub const MOVEMENT: u32 = 1;
    pub const INTERFACE: u32 = 3;
    pub const CHARACTER_SETTINGS: u32 = 7;
}

/// The key page's heading over this client's actions on each tab.
pub const SECTION_NAME: &str = "Dereth";

/// The name the key page shows for one of this client's actions.
#[must_use]
pub fn name(action: ActionId) -> Option<&'static str> {
    ACTIONS.iter().find(|a| a.action == action).map(|a| a.name)
}

/// This client's actions.
pub const ACTIONS: &[DerethAction] = {
    use pyreal_client_contract::actions::pyreal as a;
    const fn one_shot(action: ActionId, class: u32, name: &'static str) -> DerethAction {
        DerethAction {
            action,
            toggle: ToggleType::OneShot,
            class,
            name,
        }
    }
    &[
        one_shot(
            a::TOGGLE_PERFORMANCE_PANEL,
            class::INTERFACE,
            "Performance Panel",
        ),
        DerethAction {
            toggle: ToggleType::Hold,
            ..one_shot(a::MOVEMENT_HOLD_SIDESTEP, class::MOVEMENT, "Hold Sidestep")
        },
        one_shot(a::TOGGLE_TRADE_PANEL, class::INTERFACE, "Trade Panel"),
        one_shot(a::TOGGLE_ADMIN_WINDOW, class::INTERFACE, "Admin Window"),
        one_shot(
            a::SAVE_FRAME_TIMINGS,
            class::INTERFACE,
            "Save Frame Timings",
        ),
        one_shot(
            a::OPEN_FRAME_TIMINGS_FOLDER,
            class::INTERFACE,
            "Open Frame Timings Folder",
        ),
        one_shot(
            a::TOGGLE_SPELL_RESEARCH_PANEL,
            class::INTERFACE,
            "Spell Research Panel",
        ),
        one_shot(
            a::PLAYER_OPTION_AUTO_CREATE_SHORTCUTS,
            class::CHARACTER_SETTINGS,
            "Automatically Create Shortcuts",
        ),
        one_shot(
            a::TOGGLE_INVERT_MOUSE_LOOK,
            class::CHARACTER_SETTINGS,
            "Invert Mouse Look",
        ),
        one_shot(
            a::TOGGLE_RIGHT_CLICK_MOUSE_LOOK,
            class::CHARACTER_SETTINGS,
            "Right-Click Mouse Look (classic interface)",
        ),
        one_shot(
            a::TOGGLE_STRETCH_UI,
            class::CHARACTER_SETTINGS,
            "Stretch UI (classic interface)",
        ),
        one_shot(
            a::TOGGLE_MUTE_ON_LOSING_FOCUS,
            class::CHARACTER_SETTINGS,
            "Mute When Inactive",
        ),
        one_shot(a::CANCEL, class::INTERFACE, "Cancel (classic interface)"),
        one_shot(
            a::REPEAT_LAST_MESSAGE,
            class::INTERFACE,
            "Repeat Last Message (classic interface)",
        ),
        DerethAction {
            toggle: ToggleType::Hold,
            ..one_shot(
                a::LOOK_AT_FRONT,
                class::INTERFACE,
                "Look At Your Front (Horizon interface)",
            )
        },
        one_shot(a::LOOT_ALL, class::INTERFACE, "Loot All"),
        one_shot(a::USE_SPELL_SLOT_13, class::INTERFACE, "Spell Slot 13"),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 1),
            class::INTERFACE,
            "Spell Slot 14",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 2),
            class::INTERFACE,
            "Spell Slot 15",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 3),
            class::INTERFACE,
            "Spell Slot 16",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 4),
            class::INTERFACE,
            "Spell Slot 17",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 5),
            class::INTERFACE,
            "Spell Slot 18",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 6),
            class::INTERFACE,
            "Spell Slot 19",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 7),
            class::INTERFACE,
            "Spell Slot 20",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 8),
            class::INTERFACE,
            "Spell Slot 21",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 9),
            class::INTERFACE,
            "Spell Slot 22",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 10),
            class::INTERFACE,
            "Spell Slot 23",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 11),
            class::INTERFACE,
            "Spell Slot 24",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 12),
            class::INTERFACE,
            "Spell Slot 25",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 13),
            class::INTERFACE,
            "Spell Slot 26",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 14),
            class::INTERFACE,
            "Spell Slot 27",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 15),
            class::INTERFACE,
            "Spell Slot 28",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 16),
            class::INTERFACE,
            "Spell Slot 29",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 17),
            class::INTERFACE,
            "Spell Slot 30",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 18),
            class::INTERFACE,
            "Spell Slot 31",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 19),
            class::INTERFACE,
            "Spell Slot 32",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 20),
            class::INTERFACE,
            "Spell Slot 33",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 21),
            class::INTERFACE,
            "Spell Slot 34",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 22),
            class::INTERFACE,
            "Spell Slot 35",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 23),
            class::INTERFACE,
            "Spell Slot 36",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 24),
            class::INTERFACE,
            "Spell Slot 37",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 25),
            class::INTERFACE,
            "Spell Slot 38",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 26),
            class::INTERFACE,
            "Spell Slot 39",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 27),
            class::INTERFACE,
            "Spell Slot 40",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 28),
            class::INTERFACE,
            "Spell Slot 41",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 29),
            class::INTERFACE,
            "Spell Slot 42",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 30),
            class::INTERFACE,
            "Spell Slot 43",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 31),
            class::INTERFACE,
            "Spell Slot 44",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 32),
            class::INTERFACE,
            "Spell Slot 45",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 33),
            class::INTERFACE,
            "Spell Slot 46",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 34),
            class::INTERFACE,
            "Spell Slot 47",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 35),
            class::INTERFACE,
            "Spell Slot 48",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 36),
            class::INTERFACE,
            "Spell Slot 49",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 37),
            class::INTERFACE,
            "Spell Slot 50",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 38),
            class::INTERFACE,
            "Spell Slot 51",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 39),
            class::INTERFACE,
            "Spell Slot 52",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 40),
            class::INTERFACE,
            "Spell Slot 53",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 41),
            class::INTERFACE,
            "Spell Slot 54",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 42),
            class::INTERFACE,
            "Spell Slot 55",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 43),
            class::INTERFACE,
            "Spell Slot 56",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 44),
            class::INTERFACE,
            "Spell Slot 57",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 45),
            class::INTERFACE,
            "Spell Slot 58",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 46),
            class::INTERFACE,
            "Spell Slot 59",
        ),
        one_shot(
            ActionId(a::USE_SPELL_SLOT_13.0 + 47),
            class::INTERFACE,
            "Spell Slot 60",
        ),
    ]
};

/// Shift and Ctrl together, as a key's modifier mask: the key map's first and second meta keys.
const CTRL_SHIFT: u32 = 0xC000_0000;

/// The default keys of this client's actions, each a key's scan code, its modifiers and the
/// action: Ctrl+Shift+P shows or hides the performance panel (Command+Shift+P on a Mac). No
/// shipped key map binds it.
pub const DEFAULT_KEYS: &[(u16, u32, ActionId)] = &[(
    0x19,
    CTRL_SHIFT,
    pyreal_client_contract::actions::pyreal::TOGGLE_PERFORMANCE_PANEL,
)];

/// Bind [`DEFAULT_KEYS`] in `map`'s [`INPUT_MAP`], on the keyboard `map` binds keys on.
pub fn add_default_keys(map: &mut MasterInputMap) {
    for (scan, meta, action) in DEFAULT_KEYS {
        if let Some(c) = keyboard_chord(map, *scan) {
            let key = ControlChord::new(c.control, *meta, c.activation);
            map.create_input_map(INPUT_MAP).add_mapping(key, *action);
        }
    }
}

impl ActionMap {
    /// Add this client's actions in [`INPUT_MAP`]. They carry no name in the shipped string
    /// table: their name and description ids are their own action ids, which no string table
    /// answers, and the key page shows [`name`] for them.
    ///
    /// [`INPUT_MAP`] conflicts with every map a key page lists keys in: it is live for the whole
    /// run beside all of them, so a key given to one of these actions is taken from whatever
    /// else had it, after the page asks, and a key given to anything else is taken from these,
    /// as the shipped maps' own conflicts work.
    pub fn add_pyreal_actions(&mut self) {
        let mut maps: Vec<InputMapId> = crate::presentation::ROWS.iter().map(|r| r.map).collect();
        maps.sort_unstable();
        maps.dedup();
        for m in maps.into_iter().filter(|m| *m != INPUT_MAP) {
            self.add_conflict(INPUT_MAP, m);
        }
        for a in ACTIONS {
            self.insert(
                INPUT_MAP,
                a.action,
                ActionMapValue {
                    toggle_type: a.toggle,
                    action_class: a.class,
                    action_name: a.action.0,
                    description: a.action.0,
                },
            );
        }
    }
}

/// An unmodified key of the keyboard `map` binds keys on, by scan code (bit 7 for the extended
/// prefix): the device, sub-control and activation of the first unmodified keyboard key `map`
/// binds, with `scan` as the key. `None` when `map` binds no key on a keyboard.
#[must_use]
pub fn keyboard_chord(map: &MasterInputMap, scan: u16) -> Option<ControlChord> {
    let template = map
        .sections
        .iter()
        .flat_map(|s| s.bindings().iter())
        .map(|(qc, _)| *qc)
        .find(|qc| {
            map.device_type_of(qc.control) == Some(crate::spec::DeviceType::Keyboard)
                && qc.meta_mode == 0
        })?;
    let control = ControlCode::new(
        template.control.device_index(),
        template.control.sub_control(),
        scan,
    );
    Some(ControlChord::new(control, 0, template.activation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_actions_are_in_their_own_map_with_their_toggle_type() {
        let mut m = ActionMap::default();
        m.add_pyreal_actions();
        let perf = pyreal_client_contract::actions::pyreal::TOGGLE_PERFORMANCE_PANEL;
        assert!(m.is_action_allowed_in_input_map(INPUT_MAP, perf));
        assert_eq!(m.toggle_type(INPUT_MAP, perf), ToggleType::OneShot);
        assert!(
            m.is_user_bindable(INPUT_MAP, perf),
            "listed on the key page"
        );
        assert_eq!(m.action_class(INPUT_MAP, perf), class::INTERFACE);
        assert_eq!(name(perf), Some("Performance Panel"));
        for a in ACTIONS {
            assert!(m.is_user_bindable(INPUT_MAP, a.action), "{}", a.name);
        }
    }
}
