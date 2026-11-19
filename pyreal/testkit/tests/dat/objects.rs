//! Objects: the object stream over real setups and real cells.
//!
//! Fixture: the retail dats under `$PYREAL_TEST_DAT_DIR`, and the recordings
//! `pyreal_client_net::client_session::testing::session_index()` names, replayed through the real
//! session. Most scenarios open the data files directly and book the claim at the end; no whole
//! `App` is built. No count is pinned: every denominator is read off the recordings.
//!
//! **This binary must run serially.**

use std::collections::BTreeMap;
use std::sync::Arc;

use pyreal_client_net::client_session::SessionEvent;
use pyreal_client_runtime::objects::ObjectStream;
use pyreal_dat::RetailDatStore;
use pyreal_physics::{LandSource, PhysicsWorld};
use pyreal_primitives::{CellId, LocalTime, ObjectId, ServerTime};
use pyreal_protocol::objects::{ItemCreateObject, ItemSetState, ObjectCreatePayload};
use pyreal_protocol::types::physicsdesc::{flags, PhysicsDesc};
use pyreal_protocol::types::PositionWire;
use pyreal_protocol::{Message as _, Opcode};
use pyreal_testkit::HeadlessClient;

/// The retail data files. An `expect`, never a skip: a scenario that quietly passes on a machine
/// with no data files is a test that passes by skipping.
fn store() -> Arc<RetailDatStore> {
    Arc::new(pyreal_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail data files are this scenario's oracle and there are none under {} -- set \
             PYREAL_TEST_DAT_DIR",
            pyreal_dat::testing::dat_dir().display()
        )
    }))
}

/// The human setup every attachment scenario here uses: its parts really do carry a
/// holding location, which is what makes a refusal of another one a result.
const PERSON_SETUP: u32 = 0x0200_0001;
/// A holding location that setup's own parts carry.
const RIGHT_HAND: u32 = 1;
/// One that no setup carries at all.
const NOT_A_PLACE: u32 = 4242;
/// The door setup whose parts carry a collision tree and no spheres at all, so it is intangible
/// unless the tree is read.
const DOOR_SETUP: u32 = 0x0200_024F;
/// The bit the client sets when an object's parts carry that collision tree.
const HAS_PHYSICS_BSP_PS: u32 = 0x0001_0000;
/// The bit that stops a body being seen at all.
const HIDDEN_PS: u32 = 0x0000_4000;

fn create_body(
    id: ObjectId,
    setup: Option<u32>,
    cell: Option<CellId>,
    parent: Option<(ObjectId, u32)>,
) -> Vec<u8> {
    let mut physicsdesc = PhysicsDesc::default();
    if let Some(s) = setup {
        physicsdesc.bitfield |= flags::SETUP;
        physicsdesc.setup_id = Some(s);
    }
    if let Some(c) = cell {
        physicsdesc.bitfield |= flags::POSITION;
        physicsdesc.position = Some(PositionWire {
            objcell_id: c.0,
            frame: pyreal_protocol::types::Frame::default(),
        });
    }
    if parent.is_some() {
        physicsdesc.bitfield |= flags::PARENT;
        physicsdesc.parent = parent;
    }
    pyreal_protocol::write_body(&ItemCreateObject(ObjectCreatePayload {
        id,
        objdesc: pyreal_protocol::types::ObjDesc::default(),
        physicsdesc,
        wdesc: pyreal_protocol::types::PublicWeenieDesc::default(),
    }))
    .expect("the encoder writes what the decoder reads")
}

fn feed(s: &mut ObjectStream, opcode: Opcode, body: Vec<u8>, now: f64) {
    s.apply_event(&SessionEvent::WorldObject { opcode, body }, LocalTime(now));
}

use pyreal_testkit::replay::recorded_endpoint as endpoint;
/// The recording, and the endpoint it replays into, both shared from `pyreal_testkit::replay`.
use pyreal_testkit::replay::records as recording;

// =============================================================================================
// objects.create.the-shards-state-word-wins-over-the-geometry-scan
// =============================================================================================

/// The descriptor's word is the word the body ends up with, in both directions.
pub fn the_descriptors_word_wins_over_the_scan() {
    use pyreal_client_runtime::object_physics::ObjectPhysics;
    use pyreal_client_runtime::objects::Presence;
    use pyreal_primitives::{DataId, Frame, Position};

    /// Spawn one object through the real synchronisation and report the state its body ends with,
    /// alongside what the client's own scan of the setup's parts thinks.
    fn spawn(store: &RetailDatStore, setup: u32, wire: u32) -> (u32, bool) {
        let id = ObjectId(0x8000_0007);
        let at = Position::new(CellId(0x0086_0100), Frame::default());
        let mut presences = BTreeMap::new();
        presences.insert(
            id,
            Presence {
                setup_id: Some(DataId(setup)),
                scale: 1.0,
                // The achieved pose and the last pose the wire named are two fields; the
                // synchronisation reads the second, so a fixture that filled only the first would
                // create no body at all.
                position: Some(at),
                server_position: Some(at),
                ..Presence::default()
            },
        );
        let mut game = pyreal_client_model::World::new();
        game.tables.physics.insert(
            id,
            pyreal_client_model::objects::PhysicsPresence {
                state: wire,
                ..Default::default()
            },
        );
        let land: Arc<dyn LandSource> =
            Arc::new(pyreal_physics::source::StaticLandSource::default());
        let mut world = PhysicsWorld::new(land);
        let mut phys = ObjectPhysics::new();
        phys.sync(store, &mut world, &presences, &game, None);
        let h = phys.handle(id).expect("the object got a body");
        let o = world.get(h).expect("live");
        (o.state().0, o.geometry.caches_physics_bsp())
    }

    let store = store();

    // A word the recordings really carry for this setup: a door, shut.
    let (with, scan_says) = spawn(&store, DOOR_SETUP, 0x0001_0018);
    // The premise: the client's own scan of this setup's parts says the bit belongs on, so the
    // arm below is a result and not an instrument that never sets anything.
    let the_scan_would_set_it = scan_says;
    let descriptor_kept = with & HAS_PHYSICS_BSP_PS != 0 && with == 0x0001_0018;

    // The same object with the bit taken out of the word. The body loses it, because a state
    // change assigns the whole word rather than merging it -- a build that re-ran the scan
    // afterwards would keep it.
    let (without, _) = spawn(&store, DOOR_SETUP, 0x0001_0018 & !HAS_PHYSICS_BSP_PS);
    let descriptor_wins_the_other_way = without == 0x0000_0018;

    // And the bits nothing reacts to are assigned just the same.
    let (whole, _) = spawn(&store, DOOR_SETUP, 0x0001_0418);
    let assigned_whole = whole == 0x0001_0418;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.create.the-shards-state-word-wins-over-the-geometry-scan",
        move |_| {
            the_scan_would_set_it
                && descriptor_kept
                && descriptor_wins_the_other_way
                && assigned_whole
        },
    );
}

pyreal_testkit::scenarios! {
    scenario_the_descriptors_word_wins_over_the_scan => the_descriptors_word_wins_over_the_scan ["objects.create.the-shards-state-word-wins-over-the-geometry-scan"],
    scenario_every_recorded_state_change_reaches_its_body => every_recorded_state_change_reaches_its_body ["object.set-state.every-recorded-change-reaches-the-body-it-names"],
    scenario_a_hidden_body_stops_being_a_target => a_hidden_body_stops_being_a_target ["object.set-state.a-body-the-shard-hides-stops-being-a-target-and-comes-back-in-place"],
    scenario_a_new_animation_table_alone_reaches_the_body => a_new_animation_table_alone_reaches_the_body ["objects.description.a-new-animation-table-alone-reaches-the-body"],
    scenario_a_holder_holds_only_where_its_body_has_a_place => a_holder_holds_only_where_its_body_has_a_place ["objects.parent.a-holder-holds-only-at-a-place-its-own-body-has"],
    scenario_a_held_object_has_no_body_of_its_own => a_held_object_has_no_body_of_its_own ["objects.held-object.has-no-body-and-leaves-the-cell-with-its-holder"],
    scenario_an_older_teleport_puts_the_position_stamp_back => an_older_teleport_puts_the_position_stamp_back ["objects.position.an-older-teleport-generation-puts-the-position-stamp-back"],
    scenario_an_unloaded_body_is_rescued_by_a_position => an_unloaded_body_is_rescued_by_a_position ["objects.unloaded-cell.a-body-is-rescued-by-the-next-position-the-shard-sends"],
    scenario_the_fellowship_done_marker_is_consumed_and_changes_nothing => the_fellowship_done_marker_is_consumed_and_changes_nothing ["shard-message.fellowship-update-done.is-consumed-and-changes-nothing-else"],
    scenario_none_of_the_three_reaches_no_receiver => none_of_the_three_reaches_no_receiver ["shard-message.the-three-the-shard-sends-on-every-login-all-reach-a-receiver"],
    scenario_an_out_of_range_holding_part_uses_the_holders_frame => an_out_of_range_holding_part_uses_the_holders_frame ["objects.held-frame.an-out-of-range-part-uses-the-holders-own-frame"],
    scenario_a_new_parent_event_attaches_the_child_in_wire_order => a_new_parent_event_attaches_the_child_in_wire_order ["objects.parent.a-new-parent-event-attaches-the-child-in-wire-order"],
    scenario_old_and_incomplete_parent_events_change_nothing => old_and_incomplete_parent_events_change_nothing ["objects.parent.an-old-or-incomplete-parent-event-changes-nothing"],
    scenario_a_position_report_releases_an_object_from_its_parent => a_position_report_releases_an_object_from_its_parent ["objects.position.a-position-report-releases-an-object-from-its-parent"],
    scenario_a_server_named_placement_draws_its_own_frames => a_server_named_placement_draws_its_own_frames ["objects.placement.a-server-named-placement-draws-its-own-frames"],
    scenario_a_pick_sweep_uses_the_placement_the_shard_named => a_pick_sweep_uses_the_placement_the_shard_named ["objects.pick.a-sweep-uses-the-placement-the-shard-named"],
    scenario_a_held_object_is_pickable_and_a_contained_object_is_not => a_held_object_is_pickable_and_a_contained_object_is_not ["objects.held-object.is-a-pick-candidate-while-a-contained-object-is-not"],
    scenario_where_the_picture_shows_no_object_the_sweep_answers => where_the_picture_shows_no_object_the_sweep_answers ["objects.pick.where-the-picture-shows-no-object-the-sweep-answers"],
}

// =============================================================================================
// object.set-state.every-recorded-change-reaches-the-body-it-names
// =============================================================================================

/// What one recording's state changes did to the bodies they name.
#[derive(Default)]
struct Verdict {
    delivered: usize,
    on_a_live_body: usize,
    on_the_player: usize,
    no_body: usize,
    wrong_word: usize,
    written_to_a_body: usize,
    applied: usize,
}

