use super::*;

// ---------------------------------------------------------------------------------------------
// The combat keys and the gauge
//
// The claims about the casting bar are `magic`'s subject and are in `magic.rs`.
//
// **The calibration has no row of its own** and stands as a premise instead: that the driver can
// produce an action at all and that the arm can move the world. Without it every silence below
// would be indistinguishable from a broken harness.
//
// Every control is discovered from the shipped keymap: nothing here writes a key down.
// ---------------------------------------------------------------------------------------------

pub(super) fn one_set_of_keys_means_three_different_things() {
    use pyreal_input::combat::{mode, MAGIC_COMBAT_MAP, MELEE_COMBAT_MAP, MISSILE_COMBAT_MAP};

    let mut shell = maps::shell();
    let shared = keys::shared_controls(&shell);
    let mut d = maps::Driver::new();

    let mut by_mode: Vec<Vec<u32>> = Vec::new();
    for m in [mode::MELEE, mode::MISSILE, mode::MAGIC] {
        shell.set_combat_input_maps(m);
        let mut actions = Vec::new();
        for qc in &shared {
            let (down, _) = d.press_release(&mut shell, qc);
            assert_eq!(
                down.len(),
                1,
                "a shared key resolves to one action in every mode"
            );
            actions.push(down[0].id.0);
        }
        by_mode.push(actions);
    }

    // The three sets are **disjoint**: no meaning is reachable in more than one mode, which is
    // what says the mode selects the meaning rather than one map winning always.
    let disjoint = by_mode[0].iter().all(|a| !by_mode[1].contains(a))
        && by_mode[0].iter().all(|a| !by_mode[2].contains(a))
        && by_mode[1].iter().all(|a| !by_mode[2].contains(a));

    // ...and each set is the one that mode's own section binds.
    let is_the_sections = [MELEE_COMBAT_MAP, MISSILE_COMBAT_MAP, MAGIC_COMBAT_MAP]
        .iter()
        .zip(by_mode.iter())
        .all(|(map, actions)| {
            let section: std::collections::BTreeSet<u32> = maps::shipped_bindings(&shell, map.0)
                .iter()
                .map(|(a, _)| a.0)
                .collect();
            actions.iter().all(|a| section.contains(a))
        });
    let five = shared.len();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.input-map.one-set-of-keys-means-three-different-things-and-the-mode-picks-which",
        move |_| five > 0 && by_mode.iter().all(|m| m.len() == five) && disjoint && is_the_sections,
    );
}

pub(super) fn every_height_key_moves_the_height_in_both_modes() {
    use pyreal_client_contract::actions::mapped as ia;
    use pyreal_client_model::combat::{AttackHeight, CombatMode};

    // The calibration: the driver produces an action and the arm moves the world.
    let calibrated = keys::the_driver_and_the_arm_both_work();

    let mut every_mode_holds = true;
    let mut every_swing_carries_the_set_height = true;
    for (mode, imap, keys_of) in [
        (
            CombatMode::Melee,
            pyreal_input::combat::MELEE_COMBAT_MAP,
            [
                ia::COMBAT_LOW_ATTACK.0,
                ia::COMBAT_HIGH_ATTACK.0,
                ia::COMBAT_MEDIUM_ATTACK.0,
            ],
        ),
        (
            CombatMode::Missile,
            pyreal_input::combat::MISSILE_COMBAT_MAP,
            [
                ia::COMBAT_AIM_LOW.0,
                ia::COMBAT_AIM_HIGH.0,
                ia::COMBAT_AIM_MEDIUM.0,
            ],
        ),
    ] {
        let mut shell = maps::shell();
        shell.set_combat_input_maps(mode.raw());
        let mut d = maps::Driver::new();
        let mut bench = keys::Bench::new(mode);
        let mut now = 100.0;

        every_mode_holds &= bench.world().combat.requested_attack_height == AttackHeight::Medium;

        for (want_action, want_height) in [
            (keys_of[0], AttackHeight::Low),
            (keys_of[1], AttackHeight::High),
            (keys_of[2], AttackHeight::Medium),
        ] {
            let qc = maps::the_shipped_control(&shell, imap.0, want_action);
            let (down, up) = d.press_release(&mut shell, &qc);
            every_mode_holds &= down.iter().map(|e| e.id.0).eq([want_action]);
            now += 1.0;
            bench.drive(down, now);
            // Both halves: a handler that only writes the field looks identical from the field.
            every_mode_holds &= bench.world().combat.requested_attack_height == want_height
                && bench.world().combat.attack_request_in_progress
                && bench.world().combat.build_in_progress;
            now += 1.0;
            bench.drive(up, now);
            every_mode_holds &= !bench.world().combat.attack_request_in_progress
                // The release reads the height; it does not write it.
                && bench.world().combat.requested_attack_height == want_height;
        }
        every_mode_holds &= bench.height_changes() >= 3 && bench.attacks_released() >= 1;

        // **The asymmetric case**: press the low key and release the high one. The swing goes out
        // at the height that is set; releasing the same key cannot tell the two readings apart.
        let low = maps::the_shipped_control(&shell, imap.0, keys_of[0]);
        let (low_down, _) = d.press_release(&mut shell, &low);
        let mut fresh = keys::Bench::new(mode);
        now += 1.0;
        fresh.drive(low_down, now);
        every_mode_holds &= fresh.world().combat.requested_attack_height == AttackHeight::Low;
        now += 1.0;
        fresh.drive(vec![keys::released_in(keys_of[1], imap)], now);
        let swung = fresh.what_would_go_out();
        every_swing_carries_the_set_height &= !swung.is_empty()
            && swung
                .iter()
                .filter_map(keys::height_of)
                .all(|h| h == AttackHeight::Low as u32);
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.attack.every-height-key-moves-the-height-in-both-modes-and-the-swing-carries-it",
        move |_| calibrated && every_mode_holds && every_swing_carries_the_set_height,
    );
}

