//! Each 1999 body is written field by field as AC1999's server writes it, translated, then read
//! back with Pyreal's own later reader.

use pyreal_primitives::{NetQueue, ObjectId};
use pyreal_protocol::combat::{
    AttackerNotification, CombatHandlePlayerDeathEvent, DefenderNotification,
    EvasionDefenderNotification, VictimNotificationSelf,
};
use pyreal_protocol::comms::{
    CommunicationChannelBroadcastRecv, CommunicationHearDirectSpeech, CommunicationHearEmote,
    CommunicationHearSpeech, CommunicationSetSquelchDb, CommunicationTextboxString,
    CommunicationTransientString, CommunicationWeenieError,
};
use pyreal_protocol::login::LoginPlayerDescription;
use pyreal_protocol::objects::ItemSetAppraiseInfo;
use pyreal_protocol::qualities::{
    MagicDispelMultipleEnchantments, MagicRemoveEnchantment, MagicUpdateEnchantment,
    MagicUpdateSpell, QualitiesPrivateUpdateInt,
};
use pyreal_protocol::social::{AllegianceUpdate, FellowshipFullUpdate, FellowshipUpdateFellow};
use pyreal_protocol::trade::{
    BookPageDataResponse, MiscPortalStormBrewing, VendorInfo, WritingBookOpen,
};
use pyreal_protocol::types::qualities::{Enchantment, StatMod};
use pyreal_protocol::types::PublicWeenieDesc;
use pyreal_protocol::{read_body, Message, Reader, Writer};

use super::codec::write_string;
use super::{EventState, Later};
use crate::translate::Shared;

const ME: u32 = 0x5000_0001;

fn msg(opcode: u32, f: impl FnOnce(&mut Writer)) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(opcode);
    f(&mut w);
    w.into_inner()
}

/// The module with the shared state the link would give it.
#[derive(Default)]
struct T {
    s: EventState,
    shared: Shared,
}

impl T {
    fn translate(&mut self, m: &[u8]) -> Option<Vec<Later>> {
        self.s.translate(m, &mut self.shared)
    }

    fn translate_wrapped(&mut self, m: &[u8]) -> Option<Vec<Later>> {
        self.s.translate_wrapped(m, &mut self.shared)
    }

    fn set_player(&mut self, id: u32) {
        self.shared.stamps.set_player(id);
    }

    fn tally(&self) -> &super::Tally {
        self.s.tally()
    }
}

fn one(s: &mut T, m: &[u8]) -> Later {
    let mut out = s.translate(m).expect("this area's");
    assert_eq!(
        out.len(),
        1,
        "{:#06X}",
        u32::from_le_bytes(m[..4].try_into().unwrap())
    );
    out.remove(0)
}

fn later<M: Message>(l: &Later) -> M {
    assert_eq!(l.opcode, M::OPCODE.0);
    read_body(&l.body).unwrap()
}

fn state() -> T {
    let mut s = T::default();
    s.set_player(ME);
    s
}

#[test]
fn speech_tells_and_emotes_take_their_strings_and_chat_types() {
    let mut s = state();
    let speech = msg(0x0037, |w| {
        write_string(w, "Hello there");
        write_string(w, "Ann");
        w.u32(0x5000_0002);
    });
    let m: CommunicationHearSpeech = later(&one(&mut s, &speech));
    assert_eq!(
        (m.message.as_str(), m.sender_name.as_str()),
        ("Hello there", "Ann")
    );
    assert_eq!((m.sender_id.0, m.text_type), (0x5000_0002, 2));

    let tell = msg(0x0038, |w| {
        write_string(w, "psst");
        write_string(w, "Bo");
        w.u32(0x5000_0003);
        w.u32(ME);
    });
    let m: CommunicationHearDirectSpeech = later(&one(&mut s, &tell));
    assert_eq!((m.target_id.0, m.text_type, m.secret_flags), (ME, 3, 0));

    let emote = msg(0x01E0, |w| {
        w.u32(0x5000_0002);
        write_string(w, "Ann");
        write_string(w, "waves.");
    });
    let m: CommunicationHearEmote = later(&one(&mut s, &emote));
    assert_eq!((m.sender.0, m.text.as_str()), (0x5000_0002, "waves."));
}

