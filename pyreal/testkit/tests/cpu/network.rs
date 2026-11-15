//! The Dereth Network as a retail client sees it: tells to `Name@world` and `name@channel`, the
//! `@dn` command, remote tellers under stand-in ids, the Global room in a chat-room slot, remote
//! friends and squelching an address.
//!
//! The network is world to hub; a client sees only the retail messages its own world sends
//! (`docs/networking/messages/13-dereth-network.md`). Empyrean writes the inbound messages here
//! with the same protocol structs, and its room line as `game_message_turbine_chat` lays it out.

use pyreal_client_contract::UiRequest;
use pyreal_client_model::chat::text_type;
use pyreal_client_model::Request;
use pyreal_client_net::client_session::SessionEvent;
use pyreal_primitives::ObjectId;
use pyreal_protocol::comms::{self, ChatRoomMembership};
use pyreal_protocol::social::{FriendData, SocialFriendsUpdate};

use pyreal_testkit::{HeadlessClient, Inbound, Player};

const ME: ObjectId = ObjectId(0x5000_0001);
/// The stand-in id the world gives `Bob@coldeve`.
const BOB: ObjectId = ObjectId(0x6000_0005);
/// The stand-in id the world gives `Ann@coldeve`.
const ANN: ObjectId = ObjectId(0x6000_0009);
/// Empyrean's Roleplay room, which carries the Global room unless the hub says otherwise.
const ROLEPLAY_ROOM: u32 = 5;
/// Empyrean's `ChatType::Roleplay`.
const ROLEPLAY_CHAT_TYPE: u32 = 5;

/// A client in the world as `ME`, whose speech handlers can see the player.
fn a_player() -> HeadlessClient {
    let mut c = HeadlessClient::model();
    c.hud_mut().player_desc_received = true;
    c.hud_mut().player = Some(ME);
    c.hud_mut().player_body = true;
    c
}

/// One line typed into a chat window; answers the requests it put in the outbox.
fn type_line(c: &mut HeadlessClient, text: &str) -> Vec<Request> {
    c.interaction_mut().last_refusal = None;
    let before = c.outbound().len();
    c.when(Player::ui(UiRequest::ChatLine {
        text: text.to_owned(),
        window: 8,
    }));
    c.outbound()[before..].to_vec()
}

fn ui_event<M: pyreal_protocol::Message>(m: &M) -> SessionEvent {
    SessionEvent::UiEvent {
        opcode: M::OPCODE,
        blob: pyreal_protocol::write_blob(m).expect("the message encodes"),
    }
}

/// A tell from another world, as the receiving world shows it.
fn remote_tell(sender: ObjectId, name: &str, text: &str) -> SessionEvent {
    ui_event(&comms::CommunicationHearDirectSpeech {
        message: text.into(),
        sender_name: name.into(),
        sender_id: sender,
        target_id: ME,
        text_type: text_type::SPEECH_DIRECT,
        secret_flags: 0,
    })
}

/// Server text, as the world writes its network lines.
fn server_text(text: &str) -> SessionEvent {
    ui_event(&comms::CommunicationTextboxString {
        text: text.into(),
        text_type: text_type::SYSTEM,
    })
}

fn friends(rows: &[(ObjectId, &str, bool)], update_type: u32) -> SocialFriendsUpdate {
    SocialFriendsUpdate {
        friends: rows
            .iter()
            .map(|&(id, name, online)| FriendData {
                id,
                online: i32::from(online),
                appear_offline: 0,
                name: name.to_owned(),
                friends_list: Vec::new(),
                friend_of_list: Vec::new(),
            })
            .collect(),
        update_type,
    }
}

/// `text` as a one-byte-length UTF-16 string, as the room event writes its names.
fn wide_into(out: &mut Vec<u8>, text: &str) {
    let units: Vec<u16> = text.encode_utf16().collect();
    out.push(u8::try_from(units.len()).expect("under 128 units"));
    out.extend(units.into_iter().flat_map(u16::to_le_bytes));
}

/// A room line from another world, laid out as Empyrean's `game_message_turbine_chat` writes an
/// `EVENT_BINARY` `SENDTOROOMBYNAME` blob.
fn global_room_line(room: u32, speaker: &str, text: &str, sender: ObjectId) -> Vec<u8> {
    let mut inner = room.to_le_bytes().to_vec();
    wide_into(&mut inner, speaker);
    wide_into(&mut inner, text);
    for v in [0x0C, sender.0, 0, ROLEPLAY_CHAT_TYPE] {
        inner.extend(v.to_le_bytes());
    }
    let mut blob = Vec::new();
    let inner_len = u32::try_from(inner.len()).expect("small");
    for v in [
        inner_len + 32,
        1,
        1,
        1,
        0x000B_00B5,
        1,
        0x000B_00B5,
        0,
        inner_len,
    ] {
        blob.extend(u32::to_le_bytes(v));
    }
    blob.extend(inner);
    blob
}

