use super::*;

// =============================================================================================
// house.*
//
// What the client keeps of the shard's house messages, and what it asks for.
//
// # The payload is laid out by hand, and that is deliberate
//
// No recorded session carries the shard's house message: a shard only sends one to an account
// that owns a house and the recorded character owns none. So the bytes are written out by hand in
// the shard's own writer order -- as raw dwords rather than through this tree's own encoder, so
// that a decoder regression cannot hide by agreeing with the encoder that produced its input.
// =============================================================================================

/// The object this family's shard talks about, which is the player's own body.
pub(super) const HOUSE_PLAYER: ObjectId = ObjectId(0x5000_0001);

/// The quality that says when the player last bought a house.
const HOUSE_PURCHASE_TIMESTAMP: u32 = 0xC7;

/// Holtburg, a cell a house can stand in.
const HOUSE_CELL: u32 = 0xA9B4_0025;

/// The house type of a villa, and its maintenance period in seconds.
const VILLA: u32 = 2;
const THIRTY_DAYS: i64 = 2_592_000;

/// One payment of a price list: how many, how many are paid, which thing, and its two names.
///
/// The three numbers are in the shard's **member** order and not its layout order -- writing the
/// thing's id first decodes a price list with the quantity in the wrong field.
fn payment(num: i32, paid: i32, wcid: u32, name: &str, plural: &str) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&num.to_le_bytes());
    b.extend_from_slice(&paid.to_le_bytes());
    b.extend_from_slice(&wcid.to_le_bytes());
    for s in [name, plural] {
        b.extend_from_slice(&u16::try_from(s.len()).expect("a short name").to_le_bytes());
        b.extend_from_slice(s.as_bytes());
        while b.len() % 4 != 0 {
            b.push(0);
        }
    }
    b
}

/// A bare price list, which is the whole body of the maintenance-payment update.
fn payment_list(items: &[Vec<u8>]) -> Vec<u8> {
    let mut b = u32::try_from(items.len())
        .expect("a short list")
        .to_le_bytes()
        .to_vec();
    for p in items {
        b.extend_from_slice(p);
    }
    b
}

/// The whole house body: the two instants, the type, whether maintenance is waived, the two price
/// lists and the position -- in the shard's own order.
fn house_data_body(
    buy_time: i32,
    rent_time: i32,
    house_type: u32,
    maintenance_free: i32,
    buy: &[Vec<u8>],
    rent: &[Vec<u8>],
    cell: u32,
) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&buy_time.to_le_bytes());
    b.extend_from_slice(&rent_time.to_le_bytes());
    b.extend_from_slice(&house_type.to_le_bytes());
    b.extend_from_slice(&maintenance_free.to_le_bytes());
    for list in [buy, rent] {
        b.extend_from_slice(
            &u32::try_from(list.len())
                .expect("a short list")
                .to_le_bytes(),
        );
        for p in list {
            b.extend_from_slice(p);
        }
    }
    b.extend_from_slice(&cell.to_le_bytes());
    for f in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0] {
        b.extend_from_slice(&f.to_le_bytes());
    }
    b
}

/// A villa bought outright whose maintenance is two thirds paid -- so the pane takes its
/// *warning* arm, which is the one a happy path never reaches.
fn the_villa(buy_time: i32, rent_time: i32) -> Vec<u8> {
    house_data_body(
        buy_time,
        rent_time,
        VILLA,
        0,
        &[payment(30_000, 30_000, 273, "Pyreal", "Pyreals")],
        &[payment(30_000, 20_000, 273, "Pyreal", "Pyreals")],
        HOUSE_CELL,
    )
}

fn house_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .expect("a clock after 1970")
}

/// The frames the client needs after something arrives.
fn house_settle(c: &mut HeadlessClient) {
    c.tick(6);
}

