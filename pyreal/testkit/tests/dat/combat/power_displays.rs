use super::*;

// ---------------------------------------------------------------------------------------------
// combat.power-bar.*
//
// The rate claims drive the same two pieces `App::frame` drives -- the charge notices taken at
// the top of the frame, then the interaction slot -- in that order, so every level a notice
// carries is one frame behind the clock; the lag is measured rather than described.
// ---------------------------------------------------------------------------------------------

pub(super) fn the_power_bar_follows_the_clock_at_one_full_charge_a_second() {
    use pyreal_client_contract::actions::mapped as ia;
    use pyreal_client_model::combat::PowerBarMode;

    let mut b = bars::Bench::new();
    b.enter_advanced_combat();

    b.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK.0, true)], 100.0);
    let building =
        b.combat().power_bar_mode == PowerBarMode::AdvancedCombat && b.combat().build_in_progress;

    let mut drawn = Vec::new();
    let mut only_the_advanced_bar = true;
    for step in 1..=6 {
        b.frame(vec![], 100.0 + f64::from(step) * 0.1);
        only_the_advanced_bar &= b.modes().iter().all(|m| *m == PowerBarMode::AdvancedCombat);
        drawn.push(b.drawn_level());
    }

    // The ramp, one frame behind the clock, and a rate rather than six readings that match.
    let on_the_ramp = drawn
        .iter()
        .enumerate()
        .all(|(i, v)| (v - bars::step_of(i)).abs() < 1e-5);
    let steps: Vec<f32> = drawn.windows(2).map(|w| w[1] - w[0]).collect();
    let even = steps.iter().all(|s| (s - 0.1).abs() < 1e-5);
    let rising = drawn.windows(2).all(|w| w[1] > w[0]);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.power-bar.it-follows-the-clock-at-one-full-charge-a-second",
        move |_| building && only_the_advanced_bar && on_the_ramp && even && rising,
    );
}

pub(super) fn a_two_handed_style_charges_a_quarter_faster() {
    use pyreal_client_contract::actions::mapped as ia;

    let mut b = bars::Bench::new();
    b.enter_advanced_combat();
    b.set_style(pyreal_client_model::combat::DUAL_WIELD_COMBAT_STYLE);

    b.frame(vec![bars::key(ia::COMBAT_MEDIUM_ATTACK.0, true)], 200.0);
    let mut drawn = Vec::new();
    for step in 1..=6 {
        b.frame(vec![], 200.0 + f64::from(step) * 0.1);
        drawn.push(b.drawn_level());
    }
    let steps: Vec<f32> = drawn.windows(2).map(|w| w[1] - w[0]).collect();
    let steeper = steps.iter().all(|s| (s - 0.125).abs() < 1e-5);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.power-bar.a-two-handed-style-charges-a-quarter-faster-over-the-same-frames",
        move |_| steeper && !steps.is_empty(),
    );
}

pub(super) fn the_keyboard_gauge_has_seven_notches_and_starts_half_way() {
    let mut g = pyreal_client_model::combat::CombatState::begin();
    let starts_half_way = (g.ui_requested_power - 0.5).abs() < 1e-9;

    let mut seen: Vec<f32> = vec![g.ui_requested_power];
    for _ in 0..10 {
        seen.push(g.adjust_ui_requested_power(false));
    }
    for _ in 0..12 {
        seen.push(g.adjust_ui_requested_power(true));
    }
    let mut notches = seen;
    notches.sort_by(f32::total_cmp);
    notches.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let seven = notches.len() == 7;
    #[allow(clippy::cast_precision_loss)]
    let evenly_spaced = notches
        .iter()
        .enumerate()
        .all(|(i, n)| (n - i as f32 / 6.0).abs() < 1e-6);
    let saturates = (notches[0] - 0.0).abs() < 1e-9 && (notches[6] - 1.0).abs() < 1e-9;
    let half_is_a_notch = (notches[3] - 0.5).abs() < 1e-6;

    let mut d = pyreal_client_model::combat::CombatState::begin();
    let up_and_back = (d.adjust_ui_requested_power(true) - 4.0 / 6.0).abs() < 1e-6
        && (d.adjust_ui_requested_power(false) - 0.5).abs() < 1e-6;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.power-bar.the-keyboard-gauge-has-seven-notches-and-starts-half-way",
        move |_| {
            starts_half_way && seven && evenly_spaced && saturates && half_is_a_notch && up_and_back
        },
    );
}

pub(super) fn the_power_drag_is_continuous_and_lands_between_the_notches() {
    let mut g = pyreal_client_model::combat::CombatState::begin();
    let a = g.set_ui_requested_power_from_scrollbar(500);
    let b = g.set_ui_requested_power_from_scrollbar(501);
    let one_unit =
        (a - 0.5).abs() < 1e-6 && (b - 0.501).abs() < 1e-6 && (b - a - 0.001).abs() < 1e-6;

    let values: Vec<f32> = (400..420)
        .map(|p| g.set_ui_requested_power_from_scrollbar(p))
        .collect();
    let distinct = {
        let mut v = values.clone();
        v.sort_by(f32::total_cmp);
        v.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        v.len() == 20
    };
    let notches: Vec<f32> = (0u8..=6).map(|i| f32::from(i) / 6.0).collect();
    let off_the_notches = values
        .iter()
        .all(|v| notches.iter().all(|n| (n - v).abs() > 1e-4));

    let clamps = (g.set_ui_requested_power_from_scrollbar(0) - 0.0).abs() < 1e-9
        && (g.set_ui_requested_power_from_scrollbar(1_000) - 1.0).abs() < 1e-9
        && (g.set_ui_requested_power_from_scrollbar(4_000) - 1.0).abs() < 1e-9
        && (g.set_ui_requested_power_from_scrollbar(u32::MAX) - 1.0).abs() < 1e-9;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.power-bar.the-drag-is-continuous-and-lands-between-the-notches",
        move |_| one_unit && distinct && off_the_notches && clamps,
    );
}
