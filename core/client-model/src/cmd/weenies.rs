//! A world's weenie list, as Empyrean's status endpoint publishes it at `/weenies`, and the search
//! the Admin window's Find and Spawn runs over it as an administrator types.
//!
//! The document is tab-separated (`→` marks a tab):
//!
//! ```text
//! dereth-weenie-list→1
//! world→<world name>
//! weenie→<wcid>→<class name>→<weenie type>→<item type>→<creature type>→<level>→<icon>→<name>
//! ```
//!
//! A number given as `-` is none. The server's `@create` and `@ci` take the class id, so a pick
//! is sent by its id and never by its name.

/// The format's first line.
const HEADER: &str = "dereth-weenie-list\t1";

/// What a weenie is, as Find and Spawn sorts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A creature, a vendor or a pet: spawned beside the administrator.
    Creature,
    /// Something that can be carried: made in the administrator's pack.
    Item,
    /// Anything else (a generator, a portal, a door, a chest): spawned beside the administrator.
    Other,
}

/// One weenie the world can make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weenie {
    pub wcid: u32,
    pub class_name: String,
    pub name: String,
    /// The `WeenieType` number.
    pub weenie_type: u32,
    /// The `ItemType` flags.
    pub item_type: Option<u32>,
    /// The `CreatureType` number.
    pub creature_type: Option<u32>,
    pub level: Option<i32>,
    /// The icon, an `0x06` picture.
    pub icon: Option<u32>,
    pub kind: Kind,
    /// The name and class name in lower case, for the search, and where the name ends in it.
    lower: String,
    name_end: usize,
}

impl Weenie {
    /// What it is, in a word or two: its creature type, its sort of item, or its weenie type.
    #[must_use]
    pub fn about(&self) -> String {
        match self.kind {
            Kind::Creature => match self.weenie_type {
                weenie_type::VENDOR => "Vendor".to_owned(),
                weenie_type::PET | weenie_type::COMBAT_PET => "Pet".to_owned(),
                _ => self
                    .creature_type
                    .and_then(creature_type_name)
                    .map_or_else(|| "Creature".to_owned(), spaced),
            },
            Kind::Item => self
                .item_type
                .and_then(item_type_name)
                .unwrap_or("Item")
                .to_owned(),
            Kind::Other => other_name(self).to_owned(),
        }
    }

    /// The command that makes `amount` of it: in the pack for an item when `in_pack`, else beside
    /// the administrator.
    #[must_use]
    pub fn spawn_line(&self, amount: u32, in_pack: bool) -> String {
        let verb = if in_pack && self.kind == Kind::Item {
            "ci"
        } else {
            "create"
        };
        if amount > 1 {
            format!("@{verb} {} {amount}", self.wcid)
        } else {
            format!("@{verb} {}", self.wcid)
        }
    }

    /// The command that makes `amount` of it beside the administrator, named `name`.
    #[must_use]
    pub fn spawn_named_line(&self, amount: u32, name: &str) -> String {
        format!(
            "@createnamed {} {} {}",
            self.wcid,
            amount.max(1),
            name.trim()
        )
    }
}

/// The weenie types Find and Spawn tells apart.
pub mod weenie_type {
    pub const GENERIC: u32 = 1;
    pub const PORTAL: u32 = 7;
    pub const CREATURE: u32 = 10;
    pub const ADMIN: u32 = 11;
    pub const VENDOR: u32 = 12;
    pub const HOT_SPOT: u32 = 13;
    pub const CORPSE: u32 = 14;
    pub const COW: u32 = 15;
    pub const DOOR: u32 = 19;
    pub const CHEST: u32 = 20;
    pub const PRESSURE_PLATE: u32 = 24;
    pub const LIFE_STONE: u32 = 25;
    pub const SWITCH: u32 = 26;
    pub const SENTINEL: u32 = 41;
    pub const HOUSE: u32 = 53;
    pub const HOUSE_PORTAL: u32 = 59;
    pub const PET: u32 = 69;
    pub const COMBAT_PET: u32 = 71;
}

/// The item types an item can be carried as: everything but a creature, a portal and a
/// lifestone.
const CARRIED: u32 = !(0x10 | 0x1_0000 | 0x1000_0000);

/// A world's weenie list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WeenieList {
    pub world: String,
    /// Every weenie, in class id order.
    pub weenies: Vec<Weenie>,
}

/// Which weenies a search keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Only {
    #[default]
    All,
    Creatures,
    Items,
    Other,
}

impl Only {
    pub const ALL: [Self; 4] = [Self::All, Self::Creatures, Self::Items, Self::Other];

