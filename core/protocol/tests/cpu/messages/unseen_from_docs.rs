//! Contracts for message types no recording carries, built byte for byte from the catalogue.
//! Fixture: bytes typed from `docs/networking/messages/`, not from any encoder.

use pyreal_protocol::comms::CommunicationHearRangedSpeech;
use pyreal_protocol::items::ItemGetInscriptionResponse;
use pyreal_protocol::login::{LoginAccountBanned, LoginAwaitingSubscriptionExpiration};
use pyreal_protocol::objects::EffectsPlayScriptId;
use pyreal_protocol::qualities::{
    QualitiesPrivateRemoveInt64, QualitiesPrivateUpdateBool, QualitiesRemoveFloat,
    QualitiesUpdateDataId, QualitiesUpdateFloat, QualitiesUpdateInt64, QualitiesUpdateString,
};
use pyreal_protocol::read_body;
use pyreal_protocol::social::AllegianceUpdateAborted;
use pyreal_protocol::trade::{GameGameOver, GameJoinGameResponse, GameMoveResponse, GameStartGame};

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

/// `03-qualities-and-updates.md` §§1, 3: public `[seq][object][property][i64]`, nothing padded.
#[test]
fn a_public_int64_update_is_unaligned_with_an_eight_byte_value() {
    let b = cat(&[
        &[9],
        &0x5000_0001u32.to_le_bytes(),
        &4u32.to_le_bytes(),
        &(-2i64).to_le_bytes(),
    ]);
    assert_eq!(b.len(), 17);
    let m: QualitiesUpdateInt64 = read_body(&b).expect("decodes");
    assert_eq!(
        (m.0.sequence, m.0.object.0, m.0.property_id, m.0.value),
        (9, 0x5000_0001, 4, -2)
    );
}

/// §3: a bool is a dword, not a byte.
#[test]
fn a_private_bool_update_carries_a_dword() {
    let b = cat(&[&[3], &11u32.to_le_bytes(), &1i32.to_le_bytes()]);
    let m: QualitiesPrivateUpdateBool = read_body(&b).expect("decodes");
    assert_eq!((m.0.sequence, m.0.property_id, m.0.value), (3, 11, 1));
    assert!(
        read_body::<QualitiesPrivateUpdateBool>(&b[..6]).is_err(),
        "one byte is not a bool"
    );
}

/// §3: a float is a double.
#[test]
fn a_public_float_update_carries_a_double() {
    let b = cat(&[
        &[1],
        &2u32.to_le_bytes(),
        &5u32.to_le_bytes(),
        &1.25f64.to_le_bytes(),
    ]);
    let m: QualitiesUpdateFloat = read_body(&b).expect("decodes");
    assert_eq!((m.0.object.0, m.0.property_id, m.0.value), (2, 5, 1.25));
}

/// §1: the public string update aligns to 4 immediately before the string. The body starts at blob
/// offset 4, so seq, object and property end at 13 and three pad bytes follow.
#[test]
fn a_public_string_update_aligns_only_before_the_string() {
    let b = cat(&[
        &[7],
        &2u32.to_le_bytes(),
        &1u32.to_le_bytes(),
        &[0, 0, 0],
        &[3, 0],
        b"Bob",
        &[0, 0, 0],
    ]);
    let m: QualitiesUpdateString = read_body(&b).expect("decodes");
    assert_eq!(
        (m.0.object.0, m.0.property_id, m.0.value.0.as_str()),
        (2, 1, "Bob")
    );
}

/// §3: a data id is a bare `u32`.
#[test]
fn a_public_data_id_update_is_thirteen_bytes_of_header_and_value() {
    let b = cat(&[
        &[4],
        &3u32.to_le_bytes(),
        &6u32.to_le_bytes(),
        &0x0600_1234u32.to_le_bytes(),
    ]);
    let m: QualitiesUpdateDataId = read_body(&b).expect("decodes");
    assert_eq!(
        (m.0.object.0, m.0.property_id, m.0.value),
        (3, 6, 0x0600_1234)
    );
}

/// Intro and §1: removes are the update shape with no value.
#[test]
fn removes_carry_no_value() {
    let b = cat(&[&[2], &8u32.to_le_bytes()]);
    let m: QualitiesPrivateRemoveInt64 = read_body(&b).expect("decodes");
    assert_eq!((m.0.sequence, m.0.property_id), (2, 8));
    let b = cat(&[&[2], &0x5000_0001u32.to_le_bytes(), &8u32.to_le_bytes()]);
    let m: QualitiesRemoveFloat = read_body(&b).expect("decodes");
    assert_eq!((m.0.object.0, m.0.property_id), (0x5000_0001, 8));
}

