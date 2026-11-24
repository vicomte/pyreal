//! Floating combat numbers: the damage the player deals rises over what they hit, and the damage
//! they take and the healing they get rise over their own head, read from the game's own combat
//! lines in chat. Critical hits are larger and gold, evades and resists are words, and the rest
//! of the damage the player deals takes its damage type's colour.
//!
//! Each number stands on the point over its body that a name stands on, so it follows the body,
//! fades with distance as names do and is hidden behind the world. Numbers over one body stack
//! upward, newest lowest, and never overlap. Settings switch them off, or each of outgoing damage,
//! incoming damage and healing.
//!
//! The game sends no damage line for the blow that kills, only a kill line, so a creature the
//! player kills shows "Killed" instead. Periodic ticks on one body that come together show as
//! one smaller number that adds them up.

use pyreal_primitives::ObjectId;

use crate::art::Family;
use crate::draw::{with_alpha, Anchor};
use crate::ui::game::GameState;
use crate::ui::narrow;
use crate::ui::paint::{Painter, TextStyle};

/// How long a number shows, in seconds; a critical one a little longer.
pub const LIFE: f64 = 1.5;
pub const CRITICAL_LIFE: f64 = 1.9;

/// How many numbers one body shows at once: a newer one pushes the oldest out.
pub const PER_BODY: usize = 6;

/// How long after a periodic tick, in seconds, another on the same body adds to its number.
pub const TICK_MERGE: f64 = 0.5;

/// How long, in seconds, a body the player struck is remembered for its kill line.
pub const FOUGHT_SECONDS: f64 = 60.0;

/// How many bodies the player struck are remembered for their kill lines.
const FOUGHT_KEPT: usize = 8;

/// How far a number rises over its life, in layout units.
const RISE: f32 = 56.0;

/// How far over the point a name stands on a number starts, in layout units: over the player's
/// head, over a name, and over the selection's name and marker.
const CLEARANCE: [f32; 3] = [10.0, 28.0, 62.0];

/// The faintest a number over the selection fades with distance.
const SELECTION_FADE: f32 = 0.4;

/// Which switch a number answers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Damage the player deals, and their attacks evaded or spells resisted.
    Outgoing,
    /// Damage the player takes, and attacks they evade or spells they resist.
    Incoming,
    /// Health restored, to the player or by them.
    Healing,
}

/// What a number shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    Amount(u32),
    Evaded,
    Resisted,
    /// The player's blow killed it.
    Killed,
}

/// The damage's type, for the colour of the damage the player deals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Element {
    Physical,
    Fire,
    Cold,
    Acid,
    Electric,
    Nether,
    /// Health drained by life magic.
    Health,
}

/// Who a number stands over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Over {
    Player,
    /// Someone or something by the name the line gives.
    Named(String),
}

/// One combat line read as a number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub over: Over,
    pub side: Side,
    pub shown: Shown,
    pub element: Element,
    pub critical: bool,
    /// A periodic tick of damage or healing over time.
    pub periodic: bool,
}

/// One combat line read: its number, and who dealt or gave it when the line names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    pub hit: Hit,
    pub by: Option<Over>,
}

/// Which numbers show: all of them off, or each side on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switches {
    pub on: bool,
    pub outgoing: bool,
    pub incoming: bool,
    pub healing: bool,
}

impl Default for Switches {
    fn default() -> Self {
        Self {
            on: true,
            outgoing: true,
            incoming: true,
            healing: true,
        }
    }
}

/// The settings file's keys for the switches, in [`Switches::get`]'s order.
pub const KEYS: [&str; 4] = [
    "combat-numbers",
    "combat-numbers-outgoing",
    "combat-numbers-incoming",
    "combat-numbers-healing",
];

/// The settings page's rows for the switches, in [`KEYS`]' order: each caption and what it does.
pub const ROWS: [(&str, &str); 4] = [
    (
        "Combat Numbers",
        "Shows damage and healing as numbers that rise over the one who took them.",
    ),
    (
        "Damage You Deal",
        "Shows the damage you deal over what you hit, and when it evades or resists.",
    ),
    (
        "Damage You Take",
        "Shows the damage you take over your character, and when you evade or resist.",
    ),
    (
        "Healing",
        "Shows the health restored to you, or by you to another.",
    ),
];

