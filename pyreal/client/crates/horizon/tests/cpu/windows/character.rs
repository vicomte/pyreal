//! The character window's values: a value raised or lowered by magic, and its breakdown over its
//! row.

use std::sync::Arc;

use pyreal_client_contract::UiRequest;
use pyreal_horizon::art::Art;
use pyreal_horizon::draw::Rect;
use pyreal_horizon::pieces::Pieces;
use pyreal_horizon::ui::game::{GameState, Modifiers, Skill, StatRow};
use pyreal_horizon::ui::panels::WindowId;
use pyreal_horizon::ui::HorizonUi;

use crate::Harness;

fn character() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        stats: vec![
            StatRow {
                name: "Strength".into(),
                wire: 1,
                shown: "110".into(),
                modifiers: Modifiers::of(100, 110, 0),
                ..StatRow::default()
            },
            StatRow {
                name: "Health".into(),
                wire: 1,
                vital: true,
                shown: "120 / 130".into(),
                modifiers: Modifiers::of(150, 130, 0),
                ..StatRow::default()
            },
        ],
        skills: vec![
            Skill {
                name: "Melee Defense".into(),
                value: 235,
                modifiers: Modifiers::of(200, 235, 0),
                training: 3,
                id: 6,
                ..Skill::default()
            },
            Skill {
                name: "Healing".into(),
                value: 120,
                modifiers: Modifiers::of(150, 120, 0),
                training: 2,
                id: 21,
                ..Skill::default()
            },
            Skill {
                name: "Run".into(),
                value: 190,
                modifiers: Modifiers::of(200, 190, -10),
                training: 2,
                id: 24,
                ..Skill::default()
            },
        ],
        ..GameState::default()
    }
}

/// The rows of the page the character window shows, top to bottom: the stop at the left of each.
fn rows(h: &Harness) -> Vec<Rect> {
    let snap = &h.input.nav.last;
    let mut rows: Vec<Rect> = snap
        .targets
        .iter()
        .filter(|t| snap.list_of(t).is_some())
        .map(|t| t.rect)
        .collect();
    rows.sort_by(|a, b| (a.y, a.x).partial_cmp(&(b.y, b.x)).unwrap());
    rows.dedup_by(|b, a| (a.y - b.y).abs() < 1.0);
    rows
}

/// The tooltip over the middle of `row`, a frame after the pointer is put there.
fn tip_over(h: &mut Harness, state: &GameState, row: Rect) -> Option<(String, Vec<String>)> {
    h.move_to(row.x + row.w / 2.0, row.y + row.h / 2.0);
    h.frame(state);
    h.ui.windows.tip().cloned()
}

fn opened(state: &GameState) -> Harness {
    let mut h = Harness::new(Default::default());
    h.ui.windows.open(WindowId::Character, -1.0);
    h.frame(state);
    h.frame(state);
    h
}

/// Turn the character window to its skills.
fn to_skills(h: &mut Harness, state: &GameState) {
    let snap = h.input.nav.last.clone();
    let bar = snap
        .tabs_in(snap.layers.len())
        .expect("the character window's tabs are noted");
    h.input.pad.tab_step = Some((bar, 1));
    h.frame(state);
    h.frame(state);
}

/// The colours of the text drawn inside `row`.
fn colours_in(h: &Harness, row: Rect) -> Vec<u32> {
    h.list
        .quads
        .iter()
        .filter(|q| {
            q.tex.is_some() && row.contains(q.dst.x + q.dst.w / 2.0, q.dst.y + q.dst.h / 2.0)
        })
        .map(|q| q.colour)
        .collect()
}

#[test]
fn an_attribute_row_s_tooltip_gives_its_base_and_what_magic_adds() {
    let state = character();
    let mut h = opened(&state);
    let rows = rows(&h);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(
        tip_over(&mut h, &state, rows[0]),
        Some(("Strength".into(), vec!["100 (+10)".into()]))
    );
    assert_eq!(
        tip_over(&mut h, &state, rows[1]),
        Some(("Health".into(), vec!["Maximum: 150 (-20)".into()]))
    );
}

#[test]
fn a_skill_row_s_tooltip_gives_its_base_and_what_magic_and_vitae_add() {
    let state = character();
    let mut h = opened(&state);
    to_skills(&mut h, &state);
    let rows = rows(&h);
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert_eq!(
        tip_over(&mut h, &state, rows[0]),
        Some(("Melee Defense".into(), vec!["200 (+35)".into()]))
    );
    assert_eq!(
        tip_over(&mut h, &state, rows[1]),
        Some(("Healing".into(), vec!["150 (-30)".into()]))
    );
    assert_eq!(
        tip_over(&mut h, &state, rows[2]),
        Some(("Run".into(), vec!["200".into(), "Vitae: -10".into()]))
    );
}

#[test]
fn a_skill_raised_by_magic_is_drawn_green_one_lowered_red_and_one_under_vitae_alone_plain() {
    const GREEN: u32 = 0xFF7C_E07C;
    const RED: u32 = 0xFFF0_6A5A;
    let state = character();
    let art = Art::new(Arc::new(Pieces::built_in()));
    let mut h = Harness::new(Default::default());
    h.ui = HorizonUi::new(Arc::new(art), Default::default());
    h.ui.windows.open(WindowId::Character, -1.0);
    h.frame(&state);
    h.frame(&state);
    to_skills(&mut h, &state);
    let rows = rows(&h);
    assert_eq!(rows.len(), 3, "{rows:?}");
    // The value stands right of the row's stop, in the same line.
    let line = |r: Rect| Rect::new(r.x, r.y, 10_000.0, r.h);
    let buffed = colours_in(&h, line(rows[0]));
    assert!(buffed.contains(&GREEN), "Melee Defense: {buffed:x?}");
    assert!(!buffed.contains(&RED));
    let lowered = colours_in(&h, line(rows[1]));
    assert!(lowered.contains(&RED), "Healing: {lowered:x?}");
    assert!(!lowered.contains(&GREEN));
    let vitae = colours_in(&h, line(rows[2]));
    assert!(!vitae.is_empty(), "Run is drawn");
    assert!(
        !vitae.contains(&GREEN) && !vitae.contains(&RED),
        "Run: {vitae:x?}"
    );
}