/// Every recorded state change reaches the body it names, with the word that was on the wire.
///
/// The per-frame synchronisation is the point: the client runs it once a frame, so it sees every
/// intermediate word. A replay that synchronised only at the end would see the last word each
/// object was given and would score a door that opened and shut as a door that only ever shut.
pub fn every_recorded_state_change_reaches_its_body() {
    let store = store();
    let mut total = Verdict::default();

    for (id, slug) in pyreal_client_net::client_session::testing::session_index() {
        let records = recording(id);
        let mut net = endpoint(&records);
        let mut objects = ObjectStream::new();
        let land: Arc<dyn LandSource> =
            Arc::new(pyreal_physics::source::StaticLandSource::default());
        let mut world = PhysicsWorld::new(land);
        let mut entered = false;
        let mut v = Verdict::default();
        for r in &records {
            let now = LocalTime(r.t);
            if !r.c2s {
                net.feed(&r.raw, r.peer(), now);
            }
            net.tick(now);
            let _ = net.take_outgoing();
            let mut arrived: Vec<ItemSetState> = Vec::new();
            // Snapshotted **before** the pump, because the pump is what applies the message: a
            // counter read afterwards would measure nothing and report a confident zero.
            let before = objects.stats.state_events;
            for e in objects.pump(&mut net, now) {
                match &e {
                    SessionEvent::CharacterSet(set) if !entered => {
                        if let Some(ch) = set.characters.first() {
                            let account = set.account.clone();
                            net.enter_world(ch.gid, &account);
                            entered = true;
                        }
                    }
                    SessionEvent::WorldObject { opcode, body }
                        if *opcode == Opcode::ITEM_SET_STATE =>
                    {
                        let mut rd = pyreal_protocol::Reader::body(body);
                        arrived.push(ItemSetState::read(&mut rd).expect("a state change decodes"));
                    }
                    _ => {}
                }
            }
            let applied_before = objects.physics.stats.state_applied;
            objects.sync_physics(&store, &mut world);
            let applied_here = objects.physics.stats.state_applied - applied_before;
            v.delivered += arrived.len();
            v.applied +=
                usize::try_from(objects.stats.state_events - before).expect("a small count");
            v.written_to_a_body += usize::try_from(applied_here).expect("a small count");
            for m in &arrived {
                // The word is read from its one home, which is the table both of the client's own
                // readers of it read.
                if objects.physics_state(m.id) != Some(m.state) {
                    continue;
                }
                if Some(m.id) == objects.player() {
                    v.on_the_player += 1;
                    continue;
                }
                // Three answers, not two: "it landed on a body", "the object is the player's own"
                // and "the object has no body" are different, and only the first is the claim.
                match objects.physics.handle(m.id).and_then(|h| world.get(h)) {
                    None => v.no_body += 1,
                    Some(o) if o.state().0 == m.state => v.on_a_live_body += 1,
                    Some(_) => v.wrong_word += 1,
                }
            }
        }
        if v.delivered > 0 {
            println!(
                "set-state: {slug} -- {} delivered, {} accepted, {} on a live body, {} on \
                 the player, {} with no body, {} with the wrong word, {} written by the physics \
                 side",
                v.delivered,
                v.applied,
                v.on_a_live_body,
                v.on_the_player,
                v.no_body,
                v.wrong_word,
                v.written_to_a_body
            );
        }
        total.delivered += v.delivered;
        total.applied += v.applied;
        total.on_a_live_body += v.on_a_live_body;
        total.on_the_player += v.on_the_player;
        total.no_body += v.no_body;
        total.wrong_word += v.wrong_word;
        total.written_to_a_body += v.written_to_a_body;
    }

    // Every message the gate accepted reached its body with the word that was on the wire: the
    // three answers account for all of them and none carries the wrong word.
    let nothing_carries_the_wrong_word = total.wrong_word == 0;
    let all_accounted_for =
        total.on_a_live_body + total.on_the_player + total.no_body == total.applied;
    // And the physics side counted the ones that really moved a word, which cannot be more than
    // the ones the gate accepted.
    let the_physics_side_agrees =
        total.written_to_a_body > 0 && total.written_to_a_body <= total.applied;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "object.set-state.every-recorded-change-reaches-the-body-it-names",
        move |_| {
            total.delivered > 0
                && total.on_a_live_body > 0
                && nothing_carries_the_wrong_word
                && all_accounted_for
                && the_physics_side_agrees
        },
    );
}

// =============================================================================================
// object.set-state.a-body-the-shard-hides-stops-being-a-target-and-comes-back-in-place
// =============================================================================================

/// A body the shard hides is not something a swing can reach, and it comes back in place.
pub fn a_hidden_body_stops_being_a_target() {
    use pyreal_physics::source::StaticLandSource;
    use pyreal_primitives::{Frame, LandblockId, Position, Quat, Vec3};

    /// A word the recordings carry for an ordinary visible object.
    const VISIBLE: u32 = 0x0040_0408;
    const HIDDEN: u32 = VISIBLE | HIDDEN_PS;
    const REMOTE: ObjectId = ObjectId(0x7000_0252);
    const ATTACKER: ObjectId = ObjectId(0x7000_0253);

    let store = store();
    let target_at = Vec3::new(90.0, 91.0, 20.0);
    let attacker_at = Vec3::new(90.0, 90.0, 20.0);
    let cell = LandblockId(0xA9B4).cell(1);

    let placed_create = |id: ObjectId, state: u32, event: u16| -> Vec<u8> {
        pyreal_protocol::write_body(&ItemCreateObject(ObjectCreatePayload {
            id,
            objdesc: pyreal_protocol::types::ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::SETUP | flags::POSITION | flags::OBJSCALE,
                state,
                setup_id: Some(PERSON_SETUP),
                object_scale: Some(1.0),
                position: Some(PositionWire {
                    objcell_id: cell.0,
                    frame: pyreal_protocol::types::Frame {
                        origin: target_at.into(),
                        orientation: Quat::IDENTITY.into(),
                    },
                }),
                timestamps: pyreal_protocol::types::PhysicsTimestamps {
                    state: event,
                    ..pyreal_protocol::types::PhysicsTimestamps::default()
                },
                ..PhysicsDesc::default()
            },
            wdesc: pyreal_protocol::types::PublicWeenieDesc::default(),
        }))
        .expect("encode")
    };
    let change = |id: ObjectId, state: u32, event: u16| -> Vec<u8> {
        pyreal_protocol::write_body(&ItemSetState {
            id,
            state,
            timestamps: pyreal_protocol::types::PhysicsEventStamp { instance: 0, event },
        })
        .expect("encode")
    };
    let cone = pyreal_physics::detect::AttackCone {
        part_index: -1,
        left: (-0.5, 0.866_025_4),
        right: (0.5, 0.866_025_4),
        radius: 2.0,
        height: 1.0,
    };

    // The body the shard hides after it was created.
    let mut land = StaticLandSource::linear();
    land.add_flat_block(LandblockId(0xA9B4), 10);
    let mut world = PhysicsWorld::new(Arc::new(land));
    let mut stream = ObjectStream::new();
    feed(
        &mut stream,
        Opcode::ITEM_CREATE_OBJECT,
        placed_create(REMOTE, VISIBLE, 0),
        0.0,
    );
    stream.sync_physics(&store, &mut world);
    let remote = stream
        .physics
        .handle(REMOTE)
        .expect("the create made a body");
    let standing = world.get(remote).expect("live").position;
    let attacker_geometry = Arc::clone(&world.get(remote).expect("live").geometry);
    let attacker = world.create(ATTACKER, attacker_geometry, true);
    world.force_into_cell(
        attacker,
        &Position::new(cell, Frame::new(attacker_at, Quat::IDENTITY)),
    );
    let reaches = |w: &mut PhysicsWorld| w.attack(attacker, &cone).iter().any(|p| p.id == REMOTE);
    let starts_reachable = reaches(&mut world);

    feed(
        &mut stream,
        Opcode::ITEM_SET_STATE,
        change(REMOTE, HIDDEN, 1),
        1.0,
    );
    stream.sync_physics(&store, &mut world);
    let hidden_in_place = stream.physics.handle(REMOTE) == Some(remote)
        && world.get(remote).expect("still live").position == standing
        && world
            .get(remote)
            .expect("still live")
            .state()
            .ignores_collisions()
        && !world
            .get(remote)
            .expect("still live")
            .state()
            .reports_collisions();
    let hidden_is_out_of_reach = !reaches(&mut world);

    // An unchanged word is no change at all.
    let applied = stream.physics.stats.state_applied;
    feed(
        &mut stream,
        Opcode::ITEM_SET_STATE,
        change(REMOTE, HIDDEN, 2),
        2.0,
    );
    stream.sync_physics(&store, &mut world);
    let no_edge = stream.physics.stats.state_applied == applied;

    feed(
        &mut stream,
        Opcode::ITEM_SET_STATE,
        change(REMOTE, VISIBLE, 3),
        3.0,
    );
    stream.sync_physics(&store, &mut world);
    let back_in_place = stream.physics.handle(REMOTE) == Some(remote)
        && world.get(remote).expect("still live").position == standing
        && !world
            .get(remote)
            .expect("still live")
            .state()
            .ignores_collisions()
        && world
            .get(remote)
            .expect("still live")
            .state()
            .reports_collisions();
    let reachable_again = reaches(&mut world);

    // And a body that was hidden from the moment it was created is out of reach too, then comes
    // back without moving: an initially set bit is a real change and not constructor state.
    let mut land = StaticLandSource::linear();
    land.add_flat_block(LandblockId(0xA9B4), 10);
    let mut world2 = PhysicsWorld::new(Arc::new(land));
    let mut stream2 = ObjectStream::new();
    feed(
        &mut stream2,
        Opcode::ITEM_CREATE_OBJECT,
        placed_create(REMOTE, HIDDEN, 0),
        0.0,
    );
    stream2.sync_physics(&store, &mut world2);
    let remote2 = stream2
        .physics
        .handle(REMOTE)
        .expect("the create made a body");
    let where_it_stands = world2.get(remote2).expect("live").position;
    let geometry2 = Arc::clone(&world2.get(remote2).expect("live").geometry);
    let attacker2 = world2.create(ATTACKER, geometry2, true);
    world2.force_into_cell(
        attacker2,
        &Position::new(cell, Frame::new(attacker_at, Quat::IDENTITY)),
    );
    let reaches2 = |w: &mut PhysicsWorld| w.attack(attacker2, &cone).iter().any(|p| p.id == REMOTE);
    let born_hidden = !reaches2(&mut world2)
        && world2
            .get(remote2)
            .expect("live")
            .state()
            .ignores_collisions();
    feed(
        &mut stream2,
        Opcode::ITEM_SET_STATE,
        change(REMOTE, VISIBLE, 1),
        1.0,
    );
    stream2.sync_physics(&store, &mut world2);
    let unhidden_in_place = stream2.physics.handle(REMOTE) == Some(remote2)
        && world2.get(remote2).expect("live").position == where_it_stands
        && reaches2(&mut world2);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "object.set-state.a-body-the-shard-hides-stops-being-a-target-and-comes-back-in-place",
        move |_| {
            starts_reachable
                && hidden_in_place
                && hidden_is_out_of_reach
                && no_edge
                && back_in_place
                && reachable_again
                && born_hidden
                && unhidden_in_place
        },
    );
}

// =============================================================================================
// objects.description.a-new-animation-table-alone-reaches-the-body
// =============================================================================================

