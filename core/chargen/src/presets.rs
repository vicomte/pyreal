//! Community Presets: profession choices players wrote, offered beside the world's templates.
//!
//! A preset fills in the same attributes and skills a player could choose by hand, so any server
//! accepts it. One table per era; [`Preset::fit`] hides a preset the connected world's costs or
//! credits cannot hold.

use pyreal_assets::tables::{CharGen, SkillTable};
use pyreal_primitives::{EraFeatures, EraId};

use crate::{CharGenState, SkillAdvancementClass, ATTR_MIN};

const ALUVIAN: u32 = 1;
const GHARUNDIM: u32 = 2;
const SHO: u32 = 3;

/// The highest value an attribute starts at.
const ATTR_MAX: u32 = 100;
/// A cost at or above this cannot be bought (a skill that cannot be specialized).
const UNBUYABLE: i32 = 999;

/// A community preset.
#[derive(Debug, PartialEq, Eq)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    /// The one-line summary under the name.
    pub tag: &'static str,
    /// How it plays.
    pub about: &'static str,
    /// What to buy with level credits, when no variant has its own.
    pub later: &'static [&'static str],
    /// The heritage note.
    pub heritage_note: &'static str,
    pub source: &'static str,
    /// Why a heritage without a variant cannot take it. Empty when any heritage can.
    pub needs: &'static str,
    pub variants: &'static [Variant],
}

/// A preset's choices for some heritages.
#[derive(Debug, PartialEq, Eq)]
pub struct Variant {
    /// The heritages it is for; empty for any.
    pub heritages: &'static [u32],
    /// Strength, Endurance, Coordination, Quickness, Focus, Self.
    pub attributes: [u32; 6],
    pub specialized: &'static [u32],
    pub trained: &'static [u32],
    /// Its own level plan, or empty for the preset's.
    pub later: &'static [&'static str],
}

/// Whether a preset works for a heritage on the connected world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fit {
    /// It fits, using `used` skill credits and leaving `left`.
    Usable { used: i32, left: i32 },
    /// Another heritage can take it: these.
    NeedsHeritage(Vec<u32>),
    /// The world cannot hold it.
    Unusable,
}

/// The presets of `era`.
#[must_use]
pub fn presets(era: EraId) -> &'static [Preset] {
    match era {
        EraId::Launch => LAUNCH,
        EraId::Infiltration => INFILTRATION,
        EraId::Eor => EOR,
    }
}

impl Preset {
    /// The variant for `heritage`.
    #[must_use]
    pub fn variant(&self, heritage: u32) -> Option<&'static Variant> {
        let v: &'static [Variant] = self.variants;
        v.iter()
            .find(|v| v.heritages.is_empty() || v.heritages.contains(&heritage))
    }

    /// Every heritage a variant names, in variant order.
    #[must_use]
    pub fn heritages(&self) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        for h in self.variants.iter().flat_map(|v| v.heritages) {
            if !out.contains(h) {
                out.push(*h);
            }
        }
        out
    }

    /// The level plan for `variant`.
    #[must_use]
    pub fn later_for(&self, variant: &Variant) -> &'static [&'static str] {
        if variant.later.is_empty() {
            self.later
        } else {
            variant.later
        }
    }

    /// Whether the preset works for `heritage` on a world with these tables and features.
    #[must_use]
    pub fn fit(&self, tables: &crate::CreationTables, heritage: u32, features: EraFeatures) -> Fit {
        let (cg, skills) = (&tables.chargen, &tables.skills);
        if let Some(v) = self.variant(heritage) {
            return v.fit(cg, skills, heritage, features);
        }
        let others: Vec<u32> = self
            .heritages()
            .into_iter()
            .filter(|h| {
                matches!(
                    self.variant(*h).map(|v| v.fit(cg, skills, *h, features)),
                    Some(Fit::Usable { .. })
                )
            })
            .collect();
        if others.is_empty() {
            Fit::Unusable
        } else {
            Fit::NeedsHeritage(others)
        }
    }
}

