//! The icon-composition bench for the dat tier's `inventory::icon_composite`: every recording
//! replayed through the object stream and the HUD, at the instant its world holds the most objects.

#![allow(dead_code)]

use pyreal_client_net::client_session::testing::{session_names, shared_session};
use pyreal_client_net::client_session::SessionEvent;
use pyreal_client_net::recording::connection_sequence_number;
use pyreal_client_runtime::net::ClientNetwork;
use pyreal_client_runtime::objects::ObjectStream;
use pyreal_client_shell::hud::Hud;
use pyreal_primitives::LocalTime;

// =================================================================================================
// The capture reader.
// =================================================================================================

/// Every recording the corpus index names, in name order.
pub(crate) fn corpus_sessions() -> Vec<String> {
    let mut out: Vec<String> = session_names().iter().map(|s| (*s).to_owned()).collect();
    out.sort();
    out
}

pub(crate) fn replay_to(session: &str, limit: usize) -> (ObjectStream, Vec<SessionEvent>) {
    let records = shared_session(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).expect("the capture has no LoginRequest"),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut entered = false;
    let mut events = Vec::new();
    for r in records.iter().take(limit) {
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
    (objects, events)
}

/// Return the replay limit just after the first maximum object population. The end may be
/// emptied by logoff, so sample the populated instant without assuming every recording logs off.
pub(crate) fn busiest_instant(session: &str) -> usize {
    let records = shared_session(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).expect("the capture has no LoginRequest"),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut entered = false;
    let mut best = (0usize, 1usize);
    for (i, r) in records.iter().enumerate() {
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
        }
        let n = objects.world.tables.weenies.len();
        if n > best.0 {
            best = (n, i + 1);
        }
    }
    best.1
}

pub(crate) fn scene(session: &str) -> (ObjectStream, Hud) {
    let (mut objects, events) = replay_to(session, busiest_instant(session));
    let mut hud = Hud::new();
    // Loading tables is required: Hud::build_spells returns an empty book without its spell
    // table. Omitting this producer would make later spell checks vacuous; their nonempty-book
    // denominator must accompany the data join.
    hud.load_tables(&open_store(), &objects.world);
    let _ = hud.apply_events(&events, &mut objects.world);
    hud.sync(&objects, None);
    (objects, hud)
}

pub(crate) fn open_store() -> pyreal_dat::RetailDatStore {
    pyreal_dat::testing::open_store_or_fail()
}
