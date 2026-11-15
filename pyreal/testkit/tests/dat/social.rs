//! Allegiance: what the shard's roster opens, and what the typed allegiance commands send.
//!
//! Every scenario here opens the retail dats under `$PYREAL_TEST_DAT_DIR`, and **this binary must run
//! serially**.
//!
//! The model-only social claims -- the tree walk and the talk-focus mask -- are in
//! `tests/cpu/social.rs`.
//!
//! **The populated rosters here are synthesised and say so.** There is no recording of a character
//! inside an allegiance that has members; the one exception is the recorded roster the typed
//! commands run against.

use pyreal_primitives::ObjectId;
use pyreal_testkit::adapters_social::{allegiance_answer, allegiance_member};
use pyreal_testkit::{ClientSpec, HeadlessClient, Inbound, Player};

const ME: ObjectId = ObjectId(0x5000_0001);
const BOB: ObjectId = ObjectId(0x5000_0009);
const CID: ObjectId = ObjectId(0x5000_000B);

/// A client with its shell up and `ME` as the player.
///
/// The player id has to be set before the answer arrives: every walk is keyed on it, and a client
/// that had none would read the roster as somebody else's.
fn a_client_in_gameplay() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    assert!(
        c.world_mut().set_player(ME),
        "the scenario's own player id takes"
    );
    c
}

/// Deliver `answer` and run a few frames.
fn answer(c: &mut HeadlessClient, a: &pyreal_protocol::social::AllegianceUpdate) {
    c.when(Inbound::message(a));
    c.tick(3);
}

/// Whether each of the three allegiance channels is open.
fn channels(c: &HeadlessClient) -> (bool, bool, bool) {
    use pyreal_client_model::chat::TalkFocus;
    let chat = &c.view().world().chat;
    (
        chat.is_talk_focus_enabled(TalkFocus::Patron),
        chat.is_talk_focus_enabled(TalkFocus::Monarch),
        chat.is_talk_focus_enabled(TalkFocus::Vassals),
    )
}

pyreal_testkit::scenarios! {
    scenario_an_online_relative_opens_that_allegiance_channel => an_online_relative_opens_that_allegiance_channel ["allegiance.channels.an-online-patron-monarch-or-vassal-opens-that-channel"],
    scenario_every_typed_allegiance_command_sends_its_own_message => every_typed_allegiance_command_sends_its_own_message ["allegiance.commands.every-typed-allegiance-command-sends-its-own-message"],
    scenario_a_refused_allegiance_line_prints_and_sends_nothing => a_refused_allegiance_line_prints_and_sends_nothing ["allegiance.commands.a-refused-line-is-printed-where-the-player-reads-it-and-sends-nothing"],
    scenario_listening_is_an_option_and_broadcasting_is_a_channel => listening_is_an_option_and_broadcasting_is_a_channel ["allegiance.commands.listening-is-a-local-option-and-broadcasting-is-a-channel"],
}

// =============================================================================================
// 3. allegiance.channels.an-online-patron-monarch-or-vassal-opens-that-channel
// =============================================================================================

/// Who is online decides which of the three allegiance channels the player may pick.
pub fn an_online_relative_opens_that_allegiance_channel() {
    let mut c = a_client_in_gameplay();
    let shut_to_start = channels(&c) == (false, false, false);

    // Bob is monarch and the player's patron, and he is offline; Cid is the player's vassal and is
    // online. Only the vassal channel opens.
    answer(
        &mut c,
        &allegiance_answer(
            "The Hand of Dereth",
            3,
            1,
            vec![
                (None, allegiance_member(BOB, "Bob", 5, false)),
                (Some(BOB), allegiance_member(ME, "Lark", 3, true)),
                (Some(ME), allegiance_member(CID, "Cid", 1, true)),
            ],
        ),
    );
    let only_vassals = channels(&c) == (false, false, true);

    // Bob logs in: he is the patron and the monarch, and he is not the player, so both open.
    answer(
        &mut c,
        &allegiance_answer(
            "The Hand of Dereth",
            3,
            1,
            vec![
                (None, allegiance_member(BOB, "Bob", 5, true)),
                (Some(BOB), allegiance_member(ME, "Lark", 3, true)),
                (Some(ME), allegiance_member(CID, "Cid", 1, true)),
            ],
        ),
    );
    let all_three = channels(&c) == (true, true, true);

    // Everybody logs out again, and all three close through the same path.
    answer(
        &mut c,
        &allegiance_answer(
            "The Hand of Dereth",
            3,
            1,
            vec![
                (None, allegiance_member(BOB, "Bob", 5, false)),
                (Some(BOB), allegiance_member(ME, "Lark", 3, true)),
                (Some(ME), allegiance_member(CID, "Cid", 1, false)),
            ],
        ),
    );
    let shut_again = channels(&c) == (false, false, false);

    c.assert_behaviour(
        "allegiance.channels.an-online-patron-monarch-or-vassal-opens-that-channel",
        move |_| shut_to_start && only_vassals && all_three && shut_again,
    );
    c.shutdown();
}