impl Variant {
    /// The six attributes and the skills against `heritage`'s credits and costs.
    #[must_use]
    pub fn fit(
        &self,
        cg: &CharGen,
        skills: &SkillTable,
        heritage: u32,
        features: EraFeatures,
    ) -> Fit {
        let Some(hg) = cg.heritage_groups.get(&heritage) else {
            return Fit::Unusable;
        };
        let min = u32::try_from(ATTR_MIN).unwrap_or(10);
        if self
            .attributes
            .iter()
            .any(|a| !(min..=ATTR_MAX).contains(a))
            || self.attributes.iter().sum::<u32>() != hg.attribute_credits
        {
            return Fit::Unusable;
        }
        let Some(used) = credits_used(cg, skills, heritage, self.specialized, self.trained) else {
            return Fit::Unusable;
        };
        let total = i32::try_from(hg.skill_credits).unwrap_or(0);
        let left = total - used;
        if left < 0 || (left > 0 && !features.creation_credit_carry_over) {
            return Fit::Unusable;
        }
        Fit::Usable { used, left }
    }
}

/// The skill credits `specialized` and `trained` cost `heritage`, a skill in both counted once as
/// specialized. `None` when one is not in the table or cannot be bought.
#[must_use]
pub fn credits_used(
    cg: &CharGen,
    skills: &SkillTable,
    heritage: u32,
    specialized: &[u32],
    trained: &[u32],
) -> Option<i32> {
    let trained = trained.iter().filter(|s| !specialized.contains(s));
    let mut used = 0;
    for (id, special) in specialized
        .iter()
        .map(|s| (s, true))
        .chain(trained.map(|s| (s, false)))
    {
        if pyreal_rules::chargen::default_skill_class(cg, skills, heritage, *id)
            == SkillAdvancementClass::Inactive
        {
            return None;
        }
        let (train, spec) = CharGenState::skill_costs(cg, skills, heritage, *id);
        let cost = if special { spec } else { train };
        if !(0..UNBUYABLE).contains(&cost) {
            return None;
        }
        used += cost;
    }
    Some(used)
}

/// What `skill` starts at with `attributes` (in creator order) at `level`.
#[must_use]
pub fn start_value(
    skills: &SkillTable,
    skill: u32,
    level: SkillAdvancementClass,
    attributes: [u32; 6],
) -> i32 {
    // The skill formulas number them Str, End, Quick, Coord, Focus, Self.
    let at = |i: usize| i32::try_from(attributes[i]).unwrap_or(0);
    pyreal_rules::chargen::skill_score(skills, skill, level, |n| match n {
        1 => at(0),
        2 => at(1),
        3 => at(3),
        4 => at(2),
        5 => at(4),
        6 => at(5),
        _ => 0,
    })
}

// Skill ids of the world's skill table.
const BOW: u32 = 2;
const CROSSBOW: u32 = 3;
const DAGGER: u32 = 4;
const MELEE_D: u32 = 6;
const STAFF: u32 = 10;
const SWORD: u32 = 11;
const UNARMED: u32 = 13;
const ARCANE: u32 = 14;
const MANA: u32 = 16;
const ASSESS_PERSON: u32 = 19;
const HEALING: u32 = 21;
const LOCKPICK: u32 = 23;
const ASSESS_CREATURE: u32 = 27;
const CREATURE: u32 = 31;
const ITEM: u32 = 32;
const LIFE: u32 = 33;
const WAR: u32 = 34;
const FLETCHING: u32 = 37;
const ALCHEMY: u32 = 38;
const TWO_HANDED: u32 = 41;
const HEAVY: u32 = 44;
const LIGHT: u32 = 45;
const FINESSE: u32 = 46;
const MISSILE: u32 = 47;
const RECKLESSNESS: u32 = 50;
const SUMMONING: u32 = 54;

const MAGE: [u32; 6] = [10, 100, 10, 10, 100, 100];
const OLD_MAGE: [u32; 6] = [30, 50, 10, 40, 100, 100];

