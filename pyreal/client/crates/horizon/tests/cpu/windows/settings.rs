//! The Display page of the settings ends with the experimental rendering effects' boxes and their
//! Depth Mapping menu under a heading of their own: a box another box's choice rules out leaves the
//! page, one the device rules out is greyed and cannot be ticked, and the rest tick as any other.

use pyreal_client_contract::options::fidelity::{self, Availability};
use pyreal_client_contract::options::names::fidelity as names;
use pyreal_client_contract::options::store;
use pyreal_client_contract::view::PrefValue;
use pyreal_client_contract::UiRequest;
use pyreal_horizon::ui::game::GameState;
use pyreal_horizon::ui::panels::options::tab;
use pyreal_horizon::ui::panels::WindowId;

use crate::Harness;

/// The Display page, scrolled to its end, with the device `hifi` reports, an art pack open and
/// every box off.
fn client_page(hifi: Availability) -> (Harness, GameState) {
    store::init();
    for o in &fidelity::OPTIONS {
        store::set_value(o.name, PrefValue::Bool(false));
    }
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        hifi,
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.ui.windows.open(WindowId::Options, 0.0);
    h.ui.windows.options_page.tab = tab::DISPLAY;
    h.frame(&state);
    h.ui.windows.options_page.scroll[tab::DISPLAY] = 1.0e6;
    h.frame(&state);
    h.frame(&state);
    (h, state)
}

/// Scroll the Client page so the row captioned `caption` shows whole in its list, from the page's
/// top down a few pixels at a time; whether it was found. The effects' rows are more than the
/// list shows at once.
fn show(h: &mut Harness, state: &GameState, caption: &str) -> bool {
    let whole = |h: &Harness| {
        let page = &h.ui.windows.options_page;
        match (page.control("list"), page.control(caption)) {
            (Some(list), Some(r)) => r.y >= list.y && r.bottom() <= list.bottom(),
            _ => false,
        }
    };
    if whole(h) {
        return true;
    }
    h.ui.windows.options_page.scroll[tab::DISPLAY] = 0.0;
    h.frame(state);
    for _ in 0..2000 {
        if whole(h) {
            return true;
        }
        h.ui.windows.options_page.scroll[tab::DISPLAY] += 6.0;
        h.frame(state);
    }
    false
}

/// Click the box of the row captioned `caption`, and what the click asked the game for.
fn tick(h: &mut Harness, state: &GameState, caption: &str) -> Vec<UiRequest> {
    assert!(show(h, state, caption), "no {caption} row");
    let r =
        h.ui.windows
            .options_page
            .control(caption)
            .unwrap_or_else(|| panic!("no {caption} row"));
    h.move_to(r.x + 10.0, r.y + r.h / 2.0);
    h.press();
    let mut out = h.frame(state).requests;
    h.release();
    out.extend(h.frame(state).requests);
    out
}

fn sets(requests: &[UiRequest], name: &str) -> Option<PrefValue> {
    requests.iter().find_map(|r| match r {
        UiRequest::SetPreference(n, v) if *n == name => Some(v.clone()),
        _ => None,
    })
}

fn caption(name: &str) -> &'static str {
    fidelity::find(name).expect("a box").caption
}

/// Whether the page has a row captioned `caption`, wherever it is scrolled to.
fn on_the_page(h: &mut Harness, state: &GameState, caption: &str) -> bool {
    show(h, state, caption)
}

const READY: Availability = Availability {
    built: true,
    widened: true,
    supported: true,
    rays: Some(false),
    active: false,
    failed: None,
};

#[test]
fn a_new_player_starts_with_the_effects_on() {
    store::init();
    for o in &fidelity::OPTIONS {
        assert_eq!(
            store::inq_value(o.name),
            Some(PrefValue::Bool(o.on_by_default)),
            "{}",
            o.name
        );
    }
}

