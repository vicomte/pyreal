//! The death recap: the rolling record of hits taken, the recap a death makes (named by the game's
//! own line, or by the last hit, or with no hits at all), the Journal's Deaths tab it opens, its
//! key, menu entry and setting, and hiding with the interface.

use pyreal_client_contract::actions::pyreal::{HIDE_INTERFACE, TOGGLE_DEATH_RECAP};
use pyreal_client_model::chat::text_type;
use pyreal_horizon::options::HorizonOptions;
use pyreal_horizon::ui::combat_numbers::{Element, Over};
use pyreal_horizon::ui::death_recap::{
    death_line, loss_line, taken, Killing, Kind, LossLine, Recap, DEATH_LINES, KEEP_DEATHS,
    KEEP_ENTRIES, KEEP_SECONDS, LOSS_SECONDS, OPENS_KEY,
};
use pyreal_horizon::ui::game::{ChatLine, GameState, Vital};
use pyreal_horizon::ui::panels::options::tab;
use pyreal_horizon::ui::panels::{JournalTab, WindowId};

use crate::Harness;

fn chat(ty: u32, text: &str) -> ChatLine {
    ChatLine {
        chat_type: u8::try_from(ty).unwrap(),
        text: text.into(),
        window: 0,
    }
}

fn blow(by: &str, n: u32) -> ChatLine {
    chat(
        text_type::COMBAT_ENEMY,
        &format!("{by} slashes your chest for {n} points of slashing damage!"),
    )
}

fn world(deaths: i32) -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        address: "127.0.0.1:9000".into(),
        here: Some((40.4, 34.5)),
        health: Vital {
            current: 80,
            max: 100,
        },
        deaths: Some(deaths),
        ..GameState::default()
    }
}

/// One frame with `lines` in chat.
fn say(h: &mut Harness, state: &mut GameState, lines: Vec<ChatLine>) {
    state.new_chat = lines;
    h.frame(state);
    state.new_chat.clear();
}

/// `seconds` of frames at 60 a second.
fn run(h: &mut Harness, state: &GameState, seconds: f64) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    for _ in 0..(seconds * 60.0).round() as usize {
        h.frame(state);
    }
}

/// The interface with Open Journal on Death switched on.
fn opening() -> Harness {
    let mut h = Harness::with_built_in_art();
    h.ui.options.death_recap_opens = true;
    h
}

/// Whether the Journal is open at its Deaths tab.
fn deaths_shown(h: &Harness) -> bool {
    h.ui.windows.is_open(WindowId::Journal) && h.ui.windows.journal_page() == JournalTab::Deaths
}

fn recap(h: &Harness) -> &Recap {
    h.ui.windows.death_recap.last.as_ref().expect("a recap")
}

/// A fight: two blows from a drudge, a heal, then a blow from an olthoi a second apart, at
/// falling health.
fn fight(h: &mut Harness, state: &mut GameState) {
    h.frame(state);
    for (line, health) in [
        (blow("Drudge Skulker", 22), 58),
        (
            chat(
                text_type::MAGIC,
                "You cast Heal Self VI and restore 30 points of your health.",
            ),
            88,
        ),
        (blow("Drudge Skulker", 40), 48),
        (
            chat(
                text_type::COMBAT_ENEMY,
                "Critical hit! Olthoi Soldier crushes your head for 48 points of bludgeoning damage!",
            ),
            0,
        ),
    ] {
        state.health.current = health;
        say(h, state, vec![line]);
        run(h, state, 59.0 / 60.0);
    }
}

#[test]
fn the_record_keeps_hits_taken_for_fifteen_seconds_and_at_most_twenty() {
    let mut h = opening();
    let mut state = world(3);
    say(
        &mut h,
        &mut state,
        vec![
            blow("Drudge Skulker", 12),
            chat(
                text_type::MAGIC,
                "Drudge Sorcerer casts Drain Health and drains 20 points of your health.",
            ),
            chat(text_type::MAGIC, "Aldric heals you for 48 Health points."),
            chat(
                text_type::MAGIC,
                "You receive 9 points of periodic nether damage.",
            ),
            // Not taken: a blow dealt and a heal given. An evade is kept, and takes nothing.
            chat(
                text_type::COMBAT_SELF,
                "You slash Drudge Skulker for 20 points of slashing damage!",
            ),
            chat(text_type::COMBAT_SELF, "You evaded Drudge Skulker!"),
            chat(text_type::MAGIC, "You heal Aldric for 50 Health points."),
        ],
    );
    let record: Vec<_> =
        h.ui.windows
            .death_recap
            .record
            .iter()
            .map(|t| (t.source().to_owned(), t.kind, t.amount, t.element))
            .collect();
    assert_eq!(
        record,
        [
            ("Drudge Skulker".into(), Kind::Damage, 12, Element::Physical),
            ("Drudge Sorcerer".into(), Kind::Drain, 20, Element::Health),
            ("Aldric".into(), Kind::Healing, 48, Element::Health),
            ("Unknown".into(), Kind::Damage, 9, Element::Nether),
            ("Drudge Skulker".into(), Kind::Evaded, 0, Element::Physical),
        ]
    );
    // Twenty-five blows: the newest twenty are kept.
    for n in 1..=25 {
        say(&mut h, &mut state, vec![blow("Drudge Skulker", n)]);
    }
    let kept = &h.ui.windows.death_recap.record;
    assert_eq!(kept.len(), KEEP_ENTRIES);
    assert_eq!(kept.front().map(|t| t.amount), Some(6));
    assert_eq!(kept.back().map(|t| t.amount), Some(25));
    // Fifteen seconds on, they have all gone.
    run(&mut h, &state, KEEP_SECONDS + 0.5);
    assert!(h.ui.windows.death_recap.record.is_empty());
    assert!(h.ui.windows.death_recap.last.is_none(), "no death");
}

