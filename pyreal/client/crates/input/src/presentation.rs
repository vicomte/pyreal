//! The catalog of key bindings the key page lists.
//!
//! Every action a player can bind is a row here: the input map it is bound in, the group it
//! belongs to, its label, and why it does nothing when it does nothing. A row that does nothing is
//! not on the page and has no default key.
//!
//! The rows are the user-bindable entries of the shipped action map (an action and the input map
//! it is allowed in), this client's own actions, and Disable Most Weather Effects, which the
//! shipped action map has but does not let a player bind. Two kinds of shipped row are not
//! listed: the quickslots 10 to 18, which no shortcut bar shows, and the quest detail panel's
//! toggle, which opens no window.

use crate::{ActionId, InputMapId};
use pyreal_client_contract::options::sheet::{self, Needs};
use pyreal_client_contract::PlayerOption;
use pyreal_primitives::EraFeatures;

/// The input maps the rows are bound in.
pub use crate::maps as map;

/// What a row is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Movement,
    Camera,
    Combat,
    Selection,
    Panels,
    Chat,
    Shortcuts,
    CharacterOptions,
    Emotes,
    Other,
}

/// Why a row does nothing, and so is not on the page.
pub mod why {
    pub const RIGHT_CLICK_MOUSE_LOOK: &str = "It turned the classic interface's right-click mouse \
        look on or off; this client has no such mode.";
    pub const STRETCH_UI: &str = "It turned the classic interface's stretched layout on or off; \
        this client has no such layout.";
    pub const AUTO_CREATE_SHORTCUTS: &str = "Only the classic interface made a shortcut of an \
        item used.";
    pub const CANCEL: &str = "Escape is this interface's own cancel key, and it is not bound.";
    pub const REPEAT_MESSAGE: &str = "This interface's chat entry brings back earlier lines with \
        the Up and Down arrows.";
}

/// The actions with no place in the interface, which its key page leaves out: the camera views
/// and the alternate camera its own camera has no use for, the windows and panels it does not
/// have or opens from elsewhere, the retired interfaces' own switches, and the keys it does
/// without (holding a sidestep, switching the chat line, making a shortcut, ending the session).
/// The alternate camera's own map and the character options are left out whole.
pub const HIDDEN: &[&str] = &[
    "MovementHoldSidestep",
    "CameraActivateAlternateMode",
    "ToggleFellowshipPanel",
    "ToggleSpellManagementPanel",
    "ToggleSpellComponentsPanel",
    "ToggleSkillManagementPanel",
    "ToggleAttributesPanel",
    "ToggleSkillsPanel",
    "ToggleWorldPanel",
    "ToggleHousePanel",
    "ToggleCharacterOptionsPanel",
    "ToggleConfigOptionsPanel",
    "ToggleKeyboardPanel",
    "ToggleFriendsPanel",
    "ToggleCharacterTitlePanel",
    "ToggleContractsPanel",
    "ToggleChatEntry",
    "CreateShortcut",
    "LOGOUT",
    "CameraViewDefault",
    "CameraViewFirstPerson",
    "CameraViewLookDown",
    "CameraViewMapMode",
    "ToggleAbusePanel",
    "TogglePositiveEffectsPanel",
    "ToggleNegativeEffectsPanel",
    "ToggleLinkStatusPanel",
    "ToggleUrgentAssistancePanel",
    "TogglePageListPanel",
    "ToggleRadarPanel",
    "ToggleFloatingChatWindow1",
    "ToggleFloatingChatWindow2",
    "ToggleFloatingChatWindow3",
    "ToggleFloatingChatWindow4",
    "ToggleHelp",
    "TogglePluginManager",
    "TogglePerformancePanel",
    "ToggleTradePanel",
    "ToggleSpellResearchPanel",
    "ToggleInvertMouseLook",
    "ToggleRightClickMouseLook",
    "ToggleStretchUI",
    "PlayerOption_SideBySideVitals",
    "PlayerOption_AutoCreateShortcuts",
    "Cancel",
    "RepeatLastMessage",
];

/// One row of the key page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    /// The input map the action is bound in.
    pub map: InputMapId,
    /// The action, by the name a key map file gives it.
    pub action_name: &'static str,
    pub group: Group,
    /// The row's label.
    pub label: &'static str,
    /// Why the row does nothing, when it does nothing.
    pub not_used: Option<&'static str>,
}

impl Row {
    /// The action.
    ///
    /// # Panics
    /// A name the action tables do not know, which the table's own test rules out.
    #[must_use]
    pub fn action(&self) -> ActionId {
        pyreal_client_contract::actions::names::action_for_enum_name(self.action_name)
            .expect("every row names a known action")
    }

    /// The input map the action is bound in.
    #[must_use]
    pub const fn input_map(&self) -> InputMapId {
        self.map
    }

    /// Whether the keyboard editor lists this action. Hidden actions remain bindable.
    #[must_use]
    pub fn shown(&self) -> bool {
        self.not_used.is_none()
            && !(HIDDEN.contains(&self.action_name)
                || self.map == map::CAMERA_ALTERNATE
                || self.group == Group::CharacterOptions)
    }

    /// What the row's action needs of the world: a window key its window's system, a spell tab
    /// key the spell tabs, and an option key what the option's own row needs.
    #[must_use]
    pub fn needs(&self) -> Needs {
        match self.action_name {
            "ToggleHousePanel" => Needs::Housing,
            "ToggleCharacterTitlePanel" => Needs::Titles,
            "ToggleJournalPanel" | "TogglePageListPanel" => Needs::Journal,
            "ToggleContractsPanel" => Needs::Contracts,
            "ToggleTradePanel" => Needs::Trade,
            "ToggleSpellResearchPanel" => Needs::SpellResearch,
            "CombatPrevSpellTab"
            | "CombatNextSpellTab"
            | "CombatFirstSpellTab"
            | "CombatLastSpellTab" => Needs::SpellTabs,
            name => name
                .strip_prefix("PlayerOption_")
                .and_then(|option| {
                    PlayerOption::ALL
                        .into_iter()
                        .find(|o| format!("{o:?}") == option)
                })
                .and_then(sheet::row_of_option)
                .map_or(Needs::Nothing, |row| row.needs),
        }
    }

    /// Whether the keyboard editor lists this action on a world playing `features`. A row the
    /// world leaves out stays bindable, and a binding made for it stays saved.
    #[must_use]
    pub fn listed(&self, features: Option<&EraFeatures>) -> bool {
        self.shown() && self.needs().met(features)
    }

    /// The row's caption: a few actions are named by what they do here.
    #[must_use]
    pub fn caption(&self) -> &'static str {
        match self.action_name {
            "MovementWalkMode" => "Toggle Walk/Run",
            _ => self.label,
        }
    }
}

