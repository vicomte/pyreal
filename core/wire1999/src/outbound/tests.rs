use super::*;
use crate::conn::Connection;
use crate::link::Link1999;
use crate::login::tests::grant_datagram;
use crate::login::Credential;
use crate::packet::{flags, read_fragments, Header, HEADER_SIZE};
use crate::translate::Shared;
use pyreal_primitives::{ObjectId, Transport};
use pyreal_protocol::actions::{pack_action, pack_action_raw};
use pyreal_protocol::comms::{
    CharacterConfirmationResponse, CommunicationModifyAccountSquelch,
    CommunicationModifyGlobalSquelch, CommunicationSetAfkMode, CommunicationTalk,
};
use pyreal_protocol::social::{FellowshipQuitRequest, SocialClearFriends, SocialRemoveFriend};
use pyreal_protocol::trade::{AdvocateTeleport, ItemProfile};
use pyreal_protocol::{read_body, Message};

/// A link past its grant, with the login and ack taken.
fn granted() -> Link1999 {
    let mut c = Connection::new(Credential::new("demo", "secret").unwrap(), (102, 67));
    c.tick(0.0);
    c.take_outgoing();
    c.feed(&grant_datagram(b"4.35752", 0, 0), 0.1)
        .expect("granted");
    c.take_outgoing();
    Link1999::new(c, "AC1999")
}

/// Every message the link put on the wire since the last call: (1999 queue, message).
fn sent(l: &mut Link1999) -> Vec<(u16, Vec<u8>)> {
    l.connection_mut()
        .take_outgoing()
        .iter()
        .filter_map(|raw| {
            // A data packet's length does not count the flow word after the header.
            let h = Header::read(raw)?;
            let len = usize::from(h.len);
            (h.has(flags::DATA) && len > 0)
                .then(|| raw[HEADER_SIZE + 4..HEADER_SIZE + 4 + len].to_vec())
        })
        .flat_map(|body| read_fragments(&body).unwrap_or_default())
        .map(|f| (f.header.queue_id, f.payload))
        .collect()
}

fn act<M: Message>(m: &M) -> Vec<u8> {
    pack_action(77, m).unwrap()
}

fn raw(sub: u32, body: &[u8]) -> Vec<u8> {
    pack_action_raw(77, Opcode(sub), body)
}

/// The one 1999 action `later` becomes, with its order number left 0.
fn one_action(later: &[u8], shared: &mut Shared) -> Vec<u8> {
    match action_up(later, shared).unwrap() {
        Outcome::Send(ups) => match ups.as_slice() {
            [Up::Action(m)] => m.clone(),
            other => panic!("one action, not {other:?}"),
        },
        other => panic!("sent, not {other:?}"),
    }
}