#[test]
fn every_box_is_on_the_page_and_ticks() {
    let (mut h, state) = client_page(READY);
    for o in &fidelity::OPTIONS {
        assert!(on_the_page(&mut h, &state, o.caption), "{}", o.caption);
    }
    // What is always drawn has no box.
    for gone in [
        "Dynamic Lights",
        "Atmospheric Sky",
        "Water Reflections",
        "Slope Materials",
    ] {
        assert!(!on_the_page(&mut h, &state, gone), "{gone}");
    }
    let asked = tick(&mut h, &state, caption(names::AMBIENT_OCCLUSION));
    assert_eq!(
        sets(&asked, names::AMBIENT_OCCLUSION),
        Some(PrefValue::Bool(true))
    );
    let asked = tick(&mut h, &state, caption(names::SHADOWS));
    assert_eq!(sets(&asked, names::SHADOWS), Some(PrefValue::Bool(true)));
    let asked = tick(&mut h, &state, caption(names::WEATHER));
    assert_eq!(sets(&asked, names::WEATHER), Some(PrefValue::Bool(true)));
    // The bounced light ticked leaves the occlusion's box free: indoors it is the only effect.
    let asked = tick(&mut h, &state, caption(names::GLOBAL_ILLUMINATION));
    assert_eq!(
        sets(&asked, names::GLOBAL_ILLUMINATION),
        Some(PrefValue::Bool(true))
    );
    let asked = tick(&mut h, &state, caption(names::AMBIENT_OCCLUSION));
    assert_eq!(
        sets(&asked, names::AMBIENT_OCCLUSION),
        Some(PrefValue::Bool(false))
    );
}

/// God Rays starts at Low, and is on the page.
#[test]
fn god_rays_start_low_and_are_on_the_page() {
    let rays = pyreal_client_contract::options::god_rays::CAPTION;
    let (mut h, state) = client_page(READY);
    assert_eq!(store::inq_value(names::GOD_RAYS), Some(PrefValue::Int(1)));
    assert!(on_the_page(&mut h, &state, rays));
}

/// Render Scale starts at Dynamic and is on the page; the Dynamic Resolution Frame Rate shows only
/// while it is Dynamic; and Anti-Aliasing stays on the page at every scale.
#[test]
fn render_scale_shows_its_frame_rate_only_while_dynamic_and_keeps_anti_aliasing() {
    use pyreal_client_contract::options::dynamic_resolution;
    let (mut h, state) = client_page(READY);
    assert_eq!(
        store::inq_value(names::RENDER_SCALE),
        Some(PrefValue::Int(1))
    );
    assert!(on_the_page(&mut h, &state, fidelity::SCALE_CAPTION));
    assert!(on_the_page(&mut h, &state, dynamic_resolution::CAPTION));
    for share in [50, 100] {
        store::set_value(names::RENDER_SCALE, PrefValue::Int(share));
        h.frame(&state);
        assert!(
            !on_the_page(&mut h, &state, dynamic_resolution::CAPTION),
            "{share}"
        );
        assert!(
            on_the_page(&mut h, &state, caption(names::NO_ANTI_ALIASING)),
            "{share}"
        );
    }
}

/// Open the Quality Preset menu's list: its names, and the one the box shows. The list is closed
/// again with that one chosen, which changes nothing.
fn presets_shown(h: &mut Harness, state: &GameState) -> (Vec<String>, usize) {
    assert!(
        show(h, state, fidelity::PRESET_CAPTION),
        "no Quality Preset row"
    );
    let r =
        h.ui.windows
            .options_page
            .control(fidelity::PRESET_CAPTION)
            .expect("its box");
    h.move_to(r.x + 10.0, r.y + r.h / 2.0);
    h.press();
    h.frame(state);
    h.release();
    h.frame(state);
    let menu = h.ui.windows.options_page.menu.as_ref().expect("its list");
    let shown = (menu.names.clone(), menu.at);
    h.ui.windows.options_page.choose_in_open_list(shown.1);
    h.frame(state);
    shown
}