/// The rows only the page lists, after the ninth shortcut: the shortcut bar's tenth to twelfth
/// slots, which the game binds to 0, `-` and `=`.
#[rustfmt::skip]
pub const EXTRA_ROWS: &[Row] = &[
    row(M::QUICKSLOTS, "UseQuickSlot_10", G::Shortcuts, "Use Shortcut 10"),
    row(M::QUICKSLOTS, "UseQuickSlot_11", G::Shortcuts, "Use Shortcut 11"),
    row(M::QUICKSLOTS, "UseQuickSlot_12", G::Shortcuts, "Use Shortcut 12"),
];

/// The page, in order: the rows it shows, with its own after the ninth shortcut.
pub fn page_rows() -> impl Iterator<Item = &'static Row> {
    ROWS.iter().filter(|r| r.shown()).flat_map(|r| {
        let after: &'static [Row] = if r.action_name == "UseQuickSlot_9" {
            EXTRA_ROWS
        } else {
            &[]
        };
        std::iter::once(r).chain(after)
    })
}

/// The row of `action` in `map`, when the catalog has it.
#[must_use]
pub fn find(map: InputMapId, action: ActionId) -> Option<&'static Row> {
    ROWS.iter().find(|r| r.map == map && r.action() == action)
}

/// The rows of `action`, in whichever map: the camera actions are in two.
pub fn rows_of(action: ActionId) -> impl Iterator<Item = &'static Row> {
    ROWS.iter().filter(move |r| r.action() == action)
}

/// The retail page's caption for a row the shipped string table does not name: Disable Most
/// Weather Effects, under the Character Options page's own words for it.
#[must_use]
pub fn modern_caption(map: InputMapId, action: ActionId) -> Option<&'static str> {
    (map == map::CHARACTER_OPTIONS
        && action
            == pyreal_client_contract::actions::names::action_for_enum_name(
                "PlayerOption_DisableMostWeatherEffects",
            )
            .unwrap_or(ActionId(0)))
    .then_some("Disable Most Weather Effects")
}

/// The retail page's tab (action class) for a row the shipped action map gives none: Disable
/// Most Weather Effects, which goes with the other character options.
#[must_use]
pub fn modern_class(map: InputMapId, action: ActionId) -> Option<u32> {
    modern_caption(map, action).map(|_| crate::pyreal::class::CHARACTER_SETTINGS)
}

use map as M;
use why as Why;
use Group as G;

const fn row(map: InputMapId, action_name: &'static str, group: Group, label: &'static str) -> Row {
    Row {
        map,
        action_name,
        group,
        label,
        not_used: None,
    }
}

const fn row_not_used(
    map: InputMapId,
    action_name: &'static str,
    group: Group,
    label: &'static str,
    not_used: Option<&'static str>,
) -> Row {
    Row {
        map,
        action_name,
        group,
        label,
        not_used,
    }
}

