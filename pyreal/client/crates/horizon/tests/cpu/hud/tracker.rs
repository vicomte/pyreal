//! The quest tracker: pins from the Journal show on the HUD with their time left, a right click
//! unpins, a pinned timer that comes ready says so, the rent owed is told at login, and the pins
//! and their timers last across a restart.

use pyreal_client_contract::actions::pyreal::HIDE_INTERFACE;
use pyreal_client_contract::journal::{HeardTimer, JournalPage, JournalView};
use pyreal_client_contract::view::{ContractEntry, HouseDataView};
use pyreal_horizon::draw::Rect;
use pyreal_horizon::options::HorizonOptions;
use pyreal_horizon::ui::game::GameState;
use pyreal_horizon::ui::kit::BODY_TOP;
use pyreal_horizon::ui::panels::tracker::Pin;
use pyreal_horizon::ui::panels::WindowId;

use crate::Harness;

/// The wall clock in the world, Unix seconds.
const WALL: u64 = 1_800_000_000;

/// A fresh settings folder of its own.
fn settings_folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pyreal-horizon-tracker-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn world() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        address: "127.0.0.1:9000".into(),
        clock: 100.0,
        wall: WALL,
        journal: JournalView {
            loaded: true,
            ..JournalView::default()
        },
        ..GameState::default()
    }
}

fn stipend() -> ContractEntry {
    ContractEntry {
        contract_id: 7,
        name: "Stipend".into(),
        status: "Waiting to repeat".into(),
        repeat_remaining: Some(120.0),
        ..ContractEntry::default()
    }
}

/// The interface in the world, keeping its settings in `dir`.
fn in_folder(dir: &std::path::Path, state: &GameState) -> Harness {
    let mut h = Harness::new(HorizonOptions::default());
    h.ui.hud.settings_dir = Some(dir.to_owned());
    h.frame(state);
    h
}

/// The Journal window's body under its title, at a layout scale of one.
fn journal_body() -> Rect {
    let ((w, h), (x, y)) = WindowId::Journal.geometry();
    Rect::new(x + 12.0, y + BODY_TOP, w - 24.0, h - BODY_TOP - 10.0)
}

fn click(h: &mut Harness, state: &GameState, at: (f32, f32)) {
    h.move_to(at.0, at.1);
    h.press();
    h.frame(state);
    h.release();
    h.frame(state);
}

fn right_click(h: &mut Harness, state: &GameState, at: (f32, f32)) {
    h.move_to(at.0, at.1);
    h.input.down[1] = true;
    h.input.pressed[1] = true;
    h.frame(state);
    h.input.down[1] = false;
    h.input.released[1] = true;
    h.frame(state);
}

fn tracker_outline(h: &Harness) -> Option<Rect> {
    h.ui.hud
        .layout
        .outlines
        .iter()
        .find(|(n, _)| n == "tracker")
        .map(|(_, r)| *r)
}

/// Pinned with the Journal's Pin button, a contract shows on the HUD with its time left; a right
/// click on its row unpins it, and the empty tracker draws nothing.
#[test]
fn a_contract_pinned_in_the_journal_shows_on_the_hud_and_a_right_click_unpins_it() {
    let dir = settings_folder("pin");
    let mut state = world();
    state.contracts = vec![stipend()];
    let mut h = in_folder(&dir, &state);
    assert_eq!(tracker_outline(&h), None, "nothing pinned, nothing drawn");
    h.ui.windows.open(WindowId::Journal, 0.0);
    h.frame(&state);
    // The contract's row, then Pin beside Abandon.
    let b = journal_body();
    click(&mut h, &state, (b.x + 40.0, b.y + 34.0 + 20.0));
    click(&mut h, &state, (b.right() - 176.0, b.bottom() - 16.0));
    assert!(h.ui.windows.tracker.is_pinned(&Pin::Contract(7)));
    h.ui.windows.close(WindowId::Journal);
    h.frame(&state);
    assert!(tracker_outline(&h).is_some(), "the panel is drawn");
    let rows = h.ui.windows.tracker.rows(&state);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Stipend");
    assert_eq!(rows[0].right, "2m");
    assert!(rows[0].running);
    let (row, pin) = h.ui.windows.tracker.drawn()[0].clone();
    assert_eq!(pin, Some(Pin::Contract(7)));
    right_click(&mut h, &state, (row.x + row.w / 2.0, row.y + row.h / 2.0));
    assert!(!h.ui.windows.tracker.is_pinned(&Pin::Contract(7)));
    h.frame(&state);
    assert_eq!(
        tracker_outline(&h),
        None,
        "empty again, it is out of the way"
    );
}

