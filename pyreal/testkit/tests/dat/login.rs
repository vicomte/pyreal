//! The `dat` tier's login scenarios: the whole client running with no device and no window, and
//! the rule that two bodies never share one object id (fixture: the `first-login-walk-jump` recording replayed
//! through a socket-free endpoint, on the retail dats).
//!
//! Every scenario here builds a whole client -- `App::with_platform(cfg, NullPresentation,
//! Platform::headless(w, h))` -- and **this binary must run serially**.
//!
//! Each scenario is a `pub fn` with a `#[test]` beside it that runs it and checks its declaration.
//! `ALL` is this file's own list, concatenated with the other subjects' in `census.rs`, so a
//! scenario that is written and not listed shows up as a shortfall rather than as a silent gap.

use pyreal_client_net::client_session::testing::capture::Datagram;
use pyreal_client_net::client_session::SessionEvent;
use pyreal_client_runtime::frame::FrameStep;
use pyreal_client_runtime::net::ClientNetwork;
use pyreal_client_runtime::objects::ObjectStream;
use pyreal_primitives::{LocalTime, ObjectId};
use pyreal_testkit::{ClientSpec, HeadlessClient, ScenarioView};
use {
    pyreal_client_runtime::present::NullPresentation,
    pyreal_client_runtime::present::NullPresentationCounts,
};

/// The recording the identity scenario replays, by its content slug.
const FIRST_LOGIN: &str = "first-login-walk-jump";

/// The recording, and the endpoint it replays into, both from `pyreal_testkit::replay`.
use pyreal_testkit::replay::records as recording;

/// A socket-free endpoint named after the recording's own connection sequence number.
fn endpoint(records: &[Datagram], account: &str) -> ClientNetwork {
    pyreal_testkit::replay::recorded_endpoint_as(records, account, "unused")
}

/// What the null presentation was asked to do.
fn counts(v: &ScenarioView<'_>) -> NullPresentationCounts {
    v.expect_app()
        .presentation()
        .as_any()
        .downcast_ref::<NullPresentation>()
        .expect("the presentation this client was built with")
        .counts()
}

// -------------------------------------------------------------------------------------------
// 12. headless.a-whole-client-with-no-device-or-window
// -------------------------------------------------------------------------------------------

/// The whole client runs with no device and no window, on a clock that is exactly its own steps.
pub fn the_count_a_world_gives_at_login_is_passed_on() {
    let records = recording(FIRST_LOGIN);
    let told = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let listening = std::rc::Rc::clone(&told);
    let mut c = HeadlessClient::new(ClientSpec::retail());
    c.app_mut().on_world_players(Box::new(move |players| {
        listening.borrow_mut().push(players)
    }));
    c.attach_replay(endpoint(&records, "seam"));
    // Up to a little past the count: the recording goes on to log off.
    let mut after = 0;
    for r in records.iter().filter(|r| !r.c2s) {
        let app = c.app_mut();
        let now = LocalTime(app.clock().local_time);
        let net = app.replay_network_mut().expect("the endpoint is attached");
        net.feed(&r.raw, r.peer(), now);
        assert!(app.frame(), "the client stopped during the log-in");
        after += usize::from(!told.borrow().is_empty());
        if after > 10 {
            break;
        }
    }
    c.assert_behaviour(
        "net.player-count.the-count-a-world-gives-at-login-is-passed-on",
        move |_| told.borrow().len() == 1,
    );
}

pub fn a_whole_client_with_no_device_or_window() {
    use pyreal_client_runtime::platform::clock::HEADLESS_STEP;

    let mut c = HeadlessClient::new(ClientSpec::retail());
    let before = counts(&c.view());
    c.tick(2);

    c.assert_behaviour(
        "headless.a-whole-client-with-no-device-or-window",
        move |v| {
            let app = v.expect_app();
            let after = counts(v);
            let per_frame = |f: fn(&NullPresentationCounts) -> u64| f(&after) - f(&before) == 2;
            let drew = per_frame(|c| c.prepare_graphics_device)
                && per_frame(|c| c.start_frame)
                && per_frame(|c| c.draw_scene)
                // The interface and the host's controls are drawn as an overlay.
                && per_frame(|c| c.draw_overlay)
                && per_frame(|c| c.end_frame);
            // Two frames of the fixed-step clock are two steps, which no wall clock can be.
            let simulated = (app.clock().cur_time - 2.0 * HEADLESS_STEP).abs() < 1e-12
                && app.clock().external_offset() == 0.0;
            // The seed rectangle is the default 800x600 window centred on the NullWindow's
            // 1920x1080 virtual desktop: the client starts windowed and full screen follows
            // gameplay, so this is not the whole desktop.
            let no_window = app.presentation().size() == (800, 600)
                && app
                    .window_rect()
                    .is_some_and(|r| (r.left, r.top, r.right, r.bottom) == (560, 240, 1360, 840));
            drew && simulated && no_window && app.last_frame_steps() == FrameStep::ORDER
        },
    );
    c.shutdown();
}

