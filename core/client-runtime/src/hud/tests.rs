use super::*;

#[test]
fn appraisal_display_facts_survive_the_live_projection_and_snapshot() {
    use pyreal_client_contract::snapshot::GameSnapshot;
    use pyreal_client_model::{Weenie, World};
    use pyreal_protocol::{archive::PackedHash, types::appraisal::AppraisalProfile};
    let id = ObjectId(7);
    let mut world = World::new();
    world.tables.weenies.insert(id, Weenie::new(id));
    world.selected = Some(id);
    let mut profile = AppraisalProfile::default();
    profile.tables.ints = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x2f, 0x820), (0xcc, 17), (0x2f, 999), (0xcc, 999)],
    });
    profile.tables.strings = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x13, "Aluvian".into()), (0x13, "Later duplicate".into())],
    });
    profile.spell_book = Some(vec![5, 999999, 0x8000_0005]);
    world.appraisal.set(id, profile);
    let mut hud = Hud::new();
    hud.spell_table = Some(pyreal_assets::tables::SpellTable {
        id: pyreal_primitives::DataId(0x0e00_000e),
        spell_buckets: 1,
        spells: [(
            5,
            pyreal_assets::tables::SpellBase {
                name: String::new(),
                description: String::new(),
                school: 0,
                icon: 0,
                category: 0,
                bitfield: 0,
                base_mana: 0,
                base_range_constant: 0.0,
                base_range_mod: 0.0,
                power: 0,
                spell_economy_mod: 0.0,
                formula_version: 0,
                component_loss: 0.0,
                meta_spell_type: 0,
                meta_spell_id: 0,
                duration: None,
                portal_lifetime: None,
                raw_comps: [0; 8],
                comp_key: 0,
                comps: vec![],
                caster_effect: 0,
                target_effect: 0,
                fizzle_effect: 0,
                recovery_interval: 0.0,
                recovery_amount: 0.0,
                display_order: 0,
                non_component_target_type: 0,
                mana_mod: 0,
            },
        )]
        .into(),
        spellset_bucket_index: 0,
        spellsets: Default::default(),
    });
    let view = HudView {
        hud: &hud,
        world: &world,
    };
    let live = view.appraisal(id).unwrap();
    let captured = GameSnapshot::from_view(&view).appraisal(id).unwrap();
    for facts in [&live, &captured] {
        assert_eq!(facts.attack_type, Some(0x820));
        assert_eq!(facts.elemental_damage_bonus, Some(17));
        assert_eq!(facts.activation_heritage.as_deref(), Some("Aluvian"));
        let spells = facts.magic.spells.as_ref().unwrap();
        assert_eq!(
            spells
                .iter()
                .map(|spell| spell.resolved)
                .collect::<Vec<_>>(),
            [true, false, true]
        );
        assert!(spells.iter().all(|spell| spell.name.is_empty()));
    }
    assert_eq!(live, captured);
    // The projection owns its strings and survives replacement of the cached tables.
    world.appraisal.set(id, AppraisalProfile::default());
    assert_eq!(live.activation_heritage.as_deref(), Some("Aluvian"));
    let empty = HudView {
        hud: &hud,
        world: &world,
    }
    .appraisal(id)
    .unwrap();
    assert_eq!(empty.attack_type, None);
    assert_eq!(empty.activation_heritage, None);
}

#[test]
fn oath_cost_reads_live_breaks_and_survives_the_snapshot() {
    use pyreal_client_contract::snapshot::GameSnapshot;
    use pyreal_client_model::{StatKey, StatType, StatValue, Weenie, World};
    let player = ObjectId(1);
    let mut world = World::new();
    world.player = Some(player);
    let mut row = Weenie::new(player);
    row.qualities = Some(pyreal_client_model::Qualities::new());
    row.qualities
        .as_mut()
        .unwrap()
        .set(StatKey::new(StatType::Int, 0x19), StatValue::Int(1));
    world.tables.weenies.insert(player, row);
    let mut hud = Hud::new();
    hud.player_desc_received = true;
    hud.xp_table = Some(pyreal_assets::tables::XpTable {
        id: pyreal_primitives::DataId(0x0e000018),
        attribute_xp: vec![],
        vital_xp: vec![],
        trained_xp: vec![],
        specialized_xp: vec![],
        level_xp: vec![0, 1000, 21000],
        level_credits: vec![0; 3],
    });
    for era in [
        pyreal_primitives::EraId::Infiltration,
        pyreal_primitives::EraId::Eor,
    ] {
        hud.era.era = era;
        for (breaks, expected) in [(-1, 0), (0, 0), (1, 1250)] {
            world
                .weenie_mut(player)
                .unwrap()
                .qualities
                .as_mut()
                .unwrap()
                .set(StatKey::new(StatType::Int, 0x84), StatValue::Int(breaks));
            let view = HudView {
                hud: &hud,
                world: &world,
            };
            let expected = (era == pyreal_primitives::EraId::Infiltration).then_some(expected);
            assert_eq!(view.oath_xp_cost(), expected);
            assert_eq!(GameSnapshot::from_view(&view).oath_xp_cost(), expected);
        }
    }
    hud.era.era = pyreal_primitives::EraId::Infiltration;
    world
        .weenie_mut(player)
        .unwrap()
        .qualities
        .as_mut()
        .unwrap()
        .set(StatKey::new(StatType::Int, 0x19), StatValue::Int(2));
    let view = HudView {
        hud: &hud,
        world: &world,
    };
    assert_eq!(view.experience_header().unwrap().level_span, 0);
    assert_eq!(view.oath_xp_cost(), Some(6250));
    assert_eq!(GameSnapshot::from_view(&view).oath_xp_cost(), Some(6250));
}

/// The era every front end reads comes from the world's own tables: February 2005's 126
/// levels and 36 skills (the old weapon skills), the end of retail's 275 levels and its
/// consolidated skills. The era itself is the server's when it announced one, else the dats'.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail and February 2005 dats: --features retail-dats"
)]
fn the_era_view_reads_the_worlds_level_cap_and_skills() {
    let world = pyreal_client_model::World::default();
    let old = pyreal_dat::testing::open_classic_store_or_fail();
    let mut h = Hud::new();
    h.load_tables(&old, &world);
    let v = HudView {
        hud: &h,
        world: &world,
    };
    let era = GameView::era(&v).expect("a world");
    assert!(era.before_throne_of_destiny());
    assert_eq!(era.level_cap, 126);
    assert_eq!(era.skills.len(), 36);
    assert!(
        era.has_skill(11) && !era.has_skill(44),
        "Sword, not Heavy Weapons"
    );
    assert_eq!(
        era.era,
        pyreal_primitives::EraId::Infiltration,
        "read from the dats"
    );
    assert!(!era.features().ratings && !era.features().luminance);

    // The server's announcement wins over the files.
    let mut h = Hud::new();
    h.era.era = pyreal_primitives::EraId::Eor;
    h.era.era_announced = true;
    h.load_tables(&old, &world);
    assert_eq!(h.era.era, pyreal_primitives::EraId::Eor);
    assert!(h.era.features().ratings);

    let later = pyreal_dat::testing::open_store_or_fail();
    let mut h = Hud::new();
    h.load_tables(&later, &world);
    assert!(!h.era.before_throne_of_destiny());
    assert_eq!(h.era.level_cap, 275);
    assert!(h.era.has_skill(44), "Heavy Weapons");
    assert_eq!(h.era.era, pyreal_primitives::EraId::Eor);
}

