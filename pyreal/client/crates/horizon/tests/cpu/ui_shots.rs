//! Not a claim: an instrument that draws the options window, every tab scrolled from top to bottom
//! and every dropdown list opened, with the interface's own art, into PNG files for a person to
//! look at.
//!
//! - `PYREAL_UI_SHOTS`: the folder the pictures go to (the test does nothing without it).
//! - `PYREAL_UI_SET`: preferences set first, `name=value` with `;` between (`true`, `false`, an
//!   integer or a number).
//! - `PYREAL_UI_TABS`: a comma list of the tab indices to draw (default all of them).
//! - `PYREAL_UI_MENUS`: more dropdowns to open, by caption, `;` between.
//! - `PYREAL_UI_LIST`: when set, prints every control's caption on each picture's page.
//! - `PYREAL_UI_WINDOW`: the window the Resolution row reads, `<w>x<h>/<usable w>x<usable h>`,
//!   with `/full` for full screen (default: the host says nothing).

use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use pyreal_client_contract::options::fidelity::Availability;
use pyreal_client_contract::options::sheet::{self, PageId, Value};
use pyreal_client_contract::options::store;
use pyreal_client_contract::view::PrefValue;
use pyreal_horizon::art::Art;
use pyreal_horizon::draw::DrawList;
use pyreal_horizon::ui::game::GameState;
use pyreal_horizon::ui::input::InputFrame;
use pyreal_horizon::ui::panels::WindowId;
use pyreal_horizon::ui::HorizonUi;

use crate::Harness;

/// The window's size.
const W: usize = 1920;
const H: usize = 1080;

/// The dropdowns of this interface's own that the options sheet doesn't list.
const OWN_MENUS: [&str; 3] = ["Interface Scale", "Status Bar Style", "Resolution"];

#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder"]
fn draw_every_options_tab_and_dropdown_for_a_person_to_look_at() {
    let Some(dir) = std::env::var_os("PYREAL_UI_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    store::init();
    for item in std::env::var("PYREAL_UI_SET")
        .unwrap_or_default()
        .split(';')
    {
        let Some((name, v)) = item.split_once('=') else {
            continue;
        };
        let v = v.trim();
        let value = match v {
            "true" => PrefValue::Bool(true),
            "false" => PrefValue::Bool(false),
            _ => v.parse::<i32>().map_or_else(
                |_| PrefValue::Float(v.parse::<f32>().expect("a preference's value")),
                PrefValue::Int,
            ),
        };
        let _ = store::set_value(name.trim(), value);
    }
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        hifi: Availability {
            built: true,
            widened: true,
            supported: true,
            rays: Some(true),
            active: true,
            ..Availability::default()
        },
        window_sizes: std::env::var("PYREAL_UI_WINDOW").ok().map(|v| {
            let size = |t: &str| {
                let (w, h) = t.split_once('x').expect("<w>x<h>");
                (w.parse().expect("a width"), h.parse().expect("a height"))
            };
            let mut parts = v.split('/');
            pyreal_client_contract::resolution::WindowSizes {
                size: size(parts.next().unwrap_or_default()),
                usable: parts.next().map(size),
                full_screen: parts.next() == Some("full"),
            }
        }),
        ..GameState::default()
    };
    let art = Arc::new(Art::new(Arc::new(
        pyreal_horizon::pieces::Pieces::built_in(),
    )));
    let mut h = Harness {
        ui: HorizonUi::new(Arc::clone(&art), Default::default()),
        list: DrawList::default(),
        input: InputFrame::default(),
        screen: (W as f32, H as f32),
    };
    let extra: Vec<String> = std::env::var("PYREAL_UI_MENUS")
        .unwrap_or_default()
        .split(';')
        .map(|m| m.trim().to_owned())
        .filter(|m| !m.is_empty())
        .collect();
    let menus: Vec<&str> = sheet::rows_for(PageId::Client)
        .filter(|r| matches!(r.value, Value::Menu(_)))
        .map(|r| r.caption)
        .chain(OWN_MENUS)
        .chain(extra.iter().map(String::as_str))
        .collect();
    h.frame(&state);
    h.ui.windows.open(WindowId::Options, 0.0);
    let tabs = h.ui.windows.options_page.scroll.len();
    let wanted: Vec<usize> = std::env::var("PYREAL_UI_TABS").map_or_else(
        |_| (0..tabs).collect(),
        |s| s.split(',').filter_map(|t| t.trim().parse().ok()).collect(),
    );
    let mut opened = Vec::new();
    for tab in wanted {
        h.ui.windows.options_page.tab = tab;
        h.ui.windows.options_page.scroll[tab] = 0.0;
        settle(&mut h, &state);
        let mut last = f32::NEG_INFINITY;
        for step in 0..16 {
            let scroll = h.ui.windows.options_page.scroll[tab];
            if scroll <= last {
                break;
            }
            last = scroll;
            write(&dir.join(format!("tab{tab}-{step:02}.png")), &h, &art);
            if std::env::var_os("PYREAL_UI_LIST").is_some() {
                let names: Vec<&str> =
                    h.ui.windows
                        .options_page
                        .controls
                        .iter()
                        .map(|(c, _)| c.as_str())
                        .collect();
                println!("tab{tab}-{step:02}: {}", names.join(" | "));
            }
            let Some(list) = h.ui.windows.options_page.control("list") else {
                break;
            };
            for caption in &menus {
                if opened.contains(caption) {
                    continue;
                }
                let Some(r) = h.ui.windows.options_page.control(caption) else {
                    continue;
                };
                if r.y < list.y || r.bottom() > list.bottom() {
                    continue;
                }
                h.move_to(r.x + 10.0, r.y + r.h / 2.0);
                h.press();
                h.frame(&state);
                h.release();
                h.frame(&state);
                if h.ui.windows.options_page.menu.is_some() {
                    let name: String = caption
                        .chars()
                        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                        .collect();
                    write(&dir.join(format!("tab{tab}-menu-{name}.png")), &h, &art);
                    opened.push(caption);
                }
                h.ui.windows.options_page.menu = None;
                h.move_to(0.0, 0.0);
                h.frame(&state);
            }
            h.ui.windows.options_page.scroll[tab] = scroll + list.h * 0.85;
            settle(&mut h, &state);
        }
        // The Chat page's other tabs, each chosen in its list.
        if tab == 1 {
            // The third tab popped out, as the list shows it.
            h.ui.hud.log.pop_out(2, true, 0.0);
            for slot in 1..5 {
                h.ui.windows.options_page.chat_tab = slot;
                h.ui.windows.options_page.scroll[tab] = 0.0;
                settle(&mut h, &state);
                write(
                    &dir.join(format!("tab1-chat-tab-{}.png", slot + 1)),
                    &h,
                    &art,
                );
            }
            h.ui.windows.options_page.chat_tab = 0;
        }
    }
}

#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder"]
fn draw_the_cross_hotbars_with_each_pad_family_for_a_person_to_look_at() {
    let Some(dir) = std::env::var_os("PYREAL_UI_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    for (name, family) in [
        ("xbox", pyreal_input::pad::PadFamily::Xbox),
        ("playstation", pyreal_input::pad::PadFamily::PlayStation),
    ] {
        let h = crate::hud::cross::marked(family, 30);
        write(&dir.join(format!("pad-{name}.png")), &h, &h.ui.art);
        let low = pyreal_horizon::draw::Rect::new(560.0, 700.0, 800.0, 380.0);
        write_part(
            &dir.join(format!("pad-{name}-bars.png")),
            &h,
            &h.ui.art,
            Some(low),
        );
    }
}

/// Frames enough for the window to fade in and a scroll to land.
fn settle(h: &mut Harness, state: &GameState) {
    for _ in 0..30 {
        h.frame(state);
    }
}

/// The frame's quads composited in software, nearest texels and straight alpha, as a PNG.
fn write(path: &Path, h: &Harness, art: &Art) {
    write_part(path, h, art, None);
}

/// [`write`], only the part of the screen `part` covers when given.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn write_part(path: &Path, h: &Harness, art: &Art, part: Option<pyreal_horizon::draw::Rect>) {
    let (sw, sh) = (h.screen.0 as usize, h.screen.1 as usize);
    // `PYREAL_UI_ALPHA`: the interface alone, see-through where it draws nothing, and its grounds
    // the world is blurred under beside it (`<name>.grounds.txt`, `x y w h strength` a line), for
    // `engine-view --ui --ui-grounds`.
    let alpha = std::env::var_os("PYREAL_UI_ALPHA").is_some();
    let mut px = vec![
        if alpha {
            [0.0f32; 3]
        } else {
            [24.0f32, 26.0, 30.0]
        };
        sw * sh
    ];
    let mut cover = vec![0.0f32; sw * sh];
    // `PYREAL_UI_BACKDROP`: an RGB picture of the window's size drawn under the interface.
    if let Some(back) = std::env::var_os("PYREAL_UI_BACKDROP") {
        let file = std::fs::File::open(back).expect("the backdrop");
        let mut reader = png::Decoder::new(std::io::BufReader::new(file))
            .read_info()
            .expect("a PNG backdrop");
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).expect("its pixels");
        let step = info.color_type.samples();
        for (p, c) in px.iter_mut().zip(buf.chunks(step)) {
            *p = [f32::from(c[0]), f32::from(c[1]), f32::from(c[2])];
        }
    }
    for q in &h.list.quads {
        let image = q.tex.and_then(|t| art.image(t));
        // A turned quad covers its corners' bounds; each pixel is turned back into its own.
        let (sin, cos) = (
            pyreal_primitives::num::math::sinf(q.turn),
            pyreal_primitives::num::math::cosf(q.turn),
        );
        let centre = (q.dst.x + q.dst.w / 2.0, q.dst.y + q.dst.h / 2.0);
        let reach = (q.dst.w.abs() * cos.abs() + q.dst.h.abs() * sin.abs()) / 2.0;
        let tall = (q.dst.w.abs() * sin.abs() + q.dst.h.abs() * cos.abs()) / 2.0;
        let mut r = if q.turn == 0.0 {
            q.dst
        } else {
            pyreal_horizon::draw::Rect::new(
                centre.0 - reach,
                centre.1 - tall,
                2.0 * reach,
                2.0 * tall,
            )
        };
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
        for y in (r.y.max(0.0) as usize)..(r.bottom().min(sh as f32).max(0.0) as usize) {
            for x in (r.x.max(0.0) as usize)..(r.right().min(sw as f32).max(0.0) as usize) {
                let ca = [
                    ((q.colour >> 16) & 0xFF) as f32 / 255.0,
                    ((q.colour >> 8) & 0xFF) as f32 / 255.0,
                    (q.colour & 0xFF) as f32 / 255.0,
                    ((q.colour >> 24) & 0xFF) as f32 / 255.0,
                ];
                let (dx, dy) = (x as f32 + 0.5 - centre.0, y as f32 + 0.5 - centre.1);
                let (fx, fy) = (
                    centre.0 + dx * cos + dy * sin,
                    centre.1 - dx * sin + dy * cos,
                );
                if q.turn != 0.0 && !q.dst.contains(fx, fy) {
                    continue;
                }
                let texel = match &image {
                    Some(img) => {
                        let u = q.src.x + (fx - q.dst.x) / q.dst.w * q.src.w;
                        let v = q.src.y + (fy - q.dst.y) / q.dst.h * q.src.h;
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
                let p = &mut px[y * sw + x];
                for k in 0..3 {
                    p[k] = p[k] * (1.0 - a) + 255.0 * texel[k] * ca[k] * a;
                }
                let c = &mut cover[y * sw + x];
                *c = *c * (1.0 - a) + a;
            }
        }
    }
    let (x0, y0, pw, ph) = part.map_or((0, 0, sw, sh), |r| {
        let x0 = (r.x.max(0.0) as usize).min(sw);
        let y0 = (r.y.max(0.0) as usize).min(sh);
        (
            x0,
            y0,
            (r.w as usize).min(sw - x0),
            (r.h as usize).min(sh - y0),
        )
    });
    if alpha {
        let grounds: String = h
            .list
            .backdrops
            .iter()
            .map(|(r, s)| format!("{} {} {} {} {s}\n", r.x, r.y, r.w, r.h))
            .collect();
        std::fs::write(path.with_extension("grounds.txt"), grounds).expect("the grounds' file");
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let rgb: Vec<u8> = (y0..y0 + ph)
        .flat_map(|y| (y * sw + x0..y * sw + x0 + pw).map(|i| (px[i], cover[i])))
        .flat_map(|(p, a)| {
            let c = p.map(|c| {
                let c = if alpha && a > 0.0 { c / a } else { c };
                c.clamp(0.0, 255.0) as u8
            });
            if alpha {
                vec![
                    c[0],
                    c[1],
                    c[2],
                    (a * 255.0).round().clamp(0.0, 255.0) as u8,
                ]
            } else {
                c.to_vec()
            }
        })
        .collect();
    let (sw, sh) = (pw, ph);
    let file = std::fs::File::create(path).expect("the picture's file");
    #[allow(clippy::cast_possible_truncation)]
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), sw as u32, sh as u32);
    enc.set_color(if alpha {
        png::ColorType::Rgba
    } else {
        png::ColorType::Rgb
    });
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(&rgb))
        .expect("the picture written");
}