const SYBEX: &str = "The 2000 Sybex strategy guide update.";
const LR: &str = "Community template on Loresraat.";
const OG_SOURCE: &str = "Community template on Loresraat, first played by Og of Morningthaw.";
const COVENANT_SOURCE: &str =
    "Built from the Loresraat Covenant Warrior\u{2019}s high attack and defense attributes.";
const JOAT: &str =
    "Any heritage suits it. Aluvian, Gharu\u{2019}ndim, Sho and Viamontian are born \
                    with Jack of All Trades.";
const CRIT: &str = "Any heritage suits it. Umbraen, Penumbraen and Tumerok are born with \
                    critical-hit augmentations.";
const NEEDS_NOT_ALUVIAN: &str = "Needs Gharu\u{2019}ndim or Sho heritage. Aluvians already have \
                                 Assess Person free, and its two credits would be lost.";

const OG_TAG: &str = "A support mage who buffs, heals and opens portals.";
const OG_ABOUT: &str = "Creature Enchantment and Life Magic are specialized, so your buffs and \
                        heals are strong from the first day. Item Enchantment brings portal recall \
                        and armor spells. War Magic joins later for damage.";
const CLAW_TAG: &str = "A war mage who buffs and travels without help.";
const CLAW_ABOUT: &str = "War Magic is specialized for damage. Creature, Item and Mana Conversion \
                          are trained, so you cast your own buffs and portals from the start. The \
                          name stands for Creature, Life and War.";
const BATTLE_TAG: &str = "Two specialized schools for strong solo play.";
const BATTLE_ABOUT: &str = "War Magic and Life Magic are both specialized, so your attacks and \
                            heals are strong from level one. You start without buffs of your own \
                            and add them with level credits.";
const MELEE_TAG: &str = "A war mage who can stand in melee.";
const MELEE_ABOUT: &str = "War Magic is specialized for damage. Melee Defense and high Quickness \
                           let you take hits while you cast. The credits you keep buy Life Magic \
                           early.";
const SWORD_TAG: &str = "A sword fighter with high attack and defense.";
const SWORD_ABOUT: &str = "Sword and Melee Defense are specialized and backed by maxed Strength, \
                           Coordination and Quickness. Healing keeps you going between fights. \
                           Item Enchantment starts low and is there for portal recall until Focus \
                           and Self are raised.";

/// One variant for any heritage.
const fn any(
    attributes: [u32; 6],
    specialized: &'static [u32],
    trained: &'static [u32],
) -> Variant {
    Variant {
        heritages: &[],
        attributes,
        specialized,
        trained,
        later: &[],
    }
}

