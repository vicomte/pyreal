//! The first hotbar holds the shortcut bar: a click uses a shortcut, a drag moves it, and a drag
//! that lands nowhere leaves it removed.

use pyreal_client_contract::view::DropTarget;
use pyreal_client_contract::UiRequest;
use pyreal_horizon::draw::Rect;
use pyreal_horizon::ui::game::{GameState, Shortcut};
use pyreal_primitives::ObjectId;

use crate::Harness;

const SWORD: ObjectId = ObjectId(0x8000_0042);

fn player_with_a_shortcut_in_slot(slot: u32) -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        shortcuts: vec![Shortcut {
            index: slot,
            name: "Sword".into(),
            object: Some(SWORD),
            spell: None,
            ac_icon: None,
            cooldown: None,
            look: None,
        }],
        ..GameState::default()
    }
}

/// The screen rectangle of shortcut slot `slot`, as the HUD last laid it out.
fn slot_rect(h: &Harness, slot: u32) -> Rect {
    h.ui.hud
        .drop_slots
        .iter()
        .find(|(_, s)| *s == slot)
        .map(|(r, _)| *r)
        .expect("the slot is on screen")
}

fn centre(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// Whether a quad of `colour` was drawn inside `r` this frame.
fn drawn_in(h: &Harness, r: Rect, colour: u32) -> bool {
    h.list.quads.iter().any(|q| {
        q.colour == colour
            && q.dst.x >= r.x - 0.5
            && q.dst.y >= r.y - 0.5
            && q.dst.right() <= r.right() + 0.5
            && q.dst.bottom() <= r.bottom() + 0.5
    })
}

#[test]
fn a_shortcut_keeps_its_cooldown_sweep_until_the_game_ends_the_cooldown() {
    let mut state = player_with_a_shortcut_in_slot(2);
    // Its time run out by this clock, and the game still holding the cooldown.
    state.shortcuts[0].cooldown = Some((0.0, 30.0));
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.frame(&state);
    let slot = slot_rect(&h, 2);
    // With no art, the sweep is a dark wash over the slot.
    assert!(drawn_in(&h, slot, 0x9000_0000), "still cooling down");
    state.shortcuts[0].cooldown = None;
    h.frame(&state);
    assert!(!drawn_in(&h, slot, 0x9000_0000), "ended by the game");
}

#[test]
fn a_click_on_a_shortcut_selects_the_item_and_a_double_click_uses_it() {
    let state = player_with_a_shortcut_in_slot(2);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let (x, y) = centre(slot_rect(&h, 2));
    h.move_to(x, y);
    h.press();
    let down = h.frame(&state);
    assert!(
        down.requests.is_empty(),
        "nothing is asked on the press: {:?}",
        down.requests
    );
    h.release();
    let up = h.frame(&state);
    assert_eq!(up.requests, vec![UiRequest::Select(SWORD)]);
    // The second press of a double-click uses it.
    h.press();
    h.input.double = true;
    let used = h.frame(&state);
    assert_eq!(used.requests, vec![UiRequest::Use(SWORD)]);
    h.release();
    assert!(
        h.frame(&state).requests.is_empty(),
        "the release asks nothing more"
    );
}

#[test]
fn dragging_a_shortcut_to_another_slot_removes_it_then_places_it_there() {
    let state = player_with_a_shortcut_in_slot(2);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let (x, y) = centre(slot_rect(&h, 2));
    let (tx, ty) = centre(slot_rect(&h, 7));
    h.move_to(x, y);
    h.press();
    h.frame(&state);
    h.move_to(x + 20.0, y);
    let picked = h.frame(&state);
    assert_eq!(picked.requests, vec![UiRequest::RemoveShortcut(SWORD)]);
    h.move_to(tx, ty);
    h.frame(&state);
    h.release();
    let dropped = h.frame(&state);
    assert_eq!(
        dropped.requests,
        vec![UiRequest::DragDrop {
            item: SWORD,
            target: DropTarget::ShortcutAlias { slot: 7, from: 2 },
        }]
    );
}

#[test]
fn a_shortcut_dragged_off_the_bar_stays_removed() {
    let state = player_with_a_shortcut_in_slot(0);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let (x, y) = centre(slot_rect(&h, 0));
    h.move_to(x, y);
    h.press();
    h.frame(&state);
    h.move_to(900.0, 400.0);
    let picked = h.frame(&state);
    assert_eq!(picked.requests, vec![UiRequest::RemoveShortcut(SWORD)]);
    h.release();
    let dropped = h.frame(&state);
    assert!(dropped.requests.is_empty(), "{:?}", dropped.requests);
}

#[test]
fn a_shortcut_key_uses_the_item_in_its_slot_and_its_control_form_selects_it() {
    let state = player_with_a_shortcut_in_slot(2);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    // The third slot's key, then the same slot's secondary key.
    h.input.actions.push(0x1000_0044);
    let used = h.frame(&state);
    assert_eq!(used.requests, vec![UiRequest::Use(SWORD)]);
    h.input.actions.push(0x1000_0050);
    let selected = h.frame(&state);
    assert_eq!(selected.requests, vec![UiRequest::Select(SWORD)]);
    // An empty slot's key asks for nothing.
    h.input.actions.push(0x1000_0042);
    assert!(h.frame(&state).requests.is_empty());
}

#[test]
fn the_log_out_key_asks_first_and_logs_out_only_on_yes() {
    let state = player_with_a_shortcut_in_slot(0);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.input.actions.push(0x1000_0026);
    let asked = h.frame(&state);
    assert!(!asked.log_off, "the key only asks");
    // Enter answers the question's first button, Yes.
    h.input.keys.push(0x0D);
    let answered = h.frame(&state);
    assert!(answered.log_off);
}

/// The game's spell list after `requests`, applied in order to `list`.
pub fn applied(list: &[u32], requests: &[UiRequest]) -> Vec<u32> {
    let mut got = list.to_vec();
    for r in requests {
        match r {
            UiRequest::RemoveSpellFavorite { spell_id, .. } => got.retain(|s| s != spell_id),
            UiRequest::AddSpellFavorite {
                spell_id, index, ..
            } => {
                let at = usize::try_from(*index).unwrap_or(0).min(got.len());
                got.insert(at, *spell_id);
            }
            _ => {}
        }
    }
    got
}

/// The screen rectangle of spell bar slot `i` of the first tab, as the HUD last laid it out.
fn spell_slot(h: &Harness, i: usize) -> Rect {
    use pyreal_horizon::ui::kit::Drop;
    h.ui.drops
        .iter()
        .find(|(_, d)| *d == Some(Drop::SpellSlot { tab: 0, index: i }))
        .map(|(r, _)| *r)
        .expect("the spell slot is on screen")
}

/// A drag of the spell in slot `from` to slot `to`, and what the drop asked for.
fn drag_spell(h: &mut Harness, state: &GameState, from: usize, to: usize) -> Vec<UiRequest> {
    let (fx, fy) = centre(spell_slot(h, from));
    h.move_to(fx, fy);
    h.press();
    h.frame(state);
    let (tx, ty) = centre(spell_slot(h, to));
    h.move_to(tx, ty);
    let moved = h.frame(state);
    assert!(
        moved.requests.is_empty(),
        "nothing leaves the tab while it is carried"
    );
    h.release();
    h.frame(state).requests
}

#[test]
fn a_spell_dragged_along_the_spell_bar_lands_in_the_slot_it_is_dropped_on_and_leaves_a_gap() {
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        spell_tabs: vec![vec![7, 8, 9]],
        combat_mode: pyreal_client_contract::combat_mode::MAGIC,
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let dropped = drag_spell(&mut h, &state, 0, 5);
    let slots = &h.ui.hud.spell_slots;
    assert_eq!((slots.get(0, 0), slots.get(0, 5)), (None, Some(7)));
    assert_eq!(
        dropped,
        vec![
            UiRequest::RemoveSpellFavorite {
                spell_id: 7,
                tab: 0
            },
            UiRequest::AddSpellFavorite {
                spell_id: 7,
                index: 2,
                tab: 0
            },
        ]
    );
}

#[test]
fn a_spell_dropped_on_another_changes_places_with_it() {
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        spell_tabs: vec![vec![7, 8, 9]],
        combat_mode: pyreal_client_contract::combat_mode::MAGIC,
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let dropped = drag_spell(&mut h, &state, 0, 2);
    assert_eq!(h.ui.hud.spell_slots.order(0), vec![9, 8, 7]);
    assert_eq!(applied(&[7, 8, 9], &dropped), vec![9, 8, 7]);
}

#[test]
fn a_click_on_a_radar_blip_selects_it() {
    use pyreal_horizon::ui::game::{Blip, BlipKind};
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        blips: vec![Blip {
            id: ObjectId(0x8000_0077),
            dx: 0.0,
            dy: 20.0,
            kind: BlipKind::Npc,
            colour: 0,
            shape: 4,
            name: "Alcott".into(),
        }],
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    // The radar sits at the top right: a 196-pixel disc 26 from the right edge and 44 from the
    // top, its blips out to 82 pixels for 75 metres.
    let centre = (1920.0 - 26.0 - 98.0, 44.0 + 98.0);
    h.move_to(centre.0, centre.1 - 20.0 / 75.0 * 82.0);
    h.press();
    let out = h.frame(&state);
    assert!(
        out.requests
            .contains(&UiRequest::Select(ObjectId(0x8000_0077))),
        "{:?}",
        out.requests
    );
}

#[test]
fn a_spell_tab_longer_than_the_bar_is_paged_and_the_slot_keys_cast_from_the_page_shown() {
    use pyreal_horizon::ui::kit::Drop;
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        spell_tabs: vec![(100..120).collect()],
        combat_mode: pyreal_client_contract::combat_mode::MAGIC,
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let slot = |h: &Harness, i: usize| {
        h.ui.drops
            .iter()
            .find(|(_, d)| *d == Some(Drop::SpellSlot { tab: 0, index: i }))
            .map(|(r, _)| *r)
    };
    assert!(
        slot(&h, 11).is_some() && slot(&h, 12).is_none(),
        "the first page"
    );
    // The next-page arrow stands just past the bar's last slot, at its foot.
    let last = slot(&h, 11).unwrap();
    h.move_to(last.right() + 12.0, last.bottom() - 9.0);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    assert!(
        slot(&h, 12).is_some() && slot(&h, 11).is_none(),
        "the second page"
    );
    // The first slot's key casts the page's first spell, the tab's thirteenth.
    h.ui.magic_notices
        .push(pyreal_client_contract::view::MagicNotice::CastQuickslotSpell { slot: 0 });
    let cast = h.frame(&state);
    assert_eq!(cast.requests, vec![UiRequest::CastSpell { spell_id: 112 }]);
}

#[test]
fn a_full_page_of_spells_offers_an_empty_next_page_to_drop_a_new_spell_onto() {
    use pyreal_horizon::ui::kit::Drop;
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        spell_tabs: vec![(100..112).collect()],
        combat_mode: pyreal_client_contract::combat_mode::MAGIC,
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let slot = |h: &Harness, i: usize| {
        h.ui.drops
            .iter()
            .find(|(_, d)| *d == Some(Drop::SpellSlot { tab: 0, index: i }))
            .map(|(r, _)| *r)
    };
    let last = slot(&h, 11).expect("the first page");
    h.move_to(last.right() + 12.0, last.bottom() - 9.0);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    assert!(
        slot(&h, 12).is_some(),
        "the empty second page takes a drop in its first slot"
    );
}

/// One bar of favorites: the tab keys stay on it, and it casts from no bank it does not show.
#[test]
fn a_world_with_one_bar_of_favorites_stays_on_it_and_casts_only_from_it() {
    use pyreal_client_contract::view::MagicNotice as M;
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        spell_tabs: vec![vec![7]],
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    for notice in [M::NextSpellTab, M::PrevSpellTab, M::LastSpellTab] {
        h.ui.magic_notices.push(notice);
        h.frame(&state);
        assert_eq!(h.ui.hud.spell_tab(), 0, "{notice:?}");
    }
    h.ui.magic_notices.push(M::CastCurrentSpell);
    assert_eq!(
        h.frame(&state).requests,
        vec![UiRequest::CastSpell { spell_id: 7 }]
    );
}

#[test]
fn a_spell_dragged_from_the_first_spell_bar_row_to_an_empty_third_row_stays_there() {
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        spell_tabs: vec![vec![100, 101, 102, 103, 104]],
        combat_mode: pyreal_client_contract::combat_mode::MAGIC,
        ..GameState::default()
    };
    let mut h = Harness::new(pyreal_horizon::options::HorizonOptions {
        spell_rows: 4,
        ..Default::default()
    });
    // The rows chosen reach the HUD after the first frame.
    h.frame(&state);
    h.frame(&state);
    // From the first row's first slot to the third row's fourth, the tab's 28th slot.
    let dropped = drag_spell(&mut h, &state, 0, 27);
    let slots = &h.ui.hud.spell_slots;
    assert_eq!((slots.get(0, 0), slots.get(0, 27)), (None, Some(100)));
    assert_eq!(
        applied(&[100, 101, 102, 103, 104], &dropped),
        vec![101, 102, 103, 104, 100]
    );
    // The game's list closed up as asked, the spell keeps its slot.
    let state = GameState {
        spell_tabs: vec![vec![101, 102, 103, 104, 100]],
        ..state
    };
    h.frame(&state);
    assert_eq!(h.ui.hud.spell_slots.get(0, 27), Some(100));
}

