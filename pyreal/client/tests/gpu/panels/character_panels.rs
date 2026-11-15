//! The character data the panels read: the allegiance and fellowship census asserts which
//! recordings carry `Allegiance_AllegianceUpdate 0x0020` rosters and fellowship traffic, and the
//! login description answers before the player's row exists.
//! Fixture: the recorded sessions (`first-login-walk-jump`'s `0x0013 Login_PlayerDescription` is
//! the oracle; nothing about the character is written here).

#![cfg(gpu)]

use std::collections::{BTreeMap, BTreeSet};

use pyreal_client_net::client_session::testing::{session_names, shared_session};
use pyreal_client_net::client_session::SessionEvent;
use pyreal_client_net::recording::connection_sequence_number;
use pyreal_client_runtime::net::ClientNetwork;
use pyreal_client_runtime::objects::ObjectStream;
use pyreal_primitives::LocalTime;

// ---------------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------------

/// Every recording the corpus index names, in name order.
fn corpus_sessions() -> Vec<String> {
    let mut out: Vec<String> = session_names().iter().map(|s| (*s).to_owned()).collect();
    out.sort();
    out
}
fn replay(session: &str) -> Vec<SessionEvent> {
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
    events
}

fn events_in_world(events: &[SessionEvent]) -> &[SessionEvent] {
    let seen_desc = events
        .iter()
        .position(|e| matches!(e, SessionEvent::PlayerDescription(_)))
        .expect("the capture never reached 0x0013");
    let end = events[seen_desc..]
        .iter()
        .position(|e| {
            matches!(
                e,
                SessionEvent::LoggedOff
                    | SessionEvent::StateChanged(
                        pyreal_client_net::client_session::SessionState::CharacterSelect
                            | pyreal_client_net::client_session::SessionState::Disconnected(_)
                    )
            )
        })
        .map_or(events.len(), |i| seen_desc + i);
    &events[..end]
}

/// The capture's own `0x0013` — the only oracle for what the character knows.
fn player_description(
    events: &[SessionEvent],
) -> Option<&pyreal_protocol::login::LoginPlayerDescription> {
    events.iter().find_map(|e| match e {
        SessionEvent::PlayerDescription(d) => Some(&**d),
        _ => None,
    })
}

// ---------------------------------------------------------------------------------------------
// 1. What the capture does not carry.
// ---------------------------------------------------------------------------------------------

