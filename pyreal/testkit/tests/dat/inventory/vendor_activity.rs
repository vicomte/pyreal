use super::*;

pub(super) const RECORDED_PLAYER: ObjectId = ObjectId(0x5000_000A);

pub(super) fn dword(b: &[u8], n: usize) -> u32 {
    u32::from_le_bytes([b[n], b[n + 1], b[n + 2], b[n + 3]])
}

const COIN_AFTER_SALE: i32 = 9_988;
const AFTER_THE_SALE_COIN_AT: usize = 4_788;

/// A player with a pack and, optionally, some money.
///
/// `None` is not "poor": it is the shard having said nothing about the player's coin at all,
/// which is a different state.
fn a_player_with_coin(coin: Option<i32>) -> World {
    let mut w = World::new();
    let player = ObjectId(0x5000_000A);
    w.player = Some(player);
    let mut wn = pyreal_client_model::Weenie::new(player);
    wn.pwd = pyreal_protocol::types::PublicWeenieDesc {
        name: "Lark".into(),
        obj_type: pyreal_rules::weenie::item_type::CREATURE,
        bitfield: pyreal_rules::weenie::bitfield::OPENABLE,
        items_capacity: Some(10),
        containers_capacity: Some(2),
        ..pyreal_protocol::types::PublicWeenieDesc::default()
    };
    let mut q = pyreal_client_model::qualities::Qualities::new();
    if let Some(v) = coin {
        // **The literal 20, not the symbol the code under test reads.**
        q.ints
            .get_or_insert_with(std::collections::BTreeMap::new)
            .insert(20, v);
    }
    wn.qualities = Some(q);
    w.tables.weenies.insert(player, wn);
    w
}

/// The recording's own grocer, chosen by **what it sells** rather than by an index, so that it
/// does not go stale as the corpus grows.
pub(super) fn the_recorded_grocer() -> pyreal_protocol::trade::VendorInfo {
    let blobs = pyreal_client_net::client_session::testing::Corpus::load("long-solo-play")
        .expect("the locked corpus decodes")
        .expect("long-solo-play is locked")
        .blobs;
    let mut infos: Vec<pyreal_protocol::trade::VendorInfo> = Vec::new();
    for b in &blobs {
        if b.dir != pyreal_client_net::client_session::testing::Direction::ServerToClient
            || b.opcode != 0xF7B0
            || b.payload.len() < 16
        {
            continue;
        }
        let sub = u32::from_le_bytes([b.payload[12], b.payload[13], b.payload[14], b.payload[15]]);
        if sub != 0x0062 {
            continue;
        }
        if let Ok(m) =
            pyreal_protocol::read_body::<pyreal_protocol::trade::VendorInfo>(&b.payload[16..])
        {
            infos.push(m);
        }
    }
    assert!(
        infos.len() >= 2,
        "the premise: the recording carries the vendor traffic; {} decoded",
        infos.len()
    );
    infos
        .into_iter()
        .find(|m| {
            let mut probe = World::new();
            probe.handle_vendor_info(
                m,
                &mut RecordingSink::default(),
                &mut RecordingRequests::default(),
                pyreal_primitives::ServerTime(0.0),
            );
            probe.shop.list_contains_type(0x0000_0020)
                && !probe.shop.list_contains_type(0x0000_0002)
        })
        .expect("a recorded shop that sells food and no armour")
}

fn open_the_grocer(w: &mut World) {
    w.handle_vendor_info(
        &the_recorded_grocer(),
        &mut RecordingSink::default(),
        &mut RecordingRequests::default(),
        pyreal_primitives::ServerTime(0.0),
    );
}