/// A tell to an address goes by name with the address whole: a character on another world, a
/// channel, and a name given with the Discord mark, which the world takes off.
pub fn a_tell_to_an_address_goes_by_name_whole() {
    let mut c = a_player();
    let sent: Vec<Request> = [
        "@tell Bob@coldeve, long time! still playing?",
        "@tell oldguard@channel, anyone around tonight?",
        "@tell Ann@coldeve (Discord), evening",
    ]
    .iter()
    .flat_map(|line| type_line(&mut c, line))
    .collect();
    let by_name = |name: &str, text: &str| {
        Request::TalkDirectByName(comms::CommunicationTalkDirectByName {
            message: text.into(),
            target_name: name.into(),
        })
    };
    let expected = vec![
        by_name("Bob@coldeve", "long time! still playing?"),
        by_name("oldguard@channel", "anyone around tonight?"),
        by_name("Ann@coldeve (Discord)", "evening"),
    ];
    c.assert_behaviour(
        "network.tells.an-address-tell-goes-by-name-whole",
        move |_| sent == expected,
    );
}

/// `@dn`, `@derethnetwork` and `@dng` are no command of this client: each goes to the world as a
/// spoken line, and `/dn` is rewritten to `@dn` first.
pub fn the_network_command_goes_to_the_world_as_a_spoken_line() {
    let mut c = a_player();
    let lines = [
        ("@dn", "@dn"),
        ("/dn on", "@dn on"),
        ("@dn friends add Bob@coldeve", "@dn friends add Bob@coldeve"),
        ("/dng evening all", "@dng evening all"),
        ("@derethnetwork help", "@derethnetwork help"),
    ];
    let forwarded = lines.iter().all(|(typed, said)| {
        type_line(&mut c, typed)
            == vec![Request::Talk(comms::CommunicationTalk {
                message: (*said).into(),
            })]
    });
    let refused = c.view().interaction().last_refusal.is_some();
    c.assert_behaviour(
        "network.commands.dn-goes-to-the-world-as-a-spoken-line",
        move |_| forwarded && !refused,
    );
}

/// A remote tell under a stand-in id is drawn with its address as a link and makes the teller the
/// reply target, so `@r` answers it by its id; a sender past the stand-in block does not.
pub fn a_remote_teller_is_a_link_and_the_reply_target() {
    let mut c = a_player();
    c.when(Inbound::event(remote_tell(
        BOB,
        "Bob@coldeve",
        "yes! come say hi Sat",
    )));
    let drawn = c.chat_lines().last().map(|l| l.body.clone());
    let linked = drawn.as_deref()
        == Some(
            "<Tell:IIDString:1610612741:Bob@coldeve>Bob@coldeve<\\Tell> tells you, \"yes! come say hi Sat\"",
        );
    let armed = c.view().world().chat.last_teller == Some(BOB)
        && c.view().world().chat.last_teller_name == "Bob@coldeve";
    let replied = type_line(&mut c, "@r see you then")
        == vec![Request::TalkDirect(comms::CommunicationTalkDirect {
            message: "see you then".into(),
            target: BOB,
        })];

    let mut past = a_player();
    past.when(Inbound::event(remote_tell(
        ObjectId(0x7000_0001),
        "Odd@where",
        "hi",
    )));
    let not_armed = past.view().world().chat.last_teller.is_none();
    c.assert_behaviour(
        "network.tells.a-stand-in-teller-is-a-link-and-the-reply-target",
        move |_| linked && armed && replied && not_armed,
    );
}