/// One ordered event with a **hand-laid body**, through the harness's own shard.
///
/// [`Peer::event`] takes a typed message and encodes it, which is exactly what these claims may
/// not do; [`Peer::replay_blob`] re-addresses and re-stamps a blob the caller framed, so the
/// envelope is the shard's and the body is the scenario's.
pub(super) fn house_event(
    c: &mut HeadlessClient,
    peer: &mut Peer,
    opcode: pyreal_protocol::Opcode,
    body: &[u8],
) {
    let mut blob = 0xF7B0_u32.to_le_bytes().to_vec();
    // The recipient and the stamp `replay_blob` overwrites.
    blob.extend_from_slice(&[0_u8; 8]);
    blob.extend_from_slice(&opcode.0.to_le_bytes());
    blob.extend_from_slice(body);
    peer.replay_blob(c, blob);
    house_settle(c);
}

/// Tell the client how long ago the player bought a house.
fn purchased_at(c: &mut HeadlessClient, peer: &mut Peer, sequence: u8, when: i64) {
    let m = pyreal_protocol::qualities::QualitiesPrivateUpdateInt(
        pyreal_protocol::qualities::PrivateUpdate {
            sequence,
            property_id: HOUSE_PURCHASE_TIMESTAMP,
            value: i32::try_from(when).expect("an instant that fits"),
        },
    );
    peer.event(c, &m);
    house_settle(c);
}

/// A client in the world with a shard attached, a player it has adopted, and its description.
pub(super) fn a_house_client() -> (HeadlessClient, Peer) {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut peer = Peer::attach_creating(&mut c, HOUSE_PLAYER);
    {
        let w = c.world_mut();
        w.player = Some(HOUSE_PLAYER);
        w.weenie_mut(HOUSE_PLAYER)
            .expect("this shard created the player")
            .pwd
            .name = "Larktest".to_owned();
    }
    peer.event(
        &mut c,
        &pyreal_protocol::login::LoginPlayerDescription::default(),
    );
    house_settle(&mut c);
    assert!(
        c.view().world().player_qualities().is_some(),
        "the client has been told who the player is"
    );
    (c, peer)
}

/// Each payment of a price list as `(paid, num, wcid)`.
fn paid(list: &pyreal_client_model::housing::HousePaymentList) -> Vec<(i32, i32, u32)> {
    list.0.iter().map(|p| (p.paid, p.num, p.wcid)).collect()
}

/// The house the client keeps, or a panic naming the premise.
fn the_house(c: &HeadlessClient) -> pyreal_client_model::housing::HouseData {
    c.view()
        .world()
        .house
        .clone()
        .expect("the shard's house message left the client a house")
}

/// Learning who the player is asks the shard about their house, once -- and a second telling asks
/// nothing, while re-arming the flag the way a log-off does asks again.
///
/// The last step is what makes the zero in the middle a guard rather than a send path that never
/// worked.
pub(super) fn the_client_asks_about_the_house_once_when_it_learns_who_it_is() {
    let (mut c, mut peer) = a_house_client();

    let initialized = c.view().world().player_initialized;
    // The requests are drained every frame, so the counters are the durable record and the
    // outbound list is what names the one that went.
    let sent = |c: &HeadlessClient| {
        let s = &c.view().interaction().stats;
        s.requests_sent + s.requests_undeliverable
    };
    let after_first = sent(&c);
    let named = c
        .view()
        .outbound()
        .iter()
        .any(|r| matches!(r, pyreal_client_model::Request::QueryHouse(_)));

    let desc = pyreal_protocol::login::LoginPlayerDescription::default();
    peer.event(&mut c, &desc);
    house_settle(&mut c);
    let after_second = sent(&c);

    c.world_mut().player_initialized = false;
    peer.event(&mut c, &desc);
    house_settle(&mut c);
    let after_third = sent(&c);

    c.assert_behaviour(
        "house.query.the-client-asks-about-the-house-once-when-it-learns-who-it-is",
        move |_| {
            initialized && named && after_second == after_first && after_third == after_first + 1
        },
    );
    c.shutdown();
}