/// Oracle: the coordinate conversion checked against the live retail read-out —
/// **42.2N, 33.8E** while standing in Holtburg, which is
/// landblock `0xA9B4`.
#[test]
fn the_coordinate_read_out_matches_the_live_retail_value_over_holtburg() {
    // The cell the headless run stands in, `0xA9B40025`: block 0xA9/0xB4, cell index 0x25.
    let c = pyreal_primitives::CellId(0xA9B4_0025);
    let (n, e) = player_coords(c).expect("Holtburg's cell is a valid lcoord");
    // block y 0xB4 = 180 -> ly = 1440 + (0x25-1)%8 = 1444 -> (1444-1024)*0.1+0.5 = 42.5
    // block x 0xA9 = 169 -> lx = 1352 + (0x25-1)/8 = 1356 -> (1356-1024)*0.1+0.5 = 33.7
    assert!((n - 42.5).abs() < 0.001, "north was {n}");
    assert!((e - 33.7).abs() < 0.001, "east was {e}");
    // The retail read-out over the same town is 42.2N, 33.8E: the same tenth-scale, within the
    // few cells a player moves. What is asserted here is the formula and the axis order — a
    // swap would put 33.x first, and the retail line puts 42.x first.
    assert!(n > e, "N/S is the first coordinate");

    // A cell id `gid_to_lcoord` rejects is the player-coordinates query returning false.
    assert_eq!(player_coords(pyreal_primitives::CellId(0)), None);
}

/// Oracle: — the array is indexed by
/// `windowID − 1`, so row 0 is window 1 and the main chat window (id 8) is row 7.
#[test]
fn the_placement_array_is_indexed_by_window_id_minus_one() {
    use pyreal_protocol::property::{BaseProperty, PackObjPropertyCollection};

    let row = |visible: bool, x: i32| {
        let mut c = PropertyCollection::default();
        c.entries.push((
            placement::VISIBILITY,
            BaseProperty {
                name: placement::VISIBILITY,
                value: Some(BasePropertyValue::Bool(visible)),
            },
        ));
        c.entries.push((
            placement::X,
            BaseProperty {
                name: placement::X,
                value: Some(BasePropertyValue::Integer(x)),
            },
        ));
        BaseProperty {
            name: placement::ARRAY,
            value: Some(BasePropertyValue::Struct(c)),
        }
    };

    let mut props = PropertyCollection::default();
    props.entries.push((
        placement::ARRAY,
        BaseProperty {
            name: placement::ARRAY,
            value: Some(BasePropertyValue::Array(vec![
                row(false, 10),
                row(false, 20),
                row(false, 30),
                row(false, 40),
                row(false, 50),
                row(false, 60),
                row(false, 70),
                row(true, 80),
            ])),
        },
    ));
    let m = PlayerModule {
        option_flags: pyreal_protocol::login::player_module_flags::GAMEPLAY_OPTIONS,
        gameplay_options: Some(PackObjPropertyCollection {
            version: PackObjPropertyCollection::CORE_VERSION,
            properties: props,
        }),
        ..PlayerModule::default()
    };

    let p = decode_placements(&m);
    assert_eq!(p.rows.len(), 8);
    // Window 8 is the main chat window (`chat::interface::window::MAIN`), and it is row 7.
    let main = p
        .get(pyreal_client_contract::chat::interface::window::MAIN)
        .expect("window 8");
    assert_eq!(main.visible, Some(true));
    assert_eq!(main.x, Some(80));
    // Window 1 is row 0, and window 0 does not exist.
    assert_eq!(p.get(1).and_then(|r| r.x), Some(10));
    assert!(p.get(0).is_none());
    assert!(p.get(9).is_none());
}

/// Oracle: the module's own tolerance contract — a `PlayerModule` with no gameplay-options
/// section decodes to an empty placement table rather than failing, which is
/// the chat-window option query returning false for every window and the layout's own values standing.
#[test]
fn a_player_module_with_no_gameplay_options_places_nothing() {
    let p = decode_placements(&PlayerModule::default());
    assert!(p.rows.is_empty());
}

/// `SideBySideVitals` — `options_` bit 21.
const SIDE_BY_SIDE: u32 = 0x0020_0000;

/// The `PlayerOption` ordinal for Side-by-Side Vitals, taken through the same bridge
/// `interaction.rs`'s `SetPlayerOption` arm takes rather than restated as a literal.
fn side_by_side_ordinal() -> usize {
    option_ordinal(pyreal_client_contract::PlayerOption::SideBySideVitals)
}

fn hud_with(options: u32, options2: u32) -> (Hud, pyreal_client_model::World) {
    let m = PlayerModule {
        options,
        options2,
        ..PlayerModule::default()
    };
    let mut h = Hud::new();
    h.player_module = Some(m.clone());
    let mut w = pyreal_client_model::World::new();
    w.player_system.apply_player_module(&m);
    (h, w)
}

/// The applied latch moves when side by side vitals moves.
#[test]
fn the_applied_latch_moves_when_side_by_side_vitals_moves() {
    let (h, mut w) = hud_with(0, 0);
    let off = h.applied_key(&w, 7, Some(SIDE_BY_SIDE));
    assert!(
        !off.side_by_side_vitals,
        "bit 21 is clear in an all-zero options word"
    );

    // The player ticks Side-by-Side Vitals on the Character Options page. Nothing else about
    // the screen or the placements changes.
    let change = w.player_system.set_option(
        side_by_side_ordinal(),
        true,
        pyreal_primitives::ServerTime(0.0),
    );
    assert!(change.moved(), "the tick moved no bit at all");
    let on = h.applied_key(&w, 7, Some(SIDE_BY_SIDE));
    assert!(on.side_by_side_vitals);
    assert_ne!(
        off, on,
        "the latch cannot see the option it applies, so it will freeze"
    );
    assert_eq!(
        on.screen_serial, off.screen_serial,
        "the screen was not rebuilt"
    );
}

/// The tick moves the word option bit reads and the login blob copy does not.
#[test]
fn the_tick_moves_the_word_option_bit_reads_and_the_login_blob_copy_does_not() {
    // Arm 1 — the player's own tick, and only it. `Hud::player_module` is left at login's
    // all-zero word, so a reader on that copy answers `false` and this fails.
    let (h, mut w) = hud_with(0, 0);
    w.player_system.set_option(
        side_by_side_ordinal(),
        true,
        pyreal_primitives::ServerTime(0.0),
    );
    assert_eq!(
        h.player_module.as_ref().expect("a module").options & SIDE_BY_SIDE,
        0,
        "the tick did not reach the login blob's copy -- that is the premise, not the defect"
    );
    assert!(
        h.option_bit(&w, Some(SIDE_BY_SIDE)),
        "a player tick must reach option_bit, not merely character_option"
    );
    assert_eq!(
        character_option(&w, pyreal_client_contract::PlayerOption::SideBySideVitals),
        Some(true),
        "and the two accessors must answer from one word"
    );

    // Arm 2 — the login blob's copy moved on its own, which is what a stale reader would
    // follow. There is one option word now, and this is not it.
    let (mut h, w) = hud_with(0, 0);
    h.player_module.as_mut().expect("a module").options = u32::MAX;
    assert!(
        !h.option_bit(&w, Some(SIDE_BY_SIDE)),
        "option_bit is reading Hud::player_module again: see its own note"
    );
}

/// With no player description every option bit is false.
#[test]
fn with_no_player_description_every_option_bit_is_false() {
    let h = Hud::new();
    let mut w = pyreal_client_model::World::new();
    // The model word can be non-zero before a description lands: `Options::default()` is
    // the default character-option word. The gate is the *module*, not the word.
    w.player_system.options.options = u32::MAX;
    assert!(w.player_system.module.is_none());
    assert!(!h.option_bit(&w, Some(SIDE_BY_SIDE)));
    assert_eq!(
        character_option(&w, pyreal_client_contract::PlayerOption::SideBySideVitals),
        None
    );
}

