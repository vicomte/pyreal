//! The skins plugin windows are drawn in, with pictures standing in for the game's data: each
//! part of a window drawn from the skin's own pieces where the data has all of that part's
//! pictures, and plainly where it has not.

use pyreal_plugin_api::Value;
use pyreal_ui_kit::art::{Art, Image, Sprite};
use pyreal_ui_kit::draw::DrawList;

use crate::kit_tests::{node, view, window, Harness};
use crate::skin;

/// A picture for every image the data could hold, one colour drawn from its id: eight pixels
/// square, or as large as the data's own where a skin cuts parts out of it.
fn every_picture(id: u32) -> Option<Image> {
    const CUT_FROM: &[(u32, u32, u32)] = &[
        (0x0600_5E92, 150, 18),
        (0x0600_1207, 300, 36),
        (0x0600_1208, 300, 36),
        (0x0600_120A, 300, 36),
        (0x0600_128F, 138, 25),
        (0x0600_1291, 138, 25),
        (0x0600_127B, 276, 30),
        (0x0600_1287, 120, 18),
        (0x0600_1263, 16, 16),
        (0x0600_11B9, 32, 3),
    ];
    let (w, h) = CUT_FROM
        .iter()
        .find(|c| c.0 == id)
        .map_or((8, 8), |c| (c.1, c.2));
    let [b0, b1, ..] = id.to_le_bytes();
    Some(Image::solid(w, h, [b0, b1, 0x80, 0xFF]))
}

/// A window with a control of every kind a skin draws a piece for.
fn every_control() -> crate::WindowView {
    let rows: Vec<Value> = (1..=20)
        .map(|i| Value::List(vec![Value::from(format!("row {i}"))]))
        .collect();
    view(
        window(
            "main",
            vec![
                node("button").with("id", "go").with("text", "Go"),
                node("toggle")
                    .with("id", "auto")
                    .with("text", "Auto")
                    .with("value", true),
                node("separator"),
                node("list")
                    .with("id", "log")
                    .with("rows", rows)
                    .with("height", 4_i64)
                    .with("selected", 1_i64),
                node("choice")
                    .with("id", "mode")
                    .with("text", "Mode")
                    .with("options", vec![Value::from("stop"), Value::from("run")])
                    .with("value", "stop"),
                node("slider")
                    .with("id", "range")
                    .with("text", "Range")
                    .with("value", 5_i64)
                    .with("min", 0_i64)
                    .with("max", 10_i64),
                node("tabs").with("selected", 1_i64).with(
                    "children",
                    vec![
                        node("group")
                            .with("title", "One")
                            .with("children", vec![node("label").with("text", "a")]),
                        node("group")
                            .with("title", "Two")
                            .with("children", vec![node("label").with("text", "b")]),
                    ],
                ),
            ],
        ),
        1,
    )
}

/// The window's draw list, two frames in.
fn drawn(art: Art) -> (Harness, DrawList) {
    let views = [every_control()];
    let mut h = Harness::with_art(art);
    h.frame(&views);
    h.frame(&views);
    let list = h.drawn().windows.first().expect("the window").2.clone();
    (h, list)
}

/// Whether a quad of `list` draws part of piece `s`.
fn draws(list: &DrawList, s: &Sprite) -> bool {
    list.quads.iter().any(|q| {
        q.tex == Some(s.tex)
            && q.src.x >= s.src.x - 0.01
            && q.src.y >= s.src.y - 0.01
            && q.src.right() <= s.src.right() + 0.01
            && q.src.bottom() <= s.src.bottom() + 0.01
    })
}

/// Each piece in `names` that `art` has is drawn somewhere in `list`; the names drawn.
fn drawn_pieces(art: &Art, list: &DrawList, names: &[&str]) -> Vec<String> {
    names
        .iter()
        .filter(|n| art.piece(n, 1.0).is_some_and(|s| draws(list, &s)))
        .map(|n| (*n).to_owned())
        .collect()
}