/// Not a claim: the HUD with the character's vitals part spent, a party of three and the
/// fellowship window open, into `<PYREAL_UI_SHOTS>/<PYREAL_UI_HUD_NAME>.png` (default `hud`).
/// `PYREAL_UI_HUD_LONG` gives the health a ten-digit value, `PYREAL_UI_HUD_BIG` the vitals values
/// in the thousands. `PYREAL_UI_SET` sets preferences first.
/// `PYREAL_UI_HUD_SIZE` (`1280x720`), `PYREAL_UI_HUD_SCALE` (`1.5`) and `PYREAL_UI_HUD_ROWS` set the
/// window, the interface scale and the spell bar's rows; `PYREAL_UI_HUD_BARE` leaves the window shut
/// and `PYREAL_UI_HUD_MAGIC` or `PYREAL_UI_HUD_PEACE` puts the character in that stance, and
/// `PYREAL_UI_HUD_MODE` in the combat mode it names by number.
/// `PYREAL_UI_HUD_FELLOWS` puts that many members (1 to 9) in the fellowship window;
/// `PYREAL_UI_HUD_AI` makes the second and third AI Players with the character leading, and
/// `PYREAL_UI_HUD_OFFER` (`waypoint` or `note`) puts an Advocate's offer up. `PYREAL_UI_HUD_CLICK`
/// (`x,y`) clicks there first. `PYREAL_UI_HUD_ALONE` leaves the character in no fellowship, with
/// two AI Players following them.
/// `PYREAL_UI_HUD_LAYOUT` opens the HUD Layout window; `PYREAL_UI_HUD_KEYS` names the keys, and
/// `PYREAL_UI_HUD_SPELLS` puts that many spells in the first tab; `PYREAL_UI_HUD_STYLE` names
/// the status bar style by its settings word.
/// `PYREAL_UI_HUD_OPEN` opens `character` or `spellbook` in place of the fellowship window.
/// `PYREAL_UI_HUD_WAYPOINT` (`ns,ew`) sets a waypoint, the character standing at
/// `PYREAL_UI_HUD_HERE` (`ns,ew`, default `40.4,34.5`); `PYREAL_UI_HUD_CORPSE` (`ns,ew`) marks a
/// corpse there. `PYREAL_UI_HUD_TRACKER` pins a page, a contract and two quest timers to the
/// quest tracker with the rent owed; `journal` or `notebook` also opens that Journal tab.
/// `PYREAL_UI_HUD_EFFECTS` puts beneficial effects in force, four of them in their last minute,
/// and a harmful one; `PYREAL_UI_HUD_DATS` (a data folder) draws their spells' own icons.
#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder"]
fn draw_the_hud_with_vitals_and_a_fellowship_for_a_person_to_look_at() {
    use pyreal_client_contract::view::{FellowEntry, FellowshipView};
    use pyreal_horizon::ui::game::Vital;
    let Some(dir) = std::env::var_os("PYREAL_UI_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    store::init();
    for item in std::env::var("PYREAL_UI_SET")
        .unwrap_or_default()
        .split(';')
    {
        let Some((name, v)) = item.split_once('=') else {
            continue;
        };
        let v = v.trim();
        let value = match v {
            "true" => PrefValue::Bool(true),
            "false" => PrefValue::Bool(false),
            _ => v.parse::<i32>().map_or_else(
                |_| PrefValue::Float(v.parse::<f32>().expect("a preference's value")),
                PrefValue::Int,
            ),
        };
        let _ = store::set_value(name.trim(), value);
    }
    let long = std::env::var_os("PYREAL_UI_HUD_LONG").is_some();
    let big = std::env::var_os("PYREAL_UI_HUD_BIG").is_some();
    let vital = |current, max| Vital { current, max };
    let member =
        |id: u32, name: &str, level, hp: (u32, u32), sp: (u32, u32), mp: (u32, u32)| FellowEntry {
            id: pyreal_primitives::ObjectId(id),
            name: name.into(),
            level,
            xp_percent: 33,
            current_health: hp.0,
            max_health: hp.1,
            current_stamina: sp.0,
            max_stamina: sp.1,
            current_mana: mp.0,
            max_mana: mp.1,
        };
    let env = |name: &str| std::env::var(name).ok();
    // The keys as the key map names them, for the slots' corners: the shortcuts' with and
    // without Ctrl, the spell bar's first row's, and the later rows' as the layout gives them.
    let key_bindings = env("PYREAL_UI_HUD_KEYS").map(|_| {
        use pyreal_client_contract::actions::pyreal::USE_SPELL_SLOT_13;
        let numbers = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "="];
        let row = |action: u32, keys: Vec<String>| pyreal_horizon::ui::game::KeyRow {
            map: 0,
            action,
            group: 0,
            caption: String::new(),
            keys,
            changed: false,
        };
        let mut rows = Vec::new();
        for (i, n) in (0u32..).zip(numbers) {
            rows.push(row(0x1000_0042 + i, vec![n.into(), format!("CMD+{n}")]));
            rows.push(row(0x1000_0065 + i, vec![n.into()]));
            for (r, held) in (0u32..).zip(["ALT+", "SHIFT+", "CMD+ALT+", "ALT+SHIFT+"]) {
                rows.push(row(
                    USE_SPELL_SLOT_13.0 + r * 12 + i,
                    vec![format!("{held}{n}")],
                ));
            }
        }
        pyreal_horizon::ui::game::KeyBindingsView {
            rows,
            ..Default::default()
        }
    });
    // The fellowship window's members, 1 to 9.
    let fellows: usize = env("PYREAL_UI_HUD_FELLOWS")
        .and_then(|n| n.parse().ok())
        .unwrap_or(3)
        .clamp(1, 9);
    // A first spell tab of this many spells, for the spell bar's rows and its scroll.
    let spells: usize = env("PYREAL_UI_HUD_SPELLS")
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    let ai = env("PYREAL_UI_HUD_AI").is_some();
    let offer = env("PYREAL_UI_HUD_OFFER").map(|kind| {
        let (title, text) = if kind == "note" {
            (
                "Journal Note Offered",
                "~Advocate Tamsin offers you a note for your journal called Drudges.",
            )
        } else {
            (
                "Waypoint Offered",
                "~Advocate Tamsin offers you a waypoint to Holtburg square at 42.1N 33.6E.",
            )
        };
        pyreal_horizon::ui::game::Prompt {
            id: 1,
            text: text.into(),
            question: true,
            modal: false,
            waiting: 0,
            offer: Some(title.into()),
        }
    });
    let mut state = GameState {
        in_world: true,
        name: "Tester".into(),
        key_bindings,
        spell_tabs: vec![(1..=u32::try_from(spells).unwrap_or(0)).collect()],
        level: 60,
        total_xp: 1_234_567,
        xp_this: Some(40_000),
        xp_next: Some(100_000),
        combat_mode: if env("PYREAL_UI_HUD_MAGIC").is_some() {
            pyreal_client_contract::combat_mode::MAGIC
        } else if env("PYREAL_UI_HUD_PEACE").is_some() {
            pyreal_client_contract::combat_mode::NONCOMBAT
        } else {
            env("PYREAL_UI_HUD_MODE")
                .and_then(|m| m.parse().ok())
                .unwrap_or(pyreal_client_contract::combat_mode::UNDEF)
        },
        health: if long {
            vital(1_234_567_890, 2_000_000_000)
        } else if big {
            vital(123_456, 150_000)
        } else {
            vital(287, 350)
        },
        stamina: if big {
            vital(4_321, 5_000)
        } else {
            vital(410, 410)
        },
        mana: if big {
            vital(999, 1_200)
        } else {
            vital(125, 300)
        },
        fellowship: vec![
            (
                "Aldren".into(),
                vital(180, 240),
                vital(300, 300),
                vital(90, 200),
                false,
            ),
            (
                "Mirelle".into(),
                vital(60, 200),
                vital(150, 260),
                vital(240, 320),
                false,
            ),
        ],
        fellowship_view: Some(FellowshipView {
            name: "The Wanderers".into(),
            leader: pyreal_primitives::ObjectId(1),
            members: [
                member(1, "Tester", 60, (287, 350), (410, 410), (125, 300)),
                member(2, "Aldren", 58, (180, 240), (300, 300), (90, 200)),
                member(3, "Mirelle", 61, (60, 200), (150, 260), (240, 320)),
                member(4, "Corvin", 57, (200, 230), (280, 300), (60, 180)),
                member(5, "Isolde", 63, (250, 260), (320, 320), (300, 330)),
                member(6, "Brannoc", 55, (100, 220), (200, 280), (50, 150)),
                member(7, "Seraphine", 62, (210, 250), (260, 300), (280, 310)),
                member(8, "Torvald", 59, (230, 245), (290, 310), (70, 160)),
                member(9, "Wynna", 60, (190, 235), (240, 290), (220, 300)),
            ]
            .into_iter()
            .take(fellows)
            .collect(),
            ..FellowshipView::default()
        }),
        prompts: offer.into_iter().collect(),
        ..GameState::default()
    };
    let pair = |v: String| {
        let (a, b) = v.split_once(',')?;
        Some((a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?))
    };
    let waypoint = env("PYREAL_UI_HUD_WAYPOINT").and_then(pair);
    let corpse = env("PYREAL_UI_HUD_CORPSE").and_then(pair);
    if waypoint.is_some() || corpse.is_some() {
        let here = env("PYREAL_UI_HUD_HERE")
            .and_then(pair)
            .unwrap_or((40.4, 34.5));
        state.here = Some(here);
        #[allow(clippy::cast_possible_truncation)]
        let coords = (here.0 as f32, here.1 as f32);
        state.coords = Some(coords);
    }
    if env("PYREAL_UI_HUD_ALONE").is_some() {
        state.player_id = Some(pyreal_primitives::ObjectId(1));
        state.fellowship_view = None;
        state.fellowship.clear();
        state.ai_fellows = [(2, "~Tamsin", "defensive"), (3, "~Corvin", "aggressive")]
            .into_iter()
            .map(
                |(id, name, mode)| pyreal_client_contract::view::AiFellowView {
                    id: pyreal_primitives::ObjectId(id),
                    name: name.into(),
                    mode: mode.into(),
                    in_fellowship: false,
                },
            )
            .collect();
    }
    if ai {
        state.player_id = Some(pyreal_primitives::ObjectId(1));
        if let Some(f) = state.fellowship_view.as_mut() {
            for m in f.members.iter_mut().skip(1).take(2) {
                m.name = format!("~{}", m.name);
            }
        }
        for (name, ..) in state.fellowship.iter_mut().take(2) {
            *name = format!("~{name}");
        }
        state.ai_fellows = [(2, "~Aldren", "heal"), (3, "~Mirelle", "assist")]
            .into_iter()
            .take(fellows.saturating_sub(1))
            .map(
                |(id, name, mode)| pyreal_client_contract::view::AiFellowView {
                    id: pyreal_primitives::ObjectId(id),
                    name: name.into(),
                    mode: mode.into(),
                    in_fellowship: true,
                },
            )
            .collect();
    }
    // The spellbook's spells, every school and level shown.
    state.world_services.spell_filters = pyreal_client_contract::spellbook::DEFAULT_SPELL_FILTERS;
    let schools = [
        "",
        "Flame Bolt",
        "Heal Self",
        "Blood Drinker",
        "Strength Self",
    ];
    let levels = ["I", "II", "III", "IV", "V", "VI", "VII"];
    state.spells = (1u32..=4)
        .flat_map(|school| (1u32..=7).map(move |level| (school, level)))
        .map(|(school, level)| pyreal_horizon::ui::game::Spell {
            id: school * 100 + level,
            name: format!(
                "{} {}",
                schools[school as usize],
                levels[level as usize - 1]
            ),
            school,
            level,
            charge: None,
            ac_icon: 0,
            icon_power: 0,
            bitfield: 0,
        })
        .collect();
    if env("PYREAL_UI_HUD_EFFECTS").is_some() {
        state.effects = [
            ("Strength Self VI", false, 52.0),
            ("Armor Self VI", false, 37.0),
            ("Focus Self VI", false, 21.0),
            ("Regeneration Self VI", false, 7.0),
            ("Quickness Self VI", false, 1650.0),
            ("Imperil Other V", true, 140.0),
        ]
        .into_iter()
        .map(|(name, harmful, left)| pyreal_horizon::ui::game::Effect {
            spell: 0,
            layered: 0,
            name: name.into(),
            ac_icon: 0,
            harmful,
            remaining: Some(left),
            superseded: false,
        })
        .collect();
    }
    let art = Arc::new(Art::new(Arc::new(
        pyreal_horizon::pieces::Pieces::built_in(),
    )));
    // The spells' own icons, by name, from the data files' spell table.
    if let Some(dats) = env("PYREAL_UI_HUD_DATS") {
        use pyreal_assets::Decode as _;
        use pyreal_horizon::art::AcArt as _;
        use pyreal_primitives::AssetSource as _;
        let store = pyreal_dat::RetailDatStore::open_dir(std::path::Path::new(&dats))
            .expect("the data files open");
        let id = pyreal_primitives::DataId(0x0E00_000E);
        let table = pyreal_assets::tables::SpellTable::decode_payload(
            id,
            &store.read(id).expect("the spell table"),
        )
        .expect("the spell table decodes");
        for e in &mut state.effects {
            if let Some((spell, b)) = table.spells.iter().find(|(_, b)| b.name == e.name) {
                e.spell = *spell;
                e.ac_icon = b.icon;
            }
        }
        art.set_ac_store(Arc::new(store));
    }
    let size = env("PYREAL_UI_HUD_SIZE")
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?))
        })
        .unwrap_or((W as f32, H as f32));
    let options = pyreal_horizon::options::HorizonOptions {
        scale: env("PYREAL_UI_HUD_SCALE").and_then(|s| s.parse().ok()),
        status_style: env("PYREAL_UI_HUD_STYLE")
            .and_then(|w| pyreal_horizon::ui::hud::StatusStyle::from_word(&w))
            .unwrap_or_default(),
        spell_rows: env("PYREAL_UI_HUD_ROWS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1),
        ..Default::default()
    };
    let tracker = env("PYREAL_UI_HUD_TRACKER");
    if tracker.is_some() {
        tracker_state(&mut state);
    }
    let mut h = Harness {
        ui: HorizonUi::new(Arc::clone(&art), options),
        list: DrawList::default(),
        input: InputFrame::default(),
        screen: size,
    };
    h.frame(&state);
    if let Some(kind) = &tracker {
        use pyreal_horizon::ui::panels::tracker::Pin;
        let page = Pin::page(&state.journal.pages[0]).expect("a named page");
        for pin in [
            page,
            Pin::Contract(7),
            Pin::Quest("colosseum".into()),
            Pin::Heard("Quest from Ayn Tayan".into()),
        ] {
            h.ui.windows.tracker.pin(pin, &state);
        }
        if kind == "journal" || kind == "notebook" {
            use pyreal_horizon::ui::panels::JournalTab;
            h.ui.windows.open(WindowId::Journal, 0.0);
            h.ui.windows.show_journal_tab(if kind == "notebook" {
                JournalTab::Notebook
            } else {
                JournalTab::Timers
            });
        }
    }
    if let Some((ns, ew)) = waypoint {
        h.ui.windows
            .waypoints
            .set(pyreal_horizon::ui::waypoint::Waypoint { ns, ew, name: None });
    }
    if let Some((ns, ew)) = corpse {
        let at = pyreal_horizon::ui::corpse::Corpse {
            ns,
            ew,
            in_dungeon: false,
            cell: None,
        };
        h.ui.windows.waypoints.corpse.mark(at, 0.0);
    }
    if env("PYREAL_UI_HUD_LAYOUT").is_some() {
        h.ui.windows.open(WindowId::Layout, 0.0);
    } else if let Some(open) = env("PYREAL_UI_HUD_OPEN") {
        h.ui.windows.open(
            if open == "spellbook" {
                WindowId::Actions
            } else {
                WindowId::Character
            },
            0.0,
        );
    } else if env("PYREAL_UI_HUD_BARE").is_none() {
        h.ui.windows.open(WindowId::Social, 0.0);
    }
    settle(&mut h, &state);
    if let Some((x, y)) = env("PYREAL_UI_HUD_CLICK").and_then(|c| {
        let (x, y) = c.split_once(',')?;
        Some((x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?))
    }) {
        h.move_to(x, y);
        h.frame(&state);
        h.press();
        h.frame(&state);
        h.release();
        settle(&mut h, &state);
    }
    let name = std::env::var("PYREAL_UI_HUD_NAME").unwrap_or_else(|_| "hud".into());
    write(&dir.join(format!("{name}.png")), &h, &art);
}