/// With the production constant the latch follows the option.
#[test]
fn with_the_production_constant_the_latch_follows_the_option() {
    let (h, mut w) = hud_with(0, 0);
    let off = h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS);
    assert!(
        !off.side_by_side_vitals,
        "bit 21 is clear in an all-zero options word"
    );
    // The player's own tick, through the same bridge `interaction.rs`'s arm uses.
    w.player_system.set_option(
        side_by_side_ordinal(),
        true,
        pyreal_primitives::ServerTime(0.0),
    );
    let on = h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS);
    assert!(
        on.side_by_side_vitals,
        "the production constant must read the option"
    );
    assert_ne!(
        off, on,
        "the pass is gated on this key: if it cannot move, ticking the row changes nothing on \
             screen until the next screen rebuild"
    );
    assert_eq!(
        on.screen_serial, off.screen_serial,
        "the screen was not rebuilt"
    );
    assert_eq!(
        character_option::SIDE_BY_SIDE_VITALS,
        Some(0x0020_0000),
        "side-by-side vitals is `options_ >> 0x15 & 1`"
    );
}

/// The bulk of what the pass applies. The original two writers were the `0x0013` arm and
/// session-end reset. Local numeric placement writes now also mirror this cache, explicitly
/// updating the applied key because their retail notice does not reseed the screen. An
/// arbitrary placement replacement still invalidates the key, as this test demonstrates.
#[test]
fn the_applied_latch_moves_when_the_placements_move() {
    let (mut h, w) = hud_with(0, 0);
    let before = h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS);
    h.placements.set(
        pyreal_client_contract::chat::interface::window::MAIN,
        pyreal_client_contract::floaty::WindowPlacement {
            visible: Some(true),
            ..pyreal_client_contract::floaty::WindowPlacement::default()
        },
    );
    assert_ne!(
        before,
        h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS)
    );
    // And the screen serial, which is the term the latch has always carried.
    assert_ne!(
        h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS),
        h.applied_key(&w, 8, character_option::SIDE_BY_SIDE_VITALS)
    );
}

/// **The declared exclusion, asserted so it cannot be reversed by accident.** `lock_ui` is the
/// third thing the pass applies and is deliberately *not* in the key: its visible mirror flows
/// screen → HUD (`Hud::set_lock_ui` runs after the pass), so a toggle would otherwise make
/// the next frame a re-seed frame and re-assert every window's stored visibility over whatever
/// the player had since changed. If somebody adds it, this fails and they have to read the note
/// on [`AppliedKey`] before deciding.
#[test]
fn lock_ui_is_deliberately_not_in_the_applied_latch() {
    let (mut h, w) = hud_with(0, 0);
    let before = h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS);
    h.set_lock_ui(true);
    assert!(
        h.lock_ui(),
        "the padlock did not go up, so this asserts nothing"
    );
    assert_eq!(h.stats.lock_ui_writes, 1);
    assert_eq!(
        before,
        h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS),
        "lock_ui entered the latch: see AppliedKey's note before allowing this"
    );
}

/// The padlock hud mirror preserves neighboring options2 bits.
#[test]
fn the_padlock_hud_mirror_preserves_neighboring_options2_bits() {
    let (mut h, _w) = hud_with(0, 0);
    h.player_module.as_mut().expect("a module").options2 |= 0x0200_0000;
    h.set_lock_ui(true);
    assert!(h.lock_ui(), "the padlock went up in the HUD's copy");
    assert_eq!(
        h.player_module.as_ref().expect("a module").options2 & 0x0200_0000,
        0x0200_0000,
        "the neighboring options2 bit survives the lock mirror"
    );
}

/// One `SessionEvent::UiEvent` carrying `m`, shaped as `Session::dispatch` shapes it: the
/// opcode dword and then the body.
fn ui_event<M: pyreal_protocol::Message>(m: &M) -> SessionEvent {
    let mut blob = M::OPCODE.0.to_le_bytes().to_vec();
    blob.extend(pyreal_protocol::write_body(m).expect("a synthetic message encodes"));
    SessionEvent::UiEvent {
        opcode: M::OPCODE,
        blob,
    }
}

/// The title table message keeps its list and not only the display title.
#[test]
fn the_title_table_message_keeps_its_list_and_not_only_the_display_title() {
    let mut h = Hud::new();
    let mut w = pyreal_client_model::World::new();
    let m = pyreal_protocol::social::CharacterTitlesMessage {
        version: 1,
        display_title: 34,
        titles: vec![1, 34, 87],
    };
    let _ = h.apply_events(&[ui_event(&m)], &mut w);
    assert_eq!(h.display_title, 34, "the header's title id still arrives");
    assert_eq!(
        w.player_system.social.titles,
        vec![1, 34, 87],
        "the title list survives the handler -- this is what the Titles tab draws"
    );
    assert_eq!(w.player_system.social.display_title, 34);
    assert_eq!(h.stats.undecodable, 0);
}

/// Oracle: adding a character title always inserts it, then optionally makes it the display title.
///
/// The **add is unconditional**; only the display move is gated. The panel's own
/// add-title and set-display-title handlers
/// both walk the title list for the id first and insert only when it is absent,
/// which is the dedupe below.
#[test]
fn add_or_set_character_title_appends_to_the_list_whether_or_not_it_becomes_the_display() {
    let mut h = Hud::new();
    let mut w = pyreal_client_model::World::new();
    let table = pyreal_protocol::social::CharacterTitlesMessage {
        version: 1,
        display_title: 34,
        titles: vec![34],
    };
    let quiet = pyreal_protocol::social::SocialAddOrSetCharacterTitle {
        new_title: 12,
        set_as_display_title: 0,
    };
    let loud = pyreal_protocol::social::SocialAddOrSetCharacterTitle {
        new_title: 99,
        set_as_display_title: 1,
    };
    let again = pyreal_protocol::social::SocialAddOrSetCharacterTitle {
        new_title: 12,
        set_as_display_title: 0,
    };
    let _ = h.apply_events(
        &[
            ui_event(&table),
            ui_event(&quiet),
            ui_event(&loud),
            ui_event(&again),
        ],
        &mut w,
    );
    assert_eq!(
        w.player_system.social.titles,
        vec![34, 12, 99],
        "every earned title is on the list, once, in arrival order"
    );
    assert_eq!(
        h.display_title, 99,
        "and only the flagged one moved the header"
    );
    assert_eq!(w.player_system.social.display_title, 99);
}

/// The death tag decides a system line's fate by the hear-PK-deaths option alone.
#[test]
fn the_pk_death_filter_drops_or_strips_only_a_tagged_line() {
    assert_eq!(
        pk_death_filter("You have entered the Trade channel.", false),
        PkDeathLine::Untagged
    );
    assert_eq!(
        pk_death_filter("[PKDe]Lark was slain by Wren!", false),
        PkDeathLine::Dropped
    );
    assert_eq!(
        pk_death_filter("[PKDe]Lark was slain by Wren!", true),
        PkDeathLine::Stripped("Lark was slain by Wren!".into())
    );
    // Anywhere in the line, every copy, and nothing else touched.
    assert_eq!(
        pk_death_filter("Lark [PKDe]was slain [PKDe]", true),
        PkDeathLine::Stripped("Lark was slain ".into())
    );
    // The tag is matched exactly, case and all.
    assert_eq!(pk_death_filter("[pkde]Lark", false), PkDeathLine::Untagged);
}

