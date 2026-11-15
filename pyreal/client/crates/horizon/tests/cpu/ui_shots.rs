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
        for y in (r.y.max(0.0) as usize)..(r.bottom().min(sh as f32).max(0.0) as usize) {
            for x in (r.x.max(0.0) as usize)..(r.right().min(sw as f32).max(0.0) as usize) {
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
/// `PYREAL_UI_HUD_FELLOWS` puts that many members (1 to 9) in the fellowship window.
/// `PYREAL_UI_HUD_LAYOUT` opens the HUD Layout window; `PYREAL_UI_HUD_KEYS` names the keys, and
/// `PYREAL_UI_HUD_SPELLS` puts that many spells in the first tab; `PYREAL_UI_HUD_STYLE` names
/// the status bar style by its settings word.
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
    let state = GameState {
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
        ..GameState::default()
    };
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
    if env("PYREAL_UI_HUD_LAYOUT").is_some() {
        h.ui.windows.open(WindowId::Layout, 0.0);
    } else if env("PYREAL_UI_HUD_BARE").is_none() {
        h.ui.windows.open(WindowId::Social, 0.0);
    }
    settle(&mut h, &state);
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
#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder and have the dats"]
fn draw_the_creation_pages_for_a_person_to_look_at() {
    use pyreal_horizon::ui::creation::{Creation, Step};
    use pyreal_primitives::EraId;
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
            let mut h = Harness {
                ui: HorizonUi::new(Arc::clone(&art), Default::default()),
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
            for step in [Step::Profession, Step::Attributes, Step::Skills, Step::Name] {
                if let Some(c) = h.ui.pregame.creation.as_mut() {
                    c.step = step;
                }
                settle(&mut h, &state);
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
/// groups), `spellbook-school-hidden.png` (a tab whose school the filter hides),
/// `shop-row-cost.png` (100 arrows chosen at a vendor), `allegiance-rank.png` (a rank a spell
/// raises) and `exp-cap.png` (the EXP bar at the level cap).
#[test]
#[ignore = "an instrument: set PYREAL_UI_SHOTS to a folder"]
fn draw_the_skill_groups_spell_filter_shop_row_rank_and_exp_cap_for_a_person_to_look_at() {
    use pyreal_client_contract::view::{AllegianceEntry, AllegianceRoster, ShopRow, ShopView};
    use pyreal_horizon::draw::Rect;
    use pyreal_horizon::ui::game::{Modifiers, Skill, Spell};
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

    // A spellbook tab whose school the filter hides.
    let mut state = GameState {
        combat_mode: pyreal_client_contract::combat_mode::MAGIC,
        spells: vec![Spell {
            charge: None,
            id: 7,
            name: "Flame Bolt I".into(),
            school: 1,
            level: 1,
            ac_icon: 0,
            icon_power: 0,
            bitfield: 0,
        }],
        ..in_world()
    };
    state.world_services.spell_filters = pyreal_client_contract::spellbook::DEFAULT_SPELL_FILTERS
        & !pyreal_client_contract::spellbook::school_mask(1);
    let mut h = harness();
    h.frame(&state);
    h.ui.windows.open(WindowId::Actions, 0.0);
    settle(&mut h, &state);
    let r = window(&h, WindowId::Actions);
    write_part(&dir.join("spellbook-school-hidden.png"), &h, &art, Some(r));

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
