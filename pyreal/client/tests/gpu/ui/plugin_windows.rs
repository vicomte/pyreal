//! Plugin windows on the desktop, under Horizon: a window a plugin declares is drawn in Horizon's
//! art and a click on its button reaches the plugin, whole at each scale; and the plugin panel as
//! a window.
//! Fixture: a headless `App` on the retail dats with a plugin host installed and plugin packages
//! built in memory; the pointer is driven through the window's own events, and the frame is read
//! back from the device.

#![cfg(gpu)]

use crate::common::client_dir;
use crate::common::gpu_lock;

use pyreal_client::app::App;
use pyreal_client_runtime::config::Config;
use pyreal_input::host::HostEvent;
use pyreal_input::keys::MouseButton;
use pyreal_plugin_api::HostCapability;
use pyreal_plugin_host::{
    HostConfig, MemoryStorage, Package, PluginHook, PluginHost, StaticSource,
};

const MAIN: &str = r#"
local win = ui.window{ id = "main", title = "Plugin",
    ui.label{ id = "said", text = "no" },
    ui.button{ id = "go", text = "Go" },
    ui.toggle{ id = "auto", text = "Auto", value = true },
    ui.tabs{ id = "pages", selected = 2,
        ui.group{ title = "First", ui.label{ text = "first page" } },
        ui.group{ title = "Second",
            ui.number{ id = "range", text = "Range", value = 8, min = 0, max = 10, step = 2 },
            ui.choice{ id = "mode", text = "Mode", options = { "stop", "recall" }, value = "stop" },
        },
    },
}
local hud = ui.hud{ id = "hud", anchor = "top_right", ui.label{ id = "line", text = "HUD here" } }
on("ui.clicked", function(e) if e.id == "go" then win:get("said").text = "yes" end end)
on("ui.opened", function(e) hud:show() end)
"#;

const HUD: &str = r#"
ui.hud{ id = "hud", anchor = "top_right", ui.label{ id = "line", text = "HUD here" } }
"#;

/// A window of every kind of box a plugin window holds: fields, a disabled field, a list in a
/// group, a picture's place.
const RICH: &str = r#"
local rows = {}
for i = 1, 12 do rows[i] = { "Pyreal Mote " .. i, i * 100, "salvage" } end
local win = ui.window{ id = "main", title = "Plugin",
    ui.label{ id = "said", text = "Status: idle" },
    ui.row{ ui.button{ id = "go", text = "Start" }, ui.button{ id = "stop", text = "Stop" },
        ui.toggle{ id = "auto", text = "Auto", value = true } },
    ui.number{ id = "range", text = "Range", value = 8, min = 0, max = 10 },
    ui.choice{ id = "mode", text = "Mode", options = { "stop", "recall" }, value = "stop" },
    ui.input{ id = "name", text = "Name", value = "Tester" },
    ui.input{ id = "off", text = "Off", value = "x", enabled = false },
    ui.group{ title = "Loot",
        ui.list{ id = "loot", columns = { "Item", "Value", "Rule" }, rows = rows, height = 5 } },
    ui.image{ id = "map", src = "map.png", text = "no map", width = 12, height = 3 },
}
on("ui.clicked", function(e) if e.id == "go" then win:get("said").text = "yes" end end)
"#;

fn host() -> PluginHost {
    host_of(MAIN)
}

fn host_of(main: &str) -> PluginHost {
    let files = [
        (
            "plugin.toml".to_owned(),
            // The fixture has no character in the world: the plugin is active at any screen.
            b"id = \"test.window\"\nname = \"Window\"\nversion = \"1.0.0\"\napi = \"0.2\"\ncontexts = [\"pregame\", \"world\"]\n"
                .to_vec(),
        ),
        ("main.lua".to_owned(), main.as_bytes().to_vec()),
    ];
    let package = Package {
        origin: "test".into(),
        files: files.into_iter().collect(),
    };
    let config = HostConfig::new(
        "dereth-desktop",
        vec![
            HostCapability::Core,
            HostCapability::UiWindow,
            HostCapability::UiHud,
        ],
    );
    let mut host = PluginHost::new(
        config,
        Box::new(pyreal_plugin_lua::LuauEngine),
        Box::new(StaticSource {
            packages: vec![package],
        }),
        Box::new(MemoryStorage::default()),
    );
    host.load_all();
    host
}

fn hook(app: &mut App) -> &mut PluginHook {
    app.frame_hook_mut()
        .and_then(|h| h.as_any_mut().downcast_mut::<PluginHook>())
        .expect("the plugin host is installed")
}