/// The quest tracker's things for the HUD picture: a page with a timer, a contract, a quest timer
/// from the world and one heard in chat, and the rent owed in two days.
fn tracker_state(state: &mut GameState) {
    use pyreal_client_contract::journal::{HeardTimer, JournalPage, QuestTimerView};
    use pyreal_client_contract::view::{ContractEntry, HouseDataView};
    state.clock = 1_000.0;
    state.wall = 1_800_000_000;
    let page = JournalPage {
        title: "Drudge Hideout key".into(),
        page_number: 1,
        made: 1_700_000_000,
        timer_running: true,
        timer_stamp: 1_000.0 + 2.0 * 3_600.0 + 840.0,
        ..JournalPage::default()
    };
    state.journal.pages = vec![page.clone()];
    state.journal.draft = page;
    state.journal.current_page = 1;
    state.journal.loaded = true;
    state.journal.quests_known = true;
    state.journal.quests = vec![QuestTimerView {
        name: "colosseum".into(),
        title: "Colosseum".into(),
        min_delta: 72_000,
        ready_at: Some(f64::NEG_INFINITY),
        ..QuestTimerView::default()
    }];
    state.journal.heard = vec![HeardTimer {
        title: "Quest from Ayn Tayan".into(),
        ready_at: 1_000.0 + 15.0 * 3_600.0 + 1_800.0,
        seconds: 57_600,
    }];
    state.contracts = vec![ContractEntry {
        contract_id: 7,
        name: "Hunting Olthoi in the Deep".into(),
        status: "12 of 30 slain".into(),
        ..ContractEntry::default()
    }];
    state.world_services.house = Some(HouseDataView {
        rent_owed: true,
        maintenance_next_due: 1_800_000_000 + 2 * 86_400 + 3 * 3_600,
        ..HouseDataView::default()
    });
}

/// Not a claim: an instrument that draws character creation's Profession, Attributes, Skills and
/// Name pages over the world's own tables, with the interface's own art, into
/// `<PYREAL_UI_SHOTS>/creation-<era>-<heritage>-<preset>-<page>.png`.
///
/// - `PYREAL_UI_ERA`: `launch`, `infiltration` or `eor` (default all three).
/// - `PYREAL_UI_CREATION`: `heritage:preset` pairs, `;` between (default a few per era). A preset
///   the heritage cannot take is shown dimmed and explained.
/// - `PYREAL_UI_CHANGE`: when set, lowers Self by ten after choosing, to show a changed preset.
/// - `PYREAL_UI_SCALE`: the interface scale (default the window's own).
/// - `PYREAL_UI_ALL_PAGES`: when set, draws every page, not only those four.
/// - `PYREAL_UI_WHEEL`: `x,y,ticks`, the wheel turned there on each page (negative scrolls down).
#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder and have the dats"]
fn draw_the_creation_pages_for_a_person_to_look_at() {
    use pyreal_horizon::ui::creation::{Creation, Step};
    use pyreal_primitives::EraId;
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let Some(dir) = std::env::var_os("PYREAL_UI_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    let eras: Vec<EraId> = std::env::var("PYREAL_UI_ERA").map_or_else(
        |_| EraId::ALL.to_vec(),
        |s| s.split(',').filter_map(EraId::parse).collect(),
    );
    let art = Arc::new(Art::new(Arc::new(
        pyreal_horizon::pieces::Pieces::built_in(),
    )));
    for era in eras {
        let store = if era == EraId::Eor {
            pyreal_dat::testing::open_store_or_fail()
        } else {
            pyreal_dat::testing::open_classic_store_or_fail()
        };
        let tables =
            std::rc::Rc::new(pyreal_chargen::CreationTables::read(&store).expect("the tables"));
        let texts = std::rc::Rc::new(pyreal_chargen::CreationTexts::read(&store, &tables.chargen));
        let default = match era {
            EraId::Launch => "1:four;1:fourmana;3:fighter",
            EraId::Infiltration => "1:og;1:hfight;3:hfight;2:archer",
            EraId::Eor => "1:og;9:martyr;5:sarch",
        };
        let picks = std::env::var("PYREAL_UI_CREATION").unwrap_or_else(|_| default.into());
        let state = GameState {
            era: Some(era.features()),
            era_id: era,
            free_slot: Some(0),
            full_screen: Some(false),
            ..GameState::default()
        };
        for pick in picks.split(';') {
            let Some((heritage, preset)) = pick.split_once(':') else {
                continue;
            };
            let heritage: u32 = heritage.trim().parse().expect("a heritage key");
            let options = pyreal_horizon::options::HorizonOptions {
                scale: env("PYREAL_UI_SCALE").and_then(|s| s.parse().ok()),
                ..Default::default()
            };
            let mut h = Harness {
                ui: HorizonUi::new(Arc::clone(&art), options),
                list: DrawList::default(),
                input: InputFrame::default(),
                screen: (W as f32, H as f32),
            };
            let mut c = Creation::new(Rc::clone(&tables), Rc::clone(&texts), true, 0);
            assert!(c.state.choose_heritage(&tables, heritage), "{pick}");
            c.step = Step::Profession;
            h.ui.screen = pyreal_horizon::ui::Screen::Creation;
            h.ui.pregame.creation = Some(c);
            h.frame(&state);
            if let Some(c) = h.ui.pregame.creation.as_mut() {
                assert!(c.show_preset(preset.trim()), "{pick}");
                if std::env::var_os("PYREAL_UI_CHANGE").is_some() {
                    let s = c.state.get(pyreal_chargen::Attr::Self_);
                    c.state.set_attribute(pyreal_chargen::Attr::Self_, s - 10);
                }
            }
            let steps = if env("PYREAL_UI_ALL_PAGES").is_some() {
                vec![
                    Step::Heritage,
                    Step::Sex,
                    Step::Profession,
                    Step::Attributes,
                    Step::Skills,
                    Step::Town,
                    Step::Name,
                ]
            } else {
                vec![Step::Profession, Step::Attributes, Step::Skills, Step::Name]
            };
            for step in steps {
                if let Some(c) = h.ui.pregame.creation.as_mut() {
                    c.step = step;
                    if step == Step::Name {
                        c.random_name();
                    }
                }
                settle(&mut h, &state);
                if let Some((x, y, n)) = env("PYREAL_UI_WHEEL").and_then(|w| {
                    let v: Vec<f32> = w.split(',').filter_map(|n| n.trim().parse().ok()).collect();
                    (v.len() == 3).then(|| (v[0], v[1], v[2]))
                }) {
                    h.move_to(x, y);
                    h.input.wheel = n;
                    settle(&mut h, &state);
                }
                let name = format!(
                    "creation-{}-{heritage}-{}-{step:?}.png",
                    era.name(),
                    preset.trim()
                )
                .to_lowercase();
                write(&dir.join(name), &h, &art);
            }
        }
    }
}