/// A new maintenance period moves the period's start and puts every payment back to nothing paid.
pub(super) fn a_new_maintenance_period_clears_every_payment() {
    let (mut c, mut peer) = a_house_client();
    let rent_time = 1_600_000_000_i32;
    house_event(
        &mut c,
        &mut peer,
        pyreal_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(1_500_000_000, rent_time),
    );
    let before = the_house(&c);

    let next = rent_time + i32::try_from(THIRTY_DAYS).expect("a period that fits");
    house_event(
        &mut c,
        &mut peer,
        pyreal_protocol::Opcode::HOUSE_UPDATE_RENT_TIME,
        &next.to_le_bytes(),
    );

    let stats = c.view().expect_app().hud().stats;
    let after = the_house(&c);

    c.assert_behaviour(
        "house.rent.a-new-period-moves-both-dates-and-marks-every-payment-unpaid",
        move |_| {
            before.rent_time == rent_time
                && paid(&before.rent) == vec![(20_000, 30_000, 273)]
                && stats.house_rent_time_updates == 1
                && stats.house_rent_time_applied == 1
                && after.rent_time == next
                // A fresh period is an unpaid one, which is the whole point of clearing them.
                && paid(&after.rent) == vec![(0, 30_000, 273)]
                && paid(&after.buy) == paid(&before.buy)
        },
    );
    c.shutdown();
}

/// A maintenance-payment update replaces the list rather than merging into it, and leaves the
/// purchase price and the period alone.
pub(super) fn a_payment_update_replaces_the_list_rather_than_merging() {
    let (mut c, mut peer) = a_house_client();
    house_event(
        &mut c,
        &mut peer,
        pyreal_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(1_500_000_000, 1_600_000_000),
    );
    let before = the_house(&c);

    house_event(
        &mut c,
        &mut peer,
        pyreal_protocol::Opcode::HOUSE_UPDATE_RENT_PAYMENT,
        &payment_list(&[payment(30_000, 30_000, 273, "Pyreal", "Pyreals")]),
    );
    let stats = c.view().expect_app().hud().stats;
    let paid_up = the_house(&c);

    // ...and a shorter list really is shorter: a second update with one different payment in it
    // leaves nothing of the first behind.
    house_event(
        &mut c,
        &mut peer,
        pyreal_protocol::Opcode::HOUSE_UPDATE_RENT_PAYMENT,
        &payment_list(&[payment(2, 2, 21_073, "Pyreal Scarab", "Pyreal Scarabs")]),
    );
    let replaced = the_house(&c);

    c.assert_behaviour(
        "house.rent.a-payment-update-replaces-the-list-rather-than-merging-into-it",
        move |_| {
            stats.house_rent_payment_updates == 1
                && stats.house_rent_payment_applied == 1
                && paid(&paid_up.rent) == vec![(30_000, 30_000, 273)]
                && paid_up.rent.is_paid_in_full()
                && paid(&paid_up.buy) == paid(&before.buy)
                && paid_up.rent_time == before.rent_time
                && paid(&replaced.rent) == vec![(2, 2, 21_073)]
        },
    );
    c.shutdown();
}

/// Both maintenance updates arrive at a houseless character, and both are deliberately ignored:
/// no house is invented.
///
/// The pair of counters is what makes *arrived* and *applied* separable; without the second there
/// would be no telling a working guard from a message that never reached a receiver at all.
pub(super) fn neither_maintenance_update_touches_a_houseless_character() {
    let (mut c, mut peer) = a_house_client();
    house_settle(&mut c);
    let none_before = c.view().world().house.is_none();

    house_event(
        &mut c,
        &mut peer,
        pyreal_protocol::Opcode::HOUSE_UPDATE_RENT_TIME,
        &1_600_000_000_i32.to_le_bytes(),
    );
    house_event(
        &mut c,
        &mut peer,
        pyreal_protocol::Opcode::HOUSE_UPDATE_RENT_PAYMENT,
        &payment_list(&[payment(30_000, 30_000, 273, "Pyreal", "Pyreals")]),
    );

    let stats = c.view().expect_app().hud().stats;
    let invented = c.view().world().house.is_some();

    c.assert_behaviour(
        "house.rent.neither-update-touches-a-character-with-no-house",
        move |_| {
            none_before
                && stats.house_rent_time_updates == 1
                && stats.house_rent_payment_updates == 1
                && stats.house_rent_time_applied == 0
                && stats.house_rent_payment_applied == 0
                && !invented
        },
    );
    c.shutdown();
}