/// A skill row the list cuts off at its foot draws only inside the list; a press just below the
/// list, on the part of its "+1" the list hides, raises nothing.
#[test]
fn a_press_just_below_the_skill_list_raises_no_row_cut_off_there() {
    let names = [
        "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel", "India",
        "Juliet", "Kilo", "Lima", "Mike", "November", "Oscar", "Papa", "Quebec", "Romeo", "Sierra",
        "Tango", "Uniform", "Victor", "Whiskey", "Xray", "Yankee", "Zulu",
    ];
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        skills: (100u32..)
            .zip(names)
            .map(|(id, n)| Skill {
                name: n.into(),
                value: 100,
                modifiers: Modifiers::of(100, 100, 0),
                training: 2,
                id,
                cost: 1000,
                cost_10: 10_000,
                ..Skill::default()
            })
            .collect(),
        unassigned_xp: 1_000_000,
        ..GameState::default()
    };
    let mut h = opened(&state);
    to_skills(&mut h, &state);
    for _ in 0..12 {
        let snap = h.input.nav.last.clone();
        let list = snap
            .lists
            .iter()
            .map(|l| l.area)
            .max_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h)))
            .expect("the skills list");
        let plus_one = snap
            .targets
            .iter()
            .chain(snap.hidden.iter())
            .map(|t| t.rect)
            .filter(|r| (r.w - 32.0).abs() < 0.5 && (r.h - 22.0).abs() < 0.5)
            .find(|b| b.contains(b.x + b.w / 2.0, list.bottom() + 3.0));
        if let Some(b) = plus_one {
            let (x, y) = (b.x + b.w / 2.0, list.bottom() + 3.0);
            h.move_to(x, y);
            h.frame(&state);
            h.press();
            let mut requests = h.frame(&state).requests;
            h.release();
            requests.extend(h.frame(&state).requests);
            assert!(
                requests.is_empty(),
                "a press {} px below the list raised: {requests:?}",
                y - list.bottom()
            );
            return;
        }
        h.move_to(list.x + list.w / 2.0, list.y + list.h / 2.0);
        h.input.wheel = -1.0;
        h.frame(&state);
        h.frame(&state);
    }
    panic!("no row straddled the list's foot");
}

/// Train spends skill credits, which can't be had back: it asks first, and only a Yes trains.
#[test]
fn train_asks_before_it_spends_skill_credits_and_only_a_yes_trains() {
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        skills: vec![Skill {
            name: "Alchemy".into(),
            value: 10,
            modifiers: Modifiers::of(10, 10, 0),
            training: 1,
            id: 34,
            cost: 6,
            ..Skill::default()
        }],
        skill_credits: 20,
        ..GameState::default()
    };
    let train = |answer: usize| {
        let mut h = opened(&state);
        to_skills(&mut h, &state);
        let snap = h.input.nav.last.clone();
        let b = snap
            .targets
            .iter()
            .map(|t| t.rect)
            .find(|r| (r.w - 76.0).abs() < 0.5 && (r.h - 22.0).abs() < 0.5)
            .expect("the Train button");
        h.move_to(b.x + b.w / 2.0, b.y + b.h / 2.0);
        h.frame(&state);
        h.press();
        let mut asked = h.frame(&state).requests;
        h.release();
        asked.extend(h.frame(&state).requests);
        assert!(
            !asked
                .iter()
                .any(|r| matches!(r, UiRequest::TrainSkillAdvancementClass { .. })),
            "nothing is trained before the answer: {asked:?}"
        );
        h.input.keys = vec![answer];
        let mut answered = h.frame(&state).requests;
        answered.extend(h.frame(&state).requests);
        answered
    };
    use pyreal_horizon::ui::input::vk;
    assert!(
        train(vk::ENTER).contains(&UiRequest::TrainSkillAdvancementClass {
            skill: 34,
            credits: 6
        })
    );
    assert!(!train(vk::ESCAPE)
        .iter()
        .any(|r| matches!(r, UiRequest::TrainSkillAdvancementClass { .. })));
}

/// Near the cap the "+10" buys fewer than ten, and its tooltip says how many.
#[test]
fn the_raise_by_ten_tooltip_says_how_many_it_buys_near_the_cap() {
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        skills: vec![Skill {
            name: "Run".into(),
            value: 225,
            modifiers: Modifiers::of(225, 225, 0),
            training: 2,
            id: 24,
            cost: 1000,
            cost_10: 3000,
            count_10: 3,
            ..Skill::default()
        }],
        unassigned_xp: 1_000_000,
        ..GameState::default()
    };
    let mut h = opened(&state);
    to_skills(&mut h, &state);
    let ten = h
        .input
        .nav
        .last
        .targets
        .iter()
        .map(|t| t.rect)
        .find(|r| (r.w - 40.0).abs() < 0.5 && (r.h - 22.0).abs() < 0.5)
        .expect("the +10 button");
    let tip = tip_over(&mut h, &state, ten).expect("a tooltip");
    assert_eq!(tip.0, "Raise Run by 3");
}
