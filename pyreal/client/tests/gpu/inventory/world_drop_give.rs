//! A drag released over the world view arms a world pick, the pick's answer places the item, and
//! the search reason clears afterwards. A release over `<SBOX>` becomes `DropTarget::World`, which stores the item, sets the drop search reason
//! from the cursor position and starts a world pick. A pick that finds a creature sends
//! `Inventory_GiveObjectRequest 0x00CD` (byte-identical to every recorded one), a pick that finds
//! nothing sends `Inventory_DropItem 0x001B`, a refused drop sends nothing, and in every case the
//! object-found notice's tail clears the reason so the next viewport click still arms a pick.
//! Fixture: the recordings replayed over a loopback `ClientNetwork` into `Interaction`.

#![cfg(gpu)]

use crate::common::client_dir;

use std::collections::BTreeMap;
use std::path::PathBuf;

use pyreal_client_contract::gameplay::SMART_BOX;
use pyreal_client_contract::view::{DropTarget, UiRequest};
use pyreal_client_model::inventory::requests::InventoryRequest;
use pyreal_client_net::client_session::testing::{shared_session, Corpus, Direction};
use pyreal_client_net::client_session::SessionEvent;
use pyreal_client_net::recording::connection_sequence_number;
use pyreal_client_runtime::interaction::UiMouseEvent;
use pyreal_client_runtime::net::ClientNetwork;
use pyreal_client_runtime::objects::ObjectStream;
use pyreal_primitives::{AssetSource, LocalTime, ObjectId, ServerTime};
use pyreal_rules::weenie::item_type;
use pyreal_transport::wire::ParsedPacket;
use {pyreal_client_runtime::interaction, pyreal_client_runtime::interaction::SearchReason};

/// `0x00CD Inventory_GiveObjectRequest`, pinned as a literal rather than reached through a symbol.
const AC_GIVE_OBJECT_REQUEST: u32 = 0x00CD;
/// `0x001B Inventory_DropItem`.
const AC_DROP_ITEM: u32 = 0x001B;
const OP_GAME_ACTION: u32 = 0xF7B1;
const OP_CREATE_OBJECT: u32 = 0xF745;
/// Bright red — the inventory feedback channel.
const FEEDBACK_CHANNEL: u32 = 0x1A;

// =============================================================================================
// The corpus.
// =============================================================================================

/// **An `expect`:** absent dats fail the test.
fn dats() -> PathBuf {
    let d = client_dir();
    assert!(
        pyreal_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set PYREAL_TEST_DAT_DIR",
        d.display()
    );
    d
}

/// One recorded `0x00CD`, joined to whatever the corpus said about the item and the recipient.
#[derive(Debug, Clone)]
struct RecordedGive {
    session: &'static str,
    target: ObjectId,
    item: ObjectId,
    amount: u32,
    target_type: Option<u32>,
    item_stack: Option<u16>,
    /// The whole recorded body, for a byte-for-byte comparison.
    body: Vec<u8>,
}

fn recorded_gives(session: &'static str) -> Vec<RecordedGive> {
    let bl = &Corpus::shared(session).blobs;
    assert!(!bl.is_empty(), "{session} reassembled to nothing");
    let mut types: BTreeMap<ObjectId, u32> = BTreeMap::new();
    let mut stacks: BTreeMap<ObjectId, u16> = BTreeMap::new();
    let mut out = Vec::new();
    for b in bl.iter().filter(|b| b.payload.len() >= 4) {
        let c2s = b.dir == Direction::ClientToServer;
        let opcode = u32_at(&b.payload, 0);
        if !c2s && opcode == OP_CREATE_OBJECT {
            if let Ok(m) = pyreal_protocol::read_body_padded::<
                pyreal_protocol::objects::ItemCreateObject,
            >(&b.payload[4..])
            {
                types.insert(m.0.id, m.0.wdesc.obj_type);
                if let Some(s) = m.0.wdesc.stack_size {
                    stacks.insert(m.0.id, s);
                }
            }
        }
        if c2s
            && opcode == OP_GAME_ACTION
            && b.payload.len() >= 24
            && u32_at(&b.payload, 8) == AC_GIVE_OBJECT_REQUEST
        {
            let body = b.payload[12..24].to_vec();
            out.push(RecordedGive {
                session,
                target: ObjectId(u32_at(&body, 0)),
                item: ObjectId(u32_at(&body, 4)),
                amount: u32_at(&body, 8),
                target_type: None,
                item_stack: None,
                body,
            });
        }
    }
    for g in &mut out {
        g.target_type = types.get(&g.target).copied();
        g.item_stack = stacks.get(&g.item).copied();
    }
    out
}