#[test]
fn a_death_with_the_game_s_line_names_its_killer_and_opens_the_deaths_tab() {
    let mut h = opening();
    let mut state = world(3);
    fight(&mut h, &mut state);
    assert!(!deaths_shown(&h));
    state.deaths = Some(4);
    say(
        &mut h,
        &mut state,
        vec![chat(
            text_type::DEFAULT,
            "Olthoi Soldier beats you to a lifeless pulp!",
        )],
    );
    assert!(deaths_shown(&h));
    let r = recap(&h);
    assert_eq!(r.killer(), Some("Olthoi Soldier"));
    assert_eq!(r.heading(), "Killed by Olthoi Soldier");
    assert_eq!(r.detail(), "Olthoi Soldier beats you to a lifeless pulp!");
    let rows: Vec<_> = r
        .taken
        .iter()
        .map(|t| (t.source().to_owned(), t.amount_words(), t.type_words()))
        .collect();
    assert_eq!(
        rows,
        [
            ("Drudge Skulker".into(), "-22".into(), "Physical".into()),
            ("You".into(), "+30".into(), "Healing".into()),
            ("Drudge Skulker".into(), "-40".into(), "Physical".into()),
            (
                "Olthoi Soldier".into(),
                "-48".into(),
                "Physical, critical, killing blow".into()
            ),
        ]
    );
    assert_eq!(r.damage_total(), 110);
    // The seconds before the death: the last blow a second before it, the first four.
    let before: Vec<String> = r
        .taken
        .iter()
        .map(|t| pyreal_horizon::ui::death_recap::before_words(r.at, t))
        .collect();
    assert_eq!(before, ["4.0s", "3.0s", "2.0s", "1.0s"]);
    // The health over those seconds, falling to nothing at the end.
    assert!(r.health.len() >= 16, "{} samples", r.health.len());
    assert_eq!(r.health.last().copied(), Some(0.0));
    assert!(r.health.iter().any(|h| (*h - 0.88).abs() < 1e-6));
    // The record starts afresh after the death.
    assert!(h.ui.windows.death_recap.record.is_empty());
    h.frame(&state);
}

#[test]
fn a_death_with_only_hits_names_the_last_one_who_dealt_damage() {
    let mut h = opening();
    let mut state = world(3);
    fight(&mut h, &mut state);
    // A heal after the last blow names nobody.
    say(
        &mut h,
        &mut state,
        vec![chat(
            text_type::MAGIC,
            "Aldric heals you for 10 Health points.",
        )],
    );
    state.deaths = Some(4);
    h.frame(&state);
    assert!(deaths_shown(&h));
    let r = recap(&h);
    assert_eq!(r.killer(), Some("Olthoi Soldier"));
    assert_eq!(r.line, None);
    assert_eq!(r.detail(), "Named from the last hit you took.");
    // The game's line a moment later is the same death: it names the killer, and no second
    // recap is made.
    let at = r.at;
    run(&mut h, &state, 0.5);
    say(
        &mut h,
        &mut state,
        vec![chat(
            text_type::DEFAULT,
            "14:02:31 You catch Mosswart Elder's attack, with dire consequences!",
        )],
    );
    let r = recap(&h);
    assert_eq!(r.at, at);
    assert_eq!(r.killer(), Some("Mosswart Elder"));
    assert_eq!(
        r.detail(),
        "You catch Mosswart Elder's attack, with dire consequences!"
    );
}

