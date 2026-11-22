//! The Journal's notebook is the game's shared notebook: showing it tells the game, which loads
//! it, and its buttons and fields change it through the game.

use pyreal_client_contract::journal::{JournalAction, JournalPage, JournalView};
use pyreal_client_contract::UiRequest;
use pyreal_horizon::draw::Rect;
use pyreal_horizon::ui::game::GameState;
use pyreal_horizon::ui::kit::BODY_TOP;
use pyreal_horizon::ui::panels::WindowId;

use crate::Harness;

fn with_notebook() -> GameState {
    let page = JournalPage {
        label: "Camp".into(),
        title: "Drudges".into(),
        page_number: 1,
        ..JournalPage::default()
    };
    GameState {
        in_world: true,
        name: "Tester".into(),
        journal: JournalView {
            pages: vec![page.clone()],
            draft: page,
            current_page: 1,
            loaded: true,
            ..JournalView::default()
        },
        ..GameState::default()
    }
}

/// The Journal window's body under its tabs, at a layout scale of one.
fn body() -> Rect {
    let ((w, h), (x, y)) = WindowId::Journal.geometry();
    Rect::new(x + 12.0, y + BODY_TOP, w - 24.0, h - BODY_TOP - 10.0)
}

fn click(h: &mut Harness, state: &GameState, at: (f32, f32)) -> Vec<UiRequest> {
    h.move_to(at.0, at.1);
    h.press();
    let mut out = h.frame(state).requests;
    h.release();
    out.extend(h.frame(state).requests);
    out
}

/// The Notebook tab, the second of the three with the contracts, at the tabs' least width with
/// no fonts.
fn notebook_tab(h: &mut Harness, state: &GameState) -> Vec<UiRequest> {
    let b = body();
    click(h, state, (b.x + 105.0, b.y + 13.0))
}

#[test]
fn the_notebook_tab_tells_the_game_it_is_shown_and_closing_the_window_that_it_is_not() {
    let state = with_notebook();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.ui.windows.open(WindowId::Journal, 0.0);
    h.frame(&state);
    let shown = notebook_tab(&mut h, &state);
    assert!(shown.contains(&UiRequest::Journal(JournalAction::Visibility(true))));
    // New Page, beside the search at the top of the page list.
    let b = body();
    let new = click(&mut h, &state, (b.x + b.w * 0.4 - 46.0, b.y + 34.0 + 13.0));
    assert!(
        new.contains(&UiRequest::Journal(JournalAction::NewPage)),
        "{new:?}"
    );
    h.ui.windows.close(WindowId::Journal);
    let closed = h.frame(&state).requests;
    assert!(closed.contains(&UiRequest::Journal(JournalAction::Visibility(false))));
}

/// Delete Page asks first, and only Delete deletes.
#[test]
fn deleting_a_page_asks_first() {
    use pyreal_horizon::ui::input::vk;
    let state = with_notebook();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.ui.windows.open(WindowId::Journal, 0.0);
    h.frame(&state);
    notebook_tab(&mut h, &state);
    let b = body();
    let asked = click(&mut h, &state, (b.right() - 56.0, b.bottom() - 14.0));
    assert!(
        !asked
            .iter()
            .any(|r| matches!(r, UiRequest::Journal(JournalAction::Delete(_)))),
        "the button alone deletes nothing: {asked:?}"
    );
    h.input.keys.push(vk::ENTER);
    let answered = h.frame(&state).requests;
    assert!(
        answered.contains(&UiRequest::Journal(JournalAction::Delete(1))),
        "{answered:?}"
    );
}

