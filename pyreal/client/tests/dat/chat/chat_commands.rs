//! Typed chat commands: every command the client handles reaches the wire (or its local effect)
//! from a submitted chat line, and none of them is answered with "That is not a valid
//! command.".
//!
//! `pyreal_client_model::cmd::table::INITIALIZE_COMMANDS` and `TURBINE_CHAT_COMMANDS` together
//! register the client's command words, by family:
//!
//! ```text
//! character  @lifestone @lif @ls  @marketplace @mar @mp  @hor @hr  @hom @hoa
//!            @pkarena @pka  @pklarena @pla  @pklite @pkl  @age  @birth  @die
//! comms      @chat  @notell  @index  @clist  @on  @off  @afk
//! consent    @consent  @permit
//! speech     @say @s
//! local      @speaker  @endurance  @emotes  @corpse
//! layout     @saveui @loadui (and the automatic pair)  @lockui  @title
//! squelch    @squelch @unsquelch  @filter @unfilter  @messagetypes
//! other      @fillcomps  @log  @loadfile  @version
//! ```
//!
//! The oracle for each wire body is written longhand here (a `u16` `PString` count, the
//! characters, padding to four; a raw dword otherwise), so the expectation never calls the encoder
//! under test.
//!
//! Each line is submitted as the main chat window's `UiRequest::ChatLine`.
//!
//! Fixture: a headless App with no link. The primary table compares the request the app routed
//! with its expected value, then feeds that expected value to a `Session` backed by
//! `MockTransport` to check its bytes; no datagram leaves the process. The corpus station reads
//! the three `fellowship-*` recordings. The commands that change what is drawn (`@render`, `@day`,
//! `@framerate`) are `rendering::render_and_day_commands`.

use crate::common::chat::{app_with, Hand};

use pyreal_client::app::App;
use pyreal_client_model::chat_cmd as cmd;
use pyreal_client_model::Request;
use pyreal_client_net::client_session::{
    testing::{Corpus, Direction, MockTransport},
    Session, SessionEvent,
};
use pyreal_client_runtime::requests::send_request;
use pyreal_primitives::{NetQueue, ObjectId};

fn app() -> App {
    app_with(|cfg| {
        crate::common::sim_app::new(cfg).expect("required retail DATs and simulated world")
    })
}

// ---------------------------------------------------------------------------------------------
// The byte oracle, longhand
// ---------------------------------------------------------------------------------------------

/// A packed string — a `u16` count, the characters, zero padding to four.
fn pstr(s: &str) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&u16::try_from(s.len()).expect("short").to_le_bytes());
    v.extend_from_slice(s.as_bytes());
    while v.len() % 4 != 0 {
        v.push(0);
    }
    v
}

fn dw(n: u32) -> Vec<u8> {
    n.to_le_bytes().to_vec()
}

/// Ordered-action framing: the `0xF7B1` header, the `OrderedActionHeader` stamp, the opcode, and
/// the body.
fn action(stamp: u32, opcode: u32, body: &[u8]) -> Vec<u8> {
    let mut v = vec![0xb1, 0xf7, 0x00, 0x00];
    v.extend_from_slice(&stamp.to_le_bytes());
    v.extend_from_slice(&opcode.to_le_bytes());
    v.extend_from_slice(body);
    v
}

// ---------------------------------------------------------------------------------------------
// The keyboard and pointer, driven as Windows drives them
// ---------------------------------------------------------------------------------------------

