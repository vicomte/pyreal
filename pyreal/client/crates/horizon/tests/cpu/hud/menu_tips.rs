//! The main menu's tooltips name the player's key for what they open: "Journal (J)".

use pyreal_horizon::ui::game::{GameState, KeyBindingsView, KeyRow};
use pyreal_horizon::ui::hud::{window_key_action, with_shortcut};
use pyreal_horizon::ui::panels::WindowId;

use crate::Harness;

fn keys_for(window: WindowId, keys: &[&str]) -> KeyBindingsView {
    KeyBindingsView {
        rows: vec![KeyRow {
            map: 0,
            action: window_key_action(window).expect("the window has a key"),
            group: 0,
            caption: String::new(),
            keys: keys.iter().map(|k| (*k).to_owned()).collect(),
            changed: false,
        }],
        ..KeyBindingsView::default()
    }
}

/// Every tooltip title the menu bar's buttons show, found by passing the pointer along the bar.
fn bar_tips(state: &GameState) -> Vec<String> {
    let mut h = Harness::with_built_in_art();
    let mut seen = Vec::new();
    let mut x = 1500.0;
    while x < 1910.0 {
        h.move_to(x, 1080.0 - 14.0 - 24.0);
        h.frame(state);
        h.frame(state);
        if let Some((title, _)) = h.ui.hud.shown_tip() {
            if !seen.iter().any(|t| t == title) {
                seen.push(title.to_owned());
            }
        }
        x += 4.0;
    }
    seen
}

fn world(keys: KeyBindingsView) -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        key_bindings: Some(keys),
        ..GameState::default()
    }
}

#[test]
fn a_bound_item_names_its_key() {
    let keys = keys_for(WindowId::Journal, &["J"]);
    assert_eq!(
        with_shortcut(
            "Journal",
            Some(&keys),
            [window_key_action(WindowId::Journal)]
        ),
        "Journal (J)"
    );
    assert!(bar_tips(&world(keys)).contains(&"Journal (J)".to_owned()));
}

#[test]
fn a_rebound_item_names_the_new_key() {
    let keys = keys_for(WindowId::Journal, &["Shift+J"]);
    assert_eq!(
        with_shortcut(
            "Journal",
            Some(&keys),
            [window_key_action(WindowId::Journal)]
        ),
        "Journal (Shift+J)"
    );
    let tips = bar_tips(&world(keys));
    assert!(tips.contains(&"Journal (Shift+J)".to_owned()));
    assert!(!tips.contains(&"Journal (J)".to_owned()));
}

#[test]
fn an_unbound_item_shows_only_its_name() {
    // A row with no keys, a missing row and no key map at all.
    let unbound = keys_for(WindowId::Journal, &[]);
    let action = window_key_action(WindowId::Journal);
    assert_eq!(
        with_shortcut("Journal", Some(&unbound), [action]),
        "Journal"
    );
    assert_eq!(
        with_shortcut("Journal", Some(&KeyBindingsView::default()), [action]),
        "Journal"
    );
    assert_eq!(with_shortcut("Journal", None, [action]), "Journal");
    let tips = bar_tips(&world(unbound));
    assert!(tips.contains(&"Journal".to_owned()));
    assert!(!tips.iter().any(|t| t.starts_with("Journal (")));
}

#[test]
fn a_sub_menu_button_names_the_key_of_its_first_bound_entry() {
    // Travel holds Map then House; with Map unbound, House's key is the first.
    let mut keys = keys_for(WindowId::House, &["H"]);
    keys.rows.extend(keys_for(WindowId::Map, &[]).rows);
    let travel = [WindowId::Map, WindowId::House].map(window_key_action);
    assert_eq!(with_shortcut("Travel", Some(&keys), travel), "Travel (H)");
    assert!(bar_tips(&world(keys)).contains(&"Travel (H)".to_owned()));
    // With Map bound, its key comes first; with neither, just the name.
    let mut keys = keys_for(WindowId::Map, &["M"]);
    keys.rows.extend(keys_for(WindowId::House, &["H"]).rows);
    assert_eq!(with_shortcut("Travel", Some(&keys), travel), "Travel (M)");
    assert_eq!(
        with_shortcut("Travel", Some(&KeyBindingsView::default()), travel),
        "Travel"
    );
}
