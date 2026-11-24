//! Hide Interface: its key hides the HUD and every window and brings them back as they were, the
//! game's boxes still show and answer, and a notice says how to show the interface again.

use pyreal_client_contract::actions::pyreal::HIDE_INTERFACE;
use pyreal_horizon::ui::game::{GameState, KeyBindingsView, KeyRow, Prompt, Vital};
use pyreal_horizon::ui::input::vk;
use pyreal_horizon::ui::panels::WindowId;

use crate::Harness;

/// The inventory's key.
const INVENTORY_KEY: u32 = 0x1000_0019;

fn world() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        health: Vital {
            current: 80,
            max: 100,
        },
        key_bindings: Some(KeyBindingsView {
            rows: vec![KeyRow {
                map: 0,
                action: HIDE_INTERFACE.0,
                group: 0,
                caption: "Hide Interface".into(),
                keys: vec!["Alt+Z".into()],
                changed: false,
            }],
            ..KeyBindingsView::default()
        }),
        ..GameState::default()
    }
}

/// The interface over the world with the Character and Inventory windows open, drawn once.
fn shown() -> (Harness, GameState) {
    let mut h = Harness::with_built_in_art();
    h.ui.windows.open(WindowId::Character, -1.0);
    h.ui.windows.open(WindowId::Inventory, -1.0);
    let state = world();
    h.frame(&state);
    h.frame(&state);
    (h, state)
}

fn press_hide(h: &mut Harness, state: &GameState) {
    h.input.actions.push(HIDE_INTERFACE.0);
    h.frame(state);
}

/// Frames until the notice has faded.
fn after_the_notice(h: &mut Harness, state: &GameState) {
    for _ in 0..(5 * 60) {
        h.frame(state);
    }
}

#[test]
fn the_key_hides_everything_and_brings_it_back_as_it_was() {
    let (mut h, state) = shown();
    let quads = h.list.quads.len();
    assert!(quads > 0);
    press_hide(&mut h, &state);
    assert!(h.ui.hidden.is_some());
    after_the_notice(&mut h, &state);
    assert!(
        h.list.quads.is_empty(),
        "{} quads drawn",
        h.list.quads.len()
    );
    assert!(h.list.backdrops.is_empty());
    assert!(h.list.anchored.is_empty());
    // Hidden, the windows stay open.
    assert!(h.ui.windows.is_open(WindowId::Character));
    assert!(h.ui.windows.is_open(WindowId::Inventory));
    press_hide(&mut h, &state);
    h.frame(&state);
    assert!(h.ui.hidden.is_none());
    assert_eq!(h.list.quads.len(), quads, "drawn as before");
    assert!(h.ui.windows.is_open(WindowId::Character));
    assert!(h.ui.windows.is_open(WindowId::Inventory));
}

#[test]
fn the_notice_names_the_key_and_fades() {
    let (mut h, state) = shown();
    assert_eq!(
        pyreal_horizon::ui::hide::notice(state.key_bindings.as_ref()),
        "The interface is hidden. Press Alt+Z or Escape to show it."
    );
    assert_eq!(
        pyreal_horizon::ui::hide::notice(None),
        "The interface is hidden. Press Escape to show it."
    );
    press_hide(&mut h, &state);
    assert!(!h.list.quads.is_empty(), "the notice is drawn");
    after_the_notice(&mut h, &state);
    assert!(h.list.quads.is_empty());
}

#[test]
fn escape_shows_it_again_and_closes_nothing() {
    let (mut h, state) = shown();
    press_hide(&mut h, &state);
    assert!(h.ui.wants_escape, "Escape is the interface's while hidden");
    h.input.keys.push(vk::ESCAPE);
    let out = h.frame(&state);
    assert!(h.ui.hidden.is_none());
    assert!(h.ui.windows.is_open(WindowId::Character));
    assert!(h.ui.windows.is_open(WindowId::Inventory));
    assert!(out.requests.is_empty(), "{:?}", out.requests);
}

#[test]
fn a_window_key_shows_it_again_and_does_its_work() {
    let (mut h, state) = shown();
    press_hide(&mut h, &state);
    h.input.actions.push(INVENTORY_KEY);
    h.frame(&state);
    assert!(h.ui.hidden.is_none());
    assert!(
        !h.ui.windows.is_open(WindowId::Inventory),
        "the key closed it"
    );
    assert!(h.ui.windows.is_open(WindowId::Character));
}

/// Whether the pointer is over the interface anywhere on a grid across the screen, and every
/// press on it, after the presses.
fn press_all_over(h: &mut Harness, state: &GameState) -> (bool, Vec<String>) {
    let mut over = false;
    let mut asked = Vec::new();
    for y in (20..1080).step_by(60) {
        for x in (20..1920).step_by(60) {
            #[allow(clippy::cast_precision_loss)]
            h.move_to(x as f32, y as f32);
            h.frame(state);
            over |= h.ui.pointer_over_ui;
            h.press();
            let out = h.frame(state);
            over |= h.ui.pointer_over_ui;
            h.release();
            h.frame(state);
            asked.extend(out.requests.iter().map(|r| format!("{x},{y}: {r:?}")));
        }
    }
    (over, asked)
}