/// The opcode each `Request` this family builds carries, spelled out here rather than read off
/// `pyreal_protocol::Message::OPCODE` so the table and the codec cannot drift together.
fn opcode_of(r: &Request) -> u32 {
    match r {
        Request::PlayerOptionChanged(_) => 0x0005,
        Request::SetAfkMode(_) => 0x000F,
        Request::SetAfkMessage(_) => 0x0010,
        Request::TeleToPklArena(_) => 0x0026,
        Request::TeleToPkArena(_) => 0x0027,
        Request::ModifyGlobalSquelch(_) => 0x005B,
        Request::ModifyCharacterSquelch(_) => 0x0058,
        Request::ModifyAccountSquelch(_) => 0x0059,
        Request::TeleToLifestone(_) => 0x0063,
        Request::AddToChannel(_) => 0x0145,
        Request::RemoveFromChannel(_) => 0x0146,
        Request::ChannelList(_) => 0x0148,
        Request::ChannelIndex(_) => 0x0149,
        Request::QueryAge(_) => 0x01C2,
        Request::QueryBirth(_) => 0x01C4,
        Request::ClearPlayerConsentList(_) => 0x0216,
        Request::DisplayPlayerConsentList(_) => 0x0217,
        Request::RemoveFromPlayerConsentList(_) => 0x0218,
        Request::AddPlayerPermission(_) => 0x0219,
        Request::RemovePlayerPermission(_) => 0x021A,
        Request::TeleToHouse(_) => 0x0262,
        Request::TeleToMansion(_) => 0x0278,
        Request::Suicide(_) => 0x0279,
        Request::TeleToMarketplace(_) => 0x028D,
        Request::EnterPkLite(_) => 0x028F,
        other => panic!("unexpected request {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// 1. The acceptance — every wired command, typed
// ---------------------------------------------------------------------------------------------

///
/// Each typed line puts exactly the expected request in the outbox, byte for byte, and
/// `chat_commands_unimplemented` does not move: that counter is where a line that fell to the
/// catch-all lands, so a regression shows here rather than as a silent change of behaviour.
#[test]
fn every_wired_chat_command_reaches_the_wire_from_a_typed_line() {
    let mut app = app();
    let mut hand = Hand::new();
    let mut session = Session::new(MockTransport::new());

    // (typed line, the request the arm must build, the body bytes on the wire)
    let cases: Vec<(&str, Request, Vec<u8>)> = vec![
        // ---- character teleports: three names each, one opcode each ------------------------
        (
            "@lifestone",
            Request::TeleToLifestone(pyreal_protocol::combat::CharacterTeleToLifestone),
            Vec::new(),
        ),
        (
            "@ls",
            Request::TeleToLifestone(pyreal_protocol::combat::CharacterTeleToLifestone),
            Vec::new(),
        ),
        (
            "@marketplace",
            Request::TeleToMarketplace(pyreal_protocol::combat::CharacterTeleToMarketplace),
            Vec::new(),
        ),
        (
            "@mp",
            Request::TeleToMarketplace(pyreal_protocol::combat::CharacterTeleToMarketplace),
            Vec::new(),
        ),
        (
            "@hor",
            Request::TeleToHouse(pyreal_protocol::trade::HouseTeleToHouse),
            Vec::new(),
        ),
        (
            "@hr",
            Request::TeleToHouse(pyreal_protocol::trade::HouseTeleToHouse),
            Vec::new(),
        ),
        (
            "@hom",
            Request::TeleToMansion(pyreal_protocol::trade::HouseTeleToMansion),
            Vec::new(),
        ),
        (
            "@hoa",
            Request::TeleToMansion(pyreal_protocol::trade::HouseTeleToMansion),
            Vec::new(),
        ),
        // With no player object, lookup misses and all three arena aliases reach the same send arm.
        (
            "@pkarena",
            Request::TeleToPkArena(pyreal_protocol::combat::CharacterTeleToPkArena),
            Vec::new(),
        ),
        (
            "@pklarena",
            Request::TeleToPklArena(pyreal_protocol::combat::CharacterTeleToPklArena),
            Vec::new(),
        ),
        (
            "@pklite",
            Request::EnterPkLite(pyreal_protocol::combat::CharacterEnterPkLite),
            Vec::new(),
        ),
        // ---- the two personal queries, both with target 0 ----------------------------------
        (
            "@age",
            Request::QueryAge(pyreal_protocol::admin::CharacterQueryAge {
                target: ObjectId(0),
            }),
            dw(0),
        ),
        (
            "@birth",
            Request::QueryBirth(pyreal_protocol::admin::CharacterQueryBirth {
                target: ObjectId(0),
            }),
            dw(0),
        ),
        // ---- the two global-squelch toggles ------------------------------------------------
        (
            "@chat on",
            Request::ModifyGlobalSquelch(
                pyreal_protocol::comms::CommunicationModifyGlobalSquelch {
                    add: 0,
                    msg_type: cmd::GLOBAL_SQUELCH_TYPE_CHAT,
                },
            ),
            [dw(0), dw(cmd::GLOBAL_SQUELCH_TYPE_CHAT)].concat(),
        ),
        (
            "@chat off",
            Request::ModifyGlobalSquelch(
                pyreal_protocol::comms::CommunicationModifyGlobalSquelch {
                    add: 1,
                    msg_type: cmd::GLOBAL_SQUELCH_TYPE_CHAT,
                },
            ),
            [dw(1), dw(cmd::GLOBAL_SQUELCH_TYPE_CHAT)].concat(),
        ),
        (
            "@notell off",
            Request::ModifyGlobalSquelch(
                pyreal_protocol::comms::CommunicationModifyGlobalSquelch {
                    add: 0,
                    msg_type: cmd::GLOBAL_SQUELCH_TYPE_TELL,
                },
            ),
            [dw(0), dw(cmd::GLOBAL_SQUELCH_TYPE_TELL)].concat(),
        ),
        // Leave-chat and join-chat consume only the first argument. The general-chat option write
        // reports ordinal 35 to the option-change path, whose auto-save arm emits one 0x0005
        // immediately.
        (
            "@leave General ignored",
            Request::PlayerOptionChanged(
                pyreal_protocol::login::CharacterPlayerOptionChangedEvent {
                    option: 35,
                    value: 0,
                },
            ),
            [dw(35), dw(0)].concat(),
        ),
        (
            "@join general ignored",
            Request::PlayerOptionChanged(
                pyreal_protocol::login::CharacterPlayerOptionChangedEvent {
                    option: 35,
                    value: 1,
                },
            ),
            [dw(35), dw(1)].concat(),
        ),
        // The no-tell parser has no unknown-word refusal, unlike the chat-toggle parser:
        // anything but "off" is "on".
        (
            "@notell wibble",
            Request::ModifyGlobalSquelch(
                pyreal_protocol::comms::CommunicationModifyGlobalSquelch {
                    add: 1,
                    msg_type: cmd::GLOBAL_SQUELCH_TYPE_TELL,
                },
            ),
            [dw(1), dw(cmd::GLOBAL_SQUELCH_TYPE_TELL)].concat(),
        ),
        // ---- the four channel commands -----------------------------------------------------
        (
            "@index",
            Request::ChannelIndex(pyreal_protocol::comms::CommunicationChannelIndexRequest),
            Vec::new(),
        ),
        // The channel-routing command maps `fellowship-one-vassal` to `0x800`, not `0x2000`:
        // `0x2000` is `patron` in this chain, and the `0x2000` the command reference calls
        // "fellowship" belongs to talk-focus routing, which is a different table. The handler
        // reads this one.
        (
            "@clist fellowship",
            Request::ChannelList(pyreal_protocol::comms::CommunicationChannelListRequest {
                channel: 0x800,
            }),
            dw(0x800),
        ),
        (
            "@on fellowship",
            Request::AddToChannel(pyreal_protocol::comms::CommunicationAddToChannel {
                channel: 0x800,
            }),
            dw(0x800),
        ),
        (
            "@off patron",
            Request::RemoveFromChannel(pyreal_protocol::comms::CommunicationRemoveFromChannel {
                channel: 0x2000,
            }),
            dw(0x2000),
        ),
        // ---- @afk: the bare form is the `on` arm, and `msg` truncates and newline-terminates -
        (
            "@afk",
            Request::SetAfkMode(pyreal_protocol::comms::CommunicationSetAfkMode { afk: 1 }),
            dw(1),
        ),
        (
            "@afk msg back soon",
            Request::SetAfkMessage(pyreal_protocol::comms::CommunicationSetAfkMessage {
                message: "back soon\n".into(),
            }),
            pstr("back soon\n"),
        ),
        // ---- consent and permit ------------------------------------------------------------
        (
            "@consent who",
            Request::DisplayPlayerConsentList(
                pyreal_protocol::admin::CharacterDisplayPlayerConsentList,
            ),
            Vec::new(),
        ),
        (
            "@consent clear",
            Request::ClearPlayerConsentList(
                pyreal_protocol::admin::CharacterClearPlayerConsentList,
            ),
            Vec::new(),
        ),
        (
            "@consent remove Bob",
            Request::RemoveFromPlayerConsentList(
                pyreal_protocol::admin::CharacterRemoveFromPlayerConsentList { name: "Bob".into() },
            ),
            pstr("Bob"),
        ),
        (
            "@permit add Bob",
            Request::AddPlayerPermission(pyreal_protocol::admin::CharacterAddPlayerPermission {
                name: "Bob".into(),
            }),
            pstr("Bob"),
        ),
        // Name joining keeps the interior run of spaces and strips a leading `+`.
        (
            "@permit remove +Bob Smith",
            Request::RemovePlayerPermission(
                pyreal_protocol::admin::CharacterRemovePlayerPermission {
                    name: "Bob Smith".into(),
                },
            ),
            pstr("Bob Smith"),
        ),
    ];

    for (stamp, (line, want, body)) in (1u32..).zip(&cases) {
        let unimplemented = app.interaction().stats.chat_commands_unimplemented;
        let refused = app.interaction().stats.chat_commands_refused;
        let lines = app.interaction().stats.chat_lines_sent;
        hand.submit(&mut app, line);
        assert_eq!(
            app.interaction().stats.chat_lines_sent,
            lines + 1,
            "{line}: the keystrokes must reach UiRequest::ChatLine before anything else is judged"
        );
        assert_eq!(
            app.interaction().stats.chat_commands_unimplemented,
            unimplemented,
            "{line}: this is the counter a line that falls to the catch-all lands in"
        );
        assert_eq!(
            app.interaction().stats.chat_commands_refused,
            refused,
            "{line}: a wired command prints no refusal"
        );
        assert_eq!(
            app.interaction().last_sent.as_slice(),
            std::slice::from_ref(want),
            "{line}: exactly this request and nothing else"
        );
        assert!(
            send_request(&mut session, want),
            "{line}: the sender has an arm"
        );
        let packet = session
            .transport
            .sent
            .last()
            .expect("one datagram per request");
        assert_eq!(
            (packet.queue, packet.ordered),
            (NetQueue::Weenie, true),
            "{line}: `pyreal_protocol::opcodes` puts every one of these C2S rows on the Weenie queue \
             as an ordered game action"
        );
        assert_eq!(
            packet.payload,
            action(stamp, opcode_of(want), body),
            "{line}: the bytes"
        );
        assert!(app.frame());
        assert!(
            app.interaction().last_sent.is_empty(),
            "{line}: the next frame never repeats it"
        );
    }

    // The census: every distinct opcode the table above sends, plus join/leave's player-option
    // write.
    let mut opcodes: Vec<u32> = session
        .transport
        .sent
        .iter()
        .map(|p| u32::from_le_bytes(p.payload[8..12].try_into().expect("an opcode dword")))
        .collect();
    opcodes.sort_unstable();
    opcodes.dedup();
    assert_eq!(
        opcodes,
        vec![
            0x0005, 0x000F, 0x0010, 0x0026, 0x0027, 0x005B, 0x0063, 0x0145, 0x0146, 0x0148, 0x0149,
            0x01C2, 0x01C4, 0x0216, 0x0217, 0x0218, 0x0219, 0x021A, 0x0262, 0x0278, 0x028D, 0x028F,
        ],
        "twenty-two here; 0x0279 Suicide is behind the @die dialog and has its own test"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The local handlers, and the option write
// ---------------------------------------------------------------------------------------------

///
/// `@speaker`, `@endurance` and `@emotes` send nothing and each does its own local
/// thing; `@emotes` prints `emote_list_text`.
#[test]
fn the_local_commands_print_their_literal_and_send_nothing() {
    let mut app = app();
    let mut hand = Hand::new();

    let unimplemented = app.interaction().stats.chat_commands_unimplemented;
    for line in ["@speaker", "@endurance", "@emotes"] {
        hand.submit(&mut app, line);
        assert!(app.interaction().last_sent.is_empty(), "{line}: local only");
    }
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        unimplemented,
        "none of the three reaches the catch-all"
    );
    assert_eq!(
        app.interaction().stats.chat_command_lines,
        3,
        "@speaker, @endurance and @emotes each print one line"
    );
    // `@consent on|off` writes the accept-loot-permits option, then reports option ordinal 16.
    // Ordinal 16 is an auto-save option, so it leaves as a `0x0005` immediately.
    let sent = app.interaction().stats.option_changes_sent;
    hand.submit(&mut app, "@consent on");
    assert_eq!(app.interaction().stats.option_changes_sent, sent + 1);
    assert_eq!(
        app.interaction().last_sent.as_slice(),
        &[Request::PlayerOptionChanged(
            pyreal_protocol::login::CharacterPlayerOptionChangedEvent {
                option: u32::try_from(cmd::ACCEPT_LOOT_PERMITS_ORDINAL).unwrap(),
                value: 1,
            }
        )],
        "the option write and nothing else"
    );
    assert!(app.frame());
    // The equality guard makes a second `on` a no-op.
    hand.submit(&mut app, "@consent on");
    assert!(
        app.interaction().last_sent.is_empty(),
        "the equality guard means a repeated value sends nothing"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The corpus, pointed at the target and reporting an honest zero
// ---------------------------------------------------------------------------------------------

/// None of the typed commands' opcodes appears in the selected recordings, which is why the
/// bodies above are composed longhand rather than taken from a capture.
///
/// An empty result is not a negative result until the instrument is proved to be pointed at the
/// target, so this walks every decodable client-to-server `0xF7B1` in each available named
/// scenario, asserts that their aggregate exceeds one hundred actions, and prints the opcode
/// histogram of what it did find. Missing recordings and payloads shorter than twelve bytes do
/// not enter that aggregate.
#[test]
fn the_corpus_records_none_of_these() {
    let mut total = 0usize;
    let mut hits = 0usize;
    let wanted: Vec<u32> = vec![
        0x000F, 0x0010, 0x0026, 0x0027, 0x005B, 0x0063, 0x0145, 0x0146, 0x0148, 0x0149, 0x01C2,
        0x01C4, 0x0216, 0x0217, 0x0218, 0x0219, 0x021A, 0x0262, 0x0278, 0x0279, 0x028D, 0x028F,
    ];
    let mut seen: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    for scenario in [
        "fellowship-one-vassal",
        "fellowship-two-monarch",
        "fellowship-three-vassal",
    ] {
        let corpus = Corpus::shared(scenario);
        for blob in &corpus.blobs {
            if blob.dir != Direction::ClientToServer || blob.opcode != 0xf7b1 {
                continue;
            }
            if blob.payload.len() < 12 {
                continue;
            }
            total += 1;
            let op = u32::from_le_bytes(blob.payload[8..12].try_into().expect("dword"));
            *seen.entry(op).or_default() += 1;
            if wanted.contains(&op) {
                hits += 1;
            }
        }
    }
    assert!(
        total > 100,
        "the instrument must be proved to be looking: only {total} client game actions found"
    );
    println!(
        "{total} client 0xF7B1 game actions across three recordings, {} distinct opcodes",
        seen.len()
    );
    assert_eq!(
        hits, 0,
        "the selected corpus must contain no instances of these twenty-two opcodes"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. The static registry-to-WIRED allowlist comparison
// ---------------------------------------------------------------------------------------------

/// Squelch and unsquelch share one argument parser. The request split is preserved: ordinary names use `0x0058`
/// with object zero and a message type, while `-account` uses the shorter `0x0059` body.
#[test]
fn typed_squelch_commands_send_the_native_character_and_account_requests() {
    let mut app = app();
    let mut hand = Hand::new();
    let mut session = Session::new(MockTransport::new());
    let before_unimplemented = app.interaction().stats.chat_commands_unimplemented;

    let cases = [
        (
            "@squelch Baron Lark",
            Request::ModifyCharacterSquelch(
                pyreal_protocol::comms::CommunicationModifyCharacterSquelch {
                    add: 1,
                    character_id: ObjectId(0),
                    character_name: "Baron Lark".into(),
                    msg_type: 1,
                },
            ),
            [dw(1), dw(0), pstr("Baron Lark"), dw(1)].concat(),
        ),
        (
            "/unsquelch -Assessment +Baron Lark",
            Request::ModifyCharacterSquelch(
                pyreal_protocol::comms::CommunicationModifyCharacterSquelch {
                    add: 0,
                    character_id: ObjectId(0),
                    character_name: "Baron Lark".into(),
                    msg_type: 16,
                },
            ),
            [dw(0), dw(0), pstr("Baron Lark"), dw(16)].concat(),
        ),
        (
            "@squelch -AcCoUnT Baron Lark",
            Request::ModifyAccountSquelch(
                pyreal_protocol::comms::CommunicationModifyAccountSquelch {
                    add: 1,
                    character_name: "Baron Lark".into(),
                },
            ),
            [dw(1), pstr("Baron Lark")].concat(),
        ),
    ];
    for (index, (line, expected, body)) in cases.into_iter().enumerate() {
        hand.submit(&mut app, line);
        assert_eq!(
            app.interaction().last_sent.as_slice(),
            std::slice::from_ref(&expected),
            "{line}"
        );
        assert!(
            send_request(&mut session, &expected),
            "{line} has a production sender"
        );
        let packet = session.transport.sent.last().expect("one encoded request");
        assert_eq!((packet.queue, packet.ordered), (NetQueue::Weenie, true));
        assert_eq!(
            packet.payload,
            action(
                u32::try_from(index + 1).expect("three cases"),
                opcode_of(&expected),
                &body
            ),
            "{line}"
        );
    }
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        before_unimplemented
    );
    app.shutdown();
}

/// Filter and unfilter share the global-squelch modifier. They reuse the squelch category mapper but send the
/// global `0x005B` body: one add/remove dword followed by the selected `LogTextType`.
#[test]
fn typed_filter_commands_send_literal_global_squelch_requests_without_predicting_state() {
    let mut app = app();
    let mut hand = Hand::new();
    let mut session = Session::new(MockTransport::new());
    let before_unimplemented = app.interaction().stats.chat_commands_unimplemented;

    let cases = [
        (
            "@filter -Spellcasting",
            Request::ModifyGlobalSquelch(
                pyreal_protocol::comms::CommunicationModifyGlobalSquelch {
                    add: 1,
                    msg_type: pyreal_client_model::chat::text_type::SPELLCASTING,
                },
            ),
            pyreal_client_model::chat::text_type::SPELLCASTING,
        ),
        (
            "/unfilter -Assessment",
            Request::ModifyGlobalSquelch(
                pyreal_protocol::comms::CommunicationModifyGlobalSquelch {
                    add: 0,
                    msg_type: pyreal_client_model::chat::text_type::APPRAISAL,
                },
            ),
            pyreal_client_model::chat::text_type::APPRAISAL,
        ),
    ];
    for (index, (line, expected, ty)) in cases.into_iter().enumerate() {
        hand.submit(&mut app, line);
        assert_eq!(
            app.interaction().last_sent.as_slice(),
            std::slice::from_ref(&expected),
            "{line}"
        );
        assert!(
            app.objects().world.chat.squelch.global.is_empty(),
            "the request waits for authoritative 0x01F4 rather than predicting the filter"
        );
        assert!(send_request(&mut session, &expected));
        let packet = session.transport.sent.last().expect("one encoded request");
        assert_eq!((packet.queue, packet.ordered), (NetQueue::Weenie, true));
        let add = u32::from(line.starts_with("@filter"));
        assert_eq!(
            packet.payload,
            action(
                u32::try_from(index + 1).expect("two cases"),
                0x005B,
                &[dw(add), dw(ty)].concat()
            ),
            "{line} has the literal global-squelch body"
        );
    }
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        before_unimplemented
    );
    app.shutdown();
}

/// Every handler name in the command tables has an arm in `chat_command`: the sorted list of
/// wired handlers covers them all, and any name that leaves it must be deleted deliberately.
#[test]
fn the_handler_names_with_no_arm_are_exactly_these() {
    use pyreal_client_model::cmd::CommandHandler as H;
    let mut names: Vec<H> = pyreal_client_model::cmd::table::INITIALIZE_COMMANDS
        .iter()
        .chain(pyreal_client_model::cmd::table::TURBINE_CHAT_COMMANDS)
        .chain(pyreal_client_model::cmd::table::PYREAL_COMMANDS)
        .filter_map(|e| e.handler)
        .collect();
    names.sort_unstable();
    names.dedup();

    const WIRED: &[H] = &[
        H::Tell,
        H::Reply,
        H::Retell,
        H::ChannelShortcut,
        H::Friends,
        H::FriendsAdd,
        H::FriendsRemove,
        H::Guild,
        H::General,
        H::Trade,
        H::Lfg,
        H::Roleplay,
        H::Society,
        H::Olthoi,
        H::Emote,
        H::Loc,
        H::Allegiance,
        H::AllegianceHometown,
        H::AllegianceBroadcast,
        H::Motd,
        H::Lifestone,
        H::Marketplace,
        H::HouseRecall,
        H::MansionRecall,
        H::Pkarena,
        H::Pklarena,
        H::Pklite,
        H::Age,
        H::Birth,
        H::Die,
        H::Chat,
        H::Notell,
        H::Index,
        H::Clist,
        H::On,
        H::Off,
        H::Afk,
        H::Consent,
        H::Permit,
        H::Speaker,
        H::Endurance,
        H::Emotes,
        H::Say,
        H::Corpse,
        H::Join,
        H::Leave,
        H::House,
        H::Hslist,
        H::Help,
        H::Loadautoui,
        H::Loadui,
        H::Saveautoui,
        H::Saveui,
        H::Lockui,
        H::Squelch,
        H::Unsquelch,
        H::Filter,
        H::Unfilter,
        H::Fillcomps,
        H::Log,
        H::Loadfile,
        H::Framerate,
        H::Messagetypes,
        H::Version,
        H::Title,
        H::Day,
        H::Render,
        H::Tod,
        H::Weather,
        H::Clouds,
        H::Moon,
        H::Fog,
        H::Report,
        H::PerfCapture,
        H::Sepaku,
    ];

    let remaining: Vec<H> = names
        .iter()
        .copied()
        .filter(|n| !WIRED.contains(n))
        .collect();
    assert_eq!(
        remaining,
        Vec::<H>::new()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>(),
        "every registered handler in this census is listed in WIRED"
    );
}

/// PSR qualities add one bare F7CC query after the local version line.
#[test]
fn typed_version_queries_the_server_only_for_the_three_native_psr_boolean_qualities() {
    let mut app = app();
    let mut hand = Hand::new();
    app.probe_mut().objects_mut().world.seed_player_desc(
        ObjectId(0x5000_000A),
        pyreal_client_model::Qualities::default(),
    );

    let apply_bool = |app: &mut App, sequence, property_id, value| {
        let update = pyreal_protocol::qualities::QualitiesPrivateUpdateBool(
            pyreal_protocol::qualities::PrivateUpdate {
                sequence,
                property_id,
                value,
            },
        );
        let mut blob = pyreal_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_BOOL
            .0
            .to_le_bytes()
            .to_vec();
        blob.extend(pyreal_protocol::write_body(&update).expect("private Boolean quality update"));
        let event = SessionEvent::UiEvent {
            opcode: pyreal_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_BOOL,
            blob,
        };
        app.apply_hud_events(std::slice::from_ref(&event));
        assert_eq!(
            app.objects()
                .world
                .player_qualities()
                .expect("player descriptor")
                .inq_bool(property_id),
            value != 0,
            "the encoded private update reached the production quality store"
        );
    };

    // A PlayerDesc without any of the three bits keeps the ordinary local-only path.
    hand.submit(&mut app, "/version");
    assert!(
        app.interaction().last_sent.is_empty(),
        "an ordinary player sends no query"
    );
    let requests_before = app.interaction().stats.chat_command_requests;

    // The PSR-lead check asks 0x2C then 0x2D; the PSR check asks 0x61 after a false lead.
    for property_id in [0x2C, 0x2D, 0x61] {
        apply_bool(&mut app, 1, property_id, 1);
        hand.submit(&mut app, "/version");
        let [request @ Request::AdminGetServerVersion(
            pyreal_protocol::admin::AdminSendAdminGetServerVersion,
        )] = app.interaction().last_sent.as_slice()
        else {
            panic!(
                "Boolean {property_id:#04X} emitted {:?}",
                app.interaction().last_sent
            )
        };

        let mut session = Session::new(MockTransport::new());
        assert!(
            send_request(&mut session, request),
            "the bare F7CC sender is connected"
        );
        let [packet] = session.transport.sent.as_slice() else {
            panic!("one query produced {:?}", session.transport.sent)
        };
        assert_eq!((packet.queue, packet.ordered), (NetQueue::Control, false));
        assert_eq!(
            packet.payload,
            [0xCC, 0xF7, 0, 0],
            "the oracle is independent of the codec"
        );
        assert_eq!(
            session.next_action_stamp(),
            1,
            "a bare Control send consumes no action stamp"
        );

        // Removing this bit leaves none of the three set, so the next typed command is local.
        apply_bool(&mut app, 2, property_id, 0);
        hand.submit(&mut app, "/version");
        assert!(
            app.interaction().last_sent.is_empty(),
            "false {property_id:#04X} is not PSR"
        );
    }
    assert_eq!(
        app.interaction().stats.chat_command_requests,
        requests_before + 3
    );

    // The argument refusal precedes the admin-status check and therefore sends nothing even for a PSR.
    apply_bool(&mut app, 3, 0x61, 1);
    hand.submit(&mut app, "/version ignored");
    assert!(
        app.interaction().last_sent.is_empty(),
        "the argument refusal cannot send F7CC"
    );
    assert_eq!(
        app.interaction().stats.chat_command_requests,
        requests_before + 3
    );
    app.shutdown();
}
