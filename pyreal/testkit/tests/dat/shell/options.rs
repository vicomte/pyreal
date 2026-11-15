//! The character settings record: what is kept, sent back and flushed.

use super::*;
// ---------------------------------------------------------------------------------------------
// options.character-page.*
//
// **The counts here are read from the corpus index and never pinned.** Rather than a per-recording
// table of how many settings records each one sent and which header word each carried,
// `Outbound::count` answers the first from the index, and the second is a property of the bytes
// rather than of the corpus, so what is asserted is the fixed point over every recorded record
// rather than a table that a promoted recording moves.
// ---------------------------------------------------------------------------------------------

/// The ordered game action the settings record travels in, and the record's own message.
const CHARACTER_OPTIONS_EVENT: u32 = 0x01A1;

/// One recorded settings record: which recording sent it, the stamp of its envelope, and its body.
struct RecordedOptions {
    session: String,
    stamp: u32,
    body: Vec<u8>,
}

/// Every settings record the recorded clients sent, read out of the committed recordings.
///
/// **Why the payload is read here rather than through `Outbound::recorded`.** That reader answers
/// *which* message each client-to-server blob is and when it went, which is what a claim about
/// traffic needs; it does not carry the bytes, and the claim below is that the bytes come back
/// unchanged. The count it does answer is used as the cross-check, so the two readers have to agree
/// about how many there are. The missing bytes are a gap in the harness.
///
/// # Panics
/// Panics when a recording named by the index does not parse, which is a broken checkout.
fn recorded_option_records() -> Vec<RecordedOptions> {
    use pyreal_client_net::client_session::testing::{Corpus, Direction};

    let mut out: Vec<RecordedOptions> = Vec::new();
    for name in pyreal_client_net::client_session::testing::session_names() {
        let corpus = Corpus::load(name)
            .unwrap_or_else(|e| panic!("the recording {name} does not parse: {e}"))
            .unwrap_or_else(|| panic!("the decoded corpus has no recording {name}"));
        for b in &corpus.blobs {
            if b.dir != Direction::ClientToServer
                || pyreal_testkit::outbound::message_of(&b.payload) != Some(CHARACTER_OPTIONS_EVENT)
            {
                continue;
            }
            out.push(RecordedOptions {
                session: (*name).to_owned(),
                stamp: u32::from_le_bytes(b.payload[4..8].try_into().expect("a stamp")),
                body: b.payload[12..].to_vec(),
            });
        }
    }
    assert!(
        !out.is_empty(),
        "no recording carries a settings record; a scan that read nothing would pass over nothing"
    );
    // The cross-check, against the index rather than a table written here: for every recording
    // that sent one, the crate's own client-to-server reader has to agree about how many.
    //
    // It is asked only about those recordings because `Outbound::all` refuses a recording with no
    // client-to-server blob at all, and the corpus has one. That is a gap in the harness.
    let mut per_session: std::collections::BTreeMap<&str, usize> =
        std::collections::BTreeMap::new();
    for r in &out {
        *per_session.entry(r.session.as_str()).or_default() += 1;
    }
    for (name, n) in per_session {
        assert_eq!(
            pyreal_testkit::Outbound::count(name, CHARACTER_OPTIONS_EVENT),
            n,
            "{name}: the two readers disagree about how many settings records it carries"
        );
    }
    out
}

/// The one settings record a named recording sent.
fn the_record_of(session: &str) -> RecordedOptions {
    let mut it = recorded_option_records()
        .into_iter()
        .filter(|b| b.session == session);
    let first = it
        .next()
        .unwrap_or_else(|| panic!("{session} sent no settings record"));
    assert!(it.next().is_none(), "{session} sent more than one");
    first
}

/// Decode a recorded settings body into the record the client packed.
fn decode_module(body: &[u8]) -> pyreal_protocol::login::PlayerModule {
    use pyreal_protocol::Reader;
    let mut r = Reader::with_origin(body, 12);
    let m = pyreal_protocol::login::PlayerModule::read(&mut r)
        .expect("a recorded settings record decodes");
    r.expect_exhausted()
        .expect("the cursor lands on the end of the record");
    m
}

/// Re-encode one the way the client's own sender does, so the alignment origin is the blob's.
fn encode_module(m: &pyreal_protocol::login::PlayerModule) -> Vec<u8> {
    let mut w = pyreal_protocol::actions::action_body_writer();
    m.write(&mut w).expect("it encodes");
    w.into_inner()
}

/// A world whose settings are a recorded record, the way the shard's description gives it one.
fn world_with(module: &pyreal_protocol::login::PlayerModule) -> pyreal_client_model::World {
    let mut w = pyreal_client_model::World::new();
    w.player_system.apply_player_module(module);
    w
}

