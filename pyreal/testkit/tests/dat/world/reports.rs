use super::*;

// ===========================================================================================
// The world: reports, jumps, scripts, the camera, server control, doors, the map and the radar
// ===========================================================================================
//
// The claim is about a body, a producer or a script running over the retail data at a clock the
// scenario chooses, and the harness has no gesture for any of that -- so the scenario opens the
// dats itself through [`support::store`], drives the production structure directly, and books the
// claim through `assert_behaviour` on a model client.

// -------------------------------------------------------------------------------------------
// movement.position-report.*
// -------------------------------------------------------------------------------------------

/// What the client hands its position reporter is the body's own state, field for field.
pub fn the_reported_state_is_the_bodys_own() {
    use pyreal_protocol::movement::MoveTimestamps;
    use {
        pyreal_client_runtime::app::body_motion,
        pyreal_client_runtime::app::raw_motion_state_to_wire,
    };

    let store = support::store();
    let c = world_support::settled_body(&store);
    let m = body_motion(&c, MoveTimestamps::default());
    let pos = c.position();

    // Bit for bit, not approximately: nothing rounds on this path.
    let same_cell = m.position.objcell_id == pos.cell.0;
    let same_origin = m.position.frame.origin.x.to_bits() == pos.frame.origin.x.to_bits()
        && m.position.frame.origin.y.to_bits() == pos.frame.origin.y.to_bits()
        && m.position.frame.origin.z.to_bits() == pos.frame.origin.z.to_bits();
    let same_facing = m.position.frame.orientation.w.to_bits() == pos.frame.rotation.w.to_bits()
        && m.position.frame.orientation.z.to_bits() == pos.frame.rotation.z.to_bits();
    let valid = m.position_valid;
    let contact = m.contact && m.contact == c.on_ground();
    // The plane under a body resting on terrain points up. It is an input to the reporter's
    // third trigger, so it has to be a real value and not a default.
    let plane = m.contact_plane.normal.z > 0.5;
    // An idle body's raw state is the client's own default, which packs to no fields at all.
    let idle = raw_motion_state_to_wire(&c.driver().movement.interp.raw_state)
        .flags()
        .expect("the idle state encodes")
        == 0;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.position-report.the-state-the-client-reports-is-the-bodys-own",
        move |_| same_cell && same_origin && same_facing && valid && contact && plane && idle,
    );
}

