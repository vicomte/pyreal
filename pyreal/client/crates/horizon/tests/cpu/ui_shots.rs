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
const OWN_MENUS: [&str; 2] = ["Interface Scale", "Status Bar Style"];

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
    let mut px = vec![[24.0f32, 26.0, 30.0]; sw * sh];
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
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let rgb: Vec<u8> = (y0..y0 + ph)
        .flat_map(|y| px[y * sw + x0..y * sw + x0 + pw].iter())
        .flat_map(|p| p.map(|c| c.clamp(0.0, 255.0) as u8))
        .collect();
    let (sw, sh) = (pw, ph);
    let file = std::fs::File::create(path).expect("the picture's file");
    #[allow(clippy::cast_possible_truncation)]
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), sw as u32, sh as u32);
    enc.set_color(png::ColorType::Rgb);
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
/// `PYREAL_UI_HUD_HERE` (`ns,ew`, default `40.4,34.5`).
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
    if waypoint.is_some() {
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
    let art = Arc::new(Art::new(Arc::new(
        pyreal_horizon::pieces::Pieces::built_in(),
    )));
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
    let mut h = Harness {
        ui: HorizonUi::new(Arc::clone(&art), options),
        list: DrawList::default(),
        input: InputFrame::default(),
        screen: size,
    };
    h.frame(&state);
    if let Some((ns, ew)) = waypoint {
        h.ui.windows
            .waypoints
            .set(pyreal_horizon::ui::waypoint::Waypoint { ns, ew, name: None });
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
