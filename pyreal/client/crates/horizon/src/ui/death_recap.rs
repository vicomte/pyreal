//! The death recap: what killed the player and the last hits they took, on the Journal's Deaths
//! tab, which opens on a death. The last few deaths are kept, the newest first.
//!
//! The client keeps a rolling record of the damage, drains and healing the player took, read from
//! the combat lines in chat as the floating combat numbers read them: the last fifteen seconds,
//! and no more than twenty entries. It also keeps the player's health over those seconds. A death
//! is the character's death count going up, as the corpse marker reads it, or the game's own
//! line telling the player they died, which also names the killer. Without that line the killer
//! is the one who dealt the last hit.
//!
//! The killer is the one the game's death report names by object id, else the one its death line
//! names, else the last dealer; a report with no killer, or the player, means none (a fall,
//! `@die`). The items lost, from the line that comes a few seconds after the death, and the vitae
//! penalty, from its enchantment, are shown too.
//!
//! The game sends no hit line for the blow that kills, only its death line. Unless the last hit
//! seen took all the health the player had, the killing blow is added from the health they had
//! left, with the killer and damage type the death line gives.

use std::collections::VecDeque;

use crate::ui::combat_numbers::{self, Element, Over, Shown, Side};
use crate::ui::game::GameState;

/// How long the record reaches back, in seconds.
pub const KEEP_SECONDS: f64 = 15.0;

/// The most entries the record keeps.
pub const KEEP_ENTRIES: usize = 20;

/// How often the health is sampled, in seconds.
pub const HEALTH_STEP: f64 = 0.25;

/// How long after a death, in seconds, its other signal (the death count or the game's line) and
/// its last hits may still arrive and count towards it.
pub const LATE_SECONDS: f64 = 3.0;

/// How long after a death, in seconds, the game's line about the items it cost still counts.
pub const LOSS_SECONDS: f64 = 30.0;

/// The most deaths kept, the newest among them.
pub const KEEP_DEATHS: usize = 10;

/// The settings file's key for opening the recap on a death.
pub const OPENS_KEY: &str = "death-recap-opens";