/// **Only the `fellowship-*` recordings carry fellowship traffic; every other recording is an
/// asserted absence control.**
///
/// Every recording is replayed through the production `ClientNetwork` / `ObjectStream` path and
/// its fellowship messages counted: every `fellowship-*` recording carries `0x02BE
/// Fellowship_FullUpdate`, every other recording carries none of `0x02BE`, `0x01C9
/// FellowUpdateDone` and `0x01CA FellowStatsDone`, and no recording carries `0x01CA` at all. A new
/// recording outside the fellowship family that carries one fails here.
///
/// Allegiance is not asserted absent: the corpus carries `0x0020 Allegiance_AllegianceUpdate`,
/// and every recording that enters the world carries it. Its contents are asserted by
/// [`every_recorded_allegiance_update_decodes_and_its_member_count_agrees_with_its_roster`] below.
/// One message set is asserted absent and one present, so neither can pass by accident.
#[test]
fn only_the_fellowship_recordings_carry_fellowship_traffic() {
    use pyreal_protocol::Opcode;
    const FELLOWSHIP: [Opcode; 3] = [
        Opcode::FELLOWSHIP_FULL_UPDATE,
        Opcode::FELLOWSHIP_FELLOW_UPDATE_DONE,
        Opcode::FELLOWSHIP_FELLOW_STATS_DONE,
    ];

    // "The corpus carries no Y" is only as strong as the set it is read over, and the point of
    // asserting an absence is that a new capture turns it red, so every recording is read here.
    let mut fellowship_sessions = 0usize;
    let mut other_sessions = 0usize;
    let mut entered_sessions = 0usize;
    for session in corpus_sessions() {
        let events = replay(&session);
        let ops: BTreeSet<u32> = events
            .iter()
            .filter_map(|e| match e {
                SessionEvent::UiEvent { opcode, .. } => Some(opcode.0),
                _ => None,
            })
            .collect();
        let counts: [usize; 3] = std::array::from_fn(|i| {
            events
                .iter()
                .filter(|e| matches!(e, SessionEvent::UiEvent { opcode, .. } if *opcode == FELLOWSHIP[i]))
                .count()
        });
        if session.starts_with("fellowship-") {
            fellowship_sessions += 1;
            assert!(
                counts[0] > 0,
                "{session}: a fellowship recording carries no 0x02BE Fellowship_FullUpdate"
            );
        } else {
            other_sessions += 1;
            assert_eq!(
                counts, [0; 3],
                "{session}: (0x02BE, 0x01C9, 0x01CA) -- a recording outside the `fellowship-*` \
                 family carries fellowship traffic"
            );
        }
        assert_eq!(counts[2], 0, "{session} carries 0x01CA FellowStatsDone");
        // `0x0003 Allegiance_AllegianceUpdateAborted` is a *different* message from `0x0020` and
        // is absent; it stays asserted here rather than moving to the allegiance test, so that
        // the failure of one does not mask the other.
        assert!(
            !ops.contains(&Opcode::ALLEGIANCE_ALLEGIANCE_UPDATE_ABORTED.0),
            "{session} carries 0x0003 Allegiance_AllegianceUpdateAborted"
        );
        // The other direction of the same control: a recording whose character enters the world
        // carries the allegiance message this test does not claim is absent, and a recording that
        // never enters (login-only) carries no ordered event at all, so an empty histogram is never
        // why an absence above passed.
        let entered = events
            .iter()
            .any(|e| matches!(e, SessionEvent::PlayerDescription(_)));
        if entered {
            entered_sessions += 1;
            assert!(
                ops.contains(&Opcode::ALLEGIANCE_ALLEGIANCE_UPDATE.0),
                "{session} enters the world and carries no 0x0020 Allegiance_AllegianceUpdate"
            );
        } else {
            assert!(
                ops.is_empty(),
                "{session} never enters the world yet carries ordered events {ops:?}"
            );
        }
    }
    assert!(
        fellowship_sessions > 0 && other_sessions > 0 && entered_sessions > 0,
        "the corpus has fellowship recordings ({fellowship_sessions}), other recordings \
         ({other_sessions}) and recordings that enter the world ({entered_sessions})"
    );
}

/// **Every recorded allegiance update decodes, and its member count agrees with its roster.**
///
/// Every recording is replayed and every delivered `0x0020 Allegiance_AllegianceUpdate` decoded.
/// An empty hierarchy reports zero members and zero vassals; a populated one reports a positive
/// member count; every update is at allegiance version 11. The corpus carries both kinds, and each
/// `fellowship-*` recording carries a populated one (the shard's three-member hierarchy, `Two`
/// over `One` over `Three`), so the allegiance page has version/empty-tree controls and populated
/// roster examples.
///
/// The arm that applies these is `pyreal_client_runtime::interaction::apply_events`' `0x0020` case, and the
/// handler under it is the world allegiance-update handler.
#[test]
fn every_recorded_allegiance_update_decodes_and_its_member_count_agrees_with_its_roster() {
    use pyreal_protocol::{Message, Opcode};

    let mut total = 0usize;
    let mut with_members = 0usize;
    let mut versions: BTreeSet<u32> = BTreeSet::new();
    let mut per: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for session in corpus_sessions() {
        let events = replay(&session);
        let entry = per.entry(session.clone()).or_default();
        for e in &events {
            let Some((opcode, body)) = e.ui_body() else {
                continue;
            };
            if opcode != Opcode::ALLEGIANCE_ALLEGIANCE_UPDATE {
                continue;
            }
            let mut r = pyreal_protocol::archive::Reader::new(body);
            let m = pyreal_protocol::social::AllegianceUpdate::read(&mut r)
                .expect("every `0x0020` in the corpus must decode");
            total += 1;
            entry.0 += 1;
            versions.insert(m.profile.hierarchy.version);
            if m.profile.hierarchy.members.is_empty() {
                assert_eq!(
                    m.profile.total_members, 0,
                    "{session}: total_members of an empty roster"
                );
                assert_eq!(
                    m.profile.total_vassals, 0,
                    "{session}: total_vassals of an empty roster"
                );
            } else {
                with_members += 1;
                entry.1 += 1;
                assert!(
                    m.profile.total_members > 0,
                    "{session}: a roster with a member count"
                );
            }
        }
    }
    assert!(
        total > with_members && with_members > 0,
        "the corpus carries both empty and populated allegiance updates: {total} `0x0020`, \
         {with_members} with a roster; by session {per:?}"
    );
    for (session, (_, rostered)) in per.iter().filter(|(s, _)| s.starts_with("fellowship-")) {
        assert!(
            *rostered > 0,
            "{session}: a fellowship recording carries no populated allegiance update"
        );
    }
    assert_eq!(
        versions,
        BTreeSet::from([11]),
        "all at allegiance version 11"
    );
}
// ---------------------------------------------------------------------------------------------
// 2. The order the two halves of a login arrive in.
// ---------------------------------------------------------------------------------------------

