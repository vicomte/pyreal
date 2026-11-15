//! The options the interface shows, as one set: four pages, each a list of headings, each
//! heading a list of rows.
//!
//! The Game / Support page's buttons, the Character Options page (the character's own options,
//! kept by the server), the Chat Options page and the Client Options page (sound, display,
//! graphics and camera, kept in the profile). A row names where its value is kept ([`Value`]);
//! nothing here keeps a value of its own. A row for something the world's era does not have is
//! left out ([`Needs`]).
//! So is a row for something only some hosts do, where the host does not ([`Row::hosted`]): the
//! render scale, which only a window that goes by points offers.

use crate::options::config::PrefValueConst;
use crate::view::PlayerOption;

/// The four pages, in the order the tabs list them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PageId {
    /// The buttons: leaving the world, the keys, the support forms.
    GameSupport,
    /// The character's own options, kept by the server.
    Character,
    /// The chat windows' options.
    Chat,
    /// Sound, display, graphics and camera, kept in the profile.
    Client,
}

/// A localized options caption with an honest literal fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Text {
    pub table_enum: u32,
    pub token: Option<&'static str>,
    pub fallback: &'static str,
}
impl Text {
    #[must_use]
    pub const fn literal(fallback: &'static str) -> Self {
        Self {
            table_enum: 0x1000_0003,
            token: None,
            fallback,
        }
    }
    #[must_use]
    pub const fn preference(token: &'static str, fallback: &'static str) -> Self {
        Self {
            table_enum: 0x1000_0003,
            token: Some(token),
            fallback,
        }
    }
}

/// One page.
#[derive(Debug, Clone, Copy)]
pub struct Page {
    pub id: PageId,
    pub title: &'static str,
    pub headings: &'static [Heading],
}

/// One heading of a page and the rows under it.
#[derive(Debug, Clone, Copy)]
pub struct Heading {
    pub title: &'static str,
    pub text: Text,
    pub rows: &'static [Row],
}

/// What a row needs of the world's era to mean anything. A row whose need is not met is left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    /// Any world.
    Nothing,
    /// Houses.
    Housing,
    /// Rare items.
    Rares,
    /// Cloaks.
    Cloaks,
    /// Character titles.
    Titles,
    /// Secure trade.
    Trade,
    /// Salvaging and the tinkering recipes.
    Tinkering,
    /// Chess on the game boards.
    Chess,
    /// Corpse-looting permissions.
    CorpsePermissions,
    /// A fellowship founder's choice of sharing experience.
    FellowshipOptions,
    /// A fellowship's sharing of loot.
    FellowshipShareLoot,
    /// Accepting fellowship invitations at once.
    FellowshipAutoAccept,
    /// The allegiance chat room.
    AllegianceChat,
    /// The game's own chat rooms.
    GlobalChatRooms,
    /// The journal.
    Journal,
    /// The contracts.
    Contracts,
    /// The Create Spell page.
    SpellResearch,
    /// The spellcasting panel's tabs of favorites.
    SpellTabs,
    /// Anything but a Launch world: an option the client came to have after it.
    AfterLaunch,
}

impl Needs {
    /// Whether a world with `features` has what the row needs. A world whose era is not known
    /// has everything. The answer for a system with a window is that window's own.
    #[must_use]
    pub fn met(self, features: Option<&pyreal_primitives::EraFeatures>) -> bool {
        use crate::era::EraSurface as S;
        let Some(&f) = features else { return true };
        match self {
            Self::Nothing => true,
            Self::Housing => S::House.offered(f),
            Self::Rares => f.pre_order_items_and_rares,
            Self::Cloaks => S::CloakSlot.offered(f),
            Self::Titles => S::Titles.offered(f),
            Self::Trade => S::Trade.offered(f),
            Self::Tinkering => S::Salvage.offered(f),
            Self::Chess => S::Chess.offered(f),
            Self::CorpsePermissions => f.corpse_permissions,
            Self::FellowshipOptions => f.fellowship_options,
            Self::FellowshipShareLoot => f.fellowship_share_loot,
            Self::FellowshipAutoAccept => f.fellowship_auto_accept,
            Self::AllegianceChat => f.allegiance_chat,
            Self::GlobalChatRooms => f.global_chat_rooms,
            Self::Journal => S::Journal.offered(f),
            Self::Contracts => S::Contracts.offered(f),
            Self::SpellResearch => S::SpellResearch.offered(f),
            Self::SpellTabs => f.spell_tabs,
            Self::AfterLaunch => f != pyreal_primitives::EraFeatures::LAUNCH,
        }
    }
}

/// A button on the Game / Support page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// Asks, then ends the character session.
    ExitToCharacterSelection,
    /// Quits the client.
    ExitGame,
    /// The key bindings page.
    ConfigureKeyboard,
    /// The mouse-turning preset: six Client Options rows and the wheel on the camera zoom.
    MouseTurningSettings,
    /// The in-game form that sends a request for help to the server.
    UrgentAssistance,
    /// The in-game form that sends a complaint to the server's abuse log.
    ReportAbuse,
}