/// `08-communication.md`: hear speech's fields, string first, with the range float before the type.
#[test]
fn hear_ranged_speech_puts_the_range_before_the_type() {
    let b = cat(&[
        &[2, 0],
        b"Hi",
        &[3, 0],
        b"Bob",
        &[0, 0, 0],
        &0x5000_0001u32.to_le_bytes(),
        &2.5f32.to_le_bytes(),
        &2u32.to_le_bytes(),
    ]);
    let m: CommunicationHearRangedSpeech = read_body(&b).expect("decodes");
    assert_eq!(m.message, "Hi");
    assert_eq!(m.sender_name, "Bob");
    assert_eq!(m.sender_id.0, 0x5000_0001);
    assert_eq!(m.range, 2.5);
    assert_eq!(m.text_type, 2);
}

/// `01-login-and-character.md`: a signed expiry, zero or less permanent, then a reason.
#[test]
fn account_banned_carries_a_signed_expiry_and_a_reason() {
    let b = cat(&[&(-1i32).to_le_bytes(), &[4, 0], b"spam", &[0, 0]]);
    let m: LoginAccountBanned = read_body(&b).expect("decodes");
    assert_eq!((m.expiry, m.reason.as_str()), (-1, "spam"));
}

/// `01-login-and-character.md`: minutes, one dword.
#[test]
fn awaiting_subscription_expiration_is_minutes() {
    let m: LoginAwaitingSubscriptionExpiration = read_body(&30u32.to_le_bytes()).expect("decodes");
    assert_eq!(m.minutes, 30);
}

/// `02-world-objects.md`: an id and a script id.
#[test]
fn play_script_by_id_is_an_id_and_a_script() {
    let b = cat(&[&0x5000_0001u32.to_le_bytes(), &0x3300_0042u32.to_le_bytes()]);
    let m: EffectsPlayScriptId = read_body(&b).expect("decodes");
    assert_eq!((m.id.0, m.script_id), (0x5000_0001, 0x3300_0042));
}

/// `07-inventory-and-items.md`: an object, an unnamed dword the client skips, the inscription, the
/// scribe's name and account.
#[test]
fn the_inscription_response_skips_one_dword_after_the_object() {
    let b = cat(&[
        &0x8000_0001u32.to_le_bytes(),
        &0xFFFF_FFFFu32.to_le_bytes(),
        &[1, 0],
        b"x",
        &[0],
        &[2, 0],
        b"Al",
        &[1, 0],
        b"a",
        &[0],
    ]);
    let m: ItemGetInscriptionResponse = read_body(&b).expect("decodes");
    assert_eq!(m.object.0, 0x8000_0001);
    assert_eq!(m.unknown, 0xFFFF_FFFF);
    assert_eq!(
        (
            m.inscription.as_str(),
            m.scribe_name.as_str(),
            m.scribe_account.as_str()
        ),
        ("x", "Al", "a")
    );
}

/// ACE's `GameEventJoinGameResponse`, `StartGame`, `MoveResponse` and `GameOver` each write the board's
/// guid then one `int`; one value alone is refused.
#[test]
fn the_board_game_responses_carry_the_board_and_one_int() {
    let b = cat(&[&0x7A9B_0001u32.to_le_bytes(), &(-1i32).to_le_bytes()]);
    let join: GameJoinGameResponse = read_body(&b).expect("decodes");
    assert_eq!((join.game_id, join.team), (0x7A9B_0001, -1));
    let start: GameStartGame = read_body(&b).expect("decodes");
    assert_eq!((start.game_id, start.team), (0x7A9B_0001, -1));
    let moved: GameMoveResponse = read_body(&b).expect("decodes");
    assert_eq!((moved.game_id, moved.result), (0x7A9B_0001, -1));
    let over: GameGameOver = read_body(&b).expect("decodes");
    assert_eq!((over.game_id, over.team_winner), (0x7A9B_0001, -1));
    assert!(read_body::<GameJoinGameResponse>(&b[..4]).is_err());
    assert!(read_body::<GameGameOver>(&b[..4]).is_err());
}

/// `09-social.md` §3.2: no server source writes `0x0003`; the retail handler reads one dword and
/// ignores it.
#[test]
fn allegiance_update_aborted_carries_one_dword() {
    let m: AllegianceUpdateAborted = read_body(&7u32.to_le_bytes()).expect("decodes");
    assert_eq!(m.failure_type, 7);
    assert!(read_body::<AllegianceUpdateAborted>(&[]).is_err());
}
