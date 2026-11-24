//! The appraisals the interface asks for on its own, as the pointer rests on an item: which were
//! asked for and when, what came back, the numbers worth comparing, and how two items' numbers
//! compare.
//!
//! The answers live in the game's appraisal cache; the runtime reads the ones the interface
//! watches into [`GameState::appraised`]. [`Appraisals`] keeps what was asked, so an item is asked
//! about once, again only when it changes, and says plainly when an assess failed or no reply
//! came. Anything that shows an item's details reads it through [`Appraisals::status`].

use std::collections::BTreeMap;
use std::hash::{Hash as _, Hasher as _};

use pyreal_client_contract::view::AppraisalView;
use pyreal_primitives::ObjectId;

use crate::ui::game::{Examined, GameState, Item};

/// How long the pointer rests on an item before its details are shown, in seconds.
pub const HOVER_DELAY: f64 = 0.35;

/// How long an appraisal is waited for before the item says no reply came, in seconds.
pub const NO_REPLY_AFTER: f64 = 5.0;

/// How long after a request that was not answered the item may be asked about again.
pub const ASK_AGAIN_AFTER: f64 = 15.0;

/// How old a worn item's answer may be while it is compared, in seconds: its spells run out.
pub const WORN_ANSWER_LASTS: f64 = 60.0;

/// How long after an inscription is sent the item is asked about again, in seconds.
pub const AFTER_INSCRIBING: f64 = 1.0;

/// The game's appraisal of one object, as the runtime reads it for the interface.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Appraised {
    pub id: ObjectId,
    /// Which reply this is (the view's `delivery`): a later reply has a larger number.
    pub delivery: u64,
    /// The assess succeeded. A failed one carries only part of the details.
    pub success: bool,
    /// The details, as the examine window lays them out.
    pub detail: Examined,
    /// The numbers worth comparing.
    pub stats: Vec<Stat>,
}

impl Appraised {
    /// From the game's appraisal of `id` and the details the examine window builds of it.
    #[must_use]
    pub fn of(id: ObjectId, view: &AppraisalView, detail: Examined) -> Self {
        Self {
            id,
            delivery: view.delivery,
            success: view.success,
            detail,
            stats: stats_of(view),
        }
    }
}

/// What is known of an object's appraisal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status<'a> {
    /// Not asked for, and nothing known.
    Unknown,
    /// Asked for, and the reply has not come.
    Waiting,
    /// The reply came and the assess succeeded.
    Ready(&'a Appraised),
    /// The reply came and the assess failed.
    Failed(&'a Appraised),
    /// No reply came.
    NoReply,
}

impl<'a> Status<'a> {
    fn of(a: &'a Appraised) -> Self {
        if a.success {
            Self::Ready(a)
        } else {
            Self::Failed(a)
        }
    }

    /// The appraisal, when a reply came.
    #[must_use]
    pub const fn appraised(self) -> Option<&'a Appraised> {
        match self {
            Self::Ready(a) | Self::Failed(a) => Some(a),
            _ => None,
        }
    }
}

/// One object's request.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Entry {
    /// The item as its tile showed it when asked: a change means the answer is out of date.
    look: u64,
    /// When it was asked for; `None` for an answer that was already there.
    asked: Option<f64>,
    /// The delivery the answer must be newer than.
    after: u64,
    /// A reply newer than `after` has been seen.
    answered: bool,
    /// The newest delivery seen, and when it was first seen.
    seen: u64,
    seen_at: f64,
    /// Asked again with an answer in hand: that answer is shown until the new one comes.
    refreshing: bool,
}

/// The appraisals the interface asked for, by object.
#[derive(Debug, Clone, Default)]
pub struct Appraisals {
    entries: BTreeMap<ObjectId, Entry>,
    /// Objects to ask about again, and when: an inscription was written on them.
    due: BTreeMap<ObjectId, f64>,
}

impl Appraisals {
    /// Whether to ask the game to appraise `item` now, given what is `known` of it: when nothing
    /// is known and it was never asked, when it has changed since, or when a request went
    /// unanswered for [`ASK_AGAIN_AFTER`]. An item with an answer is not asked again.
    pub fn want(&mut self, item: &Item, known: Option<&Appraised>, now: f64) -> bool {
        self.want_within(item, known, now, f64::INFINITY)
    }

    /// [`Self::want`], and also when the answer is `max_age` seconds old. The old answer is shown
    /// until the new one comes.
    pub fn want_within(
        &mut self,
        item: &Item,
        known: Option<&Appraised>,
        now: f64,
        max_age: f64,
    ) -> bool {
        let look = fingerprint(item);
        let Some(e) = self.entries.get_mut(&item.id) else {
            let entry = match known {
                Some(k) => Entry {
                    look,
                    asked: None,
                    after: k.delivery.saturating_sub(1),
                    answered: true,
                    seen: k.delivery,
                    seen_at: now,
                    refreshing: false,
                },
                None => Entry {
                    look,
                    asked: Some(now),
                    after: 0,
                    answered: false,
                    seen: 0,
                    seen_at: now,
                    refreshing: false,
                },
            };
            self.entries.insert(item.id, entry);
            return known.is_none();
        };
        if e.look != look {
            *e = Entry {
                look,
                asked: Some(now),
                after: known.map_or(0, |k| k.delivery),
                answered: false,
                seen: known.map_or(0, |k| k.delivery),
                seen_at: now,
                refreshing: false,
            };
            return true;
        }
        if let Some(k) = known.filter(|k| k.delivery > e.after) {
            e.answered = true;
            e.refreshing = false;
            if k.delivery > e.seen {
                e.seen = k.delivery;
                e.seen_at = now;
            }
            if now - e.seen_at < max_age {
                return false;
            }
            e.ask_again(now);
            return true;
        }
        // Answered before and asked again elsewhere (the examine window): that answer is awaited.
        if e.answered {
            return false;
        }
        match e.asked {
            Some(t) if now - t < ASK_AGAIN_AFTER => false,
            _ => {
                e.asked = Some(now);
                true
            }
        }
    }