fn said(app: &mut App) -> String {
    hook(app)
        .host
        .window("test.window", "main")
        .map(|w| w.tree.dump())
        .unwrap_or_default()
}

fn main_open(app: &mut App) -> Option<bool> {
    hook(app).host.window("test.window", "main").map(|w| w.open)
}

/// A headless `App` on the retail dats on a display `size` pixels across and down, with `host`
/// installed, eight frames run and each plugin's main window opened from the plugin bar.
fn app_at(host: PluginHost, (width, height): (u32, u32)) -> App {
    assert!(
        pyreal_dat::testing::have_dats(),
        "the retail dats are this file's fixture: none at {} -- set PYREAL_TEST_DAT_DIR",
        client_dir().display()
    );
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        world: false,
        character: false,
        dat_dir: client_dir(),
        width,
        height,
        ..Config::default()
    })
    .unwrap_or_else(|e| panic!("a headless App on a GPU device: {e:?}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the UI shell starts: {e:?}"));
    app.load_first_pixel_scene()
        .unwrap_or_else(|e| panic!("the first scene loads: {e:?}"));
    app.install_frame_hook(Box::new(PluginHook { host }));
    for _ in 0..8 {
        app.frame();
    }
    // The player opens each plugin's main window from the plugin bar: a plugin's windows open
    // only when the player asks.
    for entry in hook(&mut app).host.bar() {
        if entry.window.is_some() && !entry.open {
            hook(&mut app)
                .host
                .toggle_window(&entry.id)
                .unwrap_or_else(|e| panic!("{} is on the bar: {e}", entry.id));
        }
    }
    for _ in 0..4 {
        app.frame();
    }
    assert!(app.shell().horizon().is_some(), "the interface is up");
    app
}

/// A left click at `(x, y)`, through the window's events, a frame for each.
fn click(app: &mut App, (x, y): (f32, f32)) {
    app.queue_window_event(HostEvent::CursorMoved {
        x: f64::from(x),
        y: f64::from(y),
    });
    app.frame();
    for pressed in [true, false] {
        app.queue_window_event(HostEvent::MouseInput {
            button: MouseButton::Left,
            pressed,
        });
        app.frame();
    }
    app.frame();
}

/// The panel titled `title` the shown interface noted last frame: where it is, and its first
/// control.
struct Panel {
    rect: (f32, f32, f32, f32),
    first: (f32, f32),
}

fn panel(app: &App, title: &str) -> Panel {
    let snap = app
        .kit_controls()
        .expect("the interface noted its controls");
    let (i, layer) = snap
        .layers
        .iter()
        .enumerate()
        .find(|(_, l)| l.name == title)
        .unwrap_or_else(|| {
            panic!(
                "no panel {title:?} is drawn: {:?}",
                snap.layers.iter().map(|l| &l.name).collect::<Vec<_>>()
            )
        });
    let first = snap
        .targets
        .iter()
        .find(|t| t.layer == i + 1)
        .map(|t| (t.rect.x + t.rect.w / 2.0, t.rect.y + t.rect.h / 2.0))
        .expect("its first control is noted");
    let r = layer.rect;
    Panel {
        rect: (r.x, r.y, r.w, r.h),
        first,
    }
}

/// The frame as the device drew it, kept as a PNG under `PYREAL_TEST_PLUGIN_WINDOWS_DIR` when it
/// is set: width, height and BGRA bytes.
fn capture(app: &mut App, name: &str) -> (u32, u32, Vec<u8>) {
    let (w, h, bgra) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame reads back");
    if let Ok(dir) = std::env::var("PYREAL_TEST_PLUGIN_WINDOWS_DIR") {
        let rgba: Vec<u8> = bgra
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], 255])
            .collect();
        let file = std::fs::File::create(std::path::Path::new(&dir).join(format!("{name}.png")))
            .expect("png file");
        let mut png = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        png.set_color(png::ColorType::Rgba);
        png.set_depth(png::BitDepth::Eight);
        png.write_header()
            .expect("png header")
            .write_image_data(&rgba)
            .expect("png pixels");
    }
    (w, h, bgra)
}

/// The colour at `(x, y)` of a captured frame `w` wide.
fn px(frame: &(u32, u32, Vec<u8>), x: f32, y: f32) -> (u8, u8, u8) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x, y) = (x as u32, y as u32);
    let i = ((y * frame.0 + x) * 4) as usize;
    (frame.2[i + 2], frame.2[i + 1], frame.2[i])
}