/// A list of who may enter is stored against the object the shard names it for, and one about the
/// player themselves is dropped rather than stored.
pub(super) fn a_restriction_update_reaches_the_object_it_names() {
    const OTHER: ObjectId = ObjectId(0x5000_0002);

    let (mut c, mut peer) = a_house_client();
    {
        let mut p = pyreal_protocol::objects::ObjectCreatePayload {
            id: OTHER,
            ..Default::default()
        };
        p.physicsdesc.bitfield |= pyreal_protocol::types::physicsdesc::flags::SETUP;
        p.physicsdesc.setup_id = Some(0x0200_0001);
        p.physicsdesc.timestamps.instance = 1;
        let blob = pyreal_protocol::write_blob(&pyreal_protocol::objects::ItemCreateObject(p))
            .expect("an object create encodes");
        peer.send(&mut c, 10, blob);
        house_settle(&mut c);
    }

    // The body is unaligned from its second byte, so it is laid out here: a sequence byte, the
    // object the list is about, the open-house flag, and an empty guest table -- whose header is
    // a single zero word, which is the third of the protocol's three hash-table headers and not
    // the one a first draft reaches for.
    let body = |seq: u8, who: ObjectId| -> Vec<u8> {
        let mut b = vec![seq];
        b.extend_from_slice(&who.0.to_le_bytes());
        b.extend_from_slice(&1_u32.to_le_bytes());
        b.extend_from_slice(&0_u32.to_le_bytes());
        b
    };

    house_event(
        &mut c,
        &mut peer,
        pyreal_protocol::Opcode::HOUSE_UPDATE_RESTRICTIONS,
        &body(1, OTHER),
    );
    let first = c.view().expect_app().hud().stats;
    let other_has = c
        .view()
        .world()
        .weenie(OTHER)
        .expect("the other object")
        .pwd
        .restrictions
        .is_some();

    house_event(
        &mut c,
        &mut peer,
        pyreal_protocol::Opcode::HOUSE_UPDATE_RESTRICTIONS,
        &body(1, HOUSE_PLAYER),
    );
    let second = c.view().expect_app().hud().stats;
    let player_has = c
        .view()
        .world()
        .weenie(HOUSE_PLAYER)
        .expect("the player")
        .pwd
        .restrictions
        .is_some();

    c.assert_behaviour(
        "house.restrictions.an-update-reaches-the-object-it-names-and-never-the-player",
        move |_| {
            first.house_restriction_updates == 1
                && first.house_restrictions_applied == 1
                && other_has
                && second.house_restriction_updates == 2
                && second.house_restrictions_applied == 1
                && !player_has
        },
    );
    c.shutdown();
}

/// The other answer a shard can give about a house that is gone forgets it exactly as the first
/// one does.
pub(super) fn a_transaction_answer_clears_the_pane_as_a_status_does() {
    let (mut c, mut peer) = a_house_client();
    purchased_at(&mut c, &mut peer, 1, house_now() - 40 * 86_400);
    house_event(
        &mut c,
        &mut peer,
        pyreal_protocol::Opcode::HOUSE_HOUSE_DATA,
        &the_villa(1_500_000_000, 1_600_000_000),
    );
    let owned = c.view().world().house.is_some();
    let notices = c.view().expect_app().hud().stats.house_status_notices;

    house_event(
        &mut c,
        &mut peer,
        pyreal_protocol::Opcode::HOUSE_HOUSE_TRANSACTION,
        &2_u32.to_le_bytes(),
    );

    let after_notices = c.view().expect_app().hud().stats.house_status_notices;
    let owns = c.view().world().house.is_some();

    c.assert_behaviour(
        "house.data.a-transaction-answer-clears-the-pane-exactly-as-a-status-answer-does",
        move |_| owned && after_notices == notices + 1 && !owns,
    );
    c.shutdown();
}
