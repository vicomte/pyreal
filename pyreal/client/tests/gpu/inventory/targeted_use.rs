//! Targeted item use: using an item arms the targeted mode and retains the item as the source
//! through selection changes, refusals and cancel; acquiring a compatible target sends one
//! `Inventory_UseWithTargetEvent 0x0035` whose bytes equal the recorded message, while an
//! incompatible target, a confirmation dialog or a retained-item target sends nothing; the
//! busy count follows the send and the recorded use-done release.
//! Fixture: the `long-solo-play` corpus (oil `0x80000A6F`, bow `0x800009A4`) applied to an
//! `ObjectStream`.
use pyreal_client_net::client_session::{
    testing::{Corpus, CorpusBlob, Direction},
    SessionEvent,
};
use pyreal_client_runtime::objects::ObjectStream;
use pyreal_primitives::{LocalTime, ObjectId};
use pyreal_protocol::Opcode;

const PLAYER: ObjectId = ObjectId(0x5000_000a);
const OIL: ObjectId = ObjectId(0x8000_0a6f);
const BOW: ObjectId = ObjectId(0x8000_09a4);

fn recorded_world(before: usize) -> ObjectStream {
    let mut objects = ObjectStream::new();
    objects.world.player = Some(PLAYER);
    let rows = &Corpus::shared("long-solo-play").blobs;
    for row in rows.iter().filter(|r| r.idx < before) {
        if let Some(e) = event(row) {
            objects.apply_event(
                &e,
                LocalTime(std::time::Duration::from_micros(row.t_rel_micros).as_secs_f64()),
            );
        }
    }
    objects
}

// Item use retains this exact source and arms targeted mode 3, not generic mode 1.
#[test]
fn recorded_oil_use_arms_the_distinct_targeted_mode_without_sending() {
    let mut objects = recorded_world(4315);
    let mut inter = pyreal_client_runtime::interaction::Interaction::default();
    inter.queue(
        Vec::new(),
        vec![pyreal_client_contract::view::UiRequest::Use(OIL)],
    );
    assert!(inter
        .run_ui_requests(
            &mut objects.world,
            false,
            pyreal_primitives::ServerTime(573.0)
        )
        .is_empty());
    assert_eq!(
        inter.target_mode(),
        pyreal_client_runtime::interaction::TargetMode::UseTarget
    );
    assert!(inter.pending_requests().is_empty());
    assert_eq!(objects.world.magic.busy_count, 0);
}

// Generic use acts on the clicked object, which may replace Use with UseTarget.
#[test]
fn generic_use_cursor_can_rearm_targeted_use_on_the_clicked_oil() {
    let mut objects = recorded_world(4315);
    objects.world.selected = None;
    let mut inter = pyreal_client_runtime::interaction::Interaction::default();
    inter.queue(
        Vec::new(),
        vec![pyreal_client_contract::view::UiRequest::SetTargetMode(
            pyreal_client_contract::view::TargetMode::Use,
        )],
    );
    inter.run_ui_requests(
        &mut objects.world,
        false,
        pyreal_primitives::ServerTime(572.0),
    );
    inter.wrapper_mouse(
        pyreal_client_runtime::interaction::UiMouseEvent {
            action: 7,
            start: true,
            x: 30,
            y: 30,
            over: None,
        },
        (800, 600),
        true,
    );
    inter.on_world_object_found(
        OIL,
        &mut objects.world,
        pyreal_primitives::ServerTime(573.0),
    );
    assert_eq!(
        inter.target_mode(),
        pyreal_client_runtime::interaction::TargetMode::UseTarget
    );
    assert!(inter.pending_requests().is_empty());
    assert_eq!(objects.world.selected, None);
}

fn arm(
    inter: &mut pyreal_client_runtime::interaction::Interaction,
    world: &mut pyreal_client_model::World,
    item: ObjectId,
    now: f64,
) {
    inter.queue(
        Vec::new(),
        vec![pyreal_client_contract::view::UiRequest::Use(item)],
    );
    assert!(inter
        .run_ui_requests(world, false, pyreal_primitives::ServerTime(now))
        .is_empty());
}

