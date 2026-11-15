use pyreal_client_contract::GameView as _;
use pyreal_protocol::types::qualities::{
    AcQualities, Attribute, AttributeCache, Enchantment as ProtocolEnchantment,
    EnchantmentRegistry, Skill as WireSkill, StatMod,
};
use pyreal_testkit::HeadlessClient;

use super::support::{a_client_with_a_peer, Peer, PLAYER};

/// The skill the penalty is read off.
const A_SKILL: u32 = 0x2C;
/// The character these scenarios build.
const BASE_ATTRIBUTE: u32 = 100;
const SKILL_RANKS: u32 = 50;
const SKILL_INIT: u32 = 5;
pub const LEVEL_OF_THE_CHARACTER: i32 = 40;
/// Experience already earned back against the current vitae point.
pub const CP_POOL: i32 = 1_000;
/// The character's level, and the pool, as the shard keys them.
const LEVEL: u32 = 25;
const VITAE_CP_POOL: u32 = 129;

/// The enchantment family a vitae belongs to, and the spell it is.
const VITAE_FAMILY: u32 = 0x0080_0000 | 0x0000_4000;
const SPELL_VITAE: u32 = 0x29A;
/// The family an ordinary beneficial skill buff belongs to.
const SKILL_BUFF: u32 = 0x0000_0010 | 0x0000_1000 | 0x0000_8000 | 0x0200_0000;

/// Four arrivals, in order: a death's penalty, then it wearing off.
pub const TICKS: [f32; 4] = [0.95, 0.96, 0.98, 1.00];

pub struct Vitae {
    c: HeadlessClient,
    peer: Peer,
}

impl Vitae {
    pub fn new() -> Self {
        let (c, peer) = a_client_with_a_peer();
        Self { c, peer }
    }

    /// Log the character in, with or without the vitae his death left him.
    pub fn describe(&mut self, carrying: Option<f32>) {
        let registry = carrying.map(|m| EnchantmentRegistry {
            flags: EnchantmentRegistry::VITAE,
            vitae: Some(one(m)),
            ..EnchantmentRegistry::default()
        });
        self.peer.event(&mut self.c, &a_character(registry));
        self.c.tick(8);
        assert!(
            self.c.view().expect_app().hud().player_desc_received,
            "the description reached the player-description arm"
        );
        assert!(
            self.c.view().expect_app().hud().skills.len() > 30,
            "the shipped skill table gives an entry per skill, or nothing below can look at one"
        );
    }

    /// One live arrival carrying a vitae.
    pub fn arrives(&mut self, multiplier: f32) {
        self.peer.event(
            &mut self.c,
            &pyreal_protocol::qualities::MagicUpdateEnchantment(one(multiplier)),
        );
        self.c.tick(8);
    }

    /// An ordinary beneficial buff on the same skill, on top of whatever is there.
    pub fn buff_the_skill(&mut self, value: f32) {
        self.peer.event(
            &mut self.c,
            &pyreal_protocol::qualities::MagicUpdateEnchantment(ProtocolEnchantment {
                id: 4624,
                category_word: 100,
                power_level: 8,
                start_time: 0.0,
                duration: 1800.0,
                caster: PLAYER,
                degrade_modifier: 0.0,
                degrade_limit: 0.0,
                last_time_degraded: 0.0,
                smod: StatMod {
                    kind: SKILL_BUFF,
                    key: A_SKILL,
                    value,
                },
                spell_set_id: None,
            }),
        );
        self.c.tick(8);
    }

    pub fn value(&self) -> Option<f32> {
        let app = self.c.view().expect_app();
        app.hud().view(app.objects()).vitae()
    }

    pub fn enchantment_counts(&self) -> (u32, u32) {
        let app = self.c.view().expect_app();
        app.hud().view(app.objects()).enchantment_counts()
    }

    pub fn registry_vitae(&self) -> f32 {
        self.c
            .view()
            .world()
            .player_qualities()
            .expect("the character's qualities")
            .enchantments
            .vitae_value()
    }

    /// What the skill's row shows: the enchanted value, and its colour -- plain (0), raised (1) or
    /// lowered (2) -- from the raw value against the enchanted one with only the vitae added back.
    pub fn skill_row(&self) -> (i32, u32) {
        let e = self.skill_entry();
        let font = match (e.effective - e.vitae).cmp(&e.level) {
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
            std::cmp::Ordering::Less => 2,
        };
        (e.effective, font)
    }