    /// What is known of `id`'s appraisal at `now`. A reply older than the item's last change is
    /// not shown: it waits for the new one.
    #[must_use]
    pub fn status<'a>(&self, id: ObjectId, known: Option<&'a Appraised>, now: f64) -> Status<'a> {
        let Some(e) = self.entries.get(&id) else {
            return known.map_or(Status::Unknown, Status::of);
        };
        if let Some(k) = known.filter(|k| k.delivery > e.after || e.refreshing) {
            return Status::of(k);
        }
        match e.asked {
            _ if e.answered => Status::Waiting,
            Some(t) if now - t >= NO_REPLY_AFTER => Status::NoReply,
            Some(_) => Status::Waiting,
            None => Status::Unknown,
        }
    }

    /// [`Self::status`] of `id` from what the runtime read into `state`.
    #[must_use]
    pub fn lookup<'a>(&self, state: &'a GameState, id: ObjectId, now: f64) -> Status<'a> {
        self.status(id, known(state, id), now)
    }

    /// Forget `id`: the next look asks again.
    pub fn forget(&mut self, id: ObjectId) {
        self.entries.remove(&id);
    }

    /// An inscription was sent for `id` at `now`: it is asked about again
    /// [`AFTER_INSCRIBING`] later, once the server has it.
    pub fn inscribed(&mut self, id: ObjectId, now: f64) {
        self.due.insert(id, now + AFTER_INSCRIBING);
    }

    /// The objects to ask about again at `now`. What was known of them is shown until the new
    /// answer comes.
    pub fn due(&mut self, now: f64) -> Vec<ObjectId> {
        let ids: Vec<ObjectId> = self
            .due
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in &ids {
            self.due.remove(id);
            if let Some(e) = self.entries.get_mut(id) {
                e.ask_again(now);
            }
        }
        ids
    }
}

impl Entry {
    /// Asked again at `now`, with the newest answer seen shown until a newer one comes.
    fn ask_again(&mut self, now: f64) {
        self.after = self.seen;
        self.asked = Some(now);
        self.answered = false;
        self.refreshing = self.seen > 0;
    }
}

/// The appraisal of `id` the runtime read into `state`, if any.
#[must_use]
pub fn known(state: &GameState, id: ObjectId) -> Option<&Appraised> {
    state.appraised.iter().find(|a| a.id == id)
}

/// What of an item's tile says it changed: everything but its cooldown and whether the game is
/// busy with it.
fn fingerprint(item: &Item) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    item.name.hash(&mut h);
    (item.ac_icon, item.underlay, item.overlay, item.stack).hash(&mut h);
    (item.equip_locations, item.attuned, item.wcid).hash(&mut h);
    if let Some(d) = &item.decoration {
        let mut d = *d;
        d.waiting = false;
        format!("{d:?}").hash(&mut h);
    }
    h.finish()
}

/// A number from an appraisal that two items can be compared by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum StatKind {
    #[default]
    ArmorLevel,
    Slashing,
    Piercing,
    Bludgeoning,
    Fire,
    Cold,
    Acid,
    Electric,
    Nether,
    Damage,
    DamageModifier,
    ElementalDamageBonus,
    Speed,
    AttackSkill,
    MeleeDefense,
    MissileDefense,
    MagicDefense,
    ManaConversion,
    SpellDamage,
    Workmanship,
    Value,
    Burden,
}

impl StatKind {
    /// The name the comparison shows.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ArmorLevel => "Armor Level",
            Self::Slashing => "Slashing",
            Self::Piercing => "Piercing",
            Self::Bludgeoning => "Bludgeoning",
            Self::Fire => "Fire",
            Self::Cold => "Cold",
            Self::Acid => "Acid",
            Self::Electric => "Electric",
            Self::Nether => "Nether",
            Self::Damage => "Damage",
            Self::DamageModifier => "Damage Modifier",
            Self::ElementalDamageBonus => "Elemental Damage Bonus",
            Self::Speed => "Speed",
            Self::AttackSkill => "Attack Skill",
            Self::MeleeDefense => "Melee Defense",
            Self::MissileDefense => "Missile Defense",
            Self::MagicDefense => "Magic Defense",
            Self::ManaConversion => "Mana Conversion",
            Self::SpellDamage => "Spell Damage",
            Self::Workmanship => "Workmanship",
            Self::Value => "Value",
            Self::Burden => "Burden",
        }
    }

    /// A smaller number is the better one: a quicker weapon, a lighter item.
    #[must_use]
    pub const fn lower_is_better(self) -> bool {
        matches!(self, Self::Speed | Self::Burden)
    }

    /// Whether the number is a bonus shown as a percentage.
    #[must_use]
    pub const fn percent(self) -> bool {
        matches!(
            self,
            Self::DamageModifier
                | Self::AttackSkill
                | Self::MeleeDefense
                | Self::MissileDefense
                | Self::MagicDefense
                | Self::ManaConversion
                | Self::SpellDamage
        )
    }

    /// Whether an item without the number has none of it, rather than an unknown amount: a bonus
    /// or a protection, but not a speed, a workmanship or a value.
    #[must_use]
    pub const fn absent_is_zero(self) -> bool {
        !matches!(
            self,
            Self::Speed | Self::Workmanship | Self::Value | Self::Burden
        )
    }

    /// `v` as the comparison writes it.
    #[must_use]
    pub fn show(self, v: f64) -> String {
        if self.percent() {
            format!("{v:+.0}%")
        } else {
            grouped(v, false)
        }
    }

    /// The difference `d` as the comparison writes it, signed.
    #[must_use]
    pub fn show_difference(self, d: f64) -> String {
        if self.percent() {
            format!("{d:+.0}%")
        } else {
            grouped(d, true)
        }
    }
}

/// `v` rounded, its thousands grouped with commas, signed when `signed`.
fn grouped(v: f64, signed: bool) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = crate::ui::hud::grouped(v.abs().round() as u64);
    let sign = if v < -0.5 {
        "-"
    } else if signed {
        "+"
    } else {
        ""
    };
    format!("{sign}{whole}")
}