// This is the production WorldObjects notice boundary, NOT an assertion about 3D ray picking or
// inventory input. The action bytes and response stream are independent recording oracles.
fn acquire(
    inter: &mut pyreal_client_runtime::interaction::Interaction,
    world: &mut pyreal_client_model::World,
    target: ObjectId,
    now: f64,
) {
    inter.wrapper_mouse(
        pyreal_client_runtime::interaction::UiMouseEvent {
            action: 7,
            start: true,
            x: 30,
            y: 30,
            over: None,
        },
        (800, 600),
        true,
    );
    inter.on_world_object_found(target, world, pyreal_primitives::ServerTime(now));
}

fn assert_recorded_request(requests: &[pyreal_client_model::Request], index: usize) {
    use pyreal_protocol::Message;
    let [pyreal_client_model::Request::UseWithTargetEvent(m)] = requests else {
        panic!("exactly one0035, got{requests:?}")
    };
    let rows = &Corpus::shared("long-solo-play").blobs;
    let row = rows.iter().find(|r| r.idx == index).unwrap();
    let mut got = pyreal_protocol::items::InventoryUseWithTargetEvent::OPCODE
        .0
        .to_le_bytes()
        .to_vec();
    got.extend(pyreal_protocol::write_body(m).expect("encode emitted request"));
    assert_eq!(
        got,
        row.payload[8..],
        "independent retail0035 body, only order stamp excluded"
    );
}

#[test]
fn retained_source_survives_selection_and_recorded_use_done_allows_a_second_request() {
    let mut objects = recorded_world(4315);
    let mut inter = pyreal_client_runtime::interaction::Interaction::default();
    arm(&mut inter, &mut objects.world, OIL, 573.0);
    assert_eq!(objects.world.targeting_object, OIL);
    objects.world.selected = Some(PLAYER); // Independent selection change after arming.
    acquire(&mut inter, &mut objects.world, BOW, 573.01); // Less than first-use throttle.
    assert_recorded_request(&inter.take_pending_requests(), 4315);
    assert_eq!(objects.world.targeting_object, ObjectId(0));
    assert_eq!(objects.world.selected, Some(PLAYER));
    assert_eq!(objects.world.magic.busy_count, 1);
    assert!(
        objects.world.weenie(OIL).is_some(),
        "no optimistic deletion"
    );
    assert!(objects.world.weenie(BOW).is_some());

    let rows = &Corpus::shared("long-solo-play").blobs;
    for row in rows.iter().filter(|r| (4316..4363).contains(&r.idx)) {
        if let Some(e) = event(row) {
            objects.apply_event(
                &e,
                LocalTime(std::time::Duration::from_micros(row.t_rel_micros).as_secs_f64()),
            );
            pyreal_client_runtime::interaction::apply_events(
                &mut inter,
                std::slice::from_ref(&e),
                &mut objects.world,
            );
        }
        if row.idx == 4324 {
            assert_eq!(objects.world.magic.busy_count, 0, "recorded01C7");
        }
    }
    // This test covers the 01C7 busy release and a second independent 0035 only; inventory
    // detach and the 25s deferred destruction after the UI 0024 message are covered elsewhere.
    assert!(
        objects.world.weenie(ObjectId(0x8000_0a74)).is_some(),
        "authoritative new item"
    );
    assert_eq!(objects.world.magic.busy_count, 0);
    let second = ObjectId(0x8000_0a73);
    arm(&mut inter, &mut objects.world, second, 579.0);
    objects.world.selected = Some(PLAYER);
    acquire(
        &mut inter,
        &mut objects.world,
        ObjectId(0x8000_099d),
        579.01,
    );
    assert_recorded_request(&inter.take_pending_requests(), 4363);
    assert_eq!(objects.world.magic.busy_count, 1);
    assert_eq!(objects.world.targeting_object, ObjectId(0));
}

