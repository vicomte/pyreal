//! Effects ending soon: a beneficial effect in its last minute shows as its own icon with its time
//! left, and a notice names it; it leaves the row when it ends or is refreshed, and a refresh
//! warns again when it next runs low.

use pyreal_horizon::options::HorizonOptions;
use pyreal_horizon::ui::game::{Effect, GameState};
use pyreal_horizon::ui::hud::effects_ending::EndingSoon;
use pyreal_horizon::ui::hud::time_left;
use pyreal_horizon::ui::panels::WindowId;

use crate::Harness;

const STRENGTH_SELF: u32 = 2;
const ARMOR_SELF: u32 = 24;
const IMPERIL: u32 = 89;

const NOTICE: &str = "Strength Self VI ends in one minute.";

fn effect(spell: u32, name: &str, harmful: bool, remaining: f64) -> Effect {
    Effect {
        spell,
        layered: spell,
        name: name.into(),
        ac_icon: 0,
        harmful,
        remaining: Some(remaining),
        superseded: false,
    }
}

fn buffed(effects: Vec<Effect>) -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        effects,
        ..GameState::default()
    }
}

fn strength(remaining: f64) -> GameState {
    buffed(vec![effect(
        STRENGTH_SELF,
        "Strength Self VI",
        false,
        remaining,
    )])
}

/// The spells the row shows, ending first.
fn row(h: &Harness) -> Vec<u32> {
    h.ui.hud.ending.shown().iter().map(|s| s.spell).collect()
}

/// How many times the notices showing say `text`.
fn said(h: &Harness, text: &str) -> usize {
    h.ui.hud
        .notice_lines()
        .iter()
        .filter(|t| **t == text)
        .count()
}

/// Frames enough for the last notice to fade and the next to be given.
fn wait(h: &mut Harness, state: &GameState) {
    for _ in 0..(5 * 60) {
        h.frame(state);
    }
}

#[test]
fn an_effect_entering_its_last_minute_shows_with_a_notice_and_counts_down() {
    let mut h = Harness::new(HorizonOptions::default());
    h.frame(&strength(61.0));
    assert!(row(&h).is_empty(), "not yet in its last minute");
    assert_eq!(said(&h, NOTICE), 0);
    h.frame(&strength(59.5));
    assert_eq!(row(&h), [STRENGTH_SELF]);
    assert_eq!(said(&h, NOTICE), 1);
    // The countdown follows the time left; the notice is said once.
    h.frame(&strength(12.2));
    let shown = &h.ui.hud.ending.shown()[0];
    assert_eq!(time_left(shown.remaining), "13");
    assert_eq!(said(&h, NOTICE), 1);
}

#[test]
fn an_effect_that_ends_leaves_the_row_without_another_notice() {
    let mut h = Harness::new(HorizonOptions::default());
    h.frame(&strength(61.0));
    h.frame(&strength(0.5));
    assert_eq!(row(&h), [STRENGTH_SELF]);
    wait(&mut h, &strength(0.5));
    // Gone from the effects in force, or at no time left: off the row.
    h.frame(&buffed(Vec::new()));
    assert!(row(&h).is_empty());
    h.frame(&strength(0.0));
    assert!(row(&h).is_empty());
    assert!(h.ui.hud.notice_lines().is_empty());
}

#[test]
fn a_refreshed_effect_leaves_the_row_and_warns_again_when_it_next_runs_low() {
    let mut h = Harness::new(HorizonOptions::default());
    h.frame(&strength(61.0));
    h.frame(&strength(40.0));
    assert_eq!(said(&h, NOTICE), 1);
    wait(&mut h, &strength(40.0));
    assert_eq!(said(&h, NOTICE), 0, "the first notice has faded");
    // Recast with its full time: off the row, and no notice.
    h.frame(&strength(1800.0));
    assert!(row(&h).is_empty());
    assert_eq!(said(&h, NOTICE), 0);
    // Its last minute again: back on the row, with the notice again.
    h.frame(&strength(59.9));
    assert_eq!(row(&h), [STRENGTH_SELF]);
    assert_eq!(said(&h, NOTICE), 1);
}

#[test]
fn a_refresh_within_the_last_minute_restarts_the_countdown_without_a_notice() {
    let mut h = Harness::new(HorizonOptions::default());
    h.frame(&strength(61.0));
    h.frame(&strength(20.0));
    wait(&mut h, &strength(20.0));
    // A short recast, still under a minute.
    h.frame(&strength(55.0));
    assert_eq!(h.ui.hud.ending.shown()[0].remaining, 55.0);
    assert_eq!(said(&h, NOTICE), 0);
}

