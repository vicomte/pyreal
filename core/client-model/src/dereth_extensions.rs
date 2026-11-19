//! Dereth's own messages, which only this client and an Empyrean world read
//! (`pyreal_protocol::extensions`): the AI Players in the player's company and their modes, and an
//! Advocate's offers of a waypoint or a journal note. The client names the extensions it knows
//! before entering the world and uses those the world answers with; a retail or ACE world answers
//! nothing and none is used.
//!
//! A mode is set with the world's `@aifellow` command and an order is a tell to the AI Player,
//! which the world carries out at once. An offer is the world's Yes/No question, worded as
//! [`parse_offer`] reads it; on a yes the world sends the waypoint or the note.

use pyreal_client_contract::journal::JournalPage;
use pyreal_primitives::ObjectId;
pub use pyreal_protocol::extensions::{
    AiFellow, JournalEntry, Waypoint, ADVOCATE_OFFERS, AI_FELLOWS, KNOWN,
};

/// How an AI fellow fights.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FellowMode {
    Aggressive,
    Defensive,
    Assist,
    Heal,
    Passive,
}

impl FellowMode {
    /// Every mode, in the order the fellowship window shows them.
    pub const ALL: [Self; 5] = [
        Self::Aggressive,
        Self::Defensive,
        Self::Assist,
        Self::Heal,
        Self::Passive,
    ];

    /// The mode's word on the wire and in `@aifellow`.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Aggressive => "aggressive",
            Self::Defensive => "defensive",
            Self::Assist => "assist",
            Self::Heal => "heal",
            Self::Passive => "passive",
        }
    }

    /// The mode's name in the interface.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Aggressive => "Aggressive",
            Self::Defensive => "Defensive",
            Self::Assist => "Assist",
            Self::Heal => "Heal",
            Self::Passive => "Passive",
        }
    }

    /// What the mode does, for a tooltip.
    #[must_use]
    pub const fn says(self) -> &'static str {
        match self {
            Self::Aggressive => "Fights anything near you.",
            Self::Defensive => "Fights what fights the fellowship.",
            Self::Assist => "Fights only what you are fighting.",
            Self::Heal => {
                "Keeps the fellowship healed and buffed, and fights only what attacks it."
            }
            Self::Passive => "Fights nothing unless told.",
        }
    }

    /// The mode a world's word names.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|m| m.word().eq_ignore_ascii_case(word.trim()))
    }

    /// The `@aifellow` line that sets `fellow`'s mode to this one. The fellow is named by its id,
    /// which no two share.
    #[must_use]
    pub fn command(self, fellow: ObjectId) -> String {
        format!("@aifellow {:#010x} {}", fellow.0, self.word())
    }
}

/// An order an AI fellow carries out at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FellowOrder {
    Attack,
    Follow,
    Stay,
    Come,
}

impl FellowOrder {
    /// Every order, in the order the fellowship window shows them.
    pub const ALL: [Self; 4] = [Self::Attack, Self::Follow, Self::Stay, Self::Come];

    /// The word told to the AI Player.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Attack => "attack",
            Self::Follow => "follow",
            Self::Stay => "stay",
            Self::Come => "come",
        }
    }

    /// The order's name in the interface.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Attack => "Attack",
            Self::Follow => "Follow",
            Self::Stay => "Stay",
            Self::Come => "Come",
        }
    }

    /// What the order does, for a tooltip.
    #[must_use]
    pub const fn says(self) -> &'static str {
        match self {
            Self::Attack => "Attacks what you are fighting.",
            Self::Follow => "Follows you again.",
            Self::Stay => "Holds where it stands for two minutes.",
            Self::Come => "Comes to you now.",
        }
    }
}

/// What an Advocate offers.
#[derive(Debug, Clone, PartialEq)]
pub enum Offered {
    /// A waypoint at `ns`, `ew` (north and east positive), labelled `label`.
    Waypoint { label: String, ns: f64, ew: f64 },
    /// A note for the journal, titled `title`. Its text comes once it is accepted.
    JournalNote { title: String },
    /// `count` journal pages with the way to `to`. They come once accepted.
    JournalPages { count: u32, to: String },
}

/// An Advocate's offer, read from the world's Yes/No question.
#[derive(Debug, Clone, PartialEq)]
pub struct AdvocateOffer {
    /// Who offers it.
    pub from: String,
    pub offered: Offered,
}