#[test]
fn incompatible_acquisition_consumes_source_without_sending_and_allows_a_fresh_arm() {
    let mut objects = recorded_world(4315);
    let mut inter = pyreal_client_runtime::interaction::Interaction::default();
    arm(&mut inter, &mut objects.world, OIL, 573.0);
    acquire(&mut inter, &mut objects.world, PLAYER, 573.01);
    assert!(inter.take_pending_requests().is_empty());
    assert_eq!(objects.world.targeting_object, ObjectId(0));
    assert_eq!(objects.world.magic.busy_count, 0);
    arm(&mut inter, &mut objects.world, OIL, 574.0);
    acquire(&mut inter, &mut objects.world, BOW, 574.01);
    assert_recorded_request(&inter.take_pending_requests(), 4315);
}

#[test]
fn first_use_busy_and_throttle_refusals_do_not_replace_the_retained_source() {
    use pyreal_client_model::inventory::requests::InventoryRequest;
    let mut objects = recorded_world(4315);
    let mut inter = pyreal_client_runtime::interaction::Interaction::default();
    let second = ObjectId(0x8000_0a73);
    assert!(
        objects.world.weenie(second).is_some(),
        "throttle is not an unknown-object gate"
    );
    arm(&mut inter, &mut objects.world, OIL, 573.0);
    arm(&mut inter, &mut objects.world, second, 573.1);
    assert_eq!(
        objects.world.targeting_object, OIL,
        "throttle preserves previous target source"
    );
    assert_eq!(
        objects.world.last_used,
        Some(pyreal_primitives::ServerTime(573.0))
    );
    objects.world.request_lock.record(
        BOW,
        InventoryRequest::Drop,
        pyreal_primitives::ServerTime(573.0),
    );
    arm(&mut inter, &mut objects.world, BOW, 574.0);
    assert_eq!(
        objects.world.targeting_object, OIL,
        "busy refusal preserves previous target source"
    );
    assert!(inter.take_pending_requests().is_empty());
    objects.world.request_lock.clear();
    arm(&mut inter, &mut objects.world, ObjectId(0xffff_ffff), 575.0);
    assert_eq!(
        objects.world.targeting_object, OIL,
        "missing source does not erase retained source"
    );
    arm(&mut inter, &mut objects.world, second, 575.0);
    assert_eq!(
        objects.world.targeting_object, second,
        "positive control after both refusal gates"
    );
}

#[test]
fn generic_use_calls_use_object_on_clicked_door_not_use_with_selected_item() {
    let mut objects = recorded_world(4112);
    let door = ObjectId(0x77f0_3053);
    objects.world.selected = Some(PLAYER);
    let mut inter = pyreal_client_runtime::interaction::Interaction::default();
    // Source 0 enters the generic toolbar/USE action, rather than using a specific item.
    arm(&mut inter, &mut objects.world, ObjectId(0), 510.0);
    assert_eq!(
        inter.target_mode(),
        pyreal_client_runtime::interaction::TargetMode::Use
    );
    acquire(&mut inter, &mut objects.world, door, 511.0);
    assert!(matches!(&inter.take_pending_requests()[..],
        [pyreal_client_model::Request::UseEvent(m)] if m.object == door));
    assert_eq!(objects.world.targeting_object, ObjectId(0));
    assert_eq!(objects.world.magic.busy_count, 1);
    assert_eq!(objects.world.selected, Some(PLAYER));
}

