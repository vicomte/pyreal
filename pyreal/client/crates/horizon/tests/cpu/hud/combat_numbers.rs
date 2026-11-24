//! Floating combat numbers: the game's combat lines read as numbers over who took them, stacked
//! without overlapping, answering their switches, and hidden with the interface.

use pyreal_client_contract::actions::pyreal::HIDE_INTERFACE;
use pyreal_client_model::chat::text_type;
use pyreal_client_model::combat::{
    attack_conditions, attacker_notification_line, damage_type, defender_notification_line,
    evasion_attacker_line, evasion_defender_line,
};
use pyreal_horizon::draw::{Anchor, DrawList, Rect};
use pyreal_horizon::options::HorizonOptions;
use pyreal_horizon::ui::combat_numbers::{
    self, killed, parse, resolve, words, CombatNumbers, Element, Hit, Kill, Number, Over, Shown,
    Side, Switches, KILL_LINES, PER_BODY,
};
use pyreal_horizon::ui::game::{Blip, BlipKind, ChatLine, GameState, Nameplate, Relation, Target};
use pyreal_primitives::ObjectId;

use crate::Harness;

const PLAYER: ObjectId = ObjectId(0x5000_0001);
const DRUDGE: ObjectId = ObjectId(0x8300_0701);
const OTHER_DRUDGE: ObjectId = ObjectId(0x8300_0702);
const FAR_DRUDGE: ObjectId = ObjectId(0x8300_0703);

fn hit(over: Over, side: Side, shown: Shown, element: Element, critical: bool) -> Hit {
    Hit {
        over,
        side,
        shown,
        element,
        critical,
        periodic: false,
    }
}

/// A periodic tick's number.
fn tick(over: Over, side: Side, n: u32, element: Element) -> Hit {
    Hit {
        periodic: true,
        ..hit(over, side, Shown::Amount(n), element, false)
    }
}

fn drudge() -> Over {
    Over::Named("Drudge Skulker".into())
}

/// The line as the chat log has it, without its line break.
fn line(s: &str) -> String {
    s.trim_end_matches('\n').to_owned()
}

#[test]
fn the_players_blows_read_as_damage_over_what_they_hit_with_its_type() {
    let melee = attacker_notification_line("Drudge Skulker", damage_type::FIRE, 0.3, 42, false, 0);
    assert_eq!(
        parse(22, &line(&melee)),
        Some(hit(
            drudge(),
            Side::Outgoing,
            Shown::Amount(42),
            Element::Fire,
            false
        ))
    );
    // The client's own critical line has two spaces after its opener, and more openers follow.
    let critical = attacker_notification_line(
        "Drudge Skulker",
        damage_type::SLASH,
        0.6,
        1,
        true,
        attack_conditions::SNEAK_ATTACK,
    );
    assert_eq!(
        parse(22, &line(&critical)),
        Some(hit(
            drudge(),
            Side::Outgoing,
            Shown::Amount(1),
            Element::Physical,
            true
        ))
    );
    // A mixed type says "hit" and names no single type.
    let mixed = attacker_notification_line(
        "Drudge Skulker",
        damage_type::FIRE | damage_type::COLD,
        0.3,
        7,
        false,
        0,
    );
    assert_eq!(
        parse(22, &line(&mixed)).map(|h| h.shown),
        Some(Shown::Amount(7))
    );
}

#[test]
fn spells_and_periodic_damage_read_their_type_from_the_verb() {
    let cases = [
        (
            "Critical hit! You freeze Drudge Skulker for 211 points with Frost Bolt VI.",
            Element::Cold,
            211,
            true,
        ),
        (
            "You jolt Drudge Skulker for 80 points with Lightning Arc V.",
            Element::Electric,
            80,
            false,
        ),
        (
            "You twist Drudge Skulker for 55 points of periodic nether damage!",
            Element::Nether,
            55,
            false,
        ),
        (
            "You bleed Drudge Skulker for 12 points of periodic health damage!",
            Element::Health,
            12,
            false,
        ),
        (
            "With Harm Other VI you drain 64 points of health from Drudge Skulker.",
            Element::Health,
            64,
            false,
        ),
    ];
    for (text, element, n, critical) in cases {
        assert_eq!(
            parse(7, text),
            Some(Hit {
                periodic: text.contains("periodic"),
                ..hit(
                    drudge(),
                    Side::Outgoing,
                    Shown::Amount(n),
                    element,
                    critical,
                )
            }),
            "{text}"
        );
    }
}