pyreal_testkit::scenarios! {
    scenario_a_whole_client_with_no_device_or_window => a_whole_client_with_no_device_or_window ["headless.a-whole-client-with-no-device-or-window"],
    scenario_the_count_a_world_gives_at_login_is_passed_on => the_count_a_world_gives_at_login_is_passed_on ["net.player-count.the-count-a-world-gives-at-login-is-passed-on"],
    scenario_two_bodies_cannot_share_one_id => two_bodies_cannot_share_one_id ["login.player-identity.two-bodies-cannot-share-one-id"],
    scenario_any_front_end_tells_the_server_the_character_arrived => any_front_end_tells_the_server_the_character_arrived ["login.arrival.any-front-end-tells-the-server-the-character-arrived"],
    scenario_any_front_end_logs_the_character_off_when_it_quits => any_front_end_logs_the_character_off_when_it_quits ["logout.quit.leaving-the-game-logs-the-character-off-before-the-client-stops"],
    scenario_a_front_ends_own_exit_from_the_world_saves_the_settings_then_logs_off => a_front_ends_own_exit_from_the_world_saves_the_settings_then_logs_off ["logout.quit.an-exit-from-the-world-saves-the-characters-settings-before-the-client-stops"],
    scenario_an_exit_before_the_world_stops_at_once_and_sends_nothing => an_exit_before_the_world_stops_at_once_and_sends_nothing ["logout.quit.an-exit-before-the-world-stops-the-client-at-once"],
    scenario_every_front_end_follows_one_game_flow => every_front_end_follows_one_game_flow ["login.phase.every-front-end-follows-one-game-flow"],
    scenario_a_boot_ends_the_session_with_the_servers_reason_whatever_draws_it => a_boot_ends_the_session_with_the_servers_reason_whatever_draws_it ["login.disconnect.a-boot-ends-the-session-with-the-servers-reason-whatever-draws-it"],
}

// -------------------------------------------------------------------------------------------
// login.player-identity.two-bodies-cannot-share-one-id
// -------------------------------------------------------------------------------------------

/// Replay a recording through the client's own three lines and hand back the object stream at its
/// fullest, with the identity the shard gave the player.
///
/// **Stopped before the log-off**, because every recording ends with one and the client empties
/// the world when a character session ends.
fn replay_to_the_world(session: &str) -> (ObjectStream, ObjectId) {
    let records = recording(session);
    let run = |limit: usize| -> (ObjectStream, usize) {
        let mut net = endpoint(&records, "pyreal-testkit");
        let mut objects = ObjectStream::new();
        let mut entered = false;
        let mut fullest = 0usize;
        for (index, r) in records.iter().enumerate() {
            if index >= limit {
                break;
            }
            let now = LocalTime(r.t);
            if !r.c2s {
                net.feed(&r.raw, r.peer(), now);
            }
            net.tick(now);
            let _ = net.take_outgoing();
            for e in objects.pump(&mut net, now) {
                if let SessionEvent::CharacterSet(set) = &e {
                    if !entered {
                        if let Some(ch) = set.characters.first() {
                            let account = set.account.clone();
                            net.enter_world(ch.gid, &account);
                            entered = true;
                        }
                    }
                }
            }
            if !objects.is_empty() {
                fullest = index;
            }
        }
        (objects, fullest)
    };
    let (_, fullest) = run(usize::MAX);
    let (stream, _) = run(fullest + 1);
    let player = stream
        .player()
        .unwrap_or_else(|| panic!("{session} never put a character in the world"));
    (stream, player)
}

