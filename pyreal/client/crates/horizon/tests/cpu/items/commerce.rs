//! The shop and trade windows follow the game: each opens with the game's vendor or trade, and
//! closing the window closes it in the game. A right-click on a row of the vendor's stock
//! examines it; on a row of the shopping list or of the items to sell it takes it back off.

use pyreal_client_contract::view::{ShopRow, ShopView, TradeView};
use pyreal_client_contract::UiRequest;
use pyreal_horizon::draw::Rect;
use pyreal_horizon::ui::game::GameState;
use pyreal_horizon::ui::panels::WindowId;
use pyreal_primitives::ObjectId;

use crate::Harness;

const BREAD: ObjectId = ObjectId(0x8000_0101);
const ARROWS: ObjectId = ObjectId(0x8000_0102);
const SWORD: ObjectId = ObjectId(0x8000_0103);

fn in_world() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    }
}

#[test]
fn a_vendor_opens_the_shop_and_closing_the_shop_closes_the_vendor() {
    let mut h = Harness::new(Default::default());
    let mut state = in_world();
    state.shop = Some(ShopView {
        open: true,
        ..ShopView::default()
    });
    h.frame(&state);
    assert!(h.ui.windows.is_open(WindowId::Vendor));
    h.ui.windows.close(WindowId::Vendor);
    let out = h.frame(&state);
    assert!(
        out.requests.contains(&UiRequest::VendorClose),
        "{:?}",
        out.requests
    );
}

#[test]
fn the_vendor_closing_closes_the_shop() {
    let mut h = Harness::new(Default::default());
    let mut state = in_world();
    state.shop = Some(ShopView {
        open: true,
        ..ShopView::default()
    });
    h.frame(&state);
    state.shop = None;
    let out = h.frame(&state);
    assert!(!h.ui.windows.is_open(WindowId::Vendor));
    assert!(!out.requests.contains(&UiRequest::VendorClose));
}

#[test]
fn a_trade_opens_the_trade_window_and_closing_it_closes_the_trade() {
    let mut h = Harness::new(Default::default());
    let mut state = in_world();
    state.trade = Some(TradeView {
        open: true,
        ..TradeView::default()
    });
    h.frame(&state);
    assert!(h.ui.windows.is_open(WindowId::Trade));
    h.ui.windows.close(WindowId::Trade);
    let out = h.frame(&state);
    assert!(
        out.requests.contains(&UiRequest::TradeClose),
        "{:?}",
        out.requests
    );
}

fn row(item: ObjectId, name: &str) -> ShopRow {
    ShopRow {
        item,
        name: name.into(),
        amount: 1,
        obj_type: 0x80,
        price: 10,
        ..ShopRow::default()
    }
}

/// A vendor open on its Buy tab (or on its Sell tab), with the shopping list and the items to
/// sell given.
fn shopping(sell_tab: bool, buy_list: Vec<ShopRow>, sell_list: Vec<ShopRow>) -> GameState {
    let mut state = in_world();
    state.shop = Some(ShopView {
        open: true,
        sell_mode: sell_tab,
        buy_list,
        sell_list,
        ..ShopView::default()
    });
    state
}

/// The rows of the vendor window's lists, top to bottom: what the pad can stop on in the window
/// that is a row's height (34 at this scale), where its buttons are shorter.
fn rows(h: &Harness) -> Vec<Rect> {
    let window =
        h.ui.windows
            .rects(1.0)
            .into_iter()
            .find(|(w, _)| *w == WindowId::Vendor)
            .map(|(_, r)| r)
            .expect("the vendor window is open");
    let mut rows: Vec<Rect> = h
        .input
        .nav
        .last
        .targets
        .iter()
        .map(|t| t.rect)
        .filter(|r| window.contains(r.x + 1.0, r.y + 1.0) && (r.h - 34.0).abs() < 0.5)
        .collect();
    rows.sort_by(|a, b| (a.y, a.x).partial_cmp(&(b.y, b.x)).unwrap());
    rows
}

fn right_click(h: &mut Harness, state: &GameState, r: Rect) -> Vec<UiRequest> {
    h.move_to(r.x + r.w / 2.0, r.y + r.h / 2.0);
    h.input.down[1] = true;
    h.input.pressed[1] = true;
    let out = h.frame(state);
    h.input.down[1] = false;
    h.input.released[1] = true;
    h.frame(state);
    out.requests
}