#[test]
fn a_death_with_no_hits_says_so() {
    let mut h = opening();
    let mut state = world(3);
    run(&mut h, &state, 1.0);
    state.deaths = Some(4);
    h.frame(&state);
    // Only the killing blow, worked out from the health left.
    let r = recap(&h);
    let blows: Vec<_> = r
        .taken
        .iter()
        .map(|t| (t.killing, t.amount, t.source().to_owned(), t.type_words()))
        .collect();
    assert_eq!(
        blows,
        [(
            Some(Killing::FromHealth),
            80,
            "Unknown".into(),
            "Killing blow".into()
        )]
    );
    assert_eq!(r.killer(), None);
    assert_eq!(r.heading(), "You died");
    assert_eq!(r.detail(), "No hits were seen before your death.");
    // With no health known there is nothing at all.
    let mut h = opening();
    let mut state = world(3);
    state.health = Vital::default();
    h.frame(&state);
    state.deaths = Some(4);
    h.frame(&state);
    assert!(recap(&h).taken.is_empty());
    assert_eq!(recap(&h).detail(), "No hits were seen before your death.");
    // The game's line with no killer is a death too.
    let mut h = opening();
    let mut state = world(3);
    h.frame(&state);
    say(
        &mut h,
        &mut state,
        vec![chat(text_type::DEFAULT, "You died!")],
    );
    assert_eq!(recap(&h).heading(), "You died");
    assert_eq!(recap(&h).detail(), "No hits were seen before your death.");
    assert!(deaths_shown(&h));
    run(&mut h, &state, 0.2);
}

#[test]
fn the_first_death_count_is_no_death_and_leaving_the_world_forgets_the_recap() {
    let mut h = opening();
    let mut state = world(3);
    h.frame(&state);
    assert!(h.ui.windows.death_recap.last.is_none(), "logging in");
    state.deaths = Some(4);
    h.frame(&state);
    assert!(h.ui.windows.death_recap.last.is_some());
    h.frame(&GameState::default());
    assert!(h.ui.windows.death_recap.last.is_none());
    // Back in with a higher count than before: still no death.
    state.deaths = Some(5);
    h.frame(&state);
    assert!(h.ui.windows.death_recap.last.is_none());
}

#[test]
fn the_game_s_death_lines_read_their_killer() {
    assert_eq!(DEATH_LINES.len(), 38);
    let cases = [
        ("Drudge Skulker splits you apart!", Some("Drudge Skulker")),
        (
            "You are fatally punctured by Banderling Guard!",
            Some("Banderling Guard"),
        ),
        (
            "Blistered by Lightning Mite's lightning, you die!",
            Some("Lightning Mite"),
        ),
        (
            "The thunder of Lugian Brute crushing Tester is followed by the deafening silence of your death!",
            Some("Lugian Brute"),
        ),
        ("You were killed by Aldric!", Some("Aldric")),
        ("You died!", None),
    ];
    for (text, killer) in cases {
        let death = death_line(0, text, "Tester").expect(text);
        assert_eq!(death.killer.as_deref(), killer, "{text}");
        assert_eq!(death.line, text);
    }
    // Each line tells the killing blow's type, or that it was critical.
    let told = |text| death_line(0, text, "Tester").map(|d| (d.element, d.critical));
    assert_eq!(
        told("Banderling Guard runs you through!"),
        Some((Some(Element::Physical), false))
    );
    assert_eq!(
        told("Your last strength withers before Shadow Child!"),
        Some((Some(Element::Nether), false))
    );
    assert_eq!(told("Drudge Skulker obliterates you!"), Some((None, true)));
    assert_eq!(told("You were killed by Aldric!"), Some((None, false)));
    // Another's death, a line in speech, and a blow are not the player's death.
    for (ty, text) in [
        (0, "Drudge Skulker splits Aldric apart!"),
        (
            0,
            "The thunder of Lugian Brute crushing Aldric is followed by the deafening silence of your death!",
        ),
        (2, "Drudge Skulker splits you apart!"),
        (0, "You killed Drudge Skulker!"),
    ] {
        assert_eq!(death_line(ty, text, "Tester"), None, "{text}");
    }
    assert_eq!(
        taken(21, "You evaded Drudge Skulker!", 0.0).map(|t| (t.kind, t.amount)),
        Some((Kind::Evaded, 0)),
        "an evade takes nothing"
    );
    assert_eq!(
        taken(
            7,
            "You cast Harm Self and drain 15 points of your health.",
            0.0
        )
        .map(|t| (t.by, t.kind)),
        Some((Some(Over::Player), Kind::Drain))
    );
}