#[test]
fn a_ranged_line_reads_its_name_first() {
    let m = msg(0x01B5, |w| {
        write_string(w, "Town Crier");
        write_string(w, "Hear ye");
        w.u32(7);
        w.f32(60.0);
    });
    let l = one(&mut state(), &m);
    let r: pyreal_protocol::comms::CommunicationHearRangedSpeech = later(&l);
    assert_eq!(
        (r.sender_name.as_str(), r.message.as_str()),
        ("Town Crier", "Hear ye")
    );
    assert_eq!(r.range, 60.0);
}

#[test]
fn a_channel_echo_with_no_name_skips_its_trailing_word() {
    let mut s = state();
    let echo = msg(0x014A, |w| {
        w.u32(0x0800);
        write_string(w, "");
        write_string(w, "hi all");
        w.u32(0);
    });
    let m: CommunicationChannelBroadcastRecv = later(&one(&mut s, &echo));
    assert_eq!(
        (m.channel, m.sender_name.as_str(), m.message.as_str()),
        (0x0800, "", "hi all")
    );
    let named = msg(0x014A, |w| {
        w.u32(0x0800);
        write_string(w, "Ann");
        write_string(w, "hi");
    });
    let m: CommunicationChannelBroadcastRecv = later(&one(&mut s, &named));
    assert_eq!(m.sender_name, "Ann");
}

#[test]
fn system_text_status_and_errors_reach_chat() {
    let mut s = state();
    let pop = msg(0xF62C, |w| {
        write_string(w, "Current world population: 3");
        w.u32(7);
    });
    let m: CommunicationTextboxString = later(&one(&mut s, &pop));
    assert_eq!(
        (m.text.as_str(), m.text_type),
        ("Current world population: 3", 7)
    );

    let mut w = Writer::new();
    w.u32(0x0016);
    w.pstring("Out of Range!").unwrap();
    let out = s.translate(&w.into_inner()).unwrap();
    let strip: CommunicationTransientString = later(&out[0]);
    let chat: CommunicationTextboxString = later(&out[1]);
    assert_eq!(
        (strip.text.as_str(), chat.text.as_str()),
        ("Out of Range!", "Out of Range!")
    );
    assert_eq!(chat.text_type, 0);

    let e: CommunicationWeenieError = later(&one(&mut s, &msg(0x001C, |w| w.u32(0x1D))));
    assert_eq!(e.error_type, 0x1D);
    assert!(out[0].is_game_event() && !out[1].is_game_event());
}

#[test]
fn squelches_build_the_whole_database_and_note_the_ids_for_the_upload() {
    let mut s = state();
    assert!(s.shared.squelched.is_empty());
    let add = |name: &str, id: u32| {
        msg(0x005A, |w| {
            write_string(w, name);
            w.u32(id);
        })
    };
    one(&mut s, &add("Ann", 0x5000_0002));
    let db: CommunicationSetSquelchDb = later(&one(&mut s, &add("Bo", 0x5000_0003)));
    let names: Vec<_> =
        db.0.character_hash
            .entries
            .iter()
            .map(|(_, i)| i.name.clone())
            .collect();
    assert_eq!(names, ["Ann", "Bo"]);
    assert!(db.0.character_hash.entries[0]
        .1
        .squelch_msgs
        .is_squelched(1));
    assert!(db.0.account_hash.entries.is_empty());

    let remove = msg(0x005B, |w| {
        write_string(w, "Ann");
        w.u32(0x5000_0002);
    });
    let db: CommunicationSetSquelchDb = later(&one(&mut s, &remove));
    assert_eq!(db.0.character_hash.entries.len(), 1);
    assert_eq!(s.shared.squelched, [0x5000_0003]);
    s.shared.stamps.enter_world();
    let out = s.translate(&description(0)).unwrap();
    assert_eq!(out.len(), 2, "the list follows the next description");
    let db: CommunicationSetSquelchDb = later(&out[1]);
    assert_eq!(db.0.character_hash.entries[0].1.name, "Bo");
}