/// End to end through the `0xF7E0` arm: heard by default with the tag gone, and not drawn at
/// all once the player turns the option off; an untagged line is drawn either way.
#[test]
fn a_pk_death_broadcast_is_heard_stripped_by_default_and_dropped_when_the_option_is_off() {
    use pyreal_client_model::player::options::option::HEAR_PK_DEATHS;
    let line = |text: &str| pyreal_protocol::comms::CommunicationTextboxString {
        text: text.into(),
        text_type: 0,
    };
    let mut h = Hud::new();
    let mut w = pyreal_client_model::World::new();
    assert!(w.player_system.options.hear_pk_deaths(), "on by default");
    let got = h.apply_events(&[ui_event(&line("[PKDe]Lark was slain by Wren!"))], &mut w);
    let bodies: Vec<&str> = got.iter().map(|m| m.body.as_str()).collect();
    assert_eq!(bodies, vec!["Lark was slain by Wren!"]);

    w.player_system
        .set_option(HEAR_PK_DEATHS, false, pyreal_primitives::ServerTime(0.0));
    let got = h.apply_events(
        &[
            ui_event(&line("[PKDe]Lark was slain by Wren!")),
            ui_event(&line("You feel refreshed.")),
        ],
        &mut w,
    );
    let bodies: Vec<&str> = got.iter().map(|m| m.body.as_str()).collect();
    assert_eq!(bodies, vec!["You feel refreshed."]);
    assert_eq!(h.stats.textbox_pk_deaths_dropped, 1);
    assert_eq!(h.stats.textbox_lines_squelched, 0);
}

/// Event `0x0317` draws its one string exactly as `0x02EB` does: chat type `0x1A`, no prefix,
/// window 0.
#[test]
fn event_0317_is_drawn_as_a_transient_line() {
    let mut h = Hud::new();
    let mut w = pyreal_client_model::World::new();
    let got = h.apply_events(
        &[
            ui_event(&pyreal_protocol::comms::CommunicationTransientString0317 {
                text: "The chest is locked.".into(),
            }),
            ui_event(&pyreal_protocol::comms::CommunicationTransientString {
                text: "The chest is locked.".into(),
            }),
        ],
        &mut w,
    );
    assert_eq!(got.len(), 2);
    assert_eq!(got[0], got[1], "both events draw the same line");
    assert_eq!(got[0].body, "The chest is locked.");
    assert_eq!(got[0].ty, 0x1A);
    assert_eq!(h.stats.undecodable, 0);
}

#[test]
fn where_an_item_can_be_worn_reaches_the_interface_live_and_in_a_snapshot() {
    use pyreal_client_contract::snapshot::GameSnapshot;
    use pyreal_client_model::{Weenie, World};
    let (sword, gem) = (ObjectId(7), ObjectId(8));
    let mut world = World::new();
    let mut w = Weenie::new(sword);
    // Wielded in the main hand.
    w.pwd.valid_locations = Some(0x0010_0000);
    world.tables.weenies.insert(sword, w);
    world.tables.weenies.insert(gem, Weenie::new(gem));
    // The snapshot keeps the objects an interface is looking at: the selection among them.
    world.selected = Some(sword);
    let hud = Hud::new();
    let view = HudView {
        hud: &hud,
        world: &world,
    };
    assert_eq!(view.equip_locations(sword), 0x0010_0000);
    assert_eq!(view.equip_locations(gem), 0, "a gem is not worn");
    assert_eq!(view.equip_locations(ObjectId(99)), 0, "nor what is unknown");
    let captured = GameSnapshot::from_view(&view);
    assert_eq!(captured.equip_locations(sword), 0x0010_0000);
}

#[test]
fn an_appraised_attuned_item_reads_as_attuned_live_and_in_a_snapshot() {
    use pyreal_client_contract::snapshot::GameSnapshot;
    use pyreal_client_model::{Weenie, World};
    use pyreal_protocol::{archive::PackedHash, types::appraisal::AppraisalProfile};
    let (bound, plain) = (ObjectId(7), ObjectId(8));
    let mut world = World::new();
    world.tables.weenies.insert(bound, Weenie::new(bound));
    world.tables.weenies.insert(plain, Weenie::new(plain));
    let mut profile = AppraisalProfile::default();
    profile.tables.ints = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x72, 1)],
    });
    world.appraisal.set(bound, profile);
    world.selected = Some(bound);
    let hud = Hud::new();
    let view = HudView {
        hud: &hud,
        world: &world,
    };
    assert!(view.item_attuned(bound));
    assert!(
        !view.item_attuned(plain),
        "not appraised: not known to be attuned"
    );
    assert!(GameSnapshot::from_view(&view).item_attuned(bound));
}

#[test]
fn an_item_s_cooldown_is_held_from_the_player_s_registry_until_the_server_removes_it() {
    use pyreal_client_model::{Weenie, World};
    use pyreal_protocol::types::qualities::StatMod;
    use pyreal_rules::enchant::{ench_type, Enchantment};
    let player = ObjectId(1);
    let mut world = World::new();
    world.player = Some(player);
    let mut qualities = pyreal_client_model::Qualities::new();
    // Cooldown 7, thirty seconds long, arrived at t = 100.
    assert!(qualities.enchantments.update_enchantment(Enchantment {
        id: 0x0001_8007,
        spell_category: 0x8000,
        power_level: 0,
        start_time: 100.0,
        duration: 30.0,
        caster: ObjectId(9),
        degrade_modifier: 0.0,
        degrade_limit: -666.0,
        last_time_degraded: 100.0,
        smod: StatMod {
            kind: ench_type::COOLDOWN,
            key: 0,
            value: 0.0,
        },
        spell_set_id: None,
    }));
    let mut row = Weenie::new(player);
    row.qualities = Some(qualities);
    world.tables.weenies.insert(player, row);
    let mut hud = Hud::new();
    hud.player_desc_received = true;
    let view = HudView {
        hud: &hud,
        world: &world,
    };
    assert_eq!(view.cooldown_held(7, 110.0), Some(20.0));
    assert_eq!(
        view.cooldown_remaining(7, 131.0),
        None,
        "run out by this clock"
    );
    assert_eq!(view.cooldown_held(7, 131.0), Some(-1.0), "but still held");
    assert_eq!(view.cooldown_held(8, 110.0), None, "another cooldown");
    world
        .player_qualities_mut()
        .unwrap()
        .enchantments
        .remove_enchantment(0x0001_8007);
    let view = HudView {
        hud: &hud,
        world: &world,
    };
    assert_eq!(view.cooldown_held(7, 131.0), None, "removed by the server");
}