/// The one **recorded** roster with members in it.
const ROSTER_SESSION: &str = "fellowship-one-vassal";
const ROSTER_BLOB: usize = 351;

/// The recorded character of the roster recording.
const RECORDED_ME: ObjectId = ObjectId(0x5000_001E);

// =============================================================================================
// 17-19. allegiance.commands.*
// =============================================================================================
//
// **None of these has a button.** Every one of the allegiance commands is reached from a typed
// line and from nowhere else in the interface, so the line is the gesture and these scenarios
// say it, as the main chat window does. There is no recorded body for any of them -- nobody typed one while the recordings were
// being made, and saying so is more useful than pretending otherwise -- so the bytes below are
// composed here longhand and compared with what the production sender makes.

use pyreal_client_model::allegiance_cmd as cmd;
use pyreal_client_model::Request;
use pyreal_protocol::social as wire;

/// A length in characters, the characters, and zero padding to the next four-byte boundary -- the
/// layout a recorded client-to-server string in the corpus shows the retail client using.
fn pstr(s: &str) -> Vec<u8> {
    let mut v = u16::try_from(s.len())
        .expect("short")
        .to_le_bytes()
        .to_vec();
    v.extend_from_slice(s.as_bytes());
    while v.len() % 4 != 0 {
        v.push(0);
    }
    v
}

fn dw(n: u32) -> Vec<u8> {
    n.to_le_bytes().to_vec()
}

/// The framing a game action goes out in: the ordered-action header, the stamp the client
/// allocated, then the opcode and the body.
fn action(stamp: u32, opcode: u32, body: &[u8]) -> Vec<u8> {
    let mut v = vec![0xb1, 0xf7, 0x00, 0x00];
    v.extend_from_slice(&stamp.to_le_bytes());
    v.extend_from_slice(&opcode.to_le_bytes());
    v.extend_from_slice(body);
    v
}

/// A client in the world as the recorded character, with the recorded roster applied, ready to be
/// typed at.
///
/// The roster is the corpus's, so every line below is typed by a character who really is in an
/// allegiance -- and **none** of the commands reads it, which is worth having a real one for: a
/// client that resolved names against the roster would silently drop every name that is not one of
/// the player's own vassals.
fn a_client_to_type_at() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    assert!(
        c.world_mut().set_player(RECORDED_ME),
        "the recorded character is this client's"
    );
    c.when(Inbound::from_corpus(
        ROSTER_SESSION,
        ROSTER_BLOB..ROSTER_BLOB + 1,
    ));
    c.tick(1);
    c
}

/// The main chat window, which is where a typed line comes from.
const MAIN_WINDOW: u32 = 8;

/// Say `line` from the main chat window, and run the frame that takes it.
fn say(c: &mut HeadlessClient, line: &str) {
    c.when(Player::ui(pyreal_client_contract::UiRequest::ChatLine {
        text: line.to_owned(),
        window: MAIN_WINDOW,
    }))
    .tick(1);
}

/// The lines the client's own scroll is holding, taken between the frame that added them and the
/// one that would drain them.
fn scroll_lines(c: &mut HeadlessClient) -> Vec<String> {
    c.app_mut()
        .apply_hud_events(&[])
        .into_iter()
        .map(|m| m.body)
        .collect()
}