#[test]
fn an_effect_first_seen_in_its_last_minute_shows_without_a_notice() {
    let mut h = Harness::new(HorizonOptions::default());
    h.frame(&strength(30.0));
    assert_eq!(row(&h), [STRENGTH_SELF]);
    assert!(h.ui.hud.notice_lines().is_empty());
}

#[test]
fn effects_entering_together_share_one_notice_and_the_row_shows_the_first_to_end_first() {
    let mut h = Harness::new(HorizonOptions::default());
    let both = |s: f64, a: f64| {
        buffed(vec![
            effect(STRENGTH_SELF, "Strength Self VI", false, s),
            effect(ARMOR_SELF, "Armor Self VI", false, a),
        ])
    };
    h.frame(&both(70.0, 65.0));
    h.frame(&both(58.0, 50.0));
    assert_eq!(row(&h), [ARMOR_SELF, STRENGTH_SELF]);
    assert_eq!(
        h.ui.hud.notice_lines(),
        ["Strength Self VI and Armor Self VI end in one minute."]
    );
}

#[test]
fn a_harmful_effect_shows_while_it_lasts_without_a_notice() {
    let mut h = Harness::new(HorizonOptions::default());
    let cursed = |left: f64| buffed(vec![effect(IMPERIL, "Imperil Other VI", true, left)]);
    h.frame(&cursed(300.0));
    assert_eq!(row(&h), [IMPERIL]);
    h.frame(&cursed(45.0));
    assert!(h.ui.hud.notice_lines().is_empty());
}

#[test]
fn the_setting_turns_the_row_off_or_widens_it() {
    let mut h = Harness::new(HorizonOptions {
        effects_ending: EndingSoon::Off,
        ..HorizonOptions::default()
    });
    h.frame(&strength(61.0));
    h.frame(&strength(30.0));
    assert!(row(&h).is_empty());
    assert!(h.ui.hud.notice_lines().is_empty());
    let mut h = Harness::new(HorizonOptions {
        effects_ending: EndingSoon::FiveMinutes,
        ..HorizonOptions::default()
    });
    h.frame(&strength(301.0));
    h.frame(&strength(299.0));
    assert_eq!(row(&h), [STRENGTH_SELF]);
    assert_eq!(said(&h, "Strength Self VI ends in five minutes."), 1);
}

#[test]
fn the_row_is_an_element_the_hud_layout_editor_moves() {
    let mut h = Harness::new(HorizonOptions::default());
    let outline = |h: &Harness| {
        h.ui.hud
            .layout
            .outlines
            .iter()
            .find(|(n, _)| n == "ending")
            .map(|(_, r)| *r)
    };
    // Nothing ending and the editor closed: nothing drawn.
    h.frame(&strength(1800.0));
    assert_eq!(outline(&h), None);
    // With the editor open, empty frames stand in so it can be taken hold of.
    h.ui.windows.open(WindowId::Layout, 0.0);
    h.frame(&strength(1800.0));
    let r = outline(&h).expect("the row's stand-in");
    let grab = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    h.move_to(grab.0, grab.1);
    h.frame(&strength(1800.0));
    h.press();
    h.frame(&strength(1800.0));
    h.move_to(grab.0 - 300.0, grab.1 + 200.0);
    h.frame(&strength(1800.0));
    h.release();
    h.frame(&strength(1800.0));
    let moved = outline(&h).expect("the row, moved");
    assert!((moved.x - (r.x - 300.0)).abs() < 1.0, "{r:?} to {moved:?}");
    assert!((moved.y - (r.y + 200.0)).abs() < 1.0, "{r:?} to {moved:?}");
}

const ARMOR_SELF_V: u32 = 23;

/// Armor Self V and VI, each with its time left; the one behind the other superseded.
fn armor(v: Option<(f64, bool)>, vi: Option<f64>) -> GameState {
    let mut effects = Vec::new();
    if let Some(left) = vi {
        effects.push(effect(ARMOR_SELF, "Armor Self VI", false, left));
    }
    if let Some((left, behind)) = v {
        effects.push(Effect {
            superseded: behind,
            ..effect(ARMOR_SELF_V, "Armor Self V", false, left)
        });
    }
    buffed(effects)
}

#[test]
fn a_weaker_effect_ending_under_a_stronger_one_is_not_shown_or_noticed() {
    let mut h = Harness::new(HorizonOptions::default());
    h.frame(&armor(Some((61.0, true)), Some(1800.0)));
    h.frame(&armor(Some((30.0, true)), Some(1770.0)));
    assert!(row(&h).is_empty());
    assert!(h.ui.hud.notice_lines().is_empty());
}