/// The 1999 presets.
pub static LAUNCH: &[Preset] = &[
    Preset {
        id: "four",
        name: "Four-School Mage",
        tag: "Every school of magic and Mana Conversion, all trained.",
        about:
            "This was the mage most players recommended in 2000. You can buff, heal, travel and \
                fight from the first day. Casting the same spell again and again weakens it for \
                everyone, and four schools give you many spells to rotate.",
        later: &[
            "Train Melee Defense at level 12.",
            "Train Lockpick or Crossbow at level 26.",
        ],
        heritage_note: "Gharu\u{2019}ndim bring a staff, Aluvians a dagger and Sho their fists as \
                        backup weapons.",
        source: SYBEX,
        needs: "",
        variants: &[any(OLD_MAGE, &[], &[CREATURE, ITEM, LIFE, WAR, MANA])],
    },
    Preset {
        id: "fourmana",
        name: "Four-School Mage, Mana",
        tag: "Mana Conversion specialized, with Item Enchantment soon after.",
        about:
            "Specializing Mana Conversion makes every spell cheaper to cast. Creature, Life and \
                War are trained from the start. The last two credits go to Assess Person so none \
                are lost.",
        later: &["Train Item Enchantment between levels 9 and 16."],
        heritage_note: "Gharu\u{2019}ndim and Sho both suit it.",
        source: "The 2000 Sybex strategy guide update, from its mage writer\u{2019}s own build.",
        needs: NEEDS_NOT_ALUVIAN,
        variants: &[Variant {
            heritages: &[GHARUNDIM, SHO],
            attributes: OLD_MAGE,
            specialized: &[MANA],
            trained: &[CREATURE, LIFE, WAR, ASSESS_PERSON],
            later: &[],
        }],
    },
    Preset {
        id: "og",
        name: "Og Mage",
        tag: OG_TAG,
        about:
            "Creature Enchantment and Life Magic are specialized, so buffs and heals are strong \
                early. Item Enchantment brings portal recall and armor spells. Specializing pays \
                off less in this era, and the choice is permanent.",
        later: &["Train War Magic once you have 16 credits."],
        heritage_note: "Any heritage suits it.",
        source: LR,
        needs: "",
        variants: &[any(OLD_MAGE, &[CREATURE, LIFE], &[ITEM, MANA])],
    },
    Preset {
        id: "battle",
        name: "Battle Mage",
        tag: BATTLE_TAG,
        about: "War Magic and Life Magic are both specialized for strong attacks and heals. The \
                last two credits go to Assess Person so none are lost. Specializing pays off less \
                in this era, and the choice is permanent.",
        later: &["Add Mana Conversion, Item and Creature Enchantment with level credits."],
        heritage_note: "Gharu\u{2019}ndim and Sho both suit it.",
        source: LR,
        needs: NEEDS_NOT_ALUVIAN,
        variants: &[Variant {
            heritages: &[GHARUNDIM, SHO],
            attributes: OLD_MAGE,
            specialized: &[WAR, LIFE],
            trained: &[ASSESS_PERSON],
            later: &[],
        }],
    },
    Preset {
        id: "archer",
        name: "Archer",
        tag: "A crossbow archer who trains everything useful.",
        about: "Crossbow, Melee Defense, Healing and Fletching keep you fighting and supplied. \
                Item Enchantment gives portal spells and bow buffs, and Life Magic heals you. \
                Focus and Self start at 30 so Arcane Lore and your spells can be used.",
        later: &["Train Alchemy for special arrows, or make an alchemy mule."],
        heritage_note: "Sho get Unarmed Combat free and Aluvians get Dagger free. Both make good \
                        backup weapons.",
        source: SYBEX,
        needs: "",
        variants: &[any(
            [60, 40, 100, 70, 30, 30],
            &[],
            &[CROSSBOW, ITEM, HEALING, MELEE_D, FLETCHING, ARCANE, LIFE],
        )],
    },
    Preset {
        id: "fighter",
        name: "Fighter",
        tag: "A fighter who specializes only the heritage weapon.",
        about: "Your heritage weapon is free to train, so specializing it costs little. \
                Everything else is trained, which suits this era\u{2019}s rules. Healing, Life and \
                Item Enchantment cover healing, buffs and portals.",
        later: &["Raise Focus and Self so your spells keep up."],
        heritage_note: "Sho fight with their fists, Gharu\u{2019}ndim with a staff and Aluvians \
                        with a dagger. Sho add Assess Creature, and the others add Lockpick.",
        source: "Built from the 2000 Sybex guide\u{2019}s advice for warriors.",
        needs: "",
        variants: &[
            Variant {
                heritages: &[SHO],
                attributes: [100, 20, 100, 50, 30, 30],
                specialized: &[UNARMED],
                trained: &[MELEE_D, HEALING, LIFE, ITEM, ARCANE, ASSESS_CREATURE],
                later: &[],
            },
            Variant {
                heritages: &[GHARUNDIM],
                attributes: [100, 20, 100, 50, 30, 30],
                specialized: &[STAFF],
                trained: &[MELEE_D, HEALING, LIFE, ITEM, ARCANE, LOCKPICK],
                later: &[],
            },
            Variant {
                heritages: &[ALUVIAN],
                attributes: [60, 20, 100, 90, 30, 30],
                specialized: &[DAGGER],
                trained: &[MELEE_D, HEALING, LIFE, ITEM, ARCANE, LOCKPICK],
                later: &[],
            },
        ],
    },
    Preset {
        id: "sword",
        name: "Swordsman",
        tag: SWORD_TAG,
        about: SWORD_ABOUT,
        later: &["Train Life Magic once you have 12 credits."],
        heritage_note: "Any heritage suits it.",
        source: COVENANT_SOURCE,
        needs: "",
        variants: &[any(
            [100, 10, 100, 100, 10, 10],
            &[SWORD, MELEE_D],
            &[HEALING, ITEM],
        )],
    },
];