/// Which message each request is, so that a line that built the wrong one is a failure and not a
/// silent pass. It is written out rather than read off the request, because a test that reads a
/// constant back through the symbol that wrote it cannot detect a wrong one.
fn opcode_of(r: &Request) -> u32 {
    match r {
        Request::AllegianceQueryName(_) => 0x0030,
        Request::AllegianceClearName(_) => 0x0031,
        Request::AllegianceSetName(_) => 0x0033,
        Request::AllegianceSetOfficer(_) => 0x003B,
        Request::AllegianceSetOfficerTitle(_) => 0x003C,
        Request::AllegianceListOfficerTitles(_) => 0x003D,
        Request::AllegianceClearOfficerTitles(_) => 0x003E,
        Request::AllegianceDoLockAction(_) => 0x003F,
        Request::AllegianceSetApprovedVassal(_) => 0x0040,
        Request::AllegianceChatGag(_) => 0x0041,
        Request::AllegianceDoHouseAction(_) => 0x0042,
        Request::AllegianceSetMotd(_) => 0x0254,
        Request::AllegianceQueryMotd(_) => 0x0255,
        Request::AllegianceClearMotd(_) => 0x0256,
        Request::AllegianceBreakAllegianceBoot(_) => 0x0277,
        Request::AllegianceInfoRequest(_) => 0x027B,
        Request::AllegianceChatBoot(_) => 0x02A0,
        Request::AllegianceAddBan(_) => 0x02A1,
        Request::AllegianceRemoveBan(_) => 0x02A2,
        Request::AllegianceListBans(_) => 0x02A3,
        Request::AllegianceRemoveOfficer(_) => 0x02A5,
        Request::AllegianceListOfficers(_) => 0x02A6,
        Request::AllegianceClearOfficers(_) => 0x02A7,
        Request::AllegianceRecallHometown(_) => 0x02AB,
        other => panic!("not an allegiance command request: {other:?}"),
    }
}