/// Not a claim: draws the Admin window's every section, then Find and Spawn searching, into
/// `PYREAL_UI_SHOTS`. `PYREAL_ADMIN_WEENIES` names a weenie list to search (a world's `/weenies`
/// saved to a file), `PYREAL_ADMIN_DATS` a folder of retail data files for the icons, and
/// `PYREAL_ADMIN_FIND` the searches, `;` between.
#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder"]
fn draw_every_admin_section_for_a_person_to_look_at() {
    use pyreal_client_runtime::weenie_list::WeenieListState;
    use pyreal_horizon::art::AcArt as _;
    let Some(dir) = std::env::var_os("PYREAL_UI_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    let weenies = std::env::var_os("PYREAL_ADMIN_WEENIES").map(|p| {
        let text = std::fs::read_to_string(p).expect("the weenie list");
        Arc::new(
            pyreal_client_model::cmd::weenies::WeenieList::parse(&text).expect("a weenie list"),
        )
    });
    let mut state = GameState {
        in_world: true,
        name: "Tester".into(),
        is_admin: true,
        weenies: weenies.map(WeenieListState::Ready),
        ..GameState::default()
    };
    state.target = Some(pyreal_horizon::ui::game::Target {
        id: pyreal_primitives::ObjectId(0x8000_1A2C),
        name: "Drudge Skulker".into(),
        level: Some(8),
        ..Default::default()
    });
    let art = Arc::new(Art::new(Arc::new(
        pyreal_horizon::pieces::Pieces::built_in(),
    )));
    if let Some(dats) = std::env::var_os("PYREAL_ADMIN_DATS") {
        let store = pyreal_dat::RetailDatStore::open_dir(std::path::Path::new(&dats))
            .expect("the data files");
        art.set_ac_store(Arc::new(store));
    }
    let mut h = Harness {
        ui: HorizonUi::new(Arc::clone(&art), Default::default()),
        list: DrawList::default(),
        input: InputFrame::default(),
        screen: (W as f32, H as f32),
    };
    h.frame(&state);
    h.ui.windows.open(WindowId::Admin, 0.0);
    for section in 0..pyreal_horizon::ui::panels::admin::SECTIONS.len() {
        h.ui.windows.admin.tab = section;
        settle(&mut h, &state);
        write(&dir.join(format!("admin-section{section}.png")), &h, &art);
    }
    h.ui.windows.admin.tab = 0;
    let finds = std::env::var("PYREAL_ADMIN_FIND").unwrap_or_default();
    for (n, find) in finds.split(';').filter(|f| !f.is_empty()).enumerate() {
        let (find, only) = find.split_once('|').unwrap_or((find, "All"));
        h.ui.windows
            .admin
            .set_field(pyreal_horizon::ui::panels::admin::field::FIND, find);
        if only == "Places" {
            h.ui.windows.admin.set_places();
        } else {
            let only = pyreal_client_model::cmd::weenies::Only::ALL
                .into_iter()
                .find(|o| o.name() == only)
                .unwrap_or_default();
            h.ui.windows.admin.set_only(only);
        }
        settle(&mut h, &state);
        write(&dir.join(format!("admin-find{n}.png")), &h, &art);
    }
}

/// Not a claim: the map window over a world's own data files and the places of its era, into
/// `<PYREAL_UI_SHOTS>/<PYREAL_MAP_NAME>.png` (default `map`).
/// `PYREAL_MAP_DATS` names the data files' folder (the end of retail's, or a `portal.dat` and
/// `cell.dat` pair); `PYREAL_MAP_ERA` the era (`launch`, `infiltration`, `eor`);
/// `PYREAL_MAP_ADMIN` draws an administrator's map; `PYREAL_MAP_AT` (`42.2,33.8`) and
/// `PYREAL_MAP_ZOOM` (`4`) the view; `PYREAL_MAP_POINT` (`42.2,33.8`) where the pointer rests;
/// `PYREAL_MAP_PLACES` a places document to draw in place of the built-in ones;
/// `PYREAL_MAP_LIST` opens the Places list; `PYREAL_MAP_HIDE` turns kinds off (`npc,camp`).
/// `PYREAL_MAP_LINK` (`42.2,33.8`) opens the map as a coordinate link does, at that place.
/// `PYREAL_MAP_CORPSE` and `PYREAL_MAP_WAYPOINT` (`42.2,33.8`) mark a corpse and set a waypoint.
#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS and PYREAL_MAP_DATS"]
fn draw_the_map_and_its_places_for_a_person_to_look_at() {
    use pyreal_horizon::art::AcArt as _;
    use pyreal_horizon::ui::panels::map::{map_pixel, PlaceKind, OVERVIEW_KEY};
    use pyreal_primitives::EraId;
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let (Some(dir), Some(dats)) = (env("PYREAL_UI_SHOTS"), env("PYREAL_MAP_DATS")) else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    let dats = std::path::PathBuf::from(dats);
    let store = if dats.join("portal.dat").exists() {
        pyreal_dat::RetailDatStore::open_classic_dir(&dats)
    } else {
        pyreal_dat::RetailDatStore::open_dir(&dats)
    }
    .expect("the data files open");
    let pieces = pyreal_horizon::pieces::Pieces::built_in();
    let art = Arc::new(Art::new(Arc::new(pieces)));
    art.set_ac_store(Arc::new(store));
    let pair = |v: String| {
        let (a, b) = v.split_once(',').expect("ns,ew");
        (
            a.trim().parse::<f64>().expect("ns"),
            b.trim().parse::<f64>().expect("ew"),
        )
    };
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        is_admin: env("PYREAL_MAP_ADMIN").is_some(),
        era_id: match env("PYREAL_MAP_ERA").as_deref() {
            Some("launch") => EraId::Launch,
            Some("infiltration") => EraId::Infiltration,
            _ => EraId::Eor,
        },
        map_places: env("PYREAL_MAP_PLACES")
            .map(|f| Arc::from(std::fs::read_to_string(f).expect("the places document"))),
        ..GameState::default()
    };
    let scale: f32 = env("PYREAL_MAP_SCALE")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.8);
    let options = pyreal_horizon::options::HorizonOptions {
        scale: Some(scale),
        ..Default::default()
    };
    let mut h = Harness {
        ui: HorizonUi::new(Arc::clone(&art), options),
        list: DrawList::default(),
        input: InputFrame::default(),
        screen: (2400.0, 1600.0),
    };
    h.frame(&state);
    h.ui.windows.open(WindowId::Map, 0.0);
    h.frame(&state);
    let start = std::time::Instant::now();
    while art.lookup(OVERVIEW_KEY).is_none() && start.elapsed().as_secs() < 60 {
        h.frame(&state);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let view = &mut h.ui.windows.map_view;
    if let Some(at) = env("PYREAL_MAP_AT").map(pair) {
        view.centre = map_pixel(at.0, at.1);
    }
    view.zoom = env("PYREAL_MAP_ZOOM")
        .and_then(|z| z.parse().ok())
        .unwrap_or(1.0);
    view.kinds_open = env("PYREAL_MAP_LIST").is_some();
    for name in env("PYREAL_MAP_HIDE").unwrap_or_default().split(',') {
        if let Some(kind) = PlaceKind::named(name.trim()) {
            view.hidden.push(kind);
        }
    }
    if let Some(at) = env("PYREAL_MAP_LINK").map(pair) {
        view.centre = map_pixel(at.0, at.1);
        view.pin = Some((at, None));
    }
    if let Some((ns, ew)) = env("PYREAL_MAP_CORPSE").map(pair) {
        let at = pyreal_horizon::ui::corpse::Corpse {
            ns,
            ew,
            in_dungeon: false,
            cell: None,
        };
        h.ui.windows.waypoints.corpse.mark(at, 0.0);
    }
    if let Some((ns, ew)) = env("PYREAL_MAP_WAYPOINT").map(pair) {
        let name = Some("Holtburg road".to_owned());
        h.ui.windows
            .waypoints
            .set(pyreal_horizon::ui::waypoint::Waypoint { ns, ew, name });
    }
    for _ in 0..60 {
        h.frame(&state);
    }
    if let Some(at) = env("PYREAL_MAP_POINT").map(pair) {
        let area = h.ui.windows.map_picture.expect("the map's picture");
        let on = h.ui.windows.map_view.on_screen(area, map_pixel(at.0, at.1));
        h.move_to(on.0, on.1);
        for _ in 0..40 {
            h.frame(&state);
        }
    }
    let name = env("PYREAL_MAP_NAME").unwrap_or_else(|| "map".to_owned());
    let window =
        h.ui.windows
            .rects(scale)
            .into_iter()
            .find(|(id, _)| *id == WindowId::Map)
            .map(|(_, r)| {
                // Room for a tooltip past the window's right edge when the pointer rests on a place.
                let wide = if env("PYREAL_MAP_POINT").is_some() {
                    1.4
                } else {
                    1.0
                };
                pyreal_horizon::draw::Rect::new(r.x, r.y, r.w * wide, r.h)
            });
    write_part(&dir.join(format!("{name}.png")), &h, &art, window);
}