#[test]
fn the_setting_keeps_the_journal_shut_and_the_key_opens_it_at_deaths() {
    let mut options = HorizonOptions {
        death_recap_opens: false,
        ..HorizonOptions::default()
    };
    assert!(!HorizonOptions::parse(&options.to_text()).death_recap_opens);
    options.death_recap_opens = true;
    assert!(HorizonOptions::parse(&options.to_text()).death_recap_opens);
    assert!(
        !HorizonOptions::default().death_recap_opens,
        "off by default"
    );

    let mut h = Harness::with_built_in_art();
    let mut state = world(3);
    fight(&mut h, &mut state);
    state.deaths = Some(4);
    h.frame(&state);
    assert!(h.ui.windows.death_recap.last.is_some());
    assert!(!h.ui.windows.is_open(WindowId::Journal));
    // The key opens the Journal at Deaths and shuts it.
    assert!(pyreal_horizon::ui::takes_action(TOGGLE_DEATH_RECAP.0));
    h.input.actions.push(TOGGLE_DEATH_RECAP.0);
    h.frame(&state);
    assert!(deaths_shown(&h));
    h.input.actions.push(TOGGLE_DEATH_RECAP.0);
    h.frame(&state);
    assert!(!h.ui.windows.is_open(WindowId::Journal));
    // With the Journal open at another tab, the key turns it to Deaths.
    h.ui.windows.open(WindowId::Journal, 0.0);
    h.ui.windows.show_journal_tab(JournalTab::Notebook);
    h.frame(&state);
    h.input.actions.push(TOGGLE_DEATH_RECAP.0);
    h.frame(&state);
    assert!(deaths_shown(&h));
    h.input.actions.push(TOGGLE_DEATH_RECAP.0);
    h.frame(&state);
    // Long after, the same recap shows again.
    run(&mut h, &state, 30.0);
    h.input.actions.push(TOGGLE_DEATH_RECAP.0);
    h.frame(&state);
    assert!(deaths_shown(&h));
    assert_eq!(recap(&h).killer(), Some("Olthoi Soldier"));
    // The next death comes first, and the one before is kept under it.
    say(&mut h, &mut state, vec![blow("Mosswart Elder", 99)]);
    state.deaths = Some(5);
    h.frame(&state);
    assert_eq!(recap(&h).killer(), Some("Mosswart Elder"));
    assert_eq!(recap(&h).taken.len(), 1);
    let killers: Vec<_> =
        h.ui.windows
            .death_recap
            .deaths()
            .map(|r| r.killer().map(str::to_owned))
            .collect();
    assert_eq!(
        killers,
        [
            Some("Mosswart Elder".to_owned()),
            Some("Olthoi Soldier".to_owned())
        ]
    );
    assert_eq!(
        h.ui.windows.chosen_death().and_then(Recap::killer),
        Some("Mosswart Elder")
    );
}

#[test]
fn the_display_page_switches_it_on() {
    let mut h = Harness::with_built_in_art();
    let state = world(3);
    h.frame(&state);
    h.ui.windows.open(WindowId::Options, 0.0);
    h.ui.windows.options_page.tab = tab::INTERFACE;
    h.frame(&state);
    let caption = "Open Journal on Death";
    let mut found = None;
    for _ in 0..2000 {
        let page = &h.ui.windows.options_page;
        if let (Some(list), Some(r)) = (page.control("list"), page.control(caption)) {
            if r.y >= list.y && r.bottom() <= list.bottom() {
                found = Some(r);
                break;
            }
        }
        h.ui.windows.options_page.scroll[tab::INTERFACE] += 6.0;
        h.frame(&state);
    }
    let r = found.expect("the Open Journal on Death row");
    h.move_to(r.x + 10.0, r.y + r.h / 2.0);
    h.press();
    let out = h.frame(&state);
    h.release();
    h.frame(&state);
    assert!(
        out.settings
            .iter()
            .any(|(k, v)| k == OPENS_KEY && v == "true"),
        "{:?}",
        out.settings
    );
}

/// Where the Deaths tab's Set Waypoint to Corpse stands, by its tooltip at the right of the
/// Journal's foot.
fn corpse_button(h: &mut Harness, state: &GameState) -> Option<(f32, f32)> {
    let k = h.ui.layout_scale(h.screen);
    let (_, w) =
        h.ui.windows
            .rects(k)
            .into_iter()
            .find(|(id, _)| *id == WindowId::Journal)?;
    let mut y = w.bottom() - 4.0;
    while y > w.bottom() - 60.0 * k {
        for x in [w.right() - 60.0 * k, w.right() - 100.0 * k] {
            h.move_to(x, y);
            h.frame(state);
            if h.ui.windows.tip().is_some_and(|(t, _)| t == "Your corpse") {
                return Some((x, y));
            }
        }
        y -= 3.0;
    }
    None
}

/// Where the Deaths tab lists the death whose heading is `heading`, by its tooltip down the left
/// of the Journal.
fn death_row(h: &mut Harness, state: &GameState, heading: &str) -> Option<(f32, f32)> {
    let k = h.ui.layout_scale(h.screen);
    let (_, w) =
        h.ui.windows
            .rects(k)
            .into_iter()
            .find(|(id, _)| *id == WindowId::Journal)?;
    let x = w.x + 80.0 * k;
    let mut y = w.y + 60.0 * k;
    while y < w.bottom() {
        h.move_to(x, y);
        h.frame(state);
        if h.ui.windows.tip().is_some_and(|(t, _)| t == heading) {
            return Some((x, y));
        }
        y += 6.0;
    }
    None
}