/// A description that moves only the animation table reaches the body and rebuilds nothing else.
pub fn a_new_animation_table_alone_reaches_the_body() {
    use pyreal_client_runtime::character::Character;
    use pyreal_dat::DbType;
    use pyreal_primitives::DataId;
    use {
        pyreal_world_data::landblock::load_region, pyreal_world_data::landblock::DEFAULT_LANDBLOCK,
    };

    /// The middle of the body's own block, where every movement scenario stands one up.
    const MID: f32 = pyreal_physics::globals::BLOCK_LENGTH * 0.5;

    let store = store();
    let region = load_region(&store).expect("the region decodes");
    let body = |store: &Arc<RetailDatStore>| {
        Character::new(store, &region, DEFAULT_LANDBLOCK, (MID, MID)).expect("the body stands up")
    };
    // The table's own default style, which is the table's row and not anything this scenario
    // chose: an animation-level observable rather than the id this scenario wrote.
    let style = |c: &Character| c.driver().motion_table.table().map(|t| t.default_style());

    let mut c0 = body(&store);
    let own_setup = c0.setup_id();
    let own_table = c0.motion_table_id();
    // The premise: the body has a table before anything changes, or "the table changed" is
    // satisfied by a body that never had one.
    let before = style(&c0).expect("the body is created with an animation table installed");
    let parts_before = c0.driver().part_array.parts.len();
    let has_parts = parts_before > 0;

    // A table the data files hold whose default style differs from this body's. Found by asking
    // the data files, so the assertion below can discriminate: a second table that answered the
    // same style would make it pass for a body that ignored the change.
    let mut other: Option<(DataId, _)> = None;
    for id in store.ids_of(DbType::MTable) {
        if id == own_table {
            continue;
        }
        let mut probe = body(&store);
        if probe.set_setup_id(own_setup, Some(id)).is_ok() {
            if let Some(after) = style(&probe).filter(|after| *after != before) {
                other = Some((id, after));
                break;
            }
        }
    }
    let (other_id, other_style) =
        other.expect("no animation table in the data files answers a different default style");

    // The change under test: the same setup the body already carries, a different table.
    let rebuilt = c0
        .set_setup_id(own_setup, Some(other_id))
        .expect("the table loads");
    let the_table_moved = c0.motion_table_id() == other_id
        && style(&c0) == Some(other_style)
        && c0.stats.motion_table_changes == 1;
    // …and the parts are untouched, which is what a body in the middle of an animation needs.
    let the_parts_did_not = !rebuilt
        && c0.setup_id() == own_setup
        && c0.stats.setup_changes == 0
        && c0.driver().part_array.parts.len() == parts_before;

    // Offering the same pair again does nothing at all -- rebuilding the manager would restart
    // the idle animation in the middle of its life.
    let mut c1 = body(&store);
    let idempotent = !c1.set_setup_id(own_setup, Some(own_table)).expect("the body's own pair")
        && c1.stats.motion_table_changes == 0
        && c1.stats.setup_changes == 0
        // The wire's "the description names none" is not a table either, and nor is no field.
        && !c1.set_setup_id(own_setup, Some(DataId(0))).expect("no table named")
        && !c1.set_setup_id(own_setup, None).expect("no table named")
        && c1.stats.motion_table_changes == 0
        && c1.motion_table_id() == own_table;
    let _ = c1
        .set_setup_id(own_setup, Some(other_id))
        .expect("the table loads");
    let the_new_arm_is_idempotent_too = !c1
        .set_setup_id(own_setup, Some(other_id))
        .expect("the table loads")
        && c1.stats.motion_table_changes == 1;

    // A table the data files do not hold is refused without disturbing the body.
    let mut c2 = body(&store);
    let missing = DataId(0x0900_FFFF);
    let really_missing = store.read_typed(DbType::MTable, missing).is_err();
    let was = style(&c2);
    let err = c2
        .set_setup_id(own_setup, Some(missing))
        .expect_err("a missing table must not build");
    let refused_cleanly = c2.motion_table_id() == own_table
        && style(&c2) == was
        && c2.stats.motion_table_changes == 0
        && matches!(err, pyreal_client_runtime::character::CharacterError::NoMotionTable(id) if id == missing);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.description.a-new-animation-table-alone-reaches-the-body",
        move |_| {
            has_parts
                && the_table_moved
                && the_parts_did_not
                && idempotent
                && the_new_arm_is_idempotent_too
                && really_missing
                && refused_cleanly
        },
    );
}

// =============================================================================================
// objects.parent.a-holder-holds-only-where-its-body-has-a-place
// =============================================================================================

/// The holding place is the gate, and it is the holder's own parts that answer.
pub fn a_holder_holds_only_where_its_body_has_a_place() {
    const HOLDER: ObjectId = ObjectId(0x7011_0001);
    const CHILD: ObjectId = ObjectId(0x7011_0002);

    let store = store();
    let attaches_at = |location: u32| -> bool {
        let mut s = ObjectStream::with_store(Arc::clone(&store));
        feed(
            &mut s,
            Opcode::ITEM_CREATE_OBJECT,
            create_body(HOLDER, Some(PERSON_SETUP), None, None),
            0.0,
        );
        feed(
            &mut s,
            Opcode::ITEM_CREATE_OBJECT,
            create_body(CHILD, Some(PERSON_SETUP), None, Some((HOLDER, location))),
            0.0,
        );
        assert!(
            s.presence(CHILD).is_some(),
            "the premise: the object itself still arrives"
        );
        s.world.physics_parent(CHILD) == Some((HOLDER, location))
    };
    // Three links against the same pair, so the only thing that varies is the place asked for.
    let a_real_place = attaches_at(RIGHT_HAND);
    let not_a_place = !attaches_at(NOT_A_PLACE);
    // The place a check that only ever asked for one could not tell from no gate at all.
    let a_hole_in_the_table = !attaches_at(7);

    // With no data files open the client cannot answer the question at all, so it refuses every
    // attachment -- the identical wire bytes, and no link.
    let mut bare = ObjectStream::new();
    feed(
        &mut bare,
        Opcode::ITEM_CREATE_OBJECT,
        create_body(HOLDER, Some(PERSON_SETUP), None, None),
        0.0,
    );
    feed(
        &mut bare,
        Opcode::ITEM_CREATE_OBJECT,
        create_body(CHILD, Some(PERSON_SETUP), None, Some((HOLDER, RIGHT_HAND))),
        0.0,
    );
    let assetless_refuses =
        bare.presence(CHILD).is_some() && bare.world.physics_parent(CHILD).is_none();

    // And a holder that really does hold a child keeps that child's placeholder alive past the
    // deadline it would otherwise have died at, while a refused attachment leaves the timer
    // running.
    let lifetime = |location: u32| -> (Option<ServerTime>, bool) {
        let mut s = ObjectStream::with_store(Arc::clone(&store));
        let mut holder = ObjectCreatePayload {
            id: HOLDER,
            objdesc: pyreal_protocol::types::ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::SETUP | flags::CHILDREN,
                setup_id: Some(PERSON_SETUP),
                children: Some(vec![pyreal_protocol::types::physicsdesc::ChildLink {
                    child_id: CHILD,
                    location_id: location,
                }]),
                ..PhysicsDesc::default()
            },
            wdesc: pyreal_protocol::types::PublicWeenieDesc::default(),
        };
        holder.physicsdesc.timestamps.instance = 1;
        feed(
            &mut s,
            Opcode::ITEM_CREATE_OBJECT,
            pyreal_protocol::write_body(&ItemCreateObject(holder)).expect("encode"),
            1000.0,
        );
        assert!(
            s.world.tables.null_physics.contains_key(CHILD),
            "the child is a placeholder"
        );
        let deadline = s.world.tables.doomed.get(CHILD).copied();
        s.use_time::<pyreal_client_net::client_session::testing::MockTransport>(
            ServerTime(1_025.001),
            None,
        );
        (deadline, s.world.tables.null_physics.contains_key(CHILD))
    };
    let (accepted_deadline, accepted_survives) = lifetime(RIGHT_HAND);
    let (refused_deadline, refused_survives) = lifetime(NOT_A_PLACE);
    let an_accepted_child_outlives_it = accepted_deadline.is_none() && accepted_survives;
    let a_refused_one_does_not = refused_deadline == Some(ServerTime(1_025.0)) && !refused_survives;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.parent.a-holder-holds-only-at-a-place-its-own-body-has",
        move |_| {
            a_real_place
                && not_a_place
                && a_hole_in_the_table
                && assetless_refuses
                && an_accepted_child_outlives_it
                && a_refused_one_does_not
        },
    );
}

// =============================================================================================
// objects.held-object.has-no-body-and-leaves-the-cell-with-its-holder
// =============================================================================================

/// A held object has no body of its own, and it leaves the room with whatever is holding it.
pub fn a_held_object_has_no_body_of_its_own() {
    use pyreal_primitives::{LandblockId, Vec3};
    use pyreal_world_data::land_source::DatLandSource;

    const HOLDER: ObjectId = ObjectId(0x7011_0001);
    const CHILD: ObjectId = ObjectId(0x7011_0002);

    let store = store();

    // 1. The room the holder is released from takes the child with it: the child is not released
    //    in its own right, and nothing on that path detaches it.
    let cell = CellId(0xA9B4_0100);
    let mut s = ObjectStream::with_store(Arc::clone(&store));
    feed(
        &mut s,
        Opcode::ITEM_CREATE_OBJECT,
        create_body(HOLDER, Some(PERSON_SETUP), Some(cell), None),
        0.0,
    );
    s.world.publish_physics_cell(HOLDER, Some(cell));
    feed(
        &mut s,
        Opcode::ITEM_CREATE_OBJECT,
        create_body(
            CHILD,
            Some(PERSON_SETUP),
            Some(cell),
            Some((HOLDER, RIGHT_HAND)),
        ),
        0.0,
    );
    let premise = s.world.physics(CHILD).and_then(|p| p.cell) == Some(cell)
        && s.world.physics_parent(CHILD) == Some((HOLDER, RIGHT_HAND));
    let left = s.world.release_obj_cell(cell, ServerTime(10.0));
    let only_the_holder_is_released = left == vec![HOLDER];
    let the_child_went_with_it = s.world.physics(CHILD).and_then(|p| p.cell).is_none()
        && s.world.tables.doomed.contains_key(CHILD)
        && s.world.physics_parent(CHILD) == Some((HOLDER, RIGHT_HAND));

    // 2. And it has no body at all: the synchronisation skips a held object, and destroys the body
    //    of one that becomes held. Measured from the arena rather than asserted from the source.
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region decodes");
    let land =
        Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the land source opens"));
    let block = LandblockId(0x7F03);
    let room = CellId(0x7F03_0100);
    land.load_block_cells(block);
    let geometry = land.env_cell(room).expect("the installed academy interior");
    let mut low = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut high = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for v in geometry.physics_polygons.iter().flat_map(|p| &p.vertices) {
        low = Vec3::new(low.x.min(v.x), low.y.min(v.y), low.z.min(v.z));
        high = Vec3::new(high.x.max(v.x), high.y.max(v.y), high.z.max(v.z));
    }
    let centre = Vec3::new((low.x + high.x) * 0.5, (low.y + high.y) * 0.5, low.z + 1.0);
    let origin = pyreal_physics::math::localtoglobal(&geometry.frame, centre);
    let mut physics = PhysicsWorld::new(Arc::clone(&land) as Arc<dyn LandSource>);
    let mut s = ObjectStream::with_store(Arc::clone(&store));
    let holder = ObjectCreatePayload {
        id: HOLDER,
        objdesc: pyreal_protocol::types::ObjDesc::default(),
        physicsdesc: PhysicsDesc {
            bitfield: flags::SETUP | flags::POSITION,
            setup_id: Some(PERSON_SETUP),
            position: Some(PositionWire {
                objcell_id: room.0,
                frame: pyreal_protocol::types::Frame {
                    origin: origin.into(),
                    orientation: geometry.frame.rotation.into(),
                },
            }),
            ..PhysicsDesc::default()
        },
        wdesc: pyreal_protocol::types::PublicWeenieDesc::default(),
    };
    feed(
        &mut s,
        Opcode::ITEM_CREATE_OBJECT,
        pyreal_protocol::write_body(&ItemCreateObject(holder)).expect("encode"),
        1000.0,
    );
    s.sync_physics_at(&store, &mut physics, LocalTime(1000.0));
    // The premise: an unheld object with a position does get a body, so "no body" below is a
    // result and not the instrument failing.
    let the_holder_has_one =
        s.physics.handle(HOLDER).is_some() && physics.by_object_id(HOLDER).is_some();
    feed(
        &mut s,
        Opcode::ITEM_CREATE_OBJECT,
        create_body(CHILD, Some(PERSON_SETUP), None, Some((HOLDER, RIGHT_HAND))),
        1000.0,
    );
    let attached = s.world.physics_parent(CHILD) == Some((HOLDER, RIGHT_HAND));
    s.sync_physics_at(&store, &mut physics, LocalTime(1001.0));
    let the_child_has_none =
        s.physics.handle(CHILD).is_none() && physics.by_object_id(CHILD).is_none();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.held-object.has-no-body-and-leaves-the-cell-with-its-holder",
        move |_| {
            premise
                && only_the_holder_is_released
                && the_child_went_with_it
                && the_holder_has_one
                && attached
                && the_child_has_none
        },
    );
}