#[test]
fn a_quality_preset_sets_its_values_and_shows_until_a_box_changes() {
    let (mut h, state) = client_page(READY);
    // Every box off is no preset's.
    let (shown, at) = presets_shown(&mut h, &state);
    assert_eq!(shown, ["Low", "Medium", "High", "Ultra", "Custom"]);
    assert_eq!(at, 4);
    // High chosen: what it sets is set, and it is shown, though Medium sets the same.
    let r =
        h.ui.windows
            .options_page
            .control(fidelity::PRESET_CAPTION)
            .expect("its box");
    h.move_to(r.x + 10.0, r.y + r.h / 2.0);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    h.ui.windows.options_page.choose_in_open_list(2);
    let asked = h.frame(&state).requests;
    for (name, value) in fidelity::PRESETS[2].values() {
        assert_eq!(sets(&asked, name), Some(value), "{name}");
    }
    let (shown, at) = presets_shown(&mut h, &state);
    assert_eq!(shown, ["Low", "Medium", "High", "Ultra"]);
    assert_eq!(at, 2);
    // A box of the preset's changed: the player's own settings.
    let asked = tick(&mut h, &state, caption(names::WEATHER));
    assert_eq!(sets(&asked, names::WEATHER), Some(PrefValue::Bool(false)));
    assert_eq!(presets_shown(&mut h, &state).1, 4);
}

#[test]
fn anti_aliasing_and_wind_are_ticked_while_their_preferences_are_off() {
    let (mut h, state) = client_page(READY);
    for name in [names::NO_ANTI_ALIASING, names::NO_WIND] {
        store::set_value(name, PrefValue::Bool(false));
        h.frame(&state);
        assert_eq!(store::inq_value(name), Some(PrefValue::Bool(false)));
        let asked = tick(&mut h, &state, caption(name));
        assert_eq!(
            sets(&asked, name),
            Some(PrefValue::Bool(true)),
            "unticked: off"
        );
        h.frame(&state);
        let asked = tick(&mut h, &state, caption(name));
        assert_eq!(
            sets(&asked, name),
            Some(PrefValue::Bool(false)),
            "ticked: on"
        );
    }
}

/// The status lines the page drew in its last frame.
fn status(h: &Harness) -> Vec<String> {
    h.ui.windows
        .options_page
        .controls
        .iter()
        .map(|(l, _)| l.clone())
        .filter(|l| l.starts_with("Status: "))
        .collect()
}

/// Started without the effects there is nothing to say until a box is ticked, every box ticks,
/// and the status line says the restart applies them; after it they draw, and it says nothing.
#[test]
fn started_without_the_effects_a_tick_says_the_restart_applies_it() {
    let (mut h, mut state) = client_page(Availability {
        built: true,
        widened: false,
        supported: true,
        rays: Some(true),
        active: false,
        failed: None,
    });
    h.frame(&state);
    assert!(status(&h).is_empty(), "{:?}", status(&h));
    let asked = tick(&mut h, &state, caption(names::WEATHER));
    assert_eq!(sets(&asked, names::WEATHER), Some(PrefValue::Bool(true)));
    let asked = tick(&mut h, &state, caption(names::SHADOWS));
    assert_eq!(sets(&asked, names::SHADOWS), Some(PrefValue::Bool(true)));
    h.frame(&state);
    // The status line stands under the heading: the page scrolled up to it.
    for _ in 0..200 {
        if !status(&h).is_empty() {
            break;
        }
        h.ui.windows.options_page.scroll[tab::DISPLAY] -= 10.0;
        h.frame(&state);
    }
    assert_eq!(status(&h), ["Status: Restart to apply"]);
    // Restarted with a box ticked: the device is asked for them, and they draw.
    state.hifi = Availability {
        widened: true,
        active: true,
        ..state.hifi.clone()
    };
    h.frame(&state);
    assert!(status(&h).is_empty(), "{:?}", status(&h));
}