    /// The name on its button.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Creatures => "Creatures",
            Self::Items => "Items",
            Self::Other => "Other",
        }
    }

    fn keeps(self, kind: Kind) -> bool {
        match self {
            Self::All => true,
            Self::Creatures => kind == Kind::Creature,
            Self::Items => kind == Kind::Item,
            Self::Other => kind == Kind::Other,
        }
    }
}

impl WeenieList {
    /// The list in `text`.
    ///
    /// # Errors
    /// The text does not begin with the format's first line.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        if lines.next() != Some(HEADER) {
            return Err("not a weenie list".to_owned());
        }
        let mut list = Self::default();
        for line in lines {
            let mut f = line.split('\t');
            match f.next() {
                Some("world") => list.world = unescape(f.next().unwrap_or("")),
                Some("weenie") => {
                    let fields: Vec<&str> = f.collect();
                    if let Some(w) = weenie(&fields) {
                        list.weenies.push(w);
                    }
                }
                _ => {}
            }
        }
        Ok(list)
    }

    /// The weenies `query` finds, best first, no more than `limit`: every word of it the start
    /// of a word of the name or of the class name, or a number that is the start of the class id. The ones whose name is the
    /// query come first, then those whose name begins with it, then those with a word beginning
    /// with it, then the rest; shorter names first within each.
    #[must_use]
    pub fn search(&self, query: &str, only: Only, limit: usize) -> Vec<&Weenie> {
        self.search_ranked(query, only, limit)
            .into_iter()
            .map(|(_, w)| w)
            .collect()
    }

    /// [`Self::search`], each with its [`name_rank`].
    #[must_use]
    pub fn search_ranked(&self, query: &str, only: Only, limit: usize) -> Vec<(u8, &Weenie)> {
        let query = query.trim().to_lowercase();
        let number = !query.is_empty() && query.bytes().all(|b| b.is_ascii_digit());
        let mut found: Vec<(u8, usize, u32, &Weenie)> = self
            .weenies
            .iter()
            .filter(|w| only.keeps(w.kind))
            .filter_map(|w| {
                let rank = if number {
                    let id = w.wcid.to_string();
                    if id == query {
                        0
                    } else if id.starts_with(&query) {
                        1
                    } else {
                        return None;
                    }
                } else {
                    let name = &w.lower[..w.name_end];
                    let class = &w.lower[w.name_end + 1..];
                    name_rank(name, class, &query)?
                };
                Some((rank, w.name.len(), w.wcid, w))
            })
            .collect();
        found.sort_unstable_by_key(|(rank, len, wcid, _)| (*rank, *len, *wcid));
        found.truncate(limit);
        found.into_iter().map(|(rank, .., w)| (rank, w)).collect()
    }

    /// The weenie with class id `wcid`.
    #[must_use]
    pub fn get(&self, wcid: u32) -> Option<&Weenie> {
        self.weenies
            .binary_search_by_key(&wcid, |w| w.wcid)
            .ok()
            .map(|n| &self.weenies[n])
    }
}

/// How well `query` (in lower case) finds a thing named `name` (in lower case), lower being
/// better: `None` unless every word of it begins a word of the name or begins `also`. The name
/// itself first, then a name beginning with it, then one with a word beginning with its first
/// word, then the rest.
#[must_use]
pub fn name_rank(name: &str, also: &str, query: &str) -> Option<u8> {
    let words: Vec<&str> = query.split_whitespace().collect();
    let begins = |q: &&str| {
        (!also.is_empty() && also.starts_with(*q))
            || name
                .split(|c: char| !c.is_alphanumeric())
                .any(|n| n.starts_with(*q))
    };
    if words.is_empty() || !words.iter().all(begins) {
        return None;
    }
    Some(if name == query {
        0
    } else if name.starts_with(query) {
        1
    } else if words
        .first()
        .is_some_and(|q| name.split(' ').any(|n| n.starts_with(q)))
    {
        2
    } else {
        3
    })
}

/// One `weenie` line's fields after its tag.
fn weenie(f: &[&str]) -> Option<Weenie> {
    let [wcid, class, weenie_type, item_type, creature_type, level, icon, name, ..] = f else {
        return None;
    };
    let number = |s: &str| (*s != *"-").then(|| s.parse::<u32>().ok()).flatten();
    let weenie_type = weenie_type.parse().ok()?;
    let item_type = number(item_type);
    let class_name = unescape(class);
    let name = unescape(name);
    let kind = kind_of(weenie_type, item_type);
    let lower_name = name.to_lowercase();
    let name_end = lower_name.len();
    let lower = format!("{lower_name}\t{}", class_name.to_lowercase());
    Some(Weenie {
        wcid: wcid.parse().ok()?,
        class_name,
        name,
        weenie_type,
        item_type,
        creature_type: number(creature_type),
        level: (*level != "-").then(|| level.parse().ok()).flatten(),
        icon: icon
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok()),
        kind,
        lower,
        name_end,
    })
}