#[test]
fn blows_and_spells_at_the_player_read_as_damage_over_the_player() {
    let melee =
        defender_notification_line("Drudge Skulker", damage_type::BLUDGEON, 0.2, 17, 3, true, 0);
    assert_eq!(
        parse(21, &line(&melee)),
        Some(hit(
            Over::Player,
            Side::Incoming,
            Shown::Amount(17),
            Element::Physical,
            true
        ))
    );
    for (text, n) in [
        (
            "Drudge Skulker scorches you for 33 points with Flame Bolt II.",
            33,
        ),
        (
            "Drudge Skulker casts Harm Self and drains 20 points of your health.",
            20,
        ),
        ("You receive 9 points of periodic nether damage.", 9),
        ("You receive 4 points of periodic harm.", 4),
    ] {
        assert_eq!(
            parse(7, text).map(|h| (h.over, h.side, h.shown)),
            Some((Over::Player, Side::Incoming, Shown::Amount(n))),
            "{text}"
        );
    }
}

#[test]
fn evades_and_resists_read_as_words_on_the_side_they_belong_to() {
    let cases = [
        (
            line(&evasion_attacker_line("Drudge Skulker")),
            drudge(),
            Side::Outgoing,
            Shown::Evaded,
        ),
        (
            line(&evasion_defender_line("Drudge Skulker")),
            Over::Player,
            Side::Incoming,
            Shown::Evaded,
        ),
        (
            "Drudge Skulker resists your spell".into(),
            drudge(),
            Side::Outgoing,
            Shown::Resisted,
        ),
        (
            "You resist the spell cast by Drudge Skulker".into(),
            Over::Player,
            Side::Incoming,
            Shown::Resisted,
        ),
    ];
    for (text, over, side, shown) in cases {
        let got = parse(7, &text).expect(&text);
        assert_eq!(
            (got.over.clone(), got.side, got.shown),
            (over, side, shown),
            "{text}"
        );
        assert!(words(&got).starts_with(|c: char| c.is_ascii_uppercase()));
    }
}

#[test]
fn healing_reads_over_who_was_healed() {
    let cases = [
        (
            "You cast Heal Self VI and restore 80 points of your health.",
            Over::Player,
            80,
            false,
        ),
        (
            "With Heal Other VI you restore 75 points of health to Aldric.",
            Over::Named("Aldric".into()),
            75,
            false,
        ),
        (
            "Aldric casts Heal Other VI and restores 70 points of your health.",
            Over::Player,
            70,
            false,
        ),
        (
            "You expertly heal yourself for 112 Health points. Your Healing Kit has 12 uses left.",
            Over::Player,
            112,
            true,
        ),
        (
            "You heal Aldric for 50 Health points.",
            Over::Named("Aldric".into()),
            50,
            false,
        ),
        (
            "Aldric heals you for 48 Health points.",
            Over::Player,
            48,
            false,
        ),
        (
            "You receive 6 points of periodic healing.",
            Over::Player,
            6,
            false,
        ),
        (
            "The Health Elixir restores 40 points of your Health.",
            Over::Player,
            40,
            false,
        ),
    ];
    for (text, over, n, critical) in cases {
        let got = parse(0, text).expect(text);
        assert_eq!(
            (got.over, got.side, got.shown, got.critical),
            (over, Side::Healing, Shown::Amount(n), critical),
            "{text}"
        );
    }
    assert_eq!(
        words(&parse(0, "You expertly heal yourself for 112 Health points.").unwrap()),
        "+112!"
    );
}

#[test]
fn a_timestamped_line_reads_as_the_same_line() {
    assert_eq!(
        parse(
            7,
            "14:02:31 You jolt Drudge Skulker for 80 points with Lightning Arc V."
        ),
        parse(
            7,
            "You jolt Drudge Skulker for 80 points with Lightning Arc V."
        ),
    );
}