/// Two bodies under one id leave the first out of the sweep, which is why a re-key onto an
/// occupied id is refused.
pub fn two_bodies_cannot_share_one_id() {
    use pyreal_physics::source::StaticLandSource;
    use pyreal_physics::{PhysicsWorld, SetupGeometry, Sphere};
    use pyreal_primitives::{Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};
    use {
        pyreal_client_runtime::character::Character,
        pyreal_client_runtime::character::PLAYER_OBJECT_ID,
    };

    /// The id this client's own body carries before a shard has named it, and the one a shard
    /// allocates first -- which is what made the collision reachable at all.
    const FIRST_SHARD_ID: ObjectId = ObjectId(0x5000_0001);

    fn a_sphere() -> std::sync::Arc<SetupGeometry> {
        std::sync::Arc::new(SetupGeometry {
            spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
            sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 0.5), 1.0),
            radius: 0.5,
            height: 1.0,
            ..SetupGeometry::default()
        })
    }

    // 1. The mechanism, stated as a measurement: the id table replaces rather than rejects, and
    //    the body that loses its key is simply never stepped again.
    let block = LandblockId(0xA9B4);
    let mut land = StaticLandSource::linear();
    land.add_flat_block(block, 10);
    let mut w = PhysicsWorld::new(std::sync::Arc::new(land));
    let place = |w: &mut PhysicsWorld, id: ObjectId| {
        let h = w.create(id, a_sphere(), true);
        let cell = block.cell(1);
        w.enter_cell(h, cell);
        let o = w.get_mut(h).expect("live");
        o.position = Position::new(
            cell,
            Frame::new(Vec3::new(96.0, 96.0, 100.0), Quat::IDENTITY),
        );
        o.transient_state.set_active_bit(true);
        o.update_time = 0.0;
        h
    };
    let first = place(&mut w, FIRST_SHARD_ID);
    let second = place(&mut w, FIRST_SHARD_ID);
    let replaced = w.by_object_id(FIRST_SHARD_ID) == Some(second);
    let swept = w.use_time(LocalTime(1.0), false);
    let first_is_out_of_the_sweep = w
        .get(first)
        .expect("still alive, merely unreachable")
        .update_time
        == 0.0;
    let second_is_in_it = w.get(second).expect("live").update_time > 0.0;

    // 2. And that is why a re-key onto an id another body already holds changes nothing and is
    //    counted -- otherwise the fix would reintroduce the eviction from the other direction.
    let store = std::sync::Arc::new(pyreal_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail data files are this scenario's oracle and there are none under {} -- set \
             PYREAL_TEST_DAT_DIR",
            pyreal_dat::testing::dat_dir().display()
        )
    }));
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region decodes");
    let mut body = Character::new(&store, &region, block.0, (96.0, 96.0)).expect("the body builds");
    let occupied = ObjectId(0x5000_0009);
    let sitting = body.world.create(occupied, a_sphere(), true);
    let refused = !body.adopt_server_id(Some(occupied))
        && body.object_id() == PLAYER_OBJECT_ID
        && body.stats.id_adoptions == 0
        && body.stats.id_adoptions_refused == 1
        && body.world.by_object_id(PLAYER_OBJECT_ID) == Some(body.handle)
        && body.world.by_object_id(occupied) == Some(sitting);

    // **What is deliberately not here, and why.** One more arm belongs to this claim: the same
    // recording with a second character standing on the recorded player's own spot, run twice --
    // once with the local body still holding the placeholder and once with it holding the
    // shard's id -- to show that the second character is refused a body in the first case and is
    // an ordinary solid one in the second. Its fixture does not build that yet: a presence that
    // fills only the body's achieved pose and not the last pose the wire named creates no body at
    // all, so the arm is not written here until the fixture fills both.

    // 4. And the whole recording made solid around it leaves the local body answering to its id
    //    and still in the sweep.
    let (mut stream, player) = replay_to_the_world(FIRST_LOGIN);
    let pos = stream
        .presence(player)
        .and_then(|p| p.position)
        .expect("the player is placed");
    let block = pos.cell.landblock();
    let mut c = Character::new(&store, &region, block.0, (96.0, 96.0)).expect("the body builds");
    c.land().load_block_cells(block);
    c.teleport(pos);
    c.adopt_server_id(Some(player));
    stream.sync_physics(&store, &mut c.world);
    let st = stream.physics.stats;
    let the_corpus_was_made_solid = st.created > 0 && st.id_collision == 0;
    let before = c.world.get(c.handle).expect("live").update_time;
    let swept_after = c.world.use_time(LocalTime(before + 1.0), false)
        && c.world.get(c.handle).expect("live").update_time > before;
    let the_local_body_survives = c.world.by_object_id(player) == Some(c.handle) && swept_after;

    for (what, ok) in [
        ("the second insert replaced the first", replaced),
        ("the sweep ran", swept),
        (
            "the first body was never stepped",
            first_is_out_of_the_sweep,
        ),
        ("the second was", second_is_in_it),
        ("a re-key onto an occupied id is refused", refused),
        ("the corpus was made solid", the_corpus_was_made_solid),
        ("the local body survives", the_local_body_survives),
    ] {
        println!("identity: {what}: {ok}");
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "login.player-identity.two-bodies-cannot-share-one-id",
        move |_| {
            replaced
                && swept
                && first_is_out_of_the_sweep
                && second_is_in_it
                && refused
                && the_corpus_was_made_solid
                && the_local_body_survives
        },
    );
}

