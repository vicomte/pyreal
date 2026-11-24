//! The corpse marker on the HUD: a death that cost something marks where the game lays the
//! corpse on the radar's rim, Set Waypoint to Corpse under the radar leads back to it, and an
//! emptied, decayed or missing corpse or a right click on its stone clears it.

use pyreal_client_contract::UiRequest;
use pyreal_horizon::ui::corpse::{self as cp, Lying, Place};
use pyreal_horizon::ui::game::{Blip, BlipKind, ChatLine, GameState, Item};
use pyreal_primitives::ObjectId;

use crate::Harness;

/// A unit of the game's coordinates in metres.
const UNIT: f64 = 240.0;

fn standing(deaths: i32, here: (f64, f64)) -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        address: "127.0.0.1:9000".into(),
        here: Some(here),
        deaths: Some(deaths),
        ..GameState::default()
    }
}

/// Where Set Waypoint to Corpse stands: where Clear Waypoint does, under the radar.
fn under_the_radar(h: &Harness) -> (f32, f32) {
    let k = h.ui.layout_scale(h.screen);
    let d = 196.0 * k;
    let x = h.screen.0 - d - 26.0 * k;
    (x + d / 2.0, 44.0 * k + d + 42.0 * k)
}

fn click(h: &mut Harness, state: &GameState, at: (f32, f32)) {
    h.move_to(at.0, at.1);
    h.frame(state);
    h.press();
    h.frame(state);
    h.release();
    h.frame(state);
}

