//! The plugins in the Horizon interface. A plugin's hotkey, its marks in the world, its windows,
//! HUDs and commands work there; a file a plugin asks for is asked in a dialog of the interface's
//! own, answered by its buttons; and a press on the interface's map reaches the plugins.
//!
//! Fixture: the client at the Horizon interface's game screen (with no art) on the retail dats,
//! on a device-free presentation that builds the world, with a body loaded, and a plugin host
//! with the Luau engine installed as the frame hook. Input is the window's: events queued as the
//! window queues them.

use super::*;
use crate::{app::CoreApp, platform::host::NullHost};
use pyreal_input::host::HostEvent;
use pyreal_input::keys::{Key, MouseButton};
use pyreal_plugin_host::{HostConfig, MemoryStorage, Package, PluginHook, PluginHost};

const CTRL: Key = Key::new(0x11, 0x1D);
const F5: Key = Key::new(0x74, 0x3F);
const ESCAPE: Key = Key::new(0x1B, 0x01);

/// A plugin with a hotkey (Ctrl+F5 runs its `/hk`, which says so) and a marker in the world.
const KEYS: &str = r#"
commands.add("hk", "run by a hotkey", function() print("hotkey") end)
ui.window({ id = "main", title = "Keys", ui.hotkey({ id = "go", command = "hk", key = "Ctrl+F5" }) })
markers.world:add({ position = { ns = 42.25, ew = 33.8 }, label = "Here" })
"#;

/// A plugin with a window holding a button (a click says so) and a HUD with a line, which opens
/// as the player opens the window.
const WINDOWED: &str = r#"
ui.window({ id = "main", title = "Windowed", ui.button({ id = "go", text = "Go" }) })
local hud = ui.hud({ id = "hud", anchor = "bottom_right", ui.label({ text = "On the HUD" }) })
on("ui.clicked", function(e) print("clicked " .. e.id) end)
on("ui.opened", function(e) hud:show() end)
"#;

/// A plugin that asks the player for a file as it loads, and says what it was given.
const PICKER: &str = r#"
task.spawn(function() print("file", pawait(files.import({ prompt = "a meta to run" }))) end)
"#;
const READS: &str = "[permissions]\n\"files.import\" = \"Reads a meta.\"\n";

fn package(id: &str, main: &str, extra: &str) -> Package {
    let manifest = format!(
        "id = \"{id}\"\nname = \"Test {id}\"\nversion = \"1.0.0\"\napi = \"0.6\"\ncontexts = [\"pregame\", \"world\"]\n{extra}"
    );
    Package {
        origin: id.to_owned(),
        files: [
            ("plugin.toml".to_owned(), manifest.into_bytes()),
            ("main.lua".to_owned(), main.as_bytes().to_vec()),
        ]
        .into_iter()
        .collect(),
    }
}

/// A desktop's plugin host over `packages`.
fn plugin_host(packages: Vec<Package>) -> PluginHost {
    use pyreal_plugin_api::HostCapability as C;
    let config = HostConfig::new(
        "dereth-desktop",
        vec![
            C::Core,
            C::UiWindow,
            C::UiHud,
            C::OverlayWorld,
            C::MapClicks,
            C::RetailUi,
        ],
    );
    let mut host = PluginHost::new(
        config,
        Box::new(pyreal_plugin_lua::LuauEngine),
        Box::new(pyreal_plugin_host::StaticSource { packages }),
        Box::new(MemoryStorage::default()),
    );
    host.load_all();
    host
}

struct Client {
    app: CoreApp<NullHost>,
    shell: ClientShell<NullHost>,
    state: std::path::PathBuf,
}