/// Where a row's value is kept, and so what kind of control shows it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    /// A button.
    Action(Act),
    /// One of the character's options, kept by the server.
    Option(PlayerOption),
    /// A bit of the character's first option word that no option of the final client names
    /// (`mask`), kept by the server all the same.
    Bit { mask: u32 },
    /// A check box on a profile preference.
    Check(&'static str),
    /// A slider on a profile preference.
    Slider(&'static str),
    /// A drop-down on a profile preference.
    Menu(&'static str),
    /// A sound: its on/off preference (true is on) and its volume.
    Sound {
        on: &'static str,
        volume: &'static str,
    },
    /// One group of a chat window's message filter: `mask` of the window's filter.
    Filter { window: u32, mask: u64 },
    /// One of the chat windows' two opacities, a gameplay option property.
    Opacity(u32),
}

/// What a row costs in frame rate, as its tooltip says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Impact {
    High,
    Medium,
    Low,
    /// A visual preference only.
    None,
    /// Low to High by the preset chosen.
    Preset,
    /// Nothing drawn: a read-out.
    Information,
    /// Memory and loading, by the art pack chosen.
    Pack,
}

impl Impact {
    /// The tooltip's line.
    #[must_use]
    pub const fn line(self) -> &'static str {
        match self {
            Self::High => "Performance: High impact",
            Self::Medium => "Performance: Medium impact",
            Self::Low => "Performance: Low impact",
            Self::None => "Performance: None (visual preference only)",
            Self::Preset => "Performance: Low to High, by preset",
            Self::Information => "Performance: None",
            Self::Pack => "Performance: Depends on the pack (memory and loading)",
        }
    }
}

/// One row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Row {
    pub caption: &'static str,
    pub value: Value,
    pub needs: Needs,
    /// The value the page's Defaults button gives a profile preference row; `None` for a row
    /// whose default is the character's, the chat window's, or none at all.
    pub default: Option<PrefValueConst>,
    /// The volume half of a sound row's UI restore value.
    pub volume_default: Option<f32>,
    /// Preference-table tokens for a slider's end captions.
    pub slider_ends: Option<(&'static str, &'static str)>,
    /// What the row does when its caption alone does not say, for a tooltip.
    pub note: Option<&'static str>,
    /// What the row costs in frame rate, for the tooltip's last line.
    pub impact: Option<Impact>,
    /// Whether only some hosts offer the row: a host offers it by registering its preference, and
    /// a page leaves it out where the host has not ([`Row::offered`]).
    pub hosted: bool,
}

impl Row {
    /// UI restore value for either preference owned by this row.
    #[must_use]
    pub fn default_for(&self, preference: &str) -> Option<PrefValueConst> {
        if self.preference() == Some(preference) {
            return self.default;
        }
        match self.value {
            Value::Sound { volume, .. } if preference == volume => {
                self.volume_default.map(PrefValueConst::Float)
            }
            _ => None,
        }
    }

    /// Whether this host offers the row: every row but a [`Row::hosted`] one, which a host offers
    /// by registering its preference.
    #[must_use]
    pub fn offered(&self) -> bool {
        !self.hosted || self.preference().is_some_and(super::store::is_registered)
    }

    /// The profile preference the row edits, if it edits one (a sound row's on/off one).
    #[must_use]
    pub const fn preference(&self) -> Option<&'static str> {
        match self.value {
            Value::Check(p) | Value::Slider(p) | Value::Menu(p) => Some(p),
            Value::Sound { on, .. } => Some(on),
            _ => None,
        }
    }
}

const fn row(caption: &'static str, value: Value) -> Row {
    Row {
        caption,
        value,
        needs: Needs::Nothing,
        default: None,
        volume_default: None,
        slider_ends: None,
        note: None,
        impact: None,
        hosted: false,
    }
}

const fn opt(caption: &'static str, option: PlayerOption) -> Row {
    row(caption, Value::Option(option))
}

const fn needs(needs: Needs, r: Row) -> Row {
    Row { needs, ..r }
}

const fn noted(note: &'static str, r: Row) -> Row {
    Row {
        note: Some(note),
        ..r
    }
}

/// A row with its tooltip: what it does and what it costs.
const fn tip(note: &'static str, impact: Impact, r: Row) -> Row {
    Row {
        note: Some(note),
        impact: Some(impact),
        ..r
    }
}

const fn pref(caption: &'static str, value: Value, default: PrefValueConst) -> Row {
    Row {
        default: Some(default),
        volume_default: match value {
            Value::Sound { .. } => Some(1.0),
            _ => None,
        },
        ..row(caption, value)
    }
}

const fn ends(left: &'static str, right: &'static str, r: Row) -> Row {
    Row {
        slider_ends: Some((left, right)),
        ..r
    }
}

/// A row only some hosts offer.
const fn hosted(r: Row) -> Row {
    Row { hosted: true, ..r }
}

/// Static registered bounds shared by the option rows.
pub use super::preferences::range as preference_range;

/// UI restore values in the page's presentation order, including paired volume controls.
#[must_use]
pub fn defaults(page: PageId) -> Vec<(&'static str, PrefValueConst)> {
    rows_for(page)
        .flat_map(|row| {
            let names = match row.value {
                Value::Sound { on, volume } => [Some(on), Some(volume)],
                _ => [row.preference(), None],
            };
            names
                .into_iter()
                .flatten()
                .filter_map(move |p| row.default_for(p).map(|v| (p, v)))
        })
        .collect()
}

const fn button(caption: &'static str, act: Act) -> Row {
    row(caption, Value::Action(act))
}

use PlayerOption as P;
use PrefValueConst::{Bool, Float, Int};

// ---- Game / Support --------------------------------------------------------------------------

const GAME_SUPPORT: [Heading; 1] = [Heading {
    title: "Game and Support",
    text: Text::literal("Game and Support"),
    rows: &[
        button("Exit to Character Selection", Act::ExitToCharacterSelection),
        button("Exit Game", Act::ExitGame),
        button("Configure Keyboard", Act::ConfigureKeyboard),
        button("Use Mouse Turning Settings", Act::MouseTurningSettings),
        button("Urgent Assistance", Act::UrgentAssistance),
        button("Report Abuse", Act::ReportAbuse),
    ],
}];

// ---- Character Options -----------------------------------------------------------------------