/// Every recorded settings record, taken in and sent back out, is the same bytes.
pub(super) fn the_settings_it_sends_back_are_byte_for_byte_the_recorded_ones() {
    use pyreal_client_model::Request;

    let records = recorded_option_records();
    let mut all_identical = true;
    let mut all_framed = true;
    let mut cleared = true;
    for record in &records {
        let module = decode_module(&record.body);
        let mut world = world_with(&module);
        let packed = world
            .player_system
            .client_packed_module()
            .expect("the record is kept");
        all_identical &= encode_module(&packed) == record.body;

        // ...and through the sender, framed: the whole blob including its envelope.
        let mut req = pyreal_client_model::RecordingRequests::default();
        assert!(
            world.player_system.save_to_server(&mut req, true),
            "a forced save always sends"
        );
        let [Request::CharacterOptionsEvent(m)] = req.0.as_slice() else {
            panic!(
                "{}: expected one settings record, got {:?}",
                record.session, req.0
            )
        };
        let framed = pyreal_protocol::actions::pack_action(record.stamp, m).expect("it frames");
        all_framed &= framed.len() > 12 && framed[12..] == record.body[..];
        cleared &= !world.player_system.is_dirty();
    }
    let how_many = records.len();

    let mut c = HeadlessClient::model();
    c.assert_behaviour("options.character-page.the-settings-it-sends-back-are-byte-for-byte-the-ones-a-real-client-sent", move |_| {
        how_many > 0 && all_identical && all_framed && cleared
    });
}

/// A deferred change reaches the shard on the frame's own timer, once.
pub(super) fn a_deferred_change_is_flushed_by_the_frame() {
    use pyreal_client_contract::view::{PlayerOption, UiRequest};
    use pyreal_client_model::Request;
    use pyreal_client_runtime::interaction::Interaction;
    use pyreal_primitives::{LocalTime, ServerTime};

    let store = pyreal_dat::testing::open_store().expect("the retail data files");
    let record = the_record_of("first-login-walk-jump");
    let mut objects = pyreal_client_runtime::objects::ObjectStream::default();
    objects
        .world
        .player_system
        .apply_player_module(&decode_module(&record.body));
    let mut inter = Interaction::default();

    inter.queue(
        Vec::new(),
        vec![UiRequest::SetPlayerOption(
            PlayerOption::DisplayTimeStamps,
            true,
        )],
    );
    assert!(inter
        .run_ui_requests(&mut objects.world, false, ServerTime(0.0))
        .is_empty());
    let deferred = objects.world.player_system.is_dirty() && inter.stats.player_modules_sent == 0;
    let _ = inter.take_pending_requests();

    // **Driven from the frame's own entry point and not from the method.** A scenario that called
    // the timer directly would survive deleting its call site in the frame: it structurally could
    // not see the change.
    let frame = |inter: &mut Interaction,
                 objects: &mut pyreal_client_runtime::objects::ObjectStream,
                 t: f64| {
        pyreal_client_runtime::interaction::use_time(
            inter,
            &store,
            None,
            objects,
            None,
            Vec::new(),
            false,
            (800, 600),
            LocalTime(t),
        );
    };

    frame(&mut inter, &mut objects, 479.0);
    let not_yet = inter.stats.player_modules_sent == 0;
    frame(&mut inter, &mut objects, 481.0);
    let flushed = inter.stats.player_modules_sent == 1;
    let one_message = matches!(
        inter.last_sent.as_slice(),
        [Request::CharacterOptionsEvent(_)]
    );
    frame(&mut inter, &mut objects, 9_999.0);
    let only_once = inter.stats.player_modules_sent == 1;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "options.character-page.a-deferred-change-reaches-the-shard-eight-minutes-later-and-once",
        move |_| deferred && not_yet && flushed && one_message && only_once,
    );
}

/// With no description nothing is sent and nothing is invented.
pub(super) fn with_no_description_nothing_is_sent() {
    use pyreal_client_contract::view::{PlayerOption, UiRequest};
    use pyreal_client_runtime::interaction::Interaction;
    use pyreal_primitives::ServerTime;

    // Nothing to send, and nothing composed out of the defaults.
    let mut world = pyreal_client_model::World::new();
    let nothing_kept = world.player_system.module.is_none();
    let mut req = pyreal_client_model::RecordingRequests::default();
    let sends_nothing = !world.player_system.save_to_server(&mut req, true) && req.0.is_empty();
    let flag_cleared = !world.player_system.is_dirty();

    // ...and the setting still moves locally, so the state is right the moment a description
    // arrives.
    let mut inter = Interaction::default();
    inter.queue(
        Vec::new(),
        vec![UiRequest::SetPlayerOption(PlayerOption::AutoTarget, false)],
    );
    let moved_locally = inter
        .run_ui_requests(&mut world, false, ServerTime(0.0))
        .is_empty()
        && !world.player_system.options.auto_target()
        && world.player_system.module.is_none();

    // And a record the shard shaped is narrowed to the shape this client sends: what a
    // description may carry and a settings record may not is dropped, with its gate.
    let record = the_record_of("first-login-walk-jump");
    let mut module = decode_module(&record.body);
    module.timestamp_format = Some("%H:%M".to_owned());
    module.spell_bars.truncate(5);
    module.option_flags = (module.option_flags & !0x0400) | 0x0080 | 0x0004;
    let shaped = world_with(&module);
    let packed = shaped
        .player_system
        .client_packed_module()
        .expect("the record is kept");
    let narrowed = packed.option_flags & 0x0080 == 0
        && packed.option_flags & 0x0004 == 0
        && packed.timestamp_format.is_none()
        && packed.spell_bars.len() == 8
        && packed.spell_bars[..5] == module.spell_bars[..]
        && !encode_module(&packed).is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "options.character-page.with-no-description-nothing-is-sent-and-nothing-is-invented",
        move |_| nothing_kept && sends_nothing && flag_cleared && moved_locally && narrowed,
    );
}
