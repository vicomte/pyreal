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