/// The lines `requests` print in chat.
fn said(requests: &[UiRequest]) -> Vec<String> {
    requests
        .iter()
        .filter_map(|r| match r {
            UiRequest::DisplayChatText { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// The game's line on what a death cost, a frame said at `state`.
fn say(h: &mut Harness, state: &GameState, text: &str) -> Vec<String> {
    let line = GameState {
        new_chat: vec![ChatLine {
            chat_type: 0,
            text: text.into(),
            window: 0,
        }],
        ..state.clone()
    };
    said(&h.frame(&line).requests)
}

const LOST: &str = "You've lost 70 Pyreals, and your Short Sword!";
const RETAINED: &str = "You have retained all your items. You do not need to recover your corpse!";
const MARKED: &str = "The place you died is marked on your map and radar.";

/// A corpse object of the player's, `0x8000_0100 + n`, lying at `place`.
fn corpse_at(n: u32, place: Place) -> Lying {
    Lying {
        id: ObjectId(0x8000_0100 + n),
        place,
    }
}

/// A place on the land by Holtburg.
fn holtburg(x: f32, y: f32) -> Place {
    Place::at(0xA9B4_0017, x, y)
}

/// The player, dead at 40.4N 34.5E, then risen 120 m west of it.
fn died() -> (Harness, GameState) {
    let at = (40.4, 34.5);
    let mut h = Harness::with_built_in_art();
    h.frame(&standing(2, at));
    let out = h.frame(&standing(3, at));
    assert!(said(&out.requests).is_empty(), "not until it is known");
    assert_eq!(h.ui.windows.waypoints.corpse.active, None);
    assert_eq!(say(&mut h, &standing(3, at), LOST), [MARKED]);
    let corpse = h.ui.windows.waypoints.corpse.active.expect("marked");
    assert_eq!(
        (corpse.ns, corpse.ew, corpse.in_dungeon),
        (40.4, 34.5, false)
    );
    let risen = standing(3, (40.4, 34.5 - 120.0 / UNIT));
    h.frame(&risen);
    (h, risen)
}

#[test]
fn a_death_marks_the_place_on_the_radar_s_rim_and_set_waypoint_to_corpse_leads_back() {
    let (mut h, risen) = died();
    // The stone stands on the rim east of the middle, the way to the corpse.
    let stone = h.ui.hud.corpse_stone.expect("the stone on the rim");
    let k = h.ui.layout_scale(h.screen);
    let d = 196.0 * k;
    let (cx, cy) = (h.screen.0 - d - 26.0 * k + d / 2.0, 44.0 * k + d / 2.0);
    let (sx, sy) = (stone.x + stone.w / 2.0, stone.y + stone.h / 2.0);
    assert!(sx > cx + d / 3.0 && (sy - cy).abs() < 2.0, "{stone:?}");
    let at = under_the_radar(&h);
    click(&mut h, &risen, at);
    let w = h.ui.windows.waypoints.active.clone().expect("the waypoint");
    assert_eq!(w.label(), "Corpse of Tester");
    assert!((w.ns - 40.4).abs() < 1e-9 && (w.ew - 34.5).abs() < 1e-9);
    // The arrow points there now; the stone stands aside for it.
    assert_eq!(h.ui.hud.corpse_stone, None);
    // A click where the button stood clears the waypoint, and the corpse stays marked.
    click(&mut h, &risen, at);
    assert_eq!(h.ui.windows.waypoints.active, None);
    assert!(h.ui.windows.waypoints.corpse.active.is_some());
    // A click on the rim's stone sets it too.
    let stone = h.ui.hud.corpse_stone.expect("the stone back");
    click(
        &mut h,
        &risen,
        (stone.x + stone.w / 2.0, stone.y + stone.h / 2.0),
    );
    assert_eq!(
        h.ui.windows.waypoints.active.map(|w| w.label()),
        Some("Corpse of Tester".to_owned())
    );
}

#[test]
fn opening_the_corpse_and_emptying_it_clears_the_marker() {
    let (mut h, _) = died();
    let by = standing(3, (40.4, 34.5 - 2.0 / UNIT));
    let corpse = |items: Vec<Item>| GameState {
        loot: Some((ObjectId(0x8000_0001), "Corpse of Tester".into(), items)),
        ..by.clone()
    };
    h.frame(&corpse(Vec::new()));
    h.frame(&corpse(vec![Item::default(), Item::default()]));
    assert!(h.ui.windows.waypoints.corpse.active.is_some(), "still full");
    let out = h.frame(&corpse(Vec::new()));
    assert_eq!(h.ui.windows.waypoints.corpse.active, None, "recovered");
    assert!(said(&out.requests).is_empty(), "{:?}", out.requests);
    h.frame(&by);
    assert_eq!(h.ui.hud.corpse_stone, None);
}

#[test]
fn back_by_the_place_with_no_corpse_there_the_marker_clears() {
    let (mut h, _) = died();
    h.ui.time += 600.0;
    let by = standing(3, (40.4 + 6.0 / UNIT, 34.5));
    // While the corpse is seen, it is kept.
    let seen = GameState {
        targets: vec![Blip {
            id: ObjectId(0x8000_0001),
            dx: 0.0,
            dy: 6.0,
            kind: BlipKind::Item,
            colour: 0,
            shape: 0,
            name: "Corpse of Tester".into(),
        }],
        ..by.clone()
    };
    for _ in 0..3 {
        h.frame(&seen);
        h.ui.time += 3.0;
    }
    assert!(h.ui.windows.waypoints.corpse.active.is_some());
    // Gone from the place, it decayed.
    h.frame(&by);
    h.ui.time += 6.0;
    let out = h.frame(&by);
    assert_eq!(h.ui.windows.waypoints.corpse.active, None);
    assert_eq!(
        said(&out.requests),
        ["Your corpse is no longer here, so its marker is cleared."]
    );
}

#[test]
fn a_death_in_a_dungeon_shows_on_the_radar_there_and_offers_no_waypoint() {
    let at = (1.2, 95.0);
    let below = |deaths, here| GameState {
        in_dungeon: true,
        ..standing(deaths, here)
    };
    let mut h = Harness::with_built_in_art();
    h.frame(&below(0, at));
    h.frame(&below(1, at));
    say(&mut h, &below(1, at), LOST);
    let risen = below(1, (1.2, 95.0 - 30.0 / UNIT));
    h.frame(&risen);
    assert!(
        h.ui.hud.corpse_stone.is_some(),
        "on the radar in the dungeon"
    );
    let at = under_the_radar(&h);
    click(&mut h, &risen, at);
    assert_eq!(h.ui.windows.waypoints.active, None, "no button to set one");
    h.frame(&standing(1, (40.0, 30.0)));
    assert_eq!(h.ui.hud.corpse_stone, None, "outdoors it is not shown");
    // Back below, a right click on the stone forgets it.
    h.frame(&risen);
    let stone = h.ui.hud.corpse_stone.expect("the stone");
    h.move_to(stone.x + stone.w / 2.0, stone.y + stone.h / 2.0);
    h.frame(&risen);
    h.input.pressed[1] = true;
    h.frame(&risen);
    assert_eq!(h.ui.windows.waypoints.corpse.active, None);
}

#[test]
fn a_death_mid_recall_is_marked_where_the_game_lays_the_corpse() {
    // The count rises with the player already at the lifestone.
    let stone = (12.0, 30.0);
    let fell = holtburg(100.0, 50.0);
    let mut h = Harness::with_built_in_art();
    let mut state = GameState {
        last_outside_death: Some(holtburg(10.0, 10.0)),
        ..standing(2, stone)
    };
    h.frame(&state);
    state.deaths = Some(3);
    h.frame(&state);
    // The game's last outside death, and the corpse itself, come just after.
    state.last_outside_death = Some(fell);
    state.own_corpses = vec![corpse_at(1, fell)];
    h.frame(&state);
    assert_eq!(say(&mut h, &state, LOST), [MARKED]);
    let c = h.ui.windows.waypoints.corpse.active.expect("marked");
    assert!(
        (c.ns - fell.ns).abs() < 1e-9 && (c.ew - fell.ew).abs() < 1e-9,
        "{c:?} at {fell:?}"
    );
    let w = h.ui.windows.waypoints.corpse.waypoint("Tester").unwrap();
    assert!(w.way_from(stone).1 > 1000.0, "not at the lifestone");
}

#[test]
fn a_death_that_kept_every_item_marks_nothing() {
    let at = (40.4, 34.5);
    let mut h = Harness::with_built_in_art();
    h.frame(&standing(2, at));
    h.frame(&standing(3, at));
    assert!(say(&mut h, &standing(3, at), RETAINED).is_empty());
    h.ui.time += 40.0;
    let out = h.frame(&standing(3, at));
    assert!(said(&out.requests).is_empty());
    assert_eq!(h.ui.windows.waypoints.corpse.active, None);
}

#[test]
fn a_pk_lite_death_to_another_player_marks_nothing() {
    let at = (40.4, 34.5);
    let duel = |deaths, pk_lite| GameState {
        pk_lite,
        targets: vec![Blip {
            id: ObjectId(0x5000_0002),
            dx: 3.0,
            dy: 0.0,
            kind: BlipKind::Player,
            colour: 0,
            shape: 0,
            name: "Gharu Duelist".into(),
        }],
        ..standing(deaths, at)
    };
    for (pk_lite, marked) in [(true, false), (false, true)] {
        let mut h = Harness::with_built_in_art();
        h.frame(&duel(2, pk_lite));
        let mut death = duel(3, pk_lite);
        death.own_deaths = vec![Some(Some("Gharu Duelist".into()))];
        h.frame(&death);
        h.ui.time += 25.0;
        h.frame(&duel(3, pk_lite));
        assert_eq!(
            h.ui.windows.waypoints.corpse.active.is_some(),
            marked,
            "PK Lite {pk_lite}"
        );
    }
}

#[test]
fn an_empty_corpse_rotting_at_once_marks_nothing() {
    let at = holtburg(100.0, 50.0);
    let stone = (at.ns + 30.0 / UNIT, at.ew);
    for rots in [true, false] {
        let mut h = Harness::with_built_in_art();
        h.frame(&standing(2, (at.ns, at.ew)));
        h.frame(&standing(3, (at.ns, at.ew)));
        h.ui.time += 4.0;
        let risen = standing(3, stone);
        let lying = GameState {
            own_corpses: vec![corpse_at(2, at)],
            ..risen.clone()
        };
        h.frame(&lying);
        h.ui.time += 11.0;
        h.frame(&lying);
        // Fifteen seconds on it rots, in sight of the lifestone.
        let after = if rots { &risen } else { &lying };
        let out = h.frame(after);
        assert!(said(&out.requests).is_empty());
        h.ui.time += 30.0;
        h.frame(after);
        assert_eq!(
            h.ui.windows.waypoints.corpse.active.is_some(),
            !rots,
            "rots {rots}"
        );
    }
}

#[test]
fn a_corpse_seen_to_decay_up_close_clears_the_marker() {
    let at = holtburg(100.0, 50.0);
    let mut h = Harness::with_built_in_art();
    let mut state = standing(2, (at.ns, at.ew));
    h.frame(&state);
    state.deaths = Some(3);
    h.frame(&state);
    state.own_corpses = vec![corpse_at(3, at)];
    assert_eq!(say(&mut h, &state, LOST), [MARKED]);
    // An hour on, the player stands by it as it rots away.
    h.ui.time += 3600.0;
    state.here = Some((at.ns + 5.0 / UNIT, at.ew));
    h.frame(&state);
    assert!(h.ui.windows.waypoints.corpse.active.is_some());
    state.own_corpses.clear();
    let out = h.frame(&state);
    assert_eq!(h.ui.windows.waypoints.corpse.active, None);
    assert_eq!(
        said(&out.requests),
        ["Your corpse is no longer here, so its marker is cleared."]
    );
}

#[test]
fn a_marker_is_restored_at_login_and_a_death_another_client_saw_is_marked() {
    let dir = std::env::temp_dir().join(format!("pyreal-hud-corpse-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(cp::file_name("127.0.0.1:9000|Tester"));
    let first = holtburg(100.0, 50.0);
    let login = |h: &mut Harness, outside: Place| {
        h.ui.hud.settings_dir = Some(dir.clone());
        let state = GameState {
            last_outside_death: Some(outside),
            ..standing(3, (first.ns, first.ew + 0.5))
        };
        said(&h.frame(&state).requests)
    };
    // Died and marked in one session, the marker is there in the next.
    let mut h = Harness::with_built_in_art();
    h.ui.hud.settings_dir = Some(dir.clone());
    let mut state = GameState {
        last_outside_death: Some(holtburg(10.0, 10.0)),
        ..standing(2, (first.ns, first.ew))
    };
    h.frame(&state);
    state.deaths = Some(3);
    h.frame(&state);
    state.last_outside_death = Some(first);
    assert_eq!(say(&mut h, &state, LOST), [MARKED]);
    let marked = h.ui.windows.waypoints.corpse.active;
    let mut h = Harness::with_built_in_art();
    assert!(login(&mut h, first).is_empty());
    assert_eq!(h.ui.windows.waypoints.corpse.active, marked);
    // Recovered, it stays forgotten at the next login.
    h.ui.windows.waypoints.corpse.forget();
    let mut h = Harness::with_built_in_art();
    assert!(login(&mut h, first).is_empty());
    assert_eq!(h.ui.windows.waypoints.corpse.active, None);
    // The game's last outside death moved while this client was away: a death it did not see.
    let since = holtburg(150.0, 20.0);
    let mut h = Harness::with_built_in_art();
    assert_eq!(login(&mut h, since), [MARKED]);
    let c =
        h.ui.windows
            .waypoints
            .corpse
            .active
            .expect("marked at login");
    assert!((c.ns - since.ns).abs() < 1e-9 && (c.ew - since.ew).abs() < 1e-9);
    assert!(file.exists());
    std::fs::remove_dir_all(&dir).ok();
}