#[test]
fn a_right_click_on_a_row_of_the_shopping_list_takes_it_back_off() {
    let state = shopping(
        false,
        vec![row(BREAD, "Bread"), row(ARROWS, "Arrows")],
        Vec::new(),
    );
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.frame(&state);
    let rows = rows(&h);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(
        right_click(&mut h, &state, rows[1]),
        vec![UiRequest::VendorClearList {
            sell: false,
            item: Some(ARROWS),
        }]
    );
}

#[test]
fn a_right_click_on_a_row_of_the_items_to_sell_takes_it_back_off() {
    let state = shopping(true, Vec::new(), vec![row(SWORD, "Sword")]);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.frame(&state);
    let rows = rows(&h);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(
        right_click(&mut h, &state, rows[0]),
        vec![UiRequest::VendorClearList {
            sell: true,
            item: Some(SWORD),
        }]
    );
}

#[test]
fn a_right_click_on_a_row_of_the_vendors_stock_examines_its_item() {
    let mut state = shopping(false, Vec::new(), Vec::new());
    if let Some(shop) = state.shop.as_mut() {
        shop.stock = vec![row(BREAD, "Bread"), row(SWORD, "Sword")];
    }
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.frame(&state);
    let rows = rows(&h);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(
        right_click(&mut h, &state, rows[1]),
        vec![UiRequest::Select(SWORD), UiRequest::Examine(SWORD)]
    );
}

/// The game answers every purchase with the stock advertised again, each stack back at the size
/// the shop gives it. The list sets a stackable row's size again whenever the object's own size is
/// not the one the list wants, so a second Buy takes the count shown and not one.
#[test]
fn a_stock_row_re_advertised_at_one_has_its_stack_size_set_again() {
    let mut h = Harness::new(Default::default());
    let mut state = in_world();
    let arrows = |stack_size| ShopRow {
        amount: -1,
        max_stack_size: 250,
        stack_size,
        ..row(ARROWS, "Arrow")
    };
    let open = |stock| ShopView {
        open: true,
        stock: vec![stock],
        ..ShopView::default()
    };
    let sized = |requests: &[UiRequest]| {
        requests
            .iter()
            .filter(|r| matches!(r, UiRequest::VendorSetObjectStackSize { .. }))
            .cloned()
            .collect::<Vec<_>>()
    };
    let set = vec![UiRequest::VendorSetObjectStackSize {
        item: ARROWS,
        size: 250,
    }];
    state.shop = Some(open(arrows(1)));
    assert_eq!(sized(&h.frame(&state).requests), set, "the first list");
    state.shop = Some(open(arrows(250)));
    assert!(
        sized(&h.frame(&state).requests).is_empty(),
        "already that size"
    );
    state.shop = Some(open(arrows(1)));
    assert_eq!(sized(&h.frame(&state).requests), set, "re-advertised");
}

/// What a press on the trade window's Accept (`0`) or Decline (`1`) sends for `trade`.
fn press_trade_button(trade: TradeView, button: f32) -> Vec<UiRequest> {
    let mut state = in_world();
    state.trade = Some(trade);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.frame(&state);
    let ((_, height), (x, y)) = WindowId::Trade.geometry();
    let foot = y + height - 10.0 - 34.0;
    h.move_to(x + 12.0 + button * 118.0 + 55.0, foot + 14.0);
    h.frame(&state);
    h.press();
    let mut asked = h.frame(&state).requests;
    h.release();
    asked.extend(h.frame(&state).requests);
    asked
}

/// The trade buttons follow the offer as the trade window shows it: a change since the accept
/// puts both lights out and Accept back, and two empty lists leave nothing to accept.
#[test]
fn the_trade_buttons_follow_the_offer_shown_rather_than_the_last_accept_message() {
    use pyreal_client_contract::view::TradeRow;
    let offer = TradeView {
        open: true,
        partner_rows: vec![TradeRow {
            item: SWORD,
            ..TradeRow::default()
        }],
        accepted: true,
        partner_accepted: true,
        acceptance_darkened: true,
        ..TradeView::default()
    };
    let accept = |r: &UiRequest| matches!(r, UiRequest::TradeAccept { .. });
    let changed = press_trade_button(offer.clone(), 0.0);
    assert!(
        changed.iter().any(accept),
        "Accept is back after a change: {changed:?}"
    );
    let declined = press_trade_button(offer, 1.0);
    assert!(
        !declined.contains(&UiRequest::TradeDecline),
        "nothing stands accepted: {declined:?}"
    );
    let empty = press_trade_button(
        TradeView {
            open: true,
            ..TradeView::default()
        },
        0.0,
    );
    assert!(!empty.iter().any(accept), "two empty lists: {empty:?}");
}