impl Switches {
    /// Whether numbers on `side` show.
    #[must_use]
    pub const fn shows(self, side: Side) -> bool {
        self.on
            && match side {
                Side::Outgoing => self.outgoing,
                Side::Incoming => self.incoming,
                Side::Healing => self.healing,
            }
    }

    /// The switch the settings file's `key` names.
    #[must_use]
    pub fn get(self, key: &str) -> Option<bool> {
        let at = KEYS.iter().position(|k| *k == key)?;
        Some([self.on, self.outgoing, self.incoming, self.healing][at])
    }

    /// Set the switch `key` names from the settings file's `value`; whether `key` is one.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        let on = value.trim() == "true";
        match KEYS.iter().position(|k| *k == key) {
            Some(0) => self.on = on,
            Some(1) => self.outgoing = on,
            Some(2) => self.incoming = on,
            Some(3) => self.healing = on,
            _ => return false,
        }
        true
    }

    /// The settings file's lines.
    #[must_use]
    pub fn to_text(self) -> String {
        KEYS.iter()
            .map(|k| format!("{k}={}\n", self.get(k).unwrap_or(true)))
            .collect()
    }
}

/// This frame's lines to read for combat: the game's combat notification lines, which come
/// whether or not combat text is squelched, then the chat lines that are not their echo.
#[must_use]
pub fn frame_lines(state: &GameState) -> Vec<&crate::ui::game::ChatLine> {
    let mut echoes: Vec<&str> = state
        .combat_lines
        .iter()
        .map(|l| without_timestamp(l.text.trim()))
        .collect();
    let chat = state.new_chat.iter().filter(|l| {
        let text = without_timestamp(l.text.trim());
        match echoes.iter().position(|e| *e == text) {
            Some(i) => {
                echoes.swap_remove(i);
                false
            }
            None => true,
        }
    });
    let mut lines: Vec<_> = state.combat_lines.iter().collect();
    lines.extend(chat);
    lines
}

/// The chat types combat lines come in: the server's broadcast, combat and magic lines, and the
/// client's own two combat types. Speech, tells and emotes are never read.
const READ_TYPES: [u32; 5] = [
    pyreal_client_model::chat::text_type::DEFAULT,
    pyreal_client_model::chat::text_type::COMBAT,
    pyreal_client_model::chat::text_type::MAGIC,
    pyreal_client_model::chat::text_type::COMBAT_ENEMY,
    pyreal_client_model::chat::text_type::COMBAT_SELF,
];

/// The words that open a line before its sentence. The first marks a critical hit.
const OPENERS: [&str; 5] = [
    "Critical hit!",
    "Overpower!",
    "Sneak Attack!",
    "Recklessness!",
    "Reckless!",
];

/// Read a chat line of `chat_type` as a number, if it is a combat line that carries one.
#[must_use]
pub fn parse(chat_type: u8, text: &str) -> Option<Hit> {
    read(chat_type, text).map(|r| r.hit)
}

/// Read a chat line of `chat_type` as [`parse`] does, with who dealt or gave the number.
#[must_use]
pub fn read(chat_type: u8, text: &str) -> Option<Read> {
    if !READ_TYPES.contains(&u32::from(chat_type)) {
        return None;
    }
    let mut s = without_timestamp(text.trim());
    let mut critical = false;
    while let Some((i, rest)) = OPENERS
        .iter()
        .enumerate()
        .find_map(|(i, o)| Some((i, s.strip_prefix(o)?)))
    {
        critical |= i == 0;
        s = rest.trim_start();
    }
    let (mut hit, by) = sentence(s)?;
    hit.critical |= critical;
    let by = by.map(|name| match name {
        "yourself" | "you" => Over::Player,
        name => Over::Named(name.to_owned()),
    });
    Some(Read { hit, by })
}

/// The line without the timestamp the game puts in front ("14:02:31 ").
pub(crate) fn without_timestamp(text: &str) -> &str {
    match text.split_once(' ') {
        Some((stamp, rest))
            if stamp.contains(':') && stamp.chars().all(|c| c.is_ascii_digit() || c == ':') =>
        {
            rest
        }
        _ => text,
    }
}