fn kind_of(wtype: u32, item_type: Option<u32>) -> Kind {
    use weenie_type as t;
    match wtype {
        t::CREATURE | t::COW | t::VENDOR | t::PET | t::COMBAT_PET | t::ADMIN | t::SENTINEL => {
            Kind::Creature
        }
        t::PORTAL
        | t::HOT_SPOT
        | t::CORPSE
        | t::DOOR
        | t::CHEST
        | t::PRESSURE_PLATE
        | t::LIFE_STONE
        | t::SWITCH
        | t::HOUSE
        | t::HOUSE_PORTAL => Kind::Other,
        _ => match item_type {
            Some(i) if i & CARRIED != 0 => Kind::Item,
            _ => Kind::Other,
        },
    }
}

/// A text field as the server wrote it: a doubled backslash is one.
fn unescape(s: &str) -> String {
    s.replace("\\\\", "\\")
}

/// `CamelCase` as words: `PhyntosWasp` as `Phyntos Wasp`.
fn spaced(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (n, c) in name.chars().enumerate() {
        if n > 0 && c.is_ascii_uppercase() {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// What something that is neither a creature nor an item is.
fn other_name(w: &Weenie) -> &'static str {
    use weenie_type as t;
    match w.weenie_type {
        t::PORTAL | t::HOUSE_PORTAL => "Portal",
        t::DOOR => "Door",
        t::CHEST => "Chest",
        t::LIFE_STONE => "Lifestone",
        t::SWITCH | t::PRESSURE_PLATE => "Switch",
        t::HOT_SPOT => "Hot spot",
        t::CORPSE => "Corpse",
        t::HOUSE => "House",
        t::GENERIC if w.class_name.ends_with("gen") || w.name.ends_with("Generator") => "Generator",
        _ => "Object",
    }
}

/// The sort of item the lowest of `flags` names.
fn item_type_name(flags: u32) -> Option<&'static str> {
    const NAMES: [&str; 32] = [
        "Melee weapon",
        "Armor",
        "Clothing",
        "Jewelry",
        "Creature",
        "Food",
        "Money",
        "Miscellaneous",
        "Missile weapon",
        "Container",
        "Trophy",
        "Gem",
        "Spell component",
        "Writing",
        "Key",
        "Caster",
        "Portal",
        "Lockable",
        "Trade note",
        "Mana stone",
        "Service",
        "Magic item",
        "Cooking",
        "Alchemy",
        "Item",
        "Fletching",
        "Alchemy",
        "Fletching",
        "Lifestone",
        "Tinkering tool",
        "Salvage",
        "Game board",
    ];
    (flags != 0).then(|| NAMES[flags.trailing_zeros() as usize])
}

/// The creature type `n`'s name, as the game's enumeration spells it.
fn creature_type_name(n: u32) -> Option<&'static str> {
    const NAMES: [&str; 102] = [
        "Invalid",
        "Olthoi",
        "Banderling",
        "Drudge",
        "Mosswart",
        "Lugian",
        "Tumerok",
        "Mite",
        "Tusker",
        "PhyntosWasp",
        "Rat",
        "Auroch",
        "Cow",
        "Golem",
        "Undead",
        "Gromnie",
        "Reedshark",
        "Armoredillo",
        "Fae",
        "Virindi",
        "Wisp",
        "Knathtead",
        "Shadow",
        "Mattekar",
        "Mumiyah",
        "Rabbit",
        "Sclavus",
        "ShallowsShark",
        "Monouga",
        "Zefir",
        "Skeleton",
        "Human",
        "Shreth",
        "Chittick",
        "Moarsman",
        "OlthoiLarvae",
        "Slithis",
        "Deru",
        "FireElemental",
        "Snowman",
        "Unknown",
        "Bunny",
        "LightningElemental",
        "Rockslide",
        "Grievver",
        "Niffis",
        "Ursuin",
        "Crystal",
        "HollowMinion",
        "Scarecrow",
        "Idol",
        "Empyrean",
        "Hopeslayer",
        "Doll",
        "Marionette",
        "Carenzi",
        "Siraluun",
        "AunTumerok",
        "HeaTumerok",
        "Simulacrum",
        "AcidElemental",
        "FrostElemental",
        "Elemental",
        "Statue",
        "Wall",
        "AlteredHuman",
        "Device",
        "Harbinger",
        "DarkSarcophagus",
        "Chicken",
        "GotrokLugian",
        "Margul",
        "BleachedRabbit",
        "NastyRabbit",
        "GrimacingRabbit",
        "Burun",
        "Target",
        "Ghost",
        "Fiun",
        "Eater",
        "Penguin",
        "Ruschk",
        "Thrungus",
        "ViamontianKnight",
        "Remoran",
        "Swarm",
        "Moar",
        "EnchantedArms",
        "Sleech",
        "Mukkir",
        "Merwart",
        "Food",
        "ParadoxOlthoi",
        "Harvest",
        "Energy",
        "Apparition",
        "Aerbax",
        "Touched",
        "BlightedMoarsman",
        "GearKnight",
        "Gurog",
        "Anekshay",
    ];
    NAMES
        .get(n as usize)
        .copied()
        .filter(|s| !matches!(*s, "Invalid" | "Unknown"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "dereth-weenie-list\t1\n\
world\tLocal\n\
weenie\t7\tdrudgeskulker\t10\t16\t3\t8\t0x06001035\tDrudge Skulker\n\
weenie\t200\tgolemmud\t10\t16\t13\t8\t0x06001224\tMud Golem\n\
weenie\t351\tswordlong\t6\t1\t-\t-\t0x06001661\tLong Sword\n\
weenie\t893\tdrudgeskulkergen\t1\t-\t-\t-\t0x06001066\tDrudge Skulker Generator\n\
weenie\t5595\tdrudgeskulkerdancer\t10\t16\t3\t4\t0x06001035\tDrudge Skulker\n\
weenie\t6355\tpyrealsliver\t44\t128\t-\t-\t0x06001C2B\tPyreal Sliver\n\
weenie\t9000\tbackslash\t1\t128\t-\t-\t-\tBack \\\\ Slash\n";

    fn list() -> WeenieList {
        WeenieList::parse(LIST).expect("a list")
    }

    fn names(found: &[&Weenie]) -> Vec<u32> {
        found.iter().map(|w| w.wcid).collect()
    }

    #[test]
    fn the_list_reads_every_weenie_and_what_it_is() {
        let l = list();
        assert_eq!(l.world, "Local");
        assert_eq!(l.weenies.len(), 7);
        let skulker = l.get(7).expect("7");
        assert_eq!(skulker.kind, Kind::Creature);
        assert_eq!(skulker.about(), "Drudge");
        assert_eq!(skulker.level, Some(8));
        assert_eq!(skulker.icon, Some(0x0600_1035));
        assert_eq!(
            l.get(351).map(Weenie::about).as_deref(),
            Some("Melee weapon")
        );
        assert_eq!(l.get(351).map(|w| w.kind), Some(Kind::Item));
        assert_eq!(l.get(893).map(Weenie::about).as_deref(), Some("Generator"));
        assert_eq!(l.get(893).map(|w| w.kind), Some(Kind::Other));
        assert_eq!(l.get(9000).map(|w| w.name.as_str()), Some("Back \\ Slash"));
        assert!(WeenieList::parse("something else").is_err());
    }

    #[test]
    fn a_search_finds_every_word_and_puts_the_closest_names_first() {
        let l = list();
        assert_eq!(names(&l.search("drudge sk", Only::All, 10)), [7, 5595, 893]);
        assert_eq!(names(&l.search("skulker", Only::Creatures, 10)), [7, 5595]);
        assert_eq!(names(&l.search("golemmud", Only::All, 10)), [200]);
        assert_eq!(
            names(&l.search("ulker", Only::All, 10)),
            [0u32; 0],
            "a word's start"
        );
        assert_eq!(names(&l.search("SLIVER", Only::Items, 10)), [6355]);
        assert_eq!(names(&l.search("drudge", Only::Other, 10)), [893]);
        assert_eq!(names(&l.search("drudge", Only::All, 1)), [7]);
    }

    #[test]
    fn a_number_finds_the_class_ids_it_begins() {
        let l = list();
        assert_eq!(names(&l.search("35", Only::All, 10)), [351]);
        assert_eq!(names(&l.search("7", Only::All, 10)), [7]);
    }

    #[test]
    fn an_item_is_made_in_the_pack_and_a_creature_beside_the_administrator() {
        let l = list();
        let sword = l.get(351).expect("sword");
        let skulker = l.get(7).expect("skulker");
        assert_eq!(sword.spawn_line(1, true), "@ci 351");
        assert_eq!(sword.spawn_line(3, false), "@create 351 3");
        assert_eq!(skulker.spawn_line(2, true), "@create 7 2");
        assert_eq!(skulker.spawn_named_line(0, " Bob "), "@createnamed 7 1 Bob");
    }
}