/// Not a claim: an instrument that draws the game windows' questions and lights, with the
/// interface's own art, into `<PYREAL_UI_SHOTS>/`: `train-question.png` (Train pressed on an
/// untrained skill), and the trade window with an offer changed since both accepted
/// (`trade-changed.png`), both accepted (`trade-accepted.png`) and nothing offered
/// (`trade-empty.png`).
#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder"]
fn draw_the_train_question_and_the_trade_lights_for_a_person_to_look_at() {
    use pyreal_client_contract::view::{TradeRow, TradeView};
    use pyreal_horizon::ui::game::{Modifiers, Skill};
    let Some(dir) = std::env::var_os("PYREAL_UI_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    store::init();
    let art = Arc::new(Art::new(Arc::new(
        pyreal_horizon::pieces::Pieces::built_in(),
    )));
    let harness = || Harness {
        ui: HorizonUi::new(Arc::clone(&art), Default::default()),
        list: DrawList::default(),
        input: InputFrame::default(),
        screen: (W as f32, H as f32),
    };
    let skill = |id, name: &str, training, cost| Skill {
        name: name.into(),
        value: 10,
        modifiers: Modifiers::of(10, 10, 0),
        training,
        id,
        cost,
        cost_10: cost * 10,
        ..Skill::default()
    };
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        skills: vec![
            skill(6, "Melee Defense", 3, 1_000),
            skill(15, "Magic Defense", 2, 800),
            skill(34, "Alchemy", 1, 6),
            skill(35, "Cooking", 1, 4),
        ],
        skill_credits: 20,
        unassigned_xp: 50_000,
        ..GameState::default()
    };
    let mut h = harness();
    h.ui.windows.open(WindowId::Character, -1.0);
    h.frame(&state);
    h.frame(&state);
    let snap = h.input.nav.last.clone();
    let bar = snap
        .tabs_in(snap.layers.len())
        .expect("the character window's tabs are noted");
    h.input.pad.tab_step = Some((bar, 1));
    settle(&mut h, &state);
    let train = h
        .input
        .nav
        .last
        .targets
        .iter()
        .map(|t| t.rect)
        .find(|r| (r.w - 76.0).abs() < 0.5 && (r.h - 22.0).abs() < 0.5)
        .expect("a Train button");
    h.move_to(train.x + train.w / 2.0, train.y + train.h / 2.0);
    h.frame(&state);
    h.press();
    h.frame(&state);
    h.release();
    h.move_to(0.0, 0.0);
    settle(&mut h, &state);
    write(&dir.join("train-question.png"), &h, &art);

    let row = |n: u32, name: &str| TradeRow {
        item: pyreal_primitives::ObjectId(0x8000_0100 + n),
        name: name.into(),
        icon: None,
    };
    let offer = |accepted, darkened, rows: bool| TradeView {
        open: true,
        partner_name: "Aldren".into(),
        self_rows: if rows {
            vec![row(1, "Sapphire")]
        } else {
            Vec::new()
        },
        partner_rows: if rows {
            vec![row(2, "Silver Sword"), row(3, "Pyreal Bar")]
        } else {
            Vec::new()
        },
        accepted,
        partner_accepted: accepted,
        acceptance_darkened: darkened,
        ..TradeView::default()
    };
    for (name, trade) in [
        ("trade-changed", offer(true, true, true)),
        ("trade-accepted", offer(true, false, true)),
        ("trade-empty", offer(false, false, false)),
    ] {
        let state = GameState {
            in_world: true,
            name: "Tester".into(),
            trade: Some(trade),
            ..GameState::default()
        };
        let mut h = harness();
        settle(&mut h, &state);
        let window =
            h.ui.windows
                .rects(1.0)
                .into_iter()
                .find(|(w, _)| *w == WindowId::Trade)
                .map(|(_, r)| r)
                .expect("the trade window");
        write_part(&dir.join(format!("{name}.png")), &h, &art, Some(window));
    }
}

/// Not a claim: character select with a short name, a long one and one being deleted chosen in
/// turn, the pointer over Delete, and the boxes Enter and Delete ask, into
/// `<PYREAL_UI_SHOTS>/lobby-<what>.png`.
#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder"]
fn draw_character_select_for_a_person_to_look_at() {
    use pyreal_horizon::options::{HorizonOptions, StartScreen};
    use pyreal_horizon::ui::game::{CharacterEntry, ConnectPhase};
    use pyreal_horizon::ui::input::vk;
    use pyreal_horizon::ui::pregame::LobbyLayout;
    use pyreal_primitives::ObjectId;
    let Some(dir) = std::env::var_os("PYREAL_UI_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    let entry = |id: u32, name: &str, delete_seconds| CharacterEntry {
        id: ObjectId(0x5000_0000 + id),
        name: name.into(),
        delete_seconds,
    };
    let state = GameState {
        connected: true,
        connect_phase: ConnectPhase::Ready,
        host: "127.0.0.1:9000".into(),
        world: Some("Test".into()),
        characters: vec![
            entry(1, "Asheron", 0),
            entry(2, "Sir Bartholomew Longname-Fairweather", 0),
            entry(3, "Leaving", 600),
        ],
        character_slots: 11,
        ..GameState::default()
    };
    let art = Arc::new(Art::new(Arc::new(
        pyreal_horizon::pieces::Pieces::built_in(),
    )));
    let options = HorizonOptions {
        screen: StartScreen::Lobby,
        ..HorizonOptions::default()
    };
    let layout = LobbyLayout::new((W as f32, H as f32), 1.0);
    for (i, what) in ["short", "long", "deleting"].into_iter().enumerate() {
        let mut h = Harness {
            ui: HorizonUi::new(Arc::clone(&art), options.clone()),
            list: DrawList::default(),
            input: InputFrame::default(),
            screen: (W as f32, H as f32),
        };
        h.frame(&state);
        for _ in 0..i {
            h.input.keys.push(vk::DOWN);
            h.frame(&state);
        }
        settle(&mut h, &state);
        write(&dir.join(format!("lobby-{what}.png")), &h, &art);
        let d = layout.delete;
        h.move_to(d.x + d.w / 2.0, d.y + d.h / 2.0);
        settle(&mut h, &state);
        write(&dir.join(format!("lobby-{what}-over-delete.png")), &h, &art);
        h.move_to(0.0, 0.0);
        if what != "deleting" {
            h.input.keys.push(vk::ENTER);
            settle(&mut h, &state);
            write(&dir.join(format!("lobby-{what}-enter.png")), &h, &art);
            h.input.keys.push(vk::ESCAPE);
            settle(&mut h, &state);
        }
        h.input.keys.push(vk::DELETE);
        settle(&mut h, &state);
        write(&dir.join(format!("lobby-{what}-delete.png")), &h, &art);
    }
}

/// Not a claim: an instrument that draws the windows a second set of window fixes touches, with
/// the interface's own art, into `<PYREAL_UI_SHOTS>/`: `skills-groups.png` (the four skill
/// groups), `shop-row-cost.png` (100 arrows chosen at a vendor), `allegiance-rank.png` (a rank a
/// spell raises) and `exp-cap.png` (the EXP bar at the level cap).
#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder"]
fn draw_the_skill_groups_shop_row_rank_and_exp_cap_for_a_person_to_look_at() {
    use pyreal_client_contract::view::{AllegianceEntry, AllegianceRoster, ShopRow, ShopView};
    use pyreal_horizon::draw::Rect;
    use pyreal_horizon::ui::game::{Modifiers, Skill};
    let Some(dir) = std::env::var_os("PYREAL_UI_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    store::init();
    let art = Arc::new(Art::new(Arc::new(
        pyreal_horizon::pieces::Pieces::built_in(),
    )));
    let harness = || Harness {
        ui: HorizonUi::new(Arc::clone(&art), Default::default()),
        list: DrawList::default(),
        input: InputFrame::default(),
        screen: (W as f32, H as f32),
    };
    let window = |h: &Harness, id: WindowId| {
        h.ui.windows
            .rects(1.0)
            .into_iter()
            .find(|(w, _)| *w == id)
            .map(|(_, r)| r)
            .expect("the window is open")
    };
    let in_world = || GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    };

    // The skill groups.
    let skill = |id, name: &str, training, u: bool| Skill {
        name: name.into(),
        value: 10,
        modifiers: Modifiers::of(10, 10, 0),
        training,
        unusable: u,
        id,
        cost: if training == 1 { 6 } else { 1_000 },
        cost_10: 10_000,
        ..Skill::default()
    };
    let state = GameState {
        skills: vec![
            skill(6, "Melee Defense", 3, false),
            skill(15, "Magic Defense", 2, false),
            skill(24, "Run", 1, false),
            skill(22, "Jump", 1, false),
            skill(34, "Alchemy", 1, true),
            skill(33, "Life Magic", 1, true),
        ],
        skill_credits: 20,
        unassigned_xp: 50_000,
        ..in_world()
    };
    let mut h = harness();
    h.ui.windows.open(WindowId::Character, -1.0);
    h.frame(&state);
    h.frame(&state);
    let snap = h.input.nav.last.clone();
    let bar = snap
        .tabs_in(snap.layers.len())
        .expect("the character window's tabs are noted");
    h.input.pad.tab_step = Some((bar, 1));
    settle(&mut h, &state);
    let r = window(&h, WindowId::Character);
    write_part(&dir.join("skills-groups.png"), &h, &art, Some(r));

    // 100 arrows chosen at a vendor.
    let state = GameState {
        shop: Some(ShopView {
            open: true,
            stock: vec![ShopRow {
                item: pyreal_primitives::ObjectId(0x8000_0201),
                name: "Arrow".into(),
                plural_name: "Arrows".into(),
                amount: -1,
                obj_type: 0x100,
                max_stack_size: 250,
                price: 4,
                unit_value: 3,
                ..ShopRow::default()
            }],
            total_value: 12_000,
            type_filters: vec![("Missile Weapons", 0x100)],
            sell_rate_bits: 1.15f32.to_bits(),
            notes_cash_anywhere: true,
            ..ShopView::default()
        }),
        ..in_world()
    };
    let mut h = harness();
    settle(&mut h, &state);
    let r = window(&h, WindowId::Vendor);
    let row = h
        .input
        .nav
        .last
        .targets
        .iter()
        .map(|t| t.rect)
        .find(|t| r.contains(t.x + 1.0, t.y + 1.0) && (t.h - 34.0).abs() < 0.5)
        .expect("the stock row");
    let click = |h: &mut Harness, at: Rect, shift: bool| {
        h.move_to(at.x + at.w / 2.0, at.y + at.h / 2.0);
        h.input.shift = shift;
        h.frame(&state);
        h.press();
        h.frame(&state);
        h.release();
        h.frame(&state);
        h.input.shift = false;
    };
    click(&mut h, row, false);
    for _ in 0..10 {
        let plus = h
            .input
            .nav
            .last
            .targets
            .iter()
            .map(|t| t.rect)
            .filter(|t| (t.w - 26.0).abs() < 0.5 && (t.h - 22.0).abs() < 0.5)
            .max_by(|a, b| a.x.total_cmp(&b.x))
            .expect("the + button");
        click(&mut h, plus, true);
    }
    h.move_to(0.0, 0.0);
    settle(&mut h, &state);
    write_part(&dir.join("shop-row-cost.png"), &h, &art, Some(r));

    // A rank a spell raises.
    let me = AllegianceEntry {
        id: pyreal_primitives::ObjectId(1),
        full_name: "Lord Tester".into(),
        logged_in: true,
        rank: 5,
        cp_cached: 0,
    };
    let state = GameState {
        player_id: Some(pyreal_primitives::ObjectId(1)),
        roster: AllegianceRoster {
            allegiance_name: "The Wanderers".into(),
            total_members: 12,
            total_vassals: 11,
            own_cp_tithed: 123_456,
            subject: Some(me.clone()),
            player_rank_quality: 6,
            monarch: Some(me),
            ..AllegianceRoster::default()
        },
        ..in_world()
    };
    let mut h = harness();
    h.frame(&state);
    h.ui.windows.open(WindowId::Allegiance, 0.0);
    settle(&mut h, &state);
    let r = window(&h, WindowId::Allegiance);
    write_part(&dir.join("allegiance-rank.png"), &h, &art, Some(r));

    // The EXP bar at the level cap.
    let state = GameState {
        level: 275,
        total_xp: 191_226_310_247,
        xp_this: Some(0),
        xp_next: Some(0),
        xp_capped: true,
        ..in_world()
    };
    let mut h = harness();
    settle(&mut h, &state);
    write_part(
        &dir.join("exp-cap.png"),
        &h,
        &art,
        Some(Rect::new(560.0, 1000.0, 800.0, 80.0)),
    );
}