    /// The vitae inputs the client gathers: the multiplier, the pool and its threshold.
    pub fn display(&self) -> Option<pyreal_client_contract::VitaeDisplay> {
        let app = self.c.view().expect_app();
        app.hud().view(app.objects()).vitae_display()
    }

    /// The skill as the client works it out now, which is what the row and its footer read.
    pub fn skill_entry(&self) -> pyreal_client_contract::SkillAdvancement {
        let app = self.c.view().expect_app();
        app.hud()
            .view(app.objects())
            .skill_advancement(A_SKILL)
            .expect("the skill's entry")
    }

    pub fn strength(&self) -> Option<i32> {
        let app = self.c.view().expect_app();
        app.hud().view(app.objects()).attribute(1)
    }

    pub fn shutdown(self) {
        self.c.shutdown();
    }
}

/// One arrival carrying a vitae. A vitae is **permanent**, which is what keeps a purge off it,
/// and its value is the multiplier the skill path reads.
fn one(multiplier: f32) -> ProtocolEnchantment {
    ProtocolEnchantment {
        id: SPELL_VITAE,
        category_word: 0,
        power_level: 1,
        start_time: 0.0,
        duration: -1.0,
        caster: PLAYER,
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        smod: StatMod {
            kind: VITAE_FAMILY,
            key: 0,
            value: multiplier,
        },
        spell_set_id: None,
    }
}

fn attribute(v: u32) -> Attribute {
    Attribute {
        level_from_cp: 0,
        init_level: v,
        cp_spent: 0,
    }
}

fn specialised(ranks: u32) -> WireSkill {
    WireSkill {
        level_from_pp: u16::try_from(ranks).expect("the rank count is small"),
        format_version: 1,
        sac: 3,
        pp: 0,
        init_level: SKILL_INIT,
        resistance_of_last_check: 0,
        last_used_time: 0.0,
    }
}

/// The character, with `registry` for his enchantments.
///
/// The description deliberately carries the character's level and **not** the level he died
/// at, so the threshold's fall-back from one to the other is the branch these scenarios take.
fn a_character(
    registry: Option<EnchantmentRegistry>,
) -> pyreal_protocol::login::LoginPlayerDescription {
    use pyreal_protocol::archive::PackedHash;
    use pyreal_protocol::types::qualities::{
        attribute_cache_mask as m, base_flags, quality_flags, AcBaseQualities, PropertyTables,
    };
    let cache = AttributeCache {
        flags: m::STRENGTH | m::COORDINATION,
        strength: Some(attribute(BASE_ATTRIBUTE)),
        coordination: Some(attribute(BASE_ATTRIBUTE)),
        ..AttributeCache::default()
    };
    let mut flags = quality_flags::ATTRIBUTE_CACHE | quality_flags::SKILLS;
    if registry.is_some() {
        flags |= quality_flags::ENCHANTMENT_REGISTRY;
    }
    pyreal_protocol::login::LoginPlayerDescription {
        qualities: AcQualities {
            base: AcBaseQualities {
                flags: base_flags::INT,
                weenie_type: 0x0A,
                tables: PropertyTables {
                    ints: Some(PackedHash {
                        table_size: 8,
                        entries: vec![(LEVEL, LEVEL_OF_THE_CHARACTER), (VITAE_CP_POOL, CP_POOL)],
                    }),
                    ..PropertyTables::default()
                },
            },
            flags,
            attribute_cache: Some(cache),
            skills: Some(PackedHash {
                table_size: 8,
                entries: vec![(A_SKILL, specialised(SKILL_RANKS))],
            }),
            enchantments: registry,
            ..AcQualities::default()
        },
        player_module: pyreal_protocol::login::PlayerModule {
            spell_bars: vec![Vec::new()],
            spell_filters: pyreal_protocol::login::PlayerModule::DEFAULT_SPELL_FILTERS,
            options2: pyreal_protocol::login::PlayerModule::DEFAULT_OPTIONS2,
            ..pyreal_protocol::login::PlayerModule::default()
        },
        content_profiles: Vec::new(),
        inventory_placements: Vec::new(),
    }
}