/// A combat sentence, its openers taken off, and the name of who dealt or gave it.
fn sentence(s: &str) -> Option<(Hit, Option<&str>)> {
    let player = |side, shown, element, by| {
        Some((
            Hit {
                over: Over::Player,
                side,
                shown,
                element,
                critical: false,
                periodic: false,
            },
            by,
        ))
    };
    let named = |name: &str, side, shown, element| {
        let over = if name == "yourself" {
            Over::Player
        } else {
            Over::Named(name.to_owned())
        };
        Some((
            Hit {
                over,
                side,
                shown,
                element,
                critical: false,
                periodic: false,
            },
            Some("you"),
        ))
    };
    // Periodic damage and healing on the player.
    if let Some(rest) = s.strip_prefix("You receive ") {
        let (n, rest) = amount(rest)?;
        let rest = rest.strip_prefix(" points of periodic ")?;
        let mut read = if rest.starts_with("healing") {
            player(Side::Healing, Shown::Amount(n), Element::Health, None)
        } else {
            let element = if rest.starts_with("nether") {
                Element::Nether
            } else {
                Element::Health
            };
            player(Side::Incoming, Shown::Amount(n), element, None)
        }?;
        read.0.periodic = true;
        return Some(read);
    }
    // Life magic cast on the player's own health.
    if let Some(rest) = s.strip_prefix("You cast ") {
        if let Some((_, rest)) = rest.split_once(" and restore ") {
            let n = health_points(rest, " points of your health")?;
            return player(
                Side::Healing,
                Shown::Amount(n),
                Element::Health,
                Some("you"),
            );
        }
        let (_, rest) = rest.split_once(" and drain ")?;
        let n = health_points(rest, " points of your health")?;
        return player(
            Side::Incoming,
            Shown::Amount(n),
            Element::Health,
            Some("you"),
        );
    }
    // Life magic cast on another's health.
    if let Some(rest) = s.strip_prefix("With ") {
        if let Some((_, rest)) = rest.split_once(" you restore ") {
            let (n, rest) = amount(rest)?;
            let name = rest.strip_prefix(" points of health to ")?;
            return named(
                trim_end(name),
                Side::Healing,
                Shown::Amount(n),
                Element::Health,
            );
        }
        let (_, rest) = rest.split_once(" you drain ")?;
        let (n, rest) = amount(rest)?;
        let name = rest.strip_prefix(" points of health from ")?;
        return named(
            trim_end(name),
            Side::Outgoing,
            Shown::Amount(n),
            Element::Health,
        );
    }
    // A life transfer: "You lose 22 points of health due to X casting Y on you". Only health is
    // read; stamina and mana are not.
    if let Some((change, rest)) = s
        .strip_prefix("You lose ")
        .map(|r| (Side::Incoming, r))
        .or_else(|| s.strip_prefix("You gain ").map(|r| (Side::Healing, r)))
    {
        let (n, rest) = amount(rest)?;
        let rest = rest.strip_prefix(" points of health due to ")?;
        let by = match rest.strip_prefix("casting ") {
            Some(_) => Some("you"),
            None => rest.split_once(" casting ").map(|(by, _)| by),
        };
        return player(change, Shown::Amount(n), Element::Health, by);
    }
    if let Some(name) = s.strip_prefix("You evaded ") {
        let by = trim_end(name);
        return player(Side::Incoming, Shown::Evaded, Element::Physical, Some(by));
    }
    if let Some(name) = s.strip_prefix("You resist the spell cast by ") {
        let by = trim_end(name);
        return player(Side::Incoming, Shown::Resisted, Element::Physical, Some(by));
    }
    // A healing kit used by the player.
    if let Some(rest) = s.strip_prefix("You ") {
        let (critical, rest) = match rest.strip_prefix("expertly ") {
            Some(rest) => (true, rest),
            None => (false, rest),
        };
        if let Some(rest) = rest.strip_prefix("heal ") {
            let (name, rest) = before_amount(rest)?;
            let n = health_points(rest, " Health points")?;
            let mut read = named(name, Side::Healing, Shown::Amount(n), Element::Health)?;
            read.0.critical = critical;
            return Some(read);
        }
        // A blow or a spell struck by the player: "You slash Drudge for 12 points ...".
        let (verb, rest) = rest.split_once(' ')?;
        let element = verb_element(verb, false)?;
        let (name, rest) = before_amount(rest)?;
        let (n, rest) = amount(rest)?;
        let element = typed_element(rest).unwrap_or(element);
        let mut read = named(name, Side::Outgoing, Shown::Amount(n), element)?;
        read.0.periodic = rest.starts_with(" points of periodic ");
        return Some(read);
    }
    if let Some(name) = s.strip_suffix(" evaded your attack.") {
        return named(name, Side::Outgoing, Shown::Evaded, Element::Physical);
    }
    if let Some(name) = s.strip_suffix(" resists your spell") {
        return named(name, Side::Outgoing, Shown::Resisted, Element::Physical);
    }
    // A healing kit used on the player by another.
    if let Some((by, rest)) = s.split_once(" heals you for ") {
        let n = health_points(rest, " Health points")?;
        return player(Side::Healing, Shown::Amount(n), Element::Health, Some(by));
    }
    // Life magic cast on the player by another.
    if let Some((by, rest)) = s.split_once(" casts ") {
        if let Some((_, rest)) = rest.split_once(" and restores ") {
            let n = health_points(rest, " points of your health")?;
            return player(Side::Healing, Shown::Amount(n), Element::Health, Some(by));
        }
        if let Some((_, rest)) = rest.split_once(" and drains ") {
            let n = health_points(rest, " points of your health")?;
            return player(Side::Incoming, Shown::Amount(n), Element::Health, Some(by));
        }
    }
    // A potion or food: "The Health Elixir restores 50 points of your Health."
    if let Some(rest) = s.strip_prefix("The ") {
        let (by, rest) = rest.split_once(" restores ")?;
        let n = health_points(rest, " points of your Health")?;
        return player(Side::Healing, Shown::Amount(n), Element::Health, Some(by));
    }
    // A blow or a spell struck at the player: "Drudge slashes your arm for 12 points ...".
    struck_at_player(s)
}

