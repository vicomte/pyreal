//! Every chat family is timestamped, language-filtered and copied to the chat log per the
//! options, whichever route produced it. Direct-push families (speech, tells, channels, emotes,
//! textbox text) are post-processed on the newly appended slice in `Hud::apply_events`: their
//! bodies are filtered, `Hud::stamp_timestamps` stamps their prefixes, their finished lines are
//! copied to the log, then they are offered to the display receivers. The scroll producer (deaths,
//! combat notices) does its own trim, filter, stamp and log before queueing its lines. Each line is
//! logged once, not by both routes.
//!
//! The trim happens at two boundaries on purpose: the scroll route trims at its producer, while
//! direct-push models keep their composed or supplied bytes (combat bodies keep their trailing
//! newline, which other chat tests rely on) and the chat window's final-string receiver trims the
//! body for the text element, so no blank row is drawn. Unifying direct pushes through the scroll
//! queue would delay them to the next event batch and could reorder them, so the routes stay
//! separate; the helper here combines a call with an empty drain and compares final line
//! properties, not interleaving within one datagram.
//!
//! Fixture: seven families built as protocol messages (the recordings carry no emote or textbox
//! example), a recorded-opcode scan of the corpus for the five recorded families, and a headless
//! App whose preferences and chat log use temporary paths, its link aimed at a loopback port this
//! process holds and never answers.

use crate::common::client_dir_or_workspace_client as client_dir;

use std::path::{Path, PathBuf};

use pyreal_client::app::App;
use pyreal_client_model::chat::text_type;
use pyreal_client_model::player::options::option::{DISPLAY_TIME_STAMPS, FILTER_LANGUAGE};
use pyreal_client_net::client_session::testing::Corpus;
use pyreal_client_net::client_session::SessionEvent;
use pyreal_client_runtime::config::Config;
use pyreal_primitives::ObjectId;
use pyreal_protocol::{Message as _, Opcode};

const LOCAL: ObjectId = ObjectId(0x5000_0001);
const SPEAKER: ObjectId = ObjectId(0x5000_0044);

// =================================================================================================
// The corpus
// =================================================================================================

fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(off..off + 4)?.try_into().ok()?))
}

/// One recorded blob, as `SessionEvent::UiEvent` carries it: the message opcode dword first, the
/// `0xF7B0` object/sequence envelope already stripped by `pyreal_client_net::client_session`.
#[derive(Debug, Clone)]
struct Recorded {
    session: String,
    opcode: u32,
    blob: Vec<u8>,
}

/// Select requested opcodes in both bare-message and `0xF7B0` game-event wrappers, stripping
/// the latter's 12-byte envelope. Looking in only one wrapper space can falsely report zero.
/// This helper selects and counts blobs; it does not decode the selected message bodies.
fn corpus_blobs(wanted: &[u32]) -> Vec<Recorded> {
    let mut out = Vec::new();
    for corpus in Corpus::shared_all() {
        for blob in &corpus.blobs {
            let b = &blob.payload;
            if let Some(op) = u32_at(b, 0).filter(|op| wanted.contains(op)) {
                out.push(Recorded {
                    session: corpus.name.clone(),
                    opcode: op,
                    blob: b.clone(),
                });
                continue;
            }
            if u32_at(b, 0) == Some(0xF7B0) {
                if let Some(op) = u32_at(b, 12).filter(|op| wanted.contains(op)) {
                    out.push(Recorded {
                        session: corpus.name.clone(),
                        opcode: op,
                        blob: b[12..].to_vec(),
                    });
                }
            }
        }
    }
    out
}

// =================================================================================================
// The application
// =================================================================================================

/// A designated temporary preferences path, separate from the normal user file. No existence
/// check is performed here, so the test does not establish that nothing is already at this path.
fn prefs_file() -> PathBuf {
    std::env::temp_dir().join("pyreal-chat-family-post-processing-not-created/prefs.ini")
}