/// The pieces a window of every control draws, by name.
const WINDOW_PIECES: &[&str] = &[
    "window.fill",
    "window.tl",
    "window.left",
    "window.br",
    "window.title.mid",
    "window.close.normal",
    "button.normal.left",
    "button.normal.right",
    "control.box-on",
    "rule",
    "list.selected",
    "scroll.track-mid",
    "scroll.thumb-mid",
    "field.mid",
    "field.arrow",
    "slider.track",
    "slider.knob",
    "tab.selected.mid",
    "tab.unselected.mid",
    "plate.fill",
    "plate.rule",
];

/// Under the retail skin, every part of a window the data has all the pictures of is drawn from
/// them: the frame, its title plate and close button, a button, a ticked box, a divider, a list's
/// chosen row and its scrollbar, a choice's field and arrow, a slider, tabs and a list's plain
/// box.
#[test]
fn the_retail_skin_draws_each_part_of_a_window_from_the_game_s_own_pictures() {
    let (h, list) = drawn(skin::retail(None, None, &every_picture));
    let drawn = drawn_pieces(h.art(), &list, WINDOW_PIECES);
    let missing: Vec<&&str> = WINDOW_PIECES
        .iter()
        .filter(|n| !drawn.iter().any(|d| d == **n))
        .collect();
    assert!(missing.is_empty(), "not drawn from the pieces: {missing:?}");
}

/// A part whose pictures are not all in the data is drawn plainly in the skin's colours, and
/// every other part still from its pieces: with the button's pictures gone, the button is the
/// plain face, the frame the game's.
#[test]
fn a_part_missing_a_picture_is_drawn_plainly_and_the_others_from_their_pieces() {
    let button = [0x0600_4C94_u32, 0x0600_4C97, 0x0600_4C9A];
    let load = |id: u32| {
        if button.contains(&id) {
            None
        } else {
            every_picture(id)
        }
    };
    let (h, list) = drawn(skin::retail(None, None, &load));
    let art = h.art();
    assert!(
        !art.has_piece("button.normal.left") && !art.has_piece("button.disabled.mid"),
        "a part is taken whole or not at all"
    );
    let drawn = drawn_pieces(art, &list, WINDOW_PIECES);
    assert!(drawn.iter().any(|d| d == "window.tl"), "{drawn:?}");
    assert!(drawn.iter().any(|d| d == "control.box-on"), "{drawn:?}");
    let face = art.palette().button;
    assert!(
        list.quads
            .iter()
            .any(|q| q.tex.is_none() && q.colour == face && q.dst.w > 10.0),
        "the button is the plain face"
    );
}

/// With none of its pictures in the data the retail skin is the retail plain skin: no piece,
/// the plain palette, the plain frame's rule round the window.
#[test]
fn with_no_pictures_the_retail_skin_draws_as_the_plain_one() {
    let (h, list) = drawn(skin::retail(None, None, &|_| None));
    let art = h.art();
    assert!(WINDOW_PIECES.iter().all(|n| !art.has_piece(n)));
    let plain = pyreal_ui_kit::palette::Palette::retail_plain();
    assert_eq!(*art.palette(), plain);
    assert!(
        list.quads
            .iter()
            .any(|q| q.tex.is_none() && q.colour == plain.panel_border),
        "the plain frame's rule"
    );
}

/// The pieces are a skin's only where the part is whole: a part cut from outside its picture
/// is no part.
#[test]
fn a_part_cut_from_outside_its_picture_is_left_out() {
    let small = |_: u32| Some(Image::solid(8, 8, [1, 2, 3, 255]));
    let pieces = skin::pieces(skin::RETAIL, &small);
    assert!(pieces.has("window.tl"), "a whole picture is cut whole");
    assert!(
        !pieces.has("field.arrow"),
        "the arrow lies past an 8 by 8 picture"
    );
}