/// One typed line per command, each putting its own message in the outbox with its own bytes.
#[allow(clippy::too_many_lines)]
pub fn every_typed_allegiance_command_sends_its_own_message() {
    let mut c = a_client_to_type_at();
    let mut session = pyreal_client_net::client_session::Session::new(
        pyreal_client_net::client_session::testing::MockTransport::new(),
    );

    // (the typed line, the message it must build, the body that must go on the wire)
    let cases: Vec<(&str, Request, Vec<u8>)> = vec![
        (
            "@allegiance name",
            Request::AllegianceQueryName(wire::AllegianceQueryAllegianceName),
            Vec::new(),
        ),
        (
            "@allegiance name set The Hand",
            Request::AllegianceSetName(wire::AllegianceSetAllegianceName {
                name: "The Hand".into(),
            }),
            pstr("The Hand"),
        ),
        (
            "@allegiance name clear",
            Request::AllegianceClearName(wire::AllegianceClearAllegianceName),
            Vec::new(),
        ),
        (
            "@motd",
            Request::AllegianceQueryMotd(wire::AllegianceQueryMotd),
            Vec::new(),
        ),
        (
            "@motd set Be excellent",
            Request::AllegianceSetMotd(wire::AllegianceSetMotd {
                motd: "Be excellent".into(),
            }),
            pstr("Be excellent"),
        ),
        (
            "@motd clear",
            Request::AllegianceClearMotd(wire::AllegianceClearMotd),
            Vec::new(),
        ),
        (
            "@allegiance motd",
            Request::AllegianceQueryMotd(wire::AllegianceQueryMotd),
            Vec::new(),
        ),
        (
            "@allegiance officer",
            Request::AllegianceListOfficers(wire::AllegianceListAllegianceOfficers),
            Vec::new(),
        ),
        (
            "@allegiance officer list",
            Request::AllegianceListOfficers(wire::AllegianceListAllegianceOfficers),
            Vec::new(),
        ),
        (
            "@allegiance officer clear",
            Request::AllegianceClearOfficers(wire::AllegianceClearAllegianceOfficers),
            Vec::new(),
        ),
        (
            "@allegiance officer add 2 Bob",
            Request::AllegianceSetOfficer(wire::AllegianceSetAllegianceOfficer {
                name: "Bob".into(),
                level: 2,
            }),
            [pstr("Bob"), dw(2)].concat(),
        ),
        (
            "@allegiance officer set 3 Bob",
            Request::AllegianceSetOfficer(wire::AllegianceSetAllegianceOfficer {
                name: "Bob".into(),
                level: 3,
            }),
            [pstr("Bob"), dw(3)].concat(),
        ),
        (
            "@allegiance officer remove Bob",
            Request::AllegianceRemoveOfficer(wire::AllegianceRemoveAllegianceOfficer {
                name: "Bob".into(),
            }),
            pstr("Bob"),
        ),
        (
            "@allegiance title",
            Request::AllegianceListOfficerTitles(wire::AllegianceListAllegianceOfficerTitles),
            Vec::new(),
        ),
        (
            "@allegiance title clear",
            Request::AllegianceClearOfficerTitles(wire::AllegianceClearAllegianceOfficerTitles),
            Vec::new(),
        ),
        // The level is **first** on the wire here, where the officer message above puts the name
        // first. The asymmetry is the client's own and is kept rather than tidied.
        (
            "@allegiance title set 1 Speaker",
            Request::AllegianceSetOfficerTitle(wire::AllegianceSetAllegianceOfficerTitle {
                level: 1,
                title: "Speaker".into(),
            }),
            [dw(1), pstr("Speaker")].concat(),
        ),
        (
            "@allegiance ban list",
            Request::AllegianceListBans(wire::AllegianceListAllegianceBans),
            Vec::new(),
        ),
        (
            "@allegiance ban add Bob",
            Request::AllegianceAddBan(wire::AllegianceAddAllegianceBan { name: "Bob".into() }),
            pstr("Bob"),
        ),
        (
            "@allegiance ban remove Bob",
            Request::AllegianceRemoveBan(wire::AllegianceRemoveAllegianceBan {
                name: "Bob".into(),
            }),
            pstr("Bob"),
        ),
        (
            "@allegiance lock",
            Request::AllegianceDoLockAction(wire::AllegianceDoAllegianceLockAction {
                action: cmd::lock_action::CHECK,
            }),
            dw(4),
        ),
        (
            "@allegiance lock on",
            Request::AllegianceDoLockAction(wire::AllegianceDoAllegianceLockAction {
                action: cmd::lock_action::ON,
            }),
            dw(2),
        ),
        (
            "@allegiance lock off",
            Request::AllegianceDoLockAction(wire::AllegianceDoAllegianceLockAction {
                action: cmd::lock_action::OFF,
            }),
            dw(1),
        ),
        (
            "@allegiance lock toggle",
            Request::AllegianceDoLockAction(wire::AllegianceDoAllegianceLockAction {
                action: cmd::lock_action::TOGGLE,
            }),
            dw(3),
        ),
        (
            "@allegiance lock bypass",
            Request::AllegianceDoLockAction(wire::AllegianceDoAllegianceLockAction {
                action: cmd::lock_action::CHECK_APPROVED,
            }),
            dw(5),
        ),
        (
            "@allegiance lock bypass clear",
            Request::AllegianceDoLockAction(wire::AllegianceDoAllegianceLockAction {
                action: cmd::lock_action::CLEAR_APPROVED,
            }),
            dw(6),
        ),
        (
            "@allegiance lock bypass Bob",
            Request::AllegianceSetApprovedVassal(wire::AllegianceSetAllegianceApprovedVassal {
                name: "Bob".into(),
            }),
            pstr("Bob"),
        ),
        (
            "@allegiance house",
            Request::AllegianceDoHouseAction(wire::AllegianceDoAllegianceHouseAction {
                action: cmd::house_action::HELP,
            }),
            dw(1),
        ),
        (
            "@allegiance house guest open",
            Request::AllegianceDoHouseAction(wire::AllegianceDoAllegianceHouseAction {
                action: cmd::house_action::GUEST_OPEN,
            }),
            dw(2),
        ),
        (
            "@allegiance house guest close",
            Request::AllegianceDoHouseAction(wire::AllegianceDoAllegianceHouseAction {
                action: cmd::house_action::GUEST_CLOSE,
            }),
            dw(3),
        ),
        (
            "@allegiance house storage open",
            Request::AllegianceDoHouseAction(wire::AllegianceDoAllegianceHouseAction {
                action: cmd::house_action::STORAGE_OPEN,
            }),
            dw(4),
        ),
        (
            "@allegiance house storage close",
            Request::AllegianceDoHouseAction(wire::AllegianceDoAllegianceHouseAction {
                action: cmd::house_action::STORAGE_CLOSE,
            }),
            dw(5),
        ),
        (
            "@allegiance info Bob",
            Request::AllegianceInfoRequest(wire::AllegianceInfoRequest { name: "Bob".into() }),
            pstr("Bob"),
        ),
        (
            "@ah",
            Request::AllegianceRecallHometown(wire::AllegianceRecallAllegianceHometown),
            Vec::new(),
        ),
        (
            "@alh",
            Request::AllegianceRecallHometown(wire::AllegianceRecallAllegianceHometown),
            Vec::new(),
        ),
        (
            "@allegiance hometown",
            Request::AllegianceRecallHometown(wire::AllegianceRecallAllegianceHometown),
            Vec::new(),
        ),
        (
            "@allegiance chat gag Bob",
            Request::AllegianceChatGag(wire::AllegianceChatGag {
                name: "Bob".into(),
                gagged: 1,
            }),
            [pstr("Bob"), dw(1)].concat(),
        ),
        (
            "@allegiance chat ungag Bob",
            Request::AllegianceChatGag(wire::AllegianceChatGag {
                name: "Bob".into(),
                gagged: 0,
            }),
            [pstr("Bob"), dw(0)].concat(),
        ),
        (
            "@allegiance chat kick Bob, being rude",
            Request::AllegianceChatBoot(wire::AllegianceChatBoot {
                name: "Bob".into(),
                reason: "being rude".into(),
            }),
            [pstr("Bob"), pstr("being rude")].concat(),
        ),
        (
            "@allegiance ch kick Bob",
            Request::AllegianceChatBoot(wire::AllegianceChatBoot {
                name: "Bob".into(),
                reason: cmd::NO_REASON_GIVEN.into(),
            }),
            [pstr("Bob"), pstr(cmd::NO_REASON_GIVEN)].concat(),
        ),
        (
            "@allegiance boot Bob",
            Request::AllegianceBreakAllegianceBoot(wire::AllegianceBreakAllegianceBoot {
                name: "Bob".into(),
                account_boot: 0,
            }),
            [pstr("Bob"), dw(0)].concat(),
        ),
        (
            "@allegiance boot -account Bob",
            Request::AllegianceBreakAllegianceBoot(wire::AllegianceBreakAllegianceBoot {
                name: "Bob".into(),
                account_boot: 1,
            }),
            [pstr("Bob"), dw(1)].concat(),
        ),
    ];

    let mut every_line_holds = true;
    for (stamp, (line, want, body)) in (1_u32..).zip(&cases) {
        let (unimplemented, refused, lines) = {
            let s = &c.view().expect_app().interaction().stats;
            (
                s.chat_commands_unimplemented,
                s.chat_commands_refused,
                s.chat_lines_sent,
            )
        };
        say(&mut c, line);
        let reached_the_client = {
            let s = &c.view().expect_app().interaction().stats;
            // The keystrokes have to become a chat line before anything else is judged, and a
            // wired command neither falls to the catch-all nor prints a refusal.
            s.chat_lines_sent == lines + 1
                && s.chat_commands_unimplemented == unimplemented
                && s.chat_commands_refused == refused
        };
        let built =
            c.view().expect_app().interaction().last_sent.as_slice() == std::slice::from_ref(want);

        // The wire: one send, one stamp, and the body composed above rather than by the writer
        // being asserted over. **Nothing leaves this process**; the transport is a mock.
        let sent = pyreal_client_runtime::requests::send_request(&mut session, want);
        let packet = session
            .transport
            .sent
            .last()
            .expect("one datagram per request");
        let framed = (packet.queue, packet.ordered) == (pyreal_primitives::NetQueue::Weenie, true)
            && packet.payload == action(stamp, opcode_of(want), body);

        c.tick(1);
        let not_repeated = c.view().expect_app().interaction().last_sent.is_empty();

        assert!(
            reached_the_client && built && sent && framed && not_repeated,
            "{line}: reached={reached_the_client} built={built} sent={sent} framed={framed} \
             once={not_repeated}; last_sent={:?}",
            c.view().expect_app().interaction().last_sent
        );
        every_line_holds &= reached_the_client && built && sent && framed && not_repeated;
    }

    // And between them the lines cover every distinct allegiance message there is a command for,
    // counted off the datagrams rather than off the table.
    let mut opcodes: Vec<u32> = session
        .transport
        .sent
        .iter()
        .map(|p| u32::from_le_bytes(p.payload[8..12].try_into().expect("an opcode dword")))
        .collect();
    opcodes.sort_unstable();
    opcodes.dedup();
    let distinct = opcodes.len();
    let counted = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .allegiance_command_requests
        == u32::try_from(cases.len()).expect("small");

    c.assert_behaviour(
        "allegiance.commands.every-typed-allegiance-command-sends-its-own-message",
        move |_| every_line_holds && distinct == 24 && counted,
    );
    c.shutdown();
}