fn all_recorded_gives() -> Vec<RecordedGive> {
    // Every recording the index names, so the give census has the whole denominator.
    pyreal_client_net::client_session::testing::session_names()
        .iter()
        .flat_map(|s| recorded_gives(s))
        .collect()
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

// =============================================================================================
// The light host — `Interaction` over a loopback session, with no `WorldScene`.
//
// `use_time`'s step 3 needs a loaded scene to sweep, so the pick's *answer* is delivered here the
// way the client delivers it: through the smart-box object-found notice and into
// `on_world_object_found`. What this host can therefore see is whether the pick was **armed** —
// `WorldPicker::looking_for_object`, not a counter.
// =============================================================================================

struct Host {
    store: pyreal_dat::RetailDatStore,
    objects: ObjectStream,
    net: ClientNetwork,
    inter: pyreal_client_runtime::interaction::Interaction,
    player: ObjectId,
    clock: f64,
    /// What the physics lookup for the current player returns; `None` is its null. Built by [`Host::embody`] only where the ground leg is the subject.
    body: Option<pyreal_client_runtime::character::Character>,
}

impl Host {
    fn new(session: &str) -> Self {
        let records = shared_session(session);
        let mut net = ClientNetwork::new(
            "127.0.0.1:19400",
            7304,
            "world-drop-give",
            "world-drop-give",
            connection_sequence_number(records).expect("the capture has no LoginRequest"),
        )
        .expect("host");
        let mut objects = ObjectStream::new();
        let mut events = Vec::new();
        let mut entered = false;
        for r in records {
            let now = LocalTime(r.t);
            if !r.c2s {
                net.feed(&r.raw, r.peer(), now);
            }
            net.tick(now);
            let _ = net.take_outgoing();
            for e in objects.pump(&mut net, now) {
                if let SessionEvent::CharacterSet(set) = &e {
                    if !entered {
                        if let Some(c) = set.characters.first() {
                            let account = set.account.clone();
                            net.enter_world(c.gid, &account);
                            entered = true;
                        }
                    }
                }
                events.push(e);
            }
        }
        let player = events
            .iter()
            .find_map(|e| match e {
                SessionEvent::PlayerCreated(id) => Some(*id),
                _ => None,
            })
            .expect("the capture's 0xF746 Login_CreatePlayer");
        objects.world.player = Some(player);
        objects.world.tables.inventories.insert(
            player,
            pyreal_client_model::objects::ObjectInventory::new(player),
        );
        let mut pw = pyreal_client_model::Weenie::new(player);
        pw.pwd.items_capacity = Some(0xFF);
        pw.pwd.containers_capacity = Some(0xFF);
        objects.world.tables.weenies.insert(player, pw);
        let store = pyreal_dat::RetailDatStore::open_dir(&dats()).expect("the retail dats open");
        let mut host = Self {
            store,
            objects,
            net,
            inter: pyreal_client_runtime::interaction::Interaction::new(),
            player,
            clock: 1.0,
            body: None,
        };
        // One frame before any gesture: the client's renderer already has viewport width and
        // height from start-up, while `use_time` writes this build's copy. The release step does
        // not drive a whole frame, so the viewport is primed here.
        let _ = host.drive();
        host
    }

    /// Give the client a real body, standing on Holtburg's terrain under `pyreal-physics`.
    ///
    /// `Character::on_ground` is read from the physics object the settle loop put
    /// there; nothing here writes `player_on_ground`.
    fn embody(&mut self) {
        let store = std::sync::Arc::new(
            pyreal_dat::RetailDatStore::open_dir(&dats()).expect("the retail dats open"),
        );
        let region = pyreal_world_data::landblock::load_region(&store).expect("the region decodes");
        let mut c = pyreal_client_runtime::character::Character::new(
            &store,
            &region,
            pyreal_world_data::landblock::DEFAULT_LANDBLOCK,
            (96.0, 96.0),
        )
        .expect("the character is created");
        for i in 1..=60 {
            c.update(LocalTime(f64::from(i) / 30.0));
        }
        assert!(
            c.on_ground(),
            "the body must settle before the ground leg is measured"
        );
        self.body = Some(c);
    }

    /// An item in the player's pack, ghosted the way a begun drag leaves it.
    fn carry(&mut self, id: ObjectId, stack: u16) -> ObjectId {
        let mut w = pyreal_client_model::Weenie::new(id);
        w.pwd.container_id = Some(self.player);
        w.pwd.stack_size = Some(stack);
        w.pwd.max_stack_size = Some(stack.max(1));
        w.pwd.name = format!("thing {:X}", id.0);
        w.waiting = true;
        w.determine_position_state();
        self.objects.world.tables.weenies.insert(id, w);
        assert!(
            self.objects.world.is_owned_by_player(id),
            "the fixture must be carried"
        );
        id
    }

    /// A loose object in the world with a given item type. Not owned by the player.
    fn world_object(&mut self, id: ObjectId, obj_type: u32, name: &str) -> ObjectId {
        let mut w = pyreal_client_model::Weenie::new(id);
        w.pwd.obj_type = obj_type;
        w.pwd.name = name.to_string();
        self.objects.world.tables.weenies.insert(id, w);
        id
    }

    fn drive(&mut self) -> Vec<(u32, Vec<u8>)> {
        self.clock += 1.0;
        let now = self.clock;
        let _ = interaction::use_time(
            &mut self.inter,
            &self.store,
            None,
            &mut self.objects,
            Some(&mut self.net),
            Vec::new(),
            false,
            (1024, 768),
            LocalTime(now),
        );
        self.net.tick(LocalTime(now));
        assert_eq!(
            self.inter.stats.requests_undeliverable, 0,
            "the session refused to encode"
        );
        let mut out = Vec::new();
        for (raw, _to) in self.net.take_outgoing() {
            let Ok(p) = ParsedPacket::parse(&raw) else {
                continue;
            };
            for f in &p.fragments {
                if f.payload.len() < 12 || u32_at(&f.payload, 0) != OP_GAME_ACTION {
                    continue;
                }
                out.push((u32_at(&f.payload, 8), f.payload[12..].to_vec()));
            }
        }
        out
    }

    /// **Step 1** — the release over the viewport. Nothing goes on the wire; what must happen is
    /// that the world-object lookup is *called*.
    ///
    /// The measure is `WorldPicker::stats.requests`, which is incremented **inside**
    /// `find_object`, past its unsigned viewport compare. `Interaction::stats.picks_requested` is
    /// deliberately not used: it counts the request whether or not a pick was armed.
    ///
    /// This stops at drop-release handling (the client's frame step 2) rather than driving a
    /// whole `use_time` frame: drawing raises the smart-box object-found notice **outside** its
    /// player-present guard, so a whole frame answers the pick with id 0 and clears the reason, and
    /// the draw tail clears `looking_for_object` either way. Stopping here lets the test read
    /// `looking_for_object` and the drop reason while they are up.
    fn release_over_the_world(&mut self, item: ObjectId) {
        let _ = self.net.take_outgoing();
        let armed = self.inter.pick.stats.requests;
        self.inter.queue(
            Vec::new(),
            vec![UiRequest::DragDrop {
                item,
                target: DropTarget::World,
            }],
        );
        self.clock += 1.0;
        let unowned =
            self.inter
                .run_ui_requests(&mut self.objects.world, false, ServerTime(self.clock));
        assert!(
            unowned.is_empty(),
            "the world drop is this file's request to own"
        );
        assert_eq!(
            self.inter.pick.stats.requests,
            armed + 1,
            "the arm must call world-object lookup -- this counter lives inside it"
        );
        assert_eq!(
            self.inter.pick.stats.outside_viewport, 0,
            "and the point must pass the viewport compare"
        );
        assert!(
            self.inter.pick.looking_for_object(),
            "and leave object lookup pending for the same-frame draw/pick pass to answer"
        );
        assert_eq!(
            self.inter.search_reason(),
            SearchReason::Drop,
            "and park the drop search reason until the notice answers"
        );
        assert!(
            self.inter.pending_requests().is_empty(),
            "nothing is built until the pick answers: {:?}",
            self.inter.pending_requests()
        );
    }

    /// **Step 2** — the smart-box object-found notice, which drawing raises with the clicked
    /// object id (zero when the ray hit nothing).
    ///
    /// `note_player_physics` first, because `use_time` runs it first: it retrieves the player
    /// physics object, which the ground-placement leg consults. `None` — no body —
    /// is the client's null pointer and takes the
    /// mid-air branch, so a host that wants the ground leg calls [`Host::embody`] first.
    fn pick_finds(&mut self, found: ObjectId) -> Vec<(u32, Vec<u8>)> {
        let _ = self.net.take_outgoing();
        self.clock += 1.0;
        self.inter.note_player_physics(self.body.as_ref());
        self.inter
            .on_world_object_found(found, &mut self.objects.world, ServerTime(self.clock));
        self.drive()
    }

    fn drop_onto(&mut self, item: ObjectId, found: ObjectId) -> Vec<(u32, Vec<u8>)> {
        self.release_over_the_world(item);
        self.pick_finds(found)
    }

    /// A left press in the middle of the viewport, arriving with the hit test's own answer —
    /// `Some(<SBOX>)`, which is what `hit_test_screen` returns now that the wrapper is
    /// mouse-visible. Returns how many world-object lookups it armed: mouse-down action 7 is gated
    /// on the search reason being below Examine (3), so a latched Drop (5) makes this **0**.
    ///
    /// The notice's own tail clears the reason in the same frame, so the gate is read **where the
    /// gate is**: mouse-down action 7 is driven apart from the draw path, the Select reason is
    /// asserted while it is up, and the frame is then completed so the notice runs.
    fn viewport_left_press(&mut self) -> u64 {
        let armed = self.inter.pick.stats.requests;
        // The smart-box mouse-down handler's action 7, alone — step 1.
        self.inter.wrapper_mouse(
            UiMouseEvent {
                action: pyreal_client_contract::actions::ui::PRIMARY_CLICK,
                start: true,
                x: 512,
                y: 384,
                over: Some(SMART_BOX),
            },
            (1024, 768),
            true,
        );
        let armed_here = self.inter.pick.stats.requests - armed;
        if armed_here > 0 {
            assert_eq!(
                self.inter.search_reason(),
                SearchReason::Select,
                "the search reason was below examine and target mode was NONE, so selection armed -- read \
                 while it is up, because the notice clears it in the same frame"
            );
        }
        // ...and the rest of the frame, whose step 3 answers the pick and clears the reason.
        let _ = self.drive();
        armed_here
    }

    fn lines(&self) -> Vec<(u32, String)> {
        self.objects
            .world
            .scroll
            .pending()
            .iter()
            .map(|f| (f.chat_type, f.body.clone()))
            .collect()
    }

    fn waiting(&self, item: ObjectId) -> bool {
        self.objects.world.weenie(item).expect("seeded").waiting
    }
}

// =============================================================================================
// The heavy host — the real `UiSystem` the retail dats build, for the hit test.
// =============================================================================================

// =============================================================================================
// 1. The give, in two steps, with the wire and the local state asserted apart.
// =============================================================================================

/// An item released over the viewport and picked onto a creature raises
/// `0x00CD` — and the pick that carries it is really armed.
#[test]
fn a_drop_onto_an_npc_arms_the_pick_and_then_raises_the_give() {
    let mut host = Host::new("early-inventory-and-casting");
    let item = host.carry(ObjectId(0x2881_0001), 1);
    let npc = host.world_object(ObjectId(0x2881_0002), item_type::CREATURE, "Aun Tanua");

    // Step 1 asserts inside `release_over_the_world`: `looking_for_object`, the drop reason,
    // silence.
    host.release_over_the_world(item);
    assert!(
        host.objects.world.request_lock.is_idle(),
        "the drop-to-pick hop takes no inventory lock: `attempt_place_in_3d` only *reads* it, and \
         nothing between the drop and the pick records one"
    );

    // Step 2: the notice.
    let sent = host.pick_finds(npc);
    assert_eq!(sent.len(), 1, "one request, and it is the give");
    assert_eq!(sent[0].0, AC_GIVE_OBJECT_REQUEST, "0x00CD");
    assert_eq!(AC_GIVE_OBJECT_REQUEST, 205, "0x00CD, pinned as a literal");
    assert_eq!(
        ObjectId(u32_at(&sent[0].1, 0)),
        npc,
        "field 0 is the recipient"
    );
    assert_eq!(ObjectId(u32_at(&sent[0].1, 4)), item, "field 1 is the item");
    assert_eq!(u32_at(&sent[0].1, 8), 1, "field 2 is object split size");
    assert_eq!(sent[0].1.len(), 12, "0x00CD is three dwords");

    // Local state, asserted apart from the wire.
    assert_eq!(
        host.search_reason_now(),
        SearchReason::None,
        "the notice tail clears the reason"
    );
    assert!(
        host.waiting(item),
        "the give request marks the item as waiting"
    );
    assert_eq!(
        host.objects.world.request_lock.pending,
        InventoryRequest::Give,
        "and now the lock is held, by the give attempt's own request recording"
    );
    assert!(host.lines().is_empty(), "a successful give says nothing");
}

impl Host {
    fn search_reason_now(&self) -> SearchReason {
        self.inter.search_reason()
    }
}

///
/// **The corpus's own recorded `0x00CD`s, reproduced through the whole gesture.**
///
/// The recorded gives are counted from all seven recordings here; the count is printed, and the
/// assertion is on the bytes, not on the count.
#[test]
fn every_recorded_give_is_reproduced_byte_for_byte_by_the_whole_gesture() {
    let gives = all_recorded_gives();
    assert!(
        !gives.is_empty(),
        "the corpus must contain at least one recorded 0x00CD or this test proves nothing"
    );
    let mut per_session: BTreeMap<&str, usize> = BTreeMap::new();
    let mut typed = 0usize;
    for (i, g) in gives.iter().enumerate() {
        *per_session.entry(g.session).or_default() += 1;
        if let Some(t) = g.target_type {
            assert_eq!(
                t,
                item_type::CREATURE,
                "give {i} in {}: the recipient's recorded item type must be exactly TYPE_CREATURE",
                g.session
            );
            typed += 1;
        }
        let mut host = Host::new("early-inventory-and-casting");
        let stack = g
            .item_stack
            .unwrap_or_else(|| u16::try_from(g.amount).expect("recorded amounts are small"));
        let item = host.carry(g.item, stack);
        let npc = host.world_object(g.target, item_type::CREATURE, "recipient");
        let sent = host.drop_onto(item, npc);
        assert_eq!(sent.len(), 1, "give {i} in {}", g.session);
        assert_eq!(
            sent[0].0, AC_GIVE_OBJECT_REQUEST,
            "give {i} in {}",
            g.session
        );
        assert_eq!(
            sent[0].1, g.body,
            "give {i} in {}: the client's twelve bytes must be the recorded twelve",
            g.session
        );
    }
    eprintln!(
        "give census: {} recorded 0x00CD {per_session:?}; {typed} recipients typed by the \
         corpus, {} never created in it",
        gives.len(),
        gives.len() - typed
    );
}

/// **The intermediate, asserted where the whole frame cannot see it.**
///
/// `use_time` runs drop release (step 2) and the draw path's tail (step 3) in the **same**
/// frame, and step 3 clears the selection cursor whether or not it swept anything — so at the end
/// of the frame `looking_for_object` is false either way, and a harness that looks only there
/// cannot tell an armed pick from an unarmed one (`stats.picks_requested` rises either way). Here
/// the two steps are driven apart, so the flag is read while it is up.
///
/// One frame is driven first because the client has viewport dimensions before any gesture,
/// while `use_time` writes this build's copy.
#[test]
fn the_world_drop_arm_arms_the_pick_before_the_frame_reads_it() {
    let mut host = Host::new("early-inventory-and-casting");
    let item = host.carry(ObjectId(0x2881_0040), 1);
    let _ = host.drive();
    assert!(
        !host.inter.pick.looking_for_object(),
        "nothing is armed to start with"
    );

    host.inter.queue(
        Vec::new(),
        vec![UiRequest::DragDrop {
            item,
            target: DropTarget::World,
        }],
    );
    host.clock += 1.0;
    // The smart-box element-message `0x15` arm alone: drop release without the later pick result.
    let unowned =
        host.inter
            .run_ui_requests(&mut host.objects.world, false, ServerTime(host.clock));
    assert!(
        unowned.is_empty(),
        "the world drop is this file's request to own"
    );
    assert!(
        host.inter.pick.looking_for_object(),
        "`world-object lookup` sets `looking_for_object`, and `use_time`'s step 3 is gated on it"
    );
    assert_eq!(host.inter.search_reason(), SearchReason::Drop);
    assert_eq!(
        host.inter.pick.click_object(),
        (ObjectId(0), -1),
        "a new request clears the last"
    );
}

/// **The drop reads the cursor, not a constant.** Drop release reads the input manager's
/// current mouse coordinates because element message `0x15`
/// carries none.
///
/// Driven with the pointer parked outside a 1024x768 viewport, which is the one input that tells a
/// cursor read from a hard-coded `(0, 0)`: both are "inside" for every ordinary drop.
/// The middle button is used to move the pointer because the smart-box mouse table has no
/// action 9, so the event sets the input manager's position and arms nothing.
///
/// It also pins what the client does on that leg, which is **nothing**: world-object lookup
/// returns false, no notice is raised, and the search reason stays Drop. The client latches here
/// too; the leg is unreachable in retail (the wrapper's box is inside the render viewport) and
/// unreachable here for the same reason, and it is counted rather than papered over so that a
/// layout which makes it reachable shows up as a number.
#[test]
fn the_drop_takes_its_point_from_the_cursor_and_not_from_a_constant() {
    let mut host = Host::new("early-inventory-and-casting");
    let item = host.carry(ObjectId(0x2881_0050), 1);
    let _ = host.net.take_outgoing();
    host.inter.queue(
        vec![UiMouseEvent {
            action: 9,
            start: true,
            x: 2000,
            y: 3000,
            over: Some(SMART_BOX),
        }],
        vec![UiRequest::DragDrop {
            item,
            target: DropTarget::World,
        }],
    );
    let sent = host.drive();

    assert!(
        sent.is_empty(),
        "nothing on the wire: no pick, so no notice, so no 3D placement attempt"
    );
    assert_eq!(
        host.inter.pick.stats.outside_viewport, 1,
        "the unsigned compare in world-object lookup rejected (2000, 3000) -- with the point read from a constant (0, 0) it would have passed"
    );
    assert_eq!(host.inter.pick.stats.requests, 0, "and nothing was armed");
    assert_eq!(host.inter.stats.drops_outside_the_viewport, 1);
    assert_eq!(
        host.inter.search_reason(),
        SearchReason::Drop,
        "the client does nothing about this leg either, and neither does this build"
    );
}

// =============================================================================================
// 2. The latch, in two steps — the half a single-drop test cannot see.
// =============================================================================================

/// **After a completed give, the next viewport click must still arm a pick.**
///
/// Drop is search reason 5. Mouse-down action 7 requires a reason below Examine (3), and action
/// 10 one below Use (4), so a latched Drop makes both false for ever. Two steps, because a
/// single-drop test sees only the drop: step 1 is the drop, step 2 is a click *after* it, and
/// only the pair can tell "the drop worked" from "the drop worked and left the viewport dead".
#[test]
fn a_viewport_click_after_a_completed_give_still_arms_a_pick() {
    let mut host = Host::new("early-inventory-and-casting");
    let item = host.carry(ObjectId(0x2881_0010), 1);
    let npc = host.world_object(ObjectId(0x2881_0011), item_type::CREATURE, "Ulgrim");

    // Step 1 — the drop, all the way through.
    let sent = host.drop_onto(item, npc);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, AC_GIVE_OBJECT_REQUEST);
    assert_eq!(
        host.search_reason_now(),
        SearchReason::None,
        "the gesture ended"
    );

    // Step 2 — an ordinary left press in the viewport, the very next frame.
    assert_eq!(
        host.viewport_left_press(),
        1,
        "the click must reach `world-object lookup` -- with drop search reason 5 latched, the below-examine gate is false and this is 0"
    );
    assert_eq!(
        host.search_reason_now(),
        SearchReason::None,
        "and the click's own gesture ended: the same-frame draw answers the armed pick in the same \
         frame and its notice tail clears the reason. A latched drop reason would still read `Drop` \
         here, because no pick means no notice means nothing clears it -- which is what this \
         discriminates against. The selection reason the gate produced is asserted inside \
         `viewport_left_press`, where it is up."
    );
}