/// Walking forward turns the reported motion state into the one the recordings carry, and
/// letting go turns it back.
pub fn walking_forward_reports_the_state_the_recordings_carry() {
    use pyreal_client_net::client_session::testing::{Corpus, Direction};
    use pyreal_client_runtime::app::raw_motion_state_to_wire;
    use pyreal_client_runtime::character::CharacterInput;
    use pyreal_primitives::LocalTime;

    /// The command a walk forward reports, as a literal: reading it back through the client's
    /// own constant would not notice a wrong constant.
    const WALK_FORWARD: u32 = 0x4500_0005;
    /// `forward_command` alone, and nothing else present.
    const FORWARD_ONLY: u32 = 0x004;

    let store = support::store();
    let mut c = world_support::settled_body(&store);

    c.input = CharacterInput {
        forward: true,
        ..CharacterInput::default()
    };
    for i in 61..=90 {
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    let s = raw_motion_state_to_wire(&c.driver().movement.interp.raw_state);
    let walking = s.flags().expect("the walking state encodes") == FORWARD_ONLY
        && s.forward_command == Some(WALK_FORWARD)
        // 1.0 and "no hold key" are the defaults and are not sent.
        && s.forward_speed.is_none()
        && s.current_holdkey.is_none()
        && s.actions.is_empty();

    c.input = CharacterInput::default();
    for i in 91..=120 {
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    let released = raw_motion_state_to_wire(&c.driver().movement.interp.raw_state)
        .flags()
        .expect("the idle state encodes")
        == 0;

    // And the walking word really is one the recorded clients sent -- counted off the corpus at
    // run time rather than written down here.
    let mut recorded = 0usize;
    for name in pyreal_client_net::client_session::testing::session_names() {
        let corpus = Corpus::shared(name);
        for b in &corpus.blobs {
            if b.dir != Direction::ClientToServer {
                continue;
            }
            let Ok(mut a) = pyreal_protocol::actions::unpack_action(&b.payload) else {
                continue;
            };
            if a.sub_type.0 != world_support::MOVE_TO_STATE {
                continue;
            }
            let Ok(m) =
                <pyreal_protocol::movement::MovementMoveToState as pyreal_protocol::Message>::read(
                    &mut a.body,
                )
            else {
                continue;
            };
            if m.0.raw_motion_state.forward_command == Some(WALK_FORWARD) {
                recorded += 1;
            }
        }
    }
    println!("report: {recorded} recorded move-to-state bodies carry the walk-forward word");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.position-report.walking-forward-reports-the-state-the-recordings-carry",
        move |_| walking && released && recorded > 0,
    );
}

/// A walking body reports where it is; a still one reports once and then falls silent.
///
/// The pair is the claim: a producer that fired every frame would satisfy the first half alone.
pub fn a_walking_body_reports_where_it_is_and_a_still_one_falls_silent() {
    use pyreal_client_runtime::app::body_motion;
    use pyreal_client_runtime::character::CharacterInput;
    use pyreal_primitives::LocalTime;
    use pyreal_protocol::movement::{
        MoveTimestamps, MovementAutonomousPosition, MovementMoveToState,
    };

    let store = support::store();

    // --- the walking arm -------------------------------------------------------------------
    let mut c = world_support::settled_body(&store);
    let (mut rep, mut session) = world_support::reporter();
    let mut frame = 60u32;
    let mut last_body_origin = c.position().frame.origin;
    // One second standing still first, so that the walk's own state edge happens inside the run
    // and can be attributed rather than being the reporter's own first frame.
    while frame < 60 + 30 {
        frame += 1;
        let now = f64::from(frame) / 30.0;
        c.update(LocalTime(now));
        rep.use_time(
            now,
            &body_motion(&c, MoveTimestamps::default()),
            &mut session,
        );
    }
    let standing_movements = rep.stats.movement_events;

    c.input = CharacterInput {
        forward: true,
        ..CharacterInput::default()
    };
    let start = c.position().frame.origin;
    while frame < 60 + 30 + 6 * 30 {
        frame += 1;
        let now = f64::from(frame) / 30.0;
        c.update(LocalTime(now));
        let before = rep.stats.position_events;
        rep.use_time(
            now,
            &body_motion(&c, MoveTimestamps::default()),
            &mut session,
        );
        if rep.stats.position_events != before {
            last_body_origin = c.position().frame.origin;
        }
    }
    let moved = ((c.position().frame.origin.x - start.x).powi(2)
        + (c.position().frame.origin.y - start.y).powi(2))
    .sqrt();

    let blobs = world_support::emitted(&session);
    let positions: Vec<&Vec<u8>> = blobs
        .iter()
        .filter(|(s, _)| *s == world_support::AUTONOMOUS_POSITION)
        .map(|(_, p)| p)
        .collect();
    // Six seconds on a one-second schedule, plus the immediate reports the cell grid forces.
    let rate = (5..=12).contains(&positions.len());
    let every_one_carries_contact = positions.iter().all(|p| {
        let m: MovementAutonomousPosition = world_support::decode(p);
        m.0.contact == 1
    });
    let last: MovementAutonomousPosition =
        world_support::decode(positions.last().expect("the walk reported something"));
    let carries_the_body = last.0.position.objcell_id == c.position().cell.0
        && last.0.position.frame.origin.x.to_bits() == last_body_origin.x.to_bits()
        && last.0.position.frame.origin.y.to_bits() == last_body_origin.y.to_bits();
    let movements = blobs
        .iter()
        .filter(|(s, _)| *s == world_support::MOVE_TO_STATE)
        .count();
    let one_edge = movements as u64 == standing_movements + 1;
    let the_edge_is_the_walk = {
        let m: MovementMoveToState = world_support::decode(
            blobs
                .iter()
                .filter(|(s, _)| *s == world_support::MOVE_TO_STATE)
                .nth(1)
                .map(|(_, p)| p)
                .expect("the walk is the second state edge"),
        );
        m.0.raw_motion_state.forward_command == Some(0x4500_0005) && m.0.contact
    };

    // --- the still arm ---------------------------------------------------------------------
    let mut still = world_support::settled_body(&store);
    let (mut rep2, mut session2) = world_support::reporter();
    let mut frame = 60u32;
    while frame < 60 + 6 * 30 {
        frame += 1;
        let now = f64::from(frame) / 30.0;
        still.update(LocalTime(now));
        rep2.use_time(
            now,
            &body_motion(&still, MoveTimestamps::default()),
            &mut session2,
        );
    }
    let idle = world_support::emitted(&session2);
    // 180 frames of standing still, two blobs: the reporter's own first cell change and the
    // first state edge. Say the denominator out loud.
    let silent = idle
        .iter()
        .filter(|(s, _)| *s == world_support::AUTONOMOUS_POSITION)
        .count()
        == 1
        && idle
            .iter()
            .filter(|(s, _)| *s == world_support::MOVE_TO_STATE)
            .count()
            == 1
        && idle.len() == 2;

    println!(
        "report: {moved:.2} m walked, {} position reports, {movements} state edges; a still \
         body sent {} blobs over 180 frames",
        positions.len(),
        idle.len()
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.position-report.a-walking-body-reports-where-it-is-and-a-still-one-falls-silent",
        move |_| {
            moved > 3.0
                && rate
                && every_one_carries_contact
                && carries_the_body
                && one_edge
                && the_edge_is_the_walk
                && silent
        },
    );
}

/// Every drawn frame reaches the position producer, and a client with nobody to report about
/// reports nothing.
///
/// A scenario may not skip for want of a graphics device, so this one is a whole headless client
/// through [`ClientSpec::retail`] -- the same client, without the device.
pub fn every_drawn_frame_reaches_the_position_reporter() {
    let mut c = HeadlessClient::new(ClientSpec::retail());
    let before = c.view().expect_app().probe().position_use_times();
    c.tick(3);

    c.assert_behaviour(
        "movement.position-report.every-drawn-frame-reaches-the-reporter-and-invents-nothing",
        move |v| {
            let app = v.expect_app();
            let s = app.probe().position_reporter_stats();
            before == 0
                && app.probe().position_use_times() == 3
                && app.probe().position_use_times() == app.frames_drawn()
                // With no link and no body there is nothing to report, and the reporter must
                // not invent one.
                && (s.position_events, s.movement_events, s.encode_failures) == (0, 0, 0)
        },
    );
    c.shutdown();
}

// -------------------------------------------------------------------------------------------
// movement.jump.*
// -------------------------------------------------------------------------------------------

/// A jump puts the body's own position and speed on the wire.
pub fn a_jump_reports_the_bodys_own_position_and_speed() {
    use pyreal_client_net::client_session::PositionReporter;
    use pyreal_client_runtime::app::body_motion;
    use pyreal_protocol::movement::{MoveTimestamps, MovementJump};
    use {
        pyreal_client_runtime::character::CharacterInput,
        pyreal_client_runtime::character::FULL_JUMP_EXTENT,
    };

    let store = support::store();
    let mut c = world_support::settled_body(&store);
    let (mut rep, mut session) = world_support::reporter();
    let stamps = MoveTimestamps {
        instance: 7,
        server_control: 3,
        teleport: 2,
        force_position: 0,
    };

    let before = c.position();
    let (accepted, velocity) = world_support::jump_frame(
        &mut c,
        61.0 / 30.0,
        CharacterInput {
            jump: true,
            ..CharacterInput::default()
        },
    );
    // The speed is read after the impulse, so it must carry the jump; a standing jump has no
    // horizontal component, like the recorded standing one.
    let impulse = velocity.z > 0.5 && velocity.x.abs() < 0.01 && velocity.y.abs() < 0.01;

    let motion = body_motion(&c, stamps);
    let pack = PositionReporter::jump_pack(FULL_JUMP_EXTENT, velocity, &motion);
    rep.send_jump_pack(pack, &mut session)
        .expect("the jump encodes");
    let one_blob = rep.stats.jump_events == 1 && session.transport.sent.len() == 1;

    let payload = session.transport.sent[0].payload.clone();
    let a = pyreal_protocol::actions::unpack_action(&payload).expect("a game action");
    let m: MovementJump = world_support::decode(&payload);
    let pos = c.position();
    let pack = a.sub_type.0 == world_support::JUMP
        && m.0.extent == FULL_JUMP_EXTENT
        && m.0.velocity == velocity
        && m.0.position.objcell_id == pos.cell.0
        && m.0.position.frame.origin.x == pos.frame.origin.x
        && m.0.position.frame.origin.y == pos.frame.origin.y
        && m.0.position.frame.origin.z == pos.frame.origin.z
        && m.0.timestamps == stamps
        // The whole blob is the header plus one pack, as both recorded ones are.
        && payload.len() == 12 + 56;
    let left_the_ground = !c.on_ground() && pos.frame.origin.z >= before.frame.origin.z;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.jump.a-jump-puts-the-bodys-own-position-and-speed-on-the-wire",
        move |_| accepted && impulse && one_blob && pack && left_the_ground,
    );
}