// =============================================================================================
// objects.position.an-older-teleport-generation-puts-the-position-stamp-back
// =============================================================================================

/// A position report from an older teleport generation is undone completely.
#[allow(clippy::too_many_lines)]
pub fn an_older_teleport_puts_the_position_stamp_back() {
    use pyreal_client_runtime::app::player_timestamps;
    use pyreal_client_runtime::objects::PlayerMotionDispatch;
    use pyreal_primitives::{Frame, Position};
    use pyreal_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};
    use pyreal_protocol::types::{Origin, PhysicsTimestamps};

    const ID: ObjectId = ObjectId(0x7000_0021);
    const HOLDER: ObjectId = ObjectId(0x7000_0022);
    const CELL: u32 = 0x7F03_01B0;

    let store = store();
    // The attachment really is validated against the data files, which is why this needs them.
    let seed = |player: bool, position: u16, teleport: u16, parent: bool| -> ObjectStream {
        let mut objects = ObjectStream::with_store(Arc::clone(&store));
        if parent {
            feed(
                &mut objects,
                Opcode::ITEM_CREATE_OBJECT,
                create_body(HOLDER, Some(PERSON_SETUP), None, None),
                0.0,
            );
        }
        if player {
            objects.apply_event(&SessionEvent::PlayerCreated(ID), LocalTime(0.0));
        }
        let mut desc = PhysicsDesc {
            bitfield: flags::POSITION | flags::ANIMFRAME | flags::SETUP,
            setup_id: Some(PERSON_SETUP),
            position: Some(PositionWire {
                objcell_id: CELL,
                frame: pyreal_protocol::types::Frame {
                    origin: pyreal_protocol::types::Vec3 {
                        x: 1.0,
                        y: 2.0,
                        z: 3.0,
                    },
                    ..Default::default()
                },
            }),
            animframe_id: Some(52),
            timestamps: PhysicsTimestamps {
                position,
                teleport,
                force_position: 20,
                server_controlled_move: 7,
                instance: 3,
                ..PhysicsTimestamps::default()
            },
            ..PhysicsDesc::default()
        };
        if parent {
            desc.bitfield |= flags::PARENT;
            desc.parent = Some((HOLDER, RIGHT_HAND));
        }
        feed(
            &mut objects,
            Opcode::ITEM_CREATE_OBJECT,
            pyreal_protocol::write_body(&ItemCreateObject(ObjectCreatePayload {
                id: ID,
                objdesc: pyreal_protocol::types::ObjDesc::default(),
                physicsdesc: desc,
                wdesc: pyreal_protocol::types::PublicWeenieDesc::default(),
            }))
            .expect("encode"),
            0.0,
        );
        assert!(
            objects.take_player_motion_dispatches().is_empty(),
            "a create that carries no motion is no teleport completion"
        );
        objects
    };
    let incoming = |position: u16, teleport: u16, force: u16, x: f32| MovementPositionEvent {
        id: ID,
        position: PositionPack {
            flags: position_flags::HAS_PLACEMENT_ID,
            origin: Origin {
                objcell_id: CELL,
                origin: pyreal_protocol::types::Vec3 { x, y: 4.0, z: 5.0 },
            },
            placement_id: Some(3),
            instance_timestamp: 3,
            position_timestamp: position,
            teleport_timestamp: teleport,
            force_position_timestamp: force,
            ..Default::default()
        },
    };
    let apply = |objects: &mut ObjectStream, m: &MovementPositionEvent| {
        feed(
            objects,
            Opcode::MOVEMENT_POSITION_EVENT,
            pyreal_protocol::write_body(m).expect("encode"),
            1.0,
        );
    };
    let destination = |m: &MovementPositionEvent| {
        Position::new(
            CellId(CELL),
            Frame::new(
                m.position.origin.origin.into(),
                m.position.orientation.into(),
            ),
        )
    };

    // 1. The rollback is complete, and it happens before anything else the message would have
    //    done -- the position, what is holding the object, and how it is posed.
    let mut rollback = true;
    for player in [false, true] {
        for parent in [false, true] {
            let mut objects = seed(player, 100, 10, parent);
            let before = objects.presence(ID).expect("created").clone();
            apply(&mut objects, &incoming(101, 9, 20, 99.0));
            let after = objects.presence(ID).expect("still here");
            rollback &= after.position_ts == 100
                && after.position == before.position
                && after.parent == before.parent
                && after.placement == 52
                && after.pending_placement.is_none()
                && player_timestamps(Some(after)) == player_timestamps(Some(&before))
                && objects.stats.position_updates == 0
                && objects.stats.position_teleport_rollbacks == 1
                && objects.stats.stale_positions == 0
                && objects.stats.teleport_stamps == 0
                && objects.take_player_motion_dispatches().is_empty();
            // The very stamp that was refused is usable again: merely keeping the position while
            // consuming the stamp would reject this one.
            let ordinary = incoming(101, 10, 20, 6.0);
            apply(&mut objects, &ordinary);
            let p = objects.presence(ID).expect("still here");
            rollback &= p.position_ts == 101
                && p.position == Some(destination(&ordinary))
                && p.parent.is_none()
                && p.pending_placement == Some(3)
                && p.teleport_ts == 10
                && objects.stats.position_updates == 1
                && objects.take_player_motion_dispatches().is_empty();
        }
    }

    // 2. A teleport is completed exactly once, and a newer teleport cannot carry a stale position
    //    past the gate.
    let mut objects = seed(true, 100, 10, false);
    let standing = objects.presence(ID).expect("created").position;
    let mut exactly_once = true;
    for pos in [100, 99] {
        apply(&mut objects, &incoming(pos, 11, 20, 99.0));
        let p = objects.presence(ID).expect("still here");
        exactly_once &= p.position == standing
            && p.teleport_ts == 10
            && objects.take_player_motion_dispatches().is_empty();
    }
    let first = incoming(101, 11, 20, 6.0);
    apply(&mut objects, &first);
    apply(&mut objects, &first);
    apply(&mut objects, &incoming(102, 10, 20, 99.0));
    exactly_once &= objects.presence(ID).expect("here").position == Some(destination(&first))
        && objects.presence(ID).expect("here").position_ts == 101;
    let second = incoming(102, 12, 20, 7.0);
    apply(&mut objects, &second);
    let ordinary = incoming(103, 12, 20, 8.0);
    apply(&mut objects, &ordinary);
    exactly_once &= objects.presence(ID).expect("here").position == Some(destination(&ordinary))
        && objects.presence(ID).expect("here").position_ts == 103
        && objects.stats.position_updates == 3
        && objects.stats.position_teleport_rollbacks == 1
        && objects.stats.stale_positions == 3
        && objects.stats.teleport_stamps == 2
        && objects.stats.player_teleports == 2;
    let journal = objects.take_player_motion_dispatches();
    exactly_once &= journal.len() == 2;
    for (entry, source) in journal.iter().zip([first, second]) {
        let PlayerMotionDispatch::Teleport {
            position,
            timestamps,
        } = entry
        else {
            panic!("a teleport completion is the only thing this drive produces")
        };
        // The completion carries the position the teleport named, not the last one the body
        // happened to reach.
        exactly_once &= *position == destination(&source)
            && timestamps.teleport == source.position.teleport_timestamp
            && (
                timestamps.instance,
                timestamps.server_control,
                timestamps.force_position,
            ) == (3, 7, 20);
    }
    exactly_once &= objects.take_player_motion_dispatches().is_empty()
        && objects.take_player_teleport().is_none();

    // 3. The forced-position generation belongs to the player's own body and moves before either
    //    rejection path, and a stale one does not block an otherwise acceptable report.
    let mut force_is_the_players = true;
    for player in [false, true] {
        let mut objects = seed(player, 100, 10, false);
        let standing = objects.presence(ID).expect("created").position;
        for (position, force) in [(100, 21), (101, 22)] {
            apply(&mut objects, &incoming(position, 9, force, 99.0));
            let p = objects.presence(ID).expect("still here");
            force_is_the_players &= p.position_ts == 100
                && p.position == standing
                && p.teleport_ts == 10
                && p.force_position_ts == if player { force } else { 20 }
                && player_timestamps(Some(p)).force_position == if player { force } else { 20 }
                && objects.take_player_motion_dispatches().is_empty();
        }
        force_is_the_players &= objects.stats.force_position_stamps == u64::from(player) * 2
            && objects.stats.position_teleport_rollbacks == 1
            && objects.stats.stale_positions == 1;
        let ordinary = incoming(101, 10, 19, 6.0);
        apply(&mut objects, &ordinary);
        force_is_the_players &= objects.presence(ID).expect("here").position
            == Some(destination(&ordinary))
            && objects.presence(ID).expect("here").force_position_ts
                == if player { 22 } else { 20 };
    }

    // 4. The comparison treats a difference of half the stamp range or more as a wrap.
    let mut wraps = true;
    for (stored, arriving, accepted, completion) in [
        (0u16, 0xFFFFu16, false, false),
        (0xFFFF, 0, true, true),
        (0, 0x8000, false, false),
        (0x8000, 0, true, true),
        (5, 5, true, false),
    ] {
        let mut objects = seed(true, 0xFFFF, stored, false);
        let standing = objects.presence(ID).expect("created").position;
        let next = incoming(0, arriving, 20, 6.0);
        apply(&mut objects, &next);
        let p = objects.presence(ID).expect("still here");
        wraps &= p.position_ts == if accepted { 0 } else { 0xFFFF }
            && p.position
                == if accepted {
                    Some(destination(&next))
                } else {
                    standing
                }
            && p.teleport_ts == if completion { arriving } else { stored }
            && objects.take_player_motion_dispatches().len() == usize::from(completion)
            && objects.take_player_motion_dispatches().is_empty();
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.position.an-older-teleport-generation-puts-the-position-stamp-back",
        move |_| rollback && exactly_once && force_is_the_players && wraps,
    );
}

// =============================================================================================
// objects.unloaded-cell.a-body-is-rescued-by-the-next-position-the-shard-sends
// =============================================================================================