const CHARACTER: [Heading; 7] = [
    Heading {
        title: "Interface Behavior",
        text: Text::preference(
            "ID_CharacterOption_UIBehavior_Section",
            "Interface Behavior",
        ),
        rows: &[
            opt("Keep Combat Targets in View", P::ViewCombatTarget),
            needs(
                Needs::Tinkering,
                opt("Salvage Multiple Materials at Once", P::SalvageMultiple),
            ),
            needs(
                Needs::AfterLaunch,
                opt(
                    "Use Main Pack as Default for Picking Up Items",
                    P::MainPackPreferred,
                ),
            ),
            opt("Vivid Targeting Indicator", P::VividTargetingIndicator),
            opt("Display Tooltips", P::ShowTooltips),
            needs(
                Needs::AfterLaunch,
                opt("Show Coordinates By the Radar", P::CoordinatesOnRadar),
            ),
            opt("Side By Side Vitals", P::SideBySideVitals),
            needs(
                Needs::AfterLaunch,
                opt("Display Spell Durations", P::SpellDuration),
            ),
            needs(
                Needs::AfterLaunch,
                opt(
                    "Use Crafting Chance of Success Dialog",
                    P::UseCraftSuccessDialog,
                ),
            ),
            needs(
                Needs::Rares,
                opt("Confirm Use of Rare Gems", P::ConfirmVolatileRareUse),
            ),
        ],
    },
    Heading {
        title: "World Display",
        text: Text::preference("ID_CharacterOption_UIDisplay_Section", "World Display"),
        rows: &[
            opt("Disable Most Weather Effects", P::DisableMostWeatherEffects),
            needs(
                Needs::AfterLaunch,
                opt("Disable Distance Fog", P::DisableDistanceFog),
            ),
            needs(
                Needs::AfterLaunch,
                opt("Always Daylight Outdoors", P::PersistentAtDay),
            ),
            needs(
                Needs::Housing,
                opt(
                    "Disable House Restriction Effects",
                    P::DisableHouseRestrictionEffects,
                ),
            ),
        ],
    },
    Heading {
        title: "Chat",
        text: Text::preference("ID_CharacterOption_Chat_Section", "Chat"),
        rows: &[
            opt(
                "Stay in Chat Mode After Sending a Message",
                P::StayInChatMode,
            ),
            needs(
                Needs::AfterLaunch,
                opt("Display Timestamps", P::DisplayTimeStamps),
            ),
            opt("Filter Language", P::FilterLanguage),
            needs(
                Needs::AllegianceChat,
                opt("Listen to Allegiance Chat", P::HearAllegianceChat),
            ),
            needs(
                Needs::GlobalChatRooms,
                opt("Listen to General Chat", P::HearGeneralChat),
            ),
            needs(
                Needs::GlobalChatRooms,
                opt("Listen to Trade Chat", P::HearTradeChat),
            ),
            needs(
                Needs::GlobalChatRooms,
                opt("Listen to LFG Chat", P::HearLFGChat),
            ),
            needs(
                Needs::GlobalChatRooms,
                opt("Listen to Roleplay Chat", P::HearRoleplayChat),
            ),
            needs(
                Needs::GlobalChatRooms,
                opt("Listen to Society Chat", P::HearSocietyChat),
            ),
            needs(
                Needs::AfterLaunch,
                opt("Listen to PK Death Messages", P::HearPKDeaths),
            ),
        ],
    },
    Heading {
        title: "Fellowship and Allegiance",
        text: Text::preference(
            "ID_CharacterOption_Grouping_Section",
            "Fellowship and Allegiance",
        ),
        rows: &[
            opt("Ignore Allegiance Requests", P::IgnoreAllegianceRequests),
            opt("Ignore Fellowship Requests", P::IgnoreFellowshipRequests),
            needs(
                Needs::AfterLaunch,
                opt(
                    "Show Allegiance Logons",
                    P::DisplayAllegianceLogonNotifications,
                ),
            ),
            needs(
                Needs::FellowshipOptions,
                opt("Share Fellowship Experience", P::FellowshipShareXP),
            ),
            needs(
                Needs::FellowshipShareLoot,
                opt("Share Fellowship Loot", P::FellowshipShareLoot),
            ),
            needs(
                Needs::FellowshipAutoAccept,
                opt(
                    "Automatically Accept Fellowship Requests",
                    P::FellowshipAutoAcceptRequests,
                ),
            ),
        ],
    },
    Heading {
        title: "Other Players",
        text: Text::preference("ID_CharacterOption_OtherPlayers_Section", "Other Players"),
        rows: &[
            needs(
                Needs::CorpsePermissions,
                opt("Accept Corpse-Looting Permissions", P::AcceptLootPermits),
            ),
            opt("Attempt to Deceive Other Players", P::UseDeception),
            opt("Let Other Players Give You Items", P::AllowGive),
            needs(
                Needs::Trade,
                opt("Ignore All Trade Requests", P::IgnoreTradeRequests),
            ),
            needs(
                Needs::Trade,
                opt(
                    "Drag Item to Player Opens Trade",
                    P::DragItemOnPlayerOpensSecureTrade,
                ),
            ),
            needs(
                Needs::AfterLaunch,
                opt("Show Your Helm or Head Gear", P::ShowHelm),
            ),
            needs(Needs::Cloaks, opt("Show Your Cloak", P::ShowCloak)),
        ],
    },
    Heading {
        title: "Allow Others to See Your",
        text: Text::literal("Allow Others to See Your"),
        rows: &[
            needs(
                Needs::AfterLaunch,
                opt("Date of Birth", P::DisplayDateOfBirth),
            ),
            needs(Needs::AfterLaunch, opt("Age", P::DisplayAge)),
            needs(Needs::Chess, opt("Chess Rank", P::DisplayChessRank)),
            needs(
                Needs::AfterLaunch,
                opt("Fishing Skill", P::DisplayFishingSkill),
            ),
            needs(
                Needs::AfterLaunch,
                opt("Number of Deaths", P::DisplayNumberDeaths),
            ),
            needs(
                Needs::Titles,
                opt("Number of Titles", P::DisplayNumberCharacterTitles),
            ),
        ],
    },
    Heading {
        title: "Combat and Movement",
        text: Text::preference(
            "ID_CharacterOption_CharacterBehavior_Section",
            "Combat and Movement",
        ),
        rows: &[
            opt("Run as Default Movement", P::ToggleRun),
            opt("Advanced Combat Interface", P::AdvancedCombatUI),
            opt("Auto Target", P::AutoTarget),
            opt("Automatically Repeat Attacks", P::AutoRepeatAttack),
            needs(
                Needs::AfterLaunch,
                opt("Use Charge Attack", P::UseChargeAttack),
            ),
            needs(
                Needs::AfterLaunch,
                opt("Lead Missile Targets", P::LeadMissileTargets),
            ),
            needs(
                Needs::AfterLaunch,
                opt("Use Fast Missiles", P::UseFastMissiles),
            ),
        ],
    },
];