/// A mistyped line is answered and sends nothing -- an arm that sent on every line would satisfy
/// the table above and empty a monarch's officer list on a typo.
pub fn a_refused_allegiance_line_prints_and_sends_nothing() {
    let mut c = a_client_to_type_at();
    let mut every_refusal_holds = true;

    for (line, want) in [
        // An unknown sub-command, and a bare command, both reach the client's own hint.
        ("@allegiance", cmd::PLEASE_SEE_HELP_ALLEGIANCE),
        ("@allegiance wibble", cmd::PLEASE_SEE_HELP_ALLEGIANCE),
        ("@allegiance name wibble", cmd::PLEASE_SEE_HELP_ALLEGIANCE),
        (
            "@allegiance house wibble open",
            cmd::PLEASE_SEE_HELP_ALLEGIANCE,
        ),
        // Four commands share one answer when the name they need is missing.
        ("@allegiance info", cmd::PLEASE_SPECIFY_AN_ACTUAL_NAME),
        ("@allegiance boot", cmd::PLEASE_SPECIFY_AN_ACTUAL_NAME),
        ("@allegiance ban add", cmd::PLEASE_SPECIFY_AN_ACTUAL_NAME),
        ("@allegiance chat gag", cmd::PLEASE_SPECIFY_AN_ACTUAL_NAME),
        (
            "@allegiance officer remove",
            cmd::PLEASE_SPECIFY_AN_ALLEGIANCE_MEMBER,
        ),
        (
            "@allegiance officer set 9 Bob",
            cmd::PLEASE_SPECIFY_A_VALID_OFFICER_LEVEL_LONG,
        ),
        (
            "@allegiance title set 0 x",
            cmd::PLEASE_SPECIFY_A_VALID_OFFICER_LEVEL,
        ),
        ("@ah now", cmd::THIS_COMMAND_TAKES_NO_ARGUMENTS),
    ] {
        let before = c
            .view()
            .expect_app()
            .interaction()
            .stats
            .allegiance_command_requests;
        say(&mut c, line);
        let silent = {
            let i = c.view().expect_app().interaction();
            i.last_sent.is_empty()
                && i.stats.allegiance_command_requests == before
                // Refused by the command itself, not dropped for want of an arm.
                && i.stats.chat_commands_unimplemented == 0
        };
        let printed = scroll_lines(&mut c)
            .iter()
            .any(|t| t.trim_end() == want.trim_end());
        assert!(
            silent && printed,
            "{line}: silent={silent} printed={printed}"
        );
        every_refusal_holds &= silent && printed;
        c.tick(1);
    }

    c.assert_behaviour(
        "allegiance.commands.a-refused-line-is-printed-where-the-player-reads-it-and-sends-nothing",
        move |_| every_refusal_holds,
    );
    c.shutdown();
}