/// The world's question's two forms, around the Advocate's name and what is offered.
const WAYPOINT_SAYS: &str = " offers you a waypoint: ";
const NOTE_SAYS: &str = " offers a note for your journal: ";
const ASKS: &str = ". Add it?";
const PAGES_SAYS: &str = " notebook pages with the way to ";
const ASKS_MANY: &str = ". Add them?";

/// The offer a Yes/No question's text makes, if it is an Advocate's: `{from} offers you a
/// waypoint: {label} ({place}). Add it?`, `{from} offers a note for your journal: "{title}".
/// Add it?` or `{from} offers {n} notebook pages with the way to {to}. Add them?`.
#[must_use]
pub fn parse_offer(text: &str) -> Option<AdvocateOffer> {
    if let Some(body) = text.trim().strip_suffix(ASKS_MANY) {
        let (head, to) = body.split_once(PAGES_SAYS)?;
        let (from, count) = head.rsplit_once(" offers ")?;
        let count: u32 = count.trim().parse().ok()?;
        return (!from.trim().is_empty() && !to.trim().is_empty()).then(|| AdvocateOffer {
            from: from.trim().to_owned(),
            offered: Offered::JournalPages {
                count,
                to: to.trim().to_owned(),
            },
        });
    }
    let body = text.trim().strip_suffix(ASKS)?;
    if let Some((from, rest)) = body.split_once(WAYPOINT_SAYS) {
        let (label, place) = rest.strip_suffix(')')?.rsplit_once(" (")?;
        let ((ns, ew), tail) = crate::cmd::waypoint::read_place(place)?;
        if !tail.trim().is_empty() || from.trim().is_empty() {
            return None;
        }
        return Some(AdvocateOffer {
            from: from.trim().to_owned(),
            offered: Offered::Waypoint {
                label: label.trim().to_owned(),
                ns,
                ew,
            },
        });
    }
    let (from, title) = body.split_once(NOTE_SAYS)?;
    let title = title.strip_prefix('"')?.strip_suffix('"')?;
    (!from.trim().is_empty()).then(|| AdvocateOffer {
        from: from.trim().to_owned(),
        offered: Offered::JournalNote {
            title: title.to_owned(),
        },
    })
}

/// The game's coordinates `(ns, ew)` of a spot in a landblock: the block is the top half of
/// `cell`, and `x`, `y` are metres in it.
#[must_use]
pub fn map_place(cell: u32, x: f32, y: f32) -> (f64, f64) {
    let unit = |block: u32, m: f32| (f64::from(block) * 192.0 + f64::from(m)) / 240.0 - 101.95;
    let (bx, by) = (cell >> 24, (cell >> 16) & 0xFF);
    (unit(by, y), unit(bx, x))
}

/// The `@way` that sets an accepted waypoint, named for its label.
#[must_use]
pub fn waypoint_command(w: &Waypoint) -> crate::cmd::waypoint::WaypointCommand {
    let (ns, ew) = map_place(w.cell, w.x, w.y);
    let label = w.label.trim();
    crate::cmd::waypoint::WaypointCommand::Set {
        ns,
        ew,
        name: (!label.is_empty()).then(|| {
            label
                .chars()
                .take(crate::cmd::waypoint::LONGEST_NAME)
                .collect()
        }),
    }
}

/// The journal page an accepted note becomes: its title and text, tagged Advocate with who gave
/// it, and its location stamped when it has one.
#[must_use]
pub fn journal_page(entry: &JournalEntry) -> JournalPage {
    let (ns, ew) = entry.location.unwrap_or_default();
    JournalPage {
        label: pyreal_client_contract::journal::ADVOCATE_TAG.to_owned(),
        from: entry.from_name.clone(),
        title: entry.title.clone(),
        notes: entry.text.clone(),
        ns: f64::from(ns),
        ew: f64::from(ew),
        location_set: entry.location.is_some(),
        ..JournalPage::default()
    }
}