/// An explicitly selected temporary log path. The host opens this path for
/// `Scroll::start_copy_output_to_file`, and each logging test uses its own tag. Remove any
/// prior file at that path first; this does not select the normal chat log path.
fn log_file(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("pyreal-chat-family-post-processing-{tag}.log"));
    let _ = std::fs::remove_file(&p);
    p
}

fn connected_config() -> Config {
    Config {
        connect: true,
        account: "p1185".into(),
        host: "127.0.0.1".into(),
        port: u32::from(crate::common::unanswered_port()),
        ui: true,
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        preferences_file: prefs_file(),
        ..Config::default()
    }
}

/// Start the gameplay screen with timestamp and language-filter options set through the
/// player option words (`PlayerSystem::options`). `App::apply_hud_events` refreshes the scroll's
/// filter/table before applying HUD events; the HUD refreshes timestamp state before draining
/// and handling a batch. Setting only the scroll fields would bypass those producer connections.
fn app_in_gameplay(stamps: bool, filter: bool, log: Option<&Path>) -> App {
    let d = client_dir();
    assert!(
        pyreal_dat::testing::have_dats(),
        "the retail dats are required at {} -- set PYREAL_TEST_DAT_DIR",
        d.display()
    );
    let mut app =
        crate::common::sim_app::new(connected_config()).expect("the application comes up headless");
    app.start_shell().expect("the UI shell comes up");
    let s = pyreal_client_runtime::scene::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..pyreal_client_runtime::scene::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    app.apply_hud_events(&[SessionEvent::PlayerDescription(Box::default())]);
    app.apply_hud_events(&[SessionEvent::PlayerCreated(LOCAL)]);
    {
        let world = &mut app.probe_mut().objects_mut().world;
        // Display Time Stamps is ordinal 33 / second option word bit 6; Filter Language is
        // ordinal 44. The actual public option constants drive both enabled and disabled arms.
        world.player_system.options.set(DISPLAY_TIME_STAMPS, stamps);
        world.player_system.options.set(FILTER_LANGUAGE, filter);
        world.player = Some(LOCAL);
        if let Some(path) = log {
            // Open the temporary file through the production platform text sink and install
            // that sink on the scroll logger.
            assert!(
                world
                    .scroll
                    .start_copy_output_to_file(&path.to_string_lossy(), 0, || {
                        pyreal_client_runtime::platform::text::open_chat_log(path)
                    }),
                "the temp chat log opens"
            );
        }
    }
    crate::common::sim_app::game_screen(&mut app);
    for _ in 0..8 {
        assert!(app.frame());
    }
    app
}

/// Hand one event to the application, then drain the scroll queue with an empty second call.
/// Scroll-route output arrives in that second return, while direct output arrives in the first.
/// Combine both and run a frame so the tests can compare finalized model and receiver state;
/// this deliberately hides the routes' batch-timing difference rather than testing equivalence
/// of their ordering within a batch.
fn hear(
    app: &mut App,
    e: SessionEvent,
) -> Vec<pyreal_client_contract::chat::interface::ChatMessage> {
    let mut out = app.apply_hud_events(&[e]);
    out.extend(app.apply_hud_events(&[]));
    assert!(app.frame(), "the client is still running");
    out
}

/// Check `H:MM:SS ` shape and ranges without pinning the wall clock. Parse the fields locally
/// and compare the whole prefix with the current `timestamp_prefix` formatter for those seconds
/// at offset zero. This helper returns no timestamp and does not check clock accuracy.
fn assert_timestamp_shape(stamp: &str, what: &str) {
    let digits: Vec<&str> = stamp.trim_end().split(':').collect();
    assert_eq!(digits.len(), 3, "{what}: H:MM:SS, got {stamp:?}");
    let h: u32 = digits[0]
        .parse()
        .unwrap_or_else(|_| panic!("{what}: hours in {stamp:?}"));
    let mi: u32 = digits[1]
        .parse()
        .unwrap_or_else(|_| panic!("{what}: minutes in {stamp:?}"));
    let s: u32 = digits[2]
        .parse()
        .unwrap_or_else(|_| panic!("{what}: seconds in {stamp:?}"));
    assert!(h < 24 && mi < 60 && s < 60, "{what}: {stamp:?}");
    assert_eq!(digits[1].len(), 2, "{what}: %M keeps its leading zero");
    assert_eq!(digits[2].len(), 2, "{what}: %S keeps its leading zero");
    assert!(stamp.ends_with(' '), "{what}: the format's trailing space");
    assert_eq!(
        stamp,
        pyreal_client_model::scroll::timestamp_prefix(i64::from(h * 3600 + mi * 60 + s), 0),
        "{what}: and it is exactly what the transcription renders for that instant"
    );
}