/// A description with no tables and the option word `options`.
fn description(options: u32) -> Vec<u8> {
    msg(0x0013, |w| {
        for v in [0, 0x0A, 0, 1, 0, options, 0, 0, 0] {
            w.u32(v);
        }
    })
}

#[test]
fn combat_notifications_keep_their_fields_with_later_strings() {
    let mut s = state();
    let hit = msg(0x01B1, |w| {
        write_string(w, "Drudge Skulker");
        w.u32(4);
        w.f64(0.25);
        w.u32(12);
        w.u32(0);
        w.u32(0);
        w.u32(0);
    });
    let m: AttackerNotification = later(&one(&mut s, &hit));
    assert_eq!(
        (m.defender_name.as_str(), m.percent, m.damage),
        ("Drudge Skulker", 0.25, 12)
    );

    let hurt = msg(0x01B2, |w| {
        write_string(w, "Drudge Skulker");
        w.u32(1);
        w.f64(0.1);
        w.u32(5);
        w.u32(2);
        w.u32(0);
        w.u32(0);
        w.u32(0);
    });
    let m: DefenderNotification = later(&one(&mut s, &hurt));
    assert_eq!((m.damage, m.damage_location), (5, 2));

    let evade = msg(0x01B4, |w| write_string(w, "Mite"));
    let m: EvasionDefenderNotification = later(&one(&mut s, &evade));
    assert_eq!(m.attacker_name, "Mite");
    let died = msg(0x01AC, |w| write_string(w, "You have died."));
    let m: VictimNotificationSelf = later(&one(&mut s, &died));
    assert_eq!(m.message, "You have died.");
    let death = msg(0x019E, |w| {
        write_string(w, "Ann was killed.");
        w.u32(ME);
        w.u32(7);
    });
    let m: CombatHandlePlayerDeathEvent = later(&one(&mut s, &death));
    assert_eq!((m.killed.0, m.killer.0), (ME, 7));
    let done = msg(0x01A7, |w| w.u32(0));
    assert_eq!(one(&mut s, &done), Later::same(0x01A7, &done));
}

fn enchantment(id: u32) -> Enchantment {
    Enchantment {
        id,
        category_word: 0x20,
        power_level: 100,
        start_time: 0.0,
        duration: 600.0,
        caster: ObjectId(ME),
        degrade_modifier: 0.0,
        degrade_limit: -666.0,
        last_time_degraded: 0.0,
        smod: StatMod {
            kind: 0x8011,
            key: 1,
            value: 10.0,
        },
        spell_set_id: None,
    }
}

#[test]
fn spells_keep_their_ids_and_enchantments_gain_layer_one() {
    let mut s = state();
    let m: MagicUpdateSpell = later(&one(&mut s, &msg(0x004C, |w| w.u32(0x2B))));
    assert_eq!(m.layered_spell_id, 0x2B);
    let add = msg(0x004E, |w| enchantment(0x2B).write(w).unwrap());
    let m: MagicUpdateEnchantment = later(&one(&mut s, &add));
    assert_eq!(m.0.id, 0x0001_002B);
    let item = msg(0x004E, |w| enchantment(0x0103_002B).write(w).unwrap());
    let m: MagicUpdateEnchantment = later(&one(&mut s, &item));
    assert_eq!(m.0.id, 0x0103_002B, "an item's slot bytes stay");
    let m: MagicRemoveEnchantment = later(&one(&mut s, &msg(0x004F, |w| w.u32(0x2B))));
    assert_eq!(m.layered_spell_id, 0x0001_002B);
    let several = msg(0x01AE, |w| {
        w.u32(2);
        w.u32(0x2B);
        w.u32(0x2C);
    });
    let m: MagicDispelMultipleEnchantments = later(&one(&mut s, &several));
    assert_eq!(m.0, [0x0001_002B, 0x0001_002C]);
}