/// A body whose room the client unloaded is rescued by the next position the shard sends for it.
#[allow(clippy::too_many_lines)]
pub fn an_unloaded_body_is_rescued_by_a_position() {
    use pyreal_client_net::client_session::testing::MockTransport;
    use pyreal_primitives::{Frame, Position, Vec3};
    use pyreal_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};
    use pyreal_protocol::objects::ItemParentEvent;
    use pyreal_world_data::land_source::DatLandSource;

    const PARENT: ObjectId = ObjectId(0x7000_6011);
    const CHILD: ObjectId = ObjectId(0x8000_6011);
    const OTHER: ObjectId = ObjectId(0x7000_6012);
    /// The flag the shard sets when a report says the body is on the ground; a report without it
    /// moves nothing, by the client's own rule.
    const GROUNDED: u32 = position_flags::IS_GROUNDED;

    let store = store();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region decodes");
    let land =
        Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the land source opens"));
    // Two real interiors, so "into another room that is loaded" is a real room.
    let mut places = Vec::new();
    for cell in [CellId(0x7F03_0100), CellId(0x8602_0100)] {
        land.load_block_cells(cell.landblock());
        let geom = land.env_cell(cell).expect("the installed academy interior");
        let mut low = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut high = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for v in geom.physics_polygons.iter().flat_map(|p| &p.vertices) {
            low = Vec3::new(low.x.min(v.x), low.y.min(v.y), low.z.min(v.z));
            high = Vec3::new(high.x.max(v.x), high.y.max(v.y), high.z.max(v.z));
        }
        assert!(
            high.x > low.x && high.y > low.y && high.z > low.z,
            "the room has an extent"
        );
        places.push(Position::new(
            cell,
            Frame::new(
                pyreal_physics::math::localtoglobal(
                    &geom.frame,
                    Vec3::new((low.x + high.x) * 0.5, (low.y + high.y) * 0.5, low.z + 1.0),
                ),
                geom.frame.rotation,
            ),
        ));
    }
    let (old, fresh) = (places[0], places[1]);

    let placed = |id: ObjectId, pos: Option<Position>, children: bool| -> Vec<u8> {
        let mut desc = PhysicsDesc {
            bitfield: flags::SETUP,
            setup_id: Some(PERSON_SETUP),
            ..PhysicsDesc::default()
        };
        if let Some(p) = pos {
            desc.bitfield |= flags::POSITION;
            desc.position = Some(PositionWire {
                objcell_id: p.cell.0,
                frame: pyreal_protocol::types::Frame {
                    origin: p.frame.origin.into(),
                    orientation: p.frame.rotation.into(),
                },
            });
        }
        if children {
            desc.bitfield |= flags::CHILDREN;
            desc.children = Some(vec![pyreal_protocol::types::physicsdesc::ChildLink {
                child_id: CHILD,
                location_id: RIGHT_HAND,
            }]);
        }
        pyreal_protocol::write_body(&ItemCreateObject(ObjectCreatePayload {
            id,
            objdesc: pyreal_protocol::types::ObjDesc::default(),
            physicsdesc: desc,
            wdesc: pyreal_protocol::types::PublicWeenieDesc::default(),
        }))
        .expect("encode")
    };
    let report = |to: Position, stamp: u16, teleport: u16, flags_word: u32| -> Vec<u8> {
        let event = MovementPositionEvent {
            id: PARENT,
            position: PositionPack {
                flags: flags_word,
                origin: pyreal_protocol::types::Origin {
                    objcell_id: to.cell.0,
                    origin: to.frame.origin.into(),
                },
                orientation: to.frame.rotation.into(),
                position_timestamp: stamp,
                teleport_timestamp: teleport,
                ..Default::default()
            },
        };
        pyreal_protocol::write_body(&event).expect("encode")
    };

    /// A body standing in `old`, holding a child, whose room has then been unloaded.
    struct Station {
        physical: PhysicsWorld,
        stream: ObjectStream,
        handle: pyreal_physics::PhysHandle,
    }

    let build = || -> Station {
        // Both rooms are loaded again at the start of each run: this scenario's whole subject is
        // a room being unloaded, and the land source is shared between its three runs.
        land.load_block_cells(old.cell.landblock());
        land.load_block_cells(fresh.cell.landblock());
        let mut physical = PhysicsWorld::new(Arc::clone(&land) as Arc<dyn LandSource>);
        let mut stream = ObjectStream::with_store(Arc::clone(&store));
        feed(
            &mut stream,
            Opcode::ITEM_CREATE_OBJECT,
            placed(CHILD, None, false),
            1000.0,
        );
        // A second holder, so the attachment that is refused below is refused for the place it
        // names and not because the client has never heard of the thing doing the holding.
        feed(
            &mut stream,
            Opcode::ITEM_CREATE_OBJECT,
            placed(OTHER, None, false),
            1000.0,
        );
        feed(
            &mut stream,
            Opcode::ITEM_CREATE_OBJECT,
            placed(PARENT, Some(old), true),
            1000.0,
        );
        stream.sync_physics_at(&store, &mut physical, LocalTime(1000.0));
        let handle = stream
            .physics
            .handle(PARENT)
            .expect("the create made a body");
        assert_eq!(physical.get(handle).expect("live").cell, Some(old.cell));
        assert_eq!(
            stream.world.physics(CHILD).expect("held").cell,
            Some(old.cell)
        );
        assert_eq!(
            stream.presence(CHILD).expect("held").parent,
            Some((PARENT, RIGHT_HAND))
        );
        // …and then the room goes.
        assert_eq!(
            stream.release_block_obj_cells_with_physics(
                old.cell.landblock(),
                LocalTime(1001.0),
                &mut physical
            ),
            1
        );
        assert!(land.release_visible_cells(old.cell.landblock()).0 > 0);
        stream.sync_physics_at(&store, &mut physical, LocalTime(1001.1));
        assert_eq!(physical.get(handle).expect("live").cell, None);
        for id in [PARENT, CHILD] {
            assert_eq!(
                stream.world.tables.doomed.get(id),
                Some(&ServerTime(1026.0))
            );
        }
        Station {
            physical,
            stream,
            handle,
        }
    };

    // 1. A fresh grounded report into the room that is still loaded rescues it, and the room it
    //    left is not reloaded to do it.
    let mut s = build();
    feed(
        &mut s.stream,
        Opcode::MOVEMENT_POSITION_EVENT,
        report(fresh, 1, 0, GROUNDED),
        1002.0,
    );
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1002.0));
    let entered = s.physical.get(s.handle).expect("live").cell == Some(fresh.cell)
        && s.stream.world.tables.doomed.get(PARENT).is_none()
        && s.stream.world.tables.doomed.get(CHILD).is_none()
        && s.stream
            .world
            .tables
            .lost_cells
            .iter()
            .all(|(_, lost)| !lost.objects.contains(&PARENT))
        && land.env_cell(old.cell).is_none()
        && s.stream.world.physics(CHILD).and_then(|p| p.cell) == Some(fresh.cell)
        && s.stream
            .world
            .weenie(CHILD)
            .is_some_and(|w| w.phys_has_cell);
    // …and it survives past the deadline it was on, without depending on the periodic refresh.
    s.stream.world.tables.visible.clear();
    s.stream
        .use_time::<MockTransport>(ServerTime(1_026.001), None);
    let survived =
        s.stream.world.physics(PARENT).is_some() && s.stream.world.physics(CHILD).is_some();

    // 2. A report that is not grounded moves no body, although the shard's word is still taken.
    let mut s = build();
    feed(
        &mut s.stream,
        Opcode::MOVEMENT_POSITION_EVENT,
        report(fresh, 1, 0, GROUNDED),
        1002.0,
    );
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1002.0));
    let standing = s.physical.get(s.handle).expect("live").position;
    let mut nudged = fresh;
    nudged.frame.origin.x += 0.125;
    let arms = |s: &Station| {
        let st = &s.stream.physics.stats;
        (
            st.teleport_arm,
            st.no_contact_arm,
            st.interpolate_arm,
            st.snap_arm,
        )
    };
    let before = arms(&s);
    feed(
        &mut s.stream,
        Opcode::MOVEMENT_POSITION_EVENT,
        report(nudged, 2, 0, 0),
        1003.0,
    );
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1003.0));
    let ungrounded_moves_nothing = arms(&s) == (before.0, before.1 + 1, before.2, before.3)
        && s.physical.get(s.handle).expect("live").position == standing
        && s.stream.presence(PARENT).expect("here").position_ts == 2
        && s.stream.presence(PARENT).expect("here").server_position == Some(nudged);

    // 3. A report the client refuses rescues nothing, and a genuine one afterwards still does.
    let mut s = build();
    let clock = s.physical.get(s.handle).expect("live").update_time;
    // An attachment the holder's parts cannot answer still advances the position stamp.
    let parent_event = ItemParentEvent {
        creature: OTHER,
        item: PARENT,
        location: NOT_A_PLACE,
        placement_frame: s.stream.presence(PARENT).expect("here").placement,
        timestamps: pyreal_protocol::types::PhysicsEventStamp {
            instance: 0,
            event: 1,
        },
    };
    feed(
        &mut s.stream,
        Opcode::ITEM_PARENT_EVENT,
        pyreal_protocol::write_body(&parent_event).expect("encode"),
        1002.0,
    );
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1002.0));
    let mut refused_rescues_nothing = s.stream.presence(PARENT).expect("here").position_ts == 1
        && s.stream.presence(PARENT).expect("here").parent.is_none()
        && s.physical.get(s.handle).expect("live").update_time == clock;
    for (stamp, teleport) in [(1u16, 0u16), (0, 0), (2, u16::MAX)] {
        feed(
            &mut s.stream,
            Opcode::MOVEMENT_POSITION_EVENT,
            report(fresh, stamp, teleport, GROUNDED),
            1003.0,
        );
        s.stream
            .sync_physics_at(&store, &mut s.physical, LocalTime(1003.0));
        refused_rescues_nothing &= s.stream.presence(PARENT).expect("here").position_ts == 1
            && s.physical.get(s.handle).expect("live").cell.is_none()
            && s.physical.get(s.handle).expect("live").update_time == clock
            && s.stream.world.tables.doomed.get(PARENT) == Some(&ServerTime(1026.0));
    }
    feed(
        &mut s.stream,
        Opcode::MOVEMENT_POSITION_EVENT,
        report(fresh, 2, 0, GROUNDED),
        1004.0,
    );
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1004.0));
    let a_genuine_one_still_does = s.stream.world.tables.doomed.get(PARENT).is_none()
        && s.physical.get(s.handle).expect("live").update_time == 1004.0;

    // 4. And the rescue is spent when it is used: a body that has entered a room is placed by a
    //    later report without going through the no-room tail again, so an unload afterwards is a
    //    fresh one and needs its own rescue.
    let mut s = build();
    feed(
        &mut s.stream,
        Opcode::MOVEMENT_POSITION_EVENT,
        report(fresh, 1, 1, GROUNDED),
        1002.0,
    );
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1002.0));
    let rescued_at = s.physical.get(s.handle).expect("live").update_time;
    let mut nudged = fresh;
    nudged.frame.origin.x += 0.125;
    feed(
        &mut s.stream,
        Opcode::MOVEMENT_POSITION_EVENT,
        report(nudged, 2, 1, GROUNDED),
        1003.0,
    );
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1003.0));
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1004.0));
    let placed_without_the_tail =
        rescued_at == 1002.0 && s.physical.get(s.handle).expect("live").update_time == 1002.0;
    // The room it is in now goes too.
    let released = s.stream.release_block_obj_cells_with_physics(
        fresh.cell.landblock(),
        LocalTime(1005.0),
        &mut s.physical,
    ) == 1
        && land.release_visible_cells(fresh.cell.landblock()).0 > 0;
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1006.0));
    let mut a_second_unload_needs_its_own_rescue = released
        && s.physical.get(s.handle).expect("live").cell.is_none()
        && s.physical.get(s.handle).expect("live").update_time == 1002.0;
    for id in [PARENT, CHILD] {
        a_second_unload_needs_its_own_rescue &=
            s.stream.world.tables.doomed.get(id) == Some(&ServerTime(1030.0));
    }
    // …and a later genuine report into the first room, loaded again, rescues it once more.
    land.load_block_cells(old.cell.landblock());
    feed(
        &mut s.stream,
        Opcode::MOVEMENT_POSITION_EVENT,
        report(old, 3, 2, GROUNDED),
        1007.0,
    );
    s.stream
        .sync_physics_at(&store, &mut s.physical, LocalTime(1007.0));
    let rescued_again = s.physical.get(s.handle).expect("live").cell == Some(old.cell)
        && s.physical.get(s.handle).expect("live").update_time == 1007.0
        && s.stream.world.tables.doomed.get(PARENT).is_none()
        && s.stream.world.tables.doomed.get(CHILD).is_none();
    s.stream.world.tables.visible.clear();
    s.stream
        .use_time::<MockTransport>(ServerTime(1_030.001), None);
    let and_survives =
        s.stream.world.physics(PARENT).is_some() && s.stream.world.physics(CHILD).is_some();

    for (what, ok) in [
        ("the body enters the loaded room", entered),
        ("and survives past the timer it was on", survived),
        (
            "an ungrounded report moves nothing",
            ungrounded_moves_nothing,
        ),
        ("a refused report rescues nothing", refused_rescues_nothing),
        (
            "a genuine one afterwards still does",
            a_genuine_one_still_does,
        ),
        (
            "a placed body is not placed through the no-room tail",
            placed_without_the_tail,
        ),
        (
            "a second unload needs its own rescue",
            a_second_unload_needs_its_own_rescue,
        ),
        ("and gets one", rescued_again && and_survives),
    ] {
        println!("rescue: {what}: {ok}");
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "objects.unloaded-cell.a-body-is-rescued-by-the-next-position-the-shard-sends",
        move |_| {
            entered
                && survived
                && ungrounded_moves_nothing
                && refused_rescues_nothing
                && a_genuine_one_still_does
                && placed_without_the_tail
                && a_second_unload_needs_its_own_rescue
                && rescued_again
                && and_survives
        },
    );
}