/// A pinned timer that comes ready gives one notice; one already ready when pinned gives none.
#[test]
fn a_pinned_timer_that_comes_ready_says_so_once() {
    let dir = settings_folder("ready");
    let mut state = world();
    state.journal.heard = vec![HeardTimer {
        title: "Quest from Ayn Tayan".into(),
        ready_at: state.clock + 60.0,
        seconds: 60,
    }];
    state.contracts = vec![ContractEntry {
        repeat_remaining: Some(0.0),
        ..stipend()
    }];
    let mut h = in_folder(&dir, &state);
    h.ui.windows
        .tracker
        .pin(Pin::Heard("Quest from Ayn Tayan".into()), &state);
    h.ui.windows.tracker.pin(Pin::Contract(7), &state);
    h.frame(&state);
    assert!(h.ui.windows.tracker.notices().is_empty());
    state.clock += 61.0;
    state.wall += 61;
    h.frame(&state);
    let notices: Vec<&str> =
        h.ui.windows
            .tracker
            .notices()
            .iter()
            .map(|(t, _)| t.as_str())
            .collect();
    assert_eq!(notices, ["Quest from Ayn Tayan is ready."]);
    let rows = h.ui.windows.tracker.rows(&state);
    assert!(
        rows.iter().all(|r| r.ready && r.right == "Ready"),
        "{rows:?}"
    );
    for _ in 0..3 {
        h.frame(&state);
    }
    assert_eq!(h.ui.windows.tracker.notices().len(), 1, "said once");
}

/// The pins are kept per character: after a restart a page's timer, whose stamp the notebook
/// reads back on the earlier session's clock, and a timer heard in chat count on from the wall
/// clock, and one that came ready while away is told at login.
#[test]
fn pins_and_their_timers_last_across_a_restart() {
    let dir = settings_folder("restart");
    let page = JournalPage {
        title: "Drudge camp".into(),
        page_number: 1,
        made: 1_700_000_000,
        timer_running: true,
        timer_stamp: 100.0 + 3_600.0,
        ..JournalPage::default()
    };
    let mut state = world();
    state.journal.pages = vec![page.clone()];
    state.journal.heard = vec![HeardTimer {
        title: "Quest from Ayn Tayan".into(),
        ready_at: 100.0 + 300.0,
        seconds: 300,
    }];
    let mut h = in_folder(&dir, &state);
    h.ui.windows
        .tracker
        .pin(Pin::page(&page).expect("a named page"), &state);
    h.ui.windows
        .tracker
        .pin(Pin::Heard("Quest from Ayn Tayan".into()), &state);
    h.frame(&state);
    // Another character's pins are its own.
    let mut other = state.clone();
    other.name = "Other".into();
    let mut o = in_folder(&dir, &other);
    o.frame(&other);
    assert!(o.ui.windows.tracker.rows(&other).is_empty());
    // A restart ten minutes later: the session's clock starts again, the heard timer is gone.
    let mut later = world();
    later.clock = 5.0;
    later.wall = WALL + 600;
    later.journal.pages = vec![page];
    let mut h = in_folder(&dir, &later);
    h.frame(&later);
    let rows = h.ui.windows.tracker.rows(&later);
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Drudge camp", "Quest from Ayn Tayan"]);
    assert_eq!(rows[0].right, "50m", "an hour less the ten minutes away");
    assert!(rows[1].ready, "five minutes ran out while away");
    let notices: Vec<&str> =
        h.ui.windows
            .tracker
            .notices()
            .iter()
            .map(|(t, _)| t.as_str())
            .collect();
    assert_eq!(notices, ["Quest from Ayn Tayan is ready."]);
}

/// At login the rent owed is told and listed, once.
#[test]
fn the_rent_owed_is_told_at_login() {
    let dir = settings_folder("rent");
    let mut state = world();
    let mut h = in_folder(&dir, &state);
    state.world_services.house = Some(HouseDataView {
        rent_owed: true,
        maintenance_next_due: i64::try_from(WALL).unwrap() + 3 * 86_400 + 60,
        ..HouseDataView::default()
    });
    h.frame(&state);
    let notices: Vec<String> =
        h.ui.windows
            .tracker
            .notices()
            .iter()
            .map(|(t, _)| t.clone())
            .collect();
    assert_eq!(notices, ["Your house's rent is due in 3 days."]);
    let rows = h.ui.windows.tracker.rows(&state);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "House rent");
    assert!(tracker_outline(&h).is_some());
    // Paid, the row goes and no second notice comes.
    state.world_services.house.as_mut().unwrap().rent_owed = false;
    for _ in 0..(7 * 60) {
        h.frame(&state);
    }
    assert!(h.ui.windows.tracker.notices().is_empty());
    assert_eq!(tracker_outline(&h), None);
}

/// Hide Interface hides the tracker with the rest of the HUD.
#[test]
fn the_tracker_hides_with_the_interface() {
    let dir = settings_folder("hide");
    let mut state = world();
    state.contracts = vec![stipend()];
    let mut h = in_folder(&dir, &state);
    h.ui.windows.tracker.pin(Pin::Contract(7), &state);
    h.frame(&state);
    let panel = tracker_outline(&h).expect("the panel");
    h.input.actions.push(HIDE_INTERFACE.0);
    h.frame(&state);
    for _ in 0..(5 * 60) {
        h.frame(&state);
    }
    assert!(h.ui.hidden.is_some());
    let inside = h
        .list
        .quads
        .iter()
        .filter(|q| panel.contains(q.dst.x + 0.5, q.dst.y + 0.5))
        .count();
    assert_eq!(inside, 0, "nothing of the tracker drawn");
}