// -------------------------------------------------------------------------------------------
// login.arrival.any-front-end-tells-the-server-the-character-arrived
// logout.quit.leaving-the-game-logs-the-character-off-before-the-client-stops
// -------------------------------------------------------------------------------------------

/// The character log-in-complete notification, the action that tells the server the character
/// has arrived.
const LOGIN_COMPLETE: u32 = 0x00A1;
/// The character log-off message.
const LOG_OFF: u32 = 0xF653;

/// A front end with a UI and nothing else: it draws nothing, answers every question the frame
/// asks with the default, and runs none of the game's per-frame duties itself. What it asks for
/// it asks for through the runtime's request queue, as any front end may.
#[derive(Debug, Default)]
struct BareLayer;

impl pyreal_client_runtime::shell::Shell for BareLayer {
    type Hud = pyreal_client_runtime::shell::PlainHud;
    type Present = dyn pyreal_client_runtime::present::Presentation;

    fn has_ui(&self) -> bool {
        true
    }
}

/// What a client did, frame by frame: everything it sent, and the game phase it was in.
#[derive(Debug, Default)]
struct Seen {
    wire: pyreal_testkit::wire::Wire,
    phases: Vec<pyreal_client_contract::pregame::GamePhase>,
}

impl Seen {
    /// Take what the client sent this frame, and the phase it is in.
    fn observe<S: pyreal_client_runtime::shell::Shell>(
        &mut self,
        app: &mut pyreal_client_runtime::app::App<S>,
    ) {
        let now = LocalTime(app.clock().local_time);
        if let Some(net) = app.replay_network_mut() {
            self.wire.ingest(&net.take_outgoing(), now);
        }
        let phase = app.host_state().phase.clone();
        if self.phases.last() != Some(&phase) {
            self.phases.push(phase);
        }
    }
}

/// A runtime client under front end `S`, with a UI when `ui`, over the retail dats and a simulated
/// world, with no server yet.
fn brought_up<S>(shell: &mut S, ui: bool) -> pyreal_client_runtime::app::App<S>
where
    S: pyreal_client_runtime::shell::Shell<
        Present = dyn pyreal_client_runtime::present::Presentation,
    >,
{
    use pyreal_client_runtime::app::{App, Platform};
    use pyreal_client_runtime::config::Config;
    use pyreal_client_runtime::present::Presentation;

    let (w, h) = (800, 600);
    let cfg = Config {
        headless: true,
        connect: false,
        sound: false,
        ui,
        width: w,
        height: h,
        dat_dir: pyreal_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir()
            .join("pyreal-seam-login-not-created")
            .join("prefs.ini"),
        ..Config::default()
    };
    // The world is simulated with no device, so the body has cells to stand in: the arrival
    // tunnel ends when the body stands in a loaded scene.
    let mut app = App::<S>::bring_up(
        cfg,
        |_| Ok(Platform::headless(w, h)),
        |_, _, _, _| {
            Ok(
                Box::new(pyreal_client_runtime::sim_present::SimPresentation::new(
                    w, h,
                )) as Box<dyn Presentation>,
            )
        },
    )
    .expect("a headless client needs the retail dats under $PYREAL_TEST_DAT_DIR");
    app.start_shell(shell).expect("the front end starts");
    app.defer_static_scene(pyreal_client_runtime::scene::SceneConfig {
        character: true,
        ..pyreal_client_runtime::scene::SceneConfig::default()
    });
    app
}