/// An effect's note and its cost, shown while the pointer is over its caption, are drawn over the
/// rows under it: after every row of the page, and not cut to the page's list.
#[test]
fn an_effect_s_note_is_drawn_over_every_row_of_the_page() {
    let (mut h, state) = client_page(Availability {
        built: true,
        widened: true,
        supported: true,
        rays: Some(false),
        active: true,
        failed: None,
    });
    // The first box, with the others in the rows under it.
    let o = &fidelity::OPTIONS[0];
    assert!(show(&mut h, &state, o.caption), "the first effect's box");
    let page = &h.ui.windows.options_page;
    let list = page.control("list").expect("the page's list");
    let r = page.control(o.caption).expect("the first effect's box");
    let (x, y) = (list.x + 40.0, r.y + r.h / 2.0);
    h.move_to(x, y);
    h.frame(&state);
    let (title, lines) = h.ui.windows.tip().expect("the note");
    assert_eq!(title, o.caption);
    assert_eq!(lines[0], o.note);
    assert!(lines.last().is_some_and(|l| l.starts_with("Performance: ")));
    // Its ground stands beside the pointer.
    let at = h
        .list
        .quads
        .iter()
        .position(|q| {
            q.tex.is_none()
                && (q.dst.x - (x + 18.0)).abs() < 0.5
                && (q.dst.y - (y + 18.0)).abs() < 0.5
        })
        .expect("the note's ground");
    h.assert_over_rows(at, list, "the note");
}

/// An effect's row the list shows only in part shows its note only over the part shown: the
/// pointer under the list, over the Defaults button, shows none.
#[test]
fn an_effect_s_note_shows_only_over_the_part_of_its_row_the_list_shows() {
    let (mut h, state) = client_page(Availability {
        built: true,
        widened: true,
        supported: true,
        rays: Some(false),
        active: true,
        failed: None,
    });
    let o = &fidelity::OPTIONS[0];
    assert!(show(&mut h, &state, o.caption), "the first effect's box");
    let page = &h.ui.windows.options_page;
    let list = page.control("list").expect("the page's list");
    let r = page.control(o.caption).expect("the first effect's box");
    // Scrolled back until the row's top is ten pixels above the list's foot.
    let row_top = r.y - 3.0;
    h.ui.windows.options_page.scroll[tab::DISPLAY] -= list.bottom() - 10.0 - row_top;
    h.frame(&state);
    let r =
        h.ui.windows
            .options_page
            .control(o.caption)
            .expect("the row, in part");
    assert!(
        r.y < list.bottom() && r.bottom() > list.bottom(),
        "{r:?} {list:?}"
    );
    h.move_to(list.x + 40.0, list.bottom() + 8.0);
    h.frame(&state);
    assert_eq!(h.ui.windows.tip(), None);
    h.move_to(list.x + 40.0, list.bottom() - 4.0);
    h.frame(&state);
    assert_eq!(h.ui.windows.tip().map(|(t, _)| t.as_str()), Some(o.caption));
}

/// The Sound tab, every sound on.
fn sound_tab() -> (Harness, GameState) {
    store::init();
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.ui.windows.open(WindowId::Options, 0.0);
    h.ui.windows.options_page.tab = tab::SOUND;
    h.frame(&state);
    h.frame(&state);
    (h, state)
}

/// The Sound tab has the Client page's sounds and nothing of the Display tab's, and the Display tab
/// none of its sounds.
#[test]
fn the_client_page_is_a_display_tab_and_a_sound_tab() {
    let (mut h, state) = sound_tab();
    let page = &h.ui.windows.options_page;
    for row in ["Ambient Sounds", "Interface Sounds"] {
        assert!(page.control(row).is_some(), "{row}");
    }
    for row in ["Full Screen", "Interface Scale"] {
        assert!(page.control(row).is_none(), "{row}");
    }
    h.ui.windows.options_page.tab = tab::DISPLAY;
    h.frame(&state);
    h.frame(&state);
    let page = &h.ui.windows.options_page;
    assert!(page.control("Interface Scale").is_some());
    assert!(page.control("Ambient Sounds").is_none());
}