// =================================================================================================
// The six families, as events
// =================================================================================================

/// Construct a player-death (`0x019E`) reference through the scroll producer. Each family is
/// fed in a separate `hear` call; prefix properties are checked against the same formatter.
fn death_event(message: &str) -> SessionEvent {
    let m = pyreal_protocol::combat::CombatHandlePlayerDeathEvent {
        message: message.to_owned(),
        killed: ObjectId(0x5000_0002),
        killer: ObjectId(0x5000_0003),
    };
    let mut w = pyreal_protocol::archive::Writer::new();
    w.u32(Opcode::COMBAT_HANDLE_PLAYER_DEATH_EVENT.0);
    m.write(&mut w).expect("encodes");
    SessionEvent::UiEvent {
        opcode: Opcode::COMBAT_HANDLE_PLAYER_DEATH_EVENT,
        blob: w.into_inner(),
    }
}

/// Construct an attacker notification (`0x01B1`) whose combat formatter appends a newline.
/// The scroll submission uses combat text type 6, plugin announcement true and window 0.
fn combat_event(defender: &str) -> SessionEvent {
    let m = pyreal_protocol::combat::AttackerNotification {
        defender_name: defender.to_owned(),
        damage_type: 1,
        percent: 0.1,
        damage: 3,
        critical: 0,
        attack_conditions: 0,
        attack_conditions_high: 0,
    };
    let mut w = pyreal_protocol::archive::Writer::new();
    w.u32(Opcode::COMBAT_HANDLE_ATTACKER_NOTIFICATION_EVENT.0);
    m.write(&mut w).expect("encodes");
    SessionEvent::UiEvent {
        opcode: Opcode::COMBAT_HANDLE_ATTACKER_NOTIFICATION_EVENT,
        blob: w.into_inner(),
    }
}

/// Construct ordinary speech (`0x02BB`) for the direct-push chat handler.
fn speech_event(message: &str) -> SessionEvent {
    let m = pyreal_protocol::comms::CommunicationHearSpeech {
        message: message.to_owned(),
        sender_name: "Alba".to_owned(),
        sender_id: SPEAKER,
        text_type: text_type::SPEECH,
    };
    let mut w = pyreal_protocol::archive::Writer::new();
    w.u32(Opcode::COMMUNICATION_HEAR_SPEECH.0);
    m.write(&mut w).expect("encodes");
    SessionEvent::UiEvent {
        opcode: Opcode::COMMUNICATION_HEAR_SPEECH,
        blob: w.into_inner(),
    }
}

/// Construct direct speech (`0x02BD`) addressed to the local player.
fn tell_event(message: &str) -> SessionEvent {
    let m = pyreal_protocol::comms::CommunicationHearDirectSpeech {
        message: message.to_owned(),
        sender_name: "Alba".to_owned(),
        sender_id: SPEAKER,
        target_id: LOCAL,
        text_type: 3,
        secret_flags: 0,
    };
    let mut w = pyreal_protocol::archive::Writer::new();
    w.u32(Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH.0);
    m.write(&mut w).expect("encodes");
    SessionEvent::UiEvent {
        opcode: Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
        blob: w.into_inner(),
    }
}