/// Every row, by group.
#[rustfmt::skip]
pub const ROWS: &[Row] = &[
    row(M::MOVEMENT, "MovementForward", G::Movement, "Walk Forward"),
    row(M::MOVEMENT, "MovementBackup", G::Movement, "Walk Backwards"),
    row(M::MOVEMENT, "MovementStop", G::Movement, "Stop Moving"),
    row(M::MOVEMENT, "MovementStrafeRight", G::Movement, "Side Step Right"),
    row(M::MOVEMENT, "MovementStrafeLeft", G::Movement, "Side Step Left"),
    row(M::MOVEMENT, "MovementTurnRight", G::Movement, "Turn Right"),
    row(M::MOVEMENT, "MovementTurnLeft", G::Movement, "Turn Left"),
    row(M::MOVEMENT, "MovementRunLock", G::Movement, "Auto Run"),
    row(M::MOVEMENT, "MovementJump", G::Movement, "Jump"),
    row(M::MOVEMENT, "MovementWalkMode", G::Movement, "Hold Run"),
    row(M::MOVEMENT, "Ready", G::Movement, "Stand"),
    row(M::MOVEMENT, "Crouch", G::Movement, "Crouch"),
    row(M::MOVEMENT, "Sitting", G::Movement, "Sit Down"),
    row(M::MOVEMENT, "Sleeping", G::Movement, "Lie Down"),
    row(M::OWN, "MovementHoldSidestep", G::Movement, "Hold Sidestep"),
    row(M::CAMERA, "CameraMoveToward", G::Camera, "Camera Closer"),
    row(M::CAMERA, "CameraMoveAway", G::Camera, "Camera Farther"),
    row(M::CAMERA, "CameraRotateLeft", G::Camera, "Camera Left Rotate"),
    row(M::CAMERA, "CameraRotateRight", G::Camera, "Camera Right Rotate"),
    row(M::CAMERA, "CameraRotateUp", G::Camera, "Camera Raise"),
    row(M::CAMERA, "CameraRotateDown", G::Camera, "Camera Lower"),
    row(M::CAMERA, "CameraViewDefault", G::Camera, "Reset View"),
    row(M::CAMERA, "CameraViewFirstPerson", G::Camera, "First Person View"),
    row(M::CAMERA, "CameraViewLookDown", G::Camera, "Floor View"),
    row(M::CAMERA, "CameraViewMapMode", G::Camera, "Map View"),
    row(M::CAMERA, "CameraInstantMouseLook", G::Camera, "Shift View"),
    row(M::CAMERA, "CameraActivateAlternateMode", G::Camera, "Toggle Alternate Camera Mode"),
    row(M::CAMERA_ALTERNATE, "CameraMoveToward", G::Camera, "Zoom Camera In (alternate)"),
    row(M::CAMERA_ALTERNATE, "CameraMoveAway", G::Camera, "Zoom Camera Out (alternate)"),
    row(M::CAMERA_ALTERNATE, "CameraRotateLeft", G::Camera, "Rotate Camera Left (alternate)"),
    row(M::CAMERA_ALTERNATE, "CameraRotateRight", G::Camera, "Rotate Camera Right (alternate)"),
    row(M::CAMERA_ALTERNATE, "CameraRotateUp", G::Camera, "Rotate Camera Up (alternate)"),
    row(M::CAMERA_ALTERNATE, "CameraRotateDown", G::Camera, "Rotate Camera Down (alternate)"),
    row(M::CAMERA_ALTERNATE, "CameraViewDefault", G::Camera, "Move Camera to Default (alternate)"),
    row(M::CAMERA_ALTERNATE, "CameraViewFirstPerson", G::Camera, "First Person Camera (alternate)"),
    row(M::CAMERA_ALTERNATE, "CameraViewLookDown", G::Camera, "Overhead View (alternate)"),
    row(M::CAMERA_ALTERNATE, "CameraViewMapMode", G::Camera, "Map View (alternate)"),
    row(M::COMBAT, "CombatToggleCombat", G::Combat, "Combat Mode"),
    row(M::MELEE, "CombatDecreaseAttackPower", G::Combat, "Decrease Power"),
    row(M::MELEE, "CombatIncreaseAttackPower", G::Combat, "Increase Power"),
    row(M::MELEE, "CombatLowAttack", G::Combat, "Attack Low"),
    row(M::MELEE, "CombatMediumAttack", G::Combat, "Attack Medium"),
    row(M::MELEE, "CombatHighAttack", G::Combat, "Attack High"),
    row(M::MISSILE, "CombatDecreaseMissileAccuracy", G::Combat, "Decrease Accuracy"),
    row(M::MISSILE, "CombatIncreaseMissileAccuracy", G::Combat, "Increase Accuracy"),
    row(M::MISSILE, "CombatAimLow", G::Combat, "Aim Low"),
    row(M::MISSILE, "CombatAimMedium", G::Combat, "Aim Medium"),
    row(M::MISSILE, "CombatAimHigh", G::Combat, "Aim High"),
    row(M::MAGIC, "CombatCastCurrentSpell", G::Combat, "Cast Spell"),
    row(M::MAGIC, "CombatPrevSpell", G::Combat, "Previous Spell"),
    row(M::MAGIC, "CombatNextSpell", G::Combat, "Next Spell"),
    row(M::MAGIC, "CombatPrevSpellTab", G::Combat, "Previous Spelltab"),
    row(M::MAGIC, "CombatNextSpellTab", G::Combat, "Next Spelltab"),
    row(M::MAGIC, "UseSpellSlot_1", G::Combat, "Spell Slot 1"),
    row(M::MAGIC, "UseSpellSlot_2", G::Combat, "Spell Slot 2"),
    row(M::MAGIC, "UseSpellSlot_3", G::Combat, "Spell Slot 3"),
    row(M::MAGIC, "UseSpellSlot_4", G::Combat, "Spell Slot 4"),
    row(M::MAGIC, "UseSpellSlot_5", G::Combat, "Spell Slot 5"),
    row(M::MAGIC, "UseSpellSlot_6", G::Combat, "Spell Slot 6"),
    row(M::MAGIC, "UseSpellSlot_7", G::Combat, "Spell Slot 7"),
    row(M::MAGIC, "UseSpellSlot_8", G::Combat, "Spell Slot 8"),
    row(M::MAGIC, "UseSpellSlot_9", G::Combat, "Spell Slot 9"),
    row(M::MAGIC, "UseSpellSlot_10", G::Combat, "Spell Slot 10"),
    row(M::MAGIC, "UseSpellSlot_11", G::Combat, "Spell Slot 11"),
    row(M::MAGIC, "UseSpellSlot_12", G::Combat, "Spell Slot 12"),
    row(M::OWN, "UseSpellSlot_13", G::Combat, "Spell Slot 13"),
    row(M::OWN, "UseSpellSlot_14", G::Combat, "Spell Slot 14"),
    row(M::OWN, "UseSpellSlot_15", G::Combat, "Spell Slot 15"),
    row(M::OWN, "UseSpellSlot_16", G::Combat, "Spell Slot 16"),
    row(M::OWN, "UseSpellSlot_17", G::Combat, "Spell Slot 17"),
    row(M::OWN, "UseSpellSlot_18", G::Combat, "Spell Slot 18"),
    row(M::OWN, "UseSpellSlot_19", G::Combat, "Spell Slot 19"),
    row(M::OWN, "UseSpellSlot_20", G::Combat, "Spell Slot 20"),
    row(M::OWN, "UseSpellSlot_21", G::Combat, "Spell Slot 21"),
    row(M::OWN, "UseSpellSlot_22", G::Combat, "Spell Slot 22"),
    row(M::OWN, "UseSpellSlot_23", G::Combat, "Spell Slot 23"),
    row(M::OWN, "UseSpellSlot_24", G::Combat, "Spell Slot 24"),
    row(M::OWN, "UseSpellSlot_25", G::Combat, "Spell Slot 25"),
    row(M::OWN, "UseSpellSlot_26", G::Combat, "Spell Slot 26"),
    row(M::OWN, "UseSpellSlot_27", G::Combat, "Spell Slot 27"),
    row(M::OWN, "UseSpellSlot_28", G::Combat, "Spell Slot 28"),
    row(M::OWN, "UseSpellSlot_29", G::Combat, "Spell Slot 29"),
    row(M::OWN, "UseSpellSlot_30", G::Combat, "Spell Slot 30"),
    row(M::OWN, "UseSpellSlot_31", G::Combat, "Spell Slot 31"),
    row(M::OWN, "UseSpellSlot_32", G::Combat, "Spell Slot 32"),
    row(M::OWN, "UseSpellSlot_33", G::Combat, "Spell Slot 33"),
    row(M::OWN, "UseSpellSlot_34", G::Combat, "Spell Slot 34"),
    row(M::OWN, "UseSpellSlot_35", G::Combat, "Spell Slot 35"),
    row(M::OWN, "UseSpellSlot_36", G::Combat, "Spell Slot 36"),
    row(M::OWN, "UseSpellSlot_37", G::Combat, "Spell Slot 37"),
    row(M::OWN, "UseSpellSlot_38", G::Combat, "Spell Slot 38"),
    row(M::OWN, "UseSpellSlot_39", G::Combat, "Spell Slot 39"),
    row(M::OWN, "UseSpellSlot_40", G::Combat, "Spell Slot 40"),
    row(M::OWN, "UseSpellSlot_41", G::Combat, "Spell Slot 41"),
    row(M::OWN, "UseSpellSlot_42", G::Combat, "Spell Slot 42"),
    row(M::OWN, "UseSpellSlot_43", G::Combat, "Spell Slot 43"),
    row(M::OWN, "UseSpellSlot_44", G::Combat, "Spell Slot 44"),
    row(M::OWN, "UseSpellSlot_45", G::Combat, "Spell Slot 45"),
    row(M::OWN, "UseSpellSlot_46", G::Combat, "Spell Slot 46"),
    row(M::OWN, "UseSpellSlot_47", G::Combat, "Spell Slot 47"),
    row(M::OWN, "UseSpellSlot_48", G::Combat, "Spell Slot 48"),
    row(M::OWN, "UseSpellSlot_49", G::Combat, "Spell Slot 49"),
    row(M::OWN, "UseSpellSlot_50", G::Combat, "Spell Slot 50"),
    row(M::OWN, "UseSpellSlot_51", G::Combat, "Spell Slot 51"),
    row(M::OWN, "UseSpellSlot_52", G::Combat, "Spell Slot 52"),
    row(M::OWN, "UseSpellSlot_53", G::Combat, "Spell Slot 53"),
    row(M::OWN, "UseSpellSlot_54", G::Combat, "Spell Slot 54"),
    row(M::OWN, "UseSpellSlot_55", G::Combat, "Spell Slot 55"),
    row(M::OWN, "UseSpellSlot_56", G::Combat, "Spell Slot 56"),
    row(M::OWN, "UseSpellSlot_57", G::Combat, "Spell Slot 57"),
    row(M::OWN, "UseSpellSlot_58", G::Combat, "Spell Slot 58"),
    row(M::OWN, "UseSpellSlot_59", G::Combat, "Spell Slot 59"),
    row(M::OWN, "UseSpellSlot_60", G::Combat, "Spell Slot 60"),
    row(M::MAGIC, "CombatFirstSpell", G::Combat, "First Spell"),
    row(M::MAGIC, "CombatLastSpell", G::Combat, "Last Spell"),
    row(M::MAGIC, "CombatFirstSpellTab", G::Combat, "First Spell Tab"),
    row(M::MAGIC, "CombatLastSpellTab", G::Combat, "Last Spell Tab"),
    row(M::ITEM_SELECTION, "SelectionSelf", G::Selection, "Select Self"),
    row(M::ITEM_SELECTION, "SelectionPickUp", G::Selection, "Move Selected to Backpack"),
    row(M::ITEM_SELECTION, "SelectionSplitStack", G::Selection, "Split Selected"),
    row(M::ITEM_SELECTION, "SelectionPreviousSelection", G::Selection, "Previous Selection"),
    row(M::ITEM_SELECTION, "SelectionClosestCompassItem", G::Selection, "Closest Compass Item"),
    row(M::ITEM_SELECTION, "SelectionPreviousCompassItem", G::Selection, "Previous Compass Item"),
    row(M::ITEM_SELECTION, "SelectionNextCompassItem", G::Selection, "Next Compass Item"),
    row(M::ITEM_SELECTION, "SelectionClosestItem", G::Selection, "Closest Item"),
    row(M::ITEM_SELECTION, "SelectionPreviousItem", G::Selection, "Previous Item"),
    row(M::ITEM_SELECTION, "SelectionNextItem", G::Selection, "Next Item"),
    row(M::ITEM_SELECTION, "SelectionClosestMonster", G::Selection, "Closest Monster"),
    row(M::ITEM_SELECTION, "SelectionPreviousMonster", G::Selection, "Previous Monster"),
    row(M::ITEM_SELECTION, "SelectionNextMonster", G::Selection, "Next Monster"),
    row(M::ITEM_SELECTION, "SelectionLastAttacker", G::Selection, "Last Attacker"),
    row(M::ITEM_SELECTION, "SelectionClosestPlayer", G::Selection, "Closest Player"),
    row(M::ITEM_SELECTION, "SelectionPreviousPlayer", G::Selection, "Previous Player"),
    row(M::ITEM_SELECTION, "SelectionNextPlayer", G::Selection, "Next Player"),
    row(M::ITEM_SELECTION, "SelectionPreviousFellow", G::Selection, "Previous Fellow"),
    row(M::ITEM_SELECTION, "SelectionNextFellow", G::Selection, "Next Fellow"),
    row(M::ITEM_SELECTION, "SelectionUseClosestUnopenedCorpse", G::Selection, "Use Closest Unopened Corpse"),
    row(M::ITEM_SELECTION, "SelectionUseNextUnopenedCorpse", G::Selection, "Use Next Unopened Corpse"),
    row(M::ITEM_SELECTION, "SelectionGive", G::Selection, "Give Selected"),
    row(M::ITEM_SELECTION, "SelectionDrop", G::Selection, "Drop Selected"),
    row(M::ITEM_SELECTION, "SelectionMoveToMainPack", G::Selection, "Place Selected Object in Main Pack"),
    row(M::ITEM_SELECTION, "SelectionClosestUnopenedCorpse", G::Selection, "Select Closest Unopened Corpse"),
    row(M::ITEM_SELECTION, "SelectionNextUnopenedCorpse", G::Selection, "Select Next Unopened Corpse"),
    row(M::UI, "USE", G::Selection, "Use Selected"),
    row(M::UI, "SelectionExamine", G::Selection, "Examine Selected"),
    row(M::UI, "ToggleAbusePanel", G::Panels, "Abuse Reporting Panel"),
    row(M::UI, "ToggleCharacterInfoPanel", G::Panels, "Character Information Panel"),
    row(M::UI, "TogglePositiveEffectsPanel", G::Panels, "Helpful Spells Panel"),
    row(M::UI, "ToggleNegativeEffectsPanel", G::Panels, "Harmful Spells Panel"),
    row(M::UI, "ToggleLinkStatusPanel", G::Panels, "Link Status Panel"),
    row(M::UI, "ToggleUrgentAssistancePanel", G::Panels, "Urgent Assistance Panel"),
    row(M::UI, "ToggleVitaePanel", G::Panels, "Vitae Panel"),
    row(M::UI, "ToggleSocialPanel", G::Panels, "Social Panel"),
    row(M::UI, "ToggleAllegiancePanel", G::Panels, "Allegiance Panel"),
    row(M::UI, "ToggleFellowshipPanel", G::Panels, "Fellowship Panel"),
    row(M::UI, "ToggleSpellManagementPanel", G::Panels, "Spell Management Panel"),
    row(M::UI, "ToggleSpellbookPanel", G::Panels, "Spellbook Panel"),
    row(M::UI, "ToggleSpellComponentsPanel", G::Panels, "Spell Components Panel"),
    row(M::UI, "ToggleSkillManagementPanel", G::Panels, "Character Detail Panel"),
    row(M::UI, "ToggleAttributesPanel", G::Panels, "Attributes Panel"),
    row(M::UI, "ToggleSkillsPanel", G::Panels, "Skills Panel"),
    row(M::UI, "ToggleWorldPanel", G::Panels, "World Panel"),
    row(M::UI, "ToggleMapPanel", G::Panels, "Map Panel"),
    row(M::UI, "ToggleHousePanel", G::Panels, "House Panel"),
    row(M::UI, "ToggleInventoryPanel", G::Panels, "Inventory Panel"),
    row(M::UI, "ToggleOptionsPanel", G::Panels, "Options Panel"),
    row(M::UI, "ToggleGameplayOptionsPanel", G::Panels, "Gameplay Options Panel"),
    row(M::UI, "ToggleCharacterOptionsPanel", G::Panels, "Character Options Panel"),
    row(M::UI, "ToggleConfigOptionsPanel", G::Panels, "Sound And Graphics Panel"),
    row(M::UI, "ToggleRadarPanel", G::Panels, "Compass Panel"),
    row(M::UI, "ToggleKeyboardPanel", G::Panels, "Keyboard Configuration Panel"),
    row(M::UI, "ToggleFriendsPanel", G::Panels, "Friends Panel"),
    row(M::UI, "ToggleCharacterTitlePanel", G::Panels, "Character Title Panel"),
    row(M::UI, "ToggleJournalPanel", G::Panels, "Journal Panel"),
    row(M::UI, "TogglePageListPanel", G::Panels, "Journal Page List Panel"),
    row(M::UI, "ToggleContractsPanel", G::Panels, "Contracts Panel"),
    row(M::OWN, "ToggleTradePanel", G::Panels, "Trade Panel"),
    row(M::OWN, "ToggleSpellResearchPanel", G::Panels, "Spell Research Panel"),
    row(M::UI, "ToggleFloatingChatWindow1", G::Chat, "Toggle Floating Chat Window 1"),
    row(M::UI, "ToggleFloatingChatWindow2", G::Chat, "Toggle Floating Chat Window 2"),
    row(M::UI, "ToggleFloatingChatWindow3", G::Chat, "Toggle Floating Chat Window 3"),
    row(M::UI, "ToggleFloatingChatWindow4", G::Chat, "Toggle Floating Chat Window 4"),
    row(M::CHAT, "MonarchReply", G::Chat, "Monarch Reply"),
    row(M::CHAT, "PatronReply", G::Chat, "Patron Reply"),
    row(M::CHAT, "Reply", G::Chat, "Reply"),
    row(M::CHAT, "EnterChatMode", G::Chat, "Enter Chat Mode"),
    row(M::CHAT, "START_COMMAND", G::Chat, "Issue Slash Command"),
    row(M::CHAT, "TellSelected", G::Chat, "Tell to Selected"),
    row(M::TOGGLE_CHAT_ENTRY, "ToggleChatEntry", G::Chat, "Switch Chat Mode"),
    row(M::QUICKSLOTS, "UseQuickSlot_1", G::Shortcuts, "Use Shortcut 1"),
    row(M::QUICKSLOTS, "UseQuickSlot_2", G::Shortcuts, "Use Shortcut 2"),
    row(M::QUICKSLOTS, "UseQuickSlot_3", G::Shortcuts, "Use Shortcut 3"),
    row(M::QUICKSLOTS, "UseQuickSlot_4", G::Shortcuts, "Use Shortcut 4"),
    row(M::QUICKSLOTS, "UseQuickSlot_5", G::Shortcuts, "Use Shortcut 5"),
    row(M::QUICKSLOTS, "UseQuickSlot_6", G::Shortcuts, "Use Shortcut 6"),
    row(M::QUICKSLOTS, "UseQuickSlot_7", G::Shortcuts, "Use Shortcut 7"),
    row(M::QUICKSLOTS, "UseQuickSlot_8", G::Shortcuts, "Use Shortcut 8"),
    row(M::QUICKSLOTS, "UseQuickSlot_9", G::Shortcuts, "Use Shortcut 9"),
    row(M::QUICKSLOTS, "SelectQuickSlot_1", G::Shortcuts, "Select Shortcut 1"),
    row(M::QUICKSLOTS, "SelectQuickSlot_2", G::Shortcuts, "Select Shortcut 2"),
    row(M::QUICKSLOTS, "SelectQuickSlot_3", G::Shortcuts, "Select Shortcut 3"),
    row(M::QUICKSLOTS, "SelectQuickSlot_4", G::Shortcuts, "Select Shortcut 4"),
    row(M::QUICKSLOTS, "SelectQuickSlot_5", G::Shortcuts, "Select Shortcut 5"),
    row(M::QUICKSLOTS, "SelectQuickSlot_6", G::Shortcuts, "Select Shortcut 6"),
    row(M::QUICKSLOTS, "SelectQuickSlot_7", G::Shortcuts, "Select Shortcut 7"),
    row(M::QUICKSLOTS, "SelectQuickSlot_8", G::Shortcuts, "Select Shortcut 8"),
    row(M::QUICKSLOTS, "SelectQuickSlot_9", G::Shortcuts, "Select Shortcut 9"),
    row(M::QUICKSLOTS, "CreateShortcut", G::Shortcuts, "Create Shortcut"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_AutoRepeatAttack", G::CharacterOptions, "Auto-Repeat Attacks"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_IgnoreAllegianceRequests", G::CharacterOptions, "Ignore Allegiance Requests"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_IgnoreFellowshipRequests", G::CharacterOptions, "Ignore Fellowship Requests"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_IgnoreTradeRequests", G::CharacterOptions, "Ignore Trade Requests"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_PersistentAtDay", G::CharacterOptions, "Always Daylight Outdoors"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_AllowGive", G::CharacterOptions, "Let Players Give You Items"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_ViewCombatTarget", G::CharacterOptions, "Auto-Track Combat Targets"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_ShowTooltips", G::CharacterOptions, "Display Tooltips"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_UseDeception", G::CharacterOptions, "Attempt To Deceive Players"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_ToggleRun", G::CharacterOptions, "Run As Default Movement"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_StayInChatMode", G::CharacterOptions, "Stay In Chat Mode After Send"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_AdvancedCombatUI", G::CharacterOptions, "Advanced Combat Interface"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_AutoTarget", G::CharacterOptions, "Auto-Target"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_VividTargetingIndicator", G::CharacterOptions, "Vivid Target Indicator"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_FellowshipShareXP", G::CharacterOptions, "Share Fellowship X P"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_AcceptLootPermits", G::CharacterOptions, "Accept Corpse Looting"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_FellowshipShareLoot", G::CharacterOptions, "Share Fellowship Loot"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_FellowshipAutoAcceptRequests", G::CharacterOptions, "Automatically Accept Fellowship Requests"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_CoordinatesOnRadar", G::CharacterOptions, "Show Radar Coordinates"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_SpellDuration", G::CharacterOptions, "Show Spell Durations"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisableHouseRestrictionEffects", G::CharacterOptions, "Disable House Effect"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DragItemOnPlayerOpensSecureTrade", G::CharacterOptions, "Drag Item to Player Opens Trade"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisplayAllegianceLogonNotifications", G::CharacterOptions, "Show Allegiance Logons"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_UseChargeAttack", G::CharacterOptions, "Use Charge Attack"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_UseCraftSuccessDialog", G::CharacterOptions, "Toggle Crafting Chance Of Success Dialog"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_HearAllegianceChat", G::CharacterOptions, "Allegiance Chat"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisplayDateOfBirth", G::CharacterOptions, "Allow Others to See Your Date of Birth"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisplayAge", G::CharacterOptions, "Allow Others to See Your Age"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisplayChessRank", G::CharacterOptions, "Allow Others to See Your Chess Rank"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisplayFishingSkill", G::CharacterOptions, "Allow Others to See Your Fishing Skill"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisplayNumberDeaths", G::CharacterOptions, "Allow Others to See Your Number of Deaths"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisplayTimeStamps", G::CharacterOptions, "Display Timestamps"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_SalvageMultiple", G::CharacterOptions, "Salvage Multiple Materials at Once"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_HearGeneralChat", G::CharacterOptions, "Listen to General Chat"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_HearTradeChat", G::CharacterOptions, "Listen to Trade Chat"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_HearLFGChat", G::CharacterOptions, "Listen to LFG Chat"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_HearRoleplayChat", G::CharacterOptions, "Listen to Roleplay Chat"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisplayNumberCharacterTitles", G::CharacterOptions, "Allow Others to See Your Number of Titles"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_MainPackPreferred", G::CharacterOptions, "Use Main Pack as Default for Picking Up Items"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_LeadMissileTargets", G::CharacterOptions, "Lead Missile Targets"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_UseFastMissiles", G::CharacterOptions, "Use Fast Missiles"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_FilterLanguage", G::CharacterOptions, "Filter Language"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_ConfirmVolatileRareUse", G::CharacterOptions, "Confirm Use of Rare Gems"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_HearSocietyChat", G::CharacterOptions, "Listen to Society Chat"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_ShowHelm", G::CharacterOptions, "Your helmet or head gear is visible."),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisableDistanceFog", G::CharacterOptions, "Turn off distance fog on the landscape."),
    row(M::CHARACTER_OPTIONS, "PlayerOption_ShowCloak", G::CharacterOptions, "Your cloak is visible."),
    row(M::CHARACTER_OPTIONS, "PlayerOption_SideBySideVitals", G::CharacterOptions, "Side By Side Vitals"),
    row_not_used(M::OWN, "PlayerOption_AutoCreateShortcuts", G::CharacterOptions, "Auto-Create Shortcuts", Some(Why::AUTO_CREATE_SHORTCUTS)),
    row(M::OWN, "ToggleInvertMouseLook", G::CharacterOptions, "Invert Mouse Look"),
    row_not_used(M::OWN, "ToggleRightClickMouseLook", G::CharacterOptions, "Right Click To Mouse Look", Some(Why::RIGHT_CLICK_MOUSE_LOOK)),
    row_not_used(M::OWN, "ToggleStretchUI", G::CharacterOptions, "Stretch U I", Some(Why::STRETCH_UI)),
    row(M::OWN, "ToggleMuteOnLosingFocus", G::CharacterOptions, "Mute On Losing Focus"),
    row(M::CHARACTER_OPTIONS, "PlayerOption_DisableMostWeatherEffects", G::CharacterOptions, "Disable Weather"),
    row(M::EMOTES, "AFKState", G::Emotes, "Chat Pose \"A F K State\""),
    row(M::EMOTES, "Akimbo", G::Emotes, "Chat Pose \"Akimbo\""),
    row(M::EMOTES, "ATOYOT", G::Emotes, "Chat Pose \"ATOYOT State\""),
    row(M::EMOTES, "AkimboState", G::Emotes, "Chat Pose \"Akimbo State\""),
    row(M::EMOTES, "AtEaseState", G::Emotes, "Chat Pose \"At Ease State\""),
    row(M::EMOTES, "Beckon", G::Emotes, "Chat Pose \"Beckon\""),
    row(M::EMOTES, "BeSeeingYou", G::Emotes, "Chat Pose \"Be Seeing You\""),
    row(M::EMOTES, "BlowKiss", G::Emotes, "Chat Pose \"Blow Kiss\""),
    row(M::EMOTES, "BowDeep", G::Emotes, "Chat Pose \"Bow Deep\""),
    row(M::EMOTES, "BowDeepState", G::Emotes, "Chat Pose \"Bow Deep State\""),
    row(M::EMOTES, "Cheer", G::Emotes, "Chat Pose \"Cheer\""),
    row(M::EMOTES, "ClapHands", G::Emotes, "Chat Pose \"Clap Hands\""),
    row(M::EMOTES, "ClapHandsState", G::Emotes, "Chat Pose \"Clap Hands State\""),
    row(M::EMOTES, "Cringe", G::Emotes, "Chat Pose \"Cringe\""),
    row(M::EMOTES, "CrossArmsState", G::Emotes, "Chat Pose \"Cross Arms State\""),
    row(M::EMOTES, "Cry", G::Emotes, "Chat Pose \"Cry\""),
    row(M::EMOTES, "CurtseyState", G::Emotes, "Chat Pose \"Curtsey State\""),
    row(M::EMOTES, "DrudgeDance", G::Emotes, "Chat Pose \"Drudge Dance\""),
    row(M::EMOTES, "DrudgeDanceState", G::Emotes, "Chat Pose \"Drudge Dance State\""),
    row(M::EMOTES, "HaveASeat", G::Emotes, "Chat Pose \"Have A Seat\""),
    row(M::EMOTES, "HaveASeatState", G::Emotes, "Chat Pose \"Have A Seat State\""),
    row(M::EMOTES, "HeartyLaugh", G::Emotes, "Chat Pose \"Hearty Laugh\""),
    row(M::EMOTES, "Helper", G::Emotes, "Chat Pose \"Helper\""),
    row(M::EMOTES, "Kneel", G::Emotes, "Chat Pose \"Kneel\""),
    row(M::EMOTES, "KneelState", G::Emotes, "Chat Pose \"Kneel State\""),
    row(M::EMOTES, "Knock", G::Emotes, "Chat Pose \"Knock\""),
    row(M::EMOTES, "Laugh", G::Emotes, "Chat Pose \"Laugh\""),
    row(M::EMOTES, "LeanState", G::Emotes, "Chat Pose \"Lean State\""),
    row(M::EMOTES, "MeditateState", G::Emotes, "Chat Pose \"Meditate State\""),
    row(M::EMOTES, "MimeDrink", G::Emotes, "Chat Pose \"Mime Drink\""),
    row(M::EMOTES, "MimeEat", G::Emotes, "Chat Pose \"Mime Eat\""),
    row(M::EMOTES, "Mock", G::Emotes, "Chat Pose \"Mock\""),
    row(M::EMOTES, "Nod", G::Emotes, "Chat Pose \"Nod\""),
    row(M::EMOTES, "NudgeLeft", G::Emotes, "Chat Pose \"Nudge Left\""),
    row(M::EMOTES, "NudgeRight", G::Emotes, "Chat Pose \"Nudge Right\""),
    row(M::EMOTES, "Plead", G::Emotes, "Chat Pose \"Plead\""),
    row(M::EMOTES, "PleadState", G::Emotes, "Chat Pose \"Plead State\""),
    row(M::EMOTES, "Point", G::Emotes, "Chat Pose \"Point\""),
    row(M::EMOTES, "PointState", G::Emotes, "Chat Pose \"Point State\""),
    row(M::EMOTES, "PointDown", G::Emotes, "Chat Pose \"Point Down\""),
    row(M::EMOTES, "PointDownState", G::Emotes, "Chat Pose \"Point Down State\""),
    row(M::EMOTES, "PointLeft", G::Emotes, "Chat Pose \"Point Left\""),
    row(M::EMOTES, "PointLeftState", G::Emotes, "Chat Pose \"Point Left State\""),
    row(M::EMOTES, "PointRight", G::Emotes, "Chat Pose \"Point Right\""),
    row(M::EMOTES, "PointRightState", G::Emotes, "Chat Pose \"Point Right State\""),
    row(M::EMOTES, "PossumState", G::Emotes, "Chat Pose \"Possum State\""),
    row(M::EMOTES, "Pray", G::Emotes, "Chat Pose \"Pray\""),
    row(M::EMOTES, "PrayState", G::Emotes, "Chat Pose \"Pray State\""),
    row(M::EMOTES, "ReadState", G::Emotes, "Chat Pose \"Read State\""),
    row(M::EMOTES, "Salute", G::Emotes, "Chat Pose \"Salute\""),
    row(M::EMOTES, "SaluteState", G::Emotes, "Chat Pose \"Salute State\""),
    row(M::EMOTES, "ScanHorizon", G::Emotes, "Chat Pose \"Scan Horizon\""),
    row(M::EMOTES, "ScratchHead", G::Emotes, "Chat Pose \"Scratch Head\""),
    row(M::EMOTES, "ScratchHeadState", G::Emotes, "Chat Pose \"Scratch Head State\""),
    row(M::EMOTES, "ShakeFist", G::Emotes, "Chat Pose \"Shake Fist\""),
    row(M::EMOTES, "ShakeFistState", G::Emotes, "Chat Pose \"Shake Fist State\""),
    row(M::EMOTES, "ShakeHead", G::Emotes, "Chat Pose \"Shake Head\""),
    row(M::EMOTES, "Shiver", G::Emotes, "Chat Pose \"Shiver\""),
    row(M::EMOTES, "ShiverState", G::Emotes, "Chat Pose \"Shiver State\""),
    row(M::EMOTES, "Shoo", G::Emotes, "Chat Pose \"Shoo\""),
    row(M::EMOTES, "Shrug", G::Emotes, "Chat Pose \"Shrug\""),
    row(M::EMOTES, "SitState", G::Emotes, "Chat Pose \"Sit State\""),
    row(M::EMOTES, "SitBackState", G::Emotes, "Chat Pose \"Sit Back State\""),
    row(M::EMOTES, "SitCrossleggedState", G::Emotes, "Chat Pose \"Sit Crosslegged State\""),
    row(M::EMOTES, "Slouch", G::Emotes, "Chat Pose \"Slouch\""),
    row(M::EMOTES, "SlouchState", G::Emotes, "Chat Pose \"Slouch State\""),
    row(M::EMOTES, "SmackHead", G::Emotes, "Chat Pose \"Smack Head\""),
    row(M::EMOTES, "SnowAngelState", G::Emotes, "Chat Pose \"Snow Angel State\""),
    row(M::EMOTES, "Spit", G::Emotes, "Chat Pose \"Spit\""),
    row(M::EMOTES, "Surrender", G::Emotes, "Chat Pose \"Surrender\""),
    row(M::EMOTES, "SurrenderState", G::Emotes, "Chat Pose \"Surrender State\""),
    row(M::EMOTES, "TalktotheHandState", G::Emotes, "Chat Pose \"Talktothe Hand State\""),
    row(M::EMOTES, "TapFoot", G::Emotes, "Chat Pose \"Tap Foot\""),
    row(M::EMOTES, "TapFootState", G::Emotes, "Chat Pose \"Tap Foot State\""),
    row(M::EMOTES, "Teapot", G::Emotes, "Chat Pose \"Teapot\""),
    row(M::EMOTES, "ThinkerState", G::Emotes, "Chat Pose \"Thinker State\""),
    row(M::EMOTES, "WarmHands", G::Emotes, "Chat Pose \"Warm Hands\""),
    row(M::EMOTES, "Wave", G::Emotes, "Chat Pose \"Wave\""),
    row(M::EMOTES, "WaveState", G::Emotes, "Chat Pose \"Wave State\""),
    row(M::EMOTES, "WaveLow", G::Emotes, "Chat Pose \"Wave Low\""),
    row(M::EMOTES, "WaveHigh", G::Emotes, "Chat Pose \"Wave High\""),
    row(M::EMOTES, "Winded", G::Emotes, "Chat Pose \"Winded\""),
    row(M::EMOTES, "WindedState", G::Emotes, "Chat Pose \"Winded State\""),
    row(M::EMOTES, "Woah", G::Emotes, "Chat Pose \"Woah\""),
    row(M::EMOTES, "WoahState", G::Emotes, "Chat Pose \"Woah State\""),
    row(M::EMOTES, "YawnStretch", G::Emotes, "Chat Pose \"Yawn Stretch\""),
    row(M::EMOTES, "YMCA", G::Emotes, "Chat Pose \"YMCA\""),
    row(M::UI, "CaptureScreenshot", G::Other, "Capture Screenshot To File"),
    row(M::UI, "ToggleHelp", G::Other, "Help"),
    row(M::UI, "TogglePluginManager", G::Other, "Display Plugin Manager"),
    row(M::UI, "LOGOUT", G::Other, "End Character Session"),
    row(M::OWN, "TogglePerformancePanel", G::Other, "Performance Panel"),
    row_not_used(M::OWN, "Cancel", G::Other, "Cancel / Options Panel", Some(Why::CANCEL)),
    row_not_used(M::OWN, "RepeatLastMessage", G::Chat, "Repeat Message (in chat box)", Some(Why::REPEAT_MESSAGE)),
    row(M::OWN, "LookAtFront", G::Camera, "Look At Your Front"),
    row(M::OWN, "ToggleAdminWindow", G::Other, "Admin Window"),
];