/// A sword's and a creature's appraisal in AC1999's release section order.
#[test]
fn an_appraisal_lands_its_sections_in_the_later_tables() {
    let mut s = state();
    let creature = msg(0x00C9, |w| {
        w.u32(0x5000_0002);
        w.u32(0x1);
        w.u32(1);
        w.u32(0xF);
        w.u32(12);
        w.u32(40);
        w.u32(50);
        for v in 1..=10 {
            w.u32(v);
        }
        w.u32(31);
        for v in [2, 3, 0, 0, 0] {
            w.u32(v);
        }
        for t in ["Female", "Aluvian", "Adventurer", "", "Bo"] {
            write_string(w, t);
        }
        for v in 1..=6 {
            w.u32(v);
        }
        w.u32(0x10);
    });
    let a: ItemSetAppraiseInfo = later(&one(&mut s, &creature));
    let c = a.profile.creature_profile.unwrap();
    assert_eq!((c.health, c.max_health, c.flags), (40, 50, 0x9));
    assert_eq!(c.attributes.unwrap()[9], 10);
    assert_eq!(c.enchantment_bitfield, Some(0x10));
    let ints = a.profile.tables.ints.unwrap().entries;
    assert_eq!(ints, [(25, 12), (2, 31), (30, 2), (35, 3)]);
    let strings = a.profile.tables.strings.unwrap().entries;
    assert_eq!(strings[2], (5, "Adventurer".to_owned()));

    let sword = msg(0x00C9, |w| {
        w.u32(0x8000_0010);
        w.u32(0x2 | 0x8 | 0x800 | 0x10 | 0x1_0000 | 0x20_0000 | 0x80_0000);
        w.u32(1);
        w.u32(1);
        w.i32(40);
        w.u32(0x2D);
        w.i32(9);
        w.f64(0.5);
        w.f64(1.0);
        w.f64(0.0);
        w.f64(1.05);
        w.f64(1.1);
        w.u32(80);
        w.u32(u32::MAX);
        w.u32(100);
        w.u32(1);
        write_string(w, "For Ann");
        write_string(w, "Bo");
        write_string(w, "bo");
        w.u32(u32::MAX);
        w.u32(u32::MAX);
        write_string(w, " ");
        w.u32(0x8);
        for v in [5, 150, u32::MAX, 300, 600, u32::MAX, 0] {
            w.u32(v);
        }
        write_string(w, "");
        w.u32(u32::MAX);
        w.u32(0);
        write_string(w, "Strength Self I");
        write_string(w, "");
        w.f64(-1.0);
    });
    let a: ItemSetAppraiseInfo = later(&one(&mut s, &sword));
    let p = a.profile;
    let weapon = p.weapon_profile.unwrap();
    assert_eq!(
        (
            weapon.weapon_damage,
            weapon.damage_mod,
            weapon.weapon_offense
        ),
        (9, 1.0, 1.1)
    );
    assert_eq!(p.tables.floats.unwrap().entries, [(29, 1.05)]);
    let ints = p.tables.ints.unwrap().entries;
    assert!(ints.contains(&(8, 80)) && ints.contains(&(5, 100)));
    assert!(!ints.iter().any(|(k, _)| [19, 86, 87, 110, 176].contains(k)));
    assert!(ints.contains(&(109, 5)) && ints.contains(&(108, 600)));
    assert_eq!(p.tables.bools.unwrap().entries, [(3, 1)]);
    let strings = p.tables.strings.unwrap().entries;
    assert!(strings.contains(&(8, "Bo".to_owned())));
    assert!(strings.contains(&(16, "Strength Self I".to_owned())));
    assert!(
        !strings.iter().any(|(k, _)| *k == 38),
        "a lone space is no destination"
    );
    assert_eq!(p.weapon_enchantment, Some(0x8));
}