fn words(ws: &[u32]) -> Vec<u8> {
    ws.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// `[0xF7B1][0][action]` then `body`.
fn expect(action: u32, body: &[u8]) -> Vec<u8> {
    let mut m = words(&[GAME_ACTION, 0, action]);
    m.extend_from_slice(body);
    m
}

#[test]
fn actions_are_numbered_from_one_and_a_refused_one_takes_no_number() {
    let mut l = granted();
    l.send(NetQueue::Weenie, true, &raw(0x001B, &words(&[0x5000_0001])));
    let refused = act(&CommunicationModifyGlobalSquelch {
        add: 1,
        msg_type: 3,
    });
    l.send(NetQueue::Weenie, true, &refused);
    l.send(
        NetQueue::Weenie,
        true,
        &act(&CommunicationSetAfkMode { afk: 1 }),
    );
    l.send(NetQueue::Weenie, true, &raw(0x01E9, &[]));
    let out = sent(&mut l);
    assert_eq!(out.len(), 2);
    assert_eq!(
        out[0],
        (WEENIE_QUEUE, words(&[GAME_ACTION, 1, 0x001B, 0x5000_0001])),
        "Pyreal's stamp 77 is not used"
    );
    assert_eq!(out[1], (WEENIE_QUEUE, words(&[GAME_ACTION, 2, 0x01E9])));

    let told = l.poll().expect("one line for the refusal");
    assert_eq!(told.opcode, 0xF7E0);
    let told: CommunicationTextboxString = read_body(&told.body).unwrap();
    assert_eq!(told.text, NOT_IN_THIS_WORLD);
    assert_eq!(told.text_type, REFUSAL_TEXT_TYPE);
    assert!(l.poll().is_none(), "the AFK change goes without a word");
}

#[test]
fn nothing_goes_up_before_the_grant_and_no_number_is_spent() {
    let mut o = Outbound::default();
    let conn = Connection::new(Credential::new("demo", "secret").unwrap(), (102, 67));
    let mut l = Link1999::new(conn, "AC1999");
    l.send(NetQueue::Weenie, true, &raw(0x01E9, &[]));
    assert!(l.connection_mut().take_outgoing().is_empty());
    assert_eq!(o.order(), 0);
    o.order = 4;
    assert_eq!(o.order(), 4);
}

#[test]
fn speech_is_a_1999_string_and_an_empty_line_is_two_bytes() {
    let mut s = Shared::default();
    let m = one_action(
        &act(&CommunicationTalk {
            message: "hi".to_owned(),
        }),
        &mut s,
    );
    assert_eq!(m, expect(0x0015, &[3, 0, b'h', b'i', 0, 0, 0, 0]));
    let m = one_action(
        &act(&CommunicationTalk {
            message: String::new(),
        }),
        &mut s,
    );
    assert_eq!(m, expect(0x0015, &[0, 0]));
    let m = one_action(
        &act(&CommunicationTalk {
            message: "caf\u{e9}".to_owned(),
        }),
        &mut s,
    );
    assert_eq!(m, expect(0x0015, &[5, 0, b'c', b'a', b'f', 0xE9, 0, 0]));
}

#[test]
fn save_typed_alone_ties_to_the_lifestone_and_longer_words_stay_speech() {
    let mut s = Shared::default();
    for line in ["@save", " /SAVE ", "@Save"] {
        let m = one_action(
            &act(&CommunicationTalk {
                message: line.to_owned(),
            }),
            &mut s,
        );
        assert_eq!(m, expect(0x01A2, &[]), "{line:?}");
    }
    for line in ["@saveui", "@save now", "save"] {
        let m = one_action(
            &act(&CommunicationTalk {
                message: line.to_owned(),
            }),
            &mut s,
        );
        assert_eq!(m[8..12], 0x0015u32.to_le_bytes(), "{line:?}");
    }
}

#[test]
fn text_with_a_nul_or_too_long_is_refused() {
    let mut s = Shared::default();
    // Pyreal's own strings are Windows-1252 already; a NUL is what has no 1999 form.
    let r = action_up(&raw(0x0015, &[3, 0, b'a', 0, b'b', 0, 0, 0]), &mut s);
    assert_eq!(r.unwrap(), Outcome::Refuse(TEXT_NOT_SENDABLE));
    let r = action_up(
        &act(&CommunicationModifyCharacterSquelch {
            add: 1,
            character_id: ObjectId(0),
            character_name: "a".repeat(NAME_MAX + 1),
            msg_type: 0,
        }),
        &mut s,
    );
    assert_eq!(r.unwrap(), Outcome::Refuse(TEXT_TOO_LONG));
}

#[test]
fn squelch_maps_to_the_four_1999_actions_and_the_later_numbers_that_mean_unsquelch_are_refused() {
    let mut s = Shared::default();
    let sq = |add, id, name: &str| {
        act(&CommunicationModifyCharacterSquelch {
            add,
            character_id: ObjectId(id),
            character_name: name.to_owned(),
            msg_type: 4,
        })
    };
    let ann = [4, 0, b'A', b'n', b'n', 0, 0, 0];
    assert_eq!(
        one_action(&sq(1, 0, "Ann"), &mut s),
        expect(0x0058, &ann),
        "the message type is dropped"
    );
    assert_eq!(one_action(&sq(0, 0, "Ann"), &mut s), expect(0x0059, &ann));
    assert_eq!(
        one_action(&sq(1, 0x5000_0002, ""), &mut s),
        expect(0x005A, &words(&[0x5000_0002]))
    );
    assert_eq!(
        one_action(&sq(0, 0x5000_0002, ""), &mut s),
        expect(0x005B, &words(&[0x5000_0002]))
    );
    assert_eq!(action_up(&sq(1, 0, ""), &mut s).unwrap(), Outcome::Drop);

    let account = act(&CommunicationModifyAccountSquelch {
        add: 1,
        character_name: "Ann".to_owned(),
    });
    let global = act(&CommunicationModifyGlobalSquelch {
        add: 0,
        msg_type: 2,
    });
    for later in [account, global] {
        assert_eq!(
            action_up(&later, &mut s).unwrap(),
            Outcome::Refuse(NOT_IN_THIS_WORLD)
        );
    }
}

#[test]
fn combat_modes_count_up_on_the_1999_wire() {
    let mut s = Shared::default();
    for (later, old) in [(1, 1), (2, 2), (4, 3), (8, 4), (16, 16)] {
        let m = one_action(&raw(0x0053, &words(&[later])), &mut s);
        assert_eq!(m, expect(0x0053, &words(&[old])), "{later}");
    }
}

#[test]
fn the_option_word_goes_up_whole_with_the_bits_pyreal_does_not_know_kept() {
    let mut s = Shared {
        options_word: Some(0x50C4_2542),
        ..Shared::default()
    };
    // Vivid targeting, ordinal 14, is bit 0x8000.
    let m = one_action(&raw(0x0005, &words(&[14, 1])), &mut s);
    assert_eq!(m, expect(0x01A1, &words(&[0x50C4_A542])));
    assert_eq!(s.options_word, Some(0x50C4_A542));
    let m = one_action(&raw(0x0005, &words(&[0, 0])), &mut s);
    assert_eq!(m, expect(0x01A1, &words(&[0x50C4_A540])), "auto-repeat off");
    // Ordinal 3 (ignore trade requests) has no 1999 bit.
    assert_eq!(
        action_up(&raw(0x0005, &words(&[3, 1])), &mut s).unwrap(),
        Outcome::Drop
    );
    // The whole module: flags, the later word, more; only the 1999 bits are taken from it.
    let later = 0xFFFF_0000 | 0x0002 | 0x0400;
    let m = one_action(&raw(0x01A1, &words(&[0x0460, later, 0, 0])), &mut s);
    assert_eq!(
        m,
        expect(0x01A1, &words(&[(0x50C4_A540 & !OPTION_MASK) | 0x0402]))
    );
    assert_eq!(OPTION_BITS.iter().fold(0, |m, (_, b)| m | b), OPTION_MASK);
}

#[test]
fn login_complete_is_followed_by_the_squelch_list() {
    let mut l = granted();
    l.send(NetQueue::Weenie, true, &raw(0x00A1, &[]));
    let out = sent(&mut l);
    assert_eq!(
        out,
        vec![
            (WEENIE_QUEUE, words(&[GAME_ACTION, 1, 0x00A1])),
            (WEENIE_QUEUE, words(&[GAME_ACTION, 2, 0x005C, 0])),
        ],
        "an empty list still goes"
    );
    // Login complete again after a teleport: the list went up once for this entry.
    l.send(NetQueue::Weenie, true, &raw(0x00A1, &[]));
    assert_eq!(
        sent(&mut l),
        vec![(WEENIE_QUEUE, words(&[GAME_ACTION, 3, 0x00A1]))]
    );
    // A new world entry uploads it again.
    l.send(NetQueue::Logon, false, &words(&[0xF657, 0x5000_0001]));
    l.send(NetQueue::Weenie, true, &raw(0x00A1, &[]));
    let out = sent(&mut l);
    assert_eq!(
        out.last(),
        Some(&(WEENIE_QUEUE, words(&[GAME_ACTION, 5, 0x005C, 0])))
    );

    let mut s = Shared::default();
    s.note_squelch(true, 7);
    s.note_squelch(true, 9);
    s.note_squelch(true, 7);
    s.note_squelch(true, 3);
    s.note_squelch(false, 9);
    let Outcome::Send(ups) = action_up(&raw(0x00A1, &[]), &mut s).unwrap() else {
        panic!("sent");
    };
    assert_eq!(
        ups[1],
        Up::Action(words(&[GAME_ACTION, 0, 0x005C, 2, 7, 3])),
        "in the order squelched, without repeats"
    );
}

#[test]
fn fellowship_vendor_and_favourite_actions_lose_their_later_fields() {
    let mut s = Shared::default();
    let m = one_action(
        &act(&FellowshipCreate {
            name: "Ann".to_owned(),
            share_xp: 1,
        }),
        &mut s,
    );
    assert_eq!(m, expect(0x00A2, &[4, 0, b'A', b'n', b'n', 0, 0, 0]));
    let m = one_action(&act(&FellowshipQuitRequest { disband: 1 }), &mut s);
    assert_eq!(m, expect(0x00A3, &[]));

    let buy = act(&VendorBuy {
        vendor_id: ObjectId(0x8000_0001),
        items: vec![ItemProfile {
            amount: 2,
            iid: ObjectId(0x8000_0002),
            pwd: None,
        }],
        alternate_currency_id: 0x1234,
    });
    let m = one_action(&buy, &mut s);
    assert_eq!(m, expect(0x005F, &words(&[0x8000_0001, 1, 2, 0x8000_0002])));

    let m = one_action(&raw(0x01E3, &words(&[0x3E1, 4, 6])), &mut s);
    assert_eq!(m, expect(0x01E3, &words(&[0x3E1, 4])));
    let m = one_action(&raw(0x01E4, &words(&[0x3E1, 6])), &mut s);
    assert_eq!(m, expect(0x01E4, &words(&[0x3E1])));
}

#[test]
fn aligned_fields_pad_after_an_empty_string() {
    let mut s = Shared::default();
    let m = one_action(
        &act(&CommunicationTalkDirect {
            message: String::new(),
            target: ObjectId(0x5000_0003),
        }),
        &mut s,
    );
    assert_eq!(
        m,
        expect(0x0032, &[0, 0, 0, 0, 3, 0, 0, 0x50]),
        "the server reads the id at the next word"
    );
    let m = one_action(&act(&AdvocateTeleport::default()), &mut s);
    assert_eq!(m.len(), 12 + 4 + 32, "an empty name is four bytes");
    assert_eq!(m[12..16], [0, 0, 0, 0]);

    let m = one_action(
        &act(&CommunicationTalkDirectByName {
            message: "yo".to_owned(),
            target_name: "Ann".to_owned(),
        }),
        &mut s,
    );
    assert_eq!(
        m,
        expect(
            0x005D,
            &[3, 0, b'y', b'o', 0, 0, 0, 0, 4, 0, b'A', b'n', b'n', 0, 0, 0]
        )
    );
}

#[test]
fn confirmation_answers_go_only_while_the_server_is_owed_one() {
    let mut s = Shared::default();
    let answer = act(&CharacterConfirmationResponse {
        confirmation_type: 7,
        context_id: 9,
        accepted: 1,
    });
    assert_eq!(
        action_up(&answer, &mut s).unwrap(),
        Outcome::Refuse(NOT_IN_THIS_WORLD)
    );
    s.questions_owed = 1;
    let m = one_action(&answer, &mut s);
    assert_eq!(m, expect(0x0275, &words(&[7, 9, 1])));
    assert_eq!(s.questions_owed, 0);
}

#[test]
fn friends_requests_become_the_1999_friends_command_on_the_control_queue() {
    let mut l = granted();
    l.send(
        NetQueue::Weenie,
        true,
        &act(&SocialAddFriend {
            name: "Ann".to_owned(),
        }),
    );
    l.send(NetQueue::Weenie, true, &act(&SocialClearFriends));
    l.send(
        NetQueue::Control,
        false,
        &pyreal_protocol::write_blob(&SocialSendFriendsCommand {
            cmd: 0,
            player: String::new(),
        })
        .unwrap(),
    );
    l.send(
        NetQueue::Weenie,
        true,
        &act(&SocialRemoveFriend {
            friend_id: ObjectId(5),
        }),
    );
    l.send(NetQueue::Weenie, true, &raw(0x01E9, &[]));
    let out = sent(&mut l);
    let mut add = words(&[FRIENDS_COMMAND, 1]);
    add.extend([4, 0, b'A', b'n', b'n', 0, 0, 0]);
    assert_eq!(out[0], (CONTROL_QUEUE, add));
    assert_eq!(
        out[1],
        (
            CONTROL_QUEUE,
            [&words(&[FRIENDS_COMMAND, 3])[..], &[0, 0, 0, 0]].concat()
        )
    );
    assert_eq!(
        out[2],
        (
            CONTROL_QUEUE,
            [&words(&[FRIENDS_COMMAND, 0])[..], &[0, 0, 0, 0]].concat()
        )
    );
    assert_eq!(
        out[3],
        (WEENIE_QUEUE, words(&[GAME_ACTION, 1, 0x01E9])),
        "the friends commands and the refused removal take no order number"
    );
    let told: CommunicationTextboxString = read_body(&l.poll().unwrap().body).unwrap();
    assert_eq!(told.text, NOT_IN_THIS_WORLD);
}

#[test]
fn appearance_and_version_requests_go_up_on_the_control_queue_and_other_bare_ones_do_not() {
    let mut l = granted();
    l.send(NetQueue::Control, false, &words(&[0xF6EA, 0x5000_0001]));
    l.send(NetQueue::Control, false, &words(&[0xF7CC]));
    l.send(NetQueue::Control, false, &words(&[0xF7E5, 1, 2]));
    assert_eq!(
        sent(&mut l),
        vec![
            (CONTROL_QUEUE, words(&[0xF6EA, 0x5000_0001])),
            (CONTROL_QUEUE, words(&[0xF7CC])),
        ]
    );
    assert!(l.poll().is_none(), "no line for a bare message");
}

#[test]
fn every_passed_action_is_one_pyreal_sends_with_that_meaning_or_none() {
    for sub in PASSED {
        let info = Opcode(sub).info();
        assert!(
            info.is_none_or(|i| i.send_queue == Some(NetQueue::Weenie)),
            "{sub:#06X} is a later action of another kind"
        );
        assert!(!DROPPED.contains(&sub));
    }
    for sub in [0x0059, 0x005B, 0x0017] {
        assert!(!PASSED.contains(&sub), "{sub:#06X} must never pass");
    }
}