#[test]
fn other_lines_read_as_nothing() {
    let none = [
        // Speech, tells and emotes are never read, whatever they say.
        (
            text_type::SPEECH,
            "You slash Drudge Skulker for 999 points of slashing damage!",
        ),
        (
            text_type::SPEECH_DIRECT,
            "Aldric heals you for 999 Health points.",
        ),
        (
            text_type::EMOTE,
            "You slash Drudge Skulker for 999 points with Flame Bolt.",
        ),
        // Stamina and mana are not health.
        (
            text_type::MAGIC,
            "With Drain Stamina you drain 30 points of stamina from Drudge Skulker.",
        ),
        (
            text_type::DEFAULT,
            "You heal yourself for 30 Stamina points.",
        ),
        (
            text_type::MAGIC,
            "You cast Revitalize Self and restore 50 points of your stamina.",
        ),
        // A kill line carries no number.
        (
            text_type::DEFAULT,
            "Your killing blow nearly turns Drudge Skulker inside-out!",
        ),
        (text_type::COMBAT, "You say, \"hello\""),
        (text_type::MAGIC, "You cast Strength Self VI on yourself."),
    ];
    for (ty, text) in none {
        assert_eq!(parse(u8::try_from(ty).unwrap(), text), None, "{text}");
    }
}

/// A plate over `id` at `at`, `distance` metres away.
fn plate(id: ObjectId, name: &str, at: (f32, f32), distance: f32, selected: bool) -> Nameplate {
    Nameplate {
        id,
        name: name.into(),
        x: at.0,
        y: at.1,
        kind: if selected {
            BlipKind::Selected
        } else {
            BlipKind::Creature
        },
        selected,
        bounds: Rect::new(at.0 - 40.0, at.1, 80.0, 160.0),
        distance: Some(distance),
        nearness: pyreal_horizon::state::nearness(distance, true),
    }
}

/// The player in a fight: two Drudge Skulkers near, the further one selected, and a third far
/// off on the radar.
fn fight() -> GameState {
    GameState {
        in_world: true,
        connected: true,
        name: "Tester".into(),
        player_id: Some(PLAYER),
        radar_range: 75.0,
        target: Some(Target {
            id: OTHER_DRUDGE,
            name: "Drudge Skulker".into(),
            relation: Relation::Engaged,
            ..Target::default()
        }),
        nameplates: vec![
            plate(DRUDGE, "Drudge Skulker", (700.0, 420.0), 4.0, false),
            plate(OTHER_DRUDGE, "Drudge Skulker", (1200.0, 400.0), 9.0, true),
        ],
        blips: vec![Blip {
            id: FAR_DRUDGE,
            dx: 70.0,
            dy: 0.0,
            kind: BlipKind::Creature,
            colour: 0,
            shape: 0,
            name: "Drudge Skulker".into(),
        }],
        ..GameState::default()
    }
}

#[test]
fn a_name_stands_over_the_selection_first_then_the_nearest_of_that_name() {
    let mut state = fight();
    assert_eq!(
        resolve(&drudge(), &state),
        Some(OTHER_DRUDGE),
        "the selection"
    );
    state.target = None;
    assert_eq!(
        resolve(&drudge(), &state),
        Some(DRUDGE),
        "the nearest named"
    );
    state.nameplates.clear();
    assert_eq!(resolve(&drudge(), &state), Some(FAR_DRUDGE), "on the radar");
    assert_eq!(resolve(&Over::Named("Tester".into()), &state), Some(PLAYER));
    assert_eq!(resolve(&Over::Player, &state), Some(PLAYER));
    assert_eq!(resolve(&Over::Named("Nobody".into()), &state), None);
}

fn chat(ty: u32, text: &str) -> ChatLine {
    ChatLine {
        chat_type: u8::try_from(ty).unwrap(),
        text: text.into(),
        window: 0,
    }
}

/// The lines of a short exchange: two blows dealt, one taken, an evade and a heal.
fn exchange() -> Vec<ChatLine> {
    vec![
        chat(
            text_type::COMBAT_SELF,
            "You slash Drudge Skulker for 20 points of slashing damage!",
        ),
        chat(
            text_type::MAGIC,
            "You scorch Drudge Skulker for 66 points with Flame Bolt IV.",
        ),
        chat(
            text_type::COMBAT_ENEMY,
            "Drudge Skulker bashes your chest for 9 points of bludgeoning damage!",
        ),
        chat(text_type::COMBAT_SELF, "Drudge Skulker evaded your attack."),
        chat(
            text_type::MAGIC,
            "You cast Heal Self VI and restore 80 points of your health.",
        ),
    ]
}