/// Defaults on the Sound tab puts back its sounds alone, and on the Display tab everything else.
#[test]
fn each_tab_s_defaults_put_back_its_own_rows() {
    let (mut h, state) = sound_tab();
    let r =
        h.ui.windows
            .options_page
            .control("Defaults")
            .expect("Defaults");
    h.move_to(r.x + 10.0, r.y + r.h / 2.0);
    h.press();
    let mut out = h.frame(&state).requests;
    h.release();
    out.extend(h.frame(&state).requests);
    let named: Vec<&str> = out
        .iter()
        .filter_map(|r| match r {
            UiRequest::SetPreference(n, _) => Some(*n),
            _ => None,
        })
        .collect();
    assert!(named.iter().all(|n| n.starts_with("Sound.")), "{named:?}");
    assert!(!named.is_empty());
}

/// Weather Sounds is a row of the Sound tab, after Ambient Sounds, and its box turns
/// `Sound.WeatherSound` off.
#[test]
fn the_sound_tab_has_weather_sounds_after_ambient_sounds() {
    use pyreal_client_contract::options::weather_sound;
    let (mut h, state) = sound_tab();
    let page = &h.ui.windows.options_page;
    let ambient = page
        .control("Ambient Sounds")
        .expect("the Ambient Sounds row");
    let weather = page
        .control(weather_sound::CAPTION)
        .expect("the Weather Sounds row");
    let interface = page
        .control("Interface Sounds")
        .expect("the Interface Sounds row");
    assert!(ambient.y < weather.y && weather.y < interface.y);
    h.move_to(weather.x + 10.0, weather.y + weather.h / 2.0);
    h.press();
    let mut out = h.frame(&state).requests;
    h.release();
    out.extend(h.frame(&state).requests);
    assert_eq!(
        sets(&out, weather_sound::WEATHER_SOUND),
        Some(PrefValue::Bool(false))
    );
}

