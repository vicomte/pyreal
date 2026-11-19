//! The map window: its picture zooms and pans for everyone, and what it says of a place follows.

use std::sync::{Arc, OnceLock};

use pyreal_client_contract::UiRequest;
use pyreal_horizon::art::Art;
use pyreal_horizon::draw::{DrawList, Rect};
use pyreal_horizon::options::HorizonOptions;
use pyreal_horizon::ui::game::GameState;
use pyreal_horizon::ui::input::InputFrame;
use pyreal_horizon::ui::panels::map::{map_pixel, tile_key, PlaceKind, OVERVIEW_KEY};
use pyreal_horizon::ui::panels::WindowId;
use pyreal_horizon::ui::HorizonUi;
use pyreal_scene::world_map::{Built, MapGrid, Pixels, WorldMap};

use crate::Harness;

fn player() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    }
}

fn administrator() -> GameState {
    GameState {
        is_admin: true,
        ..player()
    }
}

/// The map window open over the art behind it, and where its picture is.
fn opened(art: &Arc<Art>, state: &GameState) -> (Harness, Rect) {
    let mut h = Harness {
        ui: HorizonUi::new(Arc::clone(art), HorizonOptions::default()),
        list: DrawList::default(),
        input: InputFrame::default(),
        screen: (1920.0, 1080.0),
    };
    h.frame(state);
    h.ui.windows.open(WindowId::Map, 0.0);
    h.frame(state);
    let area = h.ui.windows.map_picture.expect("the map's picture");
    (h, area)
}

fn built_in() -> Arc<Art> {
    let pieces = pyreal_horizon::pieces::Pieces::built_in();
    Arc::new(Art::new(Arc::new(pieces)))
}

/// A world of one island about Holtburg, drawn once for every test.
fn drawn() -> Built {
    static DRAWN: OnceLock<(Arc<WorldMap>, Pixels)> = OnceLock::new();
    let (map, overview) = DRAWN.get_or_init(|| {
        let mut heights = [0.0; 256];
        for (i, h) in heights.iter_mut().enumerate() {
            *h = f32::from(u8::try_from(i).unwrap_or(0)) * 2.0;
        }
        let names = ["WaterDeepSea", "Grassland", "Snow"].map(str::to_owned);
        let mut grid = MapGrid::new(names.to_vec(), heights);
        for x in 1300..1400 {
            for y in 1400..1500 {
                let height = u8::try_from((x + y) % 50).unwrap_or(0);
                grid.set((x, y), if x < 1350 { 1 } else { 2 }, height, y == 1450);
            }
        }
        let map = grid.prepare();
        let overview = map.overview();
        (Arc::new(map), overview)
    });
    Built {
        map: Arc::clone(map),
        overview: Some(overview.clone()),
    }
}