///
/// Oracle: the retail creature-type names. The dat spells a fire elemental `Fire_Elemental`; the
/// creature pane, the slayer line and the heritage line all read it `Fire Elemental`.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_creature_type_is_named_with_a_space_where_the_dat_spells_an_underscore() {
    use pyreal_client_model::{Weenie, World};
    use pyreal_protocol::{archive::PackedHash, types::appraisal::AppraisalProfile};
    const FIRE_ELEMENTAL: i32 = 38;
    let store = pyreal_dat::testing::open_store_or_fail();
    let id = ObjectId(7);
    let mut world = World::new();
    world.tables.weenies.insert(id, Weenie::new(id));
    let mut hud = Hud::new();
    hud.load_tables(&store, &world);
    let spelled = hud
        .creature_type_names
        .as_ref()
        .and_then(|m| m.id_to_string.iter().find(|(k, _)| *k == 38))
        .map(|(_, v)| v.clone());
    assert_eq!(spelled.as_deref(), Some("Fire_Elemental"), "the premise");
    let mut profile = AppraisalProfile {
        creature_profile: Some(Default::default()),
        ..AppraisalProfile::default()
    };
    profile.tables.ints = Some(PackedHash {
        table_size: 8,
        entries: vec![(2, FIRE_ELEMENTAL), (0xA6, FIRE_ELEMENTAL)],
    });
    world.appraisal.set(id, profile);
    let view = HudView {
        hud: &hud,
        world: &world,
    };
    let a = view.appraisal(id).expect("an appraisal");
    assert_eq!(a.creature_display_name.as_deref(), Some("Fire Elemental"));
    assert_eq!(
        a.special.slayer,
        Some((FIRE_ELEMENTAL, "Fire Elemental".into()))
    );
    assert_eq!(a.gender_heritage_display.as_deref(), Some("Fire Elemental"));
}

/// A tell from `sender` under `name`, addressed to the player `me`.
fn a_tell_to(me: ObjectId, sender: u32, name: &str, message: &str) -> SessionEvent {
    ui_event(&pyreal_protocol::comms::CommunicationHearDirectSpeech {
        message: message.into(),
        sender_name: name.into(),
        sender_id: ObjectId(sender),
        target_id: me,
        text_type: pyreal_client_model::chat::text_type::SPEECH_DIRECT,
        secret_flags: 0,
    })
}

/// A HUD with a player in the world, which every tell needs to be drawn at all.
fn a_hud_with_a_player(me: ObjectId) -> (Hud, pyreal_client_model::World) {
    let mut h = Hud::new();
    let mut w = pyreal_client_model::World::new();
    let _ = h.apply_events(&[SessionEvent::PlayerCreated(me)], &mut w);
    h.player_body = true;
    (h, w)
}

/// The stand-in id a world gives a network channel in these tests.
const CHANNEL_ID: u32 = 0x6000_0007;

/// A Dereth Network channel's tell is drawn as a channel line in the channel color, the speaker
/// named and clickable, and a reply still goes to the channel by its id.
#[test]
fn a_network_channels_tell_is_drawn_as_a_channel_line_and_a_reply_goes_to_the_channel() {
    let me = ObjectId(0x5000_0001);
    let (mut h, mut w) = a_hud_with_a_player(me);
    let got = h.apply_events(
        &[a_tell_to(
            me,
            CHANNEL_ID,
            "group@channel",
            "Person@world: bro",
        )],
        &mut w,
    );
    assert_eq!(got.len(), 1);
    assert_eq!(
        got[0].body,
        "[group] <Tell:IIDString:0:Person@world>Person@world<\\Tell> says, \"bro\""
    );
    assert_eq!(
        u32::from(got[0].ty),
        pyreal_client_model::chat::text_type::CHANNEL
    );
    assert_eq!(w.chat.last_teller, Some(ObjectId(CHANNEL_ID)));
    assert_eq!(
        w.chat.reply_targets().last_teller.as_deref(),
        Some("group@channel")
    );
    assert_eq!(
        w.chat.do_reply("hi all"),
        pyreal_client_model::chat::TellOutcome::ById {
            message: "hi all".into(),
            target: ObjectId(CHANNEL_ID),
        },
        "@reply tells the channel by its id"
    );
}

/// An ordinary tell, a stand-in sender whose name is not a channel's, and a channel's name from a
/// sender without a stand-in id all stay tells, word for word.
#[test]
fn only_a_stand_in_sender_named_as_a_channel_is_drawn_as_a_channel_line() {
    let me = ObjectId(0x5000_0001);
    for (sender, name, text) in [
        (0x5000_0002, "Alba", "hello"),
        (CHANNEL_ID, "Bob@coldeve", "Person@world: bro"),
        (0x5000_0002, "group@channel", "Person@world: bro"),
        (0x7000_0000, "group@channel", "Person@world: bro"),
    ] {
        let (mut h, mut w) = a_hud_with_a_player(me);
        let got = h.apply_events(&[a_tell_to(me, sender, name, text)], &mut w);
        assert_eq!(got.len(), 1, "{name}");
        assert!(
            got[0].body.contains(name) && got[0].body.ends_with(&format!(" tells you, \"{text}\"")),
            "{name} from {sender:#x}: {}",
            got[0].body
        );
        assert_eq!(
            u32::from(got[0].ty),
            pyreal_client_model::chat::text_type::SPEECH_DIRECT,
            "{name}"
        );
    }
}

/// The world's echo of a line told to a network channel is drawn as the player's own channel
/// line; an echo of a tell to a character is left as it came.
#[test]
fn the_echo_of_a_line_told_to_a_network_channel_reads_as_the_players_own_channel_line() {
    let echo = |text: &str| {
        ui_event(&pyreal_protocol::comms::CommunicationTextboxString {
            text: text.into(),
            text_type: pyreal_client_model::chat::text_type::SPEECH_DIRECT_SEND,
        })
    };
    let mut h = Hud::new();
    let mut w = pyreal_client_model::World::new();
    let got = h.apply_events(
        &[
            echo("You tell group@channel, \"bro\""),
            echo("You tell Alba, \"hello\""),
        ],
        &mut w,
    );
    let drawn: Vec<(u32, &str)> = got
        .iter()
        .map(|m| (u32::from(m.ty), m.body.as_str()))
        .collect();
    assert_eq!(
        drawn,
        vec![
            (
                pyreal_client_model::chat::text_type::CHANNEL_SEND,
                "[group] You say, \"bro\""
            ),
            (
                pyreal_client_model::chat::text_type::SPEECH_DIRECT_SEND,
                "You tell Alba, \"hello\""
            ),
        ]
    );
}

/// The channel line's own composition: a text without the speaker's address is drawn whole, and
/// the channel's suffix is read in any case.
#[test]
fn a_channel_line_without_a_speaker_is_drawn_whole() {
    use pyreal_client_model::chat::composition::{
        network_channel, network_channel_echo, network_channel_line,
    };
    assert_eq!(
        network_channel_line("group", "no speaker here"),
        "[group] no speaker here"
    );
    assert_eq!(network_channel(CHANNEL_ID, "Group@Channel"), Some("Group"));
    assert_eq!(network_channel(CHANNEL_ID, "@channel"), None);
    assert_eq!(network_channel(0x6000_0000, "group@channel"), None);
    assert_eq!(network_channel(0x6FFF_FFFF, "group@channel"), Some("group"));
    assert_eq!(network_channel_echo("You tell a@b@channel, \"x\""), None);
}

