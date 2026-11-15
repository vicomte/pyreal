//! Long lists scroll inside their windows: the last row is reached by scrolling, and a press on
//! what stands round a list (a button, a box, a field) lands there, never on a row hidden past
//! the list's edge.

use pyreal_client_contract::view::{
    AllegianceEntry, AllegianceRoster, ContractEntry, FriendEntry, SquelchEntry,
};
use pyreal_client_contract::UiRequest;
use pyreal_horizon::draw::Rect;
use pyreal_horizon::ui::game::{GameState, Item};
use pyreal_horizon::ui::kit::BODY_TOP;
use pyreal_horizon::ui::panels::WindowId;
use pyreal_primitives::ObjectId;

use crate::Harness;

fn in_world() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        player_id: Some(ObjectId(0x5000_0001)),
        ..GameState::default()
    }
}

fn opened(state: &GameState, id: WindowId) -> Harness {
    let mut h = Harness::new(Default::default());
    h.frame(state);
    h.ui.windows.open(id, 0.0);
    for _ in 0..12 {
        h.frame(state);
    }
    h
}

fn window(h: &Harness, id: WindowId) -> Rect {
    h.ui.windows
        .rects(1.0)
        .into_iter()
        .find(|(w, _)| *w == id)
        .map(|(_, r)| r)
        .expect("the window is open")
}

fn body(h: &Harness, id: WindowId) -> Rect {
    let w = window(h, id);
    Rect::new(
        w.x + 12.0,
        w.y + BODY_TOP,
        w.w - 24.0,
        w.h - BODY_TOP - 10.0,
    )
}

fn press(h: &mut Harness, state: &GameState, (x, y): (f32, f32)) -> Vec<UiRequest> {
    h.move_to(x, y);
    h.frame(state);
    h.press();
    let mut asked = h.frame(state).requests;
    h.release();
    asked.extend(h.frame(state).requests);
    asked
}

/// Step the open window's tabs `n` along, as the pad's shoulders do.
fn tab(h: &mut Harness, state: &GameState, n: i32) {
    let snap = h.input.nav.last.clone();
    let bar = snap.tabs_in(snap.layers.len()).expect("the window's tabs");
    h.input.pad.tab_step = Some((bar, n));
    h.frame(state);
    h.frame(state);
}