#[test]
fn set_waypoint_to_corpse_leads_back_outside_a_dungeon_from_the_newest_death() {
    let mut h = opening();
    let mut state = world(3);
    fight(&mut h, &mut state);
    state.deaths = Some(4);
    h.frame(&state);
    // The corpse is marked once the game says the death cost something.
    say(
        &mut h,
        &mut state,
        vec![chat(text_type::DEFAULT, "You've lost your Short Sword!")],
    );
    assert!(h.ui.windows.waypoints.corpse.active.is_some());
    // Risen at the lifestone, away from the corpse.
    state.here = Some((40.4, 33.0));
    run(&mut h, &state, 0.5);
    let (x, y) = corpse_button(&mut h, &state).expect("Set Waypoint to Corpse");
    h.move_to(x, y);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    let w = h.ui.windows.waypoints.active.as_ref().expect("a waypoint");
    assert_eq!(w.label(), "Corpse of Tester");
    // The key shuts the Journal, and the recap is kept.
    h.input.actions.push(TOGGLE_DEATH_RECAP.0);
    h.frame(&state);
    assert!(!h.ui.windows.is_open(WindowId::Journal));
    assert!(h.ui.windows.death_recap.last.is_some(), "the recap is kept");

    // In a dungeon there is no waypoint to the corpse.
    let mut h = opening();
    let mut state = world(3);
    state.in_dungeon = true;
    fight(&mut h, &mut state);
    state.deaths = Some(4);
    h.frame(&state);
    run(&mut h, &state, 0.5);
    assert!(deaths_shown(&h));
    assert!(recap(&h).in_dungeon);
    assert_eq!(corpse_button(&mut h, &state), None);
}

#[test]
fn the_deaths_tab_lists_the_last_deaths_newest_first_and_shows_the_one_chosen() {
    let mut h = opening();
    let mut state = world(3);
    h.frame(&state);
    // A death to a drudge, then one to a mosswart a minute on.
    say(&mut h, &mut state, vec![blow("Drudge Skulker", 25)]);
    state.deaths = Some(4);
    h.frame(&state);
    run(&mut h, &state, 5.0);
    say(
        &mut h,
        &mut state,
        vec![chat(text_type::DEFAULT, "You've lost your Short Sword!")],
    );
    run(&mut h, &state, 60.0);
    say(&mut h, &mut state, vec![blow("Mosswart Elder", 40)]);
    state.deaths = Some(5);
    h.frame(&state);
    say(
        &mut h,
        &mut state,
        vec![chat(text_type::DEFAULT, "You've lost 12 Pyreals!")],
    );
    state.here = Some((40.4, 33.0));
    run(&mut h, &state, 0.5);
    assert!(deaths_shown(&h));
    assert_eq!(
        h.ui.windows.chosen_death().and_then(Recap::killer),
        Some("Mosswart Elder"),
        "the newest first"
    );
    assert!(corpse_button(&mut h, &state).is_some());
    // Choosing the earlier death shows it, with no waypoint to a corpse long gone.
    let (x, y) = death_row(&mut h, &state, "Killed by Drudge Skulker").expect("the earlier row");
    h.move_to(x, y);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    let chosen = h.ui.windows.chosen_death().expect("a death chosen");
    assert_eq!(chosen.killer(), Some("Drudge Skulker"));
    assert_eq!(
        chosen.loss.as_ref().map(|l| l.items.clone()),
        Some(vec!["Short Sword".to_owned()])
    );
    assert_eq!(corpse_button(&mut h, &state), None);
    // A new death is the one chosen.
    run(&mut h, &state, 5.0);
    say(&mut h, &mut state, vec![blow("Olthoi Soldier", 60)]);
    state.deaths = Some(6);
    h.frame(&state);
    assert_eq!(
        h.ui.windows.chosen_death().and_then(Recap::killer),
        Some("Olthoi Soldier")
    );

    // No more than the last ten are kept, and leaving the world lets them all go.
    for n in 7..7 + KEEP_DEATHS {
        run(&mut h, &state, 5.0);
        state.deaths = Some(i32::try_from(n).unwrap());
        h.frame(&state);
    }
    let at: Vec<f64> = h.ui.windows.death_recap.deaths().map(|r| r.at).collect();
    assert_eq!(at.len(), KEEP_DEATHS);
    assert!(at.windows(2).all(|w| w[0] > w[1]), "newest first");
    h.frame(&GameState::default());
    assert_eq!(h.ui.windows.death_recap.deaths().count(), 0);
}

#[test]
fn ago_words_say_how_long_ago_a_death_came() {
    use pyreal_horizon::ui::panels::recap::ago_words;
    assert_eq!(ago_words(12.0), "Just now");
    assert_eq!(ago_words(75.0), "1 minute ago");
    assert_eq!(ago_words(59.0 * 60.0), "59 minutes ago");
    assert_eq!(ago_words(61.0 * 60.0), "1 hour ago");
    assert_eq!(ago_words(3.0 * 3600.0), "3 hours ago");
}

#[test]
fn it_hides_with_the_interface() {
    let mut h = opening();
    let mut state = world(3);
    fight(&mut h, &mut state);
    state.deaths = Some(4);
    state.health.current = 100;
    h.frame(&state);
    run(&mut h, &state, 0.5);
    assert!(deaths_shown(&h));
    let shown = h.list.quads.len();
    h.input.actions.push(HIDE_INTERFACE.0);
    h.frame(&state);
    run(&mut h, &state, 5.0);
    assert!(h.ui.hidden.is_some());
    assert!(
        h.list.quads.is_empty(),
        "{} of {shown} quads drawn",
        h.list.quads.len()
    );
    assert!(deaths_shown(&h), "kept open");
}