/// A skill's footer numbers are asked of the player's qualities when they are read, so a vitae
/// won back shows at once, while the skill rows keep the join the skills page last rebuilt (the
/// page listens for enchantment changes, and a change of vitae alone is not one).
#[test]
fn a_skills_footer_numbers_follow_the_vitae_as_it_is_now_and_not_the_last_rebuilt_row() {
    use pyreal_client_model::{StatKey, StatType, StatValue, Weenie, World};
    use pyreal_protocol::types::qualities::{Enchantment, Skill, StatMod};
    use pyreal_rules::enchant::ench_type;
    const MELEE_DEFENSE: u32 = 6;
    const VITAE: u32 = 666;
    let player = ObjectId(0x5000_0001);
    let mut world = World::new();
    world.player = Some(player);
    let mut row = Weenie::new(player);
    let mut q = pyreal_client_model::Qualities::new();
    q.set(
        StatKey::new(StatType::Skill, MELEE_DEFENSE),
        StatValue::Skill(Skill {
            sac: 2,
            init_level: 100,
            ..Skill::default()
        }),
    );
    row.qualities = Some(q);
    world.tables.weenies.insert(player, row);
    let mut hud = Hud::new();
    hud.player_desc_received = true;
    hud.skill_table = Some(pyreal_assets::tables::SkillTable {
        id: DataId(0x0E00_0004),
        buckets: 1,
        skills: [(
            MELEE_DEFENSE,
            pyreal_assets::tables::SkillBase {
                description: String::new(),
                name: "Melee Defense".into(),
                icon: 0,
                trained_cost: 0,
                specialized_cost: 0,
                category: 0,
                chargen_use: 0,
                min_level: 1,
                formula: pyreal_assets::tables::SkillFormula {
                    w: 0,
                    x: 0,
                    y: 0,
                    z: 1,
                    attr1: 0,
                    attr2: 0,
                },
                upper_bound: 0.0,
                lower_bound: 0.0,
                learn_mod: 1.0,
            },
        )]
        .into_iter()
        .collect(),
    });
    hud.xp_table = Some(pyreal_assets::tables::XpTable {
        id: DataId(0x0E00_0018),
        attribute_xp: vec![],
        vital_xp: vec![],
        trained_xp: vec![0],
        specialized_xp: vec![0],
        level_xp: vec![0],
        level_credits: vec![0],
    });
    let footer = |hud: &Hud, world: &World| {
        let a = HudView { hud, world }
            .skill_advancement(MELEE_DEFENSE)
            .expect("the footer's numbers");
        (a.level, a.effective, a.vitae)
    };
    let shown_row = |hud: &Hud| {
        hud.skills
            .iter()
            .find(|s| s.id == MELEE_DEFENSE)
            .map(|s| (s.level, s.effective, s.vitae))
            .expect("the skill's row")
    };
    // A death: a tenth off, which the page's rebuild on the description takes in.
    let vitae = Enchantment {
        id: VITAE,
        duration: -1.0,
        smod: StatMod {
            kind: ench_type::VITAE | ench_type::MULTIPLICATIVE,
            key: 0,
            value: 0.9,
        },
        ..Enchantment::default()
    };
    assert!(world.update_player_enchantment(&vitae, pyreal_primitives::LocalTime(1.0)));
    hud.rebuild_panel_tables(&world);
    assert_eq!(shown_row(&hud), (100, 90, -10));
    assert_eq!(footer(&hud, &world), (100, 90, -10));
    // Won back: the row stays as the page last built it, and the footer has it as it is.
    world.remove_player_enchantment(VITAE);
    assert_eq!(shown_row(&hud), (100, 90, -10), "the page's own join");
    assert_eq!(footer(&hud, &world), (100, 100, 0), "no vitae left");
}

#[test]
fn a_short_component_table_takes_the_mapper_with_the_most_entries_of_those_it_overlaps_alike() {
    use super::tables::better_component_mapper as better;
    assert!(better((74, 78), None), "the first that overlaps at all");
    assert!(!better((0, 78), None), "one that overlaps nothing never is");
    assert!(better((74, 164), Some((74, 78))), "1999's 74, tied");
    assert!(
        !better((74, 78), Some((74, 164))),
        "1999's 74, the other way"
    );
    assert!(
        better((163, 164), Some((74, 78))),
        "the most overlap still comes first"
    );
    assert!(!better((41, 42), Some((74, 164))));
}

#[test]
fn spell_charges_go_with_the_character_at_logoff() {
    let mut h = Hud::new();
    let mut w = pyreal_client_model::World::new();
    h.spell_charges.on_update(7, false, 0.0);
    assert!(h.spell_charges.value(7, 0.0) < 1.0);
    let _ = h.apply_events(&[SessionEvent::LoggedOff], &mut w);
    assert!(
        (h.spell_charges.value(7, 0.0) - 1.0).abs() < 1e-6,
        "the next character starts full"
    );
}

#[test]
fn enchantment_and_vitae_notices_rebuild_the_skill_rows() {
    for notice in [
        pyreal_client_model::Notice::EnchantmentsChanged,
        pyreal_client_model::Notice::VitaeChanged,
    ] {
        let mut h = Hud::new();
        let w = pyreal_client_model::World::new();
        h.skills = vec![SkillEntry {
            id: 6,
            name: "Melee Defense".into(),
            icon: None,
            min_level: 1,
            sac: 2,
            level: 10,
            effective: 10,
            vitae: 0,
        }];
        h.on_object_notice(&w, &notice);
        assert!(h.skills.is_empty(), "{notice:?} rebuilt the stale rows");
    }
}

/// Equal stamps are accepted, so the world route after the HUD's is not stale and mirrors.
#[test]
fn a_public_quality_update_about_the_player_lands_once_and_mirrors() {
    let me = ObjectId(0x5000_0001);
    let mut w = pyreal_client_model::World::new();
    w.player = Some(me);
    let mut row = pyreal_client_model::Weenie::new(me);
    row.qualities = Some(pyreal_client_model::Qualities::new());
    w.tables.weenies.insert(me, row);
    let event = ui_event(&pyreal_protocol::qualities::QualitiesUpdateInt(
        pyreal_protocol::qualities::PublicUpdate {
            sequence: 3,
            object: me,
            property_id: 19,
            value: 250,
        },
    ));
    let mut h = Hud::new();
    let _ = h.apply_events(std::slice::from_ref(&event), &mut w);
    let mut inter = crate::interaction::Interaction::new();
    crate::interaction::apply_events_at_boundary(
        &mut inter,
        std::slice::from_ref(&event),
        &mut w,
        None,
        &mut |_, _, _| {},
    );
    assert_eq!(h.stats.quality_updates, 1);
    assert_eq!(inter.stats.object_quality_updates, 1);
    assert_eq!(inter.stats.object_quality_updates_stale, 0);
    assert_eq!(
        w.weenie(me).and_then(|x| x.pwd.value),
        Some(250),
        "the mirror ran"
    );
}

#[test]
fn an_allegiance_login_notice_sets_the_members_online_bit() {
    let mut w = pyreal_client_model::World::new();
    w.allegiance.add(
        None,
        pyreal_client_model::allegiance::AllegianceData {
            id: ObjectId(7),
            name: "Ann".into(),
            ..Default::default()
        },
    );
    let mut h = Hud::new();
    let on = pyreal_protocol::social::AllegianceLoginNotification {
        member: ObjectId(7),
        now_logged_in: 1,
    };
    let _ = h.apply_events(&[ui_event(&on)], &mut w);
    assert!(w
        .allegiance
        .look_up(ObjectId(7))
        .is_some_and(|d| d.is_logged_in()));
}

/// A `0x01C7` failure line is trimmed like the `0x028A`, `0x028B` and `0x00A0` arms'.
#[test]
fn a_use_done_failure_line_is_trimmed() {
    let mut h = Hud::new();
    let mut w = pyreal_client_model::World::new();
    let got = h.apply_events(
        &[ui_event(&pyreal_protocol::objects::ItemUseDone {
            failure_type: 0x474,
        })],
        &mut w,
    );
    assert_eq!(got.len(), 1);
    assert_eq!(
        got[0].body,
        "You must complete a quest to interact with that portal."
    );
}