/// A second jump while the body is still in the air is refused, and nothing goes out for it.
pub fn a_second_jump_in_mid_air_sends_nothing() {
    use pyreal_client_net::client_session::PositionReporter;
    use pyreal_client_runtime::app::body_motion;
    use pyreal_protocol::movement::MoveTimestamps;
    use {
        pyreal_client_runtime::character::CharacterInput,
        pyreal_client_runtime::character::FULL_JUMP_EXTENT,
    };

    let store = support::store();
    let mut c = world_support::settled_body(&store);
    let (mut rep, mut session) = world_support::reporter();
    let stamps = MoveTimestamps::default();
    let jump = CharacterInput {
        jump: true,
        ..CharacterInput::default()
    };

    let (accepted, velocity) = world_support::jump_frame(&mut c, 61.0 / 30.0, jump);
    let pack = PositionReporter::jump_pack(FULL_JUMP_EXTENT, velocity, &body_motion(&c, stamps));
    rep.send_jump_pack(pack, &mut session)
        .expect("the first jump encodes");
    let first = accepted && rep.stats.jump_events == 1;
    // The subject is alive enough to fail: it really is off the ground.
    let airborne = !c.on_ground();

    let (accepted2, velocity2) = world_support::jump_frame(&mut c, 62.0 / 30.0, jump);
    // Only an accepted jump is sent, as the client's own jump release does.
    let nothing = !accepted2 || {
        let pack =
            PositionReporter::jump_pack(FULL_JUMP_EXTENT, velocity2, &body_motion(&c, stamps));
        rep.send_jump_pack(pack, &mut session).is_none()
    };
    let still_one = rep.stats.jump_events == 1 && session.transport.sent.len() == 1;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.jump.a-second-jump-in-mid-air-is-refused-and-nothing-goes-out",
        move |_| first && airborne && !accepted2 && nothing && still_one,
    );
}