/// The line said when an accepted note is in the journal.
#[must_use]
pub fn journal_line(entry: &JournalEntry) -> String {
    format!(
        "{}'s note {} is in your journal.",
        entry.from_name, entry.title
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_advocate_s_waypoint_question_reads_as_its_offer() {
        let offer = parse_offer(
            "Advocate Tamsin offers you a waypoint: Holtburg square (42.3N 33.3E). Add it?",
        )
        .expect("an offer");
        assert_eq!(offer.from, "Advocate Tamsin");
        assert_eq!(
            offer.offered,
            Offered::Waypoint {
                label: "Holtburg square".into(),
                ns: 42.3,
                ew: 33.3,
            }
        );
        // A label with its own brackets keeps them; the place is the last pair.
        let offer =
            parse_offer("~Tamsin offers you a waypoint: Arwic (old) (56.1N 70.5W). Add it?")
                .expect("an offer");
        assert_eq!(
            offer.offered,
            Offered::Waypoint {
                label: "Arwic (old)".into(),
                ns: 56.1,
                ew: -70.5,
            }
        );
    }

    #[test]
    fn an_advocate_s_note_question_reads_as_its_offer() {
        let offer =
            parse_offer("Advocate Tamsin offers a note for your journal: \"Drudges\". Add it?")
                .expect("an offer");
        assert_eq!(offer.from, "Advocate Tamsin");
        assert_eq!(
            offer.offered,
            Offered::JournalNote {
                title: "Drudges".into()
            }
        );
    }

    #[test]
    fn other_questions_are_no_offer() {
        for text in [
            "Are you sure you want to recall?",
            "Someone offers you a waypoint: nowhere. Add it?",
            "Someone offers you a waypoint: Holtburg (42.3N). Add it?",
            " offers a note for your journal: \"x\". Add it?",
            "Tamsin offers a note for your journal: Drudges. Add it?",
        ] {
            assert_eq!(parse_offer(text), None, "{text}");
        }
    }

    #[test]
    fn a_waypoint_s_spot_is_the_map_s_coordinates() {
        // The server's own test vector: Holtburg square, cell 0xA9B40031 at (84, 7.5).
        let (ns, ew) = map_place(0xA9B4_0031, 84.0, 7.5);
        assert!((ns - 42.1).abs() < 0.05, "{ns}");
        assert!((ew - 33.6).abs() < 0.05, "{ew}");
        let w = Waypoint {
            label: "  Holtburg square ".into(),
            cell: 0xA9B4_0031,
            x: 84.0,
            y: 7.5,
            ..Waypoint::default()
        };
        let crate::cmd::waypoint::WaypointCommand::Set { name, .. } = waypoint_command(&w) else {
            panic!("a set");
        };
        assert_eq!(name.as_deref(), Some("Holtburg square"));
    }

    #[test]
    fn a_note_with_a_location_is_a_page_with_its_place_stamped() {
        let note = JournalEntry {
            from_name: "Advocate Tamsin".into(),
            title: "To Fort Tethana (1/2)".into(),
            text: "Step to the Portal to Town Network.".into(),
            location: Some((42.3, -33.3)),
            ..JournalEntry::default()
        };
        let page = journal_page(&note);
        assert!(page.location_set);
        assert!((page.ns - 42.3).abs() < 1e-4 && (page.ew + 33.3).abs() < 1e-4);
        assert_eq!(page.label, "Advocate");
        assert_eq!(page.from, "Advocate Tamsin");
        let plain = journal_page(&JournalEntry {
            location: None,
            ..note
        });
        assert!(!plain.location_set);
        assert_eq!((plain.ns, plain.ew), (0.0, 0.0));
    }

    #[test]
    fn a_mode_is_set_by_the_fellow_s_id() {
        assert_eq!(
            FellowMode::Heal.command(ObjectId(0x5000_0002)),
            "@aifellow 0x50000002 heal"
        );
        for m in FellowMode::ALL {
            assert_eq!(FellowMode::from_word(m.word()), Some(m));
        }
        assert_eq!(FellowMode::from_word(" HEAL "), Some(FellowMode::Heal));
        assert_eq!(FellowMode::from_word("berserk"), None);
    }

    #[test]
    fn an_advocate_s_route_pages_question_reads_as_its_offer() {
        assert_eq!(
            parse_offer(
                "~Advocate Tamsin offers 2 notebook pages with the way to Fort Tethana. Add them?"
            ),
            Some(AdvocateOffer {
                from: "~Advocate Tamsin".into(),
                offered: Offered::JournalPages {
                    count: 2,
                    to: "Fort Tethana".into(),
                },
            })
        );
        assert_eq!(
            parse_offer("Tamsin offers some notebook pages with the way to X. Add them?"),
            None
        );
    }
}