/// "{name} {verbs} you(r part) for N point(s) ...": damage the player takes, and the name.
fn struck_at_player(s: &str) -> Option<(Hit, Option<&str>)> {
    let mut words = s.match_indices(' ');
    let (by, (n, rest)) = words.find_map(|(i, _)| {
        let rest = &s[i + 1..];
        let (verb, after) = rest.split_once(' ')?;
        verb_element(verb, true)?;
        if !(after.starts_with("you ") || after.starts_with("your ")) {
            return None;
        }
        let (_, after) = before_amount(after)?;
        Some((&s[..i], amount(after)?))
    })?;
    let hit = Hit {
        over: Over::Player,
        side: Side::Incoming,
        shown: Shown::Amount(n),
        element: typed_element(rest).unwrap_or(Element::Physical),
        critical: false,
        periodic: false,
    };
    Some((hit, Some(by)))
}

/// What comes before " for N", and the text from the number on.
fn before_amount(s: &str) -> Option<(&str, &str)> {
    s.match_indices(" for ").find_map(|(i, m)| {
        let rest = &s[i + m.len()..];
        rest.starts_with(|c: char| c.is_ascii_digit())
            .then(|| (&s[..i], rest))
    })
}

/// The number the text starts with, and the text after it.
fn amount(s: &str) -> Option<(u32, &str)> {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    Some((s[..end].parse().ok()?, &s[end..]))
}

/// The number the text starts with, when `tail` follows it: only health is shown.
fn health_points(s: &str, tail: &str) -> Option<u32> {
    let (n, rest) = amount(s)?;
    rest.starts_with(tail).then_some(n)
}

/// A name with the line's closing stop taken off.
fn trim_end(name: &str) -> &str {
    name.trim_end_matches(['.', '!'])
}