/// The Timers tab lists each running timer once, soonest first, and those ready since the player
/// arrived; a quest already followed on a page is not listed twice.
#[test]
fn the_timers_tab_lists_every_timer_soonest_first() {
    use pyreal_client_contract::journal::{HeardTimer, QuestTimerView};
    use pyreal_client_contract::view::ContractEntry;
    use pyreal_horizon::ui::panels::{timer_rows, TimerAction};
    let mut state = with_notebook();
    state.clock = 1_000.0;
    state.journal.since = 500.0;
    state.journal.pages[0].timer_running = true;
    state.journal.pages[0].timer_stamp = 1_000.0 + 3_600.0;
    state.contracts = vec![ContractEntry {
        name: "Stipend".into(),
        repeat_remaining: Some(60.0),
        ..ContractEntry::default()
    }];
    let quest = |title: &str, min_delta, ready_at| QuestTimerView {
        name: title.to_lowercase(),
        title: title.into(),
        min_delta,
        ready_at,
        ..QuestTimerView::default()
    };
    state.journal.quests = vec![
        quest("Colosseum", 72_000, Some(900.0)),
        quest("Drudges", 72_000, Some(f64::NEG_INFINITY)),
        quest("Attuned", 0, Some(1_200.0)),
        quest("Done for good", 72_000, None),
        quest("Drudges", 72_000, Some(5_000.0)),
    ];
    state.journal.heard = vec![HeardTimer {
        title: "Quest from Ayn Tayan".into(),
        ready_at: 1_000.0 + 7_200.0,
        seconds: 7_200,
    }];
    let rows = timer_rows(&state);
    let titles: Vec<&str> = rows.iter().map(|r| r.title.as_str()).collect();
    assert_eq!(
        titles,
        ["Stipend", "Drudges", "Quest from Ayn Tayan", "Colosseum"],
        "the quest Drudges is followed on its page, so only the page is listed"
    );
    assert_eq!(rows[1].action, TimerAction::Open(1), "the page's own timer");
    assert_eq!(
        rows[3].action,
        TimerAction::Track,
        "ready since the player came"
    );
}

/// A place written in a page's notes is a link: it opens the map centred on an arrow there with
/// Create Waypoint beside it at once, kept there through a pan, and choosing it sets the waypoint
/// there under the page's title.
#[test]
fn a_place_written_in_the_notes_opens_the_map_on_an_arrow_with_create_waypoint_beside_it() {
    let mut state = with_notebook();
    for page in [&mut state.journal.draft, &mut state.journal.pages[0]] {
        page.notes = "Meet at 42.3N, 33.3E by the well.".into();
    }
    let mut h = Harness::with_built_in_art();
    h.frame(&state);
    h.ui.windows.open(WindowId::Journal, 0.0);
    h.ui.windows
        .show_journal_tab(pyreal_horizon::ui::panels::JournalTab::Notebook);
    h.frame(&state);
    h.frame(&state);
    let links = h.ui.windows.notebook_links.clone();
    let [(link, at)] = links.as_slice() else {
        panic!("one link: {links:?}");
    };
    assert_eq!(*at, (42.3, 33.3));
    assert!(link.w > 0.0, "{link:?}");
    click(
        &mut h,
        &state,
        (link.x + link.w / 2.0, link.y + link.h / 2.0),
    );
    assert!(h.ui.windows.is_open(WindowId::Map));
    let view = &h.ui.windows.map_view;
    assert_eq!(view.pin, Some(((42.3, 33.3), Some("Drudges".into()))));
    let centre = pyreal_horizon::ui::panels::map::map_pixel(42.3, 33.3);
    assert_eq!(view.centre, centre, "centred on the place");
    h.frame(&state);
    let pin = h.ui.windows.map_view.pin_mark.expect("the arrow");
    let offer = h.ui.windows.map_view.offer.clone();
    assert_eq!(offer, Some(((42.3, 33.3), Some("Drudges".into()))));
    let button = h.ui.windows.map_view.offer_button.expect("Create Waypoint");
    assert!(
        (button.x - pin.right()).abs() < 20.0 || (pin.x - button.right()).abs() < 20.0,
        "beside the arrow: {button:?} {pin:?}"
    );
    // A pan keeps it beside the arrow.
    let from = (pin.x - 60.0, pin.y + 40.0);
    h.move_to(from.0, from.1);
    h.press();
    h.frame(&state);
    h.move_to(from.0 + 30.0, from.1 + 10.0);
    h.frame(&state);
    h.release();
    h.frame(&state);
    let offer = h.ui.windows.map_view.offer.clone();
    assert_eq!(offer, Some(((42.3, 33.3), Some("Drudges".into()))));
    let button = h.ui.windows.map_view.offer_button.expect("Create Waypoint");
    click(
        &mut h,
        &state,
        (button.x + button.w / 2.0, button.y + button.h / 2.0),
    );
    let w = h.ui.windows.waypoints.active.clone().expect("a waypoint");
    assert_eq!(
        (w.ns, w.ew, w.name.as_deref()),
        (42.3, 33.3, Some("Drudges"))
    );
    assert!(
        h.ui.windows.map_view.pin.is_none(),
        "the arrow gives way to the waypoint"
    );
}