/// **The same, after a drop the game *refused*** — because the failure path is the one a player
/// hits by accident, and it must clear the reason for the same reason the success path does.
///
/// Ground placement's second gate rejects an item the player does not own with
/// *"You must first pick up the %s"* and answers the failure with
/// clearing the waiting state. The reason is still cleared, because the tail that clears it runs
/// whatever the 3D placement attempt returned.
#[test]
fn a_viewport_click_after_a_refused_drop_still_arms_a_pick() {
    let mut host = Host::new("early-inventory-and-casting");
    // Not carried: a loose thing in the world, which the drop arm refuses.
    let item = host.world_object(ObjectId(0x2881_0020), 0, "Pyreal");
    if let Some(w) = host.objects.world.tables.weenies.get_mut(item) {
        w.waiting = true;
    }
    let npc = host.world_object(ObjectId(0x2881_0021), item_type::CREATURE, "Ulgrim");

    let sent = host.drop_onto(item, npc);
    assert!(
        sent.is_empty(),
        "an unowned item is refused before any sender runs"
    );
    assert_eq!(
        host.lines(),
        vec![(
            FEEDBACK_CHANNEL,
            "You must first pick up the Pyreal".to_string()
        )],
        "an unowned item is refused with the pick-up feedback"
    );
    assert!(
        !host.waiting(item),
        "clearing the refused drop's waiting state un-ghosts a refused drop"
    );
    assert!(
        host.objects.world.request_lock.is_idle(),
        "a refused drop leaves the inventory lock idle -- no sender ran, so no request was recorded"
    );
    assert_eq!(
        host.search_reason_now(),
        SearchReason::None,
        "and the gesture still ended"
    );

    assert_eq!(
        host.viewport_left_press(),
        1,
        "the refused drop did not disable the viewport"
    );
    assert_eq!(
        host.search_reason_now(),
        SearchReason::None,
        "and that click's gesture ended too"
    );
}