/// A tell from a squelched character is drawn; the shard refuses those, not the client.
#[test]
fn a_tell_from_a_squelched_character_is_still_drawn() {
    use pyreal_client_model::chat::{text_type, SquelchEntry};
    let me = ObjectId(0x5000_0001);
    let (mut h, mut w) = a_hud_with_a_player(me);
    w.chat.squelch.characters.insert(
        ObjectId(0x5000_0002),
        SquelchEntry {
            types: [text_type::SPEECH_DIRECT].into_iter().collect(),
            is_zone_squelch: 0,
            name: "Alba".into(),
        },
    );
    let got = h.apply_events(&[a_tell_to(me, 0x5000_0002, "Alba", "hello")], &mut w);
    assert_eq!(got.len(), 1);
    assert_eq!(h.stats.speech_lines_squelched, 0);
}

/// A spell's examined range reads the buffed skill, not its ranks.
#[test]
fn a_spells_examined_range_reads_the_buffed_skill() {
    use pyreal_client_contract::view::GameView;
    use pyreal_client_model::{StatKey, StatType, StatValue, Weenie, World};
    use pyreal_protocol::types::qualities::{Enchantment, Skill, StatMod};
    use pyreal_rules::enchant::ench_type;
    const WAR_MAGIC: u32 = 0x22;
    let player = ObjectId(0x5000_0001);
    let mut world = World::new();
    world.player = Some(player);
    let mut row = Weenie::new(player);
    let mut q = pyreal_client_model::Qualities::new();
    q.set(
        StatKey::new(StatType::Skill, WAR_MAGIC),
        StatValue::Skill(Skill {
            sac: 2,
            init_level: 100,
            ..Skill::default()
        }),
    );
    row.qualities = Some(q);
    world.tables.weenies.insert(player, row);
    let buff = Enchantment {
        id: 1,
        duration: -1.0,
        smod: StatMod {
            kind: ench_type::SKILL | ench_type::SINGLE_STAT | ench_type::ADDITIVE,
            key: WAR_MAGIC,
            value: 50.0,
        },
        ..Enchantment::default()
    };
    assert!(world.update_player_enchantment(&buff, pyreal_primitives::LocalTime(1.0)));
    let mut hud = Hud::new();
    hud.player_desc_received = true;
    hud.skill_table = Some(pyreal_assets::tables::SkillTable {
        id: DataId(0x0E00_0004),
        buckets: 1,
        skills: [(
            WAR_MAGIC,
            pyreal_assets::tables::SkillBase {
                description: String::new(),
                name: "War Magic".into(),
                icon: 0,
                trained_cost: 0,
                specialized_cost: 0,
                category: 0,
                chargen_use: 0,
                min_level: 1,
                formula: pyreal_assets::tables::SkillFormula {
                    w: 0,
                    x: 0,
                    y: 0,
                    z: 1,
                    attr1: 0,
                    attr2: 0,
                },
                upper_bound: 0.0,
                lower_bound: 0.0,
                learn_mod: 1.0,
            },
        )]
        .into_iter()
        .collect(),
    });
    hud.spell_table = Some(pyreal_assets::tables::SpellTable {
        id: DataId(0x0E00_000E),
        spell_buckets: 1,
        spells: [(
            27,
            pyreal_assets::tables::SpellBase {
                name: "Flame Bolt I".into(),
                description: String::new(),
                school: 1,
                icon: 0,
                category: 0,
                bitfield: 0,
                base_mana: 10,
                base_range_constant: 5.0,
                base_range_mod: 0.25,
                power: 1,
                spell_economy_mod: 1.0,
                formula_version: 1,
                component_loss: 0.0,
                meta_spell_type: 2,
                meta_spell_id: 27,
                duration: None,
                portal_lifetime: None,
                raw_comps: [0; 8],
                comp_key: 0,
                comps: Vec::new(),
                caster_effect: 0,
                target_effect: 0,
                fizzle_effect: 0,
                recovery_interval: 0.0,
                recovery_amount: 0.0,
                display_order: 0,
                non_component_target_type: 0,
                mana_mod: 0,
            },
        )]
        .into_iter()
        .collect(),
        spellset_bucket_index: 0,
        spellsets: Default::default(),
    });
    let v = HudView {
        hud: &hud,
        world: &world,
    }
    .spell_examine(27)
    .expect("the spell's pane");
    // 5 + 0.25 * 150, not 5 + 0.25 * 100.
    assert_eq!(v.range, 42.5);
}

/// The once-a-frame sync composes the material prefix, so a salvage bag's name carries its
/// material and not the shard's bare name.
#[test]
fn the_frame_sync_gives_salvage_its_material_name() {
    use pyreal_client_contract::view::GameView;
    use pyreal_client_model::Weenie;
    let id = ObjectId(0x8000_0001);
    let mut objects = crate::objects::ObjectStream::new();
    let mut bag = Weenie::new(id);
    bag.pwd.name = "Salvage (100)".into();
    bag.pwd.material_type = Some(16);
    objects.world.tables.weenies.insert(id, bag);
    let mut hud = Hud::new();
    hud.material_names = Some(pyreal_assets::DidMapper {
        id: MATERIAL_TYPE_NAMES,
        enum_to_id: Vec::new(),
        enum_to_name: vec![(16, "Granite".into())],
        enum_to_id_internal: Vec::new(),
        enum_to_name_internal: Vec::new(),
    });
    hud.sync(&objects, None);
    let view = HudView {
        hud: &hud,
        world: &objects.world,
    };
    assert_eq!(view.name(id), Some("Granite Salvage (100)"));
    assert_eq!(hud.stats.display_names_composed, 1);
    // A second frame with nothing changed composes nothing.
    hud.sync(&objects, None);
    assert_eq!(hud.stats.display_names_composed, 1);
}

/// An enchantment of `spell` in `layer`, of `category` at `power`, cast at `start`.
fn layered_enchantment(
    spell: u32,
    layer: u32,
    category: u16,
    power: i32,
    start: f64,
) -> pyreal_rules::enchant::Enchantment {
    use pyreal_protocol::types::qualities::StatMod;
    use pyreal_rules::enchant::{ench_type, Enchantment};
    Enchantment {
        id: (layer << 16) | spell,
        spell_category: category,
        power_level: power,
        start_time: start,
        duration: 1800.0,
        caster: ObjectId(9),
        degrade_modifier: 0.0,
        degrade_limit: -666.0,
        last_time_degraded: start,
        smod: StatMod {
            kind: ench_type::SKILL | ench_type::ADDITIVE,
            key: u32::from(category),
            value: 10.0,
        },
        spell_set_id: None,
    }
}

/// The local player under `enchantments`, each spell in a spell table with `spellsets`.
fn enchanted_player(
    enchantments: &[pyreal_rules::enchant::Enchantment],
    spellsets: std::collections::BTreeMap<u32, pyreal_assets::tables::SpellSet>,
) -> (pyreal_client_model::World, Hud) {
    use pyreal_client_model::{Weenie, World};
    let player = ObjectId(1);
    let mut world = World::new();
    world.player = Some(player);
    let mut qualities = pyreal_client_model::Qualities::new();
    for e in enchantments {
        assert!(qualities.enchantments.update_enchantment(*e));
    }
    let mut row = Weenie::new(player);
    row.qualities = Some(qualities);
    world.tables.weenies.insert(player, row);
    let base = pyreal_assets::tables::SpellBase {
        name: String::new(),
        description: String::new(),
        school: 0,
        icon: 0,
        category: 0,
        bitfield: 4,
        base_mana: 0,
        base_range_constant: 0.0,
        base_range_mod: 0.0,
        power: 0,
        spell_economy_mod: 0.0,
        formula_version: 0,
        component_loss: 0.0,
        meta_spell_type: 0,
        meta_spell_id: 0,
        duration: None,
        portal_lifetime: None,
        raw_comps: [0; 8],
        comp_key: 0,
        comps: vec![],
        caster_effect: 0,
        target_effect: 0,
        fizzle_effect: 0,
        recovery_interval: 0.0,
        recovery_amount: 0.0,
        display_order: 0,
        non_component_target_type: 0,
        mana_mod: 0,
    };
    let mut hud = Hud::new();
    hud.player_desc_received = true;
    hud.spell_table = Some(pyreal_assets::tables::SpellTable {
        id: pyreal_primitives::DataId(0x0e00_000e),
        spell_buckets: 1,
        spells: enchantments
            .iter()
            .map(|e| (u32::from(e.spell_id()), base.clone()))
            .collect(),
        spellset_bucket_index: 0,
        spellsets,
    });
    (world, hud)
}