/// One number of an appraisal, without the spells on the item.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Stat {
    pub kind: StatKind,
    pub value: f64,
    /// The number as appraised, where spells on the item change it. Equal to `value` when
    /// their part could not be taken out.
    pub with_spells: Option<f64>,
}

impl Stat {
    /// A number no spell changes.
    #[must_use]
    pub const fn plain(kind: StatKind, value: f64) -> Self {
        Self {
            kind,
            value,
            with_spells: None,
        }
    }
}

/// The armour types in the order the examine window draws them, with the slot of the armour's
/// modifiers each reads.
const ARMOR_TYPES: [(StatKind, usize); 8] = [
    (StatKind::Slashing, 0),
    (StatKind::Piercing, 1),
    (StatKind::Bludgeoning, 2),
    (StatKind::Fire, 4),
    (StatKind::Cold, 3),
    (StatKind::Acid, 5),
    (StatKind::Electric, 7),
    (StatKind::Nether, 6),
];

/// A modifier such as `1.05` as the bonus it gives, in percent.
fn bonus(m: f64) -> f64 {
    ((m - 1.0) * 100.0).round()
}

/// What one item spell does to a number, read from its description in the spell table.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Effect {
    /// Adds to a number in the units the comparison shows it in (points, or percentage points).
    Add(StatKind, f64),
    /// Adds to one armour type's modifier (slot as in [`ARMOR_TYPES`]).
    ArmorMod(usize, f64),
    /// Multiplies the mana conversion modifier.
    ManaConversion(f64),
}