/// Construct a channel broadcast (`0x0147`) on channel 4.
fn channel_event(message: &str) -> SessionEvent {
    let m = pyreal_protocol::comms::CommunicationChannelBroadcastRecv {
        channel: 4,
        sender_name: "Alba".to_owned(),
        message: message.to_owned(),
    };
    let mut w = pyreal_protocol::archive::Writer::new();
    w.u32(Opcode::COMMUNICATION_CHANNEL_BROADCAST.0);
    m.write(&mut w).expect("encodes");
    SessionEvent::UiEvent {
        opcode: Opcode::COMMUNICATION_CHANNEL_BROADCAST,
        blob: w.into_inner(),
    }
}

/// Construct an emote (`0x01E0`) with an explicit speaker and supplied body.
fn emote_event(text: &str) -> SessionEvent {
    let m = pyreal_protocol::comms::CommunicationHearEmote {
        sender: SPEAKER,
        sender_name: "Alba".to_owned(),
        text: text.to_owned(),
    };
    let mut w = pyreal_protocol::archive::Writer::new();
    w.u32(Opcode::COMMUNICATION_HEAR_EMOTE.0);
    m.write(&mut w).expect("encodes");
    SessionEvent::UiEvent {
        opcode: Opcode::COMMUNICATION_HEAR_EMOTE,
        blob: w.into_inner(),
    }
}

/// Construct textbox text (`0xF7E0`) with its supplied text type. The handler retains the body
/// verbatim, making it suitable for an explicit trailing-newline receiver test. The submission
/// uses plugin announcement true and window 0; neither is separately asserted here.
fn textbox_event(text: &str, ty: u32) -> SessionEvent {
    let m = pyreal_protocol::comms::CommunicationTextboxString {
        text: text.to_owned(),
        text_type: ty,
    };
    let mut w = pyreal_protocol::archive::Writer::new();
    w.u32(Opcode::COMMUNICATION_TEXTBOX_STRING.0);
    m.write(&mut w).expect("encodes");
    SessionEvent::UiEvent {
        opcode: Opcode::COMMUNICATION_TEXTBOX_STRING,
        blob: w.into_inner(),
    }
}

// =================================================================================================
// 1. The premise, measured: both seams stamp, filter and log
// =================================================================================================

///
/// All seven constructed families carry a valid prefix with the timestamp option enabled and
/// no prefix with it disabled, using a separate application for each option state. Each event
/// gets its own call plus empty drain; the death reference is not batched with the other events.
/// Direct-route counters distinguish a stamper that ran and declined from one that never ran.
#[test]
fn every_family_carries_the_timestamp_prefix_and_only_when_the_option_is_on() {
    for stamps in [true, false] {
        let mut app = app_in_gameplay(stamps, false, None);
        let mut seen = 0usize;
        for (what, event) in [
            (
                "0x019E player death (scroll route)",
                death_event("p1185 someone died!"),
            ),
            ("0x01B1 combat", combat_event("p1185 Rabbit")),
            ("0x02BB speech", speech_event("p1185 speech")),
            ("0x02BD tell", tell_event("p1185 tell")),
            ("0x0147 channel", channel_event("p1185 channel")),
            ("0x01E0 emote", emote_event("p1185 emote")),
            (
                "0xF7E0 textbox",
                textbox_event("p1185 textbox", text_type::SPEECH),
            ),
        ] {
            let out = hear(&mut app, event);
            let line = out
                .iter()
                .find(|m| m.body.contains("p1185"))
                .unwrap_or_else(|| panic!("{what}: nothing was composed; got {out:?}"));
            if stamps {
                let stamp = line
                    .prefix
                    .clone()
                    .unwrap_or_else(|| panic!("{what}: the option is on and there is no stamp"));
                assert_timestamp_shape(&stamp, what);
            } else {
                assert_eq!(
                    line.prefix, None,
                    "{what}: the disabled timestamp option leaves no prefix"
                );
            }
            seen += 1;
        }
        assert_eq!(seen, 7, "all seven families were driven");

        // The third state: a batch with the option off is a batch that ran and declined, which an
        // absent prefix alone cannot distinguish from a stamper that never ran.
        let s = &app.hud().stats;
        if stamps {
            assert!(
                s.timestamps_stamped >= 6,
                "the direct-push families were stamped: {s:?}"
            );
        } else {
            assert_eq!(
                s.timestamps_stamped, 0,
                "nothing was stamped with the option off"
            );
            assert!(
                s.timestamps_suppressed >= 6,
                "and the stamper ran and declined: {s:?}"
            );
        }
    }
}