#[test]
fn a_vendor_list_takes_the_later_defaults_and_reads_item_descriptions() {
    let mut s = state();
    let list = |described: bool| {
        msg(0x0062, |w| {
            w.u32(0x8000_0020);
            w.u32(0x7);
            w.i32(0);
            w.i32(1000);
            w.i32(1);
            w.f32(0.8);
            w.f32(1.2);
            w.u32(1);
            w.u32(if described { 0xFF00_0005 } else { 0x00FF_FFFF });
            w.u32(0x8000_0021);
            if described {
                w.u32(0xABCD);
            }
        })
    };
    let v: VendorInfo = later(&one(&mut s, &list(false)));
    assert_eq!((v.items[0].amount, v.profile.trade_id), (-1, 0));
    assert!(v.items[0].pwd.is_none());

    // One word is no 1999 description, so the default reader refuses it.
    assert_eq!(s.translate(&list(true)), Some(Vec::new()));
    assert_eq!(s.tally().get(0x0062).malformed, 1);
    s.s.set_description_reader(|r: &mut Reader<'_>| {
        r.u32()?;
        Ok(PublicWeenieDesc::default())
    });
    let v: VendorInfo = later(&one(&mut s, &list(true)));
    assert_eq!((v.items[0].amount, v.items[0].pwd.is_some()), (5, true));
}

#[test]
fn fellowships_take_launch_defaults() {
    let mut s = state();
    let full = msg(0x00AF, |w| {
        w.u32(0x0010_0001);
        w.u32(ME);
        w.u16(500);
        w.u16(12);
        write_string(w, "Ann");
        write_string(w, "Friends");
        w.u32(ME);
    });
    let f: FellowshipFullUpdate = later(&one(&mut s, &full));
    let (id, fellow) = &f.0.members.entries[0];
    assert_eq!(
        (*id, fellow.level, fellow.cp_cache, fellow.name.as_str()),
        (ME, 12, 500, "Ann")
    );
    assert_eq!(
        (f.0.name.as_str(), f.0.share_xp, f.0.leader.0),
        ("Friends", 1, ME)
    );

    let fellow = msg(0x00B0, |w| {
        w.u32(ME);
        w.u16(1);
        w.u16(2);
        write_string(w, "Ann");
    });
    let u: FellowshipUpdateFellow = later(&one(&mut s, &fellow));
    assert_eq!((u.fellow.level, u.update_type), (2, 1));

    for (old, new) in [(0x00A7, 0x00A3), (0x00B1, 0x00A4), (0x00B3, 0x02BF)] {
        let m = msg(old, |w| w.u32(ME));
        assert_eq!(
            one(&mut s, &m),
            Later {
                opcode: new,
                body: m[4..].to_vec()
            }
        );
    }
}

#[test]
fn an_allegiance_reads_as_the_later_version_zero_hierarchy() {
    let member = |w: &mut Writer, id: u32, name: &str| {
        w.u32(id);
        w.u32(100);
        w.u32(10);
        w.u32(0x1);
        w.u8(2);
        w.u8(0);
        w.u16(3);
        w.u16(40);
        w.u16(50);
        w.f64(3600.9);
        write_string(w, name);
    };
    let m = msg(0x0020, |w| {
        w.u32(2);
        w.u32(1);
        w.u32(2);
        member(w, ME, "Ann");
        w.u32(ME);
        member(w, 0x5000_0002, "Bo");
    });
    let a: AllegianceUpdate = later(&one(&mut state(), &m));
    let h = &a.profile.hierarchy;
    assert_eq!((a.rank, h.version, a.profile.total_members), (0, 0, 2));
    assert_eq!(h.members[1].0, Some(ObjectId(ME)));
    let bo = &h.members[1].1;
    assert_eq!(
        (bo.name.as_str(), bo.rank, bo.leadership, bo.time_online),
        ("Bo", 3, 50, 3600)
    );
    assert!(bo.is_logged_in());
}

