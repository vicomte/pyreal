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

/// Every request in three frames: moved to `at`, pressed and let go.
fn click_out(
    h: &mut Harness,
    state: &GameState,
    at: (f32, f32),
) -> Vec<pyreal_client_contract::UiRequest> {
    h.move_to(at.0, at.1);
    let mut out = h.frame(state).requests;
    h.press();
    out.extend(h.frame(state).requests);
    h.release();
    out.extend(h.frame(state).requests);
    out
}

/// The lines `requests` print in chat.
fn said(requests: &[pyreal_client_contract::UiRequest]) -> Vec<String> {
    requests
        .iter()
        .filter_map(|r| match r {
            pyreal_client_contract::UiRequest::DisplayChatText { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_place_in_any_chat_line_is_a_link_that_sets_the_waypoint_there() {
    use pyreal_horizon::ui::game::ChatLine;
    let mut state = standing();
    state.new_chat = vec![ChatLine {
        chat_type: 2,
        text: r#"Bram says, "Meet at 12.3N, 45.6E by the well, not at 99 Elm.""#.into(),
        window: pyreal_client_contract::chat::interface::window::MAIN,
    }];
    let mut h = Harness::with_built_in_art();
    h.frame(&state);
    state.new_chat.clear();
    assert!(
        h.ui.hud.log.control("Place: 99 E").is_none(),
        "a number and a letter in a word is no place"
    );
    let link =
        h.ui.hud
            .log
            .control("Place: 12.3N, 45.6E")
            .expect("the place is a link");
    let out = click_out(
        &mut h,
        &state,
        (link.x + link.w / 2.0, link.y + link.h / 2.0),
    );
    let w = h.ui.windows.waypoints.active.clone().expect("the waypoint");
    assert!((w.ns - 12.3).abs() < 1e-9 && (w.ew - 45.6).abs() < 1e-9 && w.name.is_none());
    assert_eq!(said(&out), ["Waypoint set to 12.3N 45.6E."]);
    assert!(
        !out.iter()
            .any(|r| matches!(r, pyreal_client_contract::UiRequest::ChatEntry { .. })),
        "the click begins no tell: {out:?}"
    );
}

/// The last draft `requests` told the game.
fn draft(requests: &[pyreal_client_contract::UiRequest]) -> Option<String> {
    requests.iter().rev().find_map(|r| match r {
        pyreal_client_contract::UiRequest::ChatEntry {
            text,
            action: pyreal_client_contract::chat::entry::EntryAction::Draft,
            ..
        } => Some(text.clone()),
        _ => None,
    })
}

#[test]
fn share_waypoint_on_the_map_puts_its_name_and_place_on_the_chat_line_as_plain_text() {
    let state = standing();
    let mut h = Harness::with_built_in_art();
    h.frame(&state);
    h.ui.windows.waypoints.set(Waypoint {
        ns: 40.9,
        ew: -34.9,
        name: Some("Holtburg road".into()),
    });
    h.ui.windows
        .open(pyreal_horizon::ui::panels::WindowId::Map, 0.0);
    h.frame(&state);
    let share = |h: &Harness| {
        h.ui.windows
            .map_view
            .bar
            .iter()
            .find(|(n, _)| *n == "Share Waypoint")
            .map(|(_, r)| (r.x + r.w / 2.0, r.y + r.h / 2.0))
            .expect("Share Waypoint")
    };
    let at = share(&h);
    let mut out = click_out(&mut h, &state, at);
    out.extend(h.frame(&state).requests);
    const SHARED: &str = "Holtburg road at 40.9N, 34.9W";
    assert_eq!(draft(&out).as_deref(), Some(SHARED));
    // It reads back as a place, a link in this client.
    let (_, links) = pyreal_horizon::ui::hud::chat_links(SHARED);
    assert_eq!(
        links,
        [(
            17,
            29,
            pyreal_horizon::ui::hud::LinkTarget::Place("40.9N, 34.9W".into())
        )]
    );
    // Shared again, it goes after the line, which the click on the map left with the game.
    let state = GameState {
        chat_draft: SHARED.into(),
        ..state
    };
    let at = share(&h);
    let mut out = click_out(&mut h, &state, at);
    out.extend(h.frame(&state).requests);
    assert_eq!(
        draft(&out).as_deref(),
        Some(format!("{SHARED} {SHARED}").as_str())
    );
    // With no name it is the place alone.
    let bare = Waypoint {
        ns: 40.9,
        ew: -34.9,
        name: None,
    };
    assert_eq!(
        pyreal_horizon::ui::waypoint::share_text(&bare),
        "40.9N, 34.9W"
    );
}