/// Not a claim: an instrument that draws the Social window's Friends tab with fifteen friends
/// (`friends-15.png`) and the loot window over a chest of sixty items (`loot-60.png`), with the
/// interface's own art, into `<PYREAL_UI_SHOTS>/`.
#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder"]
fn draw_fifteen_friends_and_a_chest_of_sixty_for_a_person_to_look_at() {
    use pyreal_client_contract::view::FriendEntry;
    use pyreal_horizon::ui::game::Item;
    use pyreal_primitives::ObjectId;
    let Some(dir) = std::env::var_os("PYREAL_UI_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    store::init();
    let art = Arc::new(Art::new(Arc::new(
        pyreal_horizon::pieces::Pieces::built_in(),
    )));
    let harness = || Harness {
        ui: HorizonUi::new(Arc::clone(&art), Default::default()),
        list: DrawList::default(),
        input: InputFrame::default(),
        screen: (W as f32, H as f32),
    };
    let window = |h: &Harness, id: WindowId| {
        h.ui.windows
            .rects(1.0)
            .into_iter()
            .find(|(w, _)| *w == id)
            .map(|(_, r)| r)
            .expect("the window is open")
    };
    let names = [
        "Aldren",
        "Brannoc",
        "Corvin",
        "Isolde",
        "Mirelle",
        "Seraphine",
        "Torvald",
        "Wynna",
        "Ashe",
        "Bryn",
        "Caddoc",
        "Delia",
        "Eadric",
        "Fenna",
        "Garrick",
    ];
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        friend_list: names
            .iter()
            .zip(0u32..)
            .map(|(n, i)| FriendEntry {
                id: ObjectId(0x5000_0100 + i),
                name: (*n).into(),
                online: i % 3 == 0,
            })
            .collect(),
        ..GameState::default()
    };
    let mut h = harness();
    h.frame(&state);
    h.ui.windows.open(WindowId::Social, 0.0);
    h.frame(&state);
    h.frame(&state);
    let snap = h.input.nav.last.clone();
    let bar = snap.tabs_in(snap.layers.len()).expect("the window's tabs");
    h.input.pad.tab_step = Some((bar, 1));
    settle(&mut h, &state);
    let r = window(&h, WindowId::Social);
    write_part(&dir.join("friends-15.png"), &h, &art, Some(r));

    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        loot: Some((
            ObjectId(0x7000_0001),
            "Chest".into(),
            (0..60u32)
                .map(|i| Item {
                    id: ObjectId(0x8000_0300 + i),
                    name: format!("Thing {i}"),
                    stack: Some(1),
                    ..Item::default()
                })
                .collect(),
        )),
        ..GameState::default()
    };
    let mut h = harness();
    settle(&mut h, &state);
    let r = window(&h, WindowId::Loot);
    write_part(&dir.join("loot-60.png"), &h, &art, Some(r));
}