/// The same actual world-drop refusal uses the material-aware display-name lookup's
/// appropriate name. This keeps the real pick, 3D placement attempt, notice, waiting reset and
/// no-request route above; only the object's public material field differs.
#[test]
fn a_material_bearing_refused_world_drop_uses_the_display_name() {
    let mut host = Host::new("early-inventory-and-casting");
    let mapper_id = pyreal_client_runtime::hud::MATERIAL_TYPE_NAMES;
    let bytes = host
        .store
        .read(mapper_id)
        .expect("the retail material mapper");
    let mapper =
        <pyreal_assets::DidMapper as pyreal_assets::Decode>::decode_payload(mapper_id, &bytes)
            .expect("the retail material mapper decodes");
    let material_names = mapper
        .enum_to_name
        .iter()
        .filter_map(|(id, _)| {
            pyreal_client_runtime::hud::material_name_of(Some(&mapper), *id).map(|name| (*id, name))
        })
        .collect();
    host.objects.world.install_material_names(material_names);
    assert_eq!(host.objects.world.material_name(0x3A), Some("Bronze"));

    let item = host.world_object(ObjectId(0x2881_0022), 0, "Door");
    let object = host
        .objects
        .world
        .weenie_mut(item)
        .expect("the seeded loose object");
    object.pwd.material_type = Some(0x3A);
    object.waiting = true;
    let npc = host.world_object(ObjectId(0x2881_0023), item_type::CREATURE, "Ulgrim");

    let sent = host.drop_onto(item, npc);
    assert!(
        sent.is_empty(),
        "an unowned item is refused before any sender runs"
    );
    assert_eq!(
        host.lines(),
        vec![(FEEDBACK_CHANNEL, "You must first pick up the Bronze Door".to_string())],
        "3D placement uses the appropriate material-aware display name without the player-backpack alias"
    );
    assert!(!host.waiting(item), "the refused drop still clears waiting");
    assert!(host.objects.world.request_lock.is_idle());
    assert_eq!(host.search_reason_now(), SearchReason::None);
}

///
/// **The discriminating negative.** A pick that finds *nothing* is a ground drop, not a give — so
/// "the route is open" cannot be read as "everything down it is a give", and the drop arm's
/// `found ? id : 0` fork is exercised in both directions.
#[test]
fn a_drop_onto_empty_air_is_a_ground_drop_and_the_reason_still_clears() {
    let mut host = Host::new("early-inventory-and-casting");
    // The ground leg is the one arm of 3D placement that consults the
    // player's physics, so it needs a real body under it.
    host.embody();
    let item = host.carry(ObjectId(0x2881_0030), 1);
    host.release_over_the_world(item);
    let sent = host.pick_finds(ObjectId(0));
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].0, AC_DROP_ITEM,
        "0x001B Inventory_DropItem, not the give"
    );
    assert_eq!(host.search_reason_now(), SearchReason::None);

    assert_eq!(
        host.viewport_left_press(),
        1,
        "and the viewport still answers a click"
    );
    assert_eq!(
        host.search_reason_now(),
        SearchReason::None,
        "and that click's gesture ended too"
    );
}