/// The February 2005 presets.
pub static INFILTRATION: &[Preset] = &[
    Preset {
        id: "og",
        name: "Og Mage",
        tag: OG_TAG,
        about: OG_ABOUT,
        later: &["Train War Magic at level 26."],
        heritage_note: "Any heritage suits it. Gharu\u{2019}ndim start with a staff skill for \
                        emergencies.",
        source: OG_SOURCE,
        needs: "",
        variants: &[any(MAGE, &[CREATURE, LIFE], &[ITEM, MANA])],
    },
    Preset {
        id: "claw",
        name: "CLaW Mage",
        tag: CLAW_TAG,
        about: CLAW_ABOUT,
        later: &[
            "Train Life Magic at level 16.",
            "Specialize Life Magic at level 40.",
            "Specialize Creature Enchantment at level 80.",
        ],
        heritage_note: "Any heritage suits it.",
        source: LR,
        needs: "",
        variants: &[any(MAGE, &[WAR], &[CREATURE, ITEM, MANA])],
    },
    Preset {
        id: "battle",
        name: "Battle Mage",
        tag: BATTLE_TAG,
        about: BATTLE_ABOUT,
        later: &[
            "Train Mana Conversion at level 5.",
            "Train Item Enchantment at level 16.",
            "Train Creature Enchantment at level 40.",
        ],
        heritage_note: "Any heritage suits it.",
        source: LR,
        needs: "",
        variants: &[any(MAGE, &[WAR, LIFE], &[])],
    },
    Preset {
        id: "melee",
        name: "Melee Mage",
        tag: MELEE_TAG,
        about: MELEE_ABOUT,
        later: &[
            "Train Life Magic at level 7.",
            "Train Item Enchantment at level 20.",
            "Train Creature Enchantment at level 50.",
            "Specialize Melee Defense at level 100.",
        ],
        heritage_note: "Any heritage suits it.",
        source: LR,
        needs: "",
        variants: &[any([10, 30, 10, 80, 100, 100], &[WAR], &[MANA, MELEE_D])],
    },
    Preset {
        id: "archer",
        name: "Archer",
        tag: "A bow or crossbow archer with a dagger for close fights.",
        about: "Your bow and your dagger both draw on maxed Coordination and Quickness. Fletching \
                keeps you in arrows, Healing keeps you on your feet, and Melee Defense helps when \
                enemies close in. Arcane Lore lets you use magic jewelry and armor.",
        later: &[
            "Train Life Magic at level 16.",
            "Train Creature Enchantment at level 40.",
            "Train Item Enchantment at level 80.",
        ],
        heritage_note: "Aluvians get Dagger free, which leaves room for a bow. Gharu\u{2019}ndim \
                        and Sho take a crossbow.",
        source:
            "The Loresraat PvM Archer, with Bow and Dagger for this era\u{2019}s weapon skills.",
        needs: "",
        variants: &[
            Variant {
                heritages: &[ALUVIAN],
                attributes: [10, 10, 100, 100, 100, 10],
                specialized: &[BOW, DAGGER],
                trained: &[MANA, ARCANE, MELEE_D, FLETCHING, HEALING],
                later: &[],
            },
            Variant {
                heritages: &[GHARUNDIM, SHO],
                attributes: [10, 10, 100, 100, 100, 10],
                specialized: &[CROSSBOW, DAGGER],
                trained: &[MANA, ARCANE, MELEE_D, FLETCHING, HEALING],
                later: &[],
            },
        ],
    },
    Preset {
        id: "hfight",
        name: "Heritage Fighter",
        tag: "A fighter who specializes the weapon their heritage knows.",
        about: "Sho fight with their fists and Gharu\u{2019}ndim with a staff. Both skills are \
                free, so specializing costs little. Melee Defense is specialized too, and high \
                Focus feeds Healing, Arcane Lore and Life Magic.",
        later: &[],
        heritage_note: "Sho use Unarmed Combat and Gharu\u{2019}ndim use Staff.",
        source: "Built from the Loresraat Covenant Warrior\u{2019}s attributes and the 2000 \
                 guide\u{2019}s advice on heritage weapons.",
        needs: "Needs Sho or Gharu\u{2019}ndim heritage. Aluvian dagger fighters want Quickness, \
                and the Archer and Swordsman suit them.",
        variants: &[
            Variant {
                heritages: &[SHO],
                attributes: [100, 10, 100, 10, 100, 10],
                specialized: &[UNARMED, MELEE_D],
                trained: &[HEALING, LIFE, ARCANE],
                later: &[
                    "Train Creature Enchantment at level 7.",
                    "Train Item Enchantment at level 20.",
                ],
            },
            Variant {
                heritages: &[GHARUNDIM],
                attributes: [100, 10, 100, 10, 100, 10],
                specialized: &[STAFF, MELEE_D],
                trained: &[HEALING, LIFE, ARCANE],
                later: &[
                    "Train Creature Enchantment at level 5.",
                    "Train Item Enchantment at level 16.",
                ],
            },
        ],
    },
    Preset {
        id: "sword",
        name: "Swordsman",
        tag: SWORD_TAG,
        about: SWORD_ABOUT,
        later: &[
            "Train Life Magic at level 16.",
            "Train Creature Enchantment at level 40.",
        ],
        heritage_note: "Any heritage suits it. An axe or a mace costs 4 credits less to \
                        specialize.",
        source: COVENANT_SOURCE,
        needs: "",
        variants: &[any(
            [100, 10, 100, 100, 10, 10],
            &[SWORD, MELEE_D],
            &[HEALING, ITEM],
        )],
    },
];