#[cfg(test)]
mod era_tests {
    use super::*;

    fn row_named(name: &str) -> &'static Row {
        ROWS.iter()
            .chain(EXTRA_ROWS)
            .find(|r| r.action_name == name)
            .unwrap_or_else(|| panic!("no row {name}"))
    }

    /// A key for a system the world lacks is not listed, each by its own system; with everything
    /// on, or a world whose era is not known, the page is the whole page.
    #[test]
    fn a_key_row_follows_the_system_of_its_action() {
        let without = |f: fn(&mut EraFeatures)| {
            let mut features = EraFeatures::ALL;
            f(&mut features);
            features
        };
        let flags: [(&str, EraFeatures); 20] = [
            (
                "PlayerOption_FellowshipShareLoot",
                without(|f| f.fellowship_share_loot = false),
            ),
            (
                "PlayerOption_FellowshipAutoAcceptRequests",
                without(|f| f.fellowship_auto_accept = false),
            ),
            (
                "PlayerOption_HearAllegianceChat",
                without(|f| f.allegiance_chat = false),
            ),
            ("ToggleHousePanel", without(|f| f.housing = false)),
            ("ToggleCharacterTitlePanel", without(|f| f.titles = false)),
            ("ToggleJournalPanel", without(|f| f.journal = false)),
            ("TogglePageListPanel", without(|f| f.journal = false)),
            ("ToggleContractsPanel", without(|f| f.contracts = false)),
            ("ToggleTradePanel", without(|f| f.trade = false)),
            (
                "PlayerOption_HearGeneralChat",
                without(|f| f.global_chat_rooms = false),
            ),
            ("CombatNextSpellTab", without(|f| f.spell_tabs = false)),
            ("CombatLastSpellTab", without(|f| f.spell_tabs = false)),
            ("PlayerOption_ShowCloak", without(|f| f.cloaks = false)),
            (
                "PlayerOption_DisplayChessRank",
                without(|f| f.chess = false),
            ),
            (
                "PlayerOption_SalvageMultiple",
                without(|f| f.tinkering = false),
            ),
            (
                "PlayerOption_FellowshipShareXP",
                without(|f| f.fellowship_options = false),
            ),
            (
                "PlayerOption_AcceptLootPermits",
                without(|f| f.corpse_permissions = false),
            ),
            (
                "PlayerOption_IgnoreTradeRequests",
                without(|f| f.trade = false),
            ),
            (
                "PlayerOption_DragItemOnPlayerOpensSecureTrade",
                without(|f| f.trade = false),
            ),
            (
                "PlayerOption_ConfirmVolatileRareUse",
                without(|f| f.pre_order_items_and_rares = false),
            ),
        ];
        for (name, off) in flags {
            let row = row_named(name);
            assert!(row.needs().met(Some(&EraFeatures::ALL)), "{name}");
            assert!(!row.needs().met(Some(&off)), "{name}");
            assert!(row.needs().met(None), "{name}");
        }
        for row in ROWS {
            assert_eq!(row.listed(None), row.shown(), "{}", row.action_name);
            assert_eq!(
                row.listed(Some(&EraFeatures::ALL)),
                row.shown(),
                "{}",
                row.action_name
            );
        }
    }

    /// Every option key names an option of the Character Options page, so the key's answer is the
    /// row's own.
    #[test]
    fn every_option_key_with_a_need_reads_it_from_the_options_row() {
        for option in PlayerOption::ALL {
            let name = format!("PlayerOption_{option:?}");
            let Some(row) = ROWS.iter().find(|r| r.action_name == name) else {
                continue;
            };
            let want = sheet::row_of_option(option).map_or(Needs::Nothing, |r| r.needs);
            assert_eq!(row.needs(), want, "{name}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_leaves_out_what_the_interface_has_no_place_for_and_only_that() {
        let shown = |name: &str| {
            ROWS.iter()
                .filter(|r| r.action_name == name)
                .any(Row::shown)
        };
        assert!(!shown("CameraViewFirstPerson"));
        assert!(shown("CameraMoveToward"));
        for name in HIDDEN {
            assert!(
                ROWS.iter().any(|r| r.action_name == *name),
                "{name} is a row"
            );
        }
    }

    #[test]
    fn the_page_leaves_out_the_alternate_camera_the_character_options_and_the_dropped_keys() {
        let page: Vec<(InputMapId, &str)> = ROWS
            .iter()
            .filter(|r| r.shown())
            .map(|r| (r.map, r.action_name))
            .collect();
        assert!(!page.iter().any(|(m, _)| *m == map::CAMERA_ALTERNATE));
        assert!(!page.iter().any(|(m, _)| *m == map::CHARACTER_OPTIONS));
        for name in [
            "MovementHoldSidestep",
            "CameraActivateAlternateMode",
            "ToggleFriendsPanel",
            "ToggleChatEntry",
            "CreateShortcut",
            "LOGOUT",
        ] {
            assert!(!page.iter().any(|(_, n)| *n == name), "{name}");
        }
        assert!(page.contains(&(map::CAMERA, "CameraMoveToward")));
        assert!(page.contains(&(map::CHAT, "EnterChatMode")));
        // The shortcut bar's last three slots: on the page only, after the ninth.
        let page: Vec<&str> = page_rows().map(|r| r.action_name).collect();
        let ninth = page.iter().position(|n| *n == "UseQuickSlot_9").unwrap();
        assert_eq!(
            page[ninth + 1..ninth + 4],
            ["UseQuickSlot_10", "UseQuickSlot_11", "UseQuickSlot_12"]
        );
        for r in EXTRA_ROWS {
            let _ = r.action();
            assert!(!ROWS.iter().any(|x| x.action_name == r.action_name));
        }
        let walk = ROWS
            .iter()
            .find(|r| r.action_name == "MovementWalkMode")
            .unwrap();
        assert_eq!(walk.caption(), "Toggle Walk/Run");
        assert_eq!(walk.label, "Hold Run");
    }

    #[test]
    fn every_row_names_a_known_action_once_in_its_map() {
        let mut seen = std::collections::BTreeSet::new();
        for r in ROWS {
            let _ = r.action();
            assert!(
                seen.insert((r.map, r.action().0)),
                "{} twice",
                r.action_name
            );
            assert!(!r.label.is_empty(), "{}", r.action_name);
        }
        assert_eq!(ROWS.len(), 358);
    }

    #[test]
    fn the_rows_that_do_nothing_are_the_ones_with_no_counterpart() {
        let unused: Vec<&str> = ROWS
            .iter()
            .filter(|r| r.not_used.is_some())
            .map(|r| r.action_name)
            .collect();
        assert_eq!(
            unused,
            [
                "PlayerOption_AutoCreateShortcuts",
                "ToggleRightClickMouseLook",
                "ToggleStretchUI",
                "Cancel",
                "RepeatLastMessage",
            ]
        );
    }

    #[test]
    fn the_hidden_quickslots_and_the_quest_detail_panel_are_not_rows() {
        for name in [
            "UseQuickSlot_10",
            "UseQuickSlot_18",
            "ToggleQuestManagementPanel",
        ] {
            assert!(ROWS.iter().all(|r| r.action_name != name), "{name}");
        }
        assert!(ROWS.iter().any(|r| r.action_name == "UseQuickSlot_9"));
    }

    #[test]
    fn option_rows_name_the_options_on_the_sheet() {
        use pyreal_client_contract::{options::sheet, view::PlayerOption};
        for key in ROWS.iter().filter(|r| r.group == Group::CharacterOptions) {
            let Some(name) = key.action_name.strip_prefix("PlayerOption_") else {
                continue;
            };
            if name == "AutoCreateShortcuts" {
                continue;
            }
            let option = PlayerOption::ALL
                .into_iter()
                .find(|o| format!("{o:?}") == name)
                .expect("named player option");
            assert!(sheet::row_of_option(option).is_some(), "{name}");
        }
        // Era capabilities affect the options page; they do not delete the binding vocabulary.
        let cloak = sheet::row_of_option(PlayerOption::ShowCloak).expect("cloak option");
        assert_eq!(cloak.needs, sheet::Needs::Cloaks);
        assert!(ROWS
            .iter()
            .any(|r| r.action_name == "PlayerOption_ShowCloak"));
    }
}