/// **The three panels above are joined once, on the `0x0013`, so the description has to answer
/// at that moment -- row or no row.**
///
/// The player-description install path puts the login qualities onto the player's row and
/// **returns silently when there is no row yet**, which is the order the shard actually sends in:
/// ACE enqueues the player-description event before the create-object message, and the two travel
/// on different queues (`long-solo-play` carries them at
/// idx 9 on queue 9 and idx 22 on queue 10). `Hud::rebuild_panel_tables` runs on the message and
/// on nothing else, so a `Hud::player_desc` that could only read the row would answer `None` at
/// the one moment it is ever asked and the skills page, the spellbook and the attribute page
/// would stay empty for the whole session; it falls back to the parked login description.
///
/// This is a direct model-level statement of the ordering exercised through the screen above: it
/// needs no dats and no device, and it names the ordering instead of relying on a harness that
/// happens not to build a weenie.
///
/// **Falsified by** deleting the `.or_else(|| world.login_player_desc())` from `Hud::player_desc`
/// -- the first `expect` below fires.
///
/// **No datagram leaves this process.** The capture is replayed from disk into the client's own
/// model and every assertion is against that model.
#[test]
fn the_login_description_answers_before_the_players_own_row_exists() {
    let events = replay("first-login-walk-jump");
    let desc = player_description(&events).expect("0x0013");
    let want: BTreeMap<u32, u32> = desc
        .qualities
        .skills
        .as_ref()
        .expect("the capture's 0x0013 carries skills")
        .entries
        .iter()
        .map(|(k, s)| (*k, s.sac))
        .collect();
    let (id, sac) = want
        .iter()
        .map(|(k, v)| (*k, *v))
        .next()
        .expect("the capture names at least one skill");

    // No `0xF746`/`0xF745` is replayed here **on purpose**: this is exactly the window the park
    // exists for, and asserting the premise is what stops the test passing for the wrong reason.
    let mut hud = pyreal_client_shell::hud::Hud::default();
    let mut world = pyreal_client_model::World::new();
    hud.apply_events(events_in_world(&events), &mut world);
    assert!(
        world.player_qualities().is_none(),
        "the premise: no weenie row, so the description has nowhere to have been installed"
    );

    let parked = hud
        .player_desc(&world)
        .expect("the parked description answers the join");
    assert_eq!(
        parked.skill(id).map(|s| s.sac),
        Some(sac),
        "skill {id:#04X}: the answer is the capture's own skill-stat entry"
    );

    // Manually create the player's row and adopt it. This verifies that the same description then
    // answers from the row. The test does not drive a later stat-update message.
    let player = world
        .player
        .or(hud.player)
        .expect("`0xF746` named the player");
    world.player = None;
    world
        .tables
        .weenies
        .insert(player, pyreal_client_model::weenie::Weenie::new(player));
    assert!(world.set_player(player), "the identity is adopted once");
    let installed = world
        .player_qualities()
        .expect("`set_player` replayed the park onto the row");
    assert_eq!(installed.skill(id).map(|s| s.sac), Some(sac));
    assert_eq!(
        hud.player_desc(&world)
            .and_then(|q| q.skill(id))
            .map(|s| s.sac),
        Some(sac)
    );
}