/// The end-of-retail presets.
pub static EOR: &[Preset] = &[
    Preset {
        id: "og",
        name: "Og Mage",
        tag: OG_TAG,
        about: OG_ABOUT,
        later: &["Train War Magic at level 20."],
        heritage_note: "Any heritage suits it. Empyreans are born with Infused Life Magic.",
        source: OG_SOURCE,
        needs: "",
        variants: &[any(MAGE, &[CREATURE, LIFE], &[ITEM, MANA])],
    },
    Preset {
        id: "claw",
        name: "CLaW Mage",
        tag: CLAW_TAG,
        about: CLAW_ABOUT,
        later: &[
            "Train Life Magic at level 12.",
            "Specialize Life Magic at level 32.",
            "Specialize Creature Enchantment at level 70.",
        ],
        heritage_note: JOAT,
        source: LR,
        needs: "",
        variants: &[any(MAGE, &[WAR], &[CREATURE, ITEM, MANA])],
    },
    Preset {
        id: "opal",
        name: "Opal Tank Mage",
        tag: "A war mage with specialized buffs who can take a beating.",
        about: "Creature Enchantment and War Magic are both specialized, so your buffs and \
                attacks are strong together. Maxed Endurance gives you the Health to hold your \
                ground. Item Enchantment and Life Magic come with level credits.",
        later: &[
            "Train Item Enchantment at level 7.",
            "Train Life Magic at level 32.",
        ],
        heritage_note: JOAT,
        source: LR,
        needs: "",
        variants: &[any(MAGE, &[CREATURE, WAR], &[MANA])],
    },
    Preset {
        id: "battle",
        name: "Battle Mage",
        tag: BATTLE_TAG,
        about: BATTLE_ABOUT,
        later: &[
            "Train Mana Conversion at level 3.",
            "Train Item Enchantment at level 12.",
            "Train Creature Enchantment at level 32.",
        ],
        heritage_note: JOAT,
        source: LR,
        needs: "",
        variants: &[any(MAGE, &[WAR, LIFE], &[])],
    },
    Preset {
        id: "martyr",
        name: "Martyr Mage",
        tag: "A Life mage whose damage grows with your Health.",
        about: "Life Magic is specialized for the Hecatomb spells, which hit harder the more \
                Health you have. Endurance is maxed to give you that Health. Healing is \
                specialized, and Creature, Item and Mana Conversion round out a support caster.",
        later: &[
            "Specialize Mana Conversion at level 7.",
            "Specialize Creature Enchantment at level 20.",
            "Train Leadership at level 32 for Horns of Leadership.",
        ],
        heritage_note: "Any heritage suits it. Empyreans are born with Infused Life Magic.",
        source: LR,
        needs: "",
        variants: &[any(MAGE, &[LIFE, HEALING], &[CREATURE, ITEM, MANA])],
    },
    Preset {
        id: "melee",
        name: "Melee Mage",
        tag: MELEE_TAG,
        about: MELEE_ABOUT,
        later: &[
            "Train Life Magic at level 5.",
            "Train Item Enchantment at level 16.",
            "Train Creature Enchantment at level 40.",
            "Specialize Melee Defense at level 90.",
        ],
        heritage_note: JOAT,
        source: LR,
        needs: "",
        variants: &[any([10, 30, 10, 80, 100, 100], &[WAR], &[MANA, MELEE_D])],
    },
    Preset {
        id: "summon",
        name: "Summoner",
        tag: "A Life mage who fights beside summoned creatures.",
        about: "Life Magic and Summoning are specialized. Coordination is maxed for Melee \
                Defense, and Focus and Self power your spells. Summoning also draws on Endurance, \
                so luminance auras help it grow.",
        later: &[
            "Train Melee Defense at level 5.",
            "Train Item Enchantment at level 16.",
            "Train Void Magic at level 80.",
        ],
        heritage_note: JOAT,
        source: "The Loresraat Tugaker Mage. Its page lists only finished skills, so these \
                 creation skills are a proposal.",
        needs: "",
        variants: &[any(
            [10, 10, 100, 10, 100, 100],
            &[LIFE, SUMMONING],
            &[CREATURE, MANA],
        )],
    },
    Preset {
        id: "cov",
        name: "Covenant Warrior",
        tag: "A warrior who needs no magic.",
        about: "Heavy Weapons and Melee Defense are specialized. Arcane Lore handles buffs from \
                items, and Assess Creature lets you use Gearcrafted Lenses. Alchemy makes the \
                throwing phials that weaken enemies, and the three defenses let you wear Covenant \
                or Olthoi armor.",
        later: &[
            "Specialize Missile Defense at level 12.",
            "Specialize Magic Defense at level 50.",
        ],
        heritage_note: CRIT,
        source: LR,
        needs: "",
        variants: &[any(
            [100, 10, 100, 10, 100, 10],
            &[HEAVY, MELEE_D, ARCANE, ASSESS_CREATURE],
            &[HEALING, ALCHEMY],
        )],
    },
    Preset {
        id: "cov2h",
        name: "Covenant Warrior, Two Handed",
        tag: "The same warrior with a two-handed weapon.",
        about: "Two Handed Combat is specialized with Melee Defense, Arcane Lore and Assess \
                Creature. Alchemy follows quickly with the credits you keep. The rest of the plan \
                matches the Covenant Warrior.",
        later: &[
            "Train Alchemy at level 5.",
            "Specialize Missile Defense at level 20.",
            "Specialize Magic Defense at level 70.",
        ],
        heritage_note: CRIT,
        source: "Built from the Loresraat Covenant Warrior.",
        needs: "",
        variants: &[any(
            [100, 10, 100, 10, 100, 10],
            &[TWO_HANDED, MELEE_D, ARCANE, ASSESS_CREATURE],
            &[HEALING],
        )],
    },
    Preset {
        id: "alch",
        name: "Combat Alchemist",
        tag: "A thrower who fights with alchemical phials.",
        about: "Missile Weapons throws the phials, and Light Weapons covers fist fights while you \
                gather supplies. Arcane Lore and Melee Defense are specialized. Fletching and \
                Alchemy make your ammunition.",
        later: &[
            "Train Alchemy at level 7.",
            "Train Item Enchantment at level 20.",
            "Train Mana Conversion at level 40.",
            "Specialize Alchemy at level 70.",
        ],
        heritage_note: CRIT,
        source: "The Loresraat Combat Alchemist, with Missile and Light Weapons for this \
                 era\u{2019}s skills.",
        needs: "",
        variants: &[any(
            [100, 10, 100, 100, 10, 10],
            &[ARCANE, MELEE_D, MISSILE, LIGHT],
            &[HEALING, FLETCHING],
        )],
    },
    Preset {
        id: "pvm",
        name: "PvM Archer",
        tag: "An archer with a finesse weapon for close fights.",
        about: "Missile Weapons and Finesse Weapons are specialized and both draw on maxed \
                Coordination and Quickness. Fletching, Healing and Melee Defense keep you \
                supplied and standing. The credits you keep buy Life Magic early.",
        later: &[
            "Train Life Magic at level 7.",
            "Train Creature Enchantment at level 20.",
            "Train Item Enchantment at level 50.",
        ],
        heritage_note: CRIT,
        source: LR,
        needs: "",
        variants: &[any(
            [10, 10, 100, 100, 100, 10],
            &[MISSILE, FINESSE],
            &[MANA, MELEE_D, FLETCHING, HEALING],
        )],
    },
    Preset {
        id: "sarch",
        name: "Summoning Archer",
        tag: "An archer whose summons draw the enemy away.",
        about: "Missile Weapons, Summoning and Melee Defense are specialized. Recklessness adds \
                damage while enemies chase your summons. Fletching keeps you in arrows early on.",
        later: &[
            "Specialize Recklessness at level 3.",
            "Train Healing at level 9.",
            "Train Life Magic at level 40.",
            "Specialize Magic Defense at level 100.",
        ],
        heritage_note: CRIT,
        source: "The Loresraat Summoning Archer. Its page lists only finished skills, so these \
                 creation skills are a proposal.",
        needs: "",
        variants: &[any(
            [10, 100, 100, 100, 10, 10],
            &[MISSILE, SUMMONING, MELEE_D],
            &[RECKLESSNESS, FLETCHING],
        )],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn copy(p: &Preset) -> Vec<&'static str> {
        let mut out = vec![p.name, p.tag, p.about, p.heritage_note, p.source, p.needs];
        out.extend(p.later);
        out.extend(p.variants.iter().flat_map(|v| v.later));
        out
    }

    #[test]
    fn the_copy_has_no_dashes_and_every_variant_spends_its_attributes() {
        for era in EraId::ALL {
            for p in presets(era) {
                for line in copy(p) {
                    assert!(!line.contains(['\u{2014}', '\u{2013}']), "{}: {line}", p.id);
                    assert!(!line.contains("  "), "{}: {line}", p.id);
                }
                for v in p.variants {
                    assert_eq!(v.attributes.iter().sum::<u32>(), 330, "{:?} {}", era, p.id);
                }
                let all_old = [ALUVIAN, GHARUNDIM, SHO]
                    .iter()
                    .all(|h| p.variant(*h).is_some());
                assert_eq!(p.needs.is_empty(), all_old, "{}", p.id);
            }
        }
    }

    #[test]
    fn a_variant_is_found_for_its_heritages_and_the_level_plan_falls_back_to_the_presets() {
        let hf = INFILTRATION.iter().find(|p| p.id == "hfight").unwrap();
        assert!(hf.variant(ALUVIAN).is_none());
        let sho = hf.variant(SHO).unwrap();
        assert_eq!(sho.specialized, &[UNARMED, MELEE_D]);
        assert_eq!(
            hf.later_for(sho)[0],
            "Train Creature Enchantment at level 7."
        );
        assert_eq!(hf.heritages(), vec![SHO, GHARUNDIM]);
        let og = &INFILTRATION[0];
        assert_eq!(og.later_for(og.variant(ALUVIAN).unwrap()), og.later);
    }
}