fn centre(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// Wheel the list under `at` to its foot.
fn scroll_to_foot(h: &mut Harness, state: &GameState, at: (f32, f32)) {
    h.move_to(at.0, at.1);
    for _ in 0..80 {
        h.input.wheel = -1.0;
        h.frame(state);
    }
    h.frame(state);
}

/// The lowest of the targets `h` tall in `inside` (a row, after scrolling to the foot: the last).
fn lowest(h: &Harness, inside: Rect, height: f32) -> Rect {
    h.input
        .nav
        .last
        .targets
        .iter()
        .map(|t| t.rect)
        .filter(|r| (r.h - height).abs() < 0.5 && inside.contains(r.x + 1.0, r.y + 1.0))
        .max_by(|a, b| (a.y, a.x).partial_cmp(&(b.y, b.x)).unwrap())
        .expect("a row")
}

#[test]
fn the_friends_list_scrolls_to_its_last_friend_and_leaves_its_checkbox_its_press() {
    let mut state = in_world();
    state.friend_list = (0..15u32)
        .map(|i| FriendEntry {
            id: ObjectId(0x5000_0100 + i),
            name: format!("Friend {i:02}"),
            online: false,
        })
        .collect();
    let mut h = opened(&state, WindowId::Social);
    let b = body(&h, WindowId::Social);
    tab(&mut h, &state, 1);
    let foot = b.bottom() - 30.0;
    let checkbox = press(&mut h, &state, (b.x + 10.0, foot + 15.0));
    assert!(
        checkbox
            .iter()
            .any(|r| matches!(r, UiRequest::SetPlayerOption(..))),
        "the Appear offline box took the press: {checkbox:?}"
    );
    scroll_to_foot(&mut h, &state, (b.x + 100.0, b.y + 80.0));
    // Above the text field at the foot.
    let above = Rect::new(b.x, b.y, b.w, b.bottom() - 30.0 - 44.0 - b.y);
    let last = lowest(&h, above, 30.0);
    press(&mut h, &state, centre(last));
    let bw = 96.0;
    let removed = press(&mut h, &state, (b.right() - bw / 2.0, foot + 15.0));
    assert!(
        removed.contains(&UiRequest::RemoveFriend {
            target: ObjectId(0x5000_0100 + 14)
        }),
        "{removed:?}"
    );
}

#[test]
fn every_squelch_is_shown_and_the_last_reached_by_scrolling() {
    let mut state = in_world();
    state.squelches = (0..20u32)
        .map(|i| SquelchEntry {
            name: format!("Pest {i:02}"),
            account: false,
        })
        .collect();
    let mut h = opened(&state, WindowId::Social);
    let b = body(&h, WindowId::Social);
    tab(&mut h, &state, 2);
    scroll_to_foot(&mut h, &state, (b.x + 100.0, b.y + 80.0));
    // Above the text field at the foot.
    let above = Rect::new(b.x, b.y, b.w, b.bottom() - 30.0 - 44.0 - b.y);
    let last = lowest(&h, above, 30.0);
    press(&mut h, &state, centre(last));
    // Remove stands at the foot's right.
    let foot = b.bottom() - 30.0;
    let removed = press(&mut h, &state, (b.right() - 40.0, foot + 15.0));
    assert!(
        removed
            .iter()
            .any(|r| matches!(r, UiRequest::ModifyCharacterSquelch { add: false, account, .. } if account == "Pest 19")),
        "{removed:?}"
    );
}

#[test]
fn the_vassals_scroll_and_the_buttons_under_them_keep_their_presses() {
    let mut state = in_world();
    let me = ObjectId(0x5000_0001);
    let entry = |id: u32, name: &str| AllegianceEntry {
        id: ObjectId(id),
        full_name: name.into(),
        logged_in: true,
        rank: 1,
        cp_cached: 0,
    };
    state.roster = AllegianceRoster {
        total_members: 42,
        subject: Some(entry(me.0, "Tester")),
        monarch: Some(entry(0x5000_0002, "Monarch")),
        patron: Some(entry(0x5000_0002, "Monarch")),
        vassals: (0..40u32)
            .map(|i| entry(0x5000_0200 + i, &format!("Vassal {i:02}")))
            .collect(),
        ..AllegianceRoster::default()
    };
    let mut h = opened(&state, WindowId::Allegiance);
    let b = body(&h, WindowId::Allegiance);
    let foot = b.bottom() - 30.0;
    let bw = (b.w - 12.0) / 3.0;
    let broke = press(&mut h, &state, (b.x + bw + 6.0 + bw / 2.0, foot + 14.0));
    assert!(
        broke.is_empty() || !broke.iter().any(|r| matches!(r, UiRequest::Select(_))),
        "{broke:?}"
    );
    let asked = h.frame(&state).allegiance_question;
    let _ = asked;
    scroll_to_foot(&mut h, &state, (b.x + 100.0, foot - 40.0));
    let last = lowest(&h, Rect::new(b.x, b.y, b.w, foot - b.y), 26.0);
    press(&mut h, &state, centre(last));
    h.move_to(b.x + 2.0 * (bw + 6.0) + bw / 2.0, foot + 14.0);
    h.frame(&state);
    h.press();
    let out = h.frame(&state);
    h.release();
    let (_, target, _, _) = out.allegiance_question.expect("Release asks");
    assert_eq!(target, ObjectId(0x5000_0200 + 39));
}

#[test]
fn the_contracts_scroll_to_the_last_one() {
    let mut state = in_world();
    state.contracts = (0..30u32)
        .map(|i| ContractEntry {
            contract_id: 100 + i,
            name: format!("Contract {i:02}"),
            ..ContractEntry::default()
        })
        .collect();
    let mut h = opened(&state, WindowId::Journal);
    let b = body(&h, WindowId::Journal);
    let list = Rect::new(b.x, b.y + 34.0, b.w * 0.42, b.h - 34.0 - 36.0);
    scroll_to_foot(&mut h, &state, centre(list));
    let last = lowest(&h, list, 40.0);
    assert!(
        last.bottom() <= list.bottom() + 0.5,
        "inside the list: {last:?}"
    );
    press(&mut h, &state, centre(last));
    press(&mut h, &state, (b.right() - 60.0, b.bottom() - 16.0));
    h.input.keys = vec![pyreal_horizon::ui::input::vk::ENTER];
    let answered = h.frame(&state).requests;
    assert!(
        answered.contains(&UiRequest::AbandonContract { contract_id: 129 }),
        "{answered:?}"
    );
}

#[test]
#[allow(clippy::cast_possible_truncation)] // tile columns, whole pixels
fn a_chest_of_sixty_scrolls_to_its_last_item() {
    let mut state = in_world();
    let items: Vec<Item> = (0..60u32)
        .map(|i| Item {
            id: ObjectId(0x8000_0300 + i),
            name: format!("Thing {i}"),
            stack: Some(1),
            ..Item::default()
        })
        .collect();
    state.loot = Some((ObjectId(0x7000_0001), "Chest".into(), items));
    let mut h = opened(&state, WindowId::Loot);
    let b = body(&h, WindowId::Loot);
    scroll_to_foot(&mut h, &state, (b.x + 100.0, b.y + 100.0));
    let tiles: Vec<Rect> =
        h.ui.drops
            .iter()
            .filter(|(r, d)| {
                d.is_some() && (r.w - 44.0).abs() < 0.5 && b.contains(r.x + 1.0, r.y + 1.0)
            })
            .map(|(r, _)| *r)
            .collect();
    let mut xs: Vec<i32> = tiles.iter().map(|r| r.x.round() as i32).collect();
    xs.sort_unstable();
    xs.dedup();
    let cols = xs.len();
    let bottom = tiles.iter().map(|r| r.y).fold(f32::MIN, f32::max);
    let last = tiles
        .iter()
        .find(|r| (r.y - bottom).abs() < 0.5 && r.x.round() as i32 == xs[59 % cols])
        .copied()
        .expect("the last row's tile");
    let picked = press(&mut h, &state, centre(last));
    assert!(
        picked.contains(&UiRequest::Select(ObjectId(0x8000_0300 + 59))),
        "{picked:?}"
    );
}