// ===========================================================================================
// The three the shard sends on every login
// ===========================================================================================
//
// Three server-to-client messages, each of which must reach a receiver, every one replayed here
// out of the recording that carries it, framed the way the shard framed it.

// -------------------------------------------------------------------------------------------
// shard-message.fellowship-update-done.*
// -------------------------------------------------------------------------------------------

/// The marker that ends a run of fellowship updates is consumed, and consuming it is the whole
/// of what it does.
///
/// This is deliberately the weakest of the three, because the client it rebuilds does nothing
/// with the message at all. What it asserts is that the message reaches a receiver, plus the two
/// things that must **not** happen: the roster must not move, and no line must appear for the
/// player to read.
pub fn the_fellowship_done_marker_is_consumed_and_changes_nothing() {
    let mut s = shell_support::Station::new();
    let before = {
        let app = s.client.view().expect_app();
        (
            app.hud().stats.fellow_update_done,
            app.hud().stats.fellowship_lines_composed,
            app.hud().stats.fellowship_members,
        )
    };

    let blobs = shell_support::recorded_fellowship_done_markers();
    let n = blobs.len();
    for blob in blobs {
        s.replay(blob);
        s.settle();
    }

    let app = s.client.view().expect_app();
    let consumed = app.hud().stats.fellow_update_done == before.0 + n as u64
        && !pyreal_client_runtime::dropped::unreceived(
            pyreal_protocol::Opcode::FELLOWSHIP_FELLOW_UPDATE_DONE,
        );
    let changed_nothing = app.hud().stats.fellowship_lines_composed == before.1
        && app.hud().stats.fellowship_members == before.2
        && app.objects().world.fellowship.is_none();

    s.client.assert_behaviour(
        "shard-message.fellowship-update-done.is-consumed-and-changes-nothing-else",
        move |_| n >= 2 && consumed && changed_nothing,
    );
    s.client.shutdown();
}

// -------------------------------------------------------------------------------------------
// shard-message.every-one-of-the-three-reaches-a-receiver
// -------------------------------------------------------------------------------------------

/// All three, through the whole client, against its own ledger of what it threw away.
///
/// The three counters are what makes the verdict a result rather than an absence: the ledger
/// reads the same for a message with a receiver and for one that never arrived, so the arrival
/// of each is asserted first.
pub fn none_of_the_three_reaches_no_receiver() {
    let mut s = shell_support::Station::new();
    pyreal_client_runtime::dropped::clear();

    let (pop, _) = shell_support::a_recorded_pop_up();
    s.replay(pop);
    let (house, _) = shell_support::two_recorded_house_answers();
    s.replay(house);
    let markers = shell_support::recorded_fellowship_done_markers();
    let markers_sent = markers.len();
    for blob in markers {
        s.replay(blob);
    }
    s.settle();

    let app = s.client.view().expect_app();
    let arrived = app.interaction().stats.pop_up_strings == 1
        && app.hud().stats.house_status_notices == 1
        && app.hud().stats.fellow_update_done == markers_sent as u64;
    let received = [
        pyreal_protocol::Opcode::COMMUNICATION_POP_UP_STRING,
        pyreal_protocol::Opcode::HOUSE_HOUSE_STATUS,
        pyreal_protocol::Opcode::FELLOWSHIP_FELLOW_UPDATE_DONE,
    ]
    .into_iter()
    .all(|o| !pyreal_client_runtime::dropped::unreceived(o));
    let unannounced = pyreal_client_runtime::dropped::announcements()
        .iter()
        .all(|l| !l.contains("0x0004") && !l.contains("0x0226") && !l.contains("0x01C9"));

    s.client.assert_behaviour(
        "shard-message.the-three-the-shard-sends-on-every-login-all-reach-a-receiver",
        move |_| arrived && received && unannounced,
    );
    s.client.shutdown();
}

// -------------------------------------------------------------------------------------------
// The fixtures the two above share
// -------------------------------------------------------------------------------------------

/// A whole client with its shell up, and a way of putting a recorded blob into it the way the
/// shard framed it.
///
/// It is a module of its own so that it stays apart from the object-stream fixtures above.
mod shell_support {
    use pyreal_client_net::client_session::testing::{Corpus, Direction};
    use pyreal_primitives::ObjectId;
    use pyreal_protocol::Opcode;
    use pyreal_testkit::{ClientSpec, HeadlessClient};

    /// The player this bench's own create names.
    const PLAYER: ObjectId = ObjectId(0x5000_0001);
    /// The queue the client receives all three of these on.
    const UI_QUEUE: u16 = 9;
    /// ...and the one an object's create arrives on.
    const OBJECT_QUEUE: u16 = 10;

    /// Every server-to-client blob of `scenario` whose **game-event** opcode is `want`.
    ///
    /// The outer word of a game event is the wrapper's, and the opcode is the word twelve bytes
    /// in; counting by the outer word reads zero for forty of the forty-five messages this
    /// question was first asked about, which is the shape of an instrument that cannot look
    /// reporting absence.
    fn recorded(scenario: &str, want: Opcode) -> Vec<Vec<u8>> {
        let corpus = Corpus::load(scenario)
            .expect("the corpus parses")
            .unwrap_or_else(|| panic!("{scenario} is not in the corpus"));
        corpus
            .blobs
            .into_iter()
            .filter(|b| b.dir == Direction::ServerToClient)
            .filter(|b| {
                b.opcode == 0xF7B0
                    && b.payload.len() >= 16
                    && u32::from_le_bytes(b.payload[12..16].try_into().expect("four bytes"))
                        == want.0
            })
            .map(|b| b.payload)
            .collect()
    }

    /// The recordings searched for these messages, in the corpus's own order.
    fn sessions() -> &'static [&'static str] {
        pyreal_client_net::client_session::testing::session_names()
    }

    /// One recorded prompt, and what its body decodes to -- so a scenario can compare what the
    /// box says with what the shard sent without any capture text being written down here.
    pub fn a_recorded_pop_up() -> (Vec<u8>, String) {
        for s in sessions() {
            if let Some(blob) = recorded(s, Opcode::COMMUNICATION_POP_UP_STRING).pop() {
                let said = decode_pop_up(&blob);
                return (blob, said);
            }
        }
        panic!("no recording carries a prompt from the shard")
    }

    fn decode_pop_up(blob: &[u8]) -> String {
        let mut r = pyreal_protocol::archive::Reader::new(&blob[16..]);
        let m =
            <pyreal_protocol::comms::CommunicationPopUpString as pyreal_protocol::Message>::read(
                &mut r,
            )
            .expect("the recorded body decodes");
        assert!(!m.message.is_empty(), "the shard's prompt is not empty");
        m.message
    }

    /// Two recorded answers about the player's house, from two recordings.
    pub fn two_recorded_house_answers() -> (Vec<u8>, Vec<u8>) {
        let mut out = Vec::new();
        for s in sessions() {
            if let Some(blob) = recorded(s, Opcode::HOUSE_HOUSE_STATUS).pop() {
                // The whole payload is the wrapper and one word -- the word every receiver in
                // retail discards.
                assert_eq!(blob.len(), 20, "a house answer is a wrapper and one word");
                out.push(blob);
                if out.len() == 2 {
                    return (out.remove(0), out.remove(0));
                }
            }
        }
        panic!("fewer than two recordings carry the shard's answer about a house")
    }

    /// Every recorded marker that ends a run of fellowship updates.
    pub fn recorded_fellowship_done_markers() -> Vec<Vec<u8>> {
        for s in sessions() {
            let blobs = recorded(s, Opcode::FELLOWSHIP_FELLOW_UPDATE_DONE);
            if blobs.len() >= 2 {
                for b in &blobs {
                    // The wrapper alone, with no body at all.
                    assert_eq!(b.len(), 16, "the marker carries nothing");
                }
                return blobs;
            }
        }
        panic!("no recording carries two of the fellowship markers")
    }

    /// The shard, as far as the client can tell: a connection it will accept fragments on.
    /// It is the harness's `pyreal_testkit::replay::Peer`.
    use pyreal_testkit::Peer;

    /// One client with its shell up and a peer feeding it recorded bytes.
    pub struct Station {
        pub client: HeadlessClient,
        peer: Peer,
    }

    impl Station {
        pub fn new() -> Self {
            pyreal_client_runtime::dropped::clear();
            let mut client = HeadlessClient::new(ClientSpec::gameplay_in_world(4));

            let peer = Peer::attach(&mut client, PLAYER);
            let mut s = Self { client, peer };

            // A body for the player, so the client has someone for the shard to be talking to.
            let mut p = pyreal_protocol::objects::ObjectCreatePayload {
                id: PLAYER,
                ..Default::default()
            };
            p.physicsdesc.bitfield |= pyreal_protocol::types::physicsdesc::flags::SETUP;
            p.physicsdesc.setup_id = Some(0x0200_0001);
            p.physicsdesc.timestamps.instance = 1;
            let blob = pyreal_protocol::write_blob(&pyreal_protocol::objects::ItemCreateObject(p))
                .expect("the create encodes");
            s.send(OBJECT_QUEUE, blob);
            s.client.tick(1);
            s.client.world_mut().player = Some(PLAYER);
            s.client
                .world_mut()
                .weenie_mut(PLAYER)
                .expect("the player was created")
                .pwd
                .name = "Larktest".to_owned();
            s.settle();
            s
        }

        /// Let the boundary deliver, the box get made, and its element get built.
        pub fn settle(&mut self) {
            self.client.tick(6);
        }

        /// Replay one recorded game event, re-addressed to this session.
        ///
        /// The opcode and the whole body are the recording's own bytes, padding included. The
        /// wrapper in front of them is bookkeeping and **must** be rewritten: it names the
        /// recorded character rather than this one, and its stamp is the one that session had
        /// reached -- replaying that verbatim into a session whose counter starts at zero stalls
        /// every blob behind a stamp that never arrives, which is how a replay can
        /// pass while delivering nothing at all.
        pub fn replay(&mut self, blob: Vec<u8>) {
            assert!(blob.len() >= 16, "a game event is at least a wrapper");
            assert_eq!(
                u32::from_le_bytes(blob[0..4].try_into().expect("four bytes")),
                0xF7B0,
                "this helper re-stamps ordered game events only"
            );
            // The two assertions above are this helper's own; the re-addressing and the
            // re-stamping are the harness's. The queue is pinned here rather than passed,
            // because the harness sends an ordered blob on its own -- so this says the two
            // agree instead of assuming it.
            assert_eq!(
                UI_QUEUE,
                pyreal_testkit::replay::ORDERED_QUEUE,
                "one ordered queue"
            );
            self.peer.replay_blob(&mut self.client, blob);
        }

        /// Feed one blob as the shard framed it: one fragment on `queue`.
        fn send(&mut self, queue: u16, bytes: Vec<u8>) {
            self.peer.send(&mut self.client, queue, bytes);
        }
    }
}

// -------------------------------------------------------------------------------------------
// Held objects: the holding frame, parent events, placement and picking.
// -------------------------------------------------------------------------------------------

mod held {
    use std::collections::BTreeMap;

