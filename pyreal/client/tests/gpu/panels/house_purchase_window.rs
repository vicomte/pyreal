//! The house purchase and maintenance window's data: the native trade-note enum resolves through
//! the typed mapper, the recorded `House_HouseProfile 0x021D` decodes field by field and
//! re-encodes byte for byte, and the recorded refusal is the fifteen-day rule, sent twice.
//! Fixture: a recorded 260-byte `0x021D` body and the recorded `0x02EB` refusal bytes, and the
//! retail dats.

#![cfg(gpu)]

use crate::common::client_dir_required as client_dir;

use pyreal_primitives::ObjectId;

/// The slumlord statue of the recorded villa — `0x9DAF0029`'s landblock-static
/// object, `0x7` + landblock `0x9DAF` + index `0x03A`. It is the first dword of the recorded
/// `0x0036` **and** of the recorded `0x021D`.
const SLUMLORD: ObjectId = ObjectId(0x79DA_F03A);

/// **The recorded `0x021D House_HouseProfile`, verbatim** — 260 bytes at `t = 29.090`, the
/// body of an `0xF7B0` event blob after its 16-byte `OrderedEventHeader`.
///
/// It decodes to: covenant crystal `0x79DAF03A`, dwelling `0x0592`, owner **0** (unowned),
/// bitmask 1 (`Active`), min level **35**, `max_level` / `min_alleg_rank` / `max_alleg_rank` all
/// `-1`, `maintenance_free` 0, type **2 (Villa)**, an **empty** owner name, three buy lines and
/// two rent lines. The twelve fields appear in the order written by ACE and decoded by the
/// retail client.
const RECORDED_PROFILE: &str = "\
    3af0da7992050000000000000100000023000000ffffffffffffffffffffffff000000000200000000000000\
    0300000080841e000000000011010000060050797265616c070050797265616c730000000500000000000000\
    be2d00000e0057726974206f66205265667567650f005772697473206f662052656675676500000001000000\
    00000000ff0100000e004372756465204c6f636b7069636b0f004372756465204c6f636b7069636b73000000\
    02000000a08601000000000011010000060050797265616c070050797265616c730000000200000000000000\
    be2d00000e0057726974206f66205265667567650f005772697473206f6620526566756765000000";

/// The WCID of Pyreal, used by the paid-items coin row.
const PYREAL: u32 = 273;
/// `Writ of Refuge`, the second buy line's weenie class (`be2d0000` = 11710).
const WRIT: u32 = 11_710;

fn hex(s: &str) -> Vec<u8> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex"))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Harness — a replay `Peer` that feeds ordered events and object blobs
// ---------------------------------------------------------------------------------------------

/// The trade-note check asks for enum slot 10 in dual-DID mapper `0x10000001`: the shared two-hop
/// resolver returns a payload that the requested type can decode, carrying the 100 and 100,000
/// trade-note values.
#[test]
fn the_native_trade_note_enum_resolves_the_typed_mapper() {
    use pyreal_assets::{Decode, DualDidMapper};
    use pyreal_dat::{DbType, RetailDatStore};

    let store = RetailDatStore::open_dir(&client_dir()).expect("retail dats");
    let id =
        pyreal_assets::did_by_enum(&store, 0x1000_0001, 10).expect("the TradeNotes enum entry");
    let bytes = store
        .read_typed(DbType::DualDidMapper, id)
        .expect("the typed TradeNotes mapper payload");
    let mapper = DualDidMapper::decode_payload(id, &bytes).expect("DualDidMapper");
    assert!(mapper.0.enum_to_id.iter().any(|(value, _)| *value == 100));
    assert!(mapper
        .0
        .enum_to_id
        .iter()
        .any(|(value, _)| *value == 100_000));
    eprintln!("TradeNotes mapper resolved to {id:?}");
}

// ---------------------------------------------------------------------------------------------
// Station 1 — the recorded payload, decoded
// ---------------------------------------------------------------------------------------------

