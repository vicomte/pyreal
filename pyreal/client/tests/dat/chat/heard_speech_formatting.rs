//! Heard speech: the three "hear" messages (`0x02BB` speech, `0x02BC` ranged speech, `0x02BD`
//! tells) are composed by `pyreal_client_model::chat::composition` through the real `Hud`, and the
//! timestamp option stamps them.
//!
//! The whole line -- name, verb, comma and quotes -- is the message body; the prefix slot is
//! the optional timestamp. The display path builds two string records and appends them in this
//! order:
//!
//! ```text
//!   argument 4: window id
//!   argument 3: string record 2 = the timestamp
//!   argument 2: string record 1 = the body
//!   argument 1: chat type
//! ```
//!
//! Record 2 is the `%#H:%M:%S ` clock, produced only when the display-timestamps player option
//! is on (and never for chat type `0x1A`).
//!
//! | claim | oracle |
//! |---|---|
//! | the seven templates and their argument order | the client's format literals |
//! | `0x02BC` has no `You say` arm | the self-speech template belongs to the ordinary-speech handler only; the behavioural distinction is exercised below |
//! | what a line looks like | recorded `0x02BB` / `0x02BD` bodies, replayed through the real `ClientNetwork` / `ObjectStream` / `Hud` |
//! | `StayInChatMode` | the first option word shifted by 11 and masked by 1 |
//!
//! The recordings carry no `0x02BC` ([`the_corpus_carries_no_ranged_speech_at_all`]), so the
//! ranged line is a recorded `0x02BB` body re-addressed as a `0x02BC`: every string in it is off
//! the wire and the framing is this test's.
//!
//! Fixture: seven named recordings fed to a `ClientNetwork` with no socket peer. Every fixture
//! path is an `expect` or an `assert!`.

use pyreal_client_contract::chat::interface::ChatMessage;
use pyreal_client_model::chat::composition as chat;
use pyreal_client_model::player::options::option::{DISPLAY_TIME_STAMPS, STAY_IN_CHAT_MODE};
use pyreal_client_model::scroll;
use pyreal_client_net::client_session::testing::capture::{shared_session, Datagram};
use pyreal_client_net::client_session::SessionEvent;
use pyreal_client_net::recording::connection_sequence_number;
use pyreal_client_runtime::net::ClientNetwork;
use pyreal_client_runtime::objects::ObjectStream;
use pyreal_client_shell::hud::Hud;
use pyreal_primitives::LocalTime;
use pyreal_protocol::comms::{
    CommunicationHearDirectSpeech, CommunicationHearRangedSpeech, CommunicationHearSpeech,
};
use pyreal_protocol::{Message as _, Opcode};

// =============================================================================================
// The corpus -- the same replay `chat::reply_targets` uses
// =============================================================================================

/// The seven named recordings every claim in this file is measured over, in file-name order.
/// Later recordings (the `fellowship-*` sessions, with overheard says and a player-to-player tell)
/// are not part of the population.
const SEVEN_SESSIONS: [&str; 7] = [
    "ddd-interrogation-only",
    "early-inventory-and-casting",
    "first-login-walk-jump",
    "login-account-booted",
    "long-solo-play",
    "short-play-with-training",
    "short-second-connection",
];

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

/// Every incoming `0x02BB`, `0x02BC` and `0x02BD` in the seven captures, in one pass.
fn corpus_speech() -> (
    Vec<CommunicationHearSpeech>,
    Vec<CommunicationHearRangedSpeech>,
    Vec<CommunicationHearDirectSpeech>,
) {
    let (mut says, mut ranged, mut tells) = (Vec::new(), Vec::new(), Vec::new());
    for name in SEVEN_SESSIONS {
        for e in replay(shared_session(name)) {
            let Some((opcode, body)) = e.ui_body() else {
                continue;
            };
            let mut r = pyreal_protocol::archive::Reader::new(body);
            match opcode {
                Opcode::COMMUNICATION_HEAR_SPEECH => {
                    says.push(CommunicationHearSpeech::read(&mut r).expect("a recorded 0x02BB"));
                }
                Opcode::COMMUNICATION_HEAR_RANGED_SPEECH => {
                    ranged.push(
                        CommunicationHearRangedSpeech::read(&mut r).expect("a recorded 0x02BC"),
                    );
                }
                Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH => {
                    tells.push(
                        CommunicationHearDirectSpeech::read(&mut r).expect("a recorded 0x02BD"),
                    );
                }
                _ => {}
            }
        }
    }
    (says, ranged, tells)
}