/// Each constructed family is copied to the temporary chat log with its prefix before the
/// body. Read the file and require each expected prefix/body substring exactly once, covering
/// producer logging for the scroll route and direct-slice logging for the other route. A CRLF
/// presence check covers the platform text sink; this is not an exact whole-file comparison.
#[test]
fn every_family_is_copied_to_the_chat_log_with_its_prefix() {
    let path = log_file("log");
    let mut app = app_in_gameplay(true, false, Some(&path));

    let mut wanted: Vec<(&str, String)> = Vec::new();
    for (what, needle, event) in [
        ("0x019E", "p1185 log death", death_event("p1185 log death!")),
        ("0x01B1", "p1185logRabbit", combat_event("p1185logRabbit")),
        (
            "0x02BB",
            "p1185 log speech",
            speech_event("p1185 log speech"),
        ),
        ("0x02BD", "p1185 log tell", tell_event("p1185 log tell")),
        (
            "0x0147",
            "p1185 log channel",
            channel_event("p1185 log channel"),
        ),
        ("0x01E0", "p1185 log emote", emote_event("p1185 log emote")),
        (
            "0xF7E0",
            "p1185 log textbox",
            textbox_event("p1185 log textbox", text_type::SPEECH),
        ),
    ] {
        let out = hear(&mut app, event);
        let line = out
            .iter()
            .find(|m| m.body.contains(needle))
            .unwrap_or_else(|| panic!("{what}: nothing carried {needle:?}; got {out:?}"));
        let prefix = line
            .prefix
            .clone()
            .unwrap_or_else(|| panic!("{what}: no stamp"));
        wanted.push((what, format!("{prefix}{}", line.body)));
    }

    let written = std::fs::read(&path).expect("the temp chat log is readable");
    let written = String::from_utf8_lossy(&written).into_owned();
    for (what, expected) in &wanted {
        assert!(
            written.contains(expected),
            "{what}: `{expected}` is not in the chat log. Log was:\n{written}"
        );
    }
    // Written once, not twice: the scroll route logs at its producer and the direct route logs at
    // the seam, and the two must not both claim the same line.
    for (what, expected) in &wanted {
        assert_eq!(
            written.matches(expected.as_str()).count(),
            1,
            "{what}: the line was logged more than once"
        );
    }
    // The platform text sink uses CRLF; this checks its presence, not every line ending.
    assert!(written.contains("\r\n"), "the log uses CRLF: {written:?}");
    let _ = std::fs::remove_file(&path);
}

/// Each constructed family replaces an audience-1 taboo token from shipped table `0x0E00001E`
/// when Filter Language is enabled and preserves it when disabled. Surround it with spaces so
/// it remains a middle token for the space-delimited matcher. Both arms are required: a missing table cannot pass
/// merely by leaving all text untouched.
#[test]
fn every_family_passes_the_taboo_table_and_only_when_the_option_is_on() {
    for filter in [true, false] {
        let mut app = app_in_gameplay(true, filter, None);
        for (what, event) in [
            ("0x019E", death_event("p1185 shit death")),
            ("0x01B1", combat_event("p1185 shit Rabbit")),
            ("0x02BB", speech_event("p1185 shit speech")),
            ("0x02BD", tell_event("p1185 shit tell")),
            ("0x0147", channel_event("p1185 shit channel")),
            ("0x01E0", emote_event("p1185 shit emote")),
            (
                "0xF7E0",
                textbox_event("p1185 shit textbox", text_type::SPEECH),
            ),
        ] {
            let out = hear(&mut app, event);
            let line = out
                .iter()
                .find(|m| m.body.contains("p1185"))
                .unwrap_or_else(|| panic!("{what}: nothing was composed; got {out:?}"));
            if filter {
                assert!(
                    line.body.contains("****"),
                    "{what}: audience 1 of the shipped table must replace the token; got {:?}",
                    line.body
                );
                assert!(
                    !line.body.contains("shit"),
                    "{what}: the original token must not remain; got {:?}",
                    line.body
                );
            } else {
                assert!(
                    line.body.contains("shit"),
                    "{what}: with the option off the table is bypassed; got {:?}",
                    line.body
                );
            }
        }
        // The scroll producer filters its own lines and must not send them through the direct
        // slice again. This extra death-line check rejects an eight-asterisk result. It is a
        // specific output negative, not a general count of filter invocations or proof that
        // the shipped replacement token would change under every possible second pass.
        let mut app2 = app_in_gameplay(true, filter, None);
        let out = hear(&mut app2, death_event("p1185 shit death"));
        let line = out
            .iter()
            .find(|m| m.body.contains("p1185"))
            .expect("the scroll-route line");
        assert!(
            !line.body.contains("********"),
            "filtered twice: {:?}",
            line.body
        );
    }
}