#[test]
fn a_stronger_effect_ending_hands_over_to_the_weaker_one_behind_it() {
    let mut h = Harness::new(HorizonOptions::default());
    h.frame(&armor(Some((1500.0, true)), Some(61.0)));
    h.frame(&armor(Some((1480.0, true)), Some(40.0)));
    assert_eq!(row(&h), [ARMOR_SELF]);
    assert_eq!(
        h.ui.hud.notice_lines(),
        ["Armor Self VI ends in one minute."]
    );
    wait(&mut h, &armor(Some((1480.0, true)), Some(40.0)));
    // VI ends and V, with most of its time left, takes over: nothing to show yet.
    h.frame(&armor(Some((1440.0, false)), None));
    assert!(row(&h).is_empty());
    assert!(h.ui.hud.notice_lines().is_empty());
    // V then runs into its own last minute and is noticed as usual.
    h.frame(&armor(Some((59.0, false)), None));
    assert_eq!(row(&h), [ARMOR_SELF_V]);
    assert_eq!(
        h.ui.hud.notice_lines(),
        ["Armor Self V ends in one minute."]
    );
}

#[test]
fn a_weaker_effect_taking_over_inside_its_last_minute_shows_without_a_notice() {
    let mut h = Harness::new(HorizonOptions::default());
    h.frame(&armor(Some((90.0, true)), Some(70.0)));
    h.frame(&armor(Some((50.0, true)), Some(30.0)));
    assert_eq!(row(&h), [ARMOR_SELF]);
    wait(&mut h, &armor(Some((50.0, true)), Some(30.0)));
    h.frame(&armor(Some((20.0, false)), None));
    assert_eq!(row(&h), [ARMOR_SELF_V]);
    assert!(h.ui.hud.notice_lines().is_empty());
}

#[test]
fn of_two_casts_as_strong_only_the_one_in_force_shows() {
    let mut h = Harness::new(HorizonOptions::default());
    // The earlier cast of Armor Self VI, superseded by the later one.
    let both = |earlier: f64, later: f64| {
        buffed(vec![
            Effect {
                layered: (1 << 16) | ARMOR_SELF,
                superseded: true,
                ..effect(ARMOR_SELF, "Armor Self VI", false, earlier)
            },
            effect(ARMOR_SELF, "Armor Self VI", false, later),
        ])
    };
    h.frame(&both(61.0, 1800.0));
    h.frame(&both(30.0, 1770.0));
    assert!(row(&h).is_empty());
    assert!(h.ui.hud.notice_lines().is_empty());
    h.frame(&both(1.0, 61.0));
    h.frame(&both(0.5, 59.0));
    assert_eq!(row(&h), [ARMOR_SELF]);
    assert_eq!(h.ui.hud.ending.shown()[0].remaining, 59.0);
    assert_eq!(
        h.ui.hud.notice_lines(),
        ["Armor Self VI ends in one minute."]
    );
}

/// One caster's copy of Strength Self VI, in its own layer, with its time left.
fn copy(layer: u32, remaining: f64, superseded: bool) -> Effect {
    Effect {
        layered: (layer << 16) | STRENGTH_SELF,
        superseded,
        ..effect(STRENGTH_SELF, "Strength Self VI", false, remaining)
    }
}

#[test]
fn two_casters_copies_of_one_spell_each_count_down_and_are_noticed_on_their_own() {
    let mut h = Harness::new(HorizonOptions::default());
    // Two copies as strong and cast together, so neither is superseded.
    let both = |a: f64, b: f64| buffed(vec![copy(1, a, false), copy(2, b, false)]);
    h.frame(&both(61.0, 30.0));
    h.frame(&both(59.0, 29.0));
    let left: Vec<f64> =
        h.ui.hud
            .ending
            .shown()
            .iter()
            .map(|s| s.remaining)
            .collect();
    assert_eq!(left, [29.0, 59.0], "a countdown each");
    assert_eq!(
        said(&h, NOTICE),
        1,
        "the first copy entering its last minute"
    );
}

#[test]
fn a_second_caster_s_copy_taking_over_inside_its_last_minute_is_not_noticed() {
    let mut h = Harness::new(HorizonOptions::default());
    h.frame(&buffed(vec![copy(1, 100.0, false)]));
    // The other caster's copy takes over with less time left: first seen in its last minute.
    h.frame(&buffed(vec![copy(1, 99.0, true), copy(2, 59.0, false)]));
    assert_eq!(row(&h), [STRENGTH_SELF]);
    assert_eq!(h.ui.hud.ending.shown()[0].remaining, 59.0);
    assert_eq!(said(&h, NOTICE), 0);
}