/// The lines the game sends the player who died, with the killing blow's damage type and whether
/// it was critical, as far as each line tells: `{1}` is the killer, `{0}` the player.
pub const DEATH_LINES: [(&str, Option<Element>, bool); 38] = [
    ("{1} splits you apart!", Some(Element::Physical), false),
    ("{1} cleaves you in twain!", Some(Element::Physical), false),
    (
        "You are torn to ribbons by {1}'s assault!",
        Some(Element::Physical),
        false,
    ),
    (
        "{1}'s killing blow nearly turns you inside-out!",
        Some(Element::Physical),
        false,
    ),
    ("{1} runs you through!", Some(Element::Physical), false),
    (
        "You are fatally punctured by {1}!",
        Some(Element::Physical),
        false,
    ),
    (
        "Your perforated corpse falls before {1}!",
        Some(Element::Physical),
        false,
    ),
    (
        "Your death is preceded by a sharp, stabbing pain, courtesy of {1}!",
        Some(Element::Physical),
        false,
    ),
    (
        "{1} beats you to a lifeless pulp!",
        Some(Element::Physical),
        false,
    ),
    (
        "Your body is shattered by {1}'s attack!",
        Some(Element::Physical),
        false,
    ),
    (
        "The force of {1}'s assault flattens you!",
        Some(Element::Physical),
        false,
    ),
    (
        "The thunder of {1} crushing {0} is followed by the deafening silence of your death!",
        Some(Element::Physical),
        false,
    ),
    ("{1} brings you to a fiery end!", Some(Element::Fire), false),
    (
        "You are reduced to cinders by {1}!",
        Some(Element::Fire),
        false,
    ),
    (
        "You are incinerated by {1}'s assault!",
        Some(Element::Fire),
        false,
    ),
    (
        "Your seared corpse smolders before {1}!",
        Some(Element::Fire),
        false,
    ),
    ("{1}'s attack stops you cold!", Some(Element::Cold), false),
    (
        "{1}'s assault sends you to an icy death!",
        Some(Element::Cold),
        false,
    ),
    (
        "You suffer a frozen fate at the hands of {1}!",
        Some(Element::Cold),
        false,
    ),
    (
        "You are liquified by {1}'s attack!",
        Some(Element::Acid),
        false,
    ),
    (
        "Your last strength dissolves before {1}!",
        Some(Element::Acid),
        false,
    ),
    (
        "{1} reduces you to a sizzling, oozing mass!",
        Some(Element::Acid),
        false,
    ),
    (
        "Blistered by {1}'s lightning, you die!",
        Some(Element::Electric),
        false,
    ),
    (
        "Electricity from {1}'s attack tears you apart!",
        Some(Element::Electric),
        false,
    ),
    (
        "{1}'s lightning coruscates over your mortal remains!",
        Some(Element::Electric),
        false,
    ),
    (
        "You are dessicated by {1}'s attack!",
        Some(Element::Nether),
        false,
    ),
    (
        "Your last strength withers before {1}!",
        Some(Element::Nether),
        false,
    ),
    (
        "{1} reduces you to a drained, twisted corpse!",
        Some(Element::Nether),
        false,
    ),
    ("{1} obliterates you!", None, true),
    ("{1} smites you mightily!", None, true),
    ("{1} knocks you into next Morningthaw!", None, true),
    ("You are utterly destroyed by {1}'s attack!", None, true),
    (
        "You catch {1}'s attack, with dire consequences!",
        None,
        true,
    ),
    (
        "{1} slays you viciously enough to impart death several times over!",
        None,
        true,
    ),
    (
        "The deadly force of {1}'s attack is so strong that your ancestors feel it!",
        None,
        true,
    ),
    (
        "{1} sends you to your death so violently that even the lifestone flinches!",
        None,
        true,
    ),
    ("You were killed by {1}!", None, false),
    ("You died!", None, false),
];

/// What the game's death line tells of a death.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Death {
    pub killer: Option<String>,
    pub element: Option<Element>,
    pub critical: bool,
    /// The line, without its timestamp.
    pub line: String,
}

/// What an entry of the record did to the player's health.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Damage,
    /// Health drained by life magic.
    Drain,
    Healing,
    /// An attack the player evaded.
    Evaded,
    /// A spell the player resisted.
    Resisted,
}

impl Kind {
    /// Whether it took health.
    #[must_use]
    pub const fn hurts(self) -> bool {
        matches!(self, Self::Damage | Self::Drain)
    }
}

/// One hit the player took, or healing they got.
#[derive(Debug, Clone, PartialEq)]
pub struct Taken {
    /// When it came, in the interface's seconds.
    pub at: f64,
    /// Who dealt or gave it, when the line names them.
    pub by: Option<Over>,
    pub kind: Kind,
    pub amount: u32,
    pub element: Element,
    pub critical: bool,
    /// Whether this was the blow that killed, and whether it was seen or worked out.
    pub killing: Option<Killing>,
}

/// How the killing blow is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Killing {
    /// Its hit line came and took all the health left.
    Seen,
    /// No hit line came: its amount is the health left, and its type is the death line's.
    FromHealth,
}

impl Taken {
    /// The words for who dealt or gave it.
    #[must_use]
    pub fn source(&self) -> &str {
        match &self.by {
            Some(Over::Named(name)) => name,
            Some(Over::Player) => "You",
            None => "Unknown",
        }
    }