/// Opening a shop fills its purse from the player's own money, and a player the shard has said
/// nothing about is refused every purchase.
///
/// The first half is the finding and is asserted first: a scenario that only checked the second
/// would pass on a client that made every player rich.
pub(super) fn the_purse_is_filled_from_the_players_own_coin() {
    let mut c = a_retail_client();

    let mut probe = World::new();
    open_the_grocer(&mut probe);
    let item = probe.shop.stock.first().expect("the shop has stock").iid;
    let price = {
        let pwd = probe.shop.stock[0].pwd.clone();
        probe.shop.profile.vendor_sell_price_on(true, &pwd, 1)
    };
    assert!(price > 0, "the premise: the first row costs something");

    // (a) A player the shard has said nothing about.
    let mut w = a_player_with_coin(None);
    let mut out = RecordingSink::default();
    let mut req = RecordingRequests::default();
    open_the_grocer(&mut w);
    let penniless = w.shop.total_value == 0;
    req.0.clear();
    let refused = w.buy_single_item(
        item,
        1,
        &mut req,
        &mut out,
        pyreal_primitives::ServerTime(1.0),
    ) == Err(pyreal_client_model::vendor::BuyRefusal::NotEnoughMoney)
        && req.0.is_empty();

    // (b) The same player, with money.
    let mut w = a_player_with_coin(Some(1_000_000));
    let mut out = RecordingSink::default();
    let mut req = RecordingRequests::default();
    open_the_grocer(&mut w);
    let filled = w.shop.total_value == 1_000_000;
    req.0.clear();
    let bought = w.buy_single_item(
        item,
        1,
        &mut req,
        &mut out,
        pyreal_primitives::ServerTime(1.0),
    ) == Ok(true)
        && req.0.len() == 1
        && pyreal_client_runtime::vendor_view::shop(&w).total_value == 1_000_000;

    c.assert_behaviour(
        "vendor.purse.is-filled-from-the-players-own-coin-when-the-shop-opens",
        move |_| penniless && refused && filled && bought,
    );
    c.shutdown();
}

// =============================================================================================
// One store for the player's coin, because there is exactly one.
//
// The shard sums the coin and sends the total, and it **suppresses the send when the total has
// not changed** -- so a client that drops one never gets a spontaneous correction.
// =============================================================================================

const LAST_SHOP_AT: usize = 4784;
/// The number the shard put on the last coin update of the recording, and its own counter.
const SEQ_OF_THE_SALE: u8 = 5;
/// The coin property, as a number. Never the symbol the code under test reads it through.
const COIN_PROPERTY: u32 = 20;

/// One coin update for the player, in the public form, carrying `seq` as its own counter.
fn the_shard_says_the_player_has(seq: u8, value: i32) -> Inbound {
    let mut p = Vec::with_capacity(17);
    p.extend_from_slice(&0x02CE_u32.to_le_bytes());
    p.push(seq);
    p.extend_from_slice(&RECORDED_PLAYER.0.to_le_bytes());
    p.extend_from_slice(&COIN_PROPERTY.to_le_bytes());
    p.extend_from_slice(&value.to_le_bytes());
    Inbound::event(pyreal_client_net::client_session::SessionEvent::UiEvent {
        opcode: pyreal_protocol::Opcode(0x02CE),
        blob: p,
    })
}

/// The two forms a coin update arrives in -- the one addressed to the player and the one that
/// leaves the player implied -- share **one** counter, so an update older than the last one
/// already taken is dropped, and a newer one lands.
///
/// That is the sentence "there is one store" made falsifiable: two stores, each with a counter
/// of its own, would both have accepted the stale update. The shop is re-opened with the
/// recording's own last shop message between the update and the reading, so what is measured is
/// the **store** and not a number the shop was already holding.
pub(super) fn a_stale_coin_update_for_the_player_is_dropped_by_the_same_counter() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.world_mut().player = Some(RECORDED_PLAYER);
    c.when(Inbound::from_corpus(
        "long-solo-play",
        0..AFTER_THE_SALE_COIN_AT + 5,
    ))
    .tick(4);
    let the_last_word = c.view().world().shop.total_value == COIN_AFTER_SALE;

    let reopen = || Inbound::from_corpus("long-solo-play", LAST_SHOP_AT..LAST_SHOP_AT + 1);

    c.when(the_shard_says_the_player_has(SEQ_OF_THE_SALE - 1, 1))
        .tick(1);
    c.when(reopen()).tick(3);
    let the_stale_one_was_dropped = c.view().world().shop.total_value == COIN_AFTER_SALE;

    c.when(the_shard_says_the_player_has(SEQ_OF_THE_SALE + 1, 12_345))
        .tick(1);
    c.when(reopen()).tick(3);
    let the_newer_one_landed = c.view().world().shop.total_value == 12_345;

    c.assert_behaviour(
        "vendor.purse.a-stale-coin-update-for-the-player-is-dropped-by-the-same-counter",
        move |_| the_last_word && the_stale_one_was_dropped && the_newer_one_landed,
    );
    c.shutdown();
}