/// The effect of an item spell from its description ("Improves a shield or piece of armor's
/// armor value by 200 points."), or `None` for a spell that changes no compared number.
fn spell_effect(description: &str) -> Option<Effect> {
    let d = description.to_ascii_lowercase();
    let lowers = ["decreases", "worsens", "reduces", "lowers"]
        .iter()
        .any(|w| d.starts_with(w));
    let sign = if lowers { -1.0 } else { 1.0 };
    // The first number after "by" or "adding", which ends the sentence's amount.
    let at = d
        .find(" by ")
        .map(|i| i + 4)
        .or_else(|| d.find("adding ").map(|i| i + 7))?;
    let amount: String = d[at..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let n = sign * amount.parse::<f64>().ok()?;
    let has = |w: &str| d.contains(w);
    Some(if has("armor value") {
        Effect::Add(StatKind::ArmorLevel, n)
    } else if has("resistance to") {
        let slot = ARMOR_TYPES.iter().find_map(|(kind, slot)| {
            has(&format!("to {} damage", kind.name().to_ascii_lowercase())).then_some(*slot)
        })?;
        Effect::ArmorMod(slot, n / 100.0)
    } else if has("damage value") {
        Effect::Add(StatKind::Damage, n)
    } else if has("weapon's speed") {
        // A quicker weapon has a lower speed number.
        Effect::Add(StatKind::Speed, -n)
    } else if has("attack skill modifier") {
        Effect::Add(StatKind::AttackSkill, n)
    } else if has("melee defense") {
        Effect::Add(StatKind::MeleeDefense, n)
    } else if has("missile defense") {
        Effect::Add(StatKind::MissileDefense, n)
    } else if has("magic defense") {
        Effect::Add(StatKind::MagicDefense, n)
    } else if has("mana conversion") {
        Effect::ManaConversion(1.0 + n / 100.0)
    } else if has("elemental damage bonus") {
        Effect::Add(StatKind::SpellDamage, n)
    } else if has("damage modifier") {
        Effect::Add(StatKind::DamageModifier, n)
    } else {
        return None;
    })
}

/// What the spells on an item add to its numbers: each spell's effect, the strongest of each
/// spell category counting, as the game counts them.
#[derive(Debug, Clone, Default)]
struct Spells {
    add: BTreeMap<StatKind, f64>,
    armor_mods: [f64; 8],
    mana_conversion: f64,
}

impl Spells {
    fn of(p: &AppraisalView) -> Self {
        let mut strongest: BTreeMap<(u32, u8, usize), (f64, Effect)> = BTreeMap::new();
        for spell in p.magic.spells.iter().flatten().filter(|s| s.enchantment) {
            let Some(effect) = spell_effect(&spell.description) else {
                continue;
            };
            let (what, slot, size) = match effect {
                Effect::Add(kind, n) => (0, kind as usize, n.abs()),
                Effect::ArmorMod(slot, n) => (1, slot, n.abs()),
                Effect::ManaConversion(m) => (2, 0, (m - 1.0).abs()),
            };
            // A spell of no known category is its own.
            let category = if spell.category == 0 {
                spell.raw_id
            } else {
                spell.category
            };
            let e = strongest
                .entry((category, what, slot))
                .or_insert((0.0, effect));
            if size > e.0 {
                *e = (size, effect);
            }
        }
        let mut out = Self {
            mana_conversion: 1.0,
            ..Self::default()
        };
        for (_, effect) in strongest.into_values() {
            match effect {
                Effect::Add(kind, n) => *out.add.entry(kind).or_default() += n,
                Effect::ArmorMod(slot, n) => out.armor_mods[slot] += n,
                Effect::ManaConversion(m) => out.mana_conversion *= m,
            }
        }
        out
    }

    fn add(&self, kind: StatKind) -> f64 {
        self.add.get(&kind).copied().unwrap_or(0.0)
    }
}

/// The highest armour modifier the game gives: a spell past it adds nothing that shows.
const ARMOR_MOD_CAP: f64 = 2.0;

/// The numbers the appraisal's enchantment bits mark as changed by spells, by the property the
/// bit is for (see `pyreal_presentation::appraisal::HIGHLIGHTED_PROPERTIES`).
fn enchanted_kinds(p: &AppraisalView) -> Vec<StatKind> {
    let mut out = Vec::new();
    for key in p.enchantment_mods.keys() {
        match key {
            0x1C => {
                out.push(StatKind::ArmorLevel);
                out.extend(ARMOR_TYPES.iter().map(|(k, _)| *k));
            }
            0x2C => out.push(StatKind::Damage),
            0x3F => out.push(StatKind::DamageModifier),
            0x31 => out.push(StatKind::Speed),
            0x3E => out.push(StatKind::AttackSkill),
            0x1D => out.push(StatKind::MeleeDefense),
            0x90 => out.push(StatKind::ManaConversion),
            0x98 => out.push(StatKind::SpellDamage),
            k => out.extend(
                [0x0D, 0x0E, 0x0F, 0x11, 0x10, 0x12, 0x13, 0xA5]
                    .iter()
                    .position(|x| x == k)
                    .map(|i| ARMOR_TYPES[i].0),
            ),
        }
    }
    out
}

/// The numbers of `p` worth comparing, as the examine window reads them, with the spells on the
/// item taken out. A failed assess gives only what it shows: the value and the burden.
#[must_use]
pub fn stats_of(p: &AppraisalView) -> Vec<Stat> {
    use pyreal_presentation::appraisal::equip;
    let spells = Spells::of(p);
    let marked = enchanted_kinds(p);
    let mut out = Vec::new();
    // `base` without the spells, `shown` as appraised; a number the bits mark as changed that no
    // spell explains keeps its appraised value, marked as including spells.
    let put = |out: &mut Vec<Stat>, kind: StatKind, base: f64, shown: f64| {
        let changed = (base - shown).abs() > 1e-6;
        let with_spells = (changed || marked.contains(&kind)).then_some(shown);
        out.push(Stat {
            kind,
            value: base,
            with_spells,
        });
    };
    if let Some(v) = p.value {
        put(&mut out, StatKind::Value, f64::from(v), f64::from(v));
    }
    if let Some(v) = p.burden {
        put(&mut out, StatKind::Burden, f64::from(v), f64::from(v));
    }
    if !p.success {
        return out;
    }
    if let Some(w) = p.workmanship.filter(|w| *w > 0) {
        put(&mut out, StatKind::Workmanship, f64::from(w), f64::from(w));
    }
    if let (Some(level), Some(mods)) = (p.armor_level.filter(|l| *l > 0), p.armor_mods) {
        let shown = f64::from(level);
        let base = shown - spells.add(StatKind::ArmorLevel);
        put(&mut out, StatKind::ArmorLevel, base, shown);
        for (kind, slot) in ARMOR_TYPES {
            let m = f64::from(mods[slot]);
            let raised = spells.armor_mods[slot];
            // At the cap the spell's full part cannot be told, so the number stays as shown.
            let capped = raised > 0.0 && m >= ARMOR_MOD_CAP - 1e-3;
            let base_mod = if capped { m } else { m - raised };
            put(
                &mut out,
                kind,
                (base * base_mod).round(),
                (shown * m).round(),
            );
            if capped {
                if let Some(last) = out.last_mut() {
                    last.with_spells = Some((shown * m).round());
                }
            }
        }
    }
    let loc = p.valid_locations;
    if let Some(w) = p.weapon.filter(|_| loc & equip::ANY_WEAPON != 0) {
        let launcher = loc & equip::MISSILE_WEAPON != 0 && p.ammo_type != 0;
        if launcher {
            let shown = bonus(w.damage_mod);
            let base = shown - spells.add(StatKind::DamageModifier);
            put(&mut out, StatKind::DamageModifier, base, shown);
            if let Some(n) = p.elemental_damage_bonus.filter(|n| *n > 0) {
                put(
                    &mut out,
                    StatKind::ElementalDamageBonus,
                    f64::from(n),
                    f64::from(n),
                );
            }
        } else if w.weapon_damage > 0 {
            let shown = f64::from(w.weapon_damage);
            put(
                &mut out,
                StatKind::Damage,
                shown - spells.add(StatKind::Damage),
                shown,
            );
        }
        if loc & equip::SPEED_SHOWN != 0 && w.weapon_time >= 0 {
            let shown = f64::from(w.weapon_time);
            put(
                &mut out,
                StatKind::Speed,
                shown - spells.add(StatKind::Speed),
                shown,
            );
        }
        if !launcher && (w.weapon_offense - 1.0).abs() > f64::EPSILON {
            let shown = bonus(w.weapon_offense);
            let base = shown - spells.add(StatKind::AttackSkill);
            put(&mut out, StatKind::AttackSkill, base, shown);
        }
    }
    for (kind, m) in [
        StatKind::MeleeDefense,
        StatKind::MissileDefense,
        StatKind::MagicDefense,
    ]
    .into_iter()
    .zip(p.defense_mods)
    {
        if let Some(m) = m.filter(|m| (m - 1.0).abs() > f64::EPSILON) {
            let shown = bonus(m);
            put(&mut out, kind, shown - spells.add(kind), shown);
        }
    }
    if let Some(m) = p.mana_conversion_mod.filter(|m| *m != 0.0) {
        let shown = (m * 100.0).round();
        put(
            &mut out,
            StatKind::ManaConversion,
            (m / spells.mana_conversion * 100.0).round(),
            shown,
        );
    }
    if let Some(m) = p
        .elemental_damage_mod
        .filter(|m| (m - 1.0).abs() > f64::EPSILON && p.caster_damage_type.is_some())
    {
        let shown = bonus(m);
        let base = shown - spells.add(StatKind::SpellDamage);
        put(&mut out, StatKind::SpellDamage, base, shown);
    }
    out
}

/// How one number of an item compares with the same number of what is worn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Difference {
    pub kind: StatKind,
    /// The item's number, and the worn item's; `None` where it is unknown.
    pub item: Option<f64>,
    pub worn: Option<f64>,
    /// The item is better (`Some(true)`) or worse (`Some(false)`); `None` when they are the same
    /// or either is unknown.
    pub better: Option<bool>,
    /// The worn number with its spells, where they change it ([`Stat::with_spells`]).
    pub worn_with_spells: Option<f64>,
}

impl Difference {
    /// How much more the item has than what is worn, when both are known.
    #[must_use]
    pub fn by(&self) -> Option<f64> {
        Some(self.item? - self.worn?)
    }
}