#[test]
fn entering_the_world_as_another_character_leaves_the_last_ones_spell_bar_file_alone() {
    let dir = std::env::temp_dir().join(format!("pyreal-horizon-spellbar-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let alpha = GameState {
        in_world: true,
        name: "Alpha".into(),
        spell_tabs: vec![vec![1, 2, 3]],
        ..GameState::default()
    };
    let mut h = Harness::new(pyreal_horizon::options::HorizonOptions::default());
    h.ui.hud.settings_dir = Some(dir.clone());
    h.frame(&alpha);
    h.ui.hud.spell_slots.place(0, 3, 5);
    h.frame(&alpha);
    let file = dir.join(pyreal_horizon::ui::hud::spell_slots::file_name("", "Alpha"));
    let kept = std::fs::read_to_string(&file).expect("Alpha's slots are kept");
    h.frame(&GameState::default());
    let beta = GameState {
        in_world: true,
        name: "Beta".into(),
        spell_tabs: vec![vec![9]],
        ..GameState::default()
    };
    h.frame(&beta);
    h.frame(&beta);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        kept,
        "Alpha's file was rewritten"
    );
    assert_eq!(h.ui.hud.spell_slots.get(0, 0), Some(9));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Whether a ground the world is blurred under spans slots `a` to `b`, through their middles.
fn frosted(h: &Harness, a: Rect, b: Rect) -> bool {
    let y = a.y + a.h / 2.0;
    h.list.backdrops.iter().any(|(r, s)| {
        *s > 0.0
            && r.x <= a.x + a.w / 2.0
            && r.right() >= b.x + b.w / 2.0
            && r.y <= y
            && r.bottom() >= y
    })
}

#[test]
fn the_shortcut_bar_blurs_the_world_behind_its_slots() {
    let state = player_with_a_shortcut_in_slot(2);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    assert!(frosted(&h, slot_rect(&h, 0), slot_rect(&h, 11)));
}

#[test]
fn the_shortcut_bar_s_blur_reaches_the_frames_round_its_slots() {
    let state = player_with_a_shortcut_in_slot(2);
    // With the art, whose frames are soft on the inside of their rims.
    let mut h = Harness::with_built_in_art();
    h.frame(&state);
    let (a, b) = (slot_rect(&h, 0), slot_rect(&h, 11));
    assert!(h.list.backdrops.iter().any(|(r, _)| {
        r.x <= a.x && r.y <= a.y && r.right() >= b.right() && r.bottom() >= b.bottom()
    }));
}

#[test]
fn each_spell_bar_row_blurs_the_world_behind_it_as_one_ground() {
    let mut state = player_with_a_shortcut_in_slot(2);
    state.combat_mode = pyreal_client_contract::combat_mode::MAGIC;
    let mut h = Harness::new(pyreal_horizon::options::HorizonOptions {
        spell_rows: 3,
        ..Default::default()
    });
    // The rows chosen reach the HUD after the first frame.
    h.frame(&state);
    h.frame(&state);
    let (a, b) = (slot_rect(&h, 0), slot_rect(&h, 11));
    let row = h
        .list
        .backdrops
        .iter()
        .find(|(r, _)| r.x <= a.x + a.w / 2.0 && r.right() >= b.x + b.w / 2.0)
        .map(|(r, _)| *r)
        .expect("the shortcut bar's ground");
    let rows = h
        .list
        .backdrops
        .iter()
        .filter(|(r, _)| (r.x - row.x).abs() < 0.5 && (r.w - row.w).abs() < 0.5)
        .count();
    assert_eq!(rows, 4, "the shortcut bar and three spell rows");
}

#[test]
fn the_hotbars_moved_in_hud_layout_blur_the_world_where_they_were_moved_to() {
    let state = player_with_a_shortcut_in_slot(2);
    let dir = std::env::temp_dir().join(format!(
        "pyreal-horizon-hotbar-frost-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the folder");
    let file = dir.join("horizon-hud-Tester.txt");
    std::fs::write(&file, "hotbars 0.0 -300.0 1.00\n").expect("the layout");
    // Drawn with the art, so the element has quads for the layout to move.
    let mut h = Harness::with_built_in_art();
    h.frame(&state);
    let before = slot_rect(&h, 0);
    h.ui.hud.layout = pyreal_horizon::ui::layout::HudLayout::load(file);
    h.frame(&state);
    let (a, b) = (slot_rect(&h, 0), slot_rect(&h, 11));
    assert!(a.y < before.y - 100.0, "the slots moved up");
    assert!(frosted(&h, a, b));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_slot_s_dark_recess_reaches_the_frame_round_it() {
    let state = player_with_a_shortcut_in_slot(2);
    let mut h = Harness::with_built_in_art();
    h.frame(&state);
    let r = slot_rect(&h, 5);
    // The frame's opening, a unit past the icon's left, right and top.
    let icon = pyreal_horizon::ui::kit::icon_rect(r);
    let want = Rect::new(icon.x - 1.0, icon.y - 1.0, icon.w + 2.0, icon.h + 1.0);
    assert!(
        h.list
            .quads
            .iter()
            .any(|q| q.colour == 0x9608_0A0C && q.dst == want),
        "the recess over {want:?}"
    );
}