/// The sides of the numbers in the air.
fn sides(n: &CombatNumbers) -> Vec<Side> {
    n.numbers.iter().map(|n| n.hit.side).collect()
}

#[test]
fn each_switch_keeps_its_own_numbers_out_and_the_first_keeps_all_out() {
    let mut state = fight();
    state.new_chat = exchange();
    let take = |switches: Switches| {
        let mut n = CombatNumbers::default();
        n.switches = switches;
        n.take(&state, 1.0);
        sides(&n)
    };
    let all = Switches::default();
    assert_eq!(
        take(all),
        [
            Side::Outgoing,
            Side::Outgoing,
            Side::Incoming,
            Side::Outgoing,
            Side::Healing
        ]
    );
    assert_eq!(
        take(Switches {
            outgoing: false,
            ..all
        }),
        [Side::Incoming, Side::Healing]
    );
    assert_eq!(
        take(Switches {
            incoming: false,
            ..all
        }),
        [
            Side::Outgoing,
            Side::Outgoing,
            Side::Outgoing,
            Side::Healing
        ]
    );
    assert_eq!(
        take(Switches {
            healing: false,
            ..all
        }),
        [
            Side::Outgoing,
            Side::Outgoing,
            Side::Incoming,
            Side::Outgoing
        ]
    );
    assert!(take(Switches { on: false, ..all }).is_empty());
}

#[test]
fn the_switches_are_kept_in_the_settings_file_and_on_by_default() {
    let defaults = HorizonOptions::default();
    assert_eq!(defaults.combat_numbers, Switches::default());
    assert!(defaults.combat_numbers.on && defaults.combat_numbers.healing);
    let mut options = HorizonOptions::default();
    options.combat_numbers.incoming = false;
    let text = options.to_text();
    assert!(text.contains("combat-numbers=true\n"), "{text}");
    assert!(text.contains("combat-numbers-incoming=false\n"), "{text}");
    assert_eq!(
        HorizonOptions::parse(&text).combat_numbers,
        options.combat_numbers
    );
    let off = HorizonOptions::parse("combat-numbers=false\n").combat_numbers;
    assert!(!off.on && off.outgoing, "{off:?}");
}

#[test]
fn numbers_leave_after_their_life_and_one_body_keeps_only_the_newest() {
    let mut n = CombatNumbers::default();
    let mut state = fight();
    state.new_chat = exchange();
    n.take(&state, 0.0);
    assert_eq!(n.numbers.len(), 5);
    state.new_chat.clear();
    n.take(&state, 1.0);
    assert_eq!(n.numbers.len(), 5, "still in the air");
    n.take(&state, 2.0);
    assert!(n.numbers.is_empty(), "gone after their life");
    let per = u32::try_from(PER_BODY).unwrap();
    for i in 0..(per + 3) {
        n.push(Number {
            id: DRUDGE,
            hit: hit(
                drudge(),
                Side::Outgoing,
                Shown::Amount(i),
                Element::Physical,
                false,
            ),
            born: f64::from(i),
        });
    }
    let kept: Vec<Shown> = n.numbers.iter().map(|n| n.hit.shown).collect();
    assert_eq!(kept.len(), PER_BODY);
    assert_eq!(
        kept.last(),
        Some(&Shown::Amount(per + 2)),
        "the newest kept"
    );
    // Leaving the world lets them all go.
    state.in_world = false;
    n.take(&state, 9.0);
    assert!(n.numbers.is_empty());
}

/// The box round the quads of each number drawn over `id`, in drawing order, after its name's.
fn drawn_over(list: &DrawList, id: ObjectId) -> Vec<Rect> {
    list.anchored
        .iter()
        .filter(|a| a.id == id && a.anchor == Anchor::Top)
        .map(|a| {
            let q = &list.quads[a.quads.clone()];
            let x0 = q.iter().map(|q| q.dst.x).fold(f32::MAX, f32::min);
            let y0 = q.iter().map(|q| q.dst.y).fold(f32::MAX, f32::min);
            let x1 = q.iter().map(|q| q.dst.right()).fold(f32::MIN, f32::max);
            let y1 = q.iter().map(|q| q.dst.bottom()).fold(f32::MIN, f32::max);
            Rect::new(x0, y0, x1 - x0, y1 - y0)
        })
        .collect()
}

