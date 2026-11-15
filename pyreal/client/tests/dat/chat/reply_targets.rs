//! The chat reply aliases address the person they name. A tell from a player (sender id in the
//! player range, addressed to this player, not from this player) remembers its sender as the last
//! teller; a monarch (0x4000) or patron (0x2000) channel broadcast remembers its speaker; no other
//! tell or channel does. The dangerous failure is a tell sent to the wrong player, so every gate has a
//! negative control: an NPC's tell, a tell to somebody else, and a tell to oneself do not arm.
//!
//! | gate | what it stops | oracle |
//! |---|---|---|
//! | sender id in 0x50000001..0x6FFFFFFF inclusive | replying to an NPC | every recorded tell in the seven selected recordings has a sender outside that range |
//! | target id equals the local player id | replying to an overheard tell | a captured body with its target field replaced |
//! | sender id differs from target id | a self-tell arming the reply | a captured body addressed to its own sender |
//!
//! Fixture: `Communication_HearDirectSpeech 0x02BD` tells decoded from seven named recordings
//! through the transport, `ObjectStream` and the HUD (player-like cases rewrite the sender or
//! target field of a captured blob); monarch and patron broadcasts are synthetic encodings. Every
//! fixture path is an `expect`.

use pyreal_client_net::client_session::testing::capture::{shared_session, Datagram};
use pyreal_client_net::client_session::SessionEvent;
use pyreal_client_net::recording::connection_sequence_number;
use pyreal_client_runtime::net::ClientNetwork;
use pyreal_client_runtime::objects::ObjectStream;
use pyreal_primitives::{LocalTime, ObjectId};
use pyreal_protocol::comms::{CommunicationChannelBroadcastRecv, CommunicationHearDirectSpeech};
use pyreal_protocol::{Message as _, Opcode};

// ---------------------------------------------------------------------------------------------
// The capture corpus
// ---------------------------------------------------------------------------------------------

/// The seven named recordings this file's claims are measured over. Later recordings (the
/// fellowship sessions) add overheard and player-to-player traffic and are not part of the
/// population this file asserts over.
const SEVEN_SESSIONS: [&str; 7] = [
    "first-login-walk-jump",
    "early-inventory-and-casting",
    "short-second-connection",
    "login-account-booted",
    "ddd-interrogation-only",
    "long-solo-play",
    "short-play-with-training",
];

/// One recorded session through the real transport, session and object stream.
fn replay(records: &[Datagram]) -> Vec<SessionEvent> {
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).unwrap_or(0),
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

/// Every 0x02BD blob decoded from the seven selected recordings, with its source filename.
/// The leading opcode dword `SessionEvent::UiEvent` requires is kept.
fn corpus_tells() -> Vec<(String, Vec<u8>)> {
    // In file-name order, the order the recordings sit in on disk.
    let mut names = SEVEN_SESSIONS;
    names.sort_unstable();
    let mut out = Vec::new();
    for name in names {
        for e in replay(shared_session(name)) {
            if let SessionEvent::UiEvent { opcode, blob } = e {
                if opcode == Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH {
                    out.push((name.to_owned(), blob));
                }
            }
        }
    }
    assert!(
        !out.is_empty(),
        "the seven recordings carry incoming tells; none decoded"
    );
    out
}

fn decode(blob: &[u8]) -> CommunicationHearDirectSpeech {
    let mut r = pyreal_protocol::archive::Reader::new(blob.get(4..).unwrap_or_default());
    CommunicationHearDirectSpeech::read(&mut r).expect("a recorded 0x02BD decodes")
}

/// The final four u32 fields are sender id, target id, text type and secret flags. Replacing
/// the sender's four-byte field in place preserves every other byte of each selected blob.
const SENDER_ID_FROM_END: usize = 16;

fn with_sender_id(blob: &[u8], id: ObjectId) -> Vec<u8> {
    let mut out = blob.to_vec();
    let at = out.len() - SENDER_ID_FROM_END;
    out[at..at + 4].copy_from_slice(&id.0.to_le_bytes());
    out
}

fn with_target_id(blob: &[u8], id: ObjectId) -> Vec<u8> {
    let mut out = blob.to_vec();
    let at = out.len() - SENDER_ID_FROM_END + 4;
    out[at..at + 4].copy_from_slice(&id.0.to_le_bytes());
    out
}