fn middle(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

fn wheel(h: &mut Harness, state: &GameState, at: (f32, f32), notches: f32) {
    h.move_to(at.0, at.1);
    h.input.wheel = notches;
    h.frame(state);
}

fn click(h: &mut Harness, state: &GameState, at: (f32, f32)) -> Vec<UiRequest> {
    h.move_to(at.0, at.1);
    h.frame(state);
    h.press();
    let mut out = h.frame(state).requests;
    h.release();
    out.extend(h.frame(state).requests);
    out
}

#[test]
fn a_player_s_wheel_zooms_about_the_pointer_and_the_buttons_step_and_fit() {
    let (state, art) = (player(), Arc::new(Art::empty()));
    let (mut h, area) = opened(&art, &state);
    let at = (area.x + area.w * 0.7, area.y + area.h * 0.3);
    let before = h.ui.windows.map_view.on_picture(area, at);
    wheel(&mut h, &state, at, 4.0);
    let view = &h.ui.windows.map_view;
    assert!(view.zoom > 2.0, "{}", view.zoom);
    let after = view.on_picture(area, at);
    assert!(
        (before.0 - after.0).abs() < 1.0 && (before.1 - after.1).abs() < 1.0,
        "{before:?} {after:?}"
    );
    assert_eq!(
        h.input.wheel, 0.0,
        "the wheel is the map's, not the world's"
    );

    let zoom = h.ui.windows.map_view.zoom;
    let [out, closer, fit] = h.ui.windows.map_view.buttons.expect("the buttons");
    click(&mut h, &state, middle(closer));
    assert!(h.ui.windows.map_view.zoom > zoom);
    click(&mut h, &state, middle(out));
    assert!((h.ui.windows.map_view.zoom - zoom).abs() < 0.01);
    click(&mut h, &state, middle(fit));
    assert_eq!(h.ui.windows.map_view.zoom, 1.0);
}

#[test]
fn a_drag_pans_a_zoomed_map_and_the_whole_picture_stays_put() {
    let (state, art) = (player(), Arc::new(Art::empty()));
    let (mut h, area) = opened(&art, &state);
    let at = middle(area);
    let drag = |h: &mut Harness| {
        h.move_to(at.0, at.1);
        h.frame(&state);
        h.press();
        h.frame(&state);
        h.move_to(at.0 - 40.0, at.1 - 20.0);
        h.frame(&state);
        h.release();
        h.frame(&state);
    };
    let whole = h.ui.windows.map_view.centre;
    drag(&mut h);
    assert_eq!(h.ui.windows.map_view.centre, whole, "nowhere to pan to");

    wheel(&mut h, &state, at, 6.0);
    let was = h.ui.windows.map_view.centre;
    drag(&mut h);
    let view = &h.ui.windows.map_view;
    let k = view.shown().w / area.w;
    assert!((view.centre.0 - (was.0 + 40.0 * k)).abs() < 0.5);
    assert!((view.centre.1 - (was.1 + 20.0 * k)).abs() < 0.5);
}

#[test]
fn a_teleport_from_a_zoomed_map_goes_to_the_place_under_the_pointer() {
    let (state, art) = (administrator(), Arc::new(Art::empty()));
    let (mut h, area) = opened(&art, &state);
    wheel(&mut h, &state, middle(area), 6.0);
    let b = h.ui.windows.admin.map_button.expect("the button");
    click(&mut h, &state, middle(b));
    let holtburg = map_pixel(42.2, 33.8);
    h.ui.windows.map_view.centre = holtburg;
    let at = h.ui.windows.map_view.on_screen(area, holtburg);
    let sent = click(&mut h, &state, at);
    let lines: Vec<_> = sent
        .iter()
        .filter_map(|r| match r {
            UiRequest::ChatLineOnBehalf { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(lines, ["@tele 42.2n,33.8e"]);
}

/// The quads drawn from picture `key` this frame.
fn drawn_from(h: &Harness, art: &Art, key: &str) -> usize {
    let Some(Some(sprite)) = art.lookup(key) else {
        return 0;
    };
    h.list
        .quads
        .iter()
        .filter(|q| q.tex == Some(sprite.tex))
        .count()
}

#[test]
fn the_map_is_the_sea_until_its_picture_is_drawn_and_then_the_whole() {
    let (state, art) = (player(), Arc::new(Art::empty()));
    let (mut h, area) = opened(&art, &state);
    let sea = h
        .list
        .quads
        .iter()
        .find(|q| q.tex.is_none() && q.dst == area)
        .expect("the plain ground");
    assert_eq!(sea.colour, 0xFF70_859F, "the sea's colour");
    assert!(art.lookup(OVERVIEW_KEY).is_none(), "no data, no picture");

    h.ui.windows.map_view.show(drawn());
    h.frame(&state);
    assert_eq!(drawn_from(&h, &art, OVERVIEW_KEY), 1, "the whole");
    let picture = art
        .lookup(OVERVIEW_KEY)
        .flatten()
        .and_then(|s| art.image(s.tex))
        .expect("its pixels");
    assert_eq!((picture.width, picture.height), (2048, 2048));
}

#[test]
fn tiles_are_drawn_for_a_close_view_one_a_frame_and_let_go_when_the_map_closes() {
    let (state, art) = (player(), Arc::new(Art::empty()));
    let (mut h, _) = opened(&art, &state);
    h.ui.windows.map_view.show(drawn());
    h.frame(&state);
    assert!(art.lookup(OVERVIEW_KEY).is_some(), "the whole, at the fit");
    let kept = |art: &Art| {
        (0..6)
            .flat_map(|c| (0..6).map(move |r| tile_key(c, r)))
            .filter(|t| art.lookup(t).is_some())
            .count()
    };
    assert_eq!(kept(&art), 0, "no tile yet");

    let holtburg = map_pixel(42.2, 33.8);
    h.ui.windows.map_view.zoom = 12.0;
    h.ui.windows.map_view.centre = holtburg;
    h.frame(&state);
    assert_eq!(kept(&art), 1, "one a frame");
    for _ in 0..4 {
        h.frame(&state);
    }
    assert_eq!(
        drawn_from(&h, &art, &tile_key(3, 1)),
        1,
        "the tile under Holtburg"
    );

    h.ui.windows.close(WindowId::Map);
    h.frame(&state);
    assert_eq!(kept(&art), 0);
    assert!(!h.ui.windows.map_view.is_ready(), "the world let go");
    assert!(art.lookup(OVERVIEW_KEY).is_some(), "and the whole kept");
}

/// A player's zoomed map shows the major towns and the houses; an administrator's also marks the
/// portals.
#[test]
fn a_player_sees_the_towns_and_houses_and_an_administrator_every_place() {
    let art = built_in();
    let mut state = player();
    let (mut h, _) = opened(&art, &state);
    h.ui.windows.map_view.zoom = 12.0;
    h.ui.windows.map_view.centre = map_pixel(42.2, 33.8);
    h.frame(&state);
    let town = art.lookup("map.town").flatten().expect("the town icon").tex;
    let icons = |h: &Harness| h.list.quads.iter().filter(|q| q.tex == Some(town)).count();
    let marks = |h: &Harness, kind: PlaceKind| {
        h.list
            .quads
            .iter()
            .filter(|q| q.tex.is_none() && q.colour & 0x00FF_FFFF == kind.colour() & 0x00FF_FFFF)
            .count()
    };
    assert_eq!(icons(&h), 1, "Holtburg's");
    assert!(
        marks(&h, PlaceKind::Cottage) > 0,
        "the cottages about Holtburg"
    );
    assert_eq!(marks(&h, PlaceKind::Portal), 0);

    state.is_admin = true;
    h.frame(&state);
    assert_eq!(icons(&h), 1);
    assert!(marks(&h, PlaceKind::Portal) > 0, "its portals");
}

/// Marks of `kind`'s colour drawn this frame.
fn kind_marks(h: &Harness, kind: PlaceKind) -> usize {
    h.list
        .quads
        .iter()
        .filter(|q| q.tex.is_none() && q.colour & 0x00FF_FFFF == kind.colour() & 0x00FF_FFFF)
        .count()
}

/// A Launch world's map shows no houses about Holtburg, which came later, and an end-of-retail
/// world's does.
#[test]
fn a_launch_world_s_map_has_no_houses_and_an_end_of_retail_world_s_does() {
    let art = built_in();
    let mut state = player();
    state.era_id = pyreal_primitives::EraId::Launch;
    let (mut h, _) = opened(&art, &state);
    h.ui.windows.map_view.zoom = 12.0;
    h.ui.windows.map_view.centre = map_pixel(42.2, 33.8);
    h.frame(&state);
    assert_eq!(kind_marks(&h, PlaceKind::Cottage), 0, "no houses in 1999");
    state.era_id = pyreal_primitives::EraId::Eor;
    h.frame(&state);
    assert!(
        kind_marks(&h, PlaceKind::Cottage) > 0,
        "the end of retail's"
    );
}

/// The world's own places, when its server publishes them, are drawn in place of those built in,
/// and an administrator's Places list turns a kind off.
#[test]
fn a_world_s_own_places_are_drawn_and_the_places_list_turns_a_kind_off() {
    let art = built_in();
    let mut state = administrator();
    // Version 2 of a world's places: a vendor marked at 42.2N 33.8E.
    state.map_places = Some(Arc::from(
        "dereth-places-store\t2\nworld\tW\nmark\tvendor\tJonathan\t0xA9B4002A 132.000 36.000 0.000\t-\n",
    ));
    let (mut h, _) = opened(&art, &state);
    h.ui.windows.map_view.zoom = 12.0;
    h.ui.windows.map_view.centre = map_pixel(42.2, 33.8);
    h.frame(&state);
    assert_eq!(kind_marks(&h, PlaceKind::Vendor), 1, "the world's vendor");
    assert_eq!(kind_marks(&h, PlaceKind::Cottage), 0, "none built in");

    // The Places button, beside Fit, opens the list; a kind turned off there is not drawn.
    let fit = h.ui.windows.map_view.buttons.expect("the buttons")[2];
    let k = fit.h / 26.0;
    let places = (fit.right() + 4.0 * k + 32.0 * k, fit.y + fit.h / 2.0);
    click(&mut h, &state, places);
    assert!(h.ui.windows.map_view.kinds_open, "the list is open");
    assert_eq!(
        kind_marks(&h, PlaceKind::Vendor),
        2,
        "the mark and the list's swatch"
    );
    h.ui.windows.map_view.hidden.push(PlaceKind::Vendor);
    h.ui.windows.map_view.kinds_open = false;
    h.frame(&state);
    assert_eq!(kind_marks(&h, PlaceKind::Vendor), 0, "turned off");
}

/// The arrow's quad this frame, when there is one: the only picture of its picture's size.
fn arrow_quad(h: &Harness) -> Option<pyreal_horizon::draw::Quad> {
    h.list
        .quads
        .iter()
        .rfind(|q| q.tex.is_some() && q.src.w == 64.0)
        .copied()
}

#[test]
fn an_administrator_s_arrow_points_straight_down_at_the_place_and_no_one_else_has_it() {
    let art = Arc::new(Art::empty());
    let mut state = administrator();
    state.coords = Some((42.2, 33.8));
    state.heading = 90.0;
    let (mut h, area) = opened(&art, &state);
    let q = arrow_quad(&h).expect("the arrow");
    assert_eq!(q.turn, 0.0, "it does not turn with the heading");
    let picture = art.image(q.tex.expect("a picture")).expect("its pixels");
    assert!(picture.pixel(32, 60)[3] > 0, "the tip is at the foot");
    assert_eq!(picture.pixel(32, 1)[3], 0, "and nothing is at the head");
    let tip = (q.dst.x + q.dst.w / 2.0, q.dst.y + q.dst.h * 62.0 / 64.0);
    let place = h.ui.windows.map_view.on_screen(area, map_pixel(42.2, 33.8));
    assert!(
        (tip.0 - place.0).abs() < 0.1 && (tip.1 - place.1).abs() < 0.1,
        "{tip:?} {place:?}"
    );

    state.is_admin = false;
    h.frame(&state);
    assert!(arrow_quad(&h).is_none(), "a player keeps the game's mark");
}

/// Explore mode over Holtburg, with no world and no server.
fn exploring() -> GameState {
    let view = pyreal_client_runtime::explore::ExploreView {
        map_at: Some((42.1, 33.6)),
        ..Default::default()
    };
    GameState {
        coords: view.map_at,
        explore: Some(view),
        ..GameState::default()
    }
}

/// The explore corner's buttons, Map then Settings, at the interface's scale of 1.
fn corner_buttons(h: &Harness) -> [(f32, f32); 2] {
    let (sw, sh) = h.screen;
    let y = sh - 14.0 - 7.0 - 17.0;
    let right = sw - 14.0 - 6.0 - 17.0;
    [(right - 36.0, y), (right, y)]
}

fn explore_click(
    h: &mut Harness,
    state: &GameState,
    at: (f32, f32),
) -> Vec<pyreal_client_runtime::explore::Ask> {
    h.move_to(at.0, at.1);
    h.frame(state);
    h.press();
    let mut out = h.frame(state).explore;
    h.release();
    out.extend(h.frame(state).explore);
    out
}

#[test]
fn explore_s_corner_opens_the_map_and_settings_and_a_click_on_the_map_goes_there() {
    use pyreal_client_runtime::explore::Ask;
    let state = exploring();
    let mut h = Harness::with_built_in_art();
    h.frame(&state);
    let [map, settings] = corner_buttons(&h);
    explore_click(&mut h, &state, settings);
    assert!(h.ui.windows.is_open(WindowId::Options), "Settings opens");
    explore_click(&mut h, &state, settings);
    assert!(!h.ui.windows.is_open(WindowId::Options), "and closes");
    explore_click(&mut h, &state, map);
    assert!(h.ui.windows.is_open(WindowId::Map), "the map opens");
    h.frame(&state);
    let r = h.ui.windows.map_picture.expect("the map's picture");
    // A pan sends nothing.
    h.move_to(r.x + r.w * 0.4, r.y + r.h * 0.4);
    h.frame(&state);
    h.press();
    h.frame(&state);
    h.move_to(r.x + r.w * 0.5, r.y + r.h * 0.5);
    h.frame(&state);
    h.release();
    assert!(h.frame(&state).explore.is_empty(), "a drag pans");
    // A still click goes where it was made, once no second click has followed it.
    let at = (r.x + r.w * 0.3, r.y + r.h * 0.6);
    let mut asks = explore_click(&mut h, &state, at);
    assert!(asks.is_empty(), "not at once: {asks:?}");
    for _ in 0..40 {
        asks.extend(h.frame(&state).explore);
    }
    let [Ask::Go(ns, ew)] = asks.as_slice() else {
        panic!("one go: {asks:?}");
    };
    let shown = h.ui.windows.map_view.shown();
    let pixel = map_pixel(*ns, *ew);
    let back = h.ui.windows.map_view.on_screen(r, pixel);
    assert!(
        (back.0 - at.0).abs() < 2.0 && (back.1 - at.1).abs() < 2.0,
        "{back:?} {at:?} in {shown:?}"
    );
    // A player's map in the world goes nowhere.
    let (mut h, middle) = {
        let state = player();
        let mut h = Harness::with_built_in_art();
        h.frame(&state);
        h.ui.windows.open(WindowId::Map, 0.0);
        h.frame(&state);
        let r = h.ui.windows.map_picture.expect("the map's picture");
        (h, (r.x + r.w / 2.0, r.y + r.h / 2.0))
    };
    assert!(explore_click(&mut h, &player(), middle).is_empty());
}

/// A double click at `at`, the second press a fifth of a second after the first.
fn double_click(
    h: &mut Harness,
    state: &GameState,
    at: (f32, f32),
) -> Vec<pyreal_client_runtime::explore::Ask> {
    h.move_to(at.0, at.1);
    let mut asks = h.frame(state).explore;
    for t in [1000, 1200] {
        h.input.note_left_press(t);
        h.press();
        asks.extend(h.frame(state).explore);
        h.release();
        asks.extend(h.frame(state).explore);
    }
    asks
}

#[test]
fn a_double_click_on_the_map_sets_the_waypoint_there_and_explore_mode_goes_nowhere() {
    for state in [exploring(), player()] {
        let mut h = Harness::with_built_in_art();
        h.frame(&state);
        h.ui.windows.open(WindowId::Map, 0.0);
        h.frame(&state);
        let r = h.ui.windows.map_picture.expect("the map's picture");
        let at = (r.x + r.w * 0.3, r.y + r.h * 0.6);
        assert!(h.ui.windows.waypoints.active.is_none());
        let mut asks = double_click(&mut h, &state, at);
        for _ in 0..40 {
            asks.extend(h.frame(&state).explore);
        }
        assert!(asks.is_empty(), "a double click goes nowhere: {asks:?}");
        let w = h.ui.windows.waypoints.active.clone().expect("a waypoint");
        let back = h.ui.windows.map_view.on_screen(r, map_pixel(w.ns, w.ew));
        assert!(
            (back.0 - at.0).abs() < 2.0 && (back.1 - at.1).abs() < 2.0,
            "{back:?} {at:?}"
        );
        assert_eq!(w.name, None, "no place under the pointer");
    }
}

/// Under the picture every control and the player's coordinates stand inside the window and clear
/// of each other, for a player and for an administrator with Places and Teleport, at the window's
/// own size and at a larger interface scale.
#[test]
fn the_controls_under_the_map_never_overlap_for_a_player_or_an_administrator() {
    let art = built_in();
    for scale in [1.0, 1.5] {
        for admin in [false, true] {
            let mut state = if admin { administrator() } else { player() };
            state.coords = Some((42.3, -33.3));
            let mut h = Harness {
                ui: HorizonUi::new(
                    Arc::clone(&art),
                    HorizonOptions {
                        scale: Some(scale),
                        ..Default::default()
                    },
                ),
                list: DrawList::default(),
                input: InputFrame::default(),
                screen: (1920.0, 1080.0),
            };
            h.frame(&state);
            h.ui.windows.open(WindowId::Map, 0.0);
            h.ui.windows
                .waypoints
                .set(pyreal_horizon::ui::waypoint::Waypoint {
                    ns: 42.3,
                    ew: -33.3,
                    name: Some("The well".into()),
                });
            h.frame(&state);
            h.frame(&state);
            let window =
                h.ui.windows
                    .rects(scale)
                    .into_iter()
                    .find(|(id, _)| *id == WindowId::Map)
                    .map(|(_, r)| r)
                    .expect("the map window");
            let bar = h.ui.windows.map_view.bar.clone();
            let names: Vec<&str> = bar.iter().map(|(n, _)| *n).collect();
            let mut want = vec![
                "Zoom Out",
                "Zoom In",
                "Fit",
                "Coordinates",
                "Journal pages",
                "Add Waypoint to Journal",
            ];
            if admin {
                want.extend(["Places", "Teleport"]);
            }
            for w in &want {
                assert!(
                    names.contains(w),
                    "{w} at {scale} (admin {admin}): {names:?}"
                );
            }
            for (i, (a, ra)) in bar.iter().enumerate() {
                assert!(
                    ra.x >= window.x
                        && ra.right() <= window.right()
                        && ra.bottom() <= window.bottom(),
                    "{a} {ra:?} outside {window:?} at {scale} (admin {admin})"
                );
                for (b, rb) in &bar[i + 1..] {
                    assert!(
                        ra.intersect(rb).is_none(),
                        "{a} {ra:?} overlaps {b} {rb:?} at {scale} (admin {admin})"
                    );
                }
            }
        }
    }
}

/// A press on the picture let go where it began, without the pause before a double click.
fn single_click(h: &mut Harness, state: &GameState, at: (f32, f32)) {
    h.move_to(at.0, at.1);
    h.frame(state);
    h.input.note_left_press(10_000);
    h.press();
    h.frame(state);
    h.release();
    h.frame(state);
}

/// A single click on the map offers Create Waypoint beside the place clicked, for a player and for
/// an administrator, and choosing it sets the waypoint there; an administrator's teleport, once
/// armed, offers nothing.
#[test]
fn a_single_click_on_the_map_offers_create_waypoint_there_for_a_player_or_an_administrator() {
    for state in [player(), administrator()] {
        let mut h = Harness::with_built_in_art();
        h.frame(&state);
        h.ui.windows.open(WindowId::Map, 0.0);
        h.frame(&state);
        let r = h.ui.windows.map_picture.expect("the map's picture");
        let at = (r.x + r.w * 0.3, r.y + r.h * 0.6);
        single_click(&mut h, &state, at);
        let (place, name) = h.ui.windows.map_view.offer.clone().expect("an offer");
        assert_eq!(name, None, "no place under the pointer");
        let back =
            h.ui.windows
                .map_view
                .on_screen(r, map_pixel(place.0, place.1));
        assert!((back.0 - at.0).abs() < 2.0 && (back.1 - at.1).abs() < 2.0);
        assert!(h.ui.windows.waypoints.active.is_none(), "only offered");
        let button = h.ui.windows.map_view.offer_button.expect("Create Waypoint");
        assert!(r.contains(button.x, button.y) && r.contains(button.right(), button.bottom()));
        assert!(
            !button.contains(at.0, at.1),
            "beside the place, clear of a second click"
        );
        click(&mut h, &state, middle(button));
        let w = h.ui.windows.waypoints.active.clone().expect("a waypoint");
        assert_eq!((w.ns, w.ew), place);
        assert!(h.ui.windows.map_view.offer.is_none());
        if state.is_admin {
            let teleport = h.ui.windows.admin.map_button.expect("Teleport");
            click(&mut h, &state, middle(teleport));
            assert!(h.ui.windows.admin.map_armed);
            single_click(&mut h, &state, at);
            assert!(
                h.ui.windows.map_view.offer.is_none(),
                "the teleport's click"
            );
        }
    }
}

/// A drag pans and offers nothing, and a press elsewhere in the window lets an offer go.
#[test]
fn a_drag_offers_nothing_and_a_press_elsewhere_lets_create_waypoint_go() {
    let state = player();
    let mut h = Harness::with_built_in_art();
    h.frame(&state);
    h.ui.windows.open(WindowId::Map, 0.0);
    h.frame(&state);
    let r = h.ui.windows.map_picture.expect("the map's picture");
    wheel(&mut h, &state, middle(r), 4.0);
    let from = middle(r);
    let was = h.ui.windows.map_view.centre;
    h.move_to(from.0, from.1);
    h.press();
    h.frame(&state);
    h.move_to(from.0 + 40.0, from.1 + 20.0);
    h.frame(&state);
    h.release();
    h.frame(&state);
    assert_ne!(h.ui.windows.map_view.centre, was, "the drag pans");
    assert!(
        h.ui.windows.map_view.offer.is_none(),
        "a drag offers nothing"
    );
    single_click(&mut h, &state, (r.x + r.w * 0.3, r.y + r.h * 0.6));
    assert!(h.ui.windows.map_view.offer.is_some());
    let fit =
        h.ui.windows
            .map_view
            .bar
            .iter()
            .find(|(n, _)| *n == "Journal pages");
    let toggle = fit.expect("the journal's checkbox").1;
    click(&mut h, &state, middle(toggle));
    assert!(h.ui.windows.map_view.offer.is_none());
}