/// Put the `Hud` into the state the three speech handlers require before they will draw anything.
///
/// * The player body must exist for all three handlers: a client with no body draws no speech at
///   all. `Hud::player_body` is written by `Hud::sync` from the `ViewerFrame` `App::frame` builds
///   out of `WorldScene::character`; the emote and hearing tests drive that producer for real,
///   standing a recorded speaker at measured distances. Here it is set directly, because this
///   file's subject is the seven format templates and not the geometry.
/// * A physics body for the speaker: ranged speech resolves both ids and answers out of range if
///   either is missing, so `0x02BC` from a speaker the object table has never seen draws nothing.
///   The radar entry below is that body, standing at the listener's own feet so that any positive
///   `range` on the message admits it.
///
/// The asymmetry is the client's, not this helper's: the ordinary audibility check's missing-body
/// arm is an escape (audible at any range) while the ranged-distance check's missing-body arm is
/// a refusal. `0x02BB` and `0x02BD` therefore do not need the radar entry and `0x02BC` cannot do
/// without it.
fn embody(hud: &mut Hud, speaker: pyreal_primitives::ObjectId) {
    hud.player_body = true;
    hud.radar.push(pyreal_client_contract::view::RadarEntry {
        id: speaker,
        player_space: (0.0, 0.0, 0.0),
        in_world: true,
        ..pyreal_client_contract::view::RadarEntry::default()
    });
}

/// Feed one message to a real `Hud` as the network would, and take the `ChatMessage`s it pushed.
fn through_hud<M: pyreal_protocol::Message>(
    player: Option<pyreal_primitives::ObjectId>,
    m: &M,
) -> Vec<ChatMessage> {
    through_hud_from(player, None, m)
}

/// [`through_hud`] with the speaker named, so the `0x02BC` range check (`objects_in_range`) has a body to measure.
fn through_hud_from<M: pyreal_protocol::Message>(
    player: Option<pyreal_primitives::ObjectId>,
    speaker: Option<pyreal_primitives::ObjectId>,
    m: &M,
) -> Vec<ChatMessage> {
    let mut hud = Hud::default();
    let mut objects = ObjectStream::new();
    if let Some(p) = player {
        let _ = hud.apply_events(&[SessionEvent::PlayerCreated(p)], &mut objects.world);
    }
    embody(
        &mut hud,
        speaker.or(player).unwrap_or(pyreal_primitives::ObjectId(0)),
    );
    let mut w = pyreal_protocol::archive::Writer::new();
    w.u32(M::OPCODE.0);
    m.write(&mut w).expect("the message re-encodes");
    hud.apply_events(
        &[SessionEvent::UiEvent {
            opcode: M::OPCODE,
            blob: w.into_inner(),
        }],
        &mut objects.world,
    )
}

// =============================================================================================
// 1. The denominators
// =============================================================================================

/// The corpus carries no `0x02BC` at all, beside the counts of the two it does carry.
///
/// This is why the ranged line in the drawn test is a recorded `0x02BB` body re-addressed. The
/// other two counts are asserted beside it, so a replay that silently stopped decoding cannot pass
/// as a corpus with no ranged speech in it.
#[test]
fn the_corpus_carries_no_ranged_speech_at_all() {
    let (says, ranged, tells) = corpus_speech();
    assert!(
        !says.is_empty(),
        "the recordings carry overheard `0x02BB` say lines"
    );
    assert!(
        !tells.is_empty(),
        "the recordings carry incoming `0x02BD` tells"
    );
    assert_eq!(
        ranged.len(),
        0,
        "and zero `0x02BC`; the ranged-speech handler is not exercised by a captured fixture"
    );
}

// =============================================================================================
// 2. The three arms, through the real `Hud`
// =============================================================================================

