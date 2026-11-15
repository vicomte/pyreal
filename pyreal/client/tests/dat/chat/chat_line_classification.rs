//! A chat line has an owner: `UiRequest::ChatLine` hands the raw string and window id to the
//! chat-command interpreter, which speaks plain text and forwards an unknown `@` command verbatim.
//!
//! Fixture: the retail dats; an `Interaction` with no shard link.

use crate::common::client_dir;

/// A typed line has an owner and reaches the wire slot.
///
/// The request hands the raw string and window id to the chat-command interpreter. This asserts
/// that the `UiRequest::ChatLine` request has an owner, nonempty input reaches classification,
/// and speech or an unknown command reaches the wire slot. Empty input creates no history,
/// refusal, or wire request.
///
/// The two classifications are opposite in an important way: plain text is speech, and an
/// unrecognised `@` command is forwarded to the server verbatim, `@` and all, with no allow-list,
/// which permits server-defined commands such as `@acehelp`. A locally handled verb is neither,
/// and must not be spoken.
///
/// Falsified by deleting the `UiRequest::ChatLine` arm from `run_ui_requests`: every line comes
/// back unowned and nothing reaches the wire slot.
#[test]
fn a_chat_line_is_classified_and_offered_to_the_wire() {
    let store = pyreal_dat::RetailDatStore::open_dir(&client_dir()).expect("the retail dats");
    for (line, wire, classified) in [
        ("hello world", true, 1), // Chat { Say } -> Communication_Talk
        ("@acehelp", true, 1),    // ForwardVerbatim -> Communication_Talk, '@' and all
        ("@help", false, 1),      // a registered handler: local, and never spoken
        ("", false, 0),           // empty input stops before command classification
    ] {
        let mut inter = pyreal_client_runtime::interaction::Interaction::new();
        let mut objects = pyreal_client_runtime::objects::ObjectStream::new();
        inter.queue(
            Vec::new(),
            vec![pyreal_client_contract::view::UiRequest::ChatLine {
                text: line.to_owned(),
                window: 8,
            }],
        );
        let (unowned, _) = pyreal_client_runtime::interaction::use_time(
            &mut inter,
            &store,
            None,
            &mut objects,
            None,
            Vec::new(),
            false,
            (800, 600),
            pyreal_primitives::LocalTime(1.0),
        );
        assert!(
            unowned.is_empty(),
            "{line:?} is nobody's request: {unowned:?}"
        );
        assert_eq!(
            inter.stats.chat_lines_sent, classified,
            "{line:?}: nonempty input reaches chat-command classification"
        );
        if line.is_empty() {
            assert!(
                objects.world.chat.entries.is_empty(),
                "no entry history is created"
            );
            assert_eq!(inter.stats.chat_commands_refused, 0);
            assert_eq!(inter.stats.chat_commands_unimplemented, 0);
        }
        // With no session the wire slot counts it undeliverable, which is the assertion that a
        // `pyreal_client_model::Request` was built and offered at all.
        assert_eq!(
            inter.stats.requests_undeliverable,
            u64::from(wire),
            "{line:?}: {} request(s) reached the wire slot, wanted {}",
            inter.stats.requests_undeliverable,
            u64::from(wire)
        );
    }
}