fn page(w: &mut Writer, text: Option<&str>) {
    w.u32(ME);
    write_string(w, "Ann");
    write_string(w, "ann");
    w.i32(i32::from(text.is_some()));
    if let Some(t) = text {
        write_string(w, t);
    }
}

#[test]
fn books_read_their_unversioned_pages_with_or_without_a_page_list() {
    let mut s = state();
    let open = msg(0x00B4, |w| {
        w.u32(0x8000_0030);
        w.u32(10);
        w.i32(10);
        w.i32(1000);
        w.i32(2);
        page(w, Some("Dear reader,"));
        page(w, None);
        write_string(w, "");
        w.u32(0);
        write_string(w, "");
    });
    let b: WritingBookOpen = later(&one(&mut s, &open));
    let pages = &b.pages.pages;
    assert_eq!(pages[0].page_text.as_deref(), Some("Dear reader,"));
    assert_eq!((pages[0].version, pages[1].text_included), (None, 0));

    let bare = msg(0x00B4, |w| {
        w.u32(0x8000_0031);
        w.u32(0);
        write_string(w, "Ann's");
        w.u32(ME);
        write_string(w, "Ann");
    });
    let b: WritingBookOpen = later(&one(&mut s, &bare));
    assert!(b.pages.pages.is_empty());
    assert_eq!(
        (b.inscription.as_str(), b.scribe_name.as_str()),
        ("Ann's", "Ann")
    );

    let one_page = msg(0x00B8, |w| {
        w.u32(0x8000_0030);
        w.u32(0);
        page(w, Some("x"));
    });
    let p: BookPageDataResponse = later(&one(&mut s, &one_page));
    assert_eq!(p.data.page_text.as_deref(), Some("x"));
}

#[test]
fn portal_storm_warnings_carry_an_extent_and_the_storm_drops_its_text() {
    let mut s = state();
    let m: MiscPortalStormBrewing = later(&one(&mut s, &msg(0x009D, |w| w.u32(0))));
    assert!(m.extent > 0.0);
    let storm = one(&mut s, &msg(0x009F, |w| write_string(w, "A portal storm!")));
    assert_eq!((storm.opcode, storm.body.len()), (0x02CB, 0));
    assert_eq!(one(&mut s, &msg(0x009C, |_| {})).opcode, 0x02CC);
}

#[test]
fn the_description_sets_the_option_word_and_quality_updates_follow_the_player() {
    let mut s = T::default();
    let d = description(0x0402);
    let l: LoginPlayerDescription = later(&one(&mut s, &d));
    assert_eq!(l.player_module.options, 0x0402);
    assert_eq!(s.shared.options_word, Some(0x0402));

    let int = msg(0x000E, |w| {
        w.u32(ME);
        w.u32(25);
        w.i32(3);
    });
    assert_eq!(
        one(&mut s, &int).opcode,
        0x02CE,
        "public before the player is known"
    );
    s.set_player(ME);
    let p: QualitiesPrivateUpdateInt = later(&one(&mut s, &int));
    assert_eq!(p.0.sequence, 2, "one counter per object, whichever form");
    s.shared.stamps.enter_world();
    s.set_player(ME);
    let p: QualitiesPrivateUpdateInt = later(&one(&mut s, &int));
    assert_eq!(p.0.sequence, 1, "a world entry starts the counters again");
}

#[test]
fn a_wrapped_0029_is_a_title_table_and_dropped() {
    let mut s = state();
    let vital = msg(0x0029, |w| {
        for v in [ME, 2, 1, 2, 3, 4] {
            w.u32(v);
        }
    });
    assert_eq!(s.translate_wrapped(&vital), Some(Vec::new()));
    assert_eq!(s.translate(&vital).unwrap()[0].opcode, 0x02E9);
}