/// Every recorded tell reaches `Hud`'s output as one body with no prefix, and the body is the
/// one `chat::hear_direct_speech_line` composes (the speaker's name is inside the body).
#[test]
fn every_recorded_tell_comes_out_of_the_hud_as_one_body_and_no_prefix() {
    let (_, _, tells) = corpus_speech();
    let mut drawn = 0usize;
    for m in &tells {
        let out = through_hud(Some(m.target_id), m);
        let want = chat::hear_direct_speech_line(
            m.sender_id.0,
            m.target_id.0,
            Some(m.target_id.0),
            &m.sender_name,
            &m.message,
        )
        .expect("every recorded tell is addressed to the player that received it");
        assert_eq!(out.len(), 1, "one line for {m:?}");
        assert_eq!(
            out[0].prefix, None,
            "the prefix slot is the timestamp's, not the speaker's"
        );
        assert_eq!(out[0].body, want);
        assert_eq!(u32::from(out[0].ty), m.text_type);
        // The name is *inside* the body, and the verb is between it and the message.
        assert!(out[0]
            .body
            .contains(&format!("{} tells you, \"", m.sender_name)));
        drawn += 1;
    }
    assert_eq!(
        drawn,
        tells.len(),
        "every recorded tell was driven, not a subset"
    );
    assert!(drawn > 0, "the recordings carry tells to drive");
}

///
/// `0x02BB` takes the `You say` arm for the player's own line and the `says` arm for anyone
/// else's.
///
/// Driven from the same recorded message twice, changing only which id the `Hud` believes is the
/// player. Both directions, because a test that only drives the remote arm passes on a build where
/// the self-echo branch does not exist, which is what `0x02BC` is.
#[test]
fn the_recorded_say_line_echoes_as_you_say_when_the_speaker_is_the_player() {
    let (says, _, _) = corpus_speech();
    let m = says
        .iter()
        .find(|m| !m.message.is_empty() && !m.sender_name.is_empty())
        .expect("a recorded 0x02BB with a body and a speaker");

    let remote = through_hud(Some(pyreal_primitives::ObjectId(m.sender_id.0 ^ 1)), m);
    assert_eq!(remote.len(), 1);
    assert_eq!(remote[0].prefix, None);
    assert_eq!(
        remote[0].body,
        format!("{} says, \"{}\"", tell_run_of(m), m.message)
    );

    let echo = through_hud(Some(m.sender_id), m);
    assert_eq!(echo.len(), 1);
    assert_eq!(echo[0].prefix, None);
    assert_eq!(
        echo[0].body,
        format!("You say, \"{}\"", m.message),
        "`You say, \"%s\"`, the player's own line"
    );
    assert_ne!(
        echo[0].body, remote[0].body,
        "the two arms are not the same string"
    );
}

/// The subject half of a recorded speaker, spelled out here rather than taken from the local
/// formatter: a player id inside `0x50000001..=0x6FFFFFFF` becomes
/// the clickable `<Tell:...>` run, anything else is the bare name.
fn tell_run_of(m: &CommunicationHearSpeech) -> String {
    let id = m.sender_id.0;
    if (0x5000_0001..0x7000_0000).contains(&id) {
        format!("<Tell:IIDString:{id}:{0}>{0}<\\Tell>", m.sender_name)
    } else {
        m.sender_name.clone()
    }
}

/// `0x02BC` is routed to `hear_ranged_speech_line`, not to `hear_speech_line`: the one thing
/// about this wire that is easy to get wrong and that no corpus message can catch.
///
/// The ranged handler uses exactly two templates, `SAYS` and `SAYS_CLICKABLE`, and has no
/// `You say` arm, so a ranged line echoed back to its own speaker still renders as the speaker
/// saying the text. Squelch and range gates normally make that case unreachable; this test
/// reproduces the branch structure rather than claiming it occurs in ordinary play.
///
/// The body is a recorded `0x02BB` body re-addressed as a `0x02BC`; see
/// [`the_corpus_carries_no_ranged_speech_at_all`].
///
/// Falsified by routing the `0x02BC` arm to `chat::hear_speech_line`: the self-addressed case
/// below then renders `You say, "..."`.
#[test]
fn a_ranged_line_addressed_to_its_own_speaker_still_says_says() {
    let (says, _, _) = corpus_speech();
    let src = says
        .iter()
        .find(|m| !m.message.is_empty() && !m.sender_name.is_empty())
        .expect("a recorded 0x02BB to re-address");
    let m = CommunicationHearRangedSpeech {
        message: src.message.clone(),
        sender_name: src.sender_name.clone(),
        sender_id: src.sender_id,
        range: 20.0,
        text_type: src.text_type,
    };

    // The player *is* the speaker — the only case in which the two handlers differ. The speaker
    // is named so the ranged check has a body to measure; see `embody`.
    let out = through_hud_from(Some(m.sender_id), Some(m.sender_id), &m);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].prefix, None);
    assert_eq!(
        out[0].body,
        format!("{} says, \"{}\"", tell_run_of(src), m.message),
        "0x02BC has no `You say` arm"
    );
    assert!(
        !out[0].body.starts_with("You say"),
        "and it must not borrow the other handler's"
    );
}