    /// The words for what it was: the damage's type, a drain or healing, and a critical hit.
    #[must_use]
    pub fn type_words(&self) -> String {
        if self.killing == Some(Killing::FromHealth) && self.element == Element::Health {
            return if self.critical {
                "Killing blow, critical".to_owned()
            } else {
                "Killing blow".to_owned()
            };
        }
        let kind = match (self.kind, self.element) {
            (Kind::Evaded | Kind::Resisted, _) => return String::new(),
            (Kind::Healing, _) => "Healing",
            (Kind::Drain, _) => "Drain",
            (Kind::Damage, Element::Physical) => "Physical",
            (Kind::Damage, Element::Fire) => "Fire",
            (Kind::Damage, Element::Cold) => "Cold",
            (Kind::Damage, Element::Acid) => "Acid",
            (Kind::Damage, Element::Electric) => "Lightning",
            (Kind::Damage, Element::Nether) => "Nether",
            (Kind::Damage, Element::Health) => "Health",
        };
        let mut words = kind.to_owned();
        if self.critical {
            words.push_str(", critical");
        }
        if self.killing.is_some() {
            words.push_str(", killing blow");
        }
        words
    }

    /// The amount as the tab shows it: healing with a plus, the rest with a minus.
    #[must_use]
    pub fn amount_words(&self) -> String {
        match self.kind {
            Kind::Healing => format!("+{}", self.amount),
            Kind::Evaded => "Evaded".to_owned(),
            Kind::Resisted => "Resisted".to_owned(),
            Kind::Damage | Kind::Drain => format!("-{}", self.amount),
        }
    }
}

/// One death, as the Deaths tab shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Recap {
    /// When the death came, in the interface's seconds.
    pub at: f64,
    /// The killer the game's death line names.
    pub named: Option<String>,
    /// The game's death line, when it came.
    pub line: Option<String>,
    /// The hits taken before the death, oldest first.
    pub taken: Vec<Taken>,
    /// The health before the death as a share of the most, oldest first, [`HEALTH_STEP`] apart
    /// and the last at the death.
    pub health: Vec<f32>,
    /// Whether the player died in a dungeon.
    pub in_dungeon: bool,
    /// The killer the game's death report gives by object id: `Some(None)` for none.
    pub reported: Option<Option<String>>,
    /// What the death cost in items, once the game has said.
    pub loss: Option<Loss>,
}

/// What a death cost in items, as the game says a few seconds after it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loss {
    /// The items lost, as the game words each; none when all were kept.
    pub items: Vec<String>,
    /// How many fewer items Clutch of the Miser let the death take.
    pub miser: Option<u32>,
    /// The game said every item was kept.
    pub kept: bool,
}

impl Loss {
    /// The line the tab shows.
    #[must_use]
    pub fn words(&self) -> String {
        let mut words = if self.items.is_empty() {
            "You kept all your items.".to_owned()
        } else {
            format!("Lost {}.", self.items.join(", "))
        };
        if let Some(n) = self.miser {
            words.push_str(&format!(" Clutch of the Miser saved up to {n}."));
        }
        words
    }
}

/// What a death cost, from one of the game's lines after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LossLine {
    /// "You've lost ...!": the items, as the game words each.
    Lost(Vec<String>),
    /// All kept: "You have retained all your items", or a PK Lite death's return to the light.
    Kept,
    /// Clutch of the Miser took this many items off the count.
    Miser(u32),
}

/// One of the game's lines about what a death cost.
#[must_use]
pub fn loss_line(text: &str) -> Option<LossLine> {
    let text = combat_numbers::without_timestamp(text.trim());
    if let Some(list) = text.strip_prefix("You've lost ") {
        let list = list.trim_end_matches('!');
        let items = list
            .split(", ")
            .map(|i| {
                let i = i.strip_prefix("and ").unwrap_or(i);
                i.strip_prefix("your ").unwrap_or(i).to_owned()
            })
            .filter(|i| !i.is_empty())
            .collect();
        return Some(LossLine::Lost(items));
    }
    if text.starts_with("You have retained all your items.")
        || text.starts_with("You are enveloped in a feeling of warmth")
    {
        return Some(LossLine::Kept);
    }
    let n = text
        .strip_prefix("Your augmentation has reduced the number of items you can lose by ")?
        .trim_end_matches('!');
    Some(LossLine::Miser(n.parse().ok()?))
}