/// The two lines in this family that are not allegiance messages at all.
pub fn listening_is_an_option_and_broadcasting_is_a_channel() {
    let mut c = a_client_to_type_at();

    // Hearing allegiance chat starts on, so "off" is the change and "on" puts it back. Each is one
    // of the options the client tells the shard about at once rather than at the next save.
    let mut option_lines = true;
    for (line, value) in [("@allegiance chat off", 0_u32), ("@allegiance chat on", 1)] {
        say(&mut c, line);
        let i = c.view().expect_app().interaction();
        option_lines &= i.last_sent.as_slice()
            == [Request::PlayerOptionChanged(
                pyreal_protocol::login::CharacterPlayerOptionChangedEvent {
                    option: u32::try_from(cmd::HEAR_ALLEGIANCE_CHAT_ORDINAL).expect("small"),
                    value,
                },
            )]
            && i.stats.allegiance_command_requests == 0;
        c.tick(1);
    }
    // Saying it again when it already agrees changes nothing, so nothing is sent.
    say(&mut c, "@allegiance chat on");
    let already = c.view().expect_app().interaction().last_sent.is_empty();
    c.tick(1);

    let broadcast =
        Request::ChannelBroadcast(pyreal_protocol::comms::CommunicationChannelBroadcast {
            channel: cmd::ALLEGIANCE_BROADCAST_CHANNEL,
            message: "well met".into(),
        });
    say(&mut c, "@ab well met");
    let short = c.view().expect_app().interaction().last_sent.as_slice()
        == std::slice::from_ref(&broadcast);
    c.tick(1);
    say(&mut c, "@allegiance broadcast well met");
    let long = c.view().expect_app().interaction().last_sent.as_slice()
        == std::slice::from_ref(&broadcast);
    c.tick(1);

    c.assert_behaviour(
        "allegiance.commands.listening-is-a-local-option-and-broadcasting-is-a-channel",
        move |_| option_lines && already && short && long,
    );
    c.shutdown();
}