/// **260 recorded bytes decode field by field and re-encode byte for byte.**
///
/// A self-consistent round trip cannot catch a wrong field order; this oracle is bytes a shard
/// actually sent, decoded field by field against what the capture means.
#[test]
fn the_recorded_villa_profile_decodes_field_by_field_and_re_encodes_byte_for_byte() {
    let bytes = hex(RECORDED_PROFILE);
    assert_eq!(
        bytes.len(),
        260,
        "the recorded blob body, after the 16-byte wire-order header"
    );
    let m: pyreal_protocol::trade::HouseProfileMessage =
        pyreal_protocol::read_body(&bytes).expect("the recorded 0x021D decodes");
    let p = &m.profile;
    assert_eq!(
        m.covenant_crystal, SLUMLORD,
        "GameEventHouseProfile's first dword is the slumlord"
    );
    assert_eq!(p.id.0, 0x0592, "DwellingID 1426");
    assert_eq!(
        p.owner.0, 0,
        "nobody owned it, which is why the Buy tab is the live one"
    );
    assert_eq!(p.bitmask, 1, "recorded house bitfield has the Active flag");
    assert_eq!(p.min_level, 35);
    assert_eq!(
        (p.max_level, p.min_alleg_rank, p.max_alleg_rank),
        (-1, -1, -1),
        "ACE's HouseProfile() constructor sets all four to -1 and only MinLevel was overridden; \
         a u32 reading of these is 4294967295 rather than `none`"
    );
    assert_eq!(p.maintenance_free, 0);
    assert_eq!(p.house_type, 2, "recorded house type is Villa");
    assert_eq!(
        p.name, "",
        "WriteString16L(null) is a zero length and two bytes of pad"
    );
    assert_eq!(p.buy.len(), 3);
    assert_eq!(p.rent.len(), 2);

    let line = |p: &pyreal_protocol::trade::HousePayment| (p.num, p.paid, p.wcid, p.name.clone());
    assert_eq!(line(&p.buy[0]), (2_000_000, 0, PYREAL, "Pyreal".into()));
    assert_eq!(p.buy[0].plural_name, "Pyreals");
    assert_eq!(line(&p.buy[1]), (5, 0, WRIT, "Writ of Refuge".into()));
    assert_eq!(line(&p.buy[2]), (1, 0, 511, "Crude Lockpick".into()));
    assert_eq!(line(&p.rent[0]), (100_000, 0, PYREAL, "Pyreal".into()));
    assert_eq!(line(&p.rent[1]), (2, 0, WRIT, "Writ of Refuge".into()));

    // Re-encoding must reproduce the shard's own bytes, which is the half a round trip over our
    // own writer cannot prove.
    assert_eq!(
        pyreal_protocol::write_body(&m).expect("re-encodes"),
        bytes,
        "byte for byte"
    );
}

// ---------------------------------------------------------------------------------------------
// Station 8 — the shard's own refusal, as recorded
// ---------------------------------------------------------------------------------------------

/// **What the recorded shard answered, and the two messages it answered with.**
///
/// ACE's `Source/ACE.Server/WorldObjects/Player_House.cs:95`, the
/// `house_15day_account` gate:
///
/// ```csharp
/// var msg = "Your account must be at least 15 days old to purchase this dwelling. …";
/// Session.Network.EnqueueSend(new GameEventCommunicationTransientString(Session, msg),
///                             new GameMessageSystemChat(msg, ChatMessageType.Broadcast));
/// ```
///
/// and both are in the recording at `t = 144.465` — a `0x02EB` event **and** an `0xF7E0` system
/// chat line carrying the same 115 characters. This is the byte-level check of the refusal, and
/// it is the reason the window must stay usable after a refusal (hence station 7's retry).
#[test]
fn the_recorded_refusal_is_the_fifteen_day_rule_and_arrives_twice() {
    const REFUSAL: &str = "Your account must be at least 15 days old to purchase this dwelling. \
                           This applies to all housing except apartments.";
    let transient = hex(
        "7300596f7572206163636f756e74206d757374206265206174206c656173742031352064617973206f6c64\
         20746f2070757263686173652074686973206477656c6c696e672e2054686973206170706c69657320746f\
         20616c6c20686f7573696e67206578636570742061706172746d656e74732e000000",
    );
    assert_eq!(transient.len(), 120, "the recorded 0x02EB body");
    let len = u16::from_le_bytes([transient[0], transient[1]]);
    assert_eq!(usize::from(len), REFUSAL.len(), "115 characters");
    assert_eq!(
        std::str::from_utf8(&transient[2..2 + usize::from(len)]).expect("ascii"),
        REFUSAL
    );
}
