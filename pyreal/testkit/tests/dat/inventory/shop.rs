use pyreal_client_contract::view::ShopView;
use pyreal_client_model::{RecordingRequests, RecordingSink, World};
use pyreal_primitives::ObjectId;

use super::{a_gameplay_client, the_recorded_grocer};

// -----------------------------------------------------------------------------------------
// The kinds a thing can be, as the shipped strip names them. Written as literals rather than
// read back through the client's own symbol, for the reason the money lines are: a scenario
// that read the constant it is checking could not notice a wrong constant.
// -----------------------------------------------------------------------------------------
const FOOD: u32 = 0x0000_0020;
const ARMOUR: u32 = 0x0000_0002;

// =========================================================================================
// The shop snapshot the client's own seam builds out of a recorded shop message. Where a
// scenario needs a supply the recordings do not carry, only the *message* is rewritten.
// =========================================================================================

/// The recorded grocer's own world: the shop message that sells food and no armour, handled
/// by the client's own handler.
fn a_shop_that_sells_food() -> World {
    a_shop_that_sells_food_with(|_| {})
}

/// The same, with the shop's own message rewritten before the client sees it.
fn a_shop_that_sells_food_with(edit: impl Fn(&mut pyreal_protocol::trade::VendorInfo)) -> World {
    let mut info = the_recorded_grocer();
    edit(&mut info);
    let mut w = World::new();
    w.handle_vendor_info(
        &info,
        &mut RecordingSink::default(),
        &mut RecordingRequests::default(),
        pyreal_primitives::ServerTime(0.0),
    );
    w
}