/// A Global room line from another world is drawn in the room that carries it, under the
/// speaker's address with the Discord mark when it was said there; a stand-in id the world's
/// squelch table lists for everything is not shown.
pub fn a_global_room_line_from_another_world_is_drawn_in_its_room() {
    let mut c = a_player();
    let w = c.world_mut();
    w.chat.startup_turbine_chat();
    w.chat.recv_chat_room_tracker(ChatRoomMembership {
        general_room: 2,
        roleplay_room: ROLEPLAY_ROOM,
        ..ChatRoomMembership::default()
    });
    let line = global_room_line(ROLEPLAY_ROOM, "Ann@coldeve (Discord)", "evening all", ANN);
    assert!(w.recv_turbine_chat(&line), "the room line is read");
    let drawn: Vec<(u32, String)> = w
        .scroll
        .drain()
        .into_iter()
        .map(|l| (l.chat_type, l.body))
        .collect();
    let shown = drawn
        == vec![(
            0x1E,
            "[Roleplay] <Tell:IIDString:0:Ann@coldeve (Discord)>Ann@coldeve (Discord)<\\Tell> says, \"evening all\"".to_owned(),
        )];

    let mut e = pyreal_client_model::chat::SquelchEntry {
        name: "Ann@coldeve".into(),
        ..Default::default()
    };
    e.squelch_everything();
    w.chat.squelch.characters.insert(ANN, e);
    assert!(w.recv_turbine_chat(&line));
    let squelched = w.scroll.drain().is_empty();
    c.assert_behaviour(
        "network.rooms.a-global-line-is-drawn-in-its-room-under-the-address",
        move |_| shown && squelched,
    );
}

/// Remote friends are rows under stand-in ids: added by name, shown online by a status row, their
/// online line's name a link, removed by id, and gone on the world's removal row.
pub fn remote_friends_are_rows_under_stand_in_ids() {
    let mut c = a_player();
    let added = type_line(&mut c, "@friends add Bob@coldeve")
        == vec![Request::SocialAddFriend(
            pyreal_protocol::social::SocialAddFriend {
                name: "Bob@coldeve".into(),
            },
        )];
    c.when(Inbound::message(&friends(
        &[(BOB, "Bob@coldeve", false)],
        1,
    )));
    c.when(Inbound::message(&friends(&[(BOB, "Bob@coldeve", true)], 4)));
    c.when(Inbound::event(server_text(
        "<Tell:IIDString:1610612741:Bob@coldeve>Bob@coldeve<\\Tell> has come online.",
    )));
    let listed = c
        .view()
        .world()
        .friends()
        .iter()
        .any(|f| f.id == BOB && f.name == "Bob@coldeve" && f.online);
    let line_kept = c.chat_lines().last().is_some_and(|l| {
        l.body == "<Tell:IIDString:1610612741:Bob@coldeve>Bob@coldeve<\\Tell> has come online."
    });
    let removed = type_line(&mut c, "@friends remove bob@coldeve")
        == vec![Request::SocialRemoveFriend(
            pyreal_protocol::social::SocialRemoveFriend { friend_id: BOB },
        )];
    c.when(Inbound::message(&friends(&[(BOB, "Bob@coldeve", true)], 2)));
    let gone = c.view().world().friends().is_empty();
    c.assert_behaviour(
        "network.friends.a-remote-friend-is-a-row-under-its-stand-in-id",
        move |_| added && listed && line_kept && removed && gone,
    );
}

/// Squelching an address the client has no object for sends the name with id 0, for the world to
/// keep.
pub fn squelching_an_address_sends_its_name_with_id_zero() {
    let mut c = a_player();
    let sent = type_line(&mut c, "/squelch Bob@coldeve");
    let ok = matches!(
        sent.as_slice(),
        [Request::ModifyCharacterSquelch(m)]
            if m.add == 1
                && m.character_id == ObjectId(0)
                && m.character_name == "Bob@coldeve"
    );
    c.assert_behaviour(
        "network.squelch.an-address-is-squelched-by-name-with-id-zero",
        move |_| ok,
    );
}

pyreal_testkit::scenarios! {
    scenario_a_tell_to_an_address_goes_by_name_whole => a_tell_to_an_address_goes_by_name_whole ["network.tells.an-address-tell-goes-by-name-whole"],
    scenario_the_network_command_goes_to_the_world_as_a_spoken_line => the_network_command_goes_to_the_world_as_a_spoken_line ["network.commands.dn-goes-to-the-world-as-a-spoken-line"],
    scenario_a_remote_teller_is_a_link_and_the_reply_target => a_remote_teller_is_a_link_and_the_reply_target ["network.tells.a-stand-in-teller-is-a-link-and-the-reply-target"],
    scenario_a_global_room_line_from_another_world_is_drawn_in_its_room => a_global_room_line_from_another_world_is_drawn_in_its_room ["network.rooms.a-global-line-is-drawn-in-its-room-under-the-address"],
    scenario_remote_friends_are_rows_under_stand_in_ids => remote_friends_are_rows_under_stand_in_ids ["network.friends.a-remote-friend-is-a-row-under-its-stand-in-id"],
    scenario_squelching_an_address_sends_its_name_with_id_zero => squelching_an_address_sends_its_name_with_id_zero ["network.squelch.an-address-is-squelched-by-name-with-id-zero"],
}