impl Client {
    /// The client at the Horizon interface's game screen with a body in the world.
    fn new(tag: &str) -> Self {
        let state =
            std::env::temp_dir().join(format!("pyreal-plugins-{tag}-{}", std::process::id()));
        let cfg = pyreal_client_runtime::config::Config {
            headless: true,
            connect: false,
            sound: false,
            world: false,
            width: 800,
            height: 600,
            dat_dir: pyreal_dat::testing::dat_dir(),
            preferences_file: state.join("prefs.ini"),
            ..Default::default()
        };
        let mut app = CoreApp::<NullHost>::bring_up_with_store(
            cfg,
            None,
            |_| Ok(pyreal_client_runtime::app::Platform::headless(800, 600)),
            |_, _, _, _| {
                Ok(Box::new(
                    pyreal_client_runtime::sim_present::SimPresentation::new(800, 600),
                ))
            },
        )
        .expect("real tables and a device-free presentation");
        let mut shell =
            ClientShell::with_window_events(app.window.raw_handle(), Default::default());
        app.start_shell(&mut shell).expect("real input and UI");
        for _ in 0..2 {
            assert!(app.frame(&mut shell));
        }
        let mut c = Self { app, shell, state };
        c.install_horizon();
        let files = std::sync::Arc::clone(c.app.probe().dat_store());
        let cfg = pyreal_client_runtime::scene::SceneConfig {
            land_radius: 0,
            scenery_radius: 0,
            ..Default::default()
        };
        c.app
            .probe_mut()
            .load_world(&files, cfg)
            .expect("a world with the player's body");
        c.frame();
        c
    }

    /// The Horizon interface brought up again at its game screen, with no art behind it.
    fn install_horizon(&mut self) {
        let mut cx = self.app.ui_context();
        let mut ui = pyreal_horizon::runtime::HorizonFrontEnd::new(
            std::sync::Arc::new(pyreal_horizon::art::Art::empty()),
            pyreal_horizon::options::HorizonOptions {
                screen: pyreal_horizon::options::StartScreen::Game,
                ..Default::default()
            },
            None,
        );
        ui.start(&mut cx);
        self.shell.horizon.ui = Some(ui);
        self.shell.enter_horizon(&mut cx);
    }

    fn install(&mut self, host: PluginHost) {
        self.app.install_frame_hook(Box::new(PluginHook { host }));
    }

    fn host(&mut self) -> &mut PluginHost {
        &mut self
            .app
            .frame_hook_mut()
            .and_then(|h| h.as_any_mut().downcast_mut::<PluginHook>())
            .expect("the plugin host")
            .host
    }

    fn console(&mut self, id: &str) -> Vec<String> {
        self.host().console(id).unwrap_or_default()
    }