/// A thing the player owns keeps its own kind when a shop advertises it.
///
/// All three arms, with the thing's kind and the shop's word for it deliberately different,
/// so a client that read only one of them fails on one arm. Both readers of the same answer
/// are checked: the row's own kind and the strip the categories are built from.
pub fn a_thing_the_player_owns_keeps_its_own_kind_when_the_shop_advertises_it() {
    let mut c = a_gameplay_client();
    let mut w = a_shop_that_sells_food();
    let stock = w.shop.stock[0].iid;
    // The shop says Food; the thing we are about to plant says Armour. Nothing a shard could
    // send would produce this pair, which is the point.
    w.shop.stock[0].pwd.obj_type = FOOD;
    let no_other_row_is_armour = w.shop.stock[1..]
        .iter()
        .all(|p| p.pwd.obj_type & ARMOUR == 0);

    // Arm 1 -- the client has never seen the thing.
    let a = pyreal_client_runtime::vendor_view::shop(&w);
    let unseen = a
        .stock
        .iter()
        .find(|r| r.item == stock)
        .expect("the row")
        .obj_type
        == FOOD
        && a.type_filters.iter().any(|(n, _)| *n == "Food")
        && !a.type_filters.iter().any(|(n, _)| *n == "Armor");

    // Arm 2 -- the client holds it and the player does not own it.
    let player = ObjectId(0x5000_000A);
    w.player = Some(player);
    let mut wn = pyreal_client_model::weenie::Weenie::new(stock);
    wn.pwd = pyreal_protocol::types::PublicWeenieDesc {
        name: "Fried Chicken".into(),
        obj_type: ARMOUR,
        ..pyreal_protocol::types::PublicWeenieDesc::default()
    };
    w.tables.weenies.insert(stock, wn);
    let nobody_owns_it = !w.is_owned_by_player(stock);
    let b = pyreal_client_runtime::vendor_view::shop(&w);
    let unowned = b
        .stock
        .iter()
        .find(|r| r.item == stock)
        .expect("the row")
        .obj_type
        == FOOD
        && b.type_filters.iter().any(|(n, _)| *n == "Food")
        && !b.type_filters.iter().any(|(n, _)| *n == "Armor");

    // Arm 3 -- the player owns it, so it keeps its own kind.
    w.tables
        .weenies
        .get_mut(stock)
        .expect("the planted thing")
        .pwd
        .container_id = Some(player);
    let the_player_owns_it = w.is_owned_by_player(stock);
    let d = pyreal_client_runtime::vendor_view::shop(&w);
    let owned = d
        .stock
        .iter()
        .find(|r| r.item == stock)
        .expect("the row")
        .obj_type
        == ARMOUR
        && d.type_filters.iter().any(|(n, _)| *n == "Armor");

    // The three arms really are three answers and not three restatements of one.
    let names = |v: &ShopView| v.type_filters.iter().map(|(n, _)| *n).collect::<Vec<_>>();
    let the_arms_differ = names(&a) == names(&b) && names(&b) != names(&d);

    c.assert_behaviour(
        "vendor.filter.a-thing-the-player-owns-keeps-its-own-kind-when-the-shop-advertises-it",
        move |_| {
            no_other_row_is_armour
                && unseen
                && nobody_owns_it
                && unowned
                && the_player_owns_it
                && owned
                && the_arms_differ
        },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// o740: the supply the basket has taken, the stack a row is offered in, and the container.
// -----------------------------------------------------------------------------------------

/// Setting a row's stack keeps its unit price and lets the quantity dial mean something.
pub fn setting_a_rows_stack_keeps_its_unit_price_and_lets_the_quantity_dial_mean_something() {
    let mut c = a_gameplay_client();
    let target = ObjectId(0x8000_0A6E);
    let mut info = the_recorded_grocer();
    info.items.truncate(1);
    {
        let p = &mut info.items[0];
        p.iid = target;
        p.amount = -1;
        let pwd = p
            .pwd
            .as_mut()
            .expect("the recorded row describes its thing");
        pwd.stack_size = Some(1);
        pwd.max_stack_size = Some(100);
        pwd.value = Some(50);
    }
    let mut w = World::new();
    w.handle_vendor_info(
        &info,
        &mut RecordingSink::default(),
        &mut RecordingRequests::default(),
        pyreal_primitives::ServerTime(0.0),
    );

    let one_costs = w.shop.profile.vendor_sell_price(&w.shop.stock[0].pwd, 1);

    // Before: the stack is one, so the add-to-list button ignores the dial.
    let before = w.add_to_buy_list(target, 7) && w.shop.buy_list == vec![(target, 1)];
    w.shop.buy_list.clear();

    let it_was_set = w.set_object_stack_size(target, 100);
    let the_stack_and_the_price_moved_together = {
        let pwd = &w.shop.stock[0].pwd;
        pwd.stack_size == Some(100) && pwd.value == Some(50 * 100)
    };
    let one_still_costs_the_same =
        w.shop.profile.vendor_sell_price(&w.shop.stock[0].pwd, 1) == one_costs;

    // After: the same press takes the dialled number.
    let after = w.add_to_buy_list(target, 7) && w.shop.buy_list == vec![(target, 7)];

    // Doing it again changes nothing, which matters because the shelf is refilled on every
    // redraw.
    let again = w.set_object_stack_size(target, 100)
        && w.shop.stock[0].pwd.value == Some(50 * 100)
        && w.shop.stock[0].pwd.stack_size == Some(100);

    // A thing with no stack of its own at all is one of it, not none.
    w.shop.stock[0].pwd.stack_size = None;
    w.shop.stock[0].pwd.value = Some(9);
    let no_stack_is_one =
        w.set_object_stack_size(target, 4) && w.shop.stock[0].pwd.value == Some(36);

    c.assert_behaviour(
            "vendor.shelf.setting-a-rows-stack-keeps-its-unit-price-and-lets-the-quantity-dial-mean-something",
            move |_| {
                before
                    && it_was_set
                    && the_stack_and_the_price_moved_together
                    && one_still_costs_the_same
                    && after
                    && again
                    && no_stack_is_one
            },
        );
    c.shutdown();
}

/// How big a stack a row can be sold in comes from the same place its kind does.
pub fn what_a_row_stacks_to_comes_from_the_same_place_its_kind_does() {
    let mut c = a_gameplay_client();
    let mut w = a_shop_that_sells_food();
    let stock = w.shop.stock[0].iid;
    w.shop.stock[0].pwd.max_stack_size = Some(100);
    let row_max = |w: &World| {
        pyreal_client_runtime::vendor_view::shop(w)
            .stock
            .iter()
            .find(|r| r.item == stock)
            .expect("the row")
            .max_stack_size
    };

    // Arm 1 -- the client has never seen the thing.
    let unseen = row_max(&w) == 100;

    // Arm 2 -- it holds it and the player does not own it.
    let player = ObjectId(0x5000_000A);
    w.player = Some(player);
    let mut wn = pyreal_client_model::weenie::Weenie::new(stock);
    wn.pwd = pyreal_protocol::types::PublicWeenieDesc {
        max_stack_size: Some(7),
        ..pyreal_protocol::types::PublicWeenieDesc::default()
    };
    w.tables.weenies.insert(stock, wn);
    let nobody_owns_it = !w.is_owned_by_player(stock);
    let unowned = row_max(&w) == 100;

    // Arm 3 -- the player owns it, so its own answer stands.
    w.tables
        .weenies
        .get_mut(stock)
        .expect("the planted thing")
        .pwd
        .container_id = Some(player);
    let the_player_owns_it = w.is_owned_by_player(stock);
    let owned = row_max(&w) == 7;

    c.assert_behaviour(
        "vendor.shelf.what-a-row-stacks-to-comes-from-the-same-place-its-kind-does",
        move |_| unseen && nobody_owns_it && unowned && the_player_owns_it && owned,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// The tests that run the scenarios above and check their declarations.
// -----------------------------------------------------------------------------------------

pyreal_testkit::scenarios! {
    scenario_a_thing_the_player_owns_keeps_its_own_kind_when_the_shop_advertises_it => a_thing_the_player_owns_keeps_its_own_kind_when_the_shop_advertises_it ["vendor.filter.a-thing-the-player-owns-keeps-its-own-kind-when-the-shop-advertises-it"],
    scenario_setting_a_rows_stack_keeps_its_unit_price_and_lets_the_quantity_dial_mean_something => setting_a_rows_stack_keeps_its_unit_price_and_lets_the_quantity_dial_mean_something ["vendor.shelf.setting-a-rows-stack-keeps-its-unit-price-and-lets-the-quantity-dial-mean-something"],
    scenario_what_a_row_stacks_to_comes_from_the_same_place_its_kind_does => what_a_row_stacks_to_comes_from_the_same_place_its_kind_does ["vendor.shelf.what-a-row-stacks-to-comes-from-the-same-place-its-kind-does"],
}