/// The damage type a hit verb names (the player's "slash", or another's "slashes" when
/// `plural`), as the game's own table words it.
fn verb_element(verb: &str, plural: bool) -> Option<Element> {
    use pyreal_client_model::combat::{combat_hit_adjectives, damage_type as dt};
    const TYPES: [(u32, Element); 9] = [
        (dt::SLASH, Element::Physical),
        (dt::PIERCE, Element::Physical),
        (dt::BLUDGEON, Element::Physical),
        (dt::COLD, Element::Cold),
        (dt::FIRE, Element::Fire),
        (dt::ACID, Element::Acid),
        (dt::ELECTRIC, Element::Electric),
        (dt::HEALTH, Element::Health),
        (dt::NETHER, Element::Nether),
    ];
    // The game's verbs for hits from the weakest to the strongest, and "hit" for a mixed type.
    if verb == if plural { "hits" } else { "hit" } {
        return Some(Element::Physical);
    }
    if !plural && verb == "bleed" {
        return Some(Element::Health);
    }
    TYPES.iter().find_map(|(t, element)| {
        [0.05, 0.2, 0.4, 0.9]
            .iter()
            .any(|p| {
                let (one, many) = combat_hit_adjectives(*t, *p);
                verb == if plural { many } else { one }
            })
            .then_some(*element)
    })
}

/// The damage type a melee line names after the number: "... points of fire damage!".
fn typed_element(rest: &str) -> Option<Element> {
    let kind = rest
        .strip_prefix(" points of ")
        .or_else(|| rest.strip_prefix(" point of "))?;
    let word = kind.split(['/', ' ']).next()?;
    Some(match word {
        "slashing" | "piercing" | "bludgeoning" => Element::Physical,
        "cold" => Element::Cold,
        "fire" => Element::Fire,
        "acid" => Element::Acid,
        "electrical" => Element::Electric,
        "nether" => Element::Nether,
        "health" => Element::Health,
        _ => return None,
    })
}

/// The lines the game sends the player whose blow killed `{0}`, with the blow's damage type and
/// whether it was critical, as far as each line tells.
pub const KILL_LINES: [(&str, Option<Element>, bool); 37] = [
    ("You split {0} apart!", Some(Element::Physical), false),
    ("You cleave {0} in twain!", Some(Element::Physical), false),
    (
        "{0} is torn to ribbons by your assault!",
        Some(Element::Physical),
        false,
    ),
    (
        "Your killing blow nearly turns {0} inside-out!",
        Some(Element::Physical),
        false,
    ),
    ("You run {0} through!", Some(Element::Physical), false),
    ("{0} is fatally punctured!", Some(Element::Physical), false),
    (
        "{0}'s perforated corpse falls before you!",
        Some(Element::Physical),
        false,
    ),
    (
        "{0}'s death is preceded by a sharp, stabbing pain!",
        Some(Element::Physical),
        false,
    ),
    (
        "You beat {0} to a lifeless pulp!",
        Some(Element::Physical),
        false,
    ),
    (
        "{0} is shattered by your assault!",
        Some(Element::Physical),
        false,
    ),
    (
        "You flatten {0}'s body with the force of your assault!",
        Some(Element::Physical),
        false,
    ),
    (
        "The thunder of crushing {0} is followed by the deafening silence of death!",
        Some(Element::Physical),
        false,
    ),
    ("You bring {0} to a fiery end!", Some(Element::Fire), false),
    ("{0} is reduced to cinders!", Some(Element::Fire), false),
    (
        "{0} is incinerated by your assault!",
        Some(Element::Fire),
        false,
    ),
    (
        "{0}'s seared corpse smolders before you!",
        Some(Element::Fire),
        false,
    ),
    ("Your attack stops {0} cold!", Some(Element::Cold), false),
    (
        "Your assault sends {0} to an icy death!",
        Some(Element::Cold),
        false,
    ),
    ("{0} suffers a frozen fate!", Some(Element::Cold), false),
    (
        "{0} is liquified by your attack!",
        Some(Element::Acid),
        false,
    ),
    (
        "{0}'s last strength dissolves before you!",
        Some(Element::Acid),
        false,
    ),
    (
        "You reduce {0} to a sizzling, oozing mass!",
        Some(Element::Acid),
        false,
    ),
    (
        "Blistered by lightning, {0} falls!",
        Some(Element::Electric),
        false,
    ),
    (
        "Electricity tears {0} apart!",
        Some(Element::Electric),
        false,
    ),
    (
        "Your lightning coruscates over {0}'s mortal remains!",
        Some(Element::Electric),
        false,
    ),
    (
        "{0} is dessicated by your attack!",
        Some(Element::Nether),
        false,
    ),
    (
        "{0}'s last strength withers before you!",
        Some(Element::Nether),
        false,
    ),
    (
        "You reduce {0} to a drained, twisted corpse!",
        Some(Element::Nether),
        false,
    ),
    ("You obliterate {0}!", None, true),
    ("You smite {0} mightily!", None, true),
    ("You knock {0} into next Morningthaw!", None, true),
    ("{0} is utterly destroyed by your attack!", None, true),
    (
        "{0} catches your attack, with dire consequences!",
        None,
        true,
    ),
    (
        "You slay {0} viciously enough to impart death several times over!",
        None,
        true,
    ),
    (
        "The deadly force of your attack is so strong that {0}'s ancestors feel it!",
        None,
        true,
    ),
    (
        "You send {0} to death so violently that even the lifestone flinches!",
        None,
        true,
    ),
    ("You killed {0}!", None, false),
];