#[test]
fn questions_open_and_close_and_later_systems_are_dropped_and_counted() {
    let mut s = state();
    let q = msg(0x0274, |w| {
        w.i32(1);
        w.u32(9);
        w.pstring("Join?").unwrap();
    });
    assert_eq!(one(&mut s, &q).queue(), NetQueue::UiQueue);
    assert_eq!(s.shared.questions_owed, 1);

    for op in [0x002B, 0x0201, 0xF7DE] {
        assert_eq!(s.translate(&msg(op, |w| w.u32(0))), Some(Vec::new()));
        assert_eq!(s.tally().get(op).dropped, 1);
    }
    assert_eq!(s.translate(&msg(0x002D, |_| {})), None, "not this area's");
    assert_eq!(s.translate(&msg(0x0037, |w| w.u16(9))), Some(Vec::new()));
    assert_eq!(s.tally().get(0x0037).malformed, 1);
}

#[test]
fn inventory_and_session_messages_pass_with_their_numbers() {
    let mut s = state();
    for op in [
        0x0022, 0x0196, 0x01C7, 0x01C9, 0x01EA, 0xF653, 0x028B, 0x02EB,
    ] {
        let m = msg(op, |w| w.u32(5));
        assert_eq!(one(&mut s, &m), Later::same(op, &m));
    }
    let logoff = Later::same(0xF653, &msg(0xF653, |_| {}));
    assert!(!logoff.is_game_event());
}

#[test]
fn as_a_link_module_it_stamps_game_events_against_the_player_and_sends_the_rest_bare() {
    use crate::conn::Connection;
    use crate::login::Credential;
    use crate::translate::{Context, Translate, Wrapped};
    use std::collections::VecDeque;

    let mut conn = Connection::new(Credential::new("Demo", "secret").unwrap(), (102, 67));
    let mut inbox = VecDeque::new();
    let mut shared = Shared::default();
    shared.stamps.set_player(ME);
    let mut cx = Context {
        inbox: &mut inbox,
        conn: &mut conn,
        shared: &mut shared,
        wrapped: None,
    };
    let mut s = EventState::new();
    let int = msg(0x000E, |w| {
        w.u32(ME);
        w.u32(25);
        w.i32(3);
    });
    assert!(s.down(0x000E, 4, &int, &mut cx).unwrap());
    let logoff = msg(0xF653, |_| {});
    assert!(s.down(0xF653, 4, &logoff, &mut cx).unwrap());
    assert!(!s.down(0xF745, 4, &msg(0xF745, |_| {}), &mut cx).unwrap());
    let vital = msg(0x0029, |w| {
        for v in [ME, 2, 1, 2, 3, 4] {
            w.u32(v);
        }
    });
    cx.wrapped = Some(Wrapped {
        object: ME,
        stamp: 9,
    });
    assert!(
        s.down(0x0029, 4, &vital, &mut cx).unwrap(),
        "a wrapped title table"
    );
    cx.wrapped = None;
    assert!(s.down(0x0029, 4, &vital, &mut cx).unwrap());

    let word = |b: &[u8], i: usize| u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
    let got: Vec<_> = inbox
        .iter()
        .map(|m| {
            let head = (m.opcode == 0xF7B0)
                .then(|| (word(&m.body, 0), word(&m.body, 1), word(&m.body, 2)));
            (m.opcode, m.queue, head)
        })
        .collect();
    let ui = NetQueue::UiQueue;
    // Only the bare 0x0029 is a vital; the server's stamp 9 is not passed on.
    assert_eq!(
        got,
        [
            (0xF7B0, ui, Some((ME, 1, 0x02CD))),
            (0xF653, ui, None),
            (0xF7B0, ui, Some((ME, 2, 0x02E9))),
        ]
    );
    let first = &inbox[0];
    let inner: QualitiesPrivateUpdateInt = read_body(&first.body[12..]).unwrap();
    assert_eq!((inner.0.sequence, inner.0.value), (1, 3));
}
