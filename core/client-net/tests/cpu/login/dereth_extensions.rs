//! Contracts for Dereth's extensions handshake (`0xF7EF`).
//! Fixture: synthetic messages; the bytes are the server's own encoding (`pyreal_protocol::extensions`).

use crate::common::session_fixture::*;
use pyreal_client_net::client_session::testing::MockTransport;
use pyreal_client_net::client_session::Session;
use pyreal_protocol::extensions::{DerethExtensions, ADVOCATE_OFFERS, AI_FELLOWS, QUEST_TIMERS};

fn character_select(s: &mut Session<MockTransport>) {
    s.transport
        .deliver_blob(NetQueue::UiQueue, &write_blob(&character_set()).unwrap());
    s.tick(t(0.0));
    s.transport
        .deliver_blob(NetQueue::UiQueue, &write_blob(&character_set()).unwrap());
    s.tick(t(1.0));
}

#[test]
fn a_session_offering_extensions_names_them_once_on_queue_5_before_entering() {
    let mut s = session();
    s.offer_extensions(true);
    character_select(&mut s);
    assert_eq!(
        s.transport.sent_opcodes(),
        vec![0xF7EF],
        "once a connection"
    );
    assert_eq!(s.transport.sent[0].queue, NetQueue::ClientCache);
    assert!(!s.transport.sent[0].ordered);
    let hello: DerethExtensions =
        pyreal_protocol::read_body(&s.transport.sent[0].payload[4..]).unwrap();
    assert_eq!(hello.names, [AI_FELLOWS, ADVOCATE_OFFERS, QUEST_TIMERS]);
}

#[test]
fn a_session_not_offering_them_sends_what_retail_sends() {
    let mut s = session();
    character_select(&mut s);
    assert!(s.transport.sent_opcodes().is_empty());
}

#[test]
fn the_world_s_answer_keeps_the_extensions_this_client_knows() {
    let mut s = session();
    s.offer_extensions(true);
    character_select(&mut s);
    assert!(s.extensions().is_empty(), "nothing until the world answers");
    let answer = DerethExtensions {
        names: vec!["someone_elses".into(), AI_FELLOWS.into()],
    };
    s.transport
        .deliver_blob(NetQueue::UiQueue, &write_blob(&answer).unwrap());
    s.tick(t(2.0));
    assert_eq!(s.extensions(), [AI_FELLOWS]);
    assert!(s
        .drain_events()
        .any(|e| matches!(e, SessionEvent::Extensions(ref n) if n == &[AI_FELLOWS])));
}