/// A kill the game told the player of: who, and the blow's type and whether it was critical, as
/// far as its line tells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kill {
    pub name: String,
    pub element: Option<Element>,
    pub critical: bool,
}

/// Read a chat line of `chat_type` as the game telling the player their blow killed someone.
#[must_use]
pub fn killed(chat_type: u8, text: &str) -> Option<Kill> {
    use pyreal_client_model::chat::text_type;
    let ty = u32::from(chat_type);
    if ty != text_type::DEFAULT && ty != text_type::COMBAT {
        return None;
    }
    let text = without_timestamp(text.trim());
    KILL_LINES.iter().find_map(|(template, element, critical)| {
        let (before, after) = template.split_once("{0}")?;
        let name = text.strip_prefix(before)?.strip_suffix(after)?;
        (!name.is_empty()).then(|| Kill {
            name: name.to_owned(),
            element: *element,
            critical: *critical,
        })
    })
}

/// A number in the air: what it shows, over whom, and when it came.
#[derive(Debug, Clone, PartialEq)]
pub struct Number {
    pub id: ObjectId,
    pub hit: Hit,
    pub born: f64,
}

impl Number {
    /// How long it shows, in seconds.
    #[must_use]
    pub fn life(&self) -> f64 {
        if self.hit.critical || self.hit.shown == Shown::Killed {
            CRITICAL_LIFE
        } else {
            LIFE
        }
    }
}

/// A body the player struck lately, for the kill line that names it.
#[derive(Debug, Clone)]
struct Fought {
    id: ObjectId,
    name: String,
    element: Element,
    at: f64,
}

/// The numbers in the air, and the switches.
#[derive(Debug, Default)]
pub struct CombatNumbers {
    pub switches: Switches,
    /// Oldest first.
    pub numbers: Vec<Number>,
    /// The bodies the player struck lately, oldest first.
    fought: Vec<Fought>,
}

impl CombatNumbers {
    /// Read this frame's chat lines into numbers over who they name, and let the old ones go.
    pub fn take(&mut self, state: &GameState, now: f64) {
        self.numbers.retain(|n| now - n.born < n.life());
        self.fought.retain(|f| now - f.at < FOUGHT_SECONDS);
        if !self.switches.on || !state.in_world {
            self.numbers.clear();
            self.fought.clear();
            return;
        }
        for line in frame_lines(state) {
            let (hit, id) = match parse(line.chat_type, &line.text) {
                Some(hit) => {
                    if !self.switches.shows(hit.side) {
                        continue;
                    }
                    let Some(id) = resolve(&hit.over, state) else {
                        continue;
                    };
                    self.struck(id, &hit, now);
                    (hit, id)
                }
                None => {
                    let Some(kill) = killed(line.chat_type, &line.text) else {
                        continue;
                    };
                    if !self.switches.shows(Side::Outgoing) {
                        continue;
                    }
                    let Some((hit, id)) = self.kill_mark(kill, state) else {
                        continue;
                    };
                    (hit, id)
                }
            };
            self.push(Number { id, hit, born: now });
        }
    }

    /// Remember the body the player struck for its kill line.
    fn struck(&mut self, id: ObjectId, hit: &Hit, now: f64) {
        let Over::Named(name) = &hit.over else {
            return;
        };
        if hit.side != Side::Outgoing {
            return;
        }
        self.fought.retain(|f| f.id != id);
        if self.fought.len() >= FOUGHT_KEPT {
            self.fought.remove(0);
        }
        self.fought.push(Fought {
            id,
            name: name.clone(),
            element: hit.element,
            at: now,
        });
    }