impl Recap {
    /// Who killed the player: the one the game's line names, else the one who dealt the last
    /// damage or drain.
    #[must_use]
    pub fn killer(&self) -> Option<&str> {
        match &self.reported {
            Some(Some(name)) => return Some(name),
            Some(None) => return None,
            None => {}
        }
        if let Some(name) = &self.named {
            return Some(name);
        }
        self.taken
            .iter()
            .rev()
            .find(|t| t.kind.hurts() && t.by.is_some())
            .map(Taken::source)
    }

    /// The tab's heading.
    #[must_use]
    pub fn heading(&self) -> String {
        match self.killer() {
            Some("You") => "You killed yourself".to_owned(),
            Some(killer) => format!("Killed by {killer}"),
            None => "You died".to_owned(),
        }
    }

    /// The line under the heading: the game's own death line when it names the killer, else where
    /// the killer's name came from.
    #[must_use]
    pub fn detail(&self) -> String {
        if let (Some(line), Some(_)) = (&self.line, self.killer()) {
            if self.named.is_some() || matches!(self.reported, Some(Some(_))) {
                return line.clone();
            }
        }
        if self.reported == Some(None) {
            "The game names no killer.".to_owned()
        } else if self.killer().is_some() {
            "Named from the last hit you took.".to_owned()
        } else if self
            .taken
            .iter()
            .all(|t| t.killing == Some(Killing::FromHealth))
        {
            "No hits were seen before your death.".to_owned()
        } else {
            "The hits before your death did not name who dealt them.".to_owned()
        }
    }

    /// The killing blow worked out from the health left, if it was.
    pub fn inferred_mut(&mut self) -> Option<&mut Taken> {
        self.taken
            .iter_mut()
            .find(|t| t.killing == Some(Killing::FromHealth))
    }

    /// Take the game's death report: the killer by object id, which also deals the worked-out
    /// blow. A killer the client does not know (`None`) leaves it to the death line.
    pub fn learn_report(&mut self, killer: Option<Option<String>>) {
        let Some(killer) = killer else {
            return;
        };
        if let Some(blow) = self.inferred_mut() {
            blow.by = killer.clone().map(Over::Named);
        }
        self.reported = Some(killer);
    }

    /// Take a line about what the death cost.
    pub fn learn_loss(&mut self, line: LossLine) {
        let loss = self.loss.get_or_insert_with(Loss::default);
        match line {
            LossLine::Lost(items) => {
                loss.items = items;
                loss.kept = false;
            }
            LossLine::Kept => {
                loss.items.clear();
                loss.kept = true;
            }
            LossLine::Miser(n) => loss.miser = Some(n),
        }
    }

    /// Take what the game's death line tells: the killer, and the worked-out blow's dealer, type
    /// and critical.
    pub fn learn(&mut self, death: Death) {
        let reported = self.reported.is_some();
        if let Some(blow) = self.inferred_mut() {
            if let (Some(killer), false) = (&death.killer, reported) {
                blow.by = Some(Over::Named(killer.clone()));
            }
            if let Some(element) = death.element {
                blow.element = element;
            }
            blow.critical |= death.critical;
        }
        self.named = self.named.take().or(death.killer);
        self.line = self.line.take().or(Some(death.line));
    }

    /// The damage and drains taken, all told.
    #[must_use]
    pub fn damage_total(&self) -> u32 {
        self.taken
            .iter()
            .filter(|t| t.kind.hurts())
            .map(|t| t.amount)
            .sum()
    }
}

/// The rolling record, the recaps of the last deaths, and the death count it watches.
#[derive(Debug, Default)]
pub struct DeathRecap {
    /// The hits taken, oldest first.
    pub record: VecDeque<Taken>,
    /// The health samples, `(when, share of the most)`, oldest first.
    health: VecDeque<(f64, f32)>,
    /// The death count last seen, after the first.
    deaths: Option<i32>,
    /// The last health above nothing seen, in points.
    alive_health: Option<u32>,
    /// The last death's recap, kept until leaving the world.
    pub last: Option<Recap>,
    /// The deaths before it, the newest first, at most [`KEEP_DEATHS`] in all with it.
    pub earlier: VecDeque<Recap>,
}