#[test]
fn the_system_menu_has_no_death_recap() {
    let mut h = opening();
    let state = world(3);
    let bar_y = 1080.0 - 14.0 - 24.0;
    let mut system = None;
    let mut x = 1500.0;
    while x < 1910.0 && system.is_none() {
        h.move_to(x, bar_y);
        h.frame(&state);
        h.frame(&state);
        if h.ui
            .hud
            .shown_tip()
            .is_some_and(|(t, _)| t == "System" || t.starts_with("System ("))
        {
            system = Some(x);
        }
        x += 4.0;
    }
    let x = system.expect("the System medallion");
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    let mut rows: Vec<String> = Vec::new();
    let mut y = bar_y - 20.0;
    while y > 700.0 {
        h.move_to(x - 10.0, y);
        h.frame(&state);
        if let Some((t, _)) = h.ui.hud.shown_tip() {
            if rows.last().map(String::as_str) != Some(t) {
                rows.push(t.to_string());
            }
        }
        y -= 2.0;
    }
    assert!(rows.iter().any(|t| t.starts_with("Settings")), "{rows:?}");
    assert!(!rows.iter().any(|t| t.contains("Death")), "{rows:?}");
}

#[test]
fn the_killing_blow_is_worked_out_from_the_health_left_and_the_death_line() {
    let mut h = opening();
    let mut state = world(3);
    h.frame(&state);
    state.health.current = 58;
    say(&mut h, &mut state, vec![blow("Drudge Skulker", 22)]);
    run(&mut h, &state, 1.0);
    // The game sends no line for the blow that kills: only the death line and the count.
    state.health.current = 0;
    state.deaths = Some(4);
    say(
        &mut h,
        &mut state,
        vec![chat(
            text_type::DEFAULT,
            "Olthoi Soldier brings you to a fiery end!",
        )],
    );
    let r = recap(&h);
    let last = r.taken.last().expect("the killing blow");
    assert_eq!(last.killing, Some(Killing::FromHealth));
    assert_eq!(
        (last.source(), last.amount, last.type_words()),
        ("Olthoi Soldier", 58, "Fire, killing blow".to_owned())
    );
    assert_eq!(
        pyreal_horizon::ui::death_recap::before_words(r.at, last),
        "0.0s"
    );
    assert_eq!(r.damage_total(), 80);
    assert_eq!(r.taken.len(), 2);

    // A critical one, the count first and the line a moment after: the line fills it in.
    let mut h = opening();
    let mut state = world(3);
    h.frame(&state);
    say(&mut h, &mut state, vec![blow("Mosswart Elder", 30)]);
    state.health.current = 50;
    run(&mut h, &state, 1.0);
    state.health.current = 0;
    state.deaths = Some(4);
    h.frame(&state);
    let last = recap(&h).taken.last().cloned().expect("the killing blow");
    assert_eq!(
        (last.source().to_owned(), last.amount),
        ("Mosswart Elder".to_owned(), 50),
        "the last dealer, before the line"
    );
    run(&mut h, &state, 0.5);
    say(
        &mut h,
        &mut state,
        vec![chat(
            text_type::DEFAULT,
            "Mosswart Chief smites you mightily!",
        )],
    );
    let r = recap(&h);
    let last = r.taken.last().expect("the killing blow");
    assert_eq!(
        (last.source(), last.type_words()),
        ("Mosswart Chief", "Killing blow, critical".to_owned())
    );
    assert_eq!(r.killer(), Some("Mosswart Chief"));
}

#[test]
fn hits_are_kept_while_combat_text_is_squelched_and_counted_once_when_it_is_not() {
    let line = "Drudge Skulker slashes your chest for 12 points of slashing damage!";
    let mut h = opening();
    let mut state = world(3);
    h.frame(&state);
    // Squelched: the line reaches only the combat readers.
    state.combat_lines = vec![chat(text_type::COMBAT_ENEMY, line)];
    h.frame(&state);
    state.combat_lines.clear();
    assert_eq!(h.ui.windows.death_recap.record.len(), 1);
    // Not squelched: its echo in chat, with a timestamp, is the same hit.
    state.combat_lines = vec![chat(text_type::COMBAT_ENEMY, line)];
    state.new_chat = vec![
        chat(text_type::COMBAT_ENEMY, &format!("14:02:31 {line}")),
        chat(text_type::COMBAT_ENEMY, line),
    ];
    h.frame(&state);
    state.combat_lines.clear();
    state.new_chat.clear();
    assert_eq!(
        h.ui.windows.death_recap.record.len(),
        3,
        "one for the notification, one for the second line in chat"
    );
}