/// Not a claim: the log window and its input line in each of its states, Classic and Modern,
/// into `<PYREAL_UI_SHOTS>/chat-*.png`.
#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder"]
fn draw_the_chat_line_s_states_for_a_person_to_look_at() {
    use pyreal_client_contract::chat::mainchat::ChatFocusView;
    use pyreal_horizon::ui::chat::ChatStyle;
    use pyreal_horizon::ui::game::ChatLine;
    use pyreal_horizon::ui::input::vk;
    let Some(dir) = std::env::var_os("PYREAL_UI_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    store::init();
    let art = Arc::new(Art::new(Arc::new(
        pyreal_horizon::pieces::Pieces::built_in(),
    )));
    let harness = |style| Harness {
        ui: HorizonUi::new(
            Arc::clone(&art),
            pyreal_horizon::options::HorizonOptions {
                chat_style: style,
                ..Default::default()
            },
        ),
        list: DrawList::default(),
        input: InputFrame::default(),
        screen: (W as f32, H as f32),
    };
    let mut focus = ChatFocusView {
        focus: 1,
        ..ChatFocusView::default()
    };
    for f in [1, 3, 7, 8, 9] {
        focus.enabled[f] = true;
        focus.selectable[f] = true;
    }
    let mut state = GameState {
        in_world: true,
        name: "Tester".into(),
        chat_focus: focus,
        ..GameState::default()
    };
    state.chat_windows = pyreal_horizon::ui::chat::WINDOWS
        .iter()
        .map(|id| pyreal_horizon::ui::game::ChatWindow {
            id: *id,
            filter: pyreal_horizon::ui::chat::default_filter(*id),
            title: None,
        })
        .collect();
    state.reply_targets.last_teller = Some("Alba".into());
    state.reply_targets.recent = vec![
        "Alba".into(),
        "Sir Borin of Rithwic".into(),
        "Kaelis".into(),
    ];
    let line = |chat_type, text: &str| ChatLine {
        chat_type,
        text: text.into(),
        window: 0,
    };
    let lines = vec![
        line(2, "<Tell:IIDString:1342177283:Borin>Borin<\\Tell> says, \"Anyone heading to the Shoushi drudges?\""),
        line(19, "[Fellowship] <Tell:IIDString:1342177284:Mira>Mira<\\Tell> says, \"Healing up, one sec\""),
        line(21, "You hit the drudge prowler for 12 points of slashing damage!"),
        line(21, "The drudge prowler misses you."),
        line(3, "<Tell:IIDString:1342177282:Alba>Alba<\\Tell> tells you, \"Yes, 40k each. Where are you?\""),
        line(4, "You tell Kaelis, \"On my way to Holtburg\""),
    ];
    let shot = |h: &mut Harness, state: &GameState, name: &str| {
        h.frame(state);
        let d = h.ui.hud.log.dock;
        let part =
            pyreal_horizon::draw::Rect::new(d.x - 12.0, d.y - 250.0, d.w + 24.0, d.h + 262.0);
        write_part(&dir.join(format!("chat-{name}.png")), h, &art, Some(part));
    };
    let open = |h: &mut Harness, state: &GameState| {
        h.input.keys.push(vk::ENTER);
        h.input
            .actions
            .push(pyreal_client_contract::actions::chat_entry::BEGIN_CHAT_MODE.0);
        h.frame(state);
    };
    let typed = |h: &mut Harness, state: &GameState, text: &str| {
        h.input.chars = text.chars().collect();
        h.frame(state);
    };
    for (style, word) in [
        (ChatStyle::Classic, "classic"),
        (ChatStyle::Modern, "modern"),
    ] {
        let mut h = harness(style);
        h.frame(&state);
        state.new_chat.clone_from(&lines);
        h.frame(&state);
        state.new_chat.clear();
        settle(&mut h, &state);
        open(&mut h, &state);
        typed(
            &mut h,
            &state,
            "/t Alba, are you still selling those scarabs?",
        );
        shot(&mut h, &state, &format!("{word}-tell"));
        h.input.keys.push(vk::ESCAPE);
        h.frame(&state);
        // Closed, the bar shows the draft the game keeps.
        state.chat_draft = if style == ChatStyle::Modern {
            "@tell Alba, are you still selling those scarabs?".into()
        } else {
            "/t Alba, are you still selling those scarabs?".into()
        };
        h.input.mouse = (0.0, 0.0);
        shot(&mut h, &state, &format!("{word}-closed"));
        state.chat_draft.clear();
        let mut h = harness(style);
        h.frame(&state);
        state.new_chat.clone_from(&lines);
        h.frame(&state);
        state.new_chat.clear();
        settle(&mut h, &state);
        open(&mut h, &state);
        typed(&mut h, &state, "/t Al");
        shot(&mut h, &state, &format!("{word}-name"));
        typed(&mut h, &state, "ba,");
        if style == ChatStyle::Modern {
            let chip = h.ui.hud.log.control("Chat Destination").expect("the chip");
            h.input.mouse = (chip.x + chip.w / 2.0, chip.y + chip.h / 2.0);
            h.input.pressed[0] = true;
            h.frame(&state);
            h.input.pressed[0] = false;
            shot(&mut h, &state, "modern-menu");
            h.input.mouse = (0.0, 0.0);
            h.input.keys.push(vk::ESCAPE);
            h.frame(&state);
            h.input.keys.push(vk::BACK);
            shot(&mut h, &state, "modern-backspace");
            h.input.keys.push(vk::ESCAPE);
            h.frame(&state);
            let main = h.ui.hud.log.control("Main").expect("the main tab");
            h.input.mouse = (main.x + main.w / 2.0, main.y + main.h / 2.0);
            h.input.pressed[0] = true;
            h.frame(&state);
            h.input.pressed[0] = false;
            h.frame(&state);
            let name = h.ui.hud.log.control("Name: Mira").expect("Mira's name");
            h.input.mouse = (name.x + name.w / 2.0, name.y + name.h / 2.0);
            h.input.pressed[1] = true;
            h.frame(&state);
            h.input.pressed[1] = false;
            shot(&mut h, &state, "modern-name-menu");
        }
    }
}

/// Not a claim: the Journal window's three tabs (`journal-contracts.png`, `journal-notebook.png`,
/// `journal-timers.png`) into `<PYREAL_UI_SHOTS>/`, and with `PYREAL_MAP_DATS` the map with the
/// journal's pages on it (`journal-map.png`).
#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder"]
#[allow(clippy::too_many_lines)]
fn draw_the_journal_for_a_person_to_look_at() {
    use pyreal_client_contract::journal::{HeardTimer, JournalPage, JournalView, QuestTimerView};
    use pyreal_client_contract::view::ContractEntry;
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let Some(dir) = env("PYREAL_UI_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    store::init();
    let art = Arc::new(Art::new(Arc::new(
        pyreal_horizon::pieces::Pieces::built_in(),
    )));
    let wall = 1_763_380_800; // 17 Nov 2025.
    let page = |n: u32, label: &str, title: &str, notes: &str| JournalPage {
        label: label.into(),
        title: title.into(),
        notes: notes.into(),
        page_number: n,
        made: wall - 3 * 86_400,
        edited: wall - 600,
        ..JournalPage::default()
    };
    let clock = 10_000.0;
    let mut pages = vec![
        page(1, "Timer", "Aerbax's Defeat", "Can repeat every 20 hours."),
        page(
            2,
            "Quest, Al-Arqas",
            "Gharu'ndim relics",
            "Bring the three relics to Ayn Tayan in Al-Arqas.\nOne is in the Mite Maze at 40.4N, 39.5W, one is sold by the Gharu'ndim trader.\nThird: ask the Archmage.",
        ),
        page(3, "Loot", "Mana forge keys", "Keys sell for 5,000 at the Shopkeep."),
        page(4, "Advocate", "To Fort Tethana (1/2)", "Step to the Portal to Town Network.\nSigned, Tamsin of Holtburg."),
        page(5, "Advocate", "To Fort Tethana (2/2)", "Take the portal to Fort Tethana.\nSigned, Tamsin of Holtburg."),
    ];
    pages[0].timer_running = true;
    pages[0].timer_stamp = clock - 30.0;
    pages[1].timer_running = true;
    pages[1].timer_stamp = clock + 3.0 * 3_600.0 + 12.0 * 60.0;
    pages[1].hours = 20;
    for (i, (ns, ew)) in [(1, (-31.4, 13.2)), (3, (42.3, -33.3)), (4, (40.1, -42.9))] {
        pages[i].location_set = true;
        pages[i].ns = ns;
        pages[i].ew = ew;
    }
    pages[3].from = "~Advocate Tamsin".into();
    pages[4].from = "~Advocate Tamsin".into();
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        clock,
        wall,
        contracts: vec![ContractEntry {
            contract_id: 1,
            name: "Hunting Aun Ralirea".into(),
            status: "In Progress".into(),
            description: "Slay the Aun Ralirea in the Ruins of Ahurenga.".into(),
            contact: "Ralirea Scout".into(),
            timed: "Timed: 2h 5m".into(),
            repeat_remaining: Some(2.0 * 3_600.0 + 5.0 * 60.0),
            ..ContractEntry::default()
        }],
        journal: JournalView {
            draft: pages[1].clone(),
            pages,
            current_page: 2,
            loaded: true,
            quests_known: true,
            since: 0.0,
            quests: vec![QuestTimerView {
                name: "colosseumwins".into(),
                title: "Colosseum".into(),
                min_delta: 72_000,
                ready_at: Some(clock + 19.0 * 3_600.0 + 40.0 * 60.0),
                ..QuestTimerView::default()
            }],
            heard: vec![HeardTimer {
                title: "Quest from Ayn Tayan".into(),
                ready_at: clock + 6.0 * 86_400.0 + 4.0 * 3_600.0,
                seconds: 0,
            }],
            ..JournalView::default()
        },
        ..GameState::default()
    };
    let mut h = Harness {
        ui: HorizonUi::new(Arc::clone(&art), Default::default()),
        list: DrawList::default(),
        input: InputFrame::default(),
        screen: (W as f32, H as f32),
    };
    h.ui.windows.open(WindowId::Journal, -1.0);
    h.frame(&state);
    settle(&mut h, &state);
    let window = |h: &Harness| {
        h.ui.windows
            .rects(1.0)
            .into_iter()
            .find(|(id, _)| *id == WindowId::Journal)
            .map(|(_, r)| r)
    };
    for name in ["journal-contracts", "journal-notebook", "journal-timers"] {
        if name != "journal-contracts" {
            h.frame(&state);
            let snap = h.input.nav.last.clone();
            let bar = snap
                .tabs_in(snap.layers.len())
                .expect("the journal window's tabs are noted");
            h.input.pad.tab_step = Some((bar, 1));
            settle(&mut h, &state);
        }
        let r = window(&h);
        write_part(&dir.join(format!("{name}.png")), &h, &art, r);
    }
    let Some(dats) = env("PYREAL_MAP_DATS") else {
        return;
    };
    use pyreal_horizon::art::AcArt as _;
    use pyreal_horizon::ui::panels::map::{map_pixel, OVERVIEW_KEY};
    let dats = std::path::PathBuf::from(dats);
    let store = pyreal_dat::RetailDatStore::open_dir(&dats).expect("the data files open");
    art.set_ac_store(Arc::new(store));
    let state = GameState {
        is_admin: env("PYREAL_UI_ADMIN").is_some(),
        coords: env("PYREAL_UI_ADMIN").map(|_| (40.9, -37.6)),
        ..state
    };
    let mut h = Harness {
        ui: HorizonUi::new(Arc::clone(&art), Default::default()),
        list: DrawList::default(),
        input: InputFrame::default(),
        screen: (2000.0, 1500.0),
    };
    h.frame(&state);
    h.ui.windows.open(WindowId::Map, 0.0);
    h.frame(&state);
    let start = std::time::Instant::now();
    while art.lookup(OVERVIEW_KEY).is_none() && start.elapsed().as_secs() < 60 {
        h.frame(&state);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.ui.windows
        .waypoints
        .set(pyreal_horizon::ui::waypoint::Waypoint {
            ns: 42.3,
            ew: -33.3,
            name: Some("To Fort Tethana (1/2)".into()),
        });
    h.ui.windows.map_view.centre = map_pixel(41.0, -38.0);
    h.ui.windows.map_view.zoom = 3.0;
    let pin = ((40.4, -39.5), Some("Gharu'ndim relics".to_owned()));
    h.ui.windows.map_view.pin = Some(pin.clone());
    h.ui.windows.map_view.offer = Some(pin);
    for _ in 0..60 {
        h.frame(&state);
    }
    let r =
        h.ui.windows
            .rects(1.0)
            .into_iter()
            .find(|(id, _)| *id == WindowId::Map)
            .map(|(_, r)| r);
    write_part(&dir.join("journal-map.png"), &h, &art, r);
}

/// Not a claim: item details on hover over the inventory, alone, beside the worn helm, with the
/// worn helm's appraisal failed and with a spell on it, and a coat beside the three pieces it
/// would replace, into `<PYREAL_UI_SHOTS>/item-details-*.png`.
#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder"]
fn draw_item_details_on_hover_for_a_person_to_look_at() {
    use pyreal_client_contract::view::{
        AppraisalSpellView, AppraisalView, DropTarget, MagicInfoView,
    };
    use pyreal_horizon::ui::appraisals::Appraised;
    use pyreal_horizon::ui::game::{ExaminePane, Examined, Item};
    use pyreal_horizon::ui::kit::Drop;
    use pyreal_primitives::ObjectId;
    let Some(dir) = std::env::var_os("PYREAL_UI_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    store::init();
    let art = Arc::new(Art::new(Arc::new(
        pyreal_horizon::pieces::Pieces::built_in(),
    )));
    let me = ObjectId(0x5000_0001);
    let (new, old) = (ObjectId(0x8000_0020), ObjectId(0x8000_0021));
    let helm = |id, name: &str, worn| Item {
        id,
        name: name.into(),
        stack: Some(1),
        worn,
        equip_locations: 0x1,
        ..Item::default()
    };
    let spell = |name: &str, description: &str| AppraisalSpellView {
        resolved: true,
        name: name.into(),
        description: description.into(),
        ..AppraisalSpellView::default()
    };
    let view = |level: i32, value: i32, workmanship: i32, success: bool| AppraisalView {
        success,
        delivery: 1,
        value: Some(value),
        burden: Some(250),
        valid_locations: 0x1,
        armor_level: Some(level),
        armor_mods: Some([1.0, 0.8, 1.2, 0.6, 0.4, 0.6, 0.0, 0.4]),
        workmanship: Some(workmanship),
        magic: MagicInfoView {
            spells: Some(vec![
                spell(
                    "Impenetrability II",
                    "Improves a shield or piece of armor's armor value by 50 points.",
                ),
                spell(
                    "Strength Self III",
                    "Increases the wielder's Strength by 20 points.",
                ),
            ]),
            spellcraft: Some(150),
            cur_mana: Some(410),
            max_mana: Some(500),
            mana_rate: Some(-0.05),
            ..MagicInfoView::default()
        },
        ..AppraisalView::default()
    };
    let appraised = |id, title: &str, v: &AppraisalView| {
        let detail = Examined {
            id,
            pane: ExaminePane::Item,
            title: title.into(),
            runs: pyreal_presentation::appraisal::item_description_runs(v)
                .into_iter()
                .map(|r| (r.text, r.same_line, r.color))
                .collect(),
            ..Examined::default()
        };
        Appraised::of(id, v, detail)
    };
    let mut state = GameState {
        in_world: true,
        name: "Tester".into(),
        player_id: Some(me),
        pack: vec![helm(new, "Steel Helm", 0)],
        equipped: vec![helm(old, "Leather Cap", 0x1)],
        ..GameState::default()
    };
    state.appraised = vec![
        appraised(new, "Steel Helm", &view(120, 1200, 7, true)),
        appraised(old, "Leather Cap", &view(80, 400, 4, true)),
    ];
    // The cap with Impenetrability VI cast on it.
    let mut buffed = view(280, 400, 4, true);
    if let Some(spells) = buffed.magic.spells.as_mut() {
        spells.push(AppraisalSpellView {
            raw_id: 0x8000_05CE,
            enchantment: true,
            category: 160,
            ..spell(
                "Impenetrability VI",
                "Improves a shield or piece of armor's armor value by 200 points.",
            )
        });
    }
    buffed.enchantment_mods.insert(0x1C, true);
    let coat_places = 0x1E00;
    let piece = |id: u32, name: &str, places: u32| Item {
        equip_locations: places,
        ..helm(ObjectId(id), name, places)
    };
    let armour = |places: u32, level: i32, value: i32| AppraisalView {
        valid_locations: places,
        ..view(level, value, 6, true)
    };
    let mut coat = state.clone();
    coat.pack = vec![piece(0x8000_0030, "Chainmail Coat", 0)];
    coat.pack[0].equip_locations = coat_places;
    coat.equipped = vec![
        piece(0x8000_0031, "Breastplate", 0x200),
        piece(0x8000_0032, "Girth", 0x400),
        piece(0x8000_0033, "Sleeves", 0x1800),
    ];
    coat.appraised = vec![
        appraised(
            ObjectId(0x8000_0030),
            "Chainmail Coat",
            &armour(coat_places, 300, 3000),
        ),
        appraised(
            ObjectId(0x8000_0031),
            "Breastplate",
            &armour(0x200, 350, 1500),
        ),
        appraised(ObjectId(0x8000_0032), "Girth", &armour(0x400, 250, 800)),
        appraised(ObjectId(0x8000_0033), "Sleeves", &armour(0x1800, 200, 900)),
    ];
    for (name, shift, worn) in [
        ("item-details.png", false, None),
        ("item-details-compare.png", true, None),
        (
            "item-details-compare-failed.png",
            true,
            Some(view(80, 400, 4, false)),
        ),
        (
            "item-details-compare-buffed.png",
            true,
            Some(buffed.clone()),
        ),
        ("item-details-compare-coat.png", true, None),
    ] {
        let mut state = if name.ends_with("coat.png") {
            coat.clone()
        } else {
            state.clone()
        };
        if let Some(v) = worn {
            state.appraised[1] = appraised(old, "Leather Cap", &v);
        }
        let mut h = Harness {
            ui: HorizonUi::new(Arc::clone(&art), Default::default()),
            list: DrawList::default(),
            input: InputFrame::default(),
            screen: (W as f32, H as f32),
        };
        h.ui.windows.open(WindowId::Inventory, 0.0);
        settle(&mut h, &state);
        let at =
            h.ui.drops
                .iter()
                .rev()
                .find(|(_, t)| {
                    matches!(
                        t,
                        Some(Drop::Target(DropTarget::ItemListSlot { index: 0, .. }))
                    )
                })
                .map(|(r, _)| *r)
                .expect("the first slot");
        h.move_to(at.x + at.w / 2.0, at.y + at.w / 2.0);
        h.input.shift = shift;
        settle(&mut h, &state);
        write(&dir.join(name), &h, &art);
    }
}

/// Not a claim: the inventory searched, with Keep and Junk marks, the question before a Keep item
/// is dropped, and the vendor's Sell tab with Add Junk, into `<PYREAL_UI_SHOTS>/marks-*.png`.
/// With `PYREAL_MAP_DATS` the items carry icons from the data files.
#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder"]
fn draw_the_pack_search_and_item_marks_for_a_person_to_look_at() {
    use pyreal_client_contract::view::{ShopView, SlotDecoration};
    use pyreal_horizon::art::AcArt as _;
    use pyreal_horizon::ui::game::Item;
    use pyreal_horizon::ui::panels::marks::Mark;
    use pyreal_primitives::ObjectId;
    use pyreal_rules::weenie::item_type as t;
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let Some(dir) = env("PYREAL_UI_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    store::init();
    let art = Arc::new(Art::new(Arc::new(
        pyreal_horizon::pieces::Pieces::built_in(),
    )));
    if let Some(dats) = env("PYREAL_MAP_DATS").map(std::path::PathBuf::from) {
        let store = if dats.join("portal.dat").exists() {
            pyreal_dat::RetailDatStore::open_classic_dir(&dats)
        } else {
            pyreal_dat::RetailDatStore::open_dir(&dats)
        }
        .expect("the data files open");
        art.set_ac_store(Arc::new(store));
    }
    let things = [
        ("Ivory Long Sword", t::MELEE_WEAPON, 0x0600_1355),
        ("Bronze Helm", t::ARMOR, 0x0600_1393),
        ("Salvaged Iron", t::TINKERING_MATERIAL, 0x0600_33A4),
        ("Black Opal", t::GEM, 0x0600_10E6),
        ("Leather Boots", t::ARMOR, 0x0600_1043),
        ("Healing Kit", t::MISC, 0x0600_1062),
        ("Yew Longbow", t::MISSILE_WEAPON, 0x0600_1036),
        ("Chainmail Gauntlets", t::ARMOR, 0x0600_1031),
        ("Bread", t::FOOD, 0x0600_1052),
        ("Copper Ring", t::JEWELRY, 0x0600_10B2),
    ];
    let item = |i: u32, (name, kind, icon): (&str, u32, u32)| Item {
        id: ObjectId(0x8000_0400 + i),
        name: name.into(),
        wcid: 0x400 + i,
        ac_icon: Some(icon),
        stack: Some(1),
        safety: pyreal_client_contract::view::SellSafety {
            appraised: true,
            value: Some(100),
            ..Default::default()
        },
        decoration: Some(SlotDecoration {
            obj_type: kind,
            icon_id: icon,
            ..SlotDecoration::default()
        }),
        ..Item::default()
    };
    let mut state = GameState {
        in_world: true,
        name: "Tester".into(),
        player_id: Some(ObjectId(0x5000_0001)),
        pack: (0u32..).zip(things).map(|(i, x)| item(i, x)).collect(),
        pack_capacity: Some(102),
        side_packs: vec![
            (
                item(20, ("Sack", t::CONTAINER, 0x0600_1080)),
                vec![item(21, ("Diamond Helm", t::ARMOR, 0x0600_1393))],
            ),
            (item(22, ("Pouch", t::CONTAINER, 0x0600_1080)), Vec::new()),
        ],
        ..GameState::default()
    };
    let harness = || Harness {
        ui: HorizonUi::new(Arc::clone(&art), Default::default()),
        list: DrawList::default(),
        input: InputFrame::default(),
        screen: (W as f32, H as f32),
    };
    let window = |h: &Harness, id: WindowId| {
        h.ui.windows
            .rects(1.0)
            .into_iter()
            .find(|(w, _)| *w == id)
            .map(|(_, r)| r)
    };
    let mark = |h: &mut Harness, state: &GameState| {
        let it = pyreal_horizon::ui::panels::marks::carried(state)
            .find(|i| i.id == ObjectId(0x8000_0400))
            .cloned()
            .expect("carried");
        h.ui.windows.marks.set(&it, Some(Mark::Keep));
        let it = pyreal_horizon::ui::panels::marks::carried(state)
            .find(|i| i.id == ObjectId(0x8000_0402))
            .cloned()
            .expect("carried");
        h.ui.windows.marks.set(&it, Some(Mark::Junk));
        let it = pyreal_horizon::ui::panels::marks::carried(state)
            .find(|i| i.id == ObjectId(0x8000_0408))
            .cloned()
            .expect("carried");
        h.ui.windows.marks.set(&it, Some(Mark::Junk));
    };
    let mut h = harness();
    h.ui.windows.open(WindowId::Inventory, 0.0);
    settle(&mut h, &state);
    let r = window(&h, WindowId::Inventory);
    write_part(&dir.join("marks-inventory-before.png"), &h, &art, r);
    mark(&mut h, &state);
    settle(&mut h, &state);
    write_part(&dir.join("marks-inventory-marked.png"), &h, &art, r);
    h.ui.windows.pack_search = "helm".into();
    settle(&mut h, &state);
    write_part(&dir.join("marks-inventory-search-helm.png"), &h, &art, r);
    h.ui.windows.pack_search = "junk".into();
    settle(&mut h, &state);
    write_part(&dir.join("marks-inventory-search-junk.png"), &h, &art, r);
    h.ui.windows.pack_search.clear();
    let mut drop = vec![pyreal_client_contract::UiRequest::PutInWorld(ObjectId(
        0x8000_0400,
    ))];
    h.ui.windows.hold_keep(&state, &mut drop);
    settle(&mut h, &state);
    write(&dir.join("marks-keep-question.png"), &h, &art);

    state.shop = Some(ShopView {
        open: true,
        sell_mode: true,
        ..ShopView::default()
    });
    let mut h = harness();
    h.frame(&state);
    mark(&mut h, &state);
    settle(&mut h, &state);
    let r = window(&h, WindowId::Vendor);
    write_part(&dir.join("marks-vendor-sell.png"), &h, &art, r);
}

/// Not a claim: floating combat numbers over a creature and over the character a moment into a
/// fight, into `<PYREAL_UI_SHOTS>/combat-numbers-<ms>.png` for each age in `PYREAL_UI_AGES`
/// (milliseconds, default `150,500,1000`). The creature's head stands at `PYREAL_UI_OVER` (`x,y`,
/// default `957,486`) and the character's at `PYREAL_UI_PLAYER` (default `960,900`).
/// `PYREAL_UI_BACKDROP` puts a picture of the world under them.
#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder"]
fn draw_combat_numbers_for_a_person_to_look_at() {
    use pyreal_client_model::chat::text_type;
    use pyreal_horizon::draw::{Rect, Spot};
    use pyreal_horizon::ui::game::{BlipKind, ChatLine, Nameplate, Relation, Target};
    use pyreal_primitives::ObjectId;
    let Some(dir) = std::env::var_os("PYREAL_UI_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    let point = |name: &str, default: (f32, f32)| {
        std::env::var(name).ok().map_or(default, |v| {
            let (x, y) = v.split_once(',').expect("x,y");
            (x.parse().expect("x"), y.parse().expect("y"))
        })
    };
    let over = point("PYREAL_UI_OVER", (957.0, 486.0));
    let player_at = point("PYREAL_UI_PLAYER", (960.0, 900.0));
    let (player, flamma) = (ObjectId(0x5000_0001), ObjectId(0x8300_0801));
    let mut state = GameState {
        in_world: true,
        connected: true,
        name: "Tester".into(),
        player_id: Some(player),
        radar_range: 75.0,
        target: Some(Target {
            id: flamma,
            name: "Flamma".into(),
            relation: Relation::Engaged,
            ..Target::default()
        }),
        nameplates: vec![Nameplate {
            id: flamma,
            name: "Flamma".into(),
            x: over.0,
            y: over.1,
            kind: BlipKind::Selected,
            selected: true,
            bounds: Rect::new(over.0 - 40.0, over.1, 80.0, 200.0),
            distance: Some(12.0),
            nearness: 1.0,
        }],
        ..GameState::default()
    };
    let lines = [
        (
            text_type::COMBAT_SELF,
            "You slash Flamma for 38 points of slashing damage!",
        ),
        (
            text_type::MAGIC,
            "You freeze Flamma for 112 points with Frost Bolt VI.",
        ),
        (text_type::COMBAT_SELF, "Flamma evaded your attack."),
        (
            text_type::MAGIC,
            "Critical hit! You freeze Flamma for 246 points with Frost Bolt VI.",
        ),
        (
            text_type::COMBAT_ENEMY,
            "Flamma burns your chest for 27 points of fire damage!",
        ),
        (
            text_type::MAGIC,
            "You cast Heal Self VI and restore 80 points of your health.",
        ),
    ];
    let ages: Vec<u32> = std::env::var("PYREAL_UI_AGES")
        .unwrap_or_else(|_| "150,500,1000".into())
        .split(',')
        .map(|a| a.trim().parse().expect("milliseconds"))
        .collect();
    for age in ages {
        let mut h = Harness::with_built_in_art();
        // A line every tenth of a second, the last `age` ago.
        for (ty, text) in lines {
            state.new_chat = vec![ChatLine {
                chat_type: u8::try_from(ty).expect("a chat type"),
                text: text.into(),
                window: 0,
            }];
            h.frame(&state);
            state.new_chat.clear();
            for _ in 0..5 {
                h.frame(&state);
            }
        }
        for _ in 0..(age * 60 / 1000) {
            h.frame(&state);
        }
        h.list.follow(|id, _| {
            let (x, y) = if id == player { player_at } else { over };
            Some(Spot::at(x, y))
        });
        let art = Arc::clone(&h.ui.art);
        write(&dir.join(format!("combat-numbers-{age}.png")), &h, &art);
    }
}

#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder"]
fn draw_the_death_recap_for_a_person_to_look_at() {
    use pyreal_client_model::chat::text_type;
    use pyreal_horizon::ui::game::{ChatLine, Vital};
    let Some(dir) = std::env::var_os("PYREAL_UI_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    let world = |deaths| GameState {
        in_world: true,
        connected: true,
        name: "Tester".into(),
        address: "127.0.0.1:9000".into(),
        here: Some((40.4, 34.5)),
        health: Vital {
            current: 100,
            max: 100,
        },
        deaths: Some(deaths),
        ..GameState::default()
    };
    // Each picture's name, then its lines: chat type, text and the health after it.
    type Lines = &'static [(u32, &'static str, u32)];
    let fights: [(&str, Lines); 2] = [
        (
            "death-recap",
            &[
                (
                    text_type::COMBAT_ENEMY,
                    "Mosswart Elder slashes your chest for 24 points of slashing damage!",
                    76,
                ),
                (
                    text_type::MAGIC,
                    "Mosswart Elder scorches you for 41 points with Flame Bolt IV.",
                    35,
                ),
                (
                    text_type::MAGIC,
                    "You cast Heal Self VI and restore 52 points of your health.",
                    87,
                ),
                (
                    text_type::MAGIC,
                    "Drudge Sorcerer casts Drain Health and drains 18 points of your health.",
                    69,
                ),
                (
                    text_type::COMBAT_ENEMY,
                    "Mosswart Elder cuts your hand for 15 points of slashing damage!",
                    54,
                ),
                (
                    text_type::MAGIC,
                    "You receive 9 points of periodic nether damage.",
                    45,
                ),
                (
                    text_type::DEFAULT,
                    "Mosswart Elder cleaves you in twain!",
                    0,
                ),
            ],
        ),
        (
            "death-recap-no-hits",
            &[(text_type::DEFAULT, "You died!", 0)],
        ),
    ];
    for (name, lines) in fights {
        let mut h = Harness::with_built_in_art();
        h.ui.options.death_recap_opens = true;
        let mut state = world(3);
        settle(&mut h, &state);
        // An earlier death, a few minutes before, for the list of deaths.
        if name == "death-recap" {
            state.new_chat = vec![ChatLine {
                chat_type: u8::try_from(text_type::DEFAULT).expect("a chat type"),
                text: "Drudge Skulker splits you apart!".into(),
                window: 0,
            }];
            state.deaths = Some(4);
            h.frame(&state);
            state.new_chat.clear();
            for _ in 0..60 * 150 {
                h.frame(&state);
            }
        }
        let died = state.deaths.unwrap_or(3) + 1;
        for (i, (ty, text, health)) in lines.iter().enumerate() {
            state.health.current = *health;
            if i + 1 == lines.len() {
                state.deaths = Some(died);
            }
            state.new_chat = vec![ChatLine {
                chat_type: u8::try_from(*ty).expect("a chat type"),
                text: (*text).into(),
                window: 0,
            }];
            h.frame(&state);
            state.new_chat.clear();
            for _ in 0..40 {
                h.frame(&state);
            }
        }
        // The items lost, a few seconds after the death, and the vitae penalty.
        if name == "death-recap" {
            state.new_chat = vec![ChatLine {
                chat_type: 0,
                text: "You've lost 312 Pyreals, your 4 Mana Charges, and your Iron Kite Shield!"
                    .into(),
                window: 0,
            }];
            h.frame(&state);
            state.new_chat.clear();
            state.vitae = Some(0.95);
        }
        state.here = Some((40.4, 33.0));
        state.health.current = 100;
        settle(&mut h, &state);
        let art = Arc::clone(&h.ui.art);
        let k = h.ui.layout_scale(h.screen);
        let window =
            h.ui.windows
                .rects(k)
                .into_iter()
                .find(|(id, _)| *id == WindowId::Journal)
                .map(|(_, r)| r.inset(-12.0));
        write_part(&dir.join(format!("{name}.png")), &h, &art, window);
        write(&dir.join(format!("{name}-screen.png")), &h, &art);
    }
}