impl DeathRecap {
    /// Follow a frame at `now`: keep this frame's hits and health, and make a recap on a death.
    /// Whether a new death came this frame.
    pub fn follow(&mut self, state: &GameState, now: f64) -> bool {
        if !state.in_world || state.explore.is_some() {
            self.out_of_world();
            return false;
        }
        if state.health.max > 0
            && self
                .health
                .back()
                .is_none_or(|(t, _)| now - t >= HEALTH_STEP)
        {
            self.health.push_back((now, state.health.ratio()));
        }
        let mut named: Option<Death> = None;
        let before = self.alive_health;
        for line in combat_numbers::frame_lines(state) {
            if let Some(death) = death_line(line.chat_type, &line.text, &state.name) {
                named = Some(death);
                continue;
            }
            if let Some(mut t) = taken(line.chat_type, &line.text, now) {
                // A hit that takes all the health left is the killing blow.
                if t.kind.hurts() && before.is_some_and(|h| t.amount >= h) {
                    t.killing = Some(Killing::Seen);
                }
                self.take(t);
            }
        }
        if state.health.current > 0 {
            self.alive_health = Some(state.health.current);
        }
        self.trim(now);
        let counted = state
            .deaths
            .is_some_and(|n| self.deaths.is_some_and(|m| n > m));
        if state.deaths.is_some() {
            self.deaths = state.deaths;
        }
        self.follow_loss(state, now);
        let reported = state.own_deaths.last().cloned();
        if !(counted || named.is_some() || reported.is_some()) {
            return false;
        }
        // Another signal of a death already recapped.
        if let Some(r) = self.last.as_mut().filter(|r| now - r.at <= LATE_SECONDS) {
            if let Some(killer) = reported {
                r.learn_report(killer);
            }
            if let Some(death) = named {
                r.learn(death);
            }
            return false;
        }
        // The health at the death closes the samples.
        if state.health.max > 0 {
            if self.health.back().is_some_and(|(t, _)| *t == now) {
                self.health.pop_back();
            }
            self.health.push_back((now, state.health.ratio()));
        }
        let mut taken: Vec<Taken> = self.record.iter().cloned().collect();
        let seen = taken
            .iter()
            .rev()
            .find(|t| t.kind.hurts())
            .is_some_and(|t| t.killing.is_some());
        if !seen {
            if let Some(left) = before.or(self.alive_health) {
                if taken.len() >= KEEP_ENTRIES {
                    taken.remove(0);
                }
                taken.push(Taken {
                    at: now,
                    by: None,
                    kind: Kind::Damage,
                    amount: left,
                    // Health stands for a type the death line does not tell.
                    element: Element::Health,
                    critical: false,
                    killing: Some(Killing::FromHealth),
                });
            }
        }
        let mut recap = Recap {
            at: now,
            named: None,
            line: None,
            taken,
            health: self.health.iter().map(|(_, h)| *h).collect(),
            in_dungeon: state.in_dungeon,
            reported: None,
            loss: None,
        };
        // Without a death line, the worked-out blow is the last dealer's.
        let dealer = recap
            .taken
            .iter()
            .rev()
            .find(|t| t.kind.hurts() && t.killing.is_none() && t.by.is_some())
            .and_then(|t| t.by.clone());
        if let Some(blow) = recap.inferred_mut() {
            blow.by = dealer;
        }
        if let Some(killer) = reported {
            recap.learn_report(killer);
        }
        if let Some(death) = named {
            recap.learn(death);
        }
        if let Some(before) = self.last.replace(recap) {
            self.earlier.push_front(before);
            self.earlier.truncate(KEEP_DEATHS - 1);
        }
        self.record.clear();
        self.alive_health = None;
        true
    }

    /// The game's lines about what the last death cost, which come a few seconds after it.
    fn follow_loss(&mut self, state: &GameState, now: f64) {
        let Some(r) = self.last.as_mut().filter(|r| now - r.at <= LOSS_SECONDS) else {
            return;
        };
        for line in &state.new_chat {
            if let Some(loss) = loss_line(&line.text) {
                r.learn_loss(loss);
            }
        }
    }