    fn frame(&mut self) {
        assert!(self.app.frame(&mut self.shell));
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.frame();
        }
    }

    /// The player opens plugin `id`'s main window from the plugin bar: a plugin's window opens
    /// only when the player asks.
    fn open(&mut self, id: &str) {
        self.host().toggle_window(id).expect("on the plugin bar");
        self.frames(2);
    }

    fn event(&mut self, event: HostEvent) {
        self.shell.queue_window_event(event);
    }

    fn key(&mut self, key: Key, pressed: bool) {
        self.event(HostEvent::KeyboardInput {
            key,
            pressed,
            text: None,
        });
    }

    /// Ctrl+F5, pressed and let go, one frame for each.
    fn ctrl_f5(&mut self) {
        self.key(CTRL, true);
        self.key(F5, true);
        self.frame();
        self.key(F5, false);
        self.key(CTRL, false);
        self.frames(2);
    }

    /// A press and a release of the left button at `(x, y)`.
    fn click(&mut self, (x, y): (f64, f64)) {
        self.event(HostEvent::CursorMoved { x, y });
        self.frame();
        for pressed in [true, false] {
            self.event(HostEvent::MouseInput {
                button: MouseButton::Left,
                pressed,
            });
            self.frame();
        }
    }

    /// How many marks the world was last given to draw.
    fn marks(&self) -> u64 {
        self.app
            .present
            .as_any()
            .downcast_ref::<pyreal_client_runtime::sim_present::SimPresentation>()
            .expect("the device-free presentation")
            .device()
            .counts()
            .world_marks
    }

    /// Where the plugin host's question is drawn: Horizon's own top tier.
    fn asking(&self) -> &pyreal_plugin_ui::host::Asking {
        &self
            .shell
            .horizon
            .ui
            .as_ref()
            .expect("Horizon")
            .ui
            .windows
            .plugins
            .host
            .asking
    }

    /// The id of the host's question the interface draws.
    fn question_shown(&self) -> Option<u64> {
        self.asking().id()
    }

    /// The player's press on the first (`yes`) or the last button of the host's question.
    fn answer(&mut self, yes: bool) {
        let buttons = &self.asking().drawn.buttons;
        let r = if yes {
            buttons[0]
        } else {
            buttons[buttons.len() - 1]
        };
        self.click((f64::from(r.x + r.w / 2.0), f64::from(r.y + r.h / 2.0)));
        self.frames(3);
    }

    /// The controls the interface noted last frame for the pointer and the pad, with the panels
    /// they are in.
    fn controls(
        &self,
    ) -> (
        Vec<pyreal_ui_kit::nav::Target>,
        Vec<pyreal_ui_kit::nav::Layer>,
    ) {
        let snap = &self
            .shell
            .horizon
            .ui
            .as_ref()
            .expect("Horizon")
            .frame_input
            .nav
            .last;
        (snap.targets.clone(), snap.layers.clone())
    }

    /// Where the plugin window's button is: the control in the window's panel.
    fn plugin_button(&self) -> pyreal_ui_kit::draw::Rect {
        let (targets, layers) = self.controls();
        let at = 1 + layers
            .iter()
            .position(|l| l.name == "Windowed" || l.name.starts_with("Plugin"))
            .expect("the plugin's window is drawn");
        targets
            .iter()
            .find(|t| t.layer == at)
            .map(|t| t.rect)
            .expect("its button is noted")
    }

    fn finish(self) {
        let Self {
            app,
            mut shell,
            state,
        } = self;
        app.shutdown(&mut shell);
        let _ = std::fs::remove_dir_all(&state);
    }
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_plugins_hotkey_runs_its_command_and_its_mark_is_drawn_in_the_world() {
    let mut c = Client::new("keys");
    c.install(plugin_host(vec![package("test.keys", KEYS, "")]));
    c.frames(5);
    assert_eq!(c.marks(), 1, "the plugin's marker is drawn in the world");
    let before = c.console("test.keys").len();
    c.ctrl_f5();
    assert_eq!(
        c.console("test.keys")[before..],
        ["hotkey"],
        "Ctrl+F5 ran the plugin's command once"
    );
    c.finish();
}

/// Beside the host's question a plugin's marks in the world are still drawn.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn beside_the_hosts_question_a_plugins_marks_in_the_world_are_still_drawn() {
    let mut c = Client::new("beside");
    c.install(plugin_host(vec![
        package("test.keys", KEYS, ""),
        package("test.picker", PICKER, READS),
    ]));
    c.frames(4);
    let q = c.host().question().expect("test.picker's file");
    assert_eq!(c.marks(), 1, "the mark beside the question");
    c.host().answer_question(q.id, false).expect("answered");
    c.frames(3);
    assert_eq!(c.marks(), 1, "and after it");
    c.finish();
}

/// The game's panels a plugin hides or moves are hidden and moved in Horizon, as it has them.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_plugins_hidden_and_moved_panels_reach_horizon() {
    let mut c = Client::new("retail-ui");
    c.install(plugin_host(vec![package(
        "test.panels",
        "retail_ui.hide('chat') retail_ui.move('radar', 100, 200)",
        "[permissions]
\"retail_ui\" = \"Tidies the screen.\"
",
    )]));
    c.frames(5);
    let ui = &c.shell.horizon.ui.as_ref().expect("Horizon").ui;
    assert!(
        ui.hud.retail_hidden.contains("chat"),
        "Horizon's log window"
    );
    assert_eq!(ui.hud.layout.pinned.get("minimap"), Some(&(100.0, 200.0)));
    c.finish();
}