pub(super) fn the_gauge_keys_step_by_a_sixth_in_both_modes() {
    use pyreal_client_contract::actions::mapped as ia;
    use pyreal_client_model::combat::CombatMode;

    let sixth = 1.0_f32 / 6.0;
    let mut both_modes_hold = true;
    for (mode, imap, down_a, up_a) in [
        (
            CombatMode::Melee,
            pyreal_input::combat::MELEE_COMBAT_MAP,
            ia::COMBAT_DECREASE_ATTACK_POWER.0,
            ia::COMBAT_INCREASE_ATTACK_POWER.0,
        ),
        (
            CombatMode::Missile,
            pyreal_input::combat::MISSILE_COMBAT_MAP,
            ia::COMBAT_DECREASE_MISSILE_ACCURACY.0,
            ia::COMBAT_INCREASE_MISSILE_ACCURACY.0,
        ),
    ] {
        let mut shell = maps::shell();
        shell.set_combat_input_maps(mode.raw());
        let mut d = maps::Driver::new();
        let mut bench = keys::Bench::new(mode);
        let mut now = 100.0;
        both_modes_hold &= (bench.world().combat.ui_requested_power - 0.5).abs() < 1e-6;

        let step = |a: u32,
                    bench: &mut keys::Bench,
                    shell: &mut pyreal_client_shell::input::InputShell,
                    d: &mut maps::Driver,
                    now: &mut f64| {
            let qc = maps::the_shipped_control(shell, imap.0, a);
            let (down, _) = d.press_release(shell, &qc);
            assert!(
                down.iter().map(|e| e.id.0).eq([a]),
                "the key resolves to its own action"
            );
            *now += 1.0;
            bench.drive(down, *now);
            bench.world().combat.ui_requested_power
        };

        // Up from the half: one notch at a time, then it stops at full.
        let mut up = vec![0.5_f32];
        for _ in 0..5 {
            up.push(step(up_a, &mut bench, &mut shell, &mut d, &mut now));
        }
        both_modes_hold &= (up[1] - 4.0 * sixth).abs() < 1e-6
            && (up[2] - 5.0 * sixth).abs() < 1e-6
            && (up[3] - 1.0).abs() < 1e-6
            && (up[4] - 1.0).abs() < 1e-6
            && (up[5] - 1.0).abs() < 1e-6;

        // ...and all the way down, stopping at empty.
        let mut down = Vec::new();
        for _ in 0..8 {
            down.push(step(down_a, &mut bench, &mut shell, &mut d, &mut now));
        }
        let want: Vec<f32> = [5.0_f32, 4.0, 3.0, 2.0, 1.0, 0.0, 0.0, 0.0]
            .into_iter()
            .map(|n| (n * sixth).min(1.0))
            .collect();
        both_modes_hold &= down
            .iter()
            .zip(want.iter())
            .all(|(got, w)| (got - w).abs() < 1e-6);
        // Every press reached the gauge arm, which is what stops a silence reading as a step.
        both_modes_hold &= bench.desired_power_changes() == 13;
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.power-bar.the-gauge-keys-step-by-a-sixth-in-both-modes-and-saturate",
        move |_| both_modes_hold,
    );
}