#[test]
fn the_hidden_interface_takes_no_press() {
    // Shown, the same presses meet the interface.
    let (mut h, state) = shown();
    assert!(
        press_all_over(&mut h, &state).0,
        "the grid meets the interface"
    );
    let (mut h, state) = shown();
    press_hide(&mut h, &state);
    after_the_notice(&mut h, &state);
    // Hidden, they land on the world, not the invisible windows and HUD.
    let (over, asked) = press_all_over(&mut h, &state);
    assert!(!over, "the pointer met the hidden interface");
    assert!(asked.is_empty(), "{asked:?}");
    assert!(h.ui.hidden.is_some());
    assert!(h.ui.windows.is_open(WindowId::Character));
    assert!(h.ui.windows.is_open(WindowId::Inventory));
}

#[test]
fn a_question_shows_and_answers_while_hidden() {
    let (mut h, mut state) = shown();
    press_hide(&mut h, &state);
    after_the_notice(&mut h, &state);
    state.prompts = vec![Prompt {
        id: 7,
        text: "Borin Stonefist wants you to join the fellowship. Will you?".into(),
        question: true,
        modal: false,
        waiting: 0,
        offer: None,
    }];
    h.frame(&state);
    assert!(!h.list.quads.is_empty(), "the box is drawn");
    // Yes, found by pressing down the screen's middle.
    let mut answered = false;
    for y in (300..800).step_by(4) {
        #[allow(clippy::cast_precision_loss)]
        h.move_to(895.0, y as f32);
        h.press();
        let out = h.frame(&state);
        h.release();
        h.frame(&state);
        if out.answers.contains(&(7, true)) {
            answered = true;
            break;
        }
    }
    assert!(answered, "Yes answers the box");
    assert!(h.ui.hidden.is_some(), "the box leaves the interface hidden");
}

#[test]
fn a_modal_box_shows_while_hidden() {
    let (mut h, mut state) = shown();
    press_hide(&mut h, &state);
    after_the_notice(&mut h, &state);
    state.prompts = vec![Prompt {
        id: 9,
        text: "You have been disconnected from the world.".into(),
        question: false,
        modal: true,
        waiting: 0,
        offer: None,
    }];
    h.frame(&state);
    assert!(!h.list.quads.is_empty(), "the box is drawn");
}

#[test]
fn death_brings_the_interface_back() {
    let (mut h, mut state) = shown();
    press_hide(&mut h, &state);
    state.health.current = 0;
    h.frame(&state);
    assert!(h.ui.hidden.is_none());
    // While dead, the key leaves it shown.
    press_hide(&mut h, &state);
    assert!(h.ui.hidden.is_none());
}

#[test]
fn the_interface_takes_the_key_from_the_game() {
    assert!(pyreal_horizon::ui::takes_action(HIDE_INTERFACE.0));
}

#[test]
fn the_system_menu_has_no_hide_interface() {
    let mut h = Harness::with_built_in_art();
    let state = world();
    let bar_y = 1080.0 - 14.0 - 24.0;
    // The System medallion, by its tooltip along the bar.
    let mut system = None;
    let mut x = 1500.0;
    while x < 1910.0 && system.is_none() {
        h.move_to(x, bar_y);
        h.frame(&state);
        h.frame(&state);
        if h.ui
            .hud
            .shown_tip()
            .is_some_and(|(t, _)| t == "System" || t.starts_with("System ("))
        {
            system = Some(x);
        }
        x += 4.0;
    }
    let x = system.expect("the System medallion");
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    // Its menu rises above it: Settings is there, and Hide Interface is not.
    let mut rows: Vec<String> = Vec::new();
    let mut y = bar_y - 20.0;
    while y > 700.0 {
        h.move_to(x - 10.0, y);
        h.frame(&state);
        if let Some((t, _)) = h.ui.hud.shown_tip() {
            if rows.last().map(String::as_str) != Some(t) {
                rows.push(t.to_string());
            }
        }
        y -= 2.0;
    }
    assert!(rows.iter().any(|t| t.starts_with("Settings")), "{rows:?}");
    assert!(!rows.iter().any(|t| t.contains("Hide")), "{rows:?}");
}

#[test]
fn explore_mode_hides_its_corner_and_windows_too() {
    let view = pyreal_client_runtime::explore::ExploreView {
        map_at: Some((42.1, 33.6)),
        ..Default::default()
    };
    let state = GameState {
        coords: view.map_at,
        explore: Some(view),
        ..GameState::default()
    };
    let mut h = Harness::with_built_in_art();
    h.ui.windows.open(WindowId::Options, -1.0);
    h.frame(&state);
    let quads = h.list.quads.len();
    assert!(quads > 0);
    press_hide(&mut h, &state);
    after_the_notice(&mut h, &state);
    assert!(
        h.list.quads.is_empty(),
        "{} quads drawn",
        h.list.quads.len()
    );
    assert!(h.ui.windows.is_open(WindowId::Options));
    press_hide(&mut h, &state);
    h.frame(&state);
    assert_eq!(h.list.quads.len(), quads);
}
