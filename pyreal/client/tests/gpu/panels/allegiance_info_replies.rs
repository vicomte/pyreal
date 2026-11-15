//! Allegiance replies: an allegiance info response (`0x027C`) for the player prints the report and
//! leaves the panel alone, and one for a stranger prints nothing.
//! Fixture: `net::late_receivers`' socket-free replay App and production-encoded game events.

#![cfg(gpu)]

use crate::common::gpu_lock;
use crate::net::late_receivers::{chat, settle, setup};

use pyreal_client_runtime::dropped;
use pyreal_primitives::ObjectId;
use pyreal_protocol::social as wire;
use pyreal_protocol::Opcode;

/// One allegiance member.
fn member(id: u32, name: &str, rank: u32, online: bool) -> wire::AllegianceData {
    wire::AllegianceData {
        id: ObjectId(id),
        name: name.into(),
        gender: 1,
        heritage_group: 1, // Aluvian
        rank,
        level: 10 * rank,
        bitfield: u32::from(online) * wire::AllegianceData::LOGGED_IN,
        cp_tithed: 0,
        cp_cached: 0,
        loyalty: 0,
        leadership: 0,
        time_online: 0,
        allegiance_age: 0,
    }
}

/// Every version-11 block is present. [`wire::AllegianceHierarchy::write`] emits a block only when
/// its `Option` is `Some`, while `read` is driven purely by the version word.
fn version_eleven(
    members: Vec<(Option<ObjectId>, wire::AllegianceData)>,
) -> wire::AllegianceHierarchy {
    wire::AllegianceHierarchy {
        version: wire::allegiance_version::APPROVED_VASSAL,
        officers: Some(pyreal_protocol::archive::PHash::new(Vec::new())),
        officer_titles: Some(Vec::new()),
        pools: Some(wire::AllegiancePools::default()),
        motd: Some((String::new(), String::new())),
        chat_room_id: Some(0),
        bind_point: Some(pyreal_protocol::types::PositionWire::default()),
        allegiance_name: Some(("The Hand of Dereth".to_owned(), 0)),
        is_locked: Some(0),
        approved_vassal: Some(0),
        old_officer: None,
        members,
    }
}