// ---- Chat Options ----------------------------------------------------------------------------

/// The chat windows' ids, as the options name them.
pub mod window {
    pub use crate::chat::interface::window::{FLOATY_1, FLOATY_2, FLOATY_3, FLOATY_4, MAIN};
}

/// The main chat window's filter before the player sets one: every kind of message but the one
/// meant for the speech bubbles over heads (`1 << 26`).
pub const MAIN_WINDOW_DEFAULT_FILTER: u64 = 0xFBFF_FFFF;

/// The groups of a chat window's message filter, in the order the pages list them: the group's
/// caption and its mask of the filter. The first, Gameplay, is offered for the floating
/// windows only; the main window shows its messages always.
pub const FILTER_GROUPS: [(&str, u64); 13] = [
    ("Gameplay", 0x8391_2021),
    // Chat types 6, 21 and 22.
    ("Combat", 1 << 6 | 1 << 21 | 1 << 22),
    ("Magic", 0x0002_0080),
    ("Area Speech", 0x0000_1004),
    ("Tells", 0x0000_0018),
    ("Allegiance", 0x0004_0C00),
    ("Fellowship", 0x0008_0000),
    ("General", 0x0800_0000),
    ("Trade", 0x1000_0000),
    ("LFG", 0x2000_0000),
    ("Roleplay", 0x4000_0000),
    ("Society", 0x1_0000_0000),
    ("Errors", 0x0400_0000),
];

/// The filter rows of one window: `N` groups of [`FILTER_GROUPS`] from `from` on.
const fn filter_rows<const N: usize>(window: u32, from: usize) -> [Row; N] {
    let mut out = [row("", Value::Filter { window, mask: 0 }); N];
    let mut i = 0;
    while i < N {
        let (caption, mask) = FILTER_GROUPS[from + i];
        out[i] = row(caption, Value::Filter { window, mask });
        // General, Trade, LFG, Roleplay and Society are the rooms' lines.
        if mask >= 0x0800_0000 {
            out[i].needs = Needs::GlobalChatRooms;
        }
        i += 1;
    }
    out
}

const fn floaty_rows(window: u32) -> [Row; 13] {
    filter_rows::<13>(window, 0)
}

const MAIN_FILTERS: [Row; 12] = filter_rows::<12>(window::MAIN, 1);
const FLOATY_1_FILTERS: [Row; 13] = floaty_rows(window::FLOATY_1);
const FLOATY_2_FILTERS: [Row; 13] = floaty_rows(window::FLOATY_2);
const FLOATY_3_FILTERS: [Row; 13] = floaty_rows(window::FLOATY_3);
const FLOATY_4_FILTERS: [Row; 13] = floaty_rows(window::FLOATY_4);

const CHAT: [Heading; 6] = [
    Heading {
        title: "Chat Windows",
        text: Text::preference("ID_ChatOption_GeneralOptions_Section", "Chat Windows"),
        rows: &[
            row("Inactive Window Opacity", Value::Opacity(0x1000_0080)),
            row("Active Window Opacity", Value::Opacity(0x1000_0081)),
            pref(
                "Chat Font",
                Value::Menu(crate::options::names::CHAT_FONT_FACE),
                Int(2),
            ),
            pref(
                "Chat Font Size",
                Value::Menu(crate::options::names::CHAT_FONT_SIZE),
                Int(1),
            ),
        ],
    },
    Heading {
        title: "Main Chat Window",
        text: Text::preference("ID_ChatOption_MainChatWindow_Section", "Main Chat Window"),
        rows: &MAIN_FILTERS,
    },
    Heading {
        title: "Floating Chat Window 1",
        text: Text::preference(
            "ID_ChatOption_FloatyChatWindow1_Section",
            "Floating Chat Window 1",
        ),
        rows: &FLOATY_1_FILTERS,
    },
    Heading {
        title: "Floating Chat Window 2",
        text: Text::preference(
            "ID_ChatOption_FloatyChatWindow2_Section",
            "Floating Chat Window 2",
        ),
        rows: &FLOATY_2_FILTERS,
    },
    Heading {
        title: "Floating Chat Window 3",
        text: Text::preference(
            "ID_ChatOption_FloatyChatWindow3_Section",
            "Floating Chat Window 3",
        ),
        rows: &FLOATY_3_FILTERS,
    },
    Heading {
        title: "Floating Chat Window 4",
        text: Text::preference(
            "ID_ChatOption_FloatyChatWindow4_Section",
            "Floating Chat Window 4",
        ),
        rows: &FLOATY_4_FILTERS,
    },
];

// ---- Client Options --------------------------------------------------------------------------