    /// The "Killed" mark for `kill` and the body it stands over: the last struck of that name,
    /// else as [`resolve`] finds it. The player's own death is the death recap's.
    fn kill_mark(&mut self, kill: Kill, state: &GameState) -> Option<(Hit, ObjectId)> {
        let fought = self.fought.iter().rposition(|f| f.name == kill.name);
        let over = Over::Named(kill.name);
        let (id, element) = match fought {
            Some(i) => {
                let f = self.fought.remove(i);
                (f.id, kill.element.unwrap_or(f.element))
            }
            None => (
                resolve(&over, state)?,
                kill.element.unwrap_or(Element::Physical),
            ),
        };
        if Some(id) == state.player_id {
            return None;
        }
        let hit = Hit {
            over,
            side: Side::Outgoing,
            shown: Shown::Killed,
            element,
            critical: kill.critical,
            periodic: false,
        };
        Some((hit, id))
    }

    /// Add `n`, pushing out the oldest over the same body past [`PER_BODY`]. A periodic tick
    /// within [`TICK_MERGE`] of another of its kind on the same body adds to that one's number.
    pub fn push(&mut self, n: Number) {
        if let (true, Shown::Amount(more)) = (n.hit.periodic, n.hit.shown) {
            let merges = self.numbers.iter_mut().rev().find(|o| {
                o.id == n.id
                    && o.hit.periodic
                    && o.hit.side == n.hit.side
                    && o.hit.element == n.hit.element
                    && n.born - o.born < TICK_MERGE
            });
            if let Some(Number {
                hit:
                    Hit {
                        shown: Shown::Amount(sum),
                        ..
                    },
                ..
            }) = merges
            {
                *sum = sum.saturating_add(more);
                return;
            }
        }
        let over = self.numbers.iter().filter(|o| o.id == n.id).count();
        if over >= PER_BODY {
            if let Some(i) = self.numbers.iter().position(|o| o.id == n.id) {
                self.numbers.remove(i);
            }
        }
        self.numbers.push(n);
    }

    /// Draw the numbers over their bodies. Each stands where the body stood last frame, and the
    /// draw list moves it to where the body is drawn this frame.
    pub fn draw(&self, p: &mut Painter<'_>, state: &GameState, now: f64) {
        if self.numbers.is_empty() || state.teleporting {
            return;
        }
        let k = p.scale;
        let player = state.player_id;
        let mut ids: Vec<ObjectId> = self.numbers.iter().map(|n| n.id).collect();
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            // Over the player, or over a body whose name shows, at its fade.
            let (at, fade, clear) = if Some(id) == player {
                ((p.screen.0 / 2.0, p.screen.1 * 0.45), 1.0, CLEARANCE[0])
            } else {
                let Some(plate) = state.nameplates.iter().find(|n| n.id == id) else {
                    continue;
                };
                if plate.selected {
                    let fade = plate.nearness.max(SELECTION_FADE);
                    ((plate.x, plate.y), fade, CLEARANCE[2])
                } else {
                    ((plate.x, plate.y), plate.nearness, CLEARANCE[1])
                }
            };
            if fade <= 0.05 {
                continue;
            }
            // Newest first, each pushed over the one below it.
            let mut floor = 0.0_f32;
            for n in self.numbers.iter().rev().filter(|n| n.id == id) {
                let age = now - n.born;
                let style = style(n, age, fade);
                let text = words(&n.hit);
                let h = p.line_height(&style);
                let rise = narrow((age / n.life()).clamp(0.0, 1.0));
                let lift = (RISE * k * (1.0 - (1.0 - rise) * (1.0 - rise))).max(floor);
                floor = lift + h;
                let w = p.measure(&style, &text);
                let (x, y) = (at.0 - w / 2.0, at.1 - clear * k - h - lift);
                let from = p.list.mark();
                // A thick edge, so the number stands out from the world behind it.
                if let Some(edge) = style.edge {
                    let thick = TextStyle::new(style.family, style.points, edge);
                    for (dx, dy) in THICK_EDGE {
                        p.text(&thick, (x + dx * k).round(), (y + dy * k).round(), &text);
                    }
                }
                p.text(&style, x.round(), y.round(), &text);
                p.list.anchor_in_world(id, Anchor::Top, from, at, (x, y));
            }
        }
    }
}