#[test]
fn under_horizon_a_plugin_window_is_drawn_in_its_art_its_button_reaches_the_plugin_and_closed_it_is_gone(
) {
    let _gpu = gpu_lock();
    let mut app = app_at(host(), (800, 600));
    let window = panel(&app, "Plugin");
    let open = capture(&mut app, "plugin-window-horizon");
    click(&mut app, window.first);
    assert!(
        said(&mut app).contains("label #said text=\"yes\""),
        "{}",
        said(&mut app)
    );
    // The fixture has no character: Horizon is before the world, where the plugins' windows are
    // drawn over its screens. Closed (as the bar's button closes it), the window is gone.
    hook(&mut app)
        .host
        .toggle_window("test.window")
        .expect("the window closes");
    assert_eq!(main_open(&mut app), Some(false));
    for _ in 0..4 {
        app.frame();
    }
    let closed = capture(&mut app, "plugin-window-horizon-closed");
    // Where the window stood, the frame is not the same once it is closed.
    let (x, y, w, h) = window.rect;
    let mut changed = 0;
    let mut seen = 0;
    for j in 1..10_u8 {
        for i in 1..10_u8 {
            let (sx, sy) = (x + w * f32::from(i) / 10.0, y + h * f32::from(j) / 10.0);
            seen += 1;
            let (a, b) = (px(&open, sx, sy), px(&closed, sx, sy));
            let d = |x: u8, y: u8| u16::from(x.abs_diff(y));
            if d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2) > 24 {
                changed += 1;
            }
        }
    }
    assert!(
        changed * 2 > seen,
        "the window was drawn where it stood: {changed} of {seen} samples changed when it closed"
    );
}

/// Under Horizon, on a display of 800 by 600 (its layouts shrunk to fit), of 1920 by 1080 and of
/// 3840 by 2160 (drawn at twice the size), a plugin window holding fields, a list in a group and a
/// picture's place is drawn whole on the screen, and its button, where the window is drawn,
/// reaches the plugin.
#[test]
fn under_horizon_a_plugin_window_of_fields_and_lists_is_drawn_whole_at_each_scale() {
    let _gpu = gpu_lock();
    for (w, h) in [(800_u32, 600_u32), (1920, 1080), (3840, 2160)] {
        let mut app = app_at(host_of(RICH), (w, h));
        let window = panel(&app, "Plugin");
        let (x, y, ww, wh) = window.rect;
        #[allow(clippy::cast_precision_loss)]
        let (sw, sh) = (w as f32, h as f32);
        assert!(
            x >= 0.0 && y >= 0.0 && x + ww <= sw && y + wh <= sh,
            "{w}x{h}: the window is on the screen: {:?}",
            window.rect
        );
        capture(&mut app, &format!("plugin-window-horizon-{w}x{h}"));
        click(&mut app, window.first);
        assert!(
            said(&mut app).contains("label #said text=\"yes\""),
            "{w}x{h}: {}",
            said(&mut app)
        );
    }
}

/// The plugin panel (`/plugin`) is drawn as a window in Horizon's art.
#[test]
fn the_plugin_panel_is_drawn_as_a_window() {
    let _gpu = gpu_lock();
    let mut app = app_at(host_of(HUD), (800, 600));
    let name = "plugin-panel-horizon";
    let before = capture(&mut app, &format!("{name}-before"));
    hook(&mut app).host.show_panel();
    for _ in 0..4 {
        app.frame();
    }
    let window = panel(&app, "Plugins");
    let frame = capture(&mut app, name);
    let (x, y, w, h) = window.rect;
    #[allow(clippy::cast_precision_loss)]
    let (sw, sh) = (frame.0 as f32, frame.1 as f32);
    assert!(
        x >= 0.0 && y >= 0.0 && x + w <= sw && y + h <= sh,
        "the panel is on the screen: {:?}",
        window.rect
    );
    // Where the panel stands, the frame is not what it was before it opened.
    let mut changed = 0;
    let mut seen = 0;
    for j in 1..10_u8 {
        for i in 1..10_u8 {
            let (sx, sy) = (x + w * f32::from(i) / 10.0, y + h * f32::from(j) / 10.0);
            seen += 1;
            let (a, b) = (px(&before, sx, sy), px(&frame, sx, sy));
            let d = |x: u8, y: u8| u16::from(x.abs_diff(y));
            if d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2) > 24 {
                changed += 1;
            }
        }
    }
    assert!(
        changed * 2 > seen,
        "the panel is drawn where it stands: {changed} of {seen} samples changed"
    );
}