/// Whether `a` is better than `b` by `kind`; `None` when they are the same or either is unknown.
fn better(kind: StatKind, a: Option<f64>, b: Option<f64>) -> Option<bool> {
    match (a, b) {
        (Some(a), Some(b)) if (a - b).abs() > 1e-6 => Some((a > b) != kind.lower_is_better()),
        _ => None,
    }
}

/// Every number either item has, in the order [`StatKind`] lists them, and which is better.
#[must_use]
pub fn compare(item: &[Stat], worn: &[Stat]) -> Vec<Difference> {
    let find = |stats: &[Stat], kind: StatKind| {
        stats
            .iter()
            .find(|s| s.kind == kind)
            .map(|s| s.value)
            .or_else(|| kind.absent_is_zero().then_some(0.0))
    };
    let mut kinds: Vec<StatKind> = item.iter().chain(worn).map(|s| s.kind).collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds
        .into_iter()
        .filter(|kind| {
            // A protection or bonus neither item has says nothing.
            let none = |s: &[Stat]| find(s, *kind).is_none_or(|v| v == 0.0);
            !(none(item) && none(worn))
        })
        .map(|kind| {
            let (a, b) = (find(item, kind), find(worn, kind));
            Difference {
                kind,
                item: a,
                worn: b,
                better: better(kind, a, b),
                worn_with_spells: worn
                    .iter()
                    .find(|s| s.kind == kind)
                    .and_then(|s| s.with_spells),
            }
        })
        .collect()
}

/// A body part armour covers, by the places that cover it (the clothing layer and the armour
/// layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BodyPart {
    Head,
    Chest,
    Abdomen,
    UpperArms,
    LowerArms,
    Hands,
    UpperLegs,
    LowerLegs,
    Feet,
}

impl BodyPart {
    const ALL: [(Self, u32); 9] = [
        (Self::Head, 0x1),
        (Self::Chest, 0x2 | 0x200),
        (Self::Abdomen, 0x4 | 0x400),
        (Self::UpperArms, 0x8 | 0x800),
        (Self::LowerArms, 0x10 | 0x1000),
        (Self::Hands, 0x20),
        (Self::UpperLegs, 0x40 | 0x2000),
        (Self::LowerLegs, 0x80 | 0x4000),
        (Self::Feet, 0x100),
    ];

    /// The places that cover a body part.
    pub const PLACES: u32 = 0x7FFF;

    /// The body parts `places` cover, head to feet.
    pub fn of(places: u32) -> impl Iterator<Item = Self> {
        Self::ALL
            .into_iter()
            .filter(move |(_, m)| places & m != 0)
            .map(|(p, _)| p)
    }

    /// The name the comparison shows.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Head => "Head",
            Self::Chest => "Chest",
            Self::Abdomen => "Abdomen",
            Self::UpperArms => "Upper Arms",
            Self::LowerArms => "Lower Arms",
            Self::Hands => "Hands",
            Self::UpperLegs => "Upper Legs",
            Self::LowerLegs => "Lower Legs",
            Self::Feet => "Feet",
        }
    }
}

/// The armour level on one body part with the item, and with what is worn there now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartDifference {
    pub part: BodyPart,
    pub item: f64,
    pub worn: f64,
    pub better: Option<bool>,
}

/// One worn item in a comparison: its name, the places it is worn in, and its numbers.
#[derive(Debug, Clone, Copy)]
pub struct WornStats<'a> {
    pub name: &'a str,
    pub places: u32,
    pub stats: &'a [Stat],
}

/// How an item compares with what it would replace.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Compared {
    /// The worn items' names, as a sentence names them.
    pub with: String,
    /// For armour over more than one body part, its armour level on each part.
    pub parts: Vec<PartDifference>,
    /// The numbers; totals over the body parts and the worn items when `totals` is set.
    pub rows: Vec<Difference>,
    pub totals: bool,
}