/// A running jump carries its speed in the body's own frame, not the world's.
///
/// The body is turned first, which is the whole point: facing north the two frames coincide and
/// the measurement cannot tell them apart.
pub fn a_running_jump_carries_its_speed_in_the_bodys_own_frame() {
    use pyreal_client_runtime::character::CharacterInput;
    use pyreal_primitives::LocalTime;

    let store = support::store();
    let mut c = world_support::settled_body(&store);

    let turn = CharacterInput {
        turn_left: true,
        ..CharacterInput::default()
    };
    for i in 61..=100 {
        c.input = turn;
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    let heading = c.position().frame.rotation;
    let turned = heading.z.abs() > 0.05;

    let run = CharacterInput {
        forward: true,
        run: true,
        ..CharacterInput::default()
    };
    for i in 101..=190 {
        c.input = run;
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    let on_the_ground = c.on_ground();

    let (accepted, velocity) = world_support::jump_frame(
        &mut c,
        191.0 / 30.0,
        CharacterInput {
            forward: true,
            run: true,
            jump: true,
            ..CharacterInput::default()
        },
    );
    println!(
        "jump: a running jump at heading z={:.3} reports ({:.3}, {:.3}, {:.3})",
        heading.z, velocity.x, velocity.y, velocity.z
    );
    let forward = velocity.y > 1.0;
    let hardly_sideways = velocity.x.abs() < velocity.y.abs() * 0.25;
    let still_a_jump = velocity.z > 0.5;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.jump.a-running-jump-carries-its-speed-in-the-bodys-own-frame",
        move |_| turned && on_the_ground && accepted && forward && hardly_sideways && still_a_jump,
    );
}

/// The height the client works out for a jump explains every jump the recordings carry.
///
/// The extents and the speeds are **read out of the recordings at run time**, and no one jump skill
/// can be named for them: the eighteen recordings are not all one character, and the jump skill is
/// an input to the height. So the measurement is the one that does not depend on knowing it -- for
/// every recorded jump there is a jump skill that reproduces its speed from its extent exactly, bit
/// for bit, through the client's own height.
///
/// Both halves of the population are asserted, because they are different measurements: a jump
/// on the shortest height the client allows is explained by a whole range of skills, and one
/// above that floor is explained by exactly one. Without the second the claim would be about the
/// floor and not about the formula.
pub fn the_clients_jump_height_explains_the_recorded_jumps() {
    use pyreal_animation::motion::get_jump_height;

    /// The client's own shortest jump.
    const FLOOR: f32 = 0.35;
    /// The range of jump skills searched. A shipped character's is inside it by a wide margin.
    const SKILLS: std::ops::RangeInclusive<i32> = 1..=1000;

    let recorded = world_support::recorded_jumps();
    let fits = |extent: f32, speed: f32| -> Vec<i32> {
        SKILLS
            .filter(|s| {
                (get_jump_height(0.0, *s, extent, 1.0) * 19.6)
                    .sqrt()
                    .to_bits()
                    == speed.to_bits()
            })
            .collect()
    };

    let mut explained = 0usize;
    let mut floored = 0usize;
    let mut pinned = 0usize;
    let mut unexplained = 0usize;
    for (extent, speed) in &recorded {
        let skills = fits(*extent, *speed);
        if skills.is_empty() {
            // Not a failure of the formula: the height also takes what the jumper is carrying
            // and how big he is, and a recording carries neither, so a jump made under a load
            // cannot be reproduced from its extent alone. Counted and printed rather than
            // folded away, because it is the one thing in this measurement a later reader
            // would otherwise have to rediscover.
            unexplained += 1;
            println!(
                "jump height: extent {extent} speed {speed} is explained by no unburdened skill"
            );
            continue;
        }
        explained += 1;
        if get_jump_height(0.0, skills[0], *extent, 1.0) == FLOOR {
            floored += 1;
        } else if skills.len() == 1 {
            pinned += 1;
        }
    }
    println!(
        "jump: {} recorded jumps, {explained} explained by an unburdened jumper's own \
         skill ({floored} on the client's floor, {pinned} pinning one skill exactly), \
         {unexplained} not",
        recorded.len()
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "movement.jump.the-height-the-client-works-out-explains-the-recorded-jumps",
        move |_| explained >= 2 && floored > 0 && pinned > 0,
    );
}

/// Every drawn frame visits the jump dispatch, and a client nobody is pressing jump on invents
/// none.
pub fn every_drawn_frame_visits_the_jump_dispatch() {
    let mut c = HeadlessClient::new(ClientSpec::retail());
    c.tick(3);

    c.assert_behaviour(
        "movement.jump.every-drawn-frame-visits-the-jump-dispatch-and-invents-none",
        move |v| {
            let app = v.expect_app();
            app.probe().jump_use_times() == 3
                && app.probe().jump_use_times() == app.frames_drawn()
                // Nobody pressed jump, so nothing may have been invented.
                && app.probe().jump_counts() == (0, 0)
                && app.probe().position_reporter_stats().jump_events == 0
        },
    );
    c.shutdown();
}

/// The recording whose server changes the standing player's stance four times.
const STANCE_SESSION: &str = "combat-mode-while-moving";

/// The four recorded `0xF74C`s, in seconds from the recording's start, that change the player's
/// stance after his own `0x0053`.
const SERVER_STANCES: [f64; 4] = [19.50, 22.76, 29.74, 33.08];

/// What a paced replay saw: `(client time, action)` sent, and `(recorded time, client time)` arrived.
pub type Paced = (Vec<(f64, u32)>, Vec<(f64, f64)>);

/// What the client sent while `session` replayed into a whole client at the recording's own pace
/// up to `until` seconds, `(client time, action)`, and the client time each recorded datagram
/// arrived at, `(recorded time, client time)`.
pub fn paced_replay(session: &str, until: f64) -> Paced {
    use pyreal_testkit::{corpus_steps, replay};
    let recs = replay::records(session);
    let t0 = recs.first().map_or(0.0, |r| r.t);
    let times: Vec<f64> = recs.iter().filter(|r| !r.c2s).map(|r| r.t - t0).collect();
    let mut c = HeadlessClient::new(ClientSpec::simulated());
    c.attach_replay(replay::recorded_endpoint(&recs));
    let mut sent = Vec::new();
    let mut arrived = Vec::new();
    let mut seen = 0usize;
    let mut offset = None;
    let mut log = |c: &mut HeadlessClient, sent: &mut Vec<(f64, u32)>| {
        let now = c.wire_clock().0;
        let wire = c.outbound_wire();
        sent.extend(wire[seen..].iter().map(|&a| (now, a)));
        seen = wire.len();
    };
    corpus_steps::sweep_until(&mut c, session, |c| {
        let k = arrived.len();
        if times[k] > until {
            return Some(());
        }
        let now = c.wire_clock().0;
        arrived.push((times[k], now));
        log(c, &mut sent);
        let offset = *offset.get_or_insert(now - times[k]);
        if let Some(next) = times.get(k + 1) {
            while c.wire_clock().0 + 1.0 / 30.0 < offset + next {
                c.tick(1);
                log(c, &mut sent);
            }
        }
        None
    });
    c.shutdown();
    (sent, arrived)
}

/// A stance the server sets on the standing player goes out as no `0xF61C`, so the next
/// autonomous position is not held back a second behind one, as the recording's client did.
pub fn a_server_stance_change_is_not_echoed() {
    const MOVE_TO_STATE: u32 = 0xF61C;
    const AUTONOMOUS_POSITION: u32 = 0xF753;
    // Stopped well past the last stance change and before the recording's own log-off.
    let (sent, arrived) = paced_replay(STANCE_SESSION, 40.0);
    let at = |recorded: f64| {
        arrived
            .iter()
            .find(|(t, _)| *t >= recorded - 0.005)
            .map(|&(_, c)| c)
            .expect("the recording reaches the stance change")
    };
    let mut echoes = Vec::new();
    let mut gaps = Vec::new();
    for s in SERVER_STANCES {
        let c = at(s);
        let window: Vec<_> = sent
            .iter()
            .filter(|(t, a)| *a == MOVE_TO_STATE && *t > c && *t <= c + 0.3)
            .collect();
        echoes.push(window.len());
        // An echo stamps the position report's clock and holds the next one back a full second.
        let next = sent
            .iter()
            .find(|(t, a)| *a == AUTONOMOUS_POSITION && *t > c)
            .map(|&(t, _)| t - c);
        gaps.push(next);
        println!(
            "stance at {s:.2} s (client {c:.2}): {} 0xF61C within 0.3 s; next 0xF753 after {:?} \
             s; sent around it {:?}",
            window.len(),
            gaps.last(),
            sent.iter()
                .filter(|(t, _)| (c - 1.5..c + 1.5).contains(t))
                .map(|&(t, a)| (format!("{:.2}", t - c), format!("{a:04X}")))
                .collect::<Vec<_>>()
        );
    }
    let positions = sent
        .iter()
        .filter(|(_, a)| *a == AUTONOMOUS_POSITION)
        .count();
    let silent = echoes.iter().all(|n| *n == 0);
    let on_beat = gaps
        .iter()
        .all(|g| g.is_some_and(|g| g <= 1.0 + 2.0 / 30.0));
    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "movement.position-report.a-server-stance-change-is-not-echoed-and-keeps-the-beat",
        move |_| positions >= SERVER_STANCES.len() && silent && on_beat,
    );
}