/// A runtime client under front end `S`, with a UI when `ui`, logged into the recorded session
/// and standing in the world: the front end asked for the first character through
/// `UiRequest::CharacterAction`, and the recording's server datagrams have been fed one per frame
/// up to the one that created the character, and no further, because the recording goes on to
/// log off. `seen` holds everything the client sent and every phase it went through.
fn logged_in<S>(shell: &mut S, ui: bool, seen: &mut Seen) -> pyreal_client_runtime::app::App<S>
where
    S: pyreal_client_runtime::shell::Shell<
        Present = dyn pyreal_client_runtime::present::Presentation,
    >,
{
    use pyreal_client_contract::pregame::CharacterAction;
    use pyreal_client_contract::UiRequest;
    use pyreal_client_net::client_session::SessionState;

    let mut app = brought_up(shell, ui);
    let records = recording(FIRST_LOGIN);
    app.attach_replay_network(endpoint(&records, "seam"))
        .expect("a fresh client has no link");
    let mut asked = false;
    for r in records.iter().filter(|r| !r.c2s) {
        let now = LocalTime(app.clock().local_time);
        let net = app.replay_network_mut().expect("the endpoint is attached");
        net.feed(&r.raw, r.peer(), now);
        assert!(app.frame(shell), "the client stopped during the log-in");
        seen.observe(&mut app);
        let net = app.replay_network_mut().expect("the endpoint is attached");
        if !asked && net.session_state() == SessionState::CharacterSelect {
            let gid = net
                .characters()
                .characters
                .first()
                .expect("the recording's account has a character")
                .gid;
            app.submit_requests(vec![UiRequest::CharacterAction(CharacterAction::LogOn(
                gid,
            ))]);
            asked = true;
        }
        if app.objects().player().is_some() {
            break;
        }
    }
    assert!(asked, "the recording reaches character select");
    assert!(
        app.objects().player().is_some(),
        "the recording puts the character in the world"
    );
    app
}

/// Run `max` frames, feeding `seen` as they go. Answers whether the client was still running at
/// the end.
fn run_frames<S>(
    app: &mut pyreal_client_runtime::app::App<S>,
    shell: &mut S,
    seen: &mut Seen,
    max: u32,
) -> bool
where
    S: pyreal_client_runtime::shell::Shell<
        Present = dyn pyreal_client_runtime::present::Presentation,
    >,
{
    for _ in 0..max {
        let running = app.frame(shell);
        seen.observe(app);
        if !running {
            return false;
        }
    }
    true
}

/// How many times `opcode` went out as an action.
fn actions(wire: &pyreal_testkit::wire::Wire, opcode: u32) -> usize {
    wire.sub_types().iter().filter(|s| **s == opcode).count()
}

/// The runtime ticks the arrival tunnel whatever the front end does, so the server is told the
/// character arrived by a front end that never heard of the tunnel and by one with no UI at all.
pub fn any_front_end_tells_the_server_the_character_arrived() {
    // Ten simulated seconds after the last recorded datagram: the tunnel is a few seconds long.
    const FRAMES: u32 = 600;

    let mut bare = BareLayer;
    let mut seen = Seen::default();
    let mut app = logged_in(&mut bare, true, &mut seen);
    run_frames(&mut app, &mut bare, &mut seen, FRAMES);
    let bare_sent = actions(&seen.wire, LOGIN_COMPLETE);
    app.shutdown(&mut bare);

    let mut null = pyreal_client_runtime::shell::NullShell;
    let mut seen = Seen::default();
    let mut app = logged_in(&mut null, false, &mut seen);
    run_frames(&mut app, &mut null, &mut seen, FRAMES);
    let null_sent = actions(&seen.wire, LOGIN_COMPLETE);
    app.shutdown(&mut null);

    // Retail sends one or two, as the tunnel and the settling of the body fall: both are in the
    // recorded sessions. What is asserted is that it goes out, and that the front end does not
    // change how often.
    assert!(
        bare_sent >= 1 && bare_sent == null_sent,
        "the arrival is reported under a bare front end ({bare_sent}) and under none ({null_sent}), the same number of times"
    );
    pyreal_testkit::claims::note_asserted(
        "login.arrival.any-front-end-tells-the-server-the-character-arrived",
    );
}