/// Deliver SessionEvent::PlayerCreated to the HUD and the world owning its ChatState.
/// This supplies the local player id used by note_last_teller's target comparison.
fn hud_for(player: ObjectId) -> (pyreal_client_shell::hud::Hud, ObjectStream) {
    let mut hud = pyreal_client_shell::hud::Hud::default();
    let mut objects = ObjectStream::new();
    let _ = hud.apply_events(&[SessionEvent::PlayerCreated(player)], &mut objects.world);
    assert_eq!(hud.player, Some(player), "the player id reached the HUD");
    // A reply target also needs a player body: direct speech is ignored while there is none,
    // even when the player id is known. `Hud::sync` sets `Hud::player_body` from the optional
    // `ViewerFrame`; this helper sets it to true directly to isolate the recipient-name logic, so
    // it does not exercise the live geometry producer.
    hud.player_body = true;
    (hud, objects)
}

fn feed(
    hud: &mut pyreal_client_shell::hud::Hud,
    objects: &mut ObjectStream,
    opcode: Opcode,
    blob: Vec<u8>,
) {
    let _ = hud.apply_events(
        &[SessionEvent::UiEvent { opcode, blob }],
        &mut objects.world,
    );
}

fn ui_blob<M: pyreal_protocol::Message>(m: &M) -> Vec<u8> {
    let mut w = pyreal_protocol::archive::Writer::new();
    w.u32(M::OPCODE.0);
    m.write(&mut w).expect("the message encodes");
    w.into_inner()
}

// ---------------------------------------------------------------------------------------------
// 1. The misdirection guard, against the corpus
// ---------------------------------------------------------------------------------------------

