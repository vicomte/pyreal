//! The waypoint on the HUD: the Clear Waypoint button under the radar while one is followed.

use pyreal_horizon::ui::game::GameState;
use pyreal_horizon::ui::waypoint::Waypoint;

use crate::Harness;

fn standing() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        address: "127.0.0.1:9000".into(),
        here: Some((40.4, 34.5)),
        ..GameState::default()
    }
}

/// Where the Clear Waypoint button stands: centred under the radar and its coordinates.
fn clear_button(h: &Harness) -> (f32, f32) {
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

#[test]
fn clear_waypoint_under_the_radar_clears_the_waypoint_and_the_ring_goes_with_it() {
    let state = standing();
    let mut h = Harness::with_built_in_art();
    h.frame(&state);
    h.ui.windows.waypoints.set(Waypoint {
        ns: 40.9,
        ew: 34.9,
        name: Some("Holtburg road".into()),
    });
    h.ui.options.waypoint_style = pyreal_horizon::ui::waypoint::WaypointStyle::Both;
    h.frame(&state);
    let ring = h.ui.waypoint_ring.expect("the ring round the character");
    // The road is north-east, a little more north than east.
    assert!(ring.bearing > 0.6 && ring.bearing < 0.8, "{}", ring.bearing);
    // In a dungeon the ring is not laid, and the waypoint is kept.
    let below = GameState {
        in_dungeon: true,
        ..standing()
    };
    h.frame(&below);
    assert_eq!(h.ui.waypoint_ring, None);
    assert!(h.ui.windows.waypoints.active.is_some());
    let at = clear_button(&h);
    click(&mut h, &state, at);
    assert_eq!(h.ui.windows.waypoints.active, None, "cleared");
    assert_eq!(h.ui.waypoint_ring, None);
}

/// The radar shows how far the waypoint is beside its arrow on the rim, counting down as the
/// player walks, and shows nothing once they arrive, in a dungeon or with no waypoint.
#[test]
/// A long distance due east stays clear of the rim arrow it sits inside.
fn a_long_distance_beside_the_east_arrow_keeps_clear_of_it() {
    let state = GameState {
        here: Some((40.4, -80.0)),
        ..standing()
    };
    let mut h = Harness::with_built_in_art();
    h.frame(&state);
    h.ui.windows.waypoints.set(Waypoint {
        ns: 40.4,
        ew: 37.5,
        name: None,
    });
    h.frame(&state);
    let (text, at) = h.ui.hud.waypoint_distance.clone().expect("the distance");
    assert_eq!(text, "28.2 km");
    let k = h.ui.layout_scale(h.screen);
    let d = 196.0 * k;
    let cx = h.screen.0 - d - 26.0 * k + d / 2.0;
    // The rim arrow reaches in to 12 units inside the radius it turns on.
    let arrow_inner = cx + (d / 2.0 - 16.0 * k) - 12.0 * k;
    assert!(
        at.x + at.w <= arrow_inner,
        "{at:?} runs into the arrow at x {arrow_inner}"
    );
    assert!(at.x > cx, "{at:?} should sit east of the middle {cx}");
}

#[test]
fn the_radar_shows_the_waypoint_s_distance_by_its_arrow_until_the_player_arrives() {
    let mut state = standing();
    let mut h = Harness::with_built_in_art();
    h.frame(&state);
    assert_eq!(h.ui.hud.waypoint_distance, None, "no waypoint");
    h.ui.windows.waypoints.set(Waypoint {
        ns: 40.9,
        ew: 34.9,
        name: None,
    });
    h.frame(&state);
    let (text, at) = h.ui.hud.waypoint_distance.clone().expect("the distance");
    // 120 m north and 96 m east.
    assert_eq!(text, "154 m");
    // Inside the radar, up and to the right of its middle, the way the arrow points.
    let k = h.ui.layout_scale(h.screen);
    let d = 196.0 * k;
    let radar = (h.screen.0 - d - 26.0 * k, 44.0 * k);
    let (cx, cy) = (radar.0 + d / 2.0, radar.1 + d / 2.0);
    let (mx, my) = (at.x + at.w / 2.0, at.y + at.h / 2.0);
    assert!(mx > cx && my < cy, "{at:?} against the middle {cx}, {cy}");
    assert!(((mx - cx).powi(2) + (my - cy).powi(2)).sqrt() < d / 2.0);
    // Far away it reads in kilometres.
    state.here = Some((35.0, 34.9));
    h.frame(&state);
    let far = h.ui.hud.waypoint_distance.clone().expect("the distance").0;
    assert_eq!(far, "1.4 km");
    // Walking closer, it counts down.
    state.here = Some((40.9 - 30.0 / 240.0, 34.9));
    h.frame(&state);
    assert_eq!(h.ui.hud.waypoint_distance.clone().expect("near").0, "30 m");
    // In a dungeon there is no arrow, and no distance.
    let below = GameState {
        in_dungeon: true,
        ..state.clone()
    };
    h.frame(&below);
    assert_eq!(h.ui.hud.waypoint_distance, None);
    // Arriving clears the waypoint, and the distance goes with it.
    state.here = Some((40.9 - 5.0 / 240.0, 34.9));
    h.frame(&state);
    h.frame(&state);
    assert_eq!(h.ui.windows.waypoints.active, None, "arrived");
    assert_eq!(h.ui.hud.waypoint_distance, None);
}