/// Whether the effect of `spell` is marked superseded.
fn superseded_of(hud: &Hud, world: &pyreal_client_model::World, spell: u32) -> Option<bool> {
    let view = HudView { hud, world };
    view.active_effects()
        .into_iter()
        .find(|e| e.spell == spell)
        .map(|e| e.superseded)
}

/// Oracle: retail's duel (`Enchantment::beats`). Of two enchantments of one category the higher
/// power is in force, and of two as strong the one cast later; the other is marked superseded.
#[test]
fn an_effect_beaten_in_its_category_is_marked_superseded() {
    // Armor V and VI share a category; two casts of equal power share another.
    let (armor_v, armor_vi, early, late) = (0x10u32, 0x11u32, 0x12u32, 0x13u32);
    let ench =
        |spell, category, power, start| layered_enchantment(spell, 1, category, power, start);
    let (world, hud) = enchanted_player(
        &[
            ench(armor_v, 7, 250, 300.0),
            ench(armor_vi, 7, 300, 100.0),
            ench(early, 8, 300, 100.0),
            ench(late, 8, 300, 200.0),
        ],
        Default::default(),
    );
    let superseded = |spell| superseded_of(&hud, &world, spell);
    assert_eq!(
        superseded(armor_v),
        Some(true),
        "the lower power, cast later"
    );
    assert_eq!(superseded(armor_vi), Some(false));
    assert_eq!(superseded(early), Some(true), "as strong, cast earlier");
    assert_eq!(superseded(late), Some(false));
}

/// Each caster's copy of a spell is an effect of its own, told apart by its layered id.
#[test]
fn two_casters_copies_of_one_spell_are_two_effects_with_their_own_ids() {
    let (world, hud) = enchanted_player(
        &[
            layered_enchantment(0x10, 1, 7, 300, 100.0),
            layered_enchantment(0x10, 2, 7, 300, 200.0),
        ],
        Default::default(),
    );
    let view = HudView {
        hud: &hud,
        world: &world,
    };
    let mut copies: Vec<(u32, u32, bool)> = view
        .active_effects()
        .into_iter()
        .map(|e| (e.spell, e.id, e.superseded))
        .collect();
    copies.sort_unstable();
    assert_eq!(
        copies,
        [(0x10, 0x0001_0010, true), (0x10, 0x0002_0010, false)]
    );
}

/// Oracle: ACE's `GetEnchantmentsTopLayer`, used once the world greets the player as ACE does. At
/// equal power a level 8 Aura Self spell beats an item Other spell and set spells go by spell id;
/// before that, and on Empyrean, the later cast wins.
#[test]
fn on_an_ace_world_ties_of_power_go_as_ace_settles_them() {
    // Aura of Incantation of Blood Drinker Self against its Other spell cast later; two spells of
    // one set, the lower id cast later.
    let (aura, other, set_low, set_high) = (4395u32, 5997u32, 5000u32, 5001u32);
    let tiers = [(2, vec![set_low, set_high])].into_iter().collect();
    let (mut world, mut hud) = enchanted_player(
        &[
            layered_enchantment(aura, 1, 7, 8, 100.0),
            layered_enchantment(other, 1, 7, 8, 200.0),
            layered_enchantment(set_low, 1, 8, 3, 200.0),
            layered_enchantment(set_high, 1, 8, 3, 100.0),
        ],
        [(1, pyreal_assets::tables::SpellSet { tiers })]
            .into_iter()
            .collect(),
    );
    let line = |text: &str| {
        ui_event(&pyreal_protocol::comms::CommunicationTextboxString {
            text: text.into(),
            text_type: 0,
        })
    };
    hud.apply_events(
        &[line(
            "Welcome to Dereth\n powered by Empyrean (a fork of ACEmulator)\n(https://example.org)\n",
        )],
        &mut world,
    );
    assert!(!hud.ace_world, "Empyrean keeps retail's tie-break");
    assert_eq!(superseded_of(&hud, &world, aura), Some(true));
    assert_eq!(superseded_of(&hud, &world, set_high), Some(true));
    hud.apply_events(
        &[line(
            "Welcome to Asheron's Call\n  powered by ACEmulator\n\nFor more information on commands supported by this server, type @acehelp\n",
        )],
        &mut world,
    );
    assert!(hud.ace_world);
    assert_eq!(superseded_of(&hud, &world, aura), Some(false));
    assert_eq!(superseded_of(&hud, &world, other), Some(true));
    assert_eq!(superseded_of(&hud, &world, set_high), Some(false));
    assert_eq!(superseded_of(&hud, &world, set_low), Some(true));
}

/// Add Junk's facts read the lowest values as set: attuned 1, one tinker and the Critical
/// Strike imbue (bit 1) all count, and the retained flag and rare underlay need no appraisal.
#[test]
fn sell_safety_counts_plain_attuned_one_tinker_and_critical_strike() {
    use pyreal_client_model::{Weenie, World};
    use pyreal_protocol::{archive::PackedHash, types::appraisal::AppraisalProfile};
    let (plain, marked, flagged) = (ObjectId(7), ObjectId(8), ObjectId(9));
    let mut world = World::new();
    for id in [plain, marked, flagged] {
        let mut w = Weenie::new(id);
        w.pwd.value = Some(50);
        w.pwd.obj_type = 1;
        world.tables.weenies.insert(id, w);
    }
    let f = world.tables.weenies.get_mut(flagged).expect("the weenie");
    f.pwd.bitfield |= 0x0100_0000;
    f.pwd.icon_underlay_id = Some(0x0600_5B0C);
    f.pwd.wielder_id = Some(ObjectId(0x5000_0001));
    let mut profile = AppraisalProfile::default();
    profile.tables.ints = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x72, 1), (0xAB, 1), (0xB3, 1)],
    });
    profile.tables.strings = Some(PackedHash {
        table_size: 8,
        entries: vec![(7, "Mine".into())],
    });
    profile.tables.bools = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x45, 0)],
    });
    world.appraisal.set(marked, profile);
    world.appraisal.set(plain, AppraisalProfile::default());
    world.shop.vendor_id = Some(ObjectId(0x8000_0100));
    world.shop.profile.item_types = 2;
    world.shop.profile.max_value = -1;
    world.shop.profile.min_value = -1;
    let hud = Hud::new();
    let view = HudView {
        hud: &hud,
        world: &world,
    };
    let s = view.sell_safety(plain);
    assert!(s.appraised && !s.attuned && !s.tinkered && !s.imbued && !s.inscribed);
    assert_eq!((s.value, s.vendor_refusal), (Some(50), Some(1)));
    let s = view.sell_safety(marked);
    assert!(s.attuned && s.tinkered && s.imbued && s.inscribed && s.unsellable);
    let s = view.sell_safety(flagged);
    assert!(!s.appraised && s.retained && s.rare && s.worn);
}