/// The body a hit stands over: the player; the selection by its name; else the nearest of that
/// name whose name shows, or on the radar.
#[must_use]
pub fn resolve(over: &Over, state: &GameState) -> Option<ObjectId> {
    let name = match over {
        Over::Player => return state.player_id,
        Over::Named(name) => name,
    };
    if *name == state.name {
        return state.player_id;
    }
    if let Some(t) = state.target.as_ref().filter(|t| t.name == *name) {
        return Some(t.id);
    }
    let plate = state
        .nameplates
        .iter()
        .filter(|n| n.name == *name)
        .min_by(|a, b| {
            let d = |n: &crate::ui::game::Nameplate| n.distance.unwrap_or(f32::MAX);
            d(a).total_cmp(&d(b))
        });
    if let Some(plate) = plate {
        return Some(plate.id);
    }
    state
        .blips
        .iter()
        .filter(|b| b.name == *name)
        .min_by(|a, b| (a.dx * a.dx + a.dy * a.dy).total_cmp(&(b.dx * b.dx + b.dy * b.dy)))
        .map(|b| b.id)
}

/// The words a number shows: a critical one ends in "!", healing starts with "+".
#[must_use]
pub fn words(hit: &Hit) -> String {
    let bang = if hit.critical { "!" } else { "" };
    match (hit.shown, hit.side) {
        (Shown::Evaded, _) => "Evaded".to_owned(),
        (Shown::Resisted, _) => "Resisted".to_owned(),
        (Shown::Killed, _) => format!("Killed{bang}"),
        (Shown::Amount(n), Side::Healing) => format!("+{n}{bang}"),
        (Shown::Amount(n), _) => format!("{n}{bang}"),
    }
}

/// A number's colour: damage dealt by its type and gold when critical, damage taken red, healing
/// green, and evades and resists pale.
#[must_use]
pub const fn colour(hit: &Hit) -> u32 {
    match (hit.shown, hit.side) {
        (Shown::Evaded | Shown::Resisted, _) => 0xFFE6_E6E6,
        (_, Side::Healing) => 0xFF4C_E65A,
        (_, Side::Incoming) => 0xFFFF_3B30,
        (_, Side::Outgoing) if hit.critical => 0xFFFF_D21E,
        (_, Side::Outgoing) => match hit.element {
            Element::Physical => 0xFFFF_FFFF,
            Element::Fire => 0xFFFF_8A1E,
            Element::Cold => 0xFF5C_CCFF,
            Element::Acid => 0xFF9C_E62E,
            Element::Electric => 0xFFB0_9CFF,
            Element::Nether => 0xFFC2_6CFF,
            Element::Health => 0xFFFF_6E96,
        },
    }
}

/// Where the edge colour is drawn again for a thick edge, in layout units: the diagonals and
/// under.
const THICK_EDGE: [(f32, f32); 5] = [
    (-2.0, -2.0),
    (2.0, -2.0),
    (-2.0, 2.0),
    (2.0, 2.0),
    (0.0, 3.0),
];

/// How a number of `age` seconds is drawn at `fade`: its size (larger for a critical or a kill,
/// popping as it comes, and smaller for a periodic tick), its colour and edge, faded in and out.
fn style(n: &Number, age: f64, fade: f32) -> TextStyle {
    let killed = n.hit.shown == Shown::Killed;
    let words = killed || matches!(n.hit.shown, Shown::Evaded | Shown::Resisted);
    let pop = 1.0 + 0.3 * narrow((1.0 - age / 0.18).max(0.0));
    let mut points = match (words, n.hit.periodic) {
        (true, _) => 21.0,
        (false, true) => 22.0,
        (false, false) => 28.0,
    };
    if killed {
        points = if n.hit.critical { 35.0 } else { 30.0 } * pop;
    } else if n.hit.critical {
        points = 35.0 * pop;
    }
    let points = points * (0.7 + 0.3 * fade);
    let a = narrow((age / 0.08).min((n.life() - age) / 0.45).clamp(0.0, 1.0)) * fade;
    let family = if words {
        Family::Body
    } else {
        Family::BarNumbers
    };
    TextStyle::new(family, points, with_alpha(colour(&n.hit), a)).edge(with_alpha(0xFF00_0000, a))
}