/// Every recorded NPC tell in the seven selected recordings declines to become a reply target.
/// Their target ids are player-range ids; their sender ids are not. Retargeting each message to
/// the same local player isolates the sender-range guard from the target guard.
///
/// Both counters matter: as many declines as tells and zero writes prove the handler ran,
/// whereas an unreachable handler would satisfy zero writes alone. Widening
/// `Hud::CLICKABLE_PLAYER_IDS` to every id makes every tell a write and leaves an NPC remembered.
#[test]
fn no_recorded_npc_tell_arms_the_reply_key() {
    let tells = corpus_tells();
    let mut senders: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut addressed_to_a_player = 0usize;

    // The player id is taken from the tells themselves -- these really are addressed to us -- so
    // the *only* gate under test here is the sender's id range.
    let player = ObjectId(decode(&tells[0].1).target_id.0);
    let (mut hud, mut objects) = hud_for(player);
    let mut targets_seen: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();

    for (_, blob) in &tells {
        let m = decode(blob);
        senders.insert(m.sender_name.clone());
        targets_seen.insert(m.target_id.0);
        if (0x5000_0001..0x7000_0000).contains(&m.target_id.0) {
            addressed_to_a_player += 1;
        }
        // Re-target every tell at *this* player so the target gate cannot be what declines them.
        feed(
            &mut hud,
            &mut objects,
            Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
            with_target_id(blob, player),
        );
    }

    eprintln!(
        "{} tells from {} distinct speakers, {addressed_to_a_player} addressed to a \
         player object; targets {targets_seen:?}",
        tells.len(),
        senders.len()
    );
    assert_eq!(
        addressed_to_a_player,
        tells.len(),
        "every recorded tell is addressed to a player character"
    );
    assert_eq!(
        hud.stats.last_teller_declined,
        tells.len() as u64,
        "the arm ran on every recorded tell"
    );
    assert_eq!(hud.stats.last_teller_writes, 0, "and stored none of them");
    assert_eq!(objects.world.chat.last_teller, None);
    assert_eq!(
        objects.world.chat.last_teller_name, "",
        "no NPC became the reply target"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. A player's tell does arm it — off a captured blob, four bytes changed
// ---------------------------------------------------------------------------------------------

///
/// A captured NPC tell with a player-range sender id populates both remembered fields. The
/// first Jonathan message in the named corpus is used; `early-inventory-and-casting` and
/// `short-play-with-training` both carry it, so no particular session is required. Only the
/// sender field is overwritten: at most four bytes differ, and the decoded message, name and
/// target are checked separately.
///
/// Removing the name write or the `note_last_teller` call from the 0x02BD consumer fails these
/// assertions. The positive is a transformed captured blob, not an observed player-to-player
/// tell.
#[test]
fn a_players_tell_addressed_to_us_becomes_the_reply_target() {
    let tells = corpus_tells();
    let (from, recorded) = tells
        .iter()
        .find(|(_, b)| decode(b).sender_name == "Jonathan")
        .expect(
            "early-inventory-and-casting and short-play-with-training both carry Jonathan's tells",
        );
    let original = decode(recorded);
    let me = original.target_id;

    // A player character id, inside the clickable range the client tests for.
    let alba = ObjectId(0x5000_0007);
    let blob = with_sender_id(recorded, alba);
    let differing = recorded.iter().zip(&blob).filter(|(a, b)| a != b).count();
    assert!(
        differing <= 4,
        "only the four bytes of senderID may differ from the recorded blob; {differing} do"
    );
    let sent = decode(&blob);
    assert_eq!(sent.message, original.message, "the shard's own words");
    assert_eq!(sent.sender_name, "Jonathan");
    assert_eq!(sent.target_id, me);

    let (mut hud, mut objects) = hud_for(me);
    feed(
        &mut hud,
        &mut objects,
        Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
        blob,
    );

    assert_eq!(hud.stats.last_teller_writes, 1, "one tell, one write");
    assert_eq!(hud.stats.last_teller_declined, 0);
    assert_eq!(
        objects.world.chat.last_teller,
        Some(alba),
        "the sender id is remembered"
    );
    assert_eq!(
        objects.world.chat.last_teller_name, "Jonathan",
        "the sender name is remembered"
    );
    eprintln!(
        "{from}: a captured tell with senderID -> {:#010X} arms `@r`",
        alba.0
    );

    // And it is visible through the seam the screen reads, not only in the field.
    let t = hud.reply_targets(&objects.world);
    assert_eq!(t.last_teller.as_deref(), Some("Jonathan"));
    assert_eq!(t.monarch, None, "no monarch has spoken");
    assert_eq!(t.patron, None, "and no patron");
}

/// The other two gates, each on its own, each with a captured blob behind it.
///
/// A player-range sender addressed to someone else does not redirect our reply. A self-tell
/// declines too: self is inside the player range, so the range guard alone cannot stop it; the
/// self branch prints "You think" without recording a reply target. Each case uses a fresh
/// HUD and world and checks that the decline arm ran; removing either gate fails.
#[test]
fn a_tell_we_only_overheard_and_a_tell_to_ourselves_both_store_nothing() {
    let tells = corpus_tells();
    let recorded = &tells[0].1;
    let me = ObjectId(0x5000_0007);
    let someone_else = ObjectId(0x5000_0008);
    let a_player = ObjectId(0x5000_0009);

    // (a) addressed to someone else.
    let (mut hud, mut objects) = hud_for(me);
    let blob = with_target_id(&with_sender_id(recorded, a_player), someone_else);
    feed(
        &mut hud,
        &mut objects,
        Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
        blob,
    );
    assert_eq!(hud.stats.last_teller_declined, 1, "the arm ran");
    assert_eq!(
        objects.world.chat.last_teller_name, "",
        "a tell to someone else is not ours"
    );

    // (b) a tell to ourselves.
    let (mut hud, mut objects) = hud_for(me);
    let blob = with_target_id(&with_sender_id(recorded, me), me);
    feed(
        &mut hud,
        &mut objects,
        Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
        blob,
    );
    assert_eq!(hud.stats.last_teller_declined, 1, "the arm ran");
    assert_eq!(
        objects.world.chat.last_teller, None,
        "`You think, \"...\"` stores nothing"
    );
}

/// Session cleanup must discard remembered names before another character can reuse them.
/// This case arms last-teller and monarch; patron starts empty, so it does not independently
/// demonstrate clearing an armed patron name.
#[test]
fn logging_off_forgets_all_three_names() {
    let tells = corpus_tells();
    let me = ObjectId(0x5000_0007);
    let (mut hud, mut objects) = hud_for(me);
    feed(
        &mut hud,
        &mut objects,
        Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
        with_target_id(&with_sender_id(&tells[0].1, ObjectId(0x5000_0009)), me),
    );
    feed(
        &mut hud,
        &mut objects,
        Opcode::COMMUNICATION_CHANNEL_BROADCAST,
        ui_blob(&CommunicationChannelBroadcastRecv {
            channel: pyreal_client_runtime::hud::channel::MONARCH,
            sender_name: "Aldis".to_owned(),
            message: "all hail".to_owned(),
        }),
    );
    let before = hud.reply_targets(&objects.world);
    assert!(
        before.last_teller.is_some() && before.monarch.is_some(),
        "both are armed: {before:?}"
    );

    let _ = hud.apply_events(&[SessionEvent::LoggedOff], &mut objects.world);
    // The HUD's LoggedOff clears its channel names. `ObjectStream::reset` normally replaces the
    // world; that assignment is made directly here to clear the world's last-teller pair, so this
    // exercises the two state owners, not the full network-driven reset path.
    objects.world = pyreal_client_model::World::new();
    let after = hud.reply_targets(&objects.world);
    assert_eq!(
        after,
        pyreal_client_contract::chat::window::ReplyTargets::default(),
        "all three cleared"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The monarch and the patron
// ---------------------------------------------------------------------------------------------

/// Only monarch 0x4000 and patron 0x2000 broadcasts remember their speaker. Fellowship 0x800,
/// vassals 0x1000, co-vassals 0x1000000 and allegiance 0x2000000 do not; there is no vassal
/// reply alias. The channel handler has exactly those two writes.
///
/// The messages are synthetic `pyreal_protocol` encodings: the recordings carry no
/// `Communication_ChannelBroadcast 0x0147`, so this file claims no recorded-channel oracle.
/// Swapping the monarch and patron arms of `note_at_channel_speaker` is the targeted negative.
#[test]
fn only_the_monarch_and_patron_channels_remember_their_speaker() {
    let me = ObjectId(0x5000_0007);
    let (mut hud, mut objects) = hud_for(me);
    let say = |channel: u32, who: &str| CommunicationChannelBroadcastRecv {
        channel,
        sender_name: who.to_owned(),
        message: "orders".to_owned(),
    };
    let send = |hud: &mut pyreal_client_shell::hud::Hud,
                objects: &mut ObjectStream,
                m: CommunicationChannelBroadcastRecv| {
        feed(
            hud,
            objects,
            Opcode::COMMUNICATION_CHANNEL_BROADCAST,
            ui_blob(&m),
        );
    };

    // The four channels that store nothing, first, so a later write cannot mask a wrong one.
    for (channel, who) in [
        (0x800u32, "Fella"),
        (0x1000, "Vassal"),
        (0x100_0000, "CoVassal"),
        (0x200_0000, "Ally"),
    ] {
        send(&mut hud, &mut objects, say(channel, who));
    }
    assert_eq!(
        hud.stats.at_channel_name_writes, 0,
        "four channels, no name remembered"
    );
    assert_eq!(hud.reply_targets(&objects.world), Default::default());

    send(
        &mut hud,
        &mut objects,
        say(pyreal_client_runtime::hud::channel::MONARCH, "Aldis"),
    );
    send(
        &mut hud,
        &mut objects,
        say(pyreal_client_runtime::hud::channel::PATRON, "Plonk"),
    );
    assert_eq!(hud.stats.at_channel_name_writes, 2);
    let t = hud.reply_targets(&objects.world);
    assert_eq!(t.monarch.as_deref(), Some("Aldis"), "0x4000 is the monarch");
    assert_eq!(t.patron.as_deref(), Some("Plonk"), "0x2000 is the patron");
    assert_eq!(t.last_teller, None, "and a broadcast is not a tell");

    // An empty sender denotes the local broadcast echo and follows the "You say to your"
    // display branch. It must neither add a write nor erase the remembered monarch.
    send(
        &mut hud,
        &mut objects,
        say(pyreal_client_runtime::hud::channel::MONARCH, ""),
    );
    assert_eq!(
        hud.stats.at_channel_name_writes, 2,
        "our own line stores nothing"
    );
    assert_eq!(
        hud.reply_targets(&objects.world).monarch.as_deref(),
        Some("Aldis"),
        "and does not clear what was there"
    );
}