#[test]
fn targeted_confirmation_gates_and_callback_keep_identity_and_busy_order() {
    use pyreal_client_model::inventory::{
        targeted_use::{TargetedUsageConfirmation as C, TargetedUseOutcome as O},
        SplitState,
    };
    use pyreal_client_model::{Notice, RecordingRequests, RecordingSink};
    let mut objects = recorded_world(4315);
    let world = &mut objects.world;
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    // Branch control built from an otherwise compatible recorded pair: no recording contains an
    // empty-mana-stone or salvage-confirmation gesture.
    world.weenie_mut(OIL).unwrap().pwd.obj_type = 0x0008_0000;
    world.weenie_mut(OIL).unwrap().pwd.effects = Some(0);
    world.targeting_object = OIL;
    assert_eq!(
        world.target_acquired(
            &mut req,
            &mut out,
            BOW,
            SplitState::default(),
            pyreal_primitives::ServerTime(574.0)
        ),
        O::Confirming(C::ManaStone)
    );
    assert_eq!(world.targeting_object, ObjectId(0));
    assert_eq!(
        out.0,
        vec![
            Notice::DisplayString {
                feedback: pyreal_client_contract::feedback::Feedback::INFORMATION,
                channel: 0x1a,
                text: format!(
                    "Using the {} with the {}",
                    world
                        .weenie(OIL)
                        .unwrap()
                        .object_name(pyreal_client_model::weenie::NameType::Appropriate),
                    world
                        .weenie(BOW)
                        .unwrap()
                        .object_name(pyreal_client_model::weenie::NameType::Appropriate)
                )
            },
            Notice::TargetedUsageConfirmation {
                source: OIL,
                target: BOW,
                kind: C::ManaStone
            },
        ]
    );
    assert!(req.0.is_empty());
    assert_eq!(
        world.magic.busy_count, 0,
        "dialog/cancel does not claim busy"
    );
    world.selected = Some(PLAYER);
    world.confirm_targeted_usage(
        &mut req,
        &mut out,
        OIL,
        BOW,
        SplitState::default(),
        pyreal_primitives::ServerTime(575.0),
    );
    assert_recorded_request(&req.0, 4315);
    assert_eq!(world.magic.busy_count, 1);
    world.use_done(0);
    req.0.clear();
    out.0.clear();
    world.weenie_mut(BOW).unwrap().pwd.bitfield |= 0x0100_0000;
    world.targeting_object = OIL;
    assert_eq!(
        world.target_acquired(
            &mut req,
            &mut out,
            BOW,
            SplitState::default(),
            pyreal_primitives::ServerTime(576.0)
        ),
        O::Retained
    );
    assert_eq!(world.targeting_object, ObjectId(0));
    assert!(req.0.is_empty());
    assert_eq!(world.magic.busy_count, 0);
    assert!(
        matches!(&out.0[..], [Notice::DisplayString { channel: 0x1a, .. }, Notice::DisplayString { channel: 0, text , ..}] if text.contains("\"Retained\""))
    );
    world.weenie_mut(BOW).unwrap().pwd.bitfield &= !0x0100_0000;
    world.weenie_mut(OIL).unwrap().pwd.obj_type = 0x4000_0000;
    world.player_system.options.set(26, false);
    world.targeting_object = OIL;
    assert_eq!(
        world.target_acquired(
            &mut req,
            &mut out,
            BOW,
            SplitState::default(),
            pyreal_primitives::ServerTime(577.0)
        ),
        O::Confirming(C::Salvage)
    );
    assert!(req.0.is_empty());
    world.player_system.options.set(26, true);
    world.targeting_object = OIL;
    assert_eq!(
        world.target_acquired(
            &mut req,
            &mut out,
            BOW,
            SplitState::default(),
            pyreal_primitives::ServerTime(578.0)
        ),
        O::Sent
    );
    assert_recorded_request(&req.0, 4315);
}