///
/// **`0x027C` prints a chat report and leaves the allegiance panel alone.**
///
/// The response writes five chat lines with arguments
/// `(text, 0, true, 0)`; it never touches the panel or the client's own allegiance hierarchy. It
/// is `/allegiance info <name>`'s answer.
///
/// The tree:
/// monarch Bob (online) -> the target Cid (offline) -> vassals Dee (online, added first) and Eve
/// (offline, added second, so head insertion puts it first).
///
/// **Falsified by** making the arm unreachable: no lines, and `dropped::unreceived` goes true.
#[test]
fn an_allegiance_info_response_prints_the_report_and_leaves_the_panel_alone() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("alleg-info");

    // The client's own tree, so the station can prove `0x027C` did not disturb it.
    peer.event(
        &mut app,
        &wire::AllegianceUpdate {
            rank: 1,
            profile: wire::AllegianceProfile {
                total_members: 1,
                total_vassals: 0,
                hierarchy: version_eleven(vec![(None, member(77, "Solo", 1, true))]),
            },
        },
    );
    settle(&mut app);
    // `AllegianceHierarchy` has no `PartialEq`, so the snapshot is its `Debug` rendering -- which
    // is a stronger comparison than a field-by-field one, not a weaker: every field is in it.
    let tree_before = format!("{:?}", app.objects().world.allegiance);
    assert_eq!(
        app.objects().world.allegiance.total,
        1,
        "the client's own hierarchy, one member"
    );
    let before = chat(&mut app).len();

    peer.event(
        &mut app,
        &wire::AllegianceInfoResponse {
            target: ObjectId(5),
            profile: wire::AllegianceProfile {
                total_members: 4,
                total_vassals: 2,
                hierarchy: version_eleven(vec![
                    (None, member(9, "Bob", 9, true)),
                    (Some(ObjectId(9)), member(5, "Cid", 5, false)),
                    (Some(ObjectId(5)), member(10, "Dee", 1, true)),
                    (Some(ObjectId(5)), member(11, "Eve", 1, false)),
                ]),
            },
        },
    );
    settle(&mut app);

    let s = &app.interaction().stats;
    assert_eq!(s.allegiance_info_responses, 1, "one message consumed");
    assert_eq!(
        s.allegiance_info_reports, 1,
        "the target lookup found the member, so a report went out"
    );

    // The log stores an appended run and the newline that ends it as separate entries, so the
    // lines are the non-empty ones.
    let lines = chat(&mut app);
    let new: Vec<String> = lines[before..]
        .iter()
        .filter(|l| !l.trim().is_empty())
        .cloned()
        .collect();
    assert_eq!(
        new.len(),
        6,
        "the note, the header, the patron row, the Vassals label and two vassal rows: {new:?}"
    );
    assert!(
        new[0].starts_with("Note: An asterisk (*) indicates"),
        "{:?}",
        new[0]
    );
    // The full name is `title + \" \" + name`, so the rank title is part of the observable.
    assert!(
        new[1].starts_with("Allegiance information for "),
        "{:?}",
        new[1]
    );
    assert!(
        new[1].contains("Cid"),
        "the target, not the monarch: {:?}",
        new[1]
    );
    assert!(
        !new[1].contains('*'),
        "Cid is offline, so no asterisk: {:?}",
        new[1]
    );
    assert!(
        new[2].starts_with("Patron:"),
        "three leading spaces, trimmed by the chat display: {:?}",
        new[2]
    );
    assert!(new[2].contains("Bob"), "{:?}", new[2]);
    assert!(new[2].contains('*'), "Bob is online: {:?}", new[2]);
    assert!(new[3].starts_with("Vassals:"), "{:?}", new[3]);
    assert!(
        new[4].contains("Eve"),
        "head insertion puts Eve first: {:?}",
        new[4]
    );
    assert!(new[5].contains("Dee"), "{:?}", new[5]);
    assert!(new[5].contains('*'), "Dee is online: {:?}", new[5]);
    // The full-name formatter joins the rank title, one space, and the name, so the title is part
    // of the observable and a bare name would be a different line.
    assert_eq!(
        new[1], "Allegiance information for Thane Cid:",
        "rank 5 is Thane"
    );
    assert_eq!(new[2], "Patron: King Bob *", "rank 9 is King");

    // The three things a "panel" reading would have moved.
    assert_eq!(
        format!("{:?}", app.objects().world.allegiance),
        tree_before,
        "the response profile is processed without changing the client's allegiance tree"
    );
    assert_eq!(app.objects().world.allegiance_aborts, 0);
    assert!(!dropped::unreceived(
        Opcode::ALLEGIANCE_ALLEGIANCE_INFO_RESPONSE_EVENT
    ));
}

/// **A `0x027C` whose profile does not contain the target prints nothing at all.**
///
/// A failed target lookup is the handler's one guard and returns without output. It takes the
/// asterisk note with it, which is why that note is *inside* the branch rather than before it.
///
/// **Falsified by** dropping the `look_up` guard: the note and a header for an empty name go out.
#[test]
fn an_allegiance_info_response_for_a_stranger_prints_nothing() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("alleg-info-miss");
    let before = chat(&mut app).len();

    peer.event(
        &mut app,
        &wire::AllegianceInfoResponse {
            target: ObjectId(0xBEEF),
            profile: wire::AllegianceProfile {
                total_members: 1,
                total_vassals: 0,
                hierarchy: version_eleven(vec![(None, member(9, "Bob", 9, true))]),
            },
        },
    );
    settle(&mut app);

    let s = &app.interaction().stats;
    assert_eq!(s.allegiance_info_responses, 1, "the message arrived");
    assert_eq!(
        s.allegiance_info_reports, 0,
        "and the target lookup rejected it"
    );
    assert_eq!(chat(&mut app).len(), before, "not even the asterisk note");
}