#[test]
fn life_transfers_read_as_health_and_stamina_and_mana_drains_do_not() {
    let read = |text| taken(7, text, 0.0).map(|t| (t.source().to_owned(), t.kind, t.amount));
    assert_eq!(
        read("You lose 22 points of health due to Drudge Sorcerer casting Drain Health Other II on you"),
        Some(("Drudge Sorcerer".into(), Kind::Drain, 22))
    );
    assert_eq!(
        read("You gain 30 points of health due to Aldric casting Health to Health Other on you"),
        Some(("Aldric".into(), Kind::Healing, 30))
    );
    assert_eq!(
        read("You gain 52 points of health due to casting Drain Health Other I on Olthoi Warrior"),
        Some(("You".into(), Kind::Healing, 52))
    );
    for text in [
        "You lose 12 points of mana due to Zofrit Zefir casting Drain Mana Other II on you",
        "You lose 22 points of stamina due to Drudge casting Drain Stamina Other on you",
        "Drudge Sorcerer casts Drain Stamina Other and drains 30 points of your stamina.",
        "Drudge Sorcerer casts Drain Mana Other and drains 30 points of your mana.",
        "With Drain Mana Other you drain 30 points of mana from Drudge Sorcerer.",
    ] {
        assert_eq!(read(text), None, "{text}");
        assert_eq!(
            pyreal_horizon::ui::combat_numbers::parse(7, text),
            None,
            "{text}"
        );
    }
}

#[test]
fn the_game_s_report_names_the_killer_first_and_no_killer_means_none() {
    // The report names the killer by id over the line's name.
    let mut h = opening();
    let mut state = world(3);
    h.frame(&state);
    say(&mut h, &mut state, vec![blow("Drudge Skulker", 20)]);
    state.health.current = 0;
    state.deaths = Some(4);
    state.own_deaths = vec![Some(Some("Olthoi Soldier".into()))];
    say(
        &mut h,
        &mut state,
        vec![chat(text_type::DEFAULT, "Drudge Skulker splits you apart!")],
    );
    state.own_deaths.clear();
    let r = recap(&h);
    assert_eq!(r.killer(), Some("Olthoi Soldier"));
    assert_eq!(r.taken.last().map(|t| t.source()), Some("Olthoi Soldier"));
    assert_eq!(r.detail(), "Drudge Skulker splits you apart!");

    // No killer (a fall, @die): none, whatever hit the player before.
    let mut h = opening();
    let mut state = world(3);
    h.frame(&state);
    say(&mut h, &mut state, vec![blow("Drudge Skulker", 20)]);
    run(&mut h, &state, 1.0);
    state.health.current = 0;
    state.deaths = Some(4);
    h.frame(&state);
    // The report comes a frame after the count, the line with it.
    state.own_deaths = vec![Some(None)];
    say(
        &mut h,
        &mut state,
        vec![chat(text_type::DEFAULT, "You died!")],
    );
    state.own_deaths.clear();
    let r = recap(&h);
    assert_eq!(r.killer(), None);
    assert_eq!(r.heading(), "You died");
    assert_eq!(r.detail(), "The game names no killer.");
    assert_eq!(r.taken.last().map(|t| t.source()), Some("Unknown"));

    // A killer the client does not know leaves it to the line.
    let mut h = opening();
    let mut state = world(3);
    h.frame(&state);
    state.deaths = Some(4);
    state.own_deaths = vec![None];
    say(
        &mut h,
        &mut state,
        vec![chat(text_type::DEFAULT, "Gaerlan runs you through!")],
    );
    assert_eq!(recap(&h).killer(), Some("Gaerlan"));
}

#[test]
fn evades_and_resists_are_listed_and_take_nothing() {
    let mut h = opening();
    let mut state = world(3);
    h.frame(&state);
    say(
        &mut h,
        &mut state,
        vec![
            chat(text_type::COMBAT_ENEMY, "You evaded Drudge Skulker!"),
            chat(
                text_type::MAGIC,
                "You resist the spell cast by Drudge Sorcerer",
            ),
            blow("Drudge Skulker", 30),
        ],
    );
    state.deaths = Some(4);
    h.frame(&state);
    let r = recap(&h);
    let rows: Vec<_> = r
        .taken
        .iter()
        .map(|t| (t.kind, t.source().to_owned(), t.amount_words()))
        .collect();
    assert_eq!(
        rows[..3],
        [
            (Kind::Evaded, "Drudge Skulker".into(), "Evaded".into()),
            (Kind::Resisted, "Drudge Sorcerer".into(), "Resisted".into()),
            (Kind::Damage, "Drudge Skulker".into(), "-30".into()),
        ]
    );
    assert_eq!(r.damage_total(), 30 + 80, "the blow and the killing blow");
}