const SOUND: Heading = Heading {
    title: "Sound",
    text: Text::preference("ID_Sound_SoundSection", "Sound Options"),
    rows: &[
        noted(
            super::sound_style::NOTE,
            pref(
                super::sound_style::CAPTION,
                Value::Menu(crate::options::names::SOUND_STYLE),
                Int(super::sound_style::DEFAULT),
            ),
        ),
        pref(
            "Sound Output",
            Value::Menu(crate::options::names::SOUND_FEATURES),
            Int(0),
        ),
        pref(
            "Sound Effects",
            Value::Sound {
                on: crate::options::names::SOUND_DISABLED,
                volume: crate::options::names::SOUND_VOLUME,
            },
            Bool(true),
        ),
        pref(
            "Ambient Sounds",
            Value::Sound {
                on: crate::options::names::AMBIENT_SOUND_DISABLED,
                volume: crate::options::names::AMBIENT_SOUND_VOLUME,
            },
            Bool(true),
        ),
        // Offered where registered: in a build with the high-fidelity presentation.
        hosted(noted(
            super::weather_sound::NOTE,
            pref(
                super::weather_sound::CAPTION,
                Value::Sound {
                    on: crate::options::names::WEATHER_SOUND,
                    volume: crate::options::names::WEATHER_SOUND_VOLUME,
                },
                Bool(true),
            ),
        )),
        pref(
            "Interface Sounds",
            Value::Sound {
                on: crate::options::names::INTERFACE_SOUND_DISABLED,
                volume: crate::options::names::INTERFACE_SOUND_VOLUME,
            },
            Bool(true),
        ),
        pref(
            "Play Sounds Only When Active",
            Value::Check(crate::options::names::PLAY_SOUND_ONLY_WHEN_ACTIVE),
            Bool(true),
        ),
    ],
};

const CAMERA: Heading = Heading {
    title: "Camera and Mouse",
    text: Text::literal("Camera and Mouse"),
    rows: &[
        ends(
            "ID_Graphics_Value_Soft",
            "ID_Graphics_Value_Hard",
            pref(
                "Camera Stiffness",
                Value::Slider(crate::options::names::CAMERA_STIFFNESS),
                Float(0.45),
            ),
        ),
        ends(
            "ID_Graphics_Value_Slow",
            "ID_Graphics_Value_Fast",
            pref(
                "Camera Adjustment Speed",
                Value::Slider(crate::options::names::CAMERA_ADJUSTMENT_SPEED),
                Float(40.0),
            ),
        ),
        pref(
            "Align Camera to Slope",
            Value::Check(crate::options::names::CAMERA_ALIGN_TO_SLOPE),
            Bool(true),
        ),
        pref(
            "Mouselook Sensitivity",
            Value::Slider(crate::options::names::MOUSE_LOOK_SENSITIVITY),
            Float(0.55),
        ),
        pref(
            "Invert Mouselook Axes",
            Value::Check(crate::options::names::INVERT_MOUSE_LOOK_Y_AXIS),
            Bool(false),
        ),
        pref(
            "Turn Your Character with Camera Turning",
            Value::Check(crate::options::names::USE_MOUSE_TURNING),
            Bool(false),
        ),
    ],
};

/// The Display tab's headings' titles, in order. The Quality heading over the preset is the
/// interface's own ([`super::fidelity::QUALITY_HEADING`]).
pub const SCREEN_HEADING: &str = "Screen & Window";
pub const LIGHTING_HEADING: &str = "Lighting & Shadows";
pub const SKY_HEADING: &str = "Sky & Weather";
pub const SURFACES_HEADING: &str = "Surfaces & Detail";
pub const INFORMATION_HEADING: &str = "Information";