// =================================================================================================
// 2. The residue: the trim
// =================================================================================================

/// The five recorded families this file's constructed lines stand for -- combat, speech, tells,
/// channels and killer notifications -- are each in the corpus, in bare-message or `0xF7B0`
/// game-event wrappers, and every recorded one carries at least its opcode. This does not decode
/// body text or apply a trim predicate.
#[test]
fn every_recorded_chat_family_the_file_constructs_is_in_the_corpus() {
    let all = corpus_blobs(&[0x01B1, 0x02BB, 0x02BD, 0x0147, 0x01AD]);
    let n = |op: u32| all.iter().filter(|r| r.opcode == op).count();
    // Every recorded family must be present, independently of any claim about decoded text.
    assert!(n(0x01B1) > 0, "recorded combat notifications");
    assert!(n(0x02BB) > 0, "recorded speech");
    assert!(n(0x02BD) > 0, "recorded direct speech");
    assert!(n(0x0147) > 0, "recorded channel broadcasts");
    assert!(n(0x01AD) > 0, "recorded killer notifications");
    assert!(
        all.iter().all(|r| r.blob.len() >= 4),
        "every recorded blob carries at least its opcode"
    );
    // Require contributions from these two named sessions. This guards against some loader
    // omissions, but does not prove per-opcode provenance or exclude every duplicate-file bug.
    let sessions: std::collections::BTreeSet<&str> =
        all.iter().map(|r| r.session.as_str()).collect();
    assert!(
        sessions.contains("early-inventory-and-casting") && sessions.contains("long-solo-play"),
        "the selected corpus includes early-inventory-and-casting and long-solo-play: {sessions:?}"
    );
    eprintln!(
        "{} recorded blobs across five families and {} session(s); body trimming is not checked",
        all.len(),
        sessions.len()
    );
}

/// A trailing newline is kept in the model: the scroll submission trims before building the body
/// text; direct-push models keep their input or composed bytes. Trimming at the HUD boundary
/// breaks the combat damage lines, whose formatter's newline is part of their model body.
///
/// Both sources of a newline are exercised: one composed by the combat formatter and one
/// explicitly supplied to the textbox handler.
#[test]
fn the_model_keeps_the_shards_bytes() {
    let mut app = app_in_gameplay(true, false, None);

    let combat = hear(&mut app, combat_event("p1185trimRabbit"));
    let combat_body = combat
        .iter()
        .find(|m| m.body.contains("p1185trimRabbit"))
        .expect("the 0x01B1 line")
        .body
        .clone();
    assert!(
        combat_body.ends_with('\n'),
        "the combat formatter retains its trailing newline at the model boundary: {combat_body:?}"
    );

    let textbox = hear(
        &mut app,
        textbox_event("p1185 trim shard\n", text_type::SPEECH),
    );
    let textbox_body = textbox
        .iter()
        .find(|m| m.body.contains("p1185 trim shard"))
        .expect("the 0xF7E0 line")
        .body
        .clone();
    assert_eq!(
        textbox_body, "p1185 trim shard\n",
        "the textbox handler passes the supplied body through unchanged at the model boundary"
    );
}