/// `UiRequest::Quit` from a front end with no epilogue screen of its own: the log-off goes out,
/// and then the loop ends.
pub fn any_front_end_logs_the_character_off_when_it_quits() {
    let mut bare = BareLayer;
    let mut seen = Seen::default();
    let mut app = logged_in(&mut bare, true, &mut seen);
    let before = seen.wire.messages().len();
    app.submit_requests(vec![pyreal_client_contract::UiRequest::Quit]);
    let running = run_frames(&mut app, &mut bare, &mut seen, 10);
    let after: Vec<u32> = seen.wire.messages()[before..].to_vec();
    app.shutdown(&mut bare);
    assert!(!running, "the client stops after a quit");
    assert!(
        after.contains(&LOG_OFF),
        "the character's log-off went out before the client stopped: {after:08X?}"
    );
    pyreal_testkit::claims::note_asserted(
        "logout.quit.leaving-the-game-logs-the-character-off-before-the-client-stops",
    );
}

/// The character-options save, as an action.
const CHARACTER_OPTIONS: u32 = 0x01A1;

/// A front end's own Exit, pressed in the world after a change the client keeps until the way
/// out: the settings go to the server, then the log-off, and the client stops a frame later.
pub fn a_front_ends_own_exit_from_the_world_saves_the_settings_then_logs_off() {
    let mut bare = BareLayer;
    let mut seen = Seen::default();
    let mut app = logged_in(&mut bare, true, &mut seen);
    run_frames(&mut app, &mut bare, &mut seen, 2);
    // A change the client keeps to itself until the way out, such as where a window sits, only
    // marks the settings as changed; nothing goes to the server for it at the time.
    let now = pyreal_primitives::ServerTime(app.clock().cur_time);
    let player = &mut app.probe_mut().objects_mut().world.player_system;
    assert!(
        player.module.is_some(),
        "the recording gave the character its settings"
    );
    player.mark_dirty(now);
    let before = (seen.wire.sub_types().len(), seen.wire.messages().len());
    app.ui_context().quit_game();
    let running = run_frames(&mut app, &mut bare, &mut seen, 10);
    let actions_after = seen.wire.sub_types()[before.0..].to_vec();
    let messages_after = seen.wire.messages()[before.1..].to_vec();
    let saved_at_logout = app.probe().player_modules_saved_at_logout();
    app.shutdown(&mut bare);
    assert!(!running, "the client stops after the Exit");
    assert!(
        actions_after.contains(&CHARACTER_OPTIONS) && saved_at_logout == 1,
        "the settings went out on the way out: {actions_after:04X?}, saved {saved_at_logout}"
    );
    assert!(
        messages_after.contains(&LOG_OFF),
        "the character's log-off went out before the client stopped: {messages_after:08X?}"
    );
    pyreal_testkit::claims::note_asserted(
        "logout.quit.an-exit-from-the-world-saves-the-characters-settings-before-the-client-stops",
    );
}