fn overlap(a: &Rect, b: &Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

#[test]
fn numbers_over_one_body_stack_clear_of_each_other_and_of_its_name() {
    let mut h = Harness::with_built_in_art();
    let mut state = fight();
    state.new_chat = (0..PER_BODY)
        .map(|i| {
            chat(
                text_type::MAGIC,
                &format!(
                    "You scorch Drudge Skulker for {} points with Flame Bolt IV.",
                    10 + i
                ),
            )
        })
        .collect();
    h.frame(&state);
    state.new_chat.clear();
    for frames in [1, 20, 40] {
        for _ in 0..frames {
            h.frame(&state);
        }
        let boxes = drawn_over(&h.list, OTHER_DRUDGE);
        // The selection's name, then a number for each line.
        assert_eq!(boxes.len(), 1 + PER_BODY, "after {frames} more frames");
        for (i, a) in boxes.iter().enumerate() {
            for b in &boxes[i + 1..] {
                assert!(!overlap(a, b), "{a:?} and {b:?} overlap");
            }
            // Each stands over the point the name stands on.
            assert!(
                a.bottom() <= 400.0 && (a.x + a.w / 2.0 - 1200.0).abs() < 4.0,
                "{a:?}"
            );
        }
    }
}

#[test]
fn numbers_stand_over_the_player_and_hide_with_the_interface() {
    let mut h = Harness::with_built_in_art();
    let mut state = fight();
    state.new_chat = exchange();
    h.frame(&state);
    state.new_chat.clear();
    h.frame(&state);
    // The blow taken and the heal over the player, the rest over the selection.
    assert_eq!(drawn_over(&h.list, PLAYER).len(), 2);
    assert_eq!(drawn_over(&h.list, OTHER_DRUDGE).len(), 1 + 3);
    assert!(
        h.list.anchored.iter().all(|a| a.in_world),
        "hidden behind the world"
    );
    h.input.actions.push(HIDE_INTERFACE.0);
    h.frame(&state);
    h.frame(&state);
    assert!(drawn_over(&h.list, PLAYER).is_empty());
    assert!(drawn_over(&h.list, OTHER_DRUDGE).is_empty());
}

#[test]
fn numbers_fade_with_distance_and_over_a_name_not_shown_are_not_drawn() {
    let alpha = |distance: f32| {
        let mut h = Harness::with_built_in_art();
        let mut state = fight();
        state.target = None;
        state.nameplates = vec![plate(
            DRUDGE,
            "Drudge Skulker",
            (700.0, 420.0),
            distance,
            false,
        )];
        state.new_chat = vec![chat(
            text_type::MAGIC,
            "You scorch Drudge Skulker for 66 points with Flame Bolt IV.",
        )];
        h.frame(&state);
        state.new_chat.clear();
        for _ in 0..20 {
            h.frame(&state);
        }
        let a = h.list.anchored.iter().filter(|a| a.id == DRUDGE).nth(1)?;
        h.list.quads[a.quads.clone()]
            .iter()
            .map(|q| q.colour >> 24)
            .max()
    };
    let near = alpha(5.0).expect("drawn near");
    let middling = alpha(48.0).expect("drawn further off");
    assert!(middling < near, "{middling} < {near}");
    assert_eq!(alpha(59.9), None, "gone where names are");
}

/// Not a claim: what the numbers cost a HUD frame in a fight with ten creatures, printed as the
/// mean microseconds a frame with them off, on in a fight, and on with every creature's stack
/// full. Run it in release under `cargo xtask lock --quiet --`.
#[test]
#[ignore = "an instrument: prints timings"]
fn time_combat_numbers_with_ten_creatures() {
    const FRAMES: u32 = 600;
    let creatures: Vec<(ObjectId, String)> = (0..10u32)
        .map(|i| (ObjectId(0x8300_0900 + i), format!("Drudge Raider {i}")))
        .collect();
    let mut state = fight();
    state.target = None;
    state.nameplates = creatures
        .iter()
        .zip(0u16..)
        .map(|((id, name), i)| {
            let at = (
                300.0 + 140.0 * f32::from(i),
                420.0 + 10.0 * f32::from(i % 3),
            );
            plate(*id, name, at, 6.0 + f32::from(i), false)
        })
        .collect();
    // A fight: each creature strikes the player twice a second, the player strikes one creature
    // twice a second and three with a ring spell each second, and heals every two seconds.
    let fight_lines = |frame: u32| {
        let mut lines = Vec::new();
        for (i, (_, name)) in (0u32..).zip(&creatures) {
            if frame % 30 == i * 3 {
                lines.push(chat(
                    text_type::COMBAT_ENEMY,
                    &format!("{name} slashes your arm for 12 points of slashing damage!"),
                ));
            }
        }
        if frame % 30 == 1 {
            let name = &creatures[0].1;
            lines.push(chat(
                text_type::MAGIC,
                &format!("You freeze {name} for 98 points with Frost Bolt VI."),
            ));
        }
        if frame % 60 == 7 {
            for (_, name) in &creatures[1..4] {
                lines.push(chat(
                    text_type::MAGIC,
                    &format!("You scorch {name} for 61 points with Flame Ring V."),
                ));
            }
        }
        if frame % 120 == 9 {
            lines.push(chat(
                text_type::MAGIC,
                "You cast Heal Self VI and restore 80 points of your health.",
            ));
        }
        lines
    };
    // Every creature struck six times a second, and the player struck ten times a second.
    let full_lines = |frame: u32| {
        let (_, name) = &creatures[(frame % 10) as usize];
        vec![
            chat(
                text_type::MAGIC,
                &format!("Critical hit! You scorch {name} for 161 points with Flame Bolt VI."),
            ),
            chat(
                text_type::COMBAT_ENEMY,
                &format!("{name} slashes you for 12 points with Blade VI."),
            ),
        ]
    };
    // The fight, with every creature taking a nether and a bleed tick every five seconds and one
    // killed every two seconds. Its lines fill chat more, so it has its own time with numbers off.
    let ticks_lines = |frame: u32| {
        let mut lines = fight_lines(frame);
        for (i, (_, name)) in (0u32..).zip(&creatures) {
            if frame % 300 == i * 7 {
                lines.push(chat(
                    text_type::MAGIC,
                    &format!("You twist {name} for 30 points of periodic nether damage!"),
                ));
                lines.push(chat(
                    text_type::COMBAT_SELF,
                    &format!("You bleed {name} for 7 points of periodic health damage!"),
                ));
            }
        }
        if frame % 120 == 60 {
            let (_, name) = &creatures[(frame / 120 % 10) as usize];
            lines.push(chat(
                text_type::DEFAULT,
                &format!("You split {name} apart!"),
            ));
        }
        lines
    };
    let run = |on: bool, lines: &dyn Fn(u32) -> Vec<ChatLine>| {
        let mut h = Harness::with_built_in_art();
        h.ui.options.combat_numbers.on = on;
        let mut state = state.clone();
        for frame in 0..120 {
            state.new_chat = lines(frame);
            h.frame(&state);
        }
        let mut quads = 0;
        let start = std::time::Instant::now();
        for frame in 120..120 + FRAMES {
            state.new_chat = lines(frame);
            h.frame(&state);
            quads += h.list.quads.len();
        }
        let micros = start.elapsed().as_secs_f64() * 1e6 / f64::from(FRAMES);
        (micros, quads / FRAMES as usize)
    };
    let mut best = [(f64::MAX, 0); 5];
    for _ in 0..3 {
        for (i, (on, lines)) in [
            (false, &fight_lines as &dyn Fn(u32) -> Vec<ChatLine>),
            (true, &fight_lines),
            (true, &full_lines),
            (false, &ticks_lines),
            (true, &ticks_lines),
        ]
        .into_iter()
        .enumerate()
        {
            let (us, quads) = run(on, lines);
            if us < best[i].0 {
                best[i] = (us, quads);
            }
        }
    }
    for (name, (us, quads)) in [
        "off",
        "on, a fight",
        "on, every stack full",
        "off, a fight with ticks and kills",
        "on, a fight with ticks and kills",
    ]
    .iter()
    .zip(best)
    {
        println!("combat numbers {name}: {us:.1} us a HUD frame, {quads} quads");
    }
}

#[test]
fn numbers_come_while_combat_text_is_squelched_and_once_when_it_is_not() {
    let blow = "Drudge Skulker bashes your chest for 9 points of bludgeoning damage!";
    let mut h = Harness::with_built_in_art();
    let mut state = fight();
    state.combat_lines = vec![chat(text_type::COMBAT_ENEMY, blow)];
    h.frame(&state);
    assert_eq!(
        h.ui.hud.combat_numbers.numbers.len(),
        1,
        "squelched in chat"
    );
    state.new_chat = vec![chat(text_type::COMBAT_ENEMY, &format!("14:02:31 {blow}"))];
    h.frame(&state);
    assert_eq!(
        h.ui.hud.combat_numbers.numbers.len(),
        2,
        "its echo is not read"
    );
    state.combat_lines.clear();
    h.frame(&state);
    assert_eq!(
        h.ui.hud.combat_numbers.numbers.len(),
        3,
        "a line in chat alone"
    );
}

/// What the numbers over `id` show, oldest first.
fn shown_over(n: &CombatNumbers, id: ObjectId) -> Vec<Shown> {
    n.numbers
        .iter()
        .filter(|n| n.id == id)
        .map(|n| n.hit.shown)
        .collect()
}

#[test]
fn every_kill_line_reads_the_name_and_no_other_line_does() {
    for (template, element, critical) in KILL_LINES {
        let text = template.replace("{0}", "Drudge Skulker");
        assert_eq!(
            killed(0, &format!("14:02:31 {text}")),
            Some(Kill {
                name: "Drudge Skulker".into(),
                element,
                critical,
            }),
            "{text}"
        );
        assert_eq!(parse(0, &text), None, "not a number: {text}");
    }
    // The player's own death is the death recap's, and hit lines and speech are not kills.
    for (template, ..) in pyreal_horizon::ui::death_recap::DEATH_LINES {
        let text = template
            .replace("{1}", "Drudge Skulker")
            .replace("{0}", "Tester");
        assert_eq!(killed(0, &text), None, "{text}");
    }
    for (ty, text) in [
        (
            0,
            "You slash Drudge Skulker for 20 points of slashing damage!",
        ),
        (0, "Drudge Skulker died!"),
        (2, "You split Drudge Skulker apart!"),
        (0, "You killed !"),
    ] {
        assert_eq!(killed(ty, text), None, "{text}");
    }
}

#[test]
fn a_kill_with_no_damage_line_shows_killed_over_the_body_last_struck() {
    let mut n = CombatNumbers::default();
    let mut state = fight();
    // A blow on the selection, then the selection moves to the other of the same name.
    state.new_chat = vec![chat(
        text_type::MAGIC,
        "You scorch Drudge Skulker for 66 points with Flame Bolt IV.",
    )];
    n.take(&state, 0.0);
    state.target.as_mut().unwrap().id = DRUDGE;
    // The killing blow sends only its kill line, of chat type 0.
    state.new_chat = vec![chat(text_type::DEFAULT, "You obliterate Drudge Skulker!")];
    n.take(&state, 3.0);
    assert_eq!(
        shown_over(&n, OTHER_DRUDGE),
        [Shown::Killed],
        "the one struck"
    );
    let mark = &n.numbers[0].hit;
    assert_eq!(
        (mark.side, mark.element, mark.critical),
        (Side::Outgoing, Element::Fire, true),
        "a critical line takes the type of the last blow"
    );
    assert_eq!(words(mark), "Killed!");
    // A kill never struck before stands where a name would.
    state.new_chat = vec![chat(text_type::DEFAULT, "You split Drudge Skulker apart!")];
    n.take(&state, 3.1);
    assert_eq!(shown_over(&n, DRUDGE), [Shown::Killed]);
    assert_eq!(words(&n.numbers[1].hit), "Killed");
    // It lasts as a critical does, and answers the damage-you-deal switch.
    state.new_chat.clear();
    n.take(&state, 3.0 + combat_numbers::LIFE + 0.2);
    assert_eq!(n.numbers.len(), 2);
    n.take(&state, 3.2 + combat_numbers::CRITICAL_LIFE);
    assert!(n.numbers.is_empty());
    n.switches.outgoing = false;
    state.new_chat = vec![chat(text_type::DEFAULT, "You run Drudge Skulker through!")];
    n.take(&state, 10.0);
    assert!(n.numbers.is_empty());
    // The player's own name is never marked.
    n.switches.outgoing = true;
    state.new_chat = vec![chat(text_type::DEFAULT, "You killed Tester!")];
    n.take(&state, 11.0);
    assert!(n.numbers.is_empty());
}

#[test]
fn a_kill_after_a_damage_line_shows_the_number_then_killed_drawn_apart() {
    let mut h = Harness::with_built_in_art();
    let mut state = fight();
    state.combat_lines = vec![chat(
        text_type::COMBAT_SELF,
        &line(&attacker_notification_line(
            "Drudge Skulker",
            damage_type::SLASH,
            0.5,
            40,
            false,
            0,
        )),
    )];
    state.new_chat = vec![chat(
        text_type::DEFAULT,
        "14:02:31 Your killing blow nearly turns Drudge Skulker inside-out!",
    )];
    h.frame(&state);
    state.combat_lines.clear();
    state.new_chat.clear();
    h.frame(&state);
    let n = &h.ui.hud.combat_numbers;
    assert_eq!(
        shown_over(n, OTHER_DRUDGE),
        [Shown::Amount(40), Shown::Killed]
    );
    let boxes = drawn_over(&h.list, OTHER_DRUDGE);
    // The selection's name, the number and the mark, clear of each other.
    assert_eq!(boxes.len(), 3);
    assert!(!overlap(&boxes[1], &boxes[2]), "{boxes:?}");
}

#[test]
fn periodic_ticks_on_one_body_add_up_beside_its_hits() {
    let mut n = CombatNumbers::default();
    let mut state = fight();
    let nether = |amount: u32| {
        chat(
            text_type::MAGIC,
            &format!("You twist Drudge Skulker for {amount} points of periodic nether damage!"),
        )
    };
    let bleed = chat(
        text_type::COMBAT_SELF,
        "You bleed Drudge Skulker for 7 points of periodic health damage!",
    );
    let blow = chat(
        text_type::COMBAT_SELF,
        "You slash Drudge Skulker for 20 points of slashing damage!",
    );
    let received = chat(
        text_type::COMBAT,
        "You receive 9 points of periodic nether damage.",
    );
    // A heartbeat: three nether ticks, a bleed and a blow together, and one on the player.
    state.new_chat = vec![
        nether(30),
        blow.clone(),
        nether(25),
        bleed.clone(),
        nether(5),
    ];
    state.new_chat.push(received.clone());
    n.take(&state, 0.0);
    // Ticks a frame later still add up.
    state.new_chat = vec![nether(10), received.clone()];
    n.take(&state, 0.1);
    assert_eq!(
        n.numbers
            .iter()
            .map(|n| (n.id, n.hit.clone()))
            .collect::<Vec<_>>(),
        [
            (
                OTHER_DRUDGE,
                tick(drudge(), Side::Outgoing, 70, Element::Nether)
            ),
            (
                OTHER_DRUDGE,
                hit(
                    drudge(),
                    Side::Outgoing,
                    Shown::Amount(20),
                    Element::Physical,
                    false
                )
            ),
            (
                OTHER_DRUDGE,
                tick(drudge(), Side::Outgoing, 7, Element::Health)
            ),
            (
                PLAYER,
                tick(Over::Player, Side::Incoming, 18, Element::Nether)
            ),
        ]
    );
    // Hits never add up, and ticks past the window start a new number.
    state.new_chat = vec![blow.clone(), blow, nether(12)];
    n.take(&state, 0.7);
    assert_eq!(
        shown_over(&n, OTHER_DRUDGE),
        [
            Shown::Amount(70),
            Shown::Amount(20),
            Shown::Amount(7),
            Shown::Amount(20),
            Shown::Amount(20),
            Shown::Amount(12),
        ]
    );
    // A stream of ticks every five seconds beside blows keeps one tick number at a time, so the
    // blows keep their places.
    let mut most_ticks = 0;
    for second in 2..60 {
        let now = f64::from(second);
        state.new_chat = vec![chat(
            text_type::COMBAT_SELF,
            "You slash Drudge Skulker for 20 points of slashing damage!",
        )];
        if second % 5 == 0 {
            state
                .new_chat
                .extend([nether(30), nether(25), bleed.clone()]);
        }
        n.take(&state, now);
        state.new_chat.clear();
        n.take(&state, now + 0.5);
        let over = n.numbers.iter().filter(|n| n.id == OTHER_DRUDGE);
        most_ticks = most_ticks.max(over.clone().filter(|n| n.hit.periodic).count());
        assert!(over.clone().count() <= PER_BODY);
        assert!(over.clone().any(|n| !n.hit.periodic), "the blow shows");
    }
    assert_eq!(most_ticks, 2, "one nether and one bleed");
}