/// The Client Options page: the sound, the display's five topics (each most important first), then
/// the game camera's.
const CLIENT: [Heading; 7] = [
    SOUND,
    Heading {
        title: SCREEN_HEADING,
        text: Text::literal(SCREEN_HEADING),
        rows: &[
            tip(
                super::fidelity::SCALE_NOTE,
                Impact::High,
                pref(
                    super::fidelity::SCALE_CAPTION,
                    Value::Menu(crate::options::names::fidelity::RENDER_SCALE),
                    Int(super::fidelity::SCALE_DEFAULT as i32),
                ),
            ),
            tip(
                super::dynamic_resolution::NOTE,
                Impact::High,
                pref(
                    super::dynamic_resolution::CAPTION,
                    Value::Menu(crate::options::names::fidelity::DYNAMIC_RESOLUTION),
                    Int(super::dynamic_resolution::DEFAULT),
                ),
            ),
            fidelity_box(4, Impact::Low),
            fidelity_box(5, Impact::None),
            fidelity_box(6, Impact::Low),
            tip(
                "Brightens or darkens the whole picture.",
                Impact::None,
                ends(
                    "ID_Graphics_Value_Dark",
                    "ID_Graphics_Value_Bright",
                    pref(
                        "Brightness",
                        Value::Slider(crate::options::names::SCREEN_BRIGHTNESS),
                        Float(0.0),
                    ),
                ),
            ),
            tip(
                "Sets how wide the camera sees. A wider view shows more around you and draws a \
                 little more.",
                Impact::Low,
                ends(
                    "ID_Graphics_Value_Narrow",
                    "ID_Graphics_Value_Wide",
                    pref(
                        "Field of View",
                        Value::Slider(crate::options::names::FIELD_OF_VIEW),
                        Float(90.0),
                    ),
                ),
            ),
            tip(
                "Fills the screen with the game.",
                Impact::None,
                pref(
                    "Full Screen",
                    Value::Check(crate::options::names::DISPLAY_FULL_SCREEN),
                    Bool(false),
                ),
            ),
        ],
    },
    Heading {
        title: LIGHTING_HEADING,
        text: Text::literal(LIGHTING_HEADING),
        rows: &[
            fidelity_box(0, Impact::Medium),
            tip(
                super::shadow_distance::NOTE,
                Impact::Medium,
                pref(
                    super::shadow_distance::CAPTION,
                    Value::Menu(crate::options::names::fidelity::SHADOW_DISTANCE),
                    Int(super::shadow_distance::DEFAULT),
                ),
            ),
            fidelity_box(1, Impact::Medium),
            fidelity_box(2, Impact::Low),
            tip(
                super::light_distance::NOTE,
                Impact::Low,
                pref(
                    super::light_distance::CAPTION,
                    Value::Menu(crate::options::names::fidelity::LIGHT_DRAW_DISTANCE),
                    Int(super::light_distance::DEFAULT),
                ),
            ),
            fidelity_box(8, Impact::None),
        ],
    },
    Heading {
        title: SKY_HEADING,
        text: Text::literal(SKY_HEADING),
        rows: &[
            tip(
                super::cloud_detail::NOTE,
                Impact::Low,
                pref(
                    super::cloud_detail::CAPTION,
                    Value::Menu(crate::options::names::fidelity::CLOUD_DETAIL),
                    Int(super::cloud_detail::DEFAULT),
                ),
            ),
            tip(
                super::god_rays::NOTE,
                Impact::Low,
                pref(
                    super::god_rays::CAPTION,
                    Value::Menu(crate::options::names::fidelity::GOD_RAYS),
                    Int(super::god_rays::DEFAULT),
                ),
            ),
            fidelity_box(3, Impact::Low),
            tip(
                super::fog_density::NOTE,
                Impact::None,
                pref(
                    super::fog_density::CAPTION,
                    Value::Menu(crate::options::names::fidelity::FOG_DENSITY),
                    Int(super::fog_density::DEFAULT),
                ),
            ),
        ],
    },
    Heading {
        title: SURFACES_HEADING,
        text: Text::literal(SURFACES_HEADING),
        rows: &[
            tip(
                super::object_detail::NOTE,
                Impact::High,
                pref(
                    super::object_detail::CAPTION,
                    Value::Menu(crate::options::names::fidelity::OBJECT_DETAIL),
                    Int(super::object_detail::DEFAULT),
                ),
            ),
            tip(
                "Sets how far toward the horizon the landscape is drawn. Distant trees and haze \
                 follow it. Farther distances use more memory and load longer.",
                Impact::Low,
                pref(
                    "Terrain Draw Distance",
                    Value::Menu(crate::options::names::LANDSCAPE_DRAW_DISTANCE),
                    Int(8),
                ),
            ),
            tip(
                super::draw_distance::GROUND_COVER_NOTE,
                Impact::Medium,
                pref(
                    super::draw_distance::GROUND_COVER_CAPTION,
                    Value::Menu(crate::options::names::fidelity::GROUND_COVER_DRAW_DISTANCE),
                    Int(super::draw_distance::GROUND_COVER_DEFAULT),
                ),
            ),
            tip(
                super::grass_height::NOTE,
                Impact::Low,
                pref(
                    super::grass_height::CAPTION,
                    Value::Menu(crate::options::names::fidelity::GRASS_HEIGHT),
                    Int(super::grass_height::DEFAULT),
                ),
            ),
            tip(
                super::depth_mapping::NOTE,
                Impact::Low,
                pref(
                    super::depth_mapping::CAPTION,
                    Value::Menu(crate::options::names::fidelity::DEPTH_MAPPING),
                    Int(super::depth_mapping::DEFAULT),
                ),
            ),
            tip(
                super::water_detail::NOTE,
                Impact::Low,
                pref(
                    super::water_detail::CAPTION,
                    Value::Menu(crate::options::names::fidelity::WATER_DETAIL),
                    Int(super::water_detail::DEFAULT),
                ),
            ),
            tip(
                super::landscape_smoothing::NOTE,
                Impact::Low,
                pref(
                    super::landscape_smoothing::CAPTION,
                    Value::Menu(crate::options::names::fidelity::LANDSCAPE_SMOOTHING),
                    Int(super::landscape_smoothing::DEFAULT),
                ),
            ),
            tip(
                super::ground_coloring::NOTE,
                Impact::Low,
                pref(
                    super::ground_coloring::CAPTION,
                    Value::Menu(crate::options::names::GROUND_COLORING),
                    Int(super::ground_coloring::DEFAULT),
                ),
            ),
            fidelity_box(10, Impact::Medium),
            fidelity_box(7, Impact::Low),
            fidelity_box(9, Impact::Low),
            tip(
                super::portal_style::NOTE,
                Impact::Low,
                pref(
                    super::portal_style::CAPTION,
                    Value::Menu(crate::options::names::PORTAL_STYLE),
                    Int(super::portal_style::DEFAULT),
                ),
            ),
        ],
    },
    Heading {
        title: INFORMATION_HEADING,
        text: Text::literal(INFORMATION_HEADING),
        rows: &[tip(
            "Shows the frame rate and frame times in a small panel on screen.",
            Impact::Information,
            pref(
                "Performance Panel",
                Value::Check(crate::options::performance::PERFORMANCE_PANEL),
                Bool(false),
            ),
        )],
    },
    CAMERA,
];

/// The row of the `i`th box.
const fn fidelity_box(i: usize, impact: Impact) -> Row {
    let o = &super::fidelity::OPTIONS[i];
    tip(
        o.note,
        impact,
        pref(o.caption, Value::Check(o.name), Bool(o.on_by_default)),
    )
}