pub(super) fn a_value_between_notches_rounds_to_the_nearest_before_stepping() {
    let mut g = pyreal_client_model::combat::CombatState::begin();
    let six = pyreal_client_model::combat::POWER_NOTCHES == 6;

    // Between notch 3 and notch 4: stepping up lands on 5/6 and not on a sixth further along.
    g.ui_requested_power = 0.60;
    let up = (g.adjust_ui_requested_power(true) - 5.0 / 6.0).abs() < 1e-6;
    g.ui_requested_power = 0.60;
    let down = (g.adjust_ui_requested_power(false) - 3.0 / 6.0).abs() < 1e-6;

    // The stops are applied after the scale, which is what makes empty and full the ends.
    g.ui_requested_power = 0.0;
    let at_the_bottom = g.adjust_ui_requested_power(false).abs() < 1e-9;
    g.ui_requested_power = 1.0;
    let at_the_top = (g.adjust_ui_requested_power(true) - 1.0).abs() < 1e-9;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.power-bar.a-value-between-notches-rounds-to-the-nearest-before-stepping",
        move |_| six && up && down && at_the_bottom && at_the_top,
    );
}

pub(super) fn holding_a_height_key_refuses_once_and_not_once_per_repeat() {
    use pyreal_client_model::combat::{AttackHeight, CombatMode};
    use pyreal_primitives::LocalTime;

    // Nothing to swing at, so the refusal arm is the one that runs.
    let mut w = keys::a_world(CombatMode::Melee);
    w.set_selected_object(None, false, &mut pyreal_client_model::NullSink);
    let nothing_to_hit = w.get_attack_target().is_none();

    let t = LocalTime(100.0);
    let first_refuses = w
        .set_requested_attack_height(AttackHeight::High, true, t)
        .is_err();
    // Ten repeats: nothing has latched, because the refusal arm never latches it, so every one
    // refuses. That is the discriminating case for the second half of the guard.
    let mut refusals = 0;
    for i in 0..10 {
        if w.set_requested_attack_height(AttackHeight::High, true, LocalTime(100.0 + f64::from(i)))
            .is_err()
        {
            refusals += 1;
        }
    }
    let every_one_refuses = refusals == 10;

    // Now with something to hit: the first press latches, and every later repeat of the same
    // height is swallowed.
    let mut w = keys::a_world(CombatMode::Melee);
    w.set_requested_attack_height(AttackHeight::High, true, t)
        .expect("a real target");
    let latched = w.combat.attack_request_in_progress;
    let start = w.combat.build_start_time;
    // Something the whole arm would overwrite if it ran.
    w.combat.requested_attack_power = 0.25;
    for i in 1..=10 {
        w.set_requested_attack_height(AttackHeight::High, true, LocalTime(100.0 + f64::from(i)))
            .expect("still valid");
    }
    let swallowed = (w.combat.requested_attack_power - 0.25).abs() < 1e-9
        && (w.combat.build_start_time - start).abs() < 1e-9;

    // A different height runs the whole thing again -- and the charge already running is still
    // not restarted, which is a different guard and is named rather than conflated.
    w.set_requested_attack_height(AttackHeight::Low, true, LocalTime(111.0))
        .expect("valid");
    let ran_again = (w.combat.requested_attack_power - 1.0).abs() < 1e-9
        && (w.combat.build_start_time - start).abs() < 1e-9;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.attack.holding-a-height-key-refuses-once-and-not-once-per-repeat",
        move |_| {
            nothing_to_hit
                && first_refuses
                && every_one_refuses
                && latched
                && swallowed
                && ran_again
        },
    );
}

pub(super) fn at_peace_no_combat_key_reaches_an_arm() {
    use pyreal_client_contract::actions::mapped as ia;
    use pyreal_client_model::combat::{AttackHeight, CombatMode};

    let mut shell = maps::shell();
    shell.set_combat_input_maps(pyreal_input::combat::mode::MELEE);
    let mut d = maps::Driver::new();
    let qc = maps::the_shipped_control(
        &shell,
        pyreal_input::combat::MELEE_COMBAT_MAP.0,
        ia::COMBAT_HIGH_ATTACK.0,
    );
    let (down, _) = d.press_release(&mut shell, &qc);
    let the_event_exists = down.len() == 1;

    let mut readings = Vec::new();
    for mode in [CombatMode::NonCombat, CombatMode::Melee] {
        let mut bench = keys::Bench::new(mode);
        bench.drive(down.clone(), 100.0);
        readings.push((
            bench.world().combat.requested_attack_height == AttackHeight::High,
            bench.height_changes(),
        ));
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.mode.at-peace-no-combat-key-reaches-an-arm",
        move |_| the_event_exists && readings == vec![(false, 0), (true, 1)],
    );
}