    use pyreal_animation::data::LocationEntry;
    use pyreal_assets::Decode;
    use pyreal_client_net::client_session::SessionEvent;
    use pyreal_client_runtime::objects::ObjectStream;
    use pyreal_primitives::{CellId, DataId, Frame, LocalTime, ObjectId, Quat, Vec3, Viewport};
    use pyreal_protocol::objects::{ItemCreateObject, ItemParentEvent, ObjectCreatePayload};
    use pyreal_protocol::types::physicsdesc::{flags, PhysicsDesc};
    use pyreal_protocol::types::{
        ObjDesc, PhysicsEventStamp, PhysicsTimestamps, PositionWire, PublicWeenieDesc,
    };
    use pyreal_protocol::{write_body, Opcode};
    use {
        pyreal_client_runtime::models::resolve_parts,
        pyreal_client_runtime::models::resolve_parts_at,
        pyreal_client_runtime::models::PLACEMENT_RESTING,
    };
    use {pyreal_client_runtime::pick::PickScene, pyreal_client_runtime::pick::WorldPicker};

    use super::{store, PERSON_SETUP, RIGHT_HAND};

    const CELL: u32 = 0x8602_01AD;
    const HOLDER: u32 = 0x5000_0001;
    const ITEM: u32 = 0x5000_0002;
    const ITEM_SETUP: u32 = 0x0200_0ED7;
    const PICK_SETUP: u32 = 0x0200_0181;
    const MISSILE_SETUP: u32 = 0x0200_0124;
    const BOOKCASE_SETUP: u32 = 0x0200_0183;

    fn quat_about_z(deg: f64) -> Quat {
        let h = deg.to_radians() / 2.0;
        #[allow(clippy::cast_possible_truncation)]
        Quat::new(
            pyreal_primitives::num::math::cos(h) as f32,
            0.0,
            0.0,
            pyreal_primitives::num::math::sin(h) as f32,
        )
    }

    fn rotate(q: Quat, v: Vec3) -> (f64, f64, f64) {
        let (w, x, y, z) = (
            f64::from(q.w),
            f64::from(q.x),
            f64::from(q.y),
            f64::from(q.z),
        );
        let (vx, vy, vz) = (f64::from(v.x), f64::from(v.y), f64::from(v.z));
        let (tx, ty, tz) = (
            2.0 * (y * vz - z * vy),
            2.0 * (z * vx - x * vz),
            2.0 * (x * vy - y * vx),
        );
        (
            vx + w * tx + (y * tz - z * ty),
            vy + w * ty + (z * tx - x * tz),
            vz + w * tz + (x * ty - y * tx),
        )
    }

