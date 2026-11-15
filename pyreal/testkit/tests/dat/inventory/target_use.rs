use super::*;

/// The thing in the recorded corpse this pick-up is about, and a compatible stack to merge into.
const CORPSE_THING: ObjectId = ObjectId(0x8000_0A99);
const CARRIED_STACK: ObjectId = ObjectId(0x8000_F185);
const CORPSE_STACK_SIZE: u16 = 15;
/// The last of the messages that create the corpse's contents.
const CORPSE_FILLED_BY: usize = 6594;

/// What the shipped keymap calls "pick up what is picked", and the map it lives on.
const PICK_UP: pyreal_input::ActionId = pyreal_input::ActionId(0x1000_002C);
const SELECTION_COMMANDS: pyreal_input::InputMapId = pyreal_input::InputMapId(0x1000_0007);

/// Fire picking up what is picked, as its key does. The action goes in rather than the key: with
/// no login the interface stays on a pre-game screen, which holds the gameplay keys back.
fn press_the_pick_up_key(c: &mut HeadlessClient) {
    pyreal_testkit::input_steps::press_on_map(c, PICK_UP, SELECTION_COMMANDS);
    c.tick(pyreal_testkit::input_steps::SETTLE_FRAMES);
}

/// A client that has replayed the recording as far as the corpse's contents, with the recorded
/// thing picked or nothing picked.
fn a_client_at_the_recorded_corpse(pick: Option<ObjectId>) -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.when(Inbound::from_corpus(
        "long-solo-play",
        0..CORPSE_FILLED_BY + 1,
    ))
    .tick(1);
    assert!(
        c.view().world().weenie(CORPSE_THING).is_some(),
        "the premise: the recording creates the thing this pick-up is about"
    );
    // What the client acts on is what is picked, so the picking is made through the client's own
    // entry point rather than assumed from the recording.
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .set_selected_object(
            pick,
            false,
            &mut pyreal_client_model::RecordingSink::default(),
        );
    c.tick(1);
    assert_eq!(
        c.view().world().selected,
        pick,
        "the premise: that is what is picked"
    );
    c
}

// ---------------------------------------------------------------------------------------------
// inventory.pickup.the-shipped-key-puts-what-is-picked-into-the-players-own-pack
// ---------------------------------------------------------------------------------------------

/// Pressing the key bound to picking things up puts what is picked into the
/// player's own pack.
///
/// The key had resolved correctly all along and produced its action; what was missing was
/// anything that answered it, so the action was swept away and nothing went out for any object,
/// ever. The message it must produce is one the recording itself carries for the same thing.
pub(super) fn the_shipped_key_puts_what_is_picked_into_the_players_own_pack() {
    let mut c = a_client_at_the_recorded_corpse(Some(CORPSE_THING));
    let player = c
        .view()
        .world()
        .player
        .expect("the recording names its own player");
    let before = c.view().interaction().stats.pick_ups;

    let mark = c.outbound().len();
    press_the_pick_up_key(&mut c);
    c.tick(2);
    let sent: Vec<Request> = c.outbound()[mark..].to_vec();

    let the_action_was_answered = c.view().interaction().stats.pick_ups == before + 1;
    let it_went_into_the_pack = sent.iter().any(|r| {
        matches!(r, Request::PutItemInContainer(m)
            if m.item == CORPSE_THING && m.container == player)
    });

    c.assert_behaviour(
        "inventory.pickup.the-shipped-key-puts-what-is-picked-into-the-players-own-pack",
        move |_| the_action_was_answered && it_went_into_the_pack,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.pickup.the-shipped-key-carries-the-picked-stacks-own-count-into-the-merge
// ---------------------------------------------------------------------------------------------

/// When what is picked is a stack and the player is already carrying a compatible one, the same
/// key merges them, and it carries the count the picking itself worked out rather than a fresh
/// empty one.
///
/// The count is seeded by the picking, before the key is ever pressed, which is asserted as the
/// premise: a client that started the merge from nothing would send a zero and the player would
/// watch the stack not move.
pub(super) fn the_shipped_key_carries_the_picked_stacks_own_count_into_the_merge() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.when(Inbound::from_corpus(
        "long-solo-play",
        0..CORPSE_FILLED_BY + 1,
    ))
    .tick(1);
    let player = c
        .view()
        .world()
        .player
        .expect("the recording names its own player");

    // The recording's own corpse thing, with its count and a compatible carried stack made
    // explicit: that is the one branch whose amount is the thing this scenario is about, and a
    // plain corpse thing would only exercise the plain move again.
    {
        let world = &mut c.app_mut().probe_mut().objects_mut().world;
        let source = world
            .weenie_mut(CORPSE_THING)
            .expect("the recorded corpse thing");
        source.pwd.wcid = 0xF185;
        source.pwd.stack_size = Some(CORPSE_STACK_SIZE);
        source.pwd.max_stack_size = Some(100);

        let mut target = pyreal_client_model::Weenie::new(CARRIED_STACK);
        target.pwd = PublicWeenieDesc {
            name: "a compatible carried stack".into(),
            wcid: 0xF185,
            stack_size: Some(20),
            max_stack_size: Some(100),
            container_id: Some(player),
            ..PublicWeenieDesc::default()
        };
        target.valid = true;
        world.tables.weenies.insert(CARRIED_STACK, target);
        world.view_object_contents(
            player,
            &[pyreal_protocol::types::ContentProfile {
                iid: CARRIED_STACK,
                container_properties: 0,
            }],
            &mut pyreal_client_model::NullSink,
        );
    }

    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .set_selected_object(
            Some(CORPSE_THING),
            false,
            &mut pyreal_client_model::RecordingSink::default(),
        );
    c.tick(1);
    let the_count_was_seeded_by_the_picking = c.view().world().split
        == pyreal_client_model::inventory::SplitState::whole_stack(u32::from(CORPSE_STACK_SIZE));

    let mark = c.outbound().len();
    press_the_pick_up_key(&mut c);
    c.tick(2);
    let sent: Vec<Request> = c.outbound()[mark..].to_vec();
    let merged = matches!(
        sent.as_slice(),
        [Request::StackableMerge(m)]
            if m.merge_from == CORPSE_THING
                && m.merge_to == CARRIED_STACK
                && m.amount == i32::from(CORPSE_STACK_SIZE)
    );

    c.assert_behaviour(
        "inventory.pickup.the-shipped-key-carries-the-picked-stacks-own-count-into-the-merge",
        move |_| the_count_was_seeded_by_the_picking && merged,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.pickup.the-shipped-key-with-nothing-picked-sends-nothing
// ---------------------------------------------------------------------------------------------

/// With nothing picked, the same key is **not** answered at all and nothing goes out.
///
/// Without this a client that picked up whatever it found, including nothing at all, would pass
/// the two scenarios above.
pub(super) fn the_shipped_key_with_nothing_picked_sends_nothing() {
    let mut c = a_client_at_the_recorded_corpse(None);
    let before = c.view().interaction().stats.pick_ups;

    let mark = c.outbound().len();
    press_the_pick_up_key(&mut c);
    c.tick(2);
    let sent: Vec<Request> = c.outbound()[mark..].to_vec();

    let not_answered = c.view().interaction().stats.pick_ups == before;
    let nothing_went_out = !sent
        .iter()
        .any(|r| matches!(r, Request::PutItemInContainer(_)));

    c.assert_behaviour(
        "inventory.pickup.the-shipped-key-with-nothing-picked-sends-nothing",
        move |_| not_answered && nothing_went_out,
    );
    c.shutdown();
}