    /// Keep `t`, and a hit that arrives just after a death in that death's recap.
    fn take(&mut self, t: Taken) {
        if let Some(r) = self
            .last
            .as_mut()
            .filter(|r| t.at - r.at <= LATE_SECONDS && t.kind.hurts())
        {
            // The killing blow's own line, come late, replaces the one worked out.
            let mut t = t;
            if let Some(i) = r
                .taken
                .iter()
                .position(|b| b.killing == Some(Killing::FromHealth))
            {
                r.taken.remove(i);
                t.killing = Some(Killing::Seen);
            }
            if r.taken.len() >= KEEP_ENTRIES {
                r.taken.remove(0);
            }
            r.taken.push(t);
            return;
        }
        self.record.push_back(t);
    }

    /// Let go of what is older than [`KEEP_SECONDS`], and of the oldest past [`KEEP_ENTRIES`].
    fn trim(&mut self, now: f64) {
        while self
            .record
            .front()
            .is_some_and(|t| now - t.at > KEEP_SECONDS)
            || self.record.len() > KEEP_ENTRIES
        {
            self.record.pop_front();
        }
        while self
            .health
            .front()
            .is_some_and(|(t, _)| now - t > KEEP_SECONDS)
        {
            self.health.pop_front();
        }
    }

    /// The deaths kept, the newest first.
    pub fn deaths(&self) -> impl Iterator<Item = &Recap> {
        self.last.iter().chain(&self.earlier)
    }

    /// Out of the world: the record, the recaps and the death count are let go.
    pub fn out_of_world(&mut self) {
        *self = Self::default();
    }
}

/// A combat line of `chat_type` read as a hit the player took or healing they got at `now`.
#[must_use]
pub fn taken(chat_type: u8, text: &str, now: f64) -> Option<Taken> {
    let read = combat_numbers::read(chat_type, text)?;
    let hit = read.hit;
    if hit.over != Over::Player {
        return None;
    }
    let amount = match hit.shown {
        Shown::Amount(n) => n,
        Shown::Evaded | Shown::Resisted | Shown::Killed => 0,
    };
    let kind = match (hit.shown, hit.side, hit.element) {
        (_, Side::Outgoing, _) => return None,
        (Shown::Evaded, ..) => Kind::Evaded,
        (Shown::Resisted, ..) => Kind::Resisted,
        (_, Side::Healing, _) => Kind::Healing,
        (_, Side::Incoming, Element::Health) => Kind::Drain,
        (_, Side::Incoming, _) => Kind::Damage,
    };
    Some(Taken {
        at: now,
        by: read.by,
        kind,
        amount,
        element: hit.element,
        critical: hit.critical,
        killing: None,
    })
}

/// Whether a chat line of `chat_type` is the game telling the player called `name` they died, and
/// what it tells of the death.
#[must_use]
pub fn death_line(chat_type: u8, text: &str, name: &str) -> Option<Death> {
    use pyreal_client_model::chat::text_type;
    let ty = u32::from(chat_type);
    if ty != text_type::DEFAULT && ty != text_type::COMBAT {
        return None;
    }
    let text = combat_numbers::without_timestamp(text.trim());
    DEATH_LINES
        .iter()
        .find_map(|(template, element, critical)| {
            let template = template.replace("{0}", name);
            let killer = match template.split_once("{1}") {
                None if text == template => None,
                None => return None,
                Some((before, after)) => {
                    let killer = text.strip_prefix(before)?.strip_suffix(after)?;
                    if killer.is_empty() {
                        return None;
                    }
                    Some(killer.to_owned())
                }
            };
            Some(Death {
                killer,
                element: *element,
                critical: *critical,
                line: text.to_owned(),
            })
        })
}

/// The seconds before the death `at` that `t` came, as the tab shows them.
#[must_use]
pub fn before_words(at: f64, t: &Taken) -> String {
    format!("{:.1}s", (at - t.at).max(0.0))
}