/// Under Horizon the plugins' commands are handed to the interface for its cross hotbars, and one
/// run from a slot reaches its plugin as the player's own.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn under_horizon_a_plugins_command_is_offered_to_the_cross_hotbars_and_one_run_from_a_slot_reaches_it(
) {
    let mut c = Client::new("slots");
    c.install(plugin_host(vec![package("test.keys", KEYS, "")]));
    c.frames(5);
    fn seat(c: &mut Client) -> &mut pyreal_horizon::ui::plugins::PluginSeat {
        &mut c
            .shell
            .horizon
            .ui
            .as_mut()
            .expect("Horizon")
            .ui
            .windows
            .plugins
    }
    let offered: Vec<(String, String)> = seat(&mut c)
        .frame
        .commands
        .iter()
        .map(|e| (e.plugin.clone(), e.verb.clone()))
        .collect();
    assert_eq!(offered, [("test.keys".to_owned(), "hk".to_owned())]);
    let before = c.console("test.keys").len();
    seat(&mut c).push_input(pyreal_plugin_ui::Input::Command {
        plugin: "test.keys".into(),
        verb: "hk".into(),
    });
    c.frames(3);
    assert_eq!(c.console("test.keys")[before..], ["hotkey"], "it ran once");
    c.finish();
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_plugins_file_is_asked_for_in_a_dialog_its_buttons_and_escape_answer() {
    let mut c = Client::new("ask");
    c.install(plugin_host(vec![
        package("test.first", PICKER, READS),
        package("test.second", PICKER, READS),
    ]));
    c.frames(4);
    let first = c.host().question().expect("the host asks");
    assert_eq!(first.asker, "Test test.first 1.0.0 (test.first)");
    assert_eq!(c.question_shown(), Some(first.id), "the question is drawn");
    c.answer(true);
    assert_eq!(
        c.question_shown(),
        Some(first.id),
        "with no file to give, Give it answers nothing"
    );
    c.answer(false);
    assert_eq!(c.console("test.first"), ["file\tfalse\trefused"], "refused");
    c.frames(2);
    let second = c.host().question().expect("the next plugin's question");
    assert_ne!(second.id, first.id);
    assert_eq!(c.question_shown(), Some(second.id));
    // Escape refuses it.
    c.key(ESCAPE, true);
    c.key(ESCAPE, false);
    c.frames(3);
    assert_eq!(
        c.console("test.second"),
        ["file\tfalse\trefused"],
        "refused by Escape"
    );
    assert_eq!(c.host().question(), None);
    assert_eq!(c.question_shown(), None, "the question is gone");
    c.finish();
}

/// A window's tree changing faster than 30 times a second is shown afresh at most that often, the
/// latest change each time; a window opened or closed is shown at once.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_windows_changes_are_shown_at_most_thirty_times_a_second() {
    use pyreal_plugin_host::UiInput;
    let mut c = Client::new("coalesce");
    c.install(plugin_host(vec![package(
        "test.toggled",
        r#"ui.window({ id = "main", title = "Toggled", ui.toggle({ id = "t", text = "On" }) })"#,
        "",
    )]));
    c.frames(3);
    c.open("test.toggled");
    let mut cache = super::plugins::ViewCache::default();
    let seen = |views: Vec<pyreal_plugin_ui::WindowView>| {
        views
            .iter()
            .find(|v| v.window == "main")
            .map(|v| (v.revision, v.open))
            .expect("the window")
    };
    let (first, _) = seen(cache.refresh(c.host(), 10.0));
    let flip = |c: &mut Client, on: bool| {
        c.host()
            .ui_input(
                "test.toggled",
                "main",
                UiInput::Change {
                    node: "t".into(),
                    value: pyreal_plugin_api::Value::Bool(on),
                },
            )
            .expect("a change");
    };
    flip(&mut c, true);
    assert_eq!(
        seen(cache.refresh(c.host(), 10.01)),
        (first, true),
        "a hundredth of a second on, the tree shown is the last"
    );
    flip(&mut c, false);
    assert_eq!(
        seen(cache.refresh(c.host(), 10.04)),
        (first + 2, true),
        "a thirtieth on, the latest"
    );
    flip(&mut c, true);
    c.host().toggle_window("test.toggled").expect("closed");
    assert_eq!(
        seen(cache.refresh(c.host(), 10.05)),
        (first + 4, false),
        "closed: shown at once"
    );
    c.finish();
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_plugins_window_and_hud_are_drawn_and_a_click_on_its_button_reaches_it() {
    let mut c = Client::new("windows");
    c.install(plugin_host(vec![package("test.win", WINDOWED, "")]));
    c.frames(3);
    c.open("test.win");
    c.frames(2);
    let ui = &c.shell.horizon.ui.as_ref().expect("Horizon").ui;
    assert!(
        ui.windows
            .is_open(pyreal_horizon::ui::panels::WindowId::Plugin(0)),
        "Horizon seats the window among its own"
    );
    assert!(
        ui.windows
            .plugins
            .frame
            .views
            .iter()
            .any(|v| v.window == "main" && v.by_player),
        "opened by the player from the plugin bar"
    );
    assert!(
        !ui.windows.plugins.hits.is_empty(),
        "and the HUD and the bar among its HUD's elements"
    );
    let go = c.plugin_button();
    let before = c.console("test.win").len();
    c.click((f64::from(go.x + go.w / 2.0), f64::from(go.y + go.h / 2.0)));
    c.frames(2);
    assert_eq!(
        c.console("test.win")[before..],
        ["clicked go"],
        "the click reached the plugin, once"
    );
    c.finish();
}

/// A plugin that says each press on the game's map: the button and where it fell.
const MAPPED: &str = r#"
on("map.clicked", function(e) print(e.button, e.position.ns, e.position.ew) end)
"#;

/// The screen pixel a mark at `ns`, `ew` is drawn on, on Horizon's map picture drawn in `area`.
#[allow(clippy::cast_possible_truncation)]
fn map_pixel(area: pyreal_ui_kit::draw::Rect, ns: f64, ew: f64) -> (i32, i32) {
    let (x, y) = pyreal_horizon::ui::panels::map::MapView::default()
        .on_screen(area, pyreal_horizon::ui::panels::map::map_pixel(ns, ew));
    (x as i32, y as i32)
}

/// A press on Horizon's world map reaches the plugin with the place under the pointer: pressed on
/// the pixel Holtburg's mark is drawn on, it reads back Holtburg to within a pixel, and the
/// interface draws a mark at the place told on the pixel pressed.
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn a_press_on_the_map_reaches_the_plugins_with_the_place_under_the_pointer() {
    use pyreal_horizon::ui::panels::WindowId;
    let mut c = Client::new("map");
    c.install(plugin_host(vec![package("test.map", MAPPED, "")]));
    c.frames(3);
    let ui = c.shell.horizon.ui.as_mut().expect("Horizon");
    ui.ui.windows.open(WindowId::Map, 0.0);
    c.frames(3);
    let ui = c.shell.horizon.ui.as_ref().expect("Horizon");
    let area = ui.ui.windows.map_picture.expect("the map window is open");
    let pixel = map_pixel(area, 42.2, 33.8);
    let before = c.console("test.map").len();
    c.click((f64::from(pixel.0) + 0.5, f64::from(pixel.1) + 0.5));
    c.frames(2);
    let said = c.console("test.map")[before..].to_vec();
    let told: Vec<&str> = said
        .first()
        .map(|l| l.split('\t').collect())
        .unwrap_or_default();
    let place = |i: usize| told.get(i).and_then(|v| v.parse::<f64>().ok());
    let (Some(ns), Some(ew)) = (place(1), place(2)) else {
        panic!("the press told, with a place: {said:?}");
    };
    assert_eq!(
        (said.len(), told[0]),
        (1, "left"),
        "one left press: {said:?}"
    );
    assert!(
        (ns - 42.2).abs() < 0.85 && (ew - 33.8).abs() < 0.85,
        "Holtburg to within a pixel: {ns}, {ew}"
    );
    assert_eq!(
        map_pixel(area, ns, ew),
        pixel,
        "a mark at {ns}, {ew} is drawn on the pixel pressed"
    );
    c.finish();
}