///
/// A `0x02BD` addressed to somebody else draws nothing at all, and the client counts it rather
/// than losing it.
///
/// After the self-tell branch, the direct-speech handler requires the target id to equal the
/// player's id; a mismatch returns without formatting or appending text.
/// `hear_direct_speech_line` answers `None`, and the arm pushes nothing.
///
/// Three states, not two: the counters separate "no tell arrived" from "a tell arrived and was
/// correctly not drawn".
#[test]
fn a_tell_addressed_to_somebody_else_is_counted_and_not_drawn() {
    let (_, _, tells) = corpus_speech();
    let m = tells
        .iter()
        .find(|m| !m.message.is_empty())
        .expect("a recorded tell with a body");

    let mut hud = Hud::default();
    let mut objects = ObjectStream::new();
    // A player who is neither the sender nor the target: the shard copied us somebody else's tell.
    let bystander = pyreal_primitives::ObjectId(m.target_id.0 ^ 0x0F00_0000);
    assert_ne!(bystander, m.target_id);
    assert_ne!(bystander, m.sender_id);
    let _ = hud.apply_events(
        &[SessionEvent::PlayerCreated(bystander)],
        &mut objects.world,
    );
    embody(&mut hud, m.sender_id);

    let mut w = pyreal_protocol::archive::Writer::new();
    w.u32(Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH.0);
    m.write(&mut w).expect("the message re-encodes");
    let out = hud.apply_events(
        &[SessionEvent::UiEvent {
            opcode: Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
            blob: w.into_inner(),
        }],
        &mut objects.world,
    );
    assert!(out.is_empty(), "nothing is drawn: {out:?}");
    assert_eq!(hud.stats.speech_lines_composed, 0);
    assert_eq!(
        hud.stats.speech_lines_not_addressed_to_us, 1,
        "and it was seen, not lost"
    );
    assert_eq!(
        hud.stats.undecodable, 0,
        "it decoded fine — this is a branch, not a failure"
    );

    // The same message with the right player takes the other branch, so the gate is shown live in
    // both directions rather than only in the one that draws nothing.
    let drawn = through_hud(Some(m.target_id), m);
    assert_eq!(drawn.len(), 1, "addressed to us, it draws");
}

/// A self-tell is `You think, "..."`, and that branch is taken before the player id is consulted.
///
/// The sender-equals-target comparison sits above the player-target gate, so a self-tell renders
/// even for a `Hud` that has never seen a `PlayerCreated`. Asserted with no player at all, which
/// is the state that discriminates.
#[test]
fn a_self_tell_is_you_think_even_before_the_player_id_is_known() {
    let (_, _, tells) = corpus_speech();
    let src = tells
        .iter()
        .find(|m| !m.message.is_empty())
        .expect("a recorded tell with a body");
    let m = CommunicationHearDirectSpeech {
        message: src.message.clone(),
        sender_name: src.sender_name.clone(),
        sender_id: src.sender_id,
        target_id: src.sender_id,
        text_type: src.text_type,
        secret_flags: src.secret_flags,
    };
    let out = through_hud(None, &m);
    assert_eq!(
        out.len(),
        1,
        "no `PlayerCreated` has landed and it still draws"
    );
    assert_eq!(out[0].body, format!("You think, \"{}\"", m.message));
    assert_eq!(out[0].prefix, None);
}

// =============================================================================================
// 3. The timestamp — `ChatMessage::prefix`'s real producer
// =============================================================================================