#[test]
fn accepted_targeted_callback_requires_ids_and_runs_using_item_tail_after_the_send() {
    use pyreal_client_model::inventory::{targeted_use::TargetedUseOutcome, SplitState};
    use pyreal_client_model::{Notice, RecordingRequests, RecordingSink};
    let mut objects = recorded_world(4315);
    let world = &mut objects.world;
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    world.install_material_names(std::collections::BTreeMap::from([(0x3a, "Bronze".into())]));
    world.weenie_mut(OIL).unwrap().pwd.material_type = Some(0x3a);
    world.weenie_mut(BOW).unwrap().pwd.material_type = Some(0x3a);
    for (source, target) in [(ObjectId(0), BOW), (OIL, ObjectId(0))] {
        world.confirm_targeted_usage(
            &mut req,
            &mut out,
            source,
            target,
            SplitState::default(),
            pyreal_primitives::ServerTime(574.0),
        );
        assert!(req.0.is_empty() && out.0.is_empty());
        assert_eq!(world.magic.busy_count, 0);
    }
    // Positive tail: an owned container also carrying a filled mana-stone type. effects 1
    // bypasses the empty-stone dialog; the using-item step's unconditional owned-container tail
    // emits OpenContainedContainer. The normal request still uses the recorded pair's ids.
    world.weenie_mut(OIL).unwrap().pwd.obj_type = 0x0008_0200;
    world.weenie_mut(OIL).unwrap().pwd.items_capacity = Some(1);
    world.weenie_mut(OIL).unwrap().pwd.effects = Some(1);
    world.targeting_object = OIL;
    assert_eq!(
        world.target_acquired(
            &mut req,
            &mut out,
            BOW,
            SplitState::default(),
            pyreal_primitives::ServerTime(574.0)
        ),
        TargetedUseOutcome::Sent
    );
    assert_recorded_request(&req.0, 4315);
    assert_eq!(world.magic.busy_count, 1);
    assert_eq!(
        out.0,
        vec![
            Notice::DisplayString {
                feedback: pyreal_client_contract::feedback::Feedback::INFORMATION,
                channel: 0x1a,
                text: "Using the Bronze Oil of Rendering with the Bronze Training Shortbow".into()
            },
            Notice::OpenContainedContainer(OIL),
        ]
    );
}

fn event(row: &CorpusBlob) -> Option<SessionEvent> {
    if row.dir != Direction::ServerToClient {
        return None;
    }
    Some(match row.opcode {
        0xf7b0 if row.payload[12..16] == 0x13_u32.to_le_bytes() => {
            SessionEvent::PlayerDescription(Box::new(
                pyreal_protocol::read_body(&row.payload[16..])
                    .expect("recorded player description"),
            ))
        }
        0xf7b0 => SessionEvent::UiEvent {
            opcode: Opcode(u32::from_le_bytes(row.payload[12..16].try_into().unwrap())),
            blob: row.payload[12..].to_vec(),
        },
        _ if row.queue == pyreal_primitives::NetQueue::UiQueue => SessionEvent::UiEvent {
            opcode: Opcode(row.opcode),
            blob: row.payload.clone(),
        },
        0xf746 => SessionEvent::PlayerCreated(ObjectId(u32::from_le_bytes(
            row.payload[4..8].try_into().unwrap(),
        ))),
        _ => SessionEvent::WorldObject {
            opcode: Opcode(row.opcode),
            body: row.payload[4..].to_vec(),
        },
    })
}

#[test]
fn recorded_targeted_pairs_have_independent_source_and_target_descriptors() {
    let mut count = 0;
    for session in ["early-inventory-and-casting", "long-solo-play"] {
        let rows = &Corpus::shared(session).blobs;
        let mut objects = ObjectStream::new();
        for row in rows {
            if let Some(e) = event(row) {
                objects.apply_event(
                    &e,
                    LocalTime(std::time::Duration::from_micros(row.t_rel_micros).as_secs_f64()),
                );
            } else if row.opcode == 0xf7b1 && row.payload[8..12] == 0x35_u32.to_le_bytes() {
                let source = ObjectId(u32::from_le_bytes(row.payload[12..16].try_into().unwrap()));
                let target = ObjectId(u32::from_le_bytes(row.payload[16..20].try_into().unwrap()));
                for id in [source, target] {
                    let w = objects
                        .world
                        .weenie(id)
                        .expect("recorded target pair exists");
                    eprintln!("{session}/{} {id:?} {:?}, use={:?} targettype={:?}, type={:x}, container={:?}, effect={:?}", row.idx, w.pwd.name, w.pwd.useability, w.pwd.target_type, w.inq_type(), w.pwd.container_id, w.pwd.effects);
                }
                count += 1;
            }
        }
    }
    assert!(count > 0, "the recordings carry no targeted use (0x0035)");
}