/// `names` as a sentence lists them: "A", "A and B", "A, B and C".
fn listed(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The totals of `pieces` (places, numbers): a body part's number counts once for each part the
/// piece covers, value and burden once for each piece. Workmanship and speed have no total.
fn totals(pieces: &[(u32, &[Stat])]) -> Vec<Stat> {
    let mut out: Vec<Stat> = Vec::new();
    for (places, stats) in pieces {
        #[allow(clippy::cast_precision_loss)]
        let parts = BodyPart::of(*places).count().max(1) as f64;
        for s in stats.iter() {
            let n = match s.kind {
                StatKind::Workmanship | StatKind::Speed => continue,
                k if k == StatKind::ArmorLevel || ARMOR_TYPES.iter().any(|(a, _)| *a == k) => parts,
                _ => 1.0,
            };
            let t = match out.iter_mut().find(|t| t.kind == s.kind) {
                Some(t) => t,
                None => {
                    out.push(Stat::plain(s.kind, 0.0));
                    out.last_mut().expect("just pushed")
                }
            };
            t.value += s.value * n;
            if s.with_spells.is_some() || t.with_spells.is_some() {
                let before = t.with_spells.unwrap_or(t.value - s.value * n);
                t.with_spells = Some(before + s.with_spells.unwrap_or(s.value) * n);
            }
        }
    }
    out
}

/// How an item worn in `places` with numbers `item` compares with what it would replace. Armour
/// over more than one body part, or replacing more than one piece, is compared part by part, its
/// numbers totalled over the parts; anything else number by number with the one worn item.
#[must_use]
pub fn compare_worn(places: u32, item: &[Stat], worn: &[WornStats<'_>]) -> Compared {
    let names: Vec<&str> = worn.iter().map(|w| w.name).collect();
    let covered = places & BodyPart::PLACES;
    let worn_covered = worn
        .iter()
        .fold(0, |a, w| a | (w.places & BodyPart::PLACES));
    let parts: Vec<BodyPart> = BodyPart::of(covered | worn_covered).collect();
    if covered == 0 || (worn.len() <= 1 && parts.len() <= 1) {
        return Compared {
            with: listed(&names),
            rows: worn
                .first()
                .map_or_else(Vec::new, |w| compare(item, w.stats)),
            ..Compared::default()
        };
    }
    let level = |stats: &[Stat]| {
        stats
            .iter()
            .find(|s| s.kind == StatKind::ArmorLevel)
            .map_or(0.0, |s| s.value)
    };
    let part_rows = parts
        .iter()
        .map(|&part| {
            let on = |p: u32| BodyPart::of(p).any(|x| x == part);
            let a = if on(covered) { level(item) } else { 0.0 };
            let b: f64 = worn
                .iter()
                .filter(|w| on(w.places))
                .map(|w| level(w.stats))
                .sum();
            PartDifference {
                part,
                item: a,
                worn: b,
                better: better(StatKind::ArmorLevel, Some(a), Some(b)),
            }
        })
        .collect();
    let pieces: Vec<(u32, &[Stat])> = worn.iter().map(|w| (w.places, w.stats)).collect();
    Compared {
        with: listed(&names),
        parts: part_rows,
        rows: compare(&totals(&[(covered, item)]), &totals(&pieces)),
        totals: true,
    }
}

/// The weapon places of one kind: melee (one-handed or two), missile, or a caster held.
const WEAPON_KINDS: [u32; 3] = [0x0010_0000 | 0x0200_0000, 0x0040_0000, 0x0100_0000];

/// What is worn in the place `item` goes: every worn piece of armour or clothing it would
/// displace, a wielded weapon of its kind, or the worn things sharing one of its places, at most
/// two (both rings, both bracelets).
#[must_use]
pub fn worn_in_place<'a>(item: &Item, equipped: &'a [Item]) -> Vec<&'a Item> {
    if item.worn != 0 || item.equip_locations == 0 {
        return Vec::new();
    }
    let kind = WEAPON_KINDS
        .into_iter()
        .find(|k| item.equip_locations & k != 0);
    let places = kind.unwrap_or(item.equip_locations);
    let worn = equipped
        .iter()
        .filter(move |w| w.id != item.id && w.worn & places != 0);
    if kind.is_none() && places & BodyPart::PLACES != 0 {
        worn.collect()
    } else {
        worn.take(2).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyreal_client_contract::view::AppraisalSpellView;

    fn item(id: u32, stack: u32) -> Item {
        Item {
            id: ObjectId(id),
            name: "Steel Yaoji".into(),
            stack: Some(stack),
            ..Item::default()
        }
    }

    fn answer(id: u32, delivery: u64, success: bool) -> Appraised {
        Appraised {
            id: ObjectId(id),
            delivery,
            success,
            ..Appraised::default()
        }
    }

    #[test]
    fn an_item_is_asked_about_once_and_again_only_when_it_changes() {
        let mut book = Appraisals::default();
        let sword = item(1, 1);
        assert!(book.want(&sword, None, 0.0), "first look asks");
        assert!(!book.want(&sword, None, 1.0), "waiting, not asked twice");
        assert_eq!(book.status(sword.id, None, 1.0), Status::Waiting);
        let a = answer(1, 7, true);
        assert!(!book.want(&sword, Some(&a), 2.0));
        assert_eq!(book.status(sword.id, Some(&a), 2.0), Status::Ready(&a));
        assert!(!book.want(&sword, Some(&a), 100.0), "answered, never again");
        let changed = item(1, 2);
        assert!(book.want(&changed, Some(&a), 101.0), "changed, asked again");
        assert_eq!(
            book.status(sword.id, Some(&a), 101.0),
            Status::Waiting,
            "the old answer is not shown"
        );
        let b = answer(1, 9, true);
        assert_eq!(book.status(sword.id, Some(&b), 102.0), Status::Ready(&b));
    }

    #[test]
    fn a_failed_assess_and_a_missing_reply_say_so() {
        let mut book = Appraisals::default();
        let sword = item(1, 1);
        book.want(&sword, None, 0.0);
        let failed = answer(1, 3, false);
        assert_eq!(
            book.status(sword.id, Some(&failed), 1.0),
            Status::Failed(&failed)
        );
        assert!(
            !book.want(&sword, Some(&failed), 30.0),
            "a failure is not retried"
        );
        let ring = item(2, 1);
        book.want(&ring, None, 0.0);
        assert_eq!(book.status(ring.id, None, 4.9), Status::Waiting);
        assert_eq!(book.status(ring.id, None, 5.0), Status::NoReply);
        assert!(!book.want(&ring, None, 10.0));
        assert!(book.want(&ring, None, 15.0), "asked again after a while");
    }

    #[test]
    fn an_answer_already_there_is_used_without_asking() {
        let mut book = Appraisals::default();
        let a = answer(1, 4, true);
        assert!(!book.want(&item(1, 1), Some(&a), 0.0));
        assert_eq!(book.status(ObjectId(1), Some(&a), 0.0), Status::Ready(&a));
    }

    #[test]
    fn a_better_number_is_marked_and_a_lower_speed_is_better() {
        let s = Stat::plain;
        let new = [s(StatKind::ArmorLevel, 450.0), s(StatKind::Speed, 30.0)];
        let worn = [
            s(StatKind::ArmorLevel, 420.0),
            s(StatKind::Speed, 40.0),
            s(StatKind::MeleeDefense, 5.0),
        ];
        let d = compare(&new, &worn);
        let of = |k| d.iter().find(|x| x.kind == k).copied().unwrap();
        assert_eq!(of(StatKind::ArmorLevel).better, Some(true));
        assert_eq!(of(StatKind::ArmorLevel).by(), Some(30.0));
        assert_eq!(of(StatKind::Speed).better, Some(true), "quicker");
        let melee = of(StatKind::MeleeDefense);
        assert_eq!((melee.item, melee.better), (Some(0.0), Some(false)));
        assert_eq!(StatKind::Value.show_difference(800.0), "+800");
        assert_eq!(StatKind::Value.show(12_000.0), "12,000");
        assert_eq!(StatKind::Burden.show_difference(-1500.0), "-1,500");
        let nether = [s(StatKind::Nether, 0.0)];
        assert!(compare(&nether, &nether).is_empty(), "none on either");
    }

    /// An item spell from the spell table, active on the item when `on`.
    fn spell(id: u32, category: u32, on: bool, description: &str) -> AppraisalSpellView {
        AppraisalSpellView {
            resolved: true,
            raw_id: if on { id | 0x8000_0000 } else { id },
            enchantment: on,
            name: String::new(),
            description: description.into(),
            category,
        }
    }

    const IMPEN: &str = "Improves a shield or piece of armor's armor value by {} points.";

    fn helm_view(level: i32, spells: Vec<AppraisalSpellView>) -> AppraisalView {
        AppraisalView {
            success: true,
            armor_level: Some(level),
            armor_mods: Some([1.0; 8]),
            magic: pyreal_client_contract::view::MagicInfoView {
                spells: Some(spells),
                ..Default::default()
            },
            ..AppraisalView::default()
        }
    }

    fn stat(stats: &[Stat], kind: StatKind) -> Stat {
        *stats.iter().find(|s| s.kind == kind).expect("the number")
    }

    #[test]
    fn the_spells_on_an_item_are_taken_out_the_strongest_of_each_kind_counting() {
        let view = helm_view(
            645,
            vec![
                spell(2604, 391, false, &IMPEN.replace("{}", "20")),
                spell(1486, 160, true, &IMPEN.replace("{}", "200")),
                // A weaker one of the same kind under it adds nothing.
                spell(1485, 160, true, &IMPEN.replace("{}", "150")),
                // The item's own cantrip, active as it is worn.
                spell(2604, 391, true, &IMPEN.replace("{}", "20")),
                spell(
                    1562,
                    174,
                    true,
                    "Increases a shield or piece of armor's resistance to slashing damage by 50%.",
                ),
            ],
        );
        let stats = stats_of(&view);
        let al = stat(&stats, StatKind::ArmorLevel);
        assert_eq!((al.value, al.with_spells), (425.0, Some(645.0)));
        let slash = stat(&stats, StatKind::Slashing);
        assert_eq!(
            slash.value,
            (425.0f64 * 0.5).round(),
            "level and modifier both"
        );
        assert_eq!(slash.with_spells, Some(645.0));
        let pierce = stat(&stats, StatKind::Piercing);
        assert_eq!((pierce.value, pierce.with_spells), (425.0, Some(645.0)));

        // Nothing active: the numbers are as appraised.
        let plain = stats_of(&helm_view(300, vec![spell(2604, 391, false, IMPEN)]));
        assert_eq!(
            stat(&plain, StatKind::ArmorLevel),
            Stat::plain(StatKind::ArmorLevel, 300.0)
        );
    }

    #[test]
    fn a_number_marked_as_enchanted_that_no_spell_explains_says_it_includes_spells() {
        let mut view = helm_view(500, Vec::new());
        view.enchantment_mods.insert(0x1C, true);
        let al = stat(&stats_of(&view), StatKind::ArmorLevel);
        assert_eq!((al.value, al.with_spells), (500.0, Some(500.0)));
        // A protection at the game's cap cannot say how much its bane gave.
        let mut capped = helm_view(
            100,
            vec![spell(
                1562,
                174,
                true,
                "Increases a shield or piece of armor's resistance to slashing damage by 150%.",
            )],
        );
        capped.armor_mods = Some([2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
        let slash = stat(&stats_of(&capped), StatKind::Slashing);
        assert_eq!((slash.value, slash.with_spells), (200.0, Some(200.0)));
    }

    #[test]
    fn weapon_spells_come_out_of_their_numbers() {
        for (d, kind, want) in [
            (
                "Increases a weapon's damage value by 24 points.",
                StatKind::Damage,
                Some((StatKind::Damage, 24.0)),
            ),
            (
                "Improves a weapon's speed by 60 points.",
                StatKind::Speed,
                Some((StatKind::Speed, -60.0)),
            ),
            (
                "Worsens a weapon's speed by 20 points.",
                StatKind::Speed,
                Some((StatKind::Speed, 20.0)),
            ),
            (
                "Increases the Melee Defense skill modifier of a weapon or magic caster by 7.5%.",
                StatKind::MeleeDefense,
                Some((StatKind::MeleeDefense, 7.5)),
            ),
            (
                "Increases a weapon's Attack Skill modifier by 3 percentage points.",
                StatKind::AttackSkill,
                Some((StatKind::AttackSkill, 3.0)),
            ),
            (
                "Increases the elemental damage bonus of an elemental magic caster by 8%.",
                StatKind::SpellDamage,
                Some((StatKind::SpellDamage, 8.0)),
            ),
            (
                "Increases a lock's resistance to picking by 50 points.",
                StatKind::Damage,
                None,
            ),
        ] {
            let got = spell_effect(d).map(|e| match e {
                Effect::Add(k, n) => (k, n),
                other => panic!("{other:?}"),
            });
            assert_eq!(got, want, "{d} ({kind:?})");
        }
        assert_eq!(
            spell_effect("Increases a magic casting implement's mana conversion bonus by 60%."),
            Some(Effect::ManaConversion(1.6))
        );
    }

    fn worn_item(id: u32, name: &str, places: u32) -> Item {
        Item {
            id: ObjectId(id),
            name: name.into(),
            worn: places,
            equip_locations: places,
            ..Item::default()
        }
    }

    #[test]
    fn a_coat_is_compared_with_every_piece_it_displaces_part_by_part() {
        let coat = Item {
            equip_locations: 0x200 | 0x400 | 0x800 | 0x1000,
            ..item(1, 1)
        };
        let equipped = [
            worn_item(2, "Breastplate", 0x200),
            worn_item(3, "Girth", 0x400),
            worn_item(4, "Sleeves", 0x800 | 0x1000),
            worn_item(5, "Helm", 0x1),
            worn_item(6, "Shirt", 0x2 | 0x8),
        ];
        let worn: Vec<u32> = worn_in_place(&coat, &equipped)
            .iter()
            .map(|w| w.id.0)
            .collect();
        assert_eq!(
            worn,
            [2, 3, 4],
            "three pieces, and not the shirt under them"
        );

        let s = Stat::plain;
        let coat_stats = [s(StatKind::ArmorLevel, 300.0), s(StatKind::Burden, 900.0)];
        let (bp, gi, sl) = (
            [s(StatKind::ArmorLevel, 350.0), s(StatKind::Burden, 500.0)],
            [s(StatKind::ArmorLevel, 250.0), s(StatKind::Burden, 300.0)],
            [s(StatKind::ArmorLevel, 200.0), s(StatKind::Burden, 400.0)],
        );
        let c = compare_worn(
            coat.equip_locations,
            &coat_stats,
            &[
                WornStats {
                    name: "Breastplate",
                    places: 0x200,
                    stats: &bp,
                },
                WornStats {
                    name: "Girth",
                    places: 0x400,
                    stats: &gi,
                },
                WornStats {
                    name: "Sleeves",
                    places: 0x1800,
                    stats: &sl,
                },
            ],
        );
        assert_eq!(c.with, "Breastplate, Girth and Sleeves");
        let parts: Vec<(BodyPart, f64, f64)> =
            c.parts.iter().map(|p| (p.part, p.item, p.worn)).collect();
        assert_eq!(
            parts,
            [
                (BodyPart::Chest, 300.0, 350.0),
                (BodyPart::Abdomen, 300.0, 250.0),
                (BodyPart::UpperArms, 300.0, 200.0),
                (BodyPart::LowerArms, 300.0, 200.0),
            ]
        );
        assert_eq!(c.parts[0].better, Some(false));
        let total = |k| *c.rows.iter().find(|d| d.kind == k).unwrap();
        assert_eq!(total(StatKind::ArmorLevel).by(), Some(1200.0 - 1000.0));
        assert_eq!(total(StatKind::Burden).by(), Some(900.0 - 1200.0));
        assert_eq!(total(StatKind::Burden).better, Some(true), "lighter");

        // A breastplate where a coat is worn gives up the coat's other parts.
        let plate = [s(StatKind::ArmorLevel, 400.0)];
        let worn_coat = [s(StatKind::ArmorLevel, 300.0)];
        let c = compare_worn(
            0x200,
            &plate,
            &[WornStats {
                name: "Coat",
                places: 0x1E00,
                stats: &worn_coat,
            }],
        );
        assert_eq!(c.parts.len(), 4);
        assert_eq!((c.parts[1].item, c.parts[1].worn), (0.0, 300.0));
        assert_eq!(
            c.rows
                .iter()
                .find(|d| d.kind == StatKind::ArmorLevel)
                .unwrap()
                .by(),
            Some(400.0 - 1200.0)
        );
    }

    #[test]
    fn a_weapon_is_compared_with_one_of_its_kind_and_rings_with_both() {
        let sword = Item {
            equip_locations: 0x10_0000,
            ..item(1, 1)
        };
        let bow = worn_item(2, "Bow", 0x40_0000);
        assert!(
            worn_in_place(&sword, std::slice::from_ref(&bow)).is_empty(),
            "a bow is not a sword"
        );
        let axe = worn_item(3, "Axe", 0x10_0000);
        let two_hander = worn_item(4, "Greatsword", 0x200_0000);
        assert_eq!(worn_in_place(&sword, &[bow.clone(), axe.clone()]), [&axe]);
        assert_eq!(
            worn_in_place(&sword, std::slice::from_ref(&two_hander)),
            [&two_hander]
        );
        let ring = Item {
            equip_locations: 0x4_0000 | 0x8_0000,
            ..item(5, 1)
        };
        let rings = [
            worn_item(6, "Ring", 0x4_0000),
            worn_item(7, "Ring", 0x8_0000),
        ];
        assert_eq!(worn_in_place(&ring, &rings).len(), 2);
        // One ring is one comparison of its own.
        let c = compare_worn(
            ring.equip_locations,
            &[],
            &[WornStats {
                name: "Ring",
                places: 0x4_0000,
                stats: &[],
            }],
        );
        assert!(!c.totals && c.parts.is_empty());
    }

    #[test]
    fn a_worn_answer_is_asked_for_again_after_a_minute_and_shown_meanwhile() {
        let mut book = Appraisals::default();
        let cap = item(1, 1);
        let a = answer(1, 3, true);
        assert!(!book.want_within(&cap, Some(&a), 0.0, WORN_ANSWER_LASTS));
        assert!(!book.want_within(&cap, Some(&a), 59.0, WORN_ANSWER_LASTS));
        assert!(book.want_within(&cap, Some(&a), 60.0, WORN_ANSWER_LASTS));
        assert!(
            !book.want_within(&cap, Some(&a), 61.0, WORN_ANSWER_LASTS),
            "once"
        );
        assert_eq!(
            book.status(cap.id, Some(&a), 70.0),
            Status::Ready(&a),
            "the old one meanwhile"
        );
        let b = answer(1, 8, true);
        assert!(!book.want_within(&cap, Some(&b), 71.0, WORN_ANSWER_LASTS));
        assert_eq!(book.status(cap.id, Some(&b), 71.0), Status::Ready(&b));
        assert!(!book.want_within(&cap, Some(&b), 130.0, WORN_ANSWER_LASTS));
        assert!(
            book.want_within(&cap, Some(&b), 131.0, WORN_ANSWER_LASTS),
            "a minute after the new one"
        );
    }

    #[test]
    fn an_inscribed_item_is_asked_about_again_a_second_later() {
        let mut book = Appraisals::default();
        let scroll = item(1, 1);
        let a = answer(1, 4, true);
        book.want(&scroll, Some(&a), 0.0);
        book.inscribed(scroll.id, 10.0);
        assert!(book.due(10.5).is_empty());
        assert_eq!(book.due(11.0), [scroll.id]);
        assert!(book.due(12.0).is_empty(), "once");
        assert_eq!(
            book.status(scroll.id, Some(&a), 11.5),
            Status::Ready(&a),
            "the old one meanwhile"
        );
        let b = answer(1, 6, true);
        assert_eq!(book.status(scroll.id, Some(&b), 12.0), Status::Ready(&b));
        // An item never hovered is asked about all the same.
        book.inscribed(ObjectId(9), 20.0);
        assert_eq!(book.due(21.0), [ObjectId(9)]);
    }
}