/// The timestamp prefix has a producer, and it is the option word that turns it on.
///
/// `Hud::apply_events` pushes the display-timestamps option (ordinal 33, `options2_` bit 6) off
/// `PlayerSystem::options` at the head of every batch, and the stamp is
/// `scroll::timestamp_prefix`.
///
/// Asserted both ways from the option word and not by writing `display_time_stamps`: a test that
/// sets the field cannot show a missing producer. The counters are the third state: a batch with
/// the option off is a batch that ran and declined.
#[test]
fn the_option_word_turns_the_timestamp_prefix_on_and_off() {
    let (_, _, tells) = corpus_speech();
    let m = tells
        .iter()
        .find(|m| !m.message.is_empty())
        .expect("a recorded tell");

    let drive = |on: bool| -> (Vec<ChatMessage>, u64, u64) {
        let mut hud = Hud::default();
        let mut objects = ObjectStream::new();
        // The option word, as a `0x0013` would leave it. Ordinal 33 is `options2_` bit 6, written
        // here as the literal the client uses.
        assert_eq!(DISPLAY_TIME_STAMPS, 33);
        objects
            .world
            .player_system
            .options
            .set(DISPLAY_TIME_STAMPS, on);
        assert_eq!(
            objects.world.player_system.options.options2 & 0x0000_0040 != 0,
            on,
            "the bit really is options2_ 0x40"
        );
        let _ = hud.apply_events(
            &[SessionEvent::PlayerCreated(m.target_id)],
            &mut objects.world,
        );
        embody(&mut hud, m.sender_id);
        let mut w = pyreal_protocol::archive::Writer::new();
        w.u32(Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH.0);
        m.write(&mut w).expect("re-encodes");
        let out = hud.apply_events(
            &[SessionEvent::UiEvent {
                opcode: Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
                blob: w.into_inner(),
            }],
            &mut objects.world,
        );
        (
            out,
            hud.stats.timestamps_stamped,
            hud.stats.timestamps_suppressed,
        )
    };

    let (off, stamped_off, suppressed_off) = drive(false);
    assert_eq!(off.len(), 1);
    assert_eq!(off[0].prefix, None, "the shipped default has no stamp");
    assert_eq!(
        (stamped_off, suppressed_off),
        (0, 1),
        "the stamper ran and declined"
    );

    let (on, stamped_on, suppressed_on) = drive(true);
    assert_eq!(on.len(), 1);
    assert_eq!((stamped_on, suppressed_on), (1, 0));
    let stamp = on[0]
        .prefix
        .clone()
        .expect("the option is on, so there is a stamp");
    assert_eq!(on[0].body, off[0].body, "the body is the same either way");

    // The shape, without pinning the wall clock: `H:MM:SS ` with a trailing space, and it is the
    // same string `scroll::timestamp_prefix` renders for some second of some day.
    let digits: Vec<&str> = stamp.trim_end().split(':').collect();
    assert_eq!(digits.len(), 3, "H:MM:SS, got {stamp:?}");
    let h: u32 = digits[0].parse().expect("hours");
    let mi: u32 = digits[1].parse().expect("minutes");
    let s: u32 = digits[2].parse().expect("seconds");
    assert!(h < 24 && mi < 60 && s < 60, "{stamp:?}");
    assert_eq!(digits[1].len(), 2, "%M keeps its leading zero");
    assert_eq!(digits[2].len(), 2, "%S keeps its leading zero");
    assert!(stamp.ends_with(' '), "the format's trailing space");
    assert_eq!(
        stamp,
        scroll::timestamp_prefix(i64::from(h * 3600 + mi * 60 + s), 0),
        "and it is exactly what the transcription renders for that instant"
    );
}

// =============================================================================================
// 4. The stay-in-chat-mode option
// =============================================================================================

/// `StayInChatMode` is not an auto-save option.
///
/// Auto-save options go out at once as `0x0005`; `StayInChatMode` is ordinal 11, one of the
/// deferred options, and its bit reaches the shard on the next `0x01A1`. The list lengths and the
/// membership are reproduced here.
#[test]
fn stay_in_chat_mode_is_not_an_auto_save_option() {
    use pyreal_client_model::player::options::{
        is_auto_save_option, AUTO_SAVE_ORDINALS, PLAYER_OPTIONS,
    };
    assert_eq!(PLAYER_OPTIONS.len(), 53, "53 ordinals");
    assert_eq!(AUTO_SAVE_ORDINALS.len(), 22, "22 of them are auto-save");
    assert_eq!(STAY_IN_CHAT_MODE, 11);
    assert_eq!(PLAYER_OPTIONS[STAY_IN_CHAT_MODE].0, "StayInChatMode");
    // The accessor shifts the first option word by 11 and masks one bit: option-word-one bit 11.
    // The literal, not the symbol, is pinned here.
    assert_eq!(PLAYER_OPTIONS[STAY_IN_CHAT_MODE].2, 0x0000_0800);
    assert!(matches!(
        PLAYER_OPTIONS[STAY_IN_CHAT_MODE].1,
        pyreal_client_model::player::OptionWord::One
    ));
    assert!(
        !is_auto_save_option(STAY_IN_CHAT_MODE),
        "so the auto-save route does not touch this row"
    );
    // And it is clear in the shipped default.
    assert_eq!(
        pyreal_client_model::player::options::DEFAULT_OPTIONS & 0x0000_0800,
        0
    );
}
