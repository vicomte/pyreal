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