/// Not a claim: draws the Client page scrolled to the experimental effects, with the interface's
/// own art, as the device draws them, with the pointer over the last effect's caption, and with the
/// Quality Preset's list open, into the PNG files `PYREAL_HORIZON_SHOT` names (`<path>-page.png`,
/// `<path>-note.png` and `<path>-list.png`), for a person to look at. The quads are composited in
/// software: nearest texels, straight alpha.
#[test]
#[ignore = "an instrument: set PYREAL_HORIZON_SHOT to an output path without its extension"]
fn draw_the_effects_section_for_a_person_to_look_at() {
    use pyreal_horizon::art::Art;
    use pyreal_horizon::draw::DrawList;
    use pyreal_horizon::ui::input::InputFrame;
    use pyreal_horizon::ui::HorizonUi;
    use std::sync::Arc;
    /// Where the pointer is when the picture is taken.
    #[derive(Clone, Copy)]
    enum Pointer {
        Away,
        /// Over the last effect's caption, its note shown.
        OverNote,
        /// The Quality Preset's list opened.
        ListOpen,
    }
    let Some(path) = std::env::var_os("PYREAL_HORIZON_SHOT") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let hifi = Availability {
        built: true,
        widened: true,
        supported: true,
        rays: Some(false),
        active: true,
        failed: None,
    };
    for (suffix, pointer) in [
        ("page", Pointer::Away),
        ("note", Pointer::OverNote),
        ("list", Pointer::ListOpen),
    ] {
        store::init();
        store::set_value(names::SHADOWS, PrefValue::Bool(true));
        store::set_value(names::AMBIENT_OCCLUSION, PrefValue::Bool(true));
        let state = GameState {
            in_world: true,
            name: "Tester".into(),
            hifi: hifi.clone(),
            ..GameState::default()
        };
        let art = Arc::new(Art::new(Arc::new(
            pyreal_horizon::pieces::Pieces::built_in(),
        )));
        let mut h = Harness {
            ui: HorizonUi::new(Arc::clone(&art), Default::default()),
            list: DrawList::default(),
            input: InputFrame::default(),
            screen: (1920.0, 1080.0),
        };
        h.frame(&state);
        h.ui.windows.open(WindowId::Options, 0.0);
        h.ui.windows.options_page.tab = tab::DISPLAY;
        for _ in 0..3 {
            h.frame(&state);
        }
        h.ui.windows.options_page.scroll[tab::DISPLAY] = 1.0e6;
        for _ in 0..30 {
            h.frame(&state);
        }
        let page = &h.ui.windows.options_page;
        let list = page.control("list").expect("the page's list");
        match pointer {
            Pointer::Away => {}
            Pointer::OverNote => {
                let r = page
                    .control(fidelity::OPTIONS[fidelity::OPTIONS.len() - 1].caption)
                    .expect("the last effect's box");
                h.move_to(list.x + 120.0, r.y + r.h / 2.0);
            }
            Pointer::ListOpen => {
                assert!(
                    show(&mut h, &state, fidelity::PRESET_CAPTION),
                    "no Quality Preset row"
                );
                let r =
                    h.ui.windows
                        .options_page
                        .control(fidelity::PRESET_CAPTION)
                        .expect("its box");
                h.move_to(r.x + 10.0, r.y + r.h / 2.0);
                h.press();
                h.frame(&state);
                h.release();
            }
        }
        h.frame(&state);
        let (w, hgt) = (1920usize, 1080usize);
        let mut px = vec![[24.0f32, 26.0, 30.0]; w * hgt];
        for q in &h.list.quads {
            let image = q.tex.and_then(|t| art.image(t));
            let mut r = q.dst;
            if let Some(c) = q.clip {
                match r.intersect(&c) {
                    Some(v) => r = v,
                    None => continue,
                }
            }
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            for y in (r.y.max(0.0) as usize)..(r.bottom().min(hgt as f32).max(0.0) as usize) {
                for x in (r.x.max(0.0) as usize)..(r.right().min(w as f32).max(0.0) as usize) {
                    let ca = [
                        ((q.colour >> 16) & 0xFF) as f32 / 255.0,
                        ((q.colour >> 8) & 0xFF) as f32 / 255.0,
                        (q.colour & 0xFF) as f32 / 255.0,
                        ((q.colour >> 24) & 0xFF) as f32 / 255.0,
                    ];
                    let texel = match &image {
                        Some(img) => {
                            let u = q.src.x + (x as f32 + 0.5 - q.dst.x) / q.dst.w * q.src.w;
                            let v = q.src.y + (y as f32 + 0.5 - q.dst.y) / q.dst.h * q.src.h;
                            let tx = (u.max(0.0) as usize).min(img.width as usize - 1);
                            let ty = (v.max(0.0) as usize).min(img.height as usize - 1);
                            let i = (ty * img.width as usize + tx) * 4;
                            let b = &img.bgra[i..i + 4];
                            [
                                f32::from(b[2]) / 255.0,
                                f32::from(b[1]) / 255.0,
                                f32::from(b[0]) / 255.0,
                                f32::from(b[3]) / 255.0,
                            ]
                        }
                        None if q.tex.is_some() => continue,
                        None => [1.0, 1.0, 1.0, 1.0],
                    };
                    let a = texel[3] * ca[3];
                    let p = &mut px[y * w + x];
                    for k in 0..3 {
                        p[k] = p[k] * (1.0 - a) + 255.0 * texel[k] * ca[k] * a;
                    }
                }
            }
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let rgb: Vec<u8> = px
            .iter()
            .flat_map(|p| p.map(|c| c.clamp(0.0, 255.0) as u8))
            .collect();
        let out = path.with_file_name(format!(
            "{}-{suffix}.png",
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("horizon")
        ));
        let file = std::fs::File::create(&out).expect("the picture's file");
        #[allow(clippy::cast_possible_truncation)]
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, hgt as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut wr| wr.write_image_data(&rgb))
            .expect("the picture writes");
        eprintln!("wrote {}", out.display());
    }
}

/// Light Draw Distance starts at 1.5x, and is on the page.
#[test]
fn the_light_draw_distance_starts_at_one_and_a_half_and_is_on_the_page() {
    let lights = pyreal_client_contract::options::light_distance::CAPTION;
    let (mut h, state) = client_page(READY);
    assert_eq!(
        store::inq_value(names::LIGHT_DRAW_DISTANCE),
        Some(PrefValue::Int(150))
    );
    assert!(on_the_page(&mut h, &state, lights));
}