/// A front end's own Exit at character select: no character to log off, no settings to save, and
/// the client stops on that frame.
pub fn an_exit_before_the_world_stops_at_once_and_sends_nothing() {
    use pyreal_client_net::client_session::SessionState;

    let mut bare = BareLayer;
    let mut seen = Seen::default();
    let mut app = brought_up(&mut bare, true);
    let records = recording(FIRST_LOGIN);
    app.attach_replay_network(endpoint(&records, "seam"))
        .expect("a fresh client has no link");
    for r in records.iter().filter(|r| !r.c2s) {
        let now = LocalTime(app.clock().local_time);
        let net = app.replay_network_mut().expect("the endpoint is attached");
        net.feed(&r.raw, r.peer(), now);
        assert!(app.frame(&mut bare), "the client stopped during the log-in");
        seen.observe(&mut app);
        let net = app.replay_network_mut().expect("the endpoint is attached");
        if net.session_state() == SessionState::CharacterSelect {
            break;
        }
    }
    let before = (seen.wire.sub_types().len(), seen.wire.messages().len());
    app.ui_context().quit_game();
    let running = app.frame(&mut bare);
    seen.observe(&mut app);
    let actions_after = seen.wire.sub_types()[before.0..].to_vec();
    let messages_after = seen.wire.messages()[before.1..].to_vec();
    app.shutdown(&mut bare);
    assert!(!running, "the client stops on the frame of the Exit");
    assert!(
        !actions_after.contains(&CHARACTER_OPTIONS) && !messages_after.contains(&LOG_OFF),
        "nothing of a log-off went out: {actions_after:04X?} {messages_after:08X?}"
    );
    pyreal_testkit::claims::note_asserted(
        "logout.quit.an-exit-before-the-world-stops-the-client-at-once",
    );
}

// -------------------------------------------------------------------------------------------
// login.phase.every-front-end-follows-one-game-flow
// login.disconnect.a-boot-ends-the-session-with-the-servers-reason-whatever-draws-it
// -------------------------------------------------------------------------------------------

/// The recording whose server boots the account.
const BOOTED: &str = "login-account-booted";

/// The game phase is the runtime's: a front end that draws nothing and decides nothing goes from
/// connecting to the character list, into the world and, after a quit, out of it, on the same
/// edges the retail screens take.
pub fn every_front_end_follows_one_game_flow() {
    use pyreal_client_contract::pregame::GamePhase;

    let mut bare = BareLayer;
    let mut seen = Seen::default();
    let mut app = logged_in(&mut bare, true, &mut seen);
    run_frames(&mut app, &mut bare, &mut seen, 600);
    app.submit_requests(vec![pyreal_client_contract::UiRequest::Quit]);
    run_frames(&mut app, &mut bare, &mut seen, 10);
    app.shutdown(&mut bare);
    let order = |p: &GamePhase| seen.phases.iter().position(|q| q == p);
    let (select, entering, world, leaving) = (
        order(&GamePhase::CharacterSelect),
        order(&GamePhase::EnteringWorld),
        order(&GamePhase::InWorld),
        order(&GamePhase::LoggingOff),
    );
    assert!(
        seen.phases.first() == Some(&GamePhase::Connecting)
            && select.is_some()
            && select < entering
            && entering < world
            && world < leaving,
        "the phases were {:?}",
        seen.phases
    );
    pyreal_testkit::claims::note_asserted("login.phase.every-front-end-follows-one-game-flow");
}

/// The runtime resolves why the session ended, so a front end that resolves nothing still shows
/// the server's own reason for a boot.
pub fn a_boot_ends_the_session_with_the_servers_reason_whatever_draws_it() {
    use pyreal_client_contract::pregame::{DisconnectNotice, GamePhase};

    let mut bare = BareLayer;
    let mut seen = Seen::default();
    let mut app = brought_up(&mut bare, true);
    let records = recording(BOOTED);
    app.attach_replay_network(endpoint(&records, "seam"))
        .expect("a fresh client has no link");
    for r in records.iter().filter(|r| !r.c2s) {
        let now = LocalTime(app.clock().local_time);
        app.replay_network_mut()
            .expect("the endpoint is attached")
            .feed(&r.raw, r.peer(), now);
        if !app.frame(&mut bare) {
            break;
        }
        seen.observe(&mut app);
    }
    run_frames(&mut app, &mut bare, &mut seen, 3);
    let host = app.host_state().clone();
    app.shutdown(&mut bare);
    let Some(GamePhase::Disconnected(DisconnectNotice::Booted(reason))) = seen.phases.last() else {
        panic!("the session did not end in a boot: {:?}", seen.phases);
    };
    assert_eq!(
        host.error.as_deref(),
        Some(
            pyreal_client_contract::disconnect::account_booted_message(reason.as_deref()).as_str()
        ),
        "the disconnected screen shows the boot's own sentence"
    );
    pyreal_testkit::claims::note_asserted(
        "login.disconnect.a-boot-ends-the-session-with-the-servers-reason-whatever-draws-it",
    );
}