/// The four pages, in tab order.
pub const PAGES: [Page; 4] = [
    Page {
        id: PageId::GameSupport,
        title: "Game / Support",
        headings: &GAME_SUPPORT,
    },
    Page {
        id: PageId::Character,
        title: "Character Options",
        headings: &CHARACTER,
    },
    Page {
        id: PageId::Chat,
        title: "Chat Options",
        headings: &CHAT,
    },
    Page {
        id: PageId::Client,
        title: "Client Options",
        headings: &CLIENT,
    },
];

/// One page by its id.
#[must_use]
pub fn page(id: PageId) -> &'static Page {
    match id {
        PageId::GameSupport => &PAGES[0],
        PageId::Character => &PAGES[1],
        PageId::Chat => &PAGES[2],
        PageId::Client => &PAGES[3],
    }
}

/// The headings of `page`, each with its rows, in order; a row this host does not offer
/// ([`Row::offered`]) is left out, and so is a heading with no row left.
pub fn headings_for(id: PageId) -> impl Iterator<Item = (&'static Heading, Vec<&'static Row>)> {
    page(id).headings.iter().filter_map(move |h| {
        let rows: Vec<&'static Row> = h.rows.iter().filter(|r| r.offered()).collect();
        (!rows.is_empty()).then_some((h, rows))
    })
}

/// Every row of `page` this host offers, in order.
pub fn rows_for(id: PageId) -> impl Iterator<Item = &'static Row> {
    page(id)
        .headings
        .iter()
        .flat_map(|h| h.rows.iter())
        .filter(move |r| r.offered())
}

/// The row of `page` that edits `preference`, if there is one.
#[must_use]
pub fn row_of_preference(id: PageId, preference: &str) -> Option<&'static Row> {
    page(id)
        .headings
        .iter()
        .flat_map(|h| h.rows.iter())
        .find(|r| {
            r.preference() == Some(preference)
                || matches!(r.value, Value::Sound { volume, .. } if volume == preference)
        })
}

/// The row of the Character Options page that edits `option`.
#[must_use]
pub fn row_of_option(option: PlayerOption) -> Option<&'static Row> {
    page(PageId::Character)
        .headings
        .iter()
        .flat_map(|h| h.rows.iter())
        .find(|r| r.value == Value::Option(option))
}

/// The captions under a slider's two ends, left then right, for the sliders the modern page
/// labels; both interfaces draw the same words.
pub const SLIDER_ENDS: [(&str, &str, &str); 5] = [
    (crate::options::names::CAMERA_STIFFNESS, "Soft", "Hard"),
    (
        crate::options::names::CAMERA_ADJUSTMENT_SPEED,
        "Slow",
        "Fast",
    ),
    (crate::options::names::FIELD_OF_VIEW, "Narrow", "Wide"),
    (crate::options::names::SCREEN_BRIGHTNESS, "Dark", "Bright"),
    (crate::options::names::DEGRADE_DISTANCE, "Close", "Far"),
];