    fn create_event(
        id: u32,
        setup: u32,
        parent: Option<(u32, u32)>,
        placement: Option<u32>,
    ) -> SessionEvent {
        let mut physicsdesc = PhysicsDesc {
            bitfield: flags::SETUP,
            setup_id: Some(setup),
            timestamps: PhysicsTimestamps::default(),
            ..PhysicsDesc::default()
        };
        if let Some((holder, location)) = parent {
            physicsdesc.bitfield |= flags::PARENT;
            physicsdesc.parent = Some((ObjectId(holder), location));
        } else {
            physicsdesc.bitfield |= flags::POSITION;
            physicsdesc.position = Some(PositionWire {
                objcell_id: CELL,
                ..PositionWire::default()
            });
        }
        if let Some(placement) = placement {
            physicsdesc.bitfield |= flags::ANIMFRAME;
            physicsdesc.animframe_id = Some(placement);
        }
        let body = write_body(&ItemCreateObject(ObjectCreatePayload {
            id: ObjectId(id),
            objdesc: ObjDesc::default(),
            physicsdesc,
            wdesc: PublicWeenieDesc::default(),
        }))
        .expect("the create encodes");
        SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body,
        }
    }

    fn contained_create(id: u32, setup: u32) -> SessionEvent {
        let physicsdesc = PhysicsDesc {
            bitfield: flags::SETUP,
            setup_id: Some(setup),
            ..PhysicsDesc::default()
        };
        let body = write_body(&ItemCreateObject(ObjectCreatePayload {
            id: ObjectId(id),
            objdesc: ObjDesc::default(),
            physicsdesc,
            wdesc: PublicWeenieDesc::default(),
        }))
        .expect("the contained create encodes");
        SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body,
        }
    }

    fn parent_event(
        holder: u32,
        item: u32,
        location: u32,
        placement: u32,
        stamp: u16,
    ) -> SessionEvent {
        let body = write_body(&ItemParentEvent {
            creature: ObjectId(holder),
            item: ObjectId(item),
            location,
            placement_frame: placement,
            timestamps: PhysicsEventStamp {
                instance: 0,
                event: stamp,
            },
        })
        .expect("the parent event encodes");
        SessionEvent::WorldObject {
            opcode: Opcode::ITEM_PARENT_EVENT,
            body,
        }
    }

    fn objects() -> ObjectStream {
        ObjectStream::with_store(store())
    }

    pub(super) fn an_out_of_range_holding_part_uses_the_holders_frame() -> bool {
        let root = Frame::new(Vec3::new(1.0, 2.0, 3.0), quat_about_z(90.0));
        let parts = vec![Frame::new(Vec3::new(9.0, 9.0, 9.0), Quat::IDENTITY); 2];
        let holding = LocationEntry {
            part_id: 2,
            frame: Frame::new(Vec3::new(0.5, 0.0, 0.0), Quat::IDENTITY),
        };
        let want = pyreal_animation::frame::combine(&root, &holding.frame);
        let fallback = pyreal_client_runtime::models::child_frame(&root, &parts, &holding) == want;
        assert_eq!(
            pyreal_client_runtime::models::child_frame(&root, &parts, &holding),
            want,
            "an index equal to the part count is already out of range"
        );
        let inside = LocationEntry {
            part_id: 1,
            ..holding
        };
        let present_part_wins =
            pyreal_client_runtime::models::child_frame(&root, &parts, &inside) != want;
        assert!(present_part_wins);
        fallback && present_part_wins
    }

    pub(super) fn a_new_parent_event_attaches_the_child_in_wire_order() -> bool {
        let mut s = objects();
        s.apply_event(
            &create_event(HOLDER, PERSON_SETUP, None, None),
            LocalTime(0.0),
        );
        s.apply_event(&create_event(ITEM, ITEM_SETUP, None, None), LocalTime(0.0));
        let unhandled = s.stats.unhandled;
        s.apply_event(
            &parent_event(HOLDER, ITEM, RIGHT_HAND, 1, 3),
            LocalTime(0.0),
        );
        assert_eq!(s.stats.unhandled, unhandled);
        assert_eq!(s.stats.parent_events, 1);
        let item = s.presence(ObjectId(ITEM)).expect("the child remains known");
        assert_eq!(item.parent, Some((ObjectId(HOLDER), RIGHT_HAND)));
        assert_eq!(item.placement, 1);
        assert!(item.position.is_none());
        assert_eq!(item.position_ts, 3);
        let holder = s
            .presence(ObjectId(HOLDER))
            .expect("the holder remains known");
        assert!(holder.parent.is_none());
        assert!(holder.position.is_some());
        let attached = s.stats.unhandled == unhandled
            && s.stats.parent_events == 1
            && item.parent == Some((ObjectId(HOLDER), RIGHT_HAND))
            && item.placement == 1
            && item.position.is_none()
            && item.position_ts == 3
            && holder.parent.is_none()
            && holder.position.is_some();

        let mut reversed = objects();
        reversed.apply_event(
            &create_event(HOLDER, PERSON_SETUP, None, None),
            LocalTime(0.0),
        );
        reversed.apply_event(
            &create_event(ITEM, PERSON_SETUP, None, None),
            LocalTime(0.0),
        );
        reversed.apply_event(
            &parent_event(ITEM, HOLDER, RIGHT_HAND, 1, 3),
            LocalTime(0.0),
        );
        let reversed_child = reversed
            .presence(ObjectId(HOLDER))
            .expect("the reversed child exists");
        let reversed_holder = reversed
            .presence(ObjectId(ITEM))
            .expect("the reversed holder exists");
        assert_eq!(reversed_child.parent, Some((ObjectId(ITEM), RIGHT_HAND)));
        assert!(reversed_holder.parent.is_none());
        attached
            && reversed_child.parent == Some((ObjectId(ITEM), RIGHT_HAND))
            && reversed_holder.parent.is_none()
    }

    pub(super) fn old_and_incomplete_parent_events_change_nothing() -> bool {
        let mut stale = objects();
        stale.apply_event(
            &create_event(HOLDER, PERSON_SETUP, None, None),
            LocalTime(0.0),
        );
        stale.apply_event(&create_event(ITEM, ITEM_SETUP, None, None), LocalTime(0.0));
        stale.apply_event(
            &parent_event(HOLDER, ITEM, RIGHT_HAND, 1, 7),
            LocalTime(0.0),
        );
        stale.apply_event(&parent_event(HOLDER, ITEM, 2, 3, 7), LocalTime(0.0));
        stale.apply_event(&parent_event(HOLDER, ITEM, 2, 3, 4), LocalTime(0.0));
        assert_eq!(stale.stats.parent_events, 1);
        assert_eq!(stale.stats.parent_events_stale, 2);
        let item = stale
            .presence(ObjectId(ITEM))
            .expect("the attached child exists");
        assert_eq!(item.parent, Some((ObjectId(HOLDER), RIGHT_HAND)));
        assert_eq!(item.placement, 1);
        let stale_refused = stale.stats.parent_events == 1
            && stale.stats.parent_events_stale == 2
            && item.parent == Some((ObjectId(HOLDER), RIGHT_HAND))
            && item.placement == 1;

        let mut missing_item = objects();
        missing_item.apply_event(
            &create_event(HOLDER, PERSON_SETUP, None, None),
            LocalTime(0.0),
        );
        missing_item.apply_event(
            &parent_event(HOLDER, ITEM, RIGHT_HAND, 1, 3),
            LocalTime(0.0),
        );
        assert_eq!(missing_item.stats.parent_events_unknown_item, 1);
        assert_eq!(missing_item.stats.parent_events, 0);
        let missing_item_counted = missing_item.stats.parent_events_unknown_item == 1
            && missing_item.stats.parent_events == 0;

        let mut missing_holder = objects();
        missing_holder.apply_event(&create_event(ITEM, ITEM_SETUP, None, None), LocalTime(0.0));
        let before = missing_holder
            .presence(ObjectId(ITEM))
            .expect("the item exists");
        let before_parent = before.parent;
        let before_position = before.position;
        let before_placement = before.placement;
        missing_holder.apply_event(
            &parent_event(HOLDER, ITEM, RIGHT_HAND, 1, 3),
            LocalTime(0.0),
        );
        assert_eq!(missing_holder.stats.parent_events_unknown_creature, 1);
        assert_eq!(missing_holder.stats.parent_events, 0);
        let after = missing_holder
            .presence(ObjectId(ITEM))
            .expect("the item still exists");
        assert_eq!(after.parent, before_parent);
        assert_eq!(after.position, before_position);
        assert_eq!(after.placement, before_placement);
        let missing_holder_unchanged = missing_holder.stats.parent_events_unknown_creature == 1
            && missing_holder.stats.parent_events == 0
            && after.parent == before_parent
            && after.position == before_position
            && after.placement == before_placement;
        stale_refused && missing_item_counted && missing_holder_unchanged
    }

    pub(super) fn a_position_report_releases_an_object_from_its_parent() -> bool {
        use pyreal_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};

        let mut s = objects();
        s.apply_event(
            &create_event(HOLDER, PERSON_SETUP, None, None),
            LocalTime(0.0),
        );
        s.apply_event(
            &create_event(ITEM, ITEM_SETUP, Some((HOLDER, RIGHT_HAND)), Some(1)),
            LocalTime(0.0),
        );
        assert_eq!(
            s.presence(ObjectId(ITEM))
                .expect("the held item exists")
                .parent,
            Some((ObjectId(HOLDER), RIGHT_HAND))
        );
        let position = PositionPack {
            flags: position_flags::HAS_PLACEMENT_ID,
            origin: pyreal_protocol::types::Origin {
                objcell_id: CELL,
                origin: pyreal_protocol::types::Vec3::default(),
            },
            placement_id: Some(101),
            position_timestamp: 9,
            ..PositionPack::default()
        };
        let body = write_body(&MovementPositionEvent {
            id: ObjectId(ITEM),
            position,
        })
        .expect("the position event encodes");
        s.apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode::MOVEMENT_POSITION_EVENT,
                body,
            },
            LocalTime(0.0),
        );
        let item = s.presence(ObjectId(ITEM)).expect("the dropped item exists");
        assert!(item.parent.is_none());
        assert_eq!(
            item.position.map(|position| position.cell),
            Some(CellId(CELL))
        );
        assert_eq!(item.pending_placement, Some(101));
        item.parent.is_none()
            && item.position.map(|position| position.cell) == Some(CellId(CELL))
            && item.pending_placement == Some(101)
    }

    fn setup(id: u32) -> pyreal_assets::Setup {
        let store = store();
        let did = DataId(id);
        let bytes = store
            .read_typed(pyreal_dat::DbType::Setup, did)
            .expect("the setup reads");
        pyreal_assets::Setup::decode_payload(did, &bytes).expect("the setup decodes")
    }

    fn moved(a: Vec3, b: Vec3) -> f32 {
        Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z).magnitude()
    }

    pub(super) fn a_server_named_placement_draws_its_own_frames() -> bool {
        let store = store();
        let mut every_part = true;
        for (id, named) in [(MISSILE_SETUP, 52u32), (BOOKCASE_SETUP, 103)] {
            let did = DataId(id);
            let s = setup(id);
            let want = &s.placement_frames[&named].frames;
            let resting = &s.placement_frames[&PLACEMENT_RESTING].frames;
            assert_ne!(
                want, resting,
                "{did} named placement must differ from Resting"
            );
            let drawn = resolve_parts_at(&store, did, named);
            assert_eq!(drawn.len(), s.parts.len());
            assert_eq!(want.len(), s.parts.len());
            for (index, part) in drawn.iter().enumerate() {
                assert_eq!(part.frame, want[index]);
                every_part &= part.frame == want[index];
            }
            let installed = resolve_parts(&store, did);
            assert_eq!(installed.len(), s.parts.len());
            assert_eq!(resting.len(), s.parts.len());
            for (index, part) in installed.iter().enumerate() {
                assert_eq!(part.frame, resting[index]);
                every_part &= part.frame == resting[index];
            }
            let moves = drawn
                .iter()
                .zip(&installed)
                .map(|(a, b)| moved(a.frame.origin, b.frame.origin))
                .fold(0.0f32, f32::max)
                > 0.0;
            assert!(moves);
            every_part &= want != resting && moves;
        }
        every_part
    }

    struct PickAt {
        viewer: Frame,
        frames: BTreeMap<ObjectId, Frame>,
    }

    impl PickScene for PickAt {
        fn viewer(&self) -> Frame {
            self.viewer
        }

        fn object_frame(&self, id: ObjectId) -> Option<Frame> {
            self.frames.get(&id).copied()
        }

        fn fov_y_rad(&self, _viewport: (u32, u32)) -> f32 {
            45.0f32.to_radians()
        }
    }

    fn sphere_centre(placement: u32) -> (Vec3, f32) {
        let store = store();
        let s = setup(PICK_SETUP);
        let frame = s.placement_frames[&placement].frames[0];
        let bytes = store
            .read_typed(pyreal_dat::DbType::GfxObj, s.parts[0])
            .expect("the pick part reads");
        let gfx = pyreal_assets::GfxObj::decode_payload(s.parts[0], &bytes)
            .expect("the pick part decodes");
        let sphere = gfx
            .drawing_bsp
            .as_ref()
            .and_then(|tree| tree.nodes.first().and_then(|node| node.sphere))
            .expect("the pick part has a drawing sphere");
        let rotated = rotate(frame.rotation, sphere.center);
        #[allow(clippy::cast_possible_truncation)]
        let centre = Vec3::new(
            frame.origin.x + rotated.0 as f32,
            frame.origin.y + rotated.1 as f32,
            frame.origin.z + rotated.2 as f32,
        );
        (centre, sphere.radius)
    }

    fn pick_scene(centre: Vec3) -> PickAt {
        PickAt {
            viewer: Frame::new(
                Vec3::new(centre.x, centre.y - 3.0, centre.z),
                Quat::IDENTITY,
            ),
            frames: [(ObjectId(ITEM), Frame::new(Vec3::ZERO, Quat::IDENTITY))]
                .into_iter()
                .collect(),
        }
    }

    pub(super) fn a_pick_sweep_uses_the_placement_the_shard_named() -> bool {
        let store = store();
        let (combat, radius) = sphere_centre(1);
        let (resting, _) = sphere_centre(101);
        let lateral = ((combat.x - resting.x).powi(2) + (combat.z - resting.z).powi(2)).sqrt();
        assert!(lateral > radius + 0.2);
        let screen = (800u32, 600u32);
        let viewport = Viewport {
            x: 0,
            y: 0,
            width: screen.0,
            height: screen.1,
        };
        let scene = pick_scene(combat);
        let mut pick = WorldPicker::new();

        let mut posed = objects();
        posed.apply_event(
            &create_event(ITEM, PICK_SETUP, None, Some(1)),
            LocalTime(0.0),
        );
        let armed_hit = pick.find_object(399, 299, viewport);
        assert!(armed_hit);
        let hit = pick.draw_no_blit(&store, &scene, &posed, screen, viewport);
        assert_eq!(hit, Some(ObjectId(ITEM)));

        let mut resting_objects = objects();
        resting_objects.apply_event(
            &create_event(ITEM, PICK_SETUP, None, Some(101)),
            LocalTime(0.0),
        );
        let armed_miss = pick.find_object(399, 299, viewport);
        assert!(armed_miss);
        let miss = pick.draw_no_blit(&store, &scene, &resting_objects, screen, viewport);
        assert_eq!(miss, Some(ObjectId(0)));
        lateral > radius + 0.2
            && armed_hit
            && hit == Some(ObjectId(ITEM))
            && armed_miss
            && miss == Some(ObjectId(0))
    }

    /// [`PickAt`] with a drawn picture: what it shows at the click and how far off it is.
    struct Pictured {
        swept: PickAt,
        shows: ObjectId,
        front: Option<f32>,
    }

    impl PickScene for Pictured {
        fn viewer(&self) -> Frame {
            self.swept.viewer
        }

        fn object_frame(&self, id: ObjectId) -> Option<Frame> {
            self.swept.object_frame(id)
        }

        fn fov_y_rad(&self, viewport: (u32, u32)) -> f32 {
            self.swept.fov_y_rad(viewport)
        }

        fn picked(&self, _at: (f32, f32)) -> Option<ObjectId> {
            Some(self.shows)
        }

        fn drawn_distance(&self, _at: (f32, f32)) -> Option<f32> {
            self.front
        }
    }

    pub(super) fn where_the_picture_shows_no_object_the_sweep_answers() -> bool {
        let store = store();
        let (combat, _) = sphere_centre(1);
        let screen = (800u32, 600u32);
        let viewport = Viewport {
            x: 0,
            y: 0,
            width: screen.0,
            height: screen.1,
        };
        let mut posed = objects();
        posed.apply_event(
            &create_event(ITEM, PICK_SETUP, None, Some(1)),
            LocalTime(0.0),
        );
        // The item is three metres off: the sky or the ground behind it lets the sweep answer,
        // a wall half a metre off does not, and an object the picture names is the answer.
        let other = ObjectId(0x5000_0099);
        let cases = [
            (ObjectId(0), None, ObjectId(ITEM)),
            (ObjectId(0), Some(10.0), ObjectId(ITEM)),
            (ObjectId(0), Some(0.5), ObjectId(0)),
            (other, Some(0.5), other),
        ];
        let mut pick = WorldPicker::new();
        cases.iter().all(|&(shows, front, want)| {
            let scene = Pictured {
                swept: pick_scene(combat),
                shows,
                front,
            };
            assert!(pick.find_object(399, 299, viewport));
            let got = pick.draw_no_blit(&store, &scene, &posed, screen, viewport);
            assert_eq!(
                got,
                Some(want),
                "shows {shows:?} with a surface at {front:?}"
            );
            got == Some(want)
        })
    }

    pub(super) fn a_held_object_is_pickable_and_a_contained_object_is_not() -> bool {
        let store = store();
        let (combat, _) = sphere_centre(1);
        let screen = (800u32, 600u32);
        let viewport = Viewport {
            x: 0,
            y: 0,
            width: screen.0,
            height: screen.1,
        };
        let scene = pick_scene(combat);

        let mut held = objects();
        held.apply_event(
            &create_event(HOLDER, PERSON_SETUP, None, None),
            LocalTime(0.0),
        );
        held.apply_event(
            &create_event(ITEM, PICK_SETUP, Some((HOLDER, RIGHT_HAND)), Some(1)),
            LocalTime(0.0),
        );
        let item = held.presence(ObjectId(ITEM)).expect("the held item exists");
        let really_held = item.parent.is_some() && item.position.is_none();
        assert!(really_held);
        let mut pick = WorldPicker::new();
        let armed_held = pick.find_object(399, 299, viewport);
        assert!(armed_held);
        let held_pick = pick.draw_no_blit(&store, &scene, &held, screen, viewport);
        assert_eq!(held_pick, Some(ObjectId(ITEM)));

        let mut contained = objects();
        contained.apply_event(&contained_create(ITEM, PICK_SETUP), LocalTime(0.0));
        let mut pick = WorldPicker::new();
        let armed_contained = pick.find_object(399, 299, viewport);
        assert!(armed_contained);
        let contained_pick = pick.draw_no_blit(&store, &scene, &contained, screen, viewport);
        assert_eq!(contained_pick, Some(ObjectId(0)));
        really_held
            && armed_held
            && held_pick == Some(ObjectId(ITEM))
            && armed_contained
            && contained_pick == Some(ObjectId(0))
    }
}

pub fn an_out_of_range_holding_part_uses_the_holders_frame() {
    let held = held::an_out_of_range_holding_part_uses_the_holders_frame();
    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "objects.held-frame.an-out-of-range-part-uses-the-holders-own-frame",
        move |_| held,
    );
}

pub fn a_new_parent_event_attaches_the_child_in_wire_order() {
    let attached = held::a_new_parent_event_attaches_the_child_in_wire_order();
    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "objects.parent.a-new-parent-event-attaches-the-child-in-wire-order",
        move |_| attached,
    );
}

pub fn old_and_incomplete_parent_events_change_nothing() {
    let unchanged = held::old_and_incomplete_parent_events_change_nothing();
    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "objects.parent.an-old-or-incomplete-parent-event-changes-nothing",
        move |_| unchanged,
    );
}

pub fn a_position_report_releases_an_object_from_its_parent() {
    let released = held::a_position_report_releases_an_object_from_its_parent();
    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "objects.position.a-position-report-releases-an-object-from-its-parent",
        move |_| released,
    );
}

pub fn a_server_named_placement_draws_its_own_frames() {
    let placed = held::a_server_named_placement_draws_its_own_frames();
    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "objects.placement.a-server-named-placement-draws-its-own-frames",
        move |_| placed,
    );
}

pub fn a_pick_sweep_uses_the_placement_the_shard_named() {
    let picked = held::a_pick_sweep_uses_the_placement_the_shard_named();
    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "objects.pick.a-sweep-uses-the-placement-the-shard-named",
        move |_| picked,
    );
}

pub fn where_the_picture_shows_no_object_the_sweep_answers() {
    let picked = held::where_the_picture_shows_no_object_the_sweep_answers();
    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "objects.pick.where-the-picture-shows-no-object-the-sweep-answers",
        move |_| picked,
    );
}

pub fn a_held_object_is_pickable_and_a_contained_object_is_not() {
    let picked = held::a_held_object_is_pickable_and_a_contained_object_is_not();
    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "objects.held-object.is-a-pick-candidate-while-a-contained-object-is-not",
        move |_| picked,
    );
}