#[test]
fn the_items_lost_come_seconds_after_the_death_and_are_shown() {
    assert_eq!(
        loss_line(
            "You've lost 1,234 Pyreals, your 23 Pearls of Spirit Drinking, your 2 Evader's Crystals, and your Sword of Lost Hope!"
        ),
        Some(LossLine::Lost(vec![
            "1,234 Pyreals".into(),
            "23 Pearls of Spirit Drinking".into(),
            "2 Evader's Crystals".into(),
            "Sword of Lost Hope".into(),
        ]))
    );
    assert_eq!(
        loss_line("You've lost your Shield of Engorgement!"),
        Some(LossLine::Lost(vec!["Shield of Engorgement".into()]))
    );
    assert_eq!(
        loss_line("You have retained all your items. You do not need to recover your corpse!"),
        Some(LossLine::Kept)
    );
    assert_eq!(
        loss_line("You are enveloped in a feeling of warmth as you are brought back into the protection of the Light. You are once again a Non-Player Killer."),
        Some(LossLine::Kept),
        "a PK Lite death"
    );
    assert_eq!(
        loss_line("Your augmentation has reduced the number of items you can lose by 5!"),
        Some(LossLine::Miser(5))
    );
    assert_eq!(loss_line("You've got mail!"), None);

    let mut h = opening();
    let mut state = world(3);
    h.frame(&state);
    state.deaths = Some(4);
    h.frame(&state);
    assert_eq!(recap(&h).loss, None, "not said yet");
    run(&mut h, &state, 4.0);
    say(
        &mut h,
        &mut state,
        vec![
            chat(
                text_type::DEFAULT,
                "Your augmentation has reduced the number of items you can lose by 5!",
            ),
            chat(
                text_type::DEFAULT,
                "You've lost 70 Pyreals, and your Short Sword!",
            ),
        ],
    );
    let loss = recap(&h).loss.clone().expect("the items lost");
    assert_eq!(loss.items, ["70 Pyreals", "Short Sword"]);
    assert_eq!(
        loss.words(),
        "Lost 70 Pyreals, Short Sword. Clutch of the Miser saved up to 5."
    );
    h.frame(&state);
    // Long after, a line like it is not this death's.
    run(&mut h, &state, LOSS_SECONDS);
    say(
        &mut h,
        &mut state,
        vec![chat(
            text_type::DEFAULT,
            "You have retained all your items. You do not need to recover your corpse!",
        )],
    );
    assert_eq!(recap(&h).loss.as_ref().map(|l| l.items.len()), Some(2));
}

#[test]
fn several_deaths_in_a_row_each_get_their_own_recap_and_items() {
    let mut h = opening();
    let mut state = world(3);
    h.frame(&state);
    say(&mut h, &mut state, vec![blow("Drudge Skulker", 25)]);
    state.deaths = Some(4);
    h.frame(&state);
    run(&mut h, &state, 5.0);
    say(
        &mut h,
        &mut state,
        vec![chat(
            text_type::DEFAULT,
            "You have retained all your items. You do not need to recover your corpse!",
        )],
    );
    let first = recap(&h).at;
    assert_eq!(recap(&h).loss.as_ref().map(|l| l.items.len()), Some(0));
    // Risen, and killed again ten seconds on.
    state.health.current = 75;
    run(&mut h, &state, 10.0);
    say(&mut h, &mut state, vec![blow("Mosswart Elder", 40)]);
    state.health.current = 0;
    state.deaths = Some(5);
    h.frame(&state);
    let r = recap(&h);
    assert!(r.at > first);
    assert_eq!(r.loss, None, "its own items not said yet");
    assert_eq!(r.killer(), Some("Mosswart Elder"));
    assert!(r.taken.iter().all(|t| t.source() != "Drudge Skulker"));
    run(&mut h, &state, 4.0);
    say(
        &mut h,
        &mut state,
        vec![chat(text_type::DEFAULT, "You've lost 12 Pyreals!")],
    );
    assert_eq!(
        recap(&h).loss.as_ref().map(|l| l.items.clone()),
        Some(vec!["12 Pyreals".to_owned()])
    );
}

#[test]
fn player_killer_broadcasts_and_another_s_death_are_no_death_of_the_player() {
    let mut h = opening();
    let mut state = world(3);
    h.frame(&state);
    for text in [
        "Aldric has defeated Tester!",
        "Tester has defeated Aldric! The kill occured at 12.3N, 45.6E.",
        "Gaerlan splits Aldric apart!",
        "Your fellow Aldric has died!",
    ] {
        say(&mut h, &mut state, vec![chat(text_type::DEFAULT, text)]);
    }
    assert!(h.ui.windows.death_recap.last.is_none());
}

#[test]
fn the_vitae_penalty_is_the_enchantment_s() {
    use pyreal_horizon::ui::panels::recap::vitae_words;
    assert_eq!(vitae_words(Some(0.95)), "Your vitae penalty is 5%.");
    assert_eq!(vitae_words(Some(0.6)), "Your vitae penalty is 40%.");
    assert_eq!(vitae_words(Some(1.0)), "You have no vitae penalty.");
    assert_eq!(vitae_words(None), "You have no vitae penalty.");
}