/// The two end captions of `preference`'s slider, if the modern page labels it.
#[must_use]
pub fn slider_ends(preference: &str) -> Option<(&'static str, &'static str)> {
    SLIDER_ENDS
        .iter()
        .find(|(p, _, _)| *p == preference)
        .map(|&(_, left, right)| (left, right))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_character_option_on_a_page_is_on_the_character_page_once() {
        let mut seen: Vec<PlayerOption> = rows_for(PageId::Character)
            .filter_map(|r| match r.value {
                Value::Option(o) => Some(o),
                _ => None,
            })
            .collect();
        seen.sort_unstable();
        seen.dedup();
        // Every option but the two set elsewhere: the radar's lock and the friends panel's
        // appear-offline box.
        let mut want: Vec<PlayerOption> = PlayerOption::ALL
            .into_iter()
            .filter(|o| !matches!(o, P::LockUI | P::AppearOffline))
            .collect();
        want.sort_unstable();
        assert_eq!(seen, want);
        let all: Vec<&Row> = page(PageId::Character)
            .headings
            .iter()
            .flat_map(|h| h.rows.iter())
            .collect();
        for o in want {
            assert_eq!(
                all.iter().filter(|r| r.value == Value::Option(o)).count(),
                1,
                "{o:?}"
            );
        }
    }

    #[test]
    fn the_four_pages_are_in_tab_order_and_each_has_rows() {
        let ids: Vec<PageId> = PAGES.iter().map(|p| p.id).collect();
        assert_eq!(
            ids,
            [
                PageId::GameSupport,
                PageId::Character,
                PageId::Chat,
                PageId::Client
            ]
        );
        for p in PAGES {
            assert!(rows_for(p.id).count() > 0, "{:?}", p.id);
        }
        assert_eq!(rows_for(PageId::Chat).count(), 2 + 2 + 12 + 13 * 4);
        // The chat font's face and size are the chat page's, under the windows' opacity.
        let chat: Vec<Option<&str>> = rows_for(PageId::Chat)
            .take(4)
            .map(Row::preference)
            .collect();
        assert_eq!(
            chat[2..],
            [Some("UI.ChatFontFace"), Some("UI.ChatFontSize")]
        );
        assert!(!rows_for(PageId::Client)
            .any(|r| r.preference().is_some_and(|p| p.starts_with("UI.ChatFont"))));
    }

    #[test]
    fn the_client_page_has_mouse_turning_and_invert_and_no_vertical_sync() {
        let has = |p: &str| rows_for(PageId::Client).any(|r| r.preference() == Some(p));
        assert!(has("Input.InvertMouseLookYAxis"));
        assert!(has("Input.UseMouseTurning"));
        // No row for vertical sync: it does nothing in a borderless window.
        assert!(!has("Display.SyncToRefresh"));
    }

    #[test]
    fn a_row_the_era_lacks_is_marked_and_met_by_an_era_that_has_it() {
        let f = pyreal_primitives::EraFeatures::INFILTRATION;
        let cloak = row_of_option(P::ShowCloak).unwrap();
        assert!(!cloak.needs.met(Some(&f)));
        assert!(cloak
            .needs
            .met(Some(&pyreal_primitives::EraFeatures::END_OF_RETAIL)));
        assert!(cloak.needs.met(None));
        assert!(row_of_option(P::DisableHouseRestrictionEffects)
            .unwrap()
            .needs
            .met(Some(&f)));
    }

    /// An option for a system the world lacks is left out, each by its own system; a world whose
    /// era is not known has every row.
    #[test]
    fn an_option_row_follows_the_system_it_belongs_to() {
        use pyreal_primitives::EraFeatures as F;
        let without = |f: fn(&mut F)| {
            let mut features = F::ALL;
            f(&mut features);
            features
        };
        for (option, off) in [
            (P::SalvageMultiple, without(|f| f.tinkering = false)),
            (P::DisplayChessRank, without(|f| f.chess = false)),
            (
                P::AcceptLootPermits,
                without(|f| f.corpse_permissions = false),
            ),
            (
                P::FellowshipShareXP,
                without(|f| f.fellowship_options = false),
            ),
            (P::IgnoreTradeRequests, without(|f| f.trade = false)),
            (
                P::FellowshipShareLoot,
                without(|f| f.fellowship_share_loot = false),
            ),
            (
                P::FellowshipAutoAcceptRequests,
                without(|f| f.fellowship_auto_accept = false),
            ),
            (
                P::HearAllegianceChat,
                without(|f| f.allegiance_chat = false),
            ),
            (P::HearGeneralChat, without(|f| f.global_chat_rooms = false)),
            (P::HearSocietyChat, without(|f| f.global_chat_rooms = false)),
            (
                P::DragItemOnPlayerOpensSecureTrade,
                without(|f| f.trade = false),
            ),
        ] {
            let needs = row_of_option(option).unwrap().needs;
            assert!(needs.met(Some(&F::ALL)), "{option:?}");
            assert!(!needs.met(Some(&off)), "{option:?}");
            assert!(needs.met(None), "{option:?}");
        }
    }

    /// The Display tab's five topics follow the sound, each effect box and menu under one of them
    /// once, at its default, and every display row has a tooltip with its cost.
    #[test]
    fn the_display_topics_hold_every_effect_once_at_its_default_each_with_a_tooltip() {
        use crate::options::fidelity;
        let titles: Vec<&str> = page(PageId::Client)
            .headings
            .iter()
            .map(|h| h.title)
            .collect();
        assert_eq!(
            titles,
            [
                "Sound",
                SCREEN_HEADING,
                LIGHTING_HEADING,
                SKY_HEADING,
                SURFACES_HEADING,
                INFORMATION_HEADING,
                "Camera and Mouse"
            ]
        );
        let display: Vec<&Row> = page(PageId::Client).headings[1..6]
            .iter()
            .flat_map(|h| h.rows.iter())
            .collect();
        for o in &fidelity::OPTIONS {
            let rows: Vec<&&Row> = display
                .iter()
                .filter(|r| r.value == Value::Check(o.name))
                .collect();
            assert_eq!(rows.len(), 1, "{}", o.name);
            assert_eq!(rows[0].caption, o.caption);
            assert_eq!(rows[0].default, Some(Bool(o.on_by_default)), "{}", o.name);
        }
        for (name, _) in fidelity::MENUS {
            let n = display
                .iter()
                .filter(|r| r.value == Value::Menu(name))
                .count();
            assert_eq!(n, 1, "{name}");
        }
        let default_of = |name: &'static str| {
            display
                .iter()
                .find(|r| r.value == Value::Menu(name))
                .and_then(|r| r.default)
        };
        use crate::options::names::fidelity as n;
        assert_eq!(
            default_of(n::RENDER_SCALE),
            Some(Int(crate::options::render_scale::DYNAMIC as i32))
        );
        assert_eq!(
            default_of(n::GOD_RAYS),
            Some(Int(crate::options::god_rays::DEFAULT))
        );
        assert_eq!(
            default_of(n::DEPTH_MAPPING),
            Some(Int(crate::options::depth_mapping::DEFAULT))
        );
        assert_eq!(
            default_of(n::GROUND_COVER_DRAW_DISTANCE),
            Some(Int(crate::options::draw_distance::GROUND_COVER_DEFAULT))
        );
        assert_eq!(
            default_of(n::LIGHT_DRAW_DISTANCE),
            Some(Int(crate::options::light_distance::DEFAULT))
        );
        assert_eq!(
            default_of(n::SHADOW_DISTANCE),
            Some(Int(crate::options::shadow_distance::DEFAULT))
        );
        assert_eq!(
            default_of(n::CLOUD_DETAIL),
            Some(Int(crate::options::cloud_detail::DEFAULT))
        );
        assert_eq!(
            default_of(n::WATER_DETAIL),
            Some(Int(crate::options::water_detail::DEFAULT))
        );
        for r in &display {
            assert!(r.note.is_some() && r.impact.is_some(), "{}", r.caption);
        }
        // The window's own render scale and the retail degrade distance have no row.
        for gone in [
            crate::options::names::DISPLAY_RENDER_SCALE,
            crate::options::names::DEGRADE_DISTANCE,
        ] {
            assert!(row_of_preference(PageId::Client, gone).is_none(), "{gone}");
        }
    }

    #[test]
    fn every_profile_row_has_a_default_and_a_registered_preference() {
        crate::options::store::init();
        for r in rows_for(PageId::Client) {
            let p = r.preference().unwrap();
            assert!(r.default.is_some(), "{p}");
            assert!(crate::options::store::is_registered(p), "{p}");
        }
    }
}
