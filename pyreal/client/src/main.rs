//! The `pyreal` entry point. Everything it calls lives in the library half of this crate,
//! so the integration tests in `tests/` can drive the same `App` the binary drives.
//!
//! `WinMain`'s steps 9 to 12: parse, initialize the client, run it, and clean up.
//! The library owns the application behavior; this binary only performs those startup steps.
//!
//! # The subsystem, and `--no-console`
//!
//! The retail entry point is `WinMain`, not `main`: the client is a **windowed** application and
//! Windows makes no console for one. Cargo's default is the console subsystem, and a
//! console-subsystem child of a windowed process gets a console *window* -- a black rectangle on
//! the screen beside the game.
//!
//! The attribute below fixes the subsystem to the one the original had, and [`pyreal_console`]
//! gives back the one part of the console subsystem a developer wants: a run started from a
//! terminal borrows that terminal for its output. A run started from Explorer or the launcher has
//! no console to borrow and is shown none -- just the game's window. `--no-console` does not even
//! borrow one. A console is never created.
//!
//! One behaviour does change for a run started **by hand from a shell**: cmd and PowerShell do not
//! wait for a windows-subsystem process, so the prompt returns immediately and the client's output
//! arrives behind it. `cargo run`, and any launcher that waits on the process itself, are
//! unaffected.
#![windows_subsystem = "windows"]

use pyreal_client::Dereth;
use pyreal_client_runtime::config::Config;
use pyreal_client_runtime::corestrings;
use pyreal_desktop::crashlog;

mod input_live;
mod input_replay;

fn main() -> std::process::ExitCode {
    // Before *everything*, including the crash log's own `START` record and the stderr line below:
    // `std` resolves the standard handles when it writes, so the console has to exist by the first
    // write or that write is lost. `--no-console` is read straight out of `argv` here rather than
    // taken off the parsed `Config`, because the parse comes later and reports its failures on the
    // stderr this call is what provides. See `pyreal_client_runtime::config::no_console_in_argv`.
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if pyreal_client_runtime::config::no_console_in_argv(&argv) {
        // Nothing to do, and that is the whole feature: no console is borrowed, and the process's
        // streams stay as the windows subsystem left them.
    } else {
        let _ = pyreal_console::attach();
    }
    // `--version` alone: what this build is, and nothing started, written or read.
    if argv == ["--version"] {
        print!("{}", pyreal_client::version_text());
        return std::process::ExitCode::SUCCESS;
    }
    // Before anything else, so that a failure inside argument parsing is recorded too.
    crashlog::install::<Dereth>();
    match run() {
        Ok(()) => {
            crashlog::finished::<Dereth>(0, "ran to the end of main");
            std::process::ExitCode::SUCCESS
        }
        Err(message) => {
            // The final error display is a modal dialog, after which the
            // process ends. Without a UI framework the honest equivalent is the same text on stderr
            // and a non-zero exit. It is the dialog's stand-in rather than a log line, and a parse
            // failure reaches it before the log is installed, so it is written directly.
            eprintln!("{}: {message}", corestrings::CAPTION_ERROR);
            crashlog::finished::<Dereth>(1, &format!("{}: {message}", corestrings::CAPTION_ERROR));
            std::process::ExitCode::FAILURE
        }
    }
}

/// `WinMain` steps 9 to 12: parse, initialize the client, run it, and clean up.
fn run() -> Result<(), String> {
    // parse the command line, make the settings folder and install the log.
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    // A link opened with the app while it runs (on the Mac, also the one it was launched with).
    pyreal_console::receive_open_urls();
    // A join link the system started the app with adds its world to the list.
    let join = pyreal_launch::join::from_args(&argv).map(str::to_owned);
    argv.retain(|a| Some(a) != join.as_ref());
    let replay = input_replay::Replay::load(&mut argv)?;
    let live = input_live::LiveInput::load(&mut argv)?;
    // The log's writer is held to the end of the run, and drained when it goes.
    let (cfg, _log) = pyreal_desktop::start::<Dereth>(&argv)?;
    run_with(cfg, replay, live, join)
}

/// A `StartupError` on its way to `main`'s stderr sentence, with its cause logged first.
///
/// `StartupError::Device`'s text is corestrings 127, which only reports a fatal Windows API issue.
/// The variant also carries the underlying failure for this log line, so a start-up that dies on,
/// say, a missing Vulkan loader says so instead of asking the player to reboot.
#[cfg(gpu)]
fn fatal(e: pyreal_client_runtime::app::StartupError) -> String {
    if let Some(cause) = e.cause() {
        tracing::error!("{cause}");
    }
    e.to_string()
}

#[cfg(gpu)]
fn run_with(
    mut cfg: Config,
    replay: Option<input_replay::Replay>,
    live: Option<input_live::LiveInput>,
    join: Option<String>,
) -> Result<(), String> {
    // Started with no server, the windowed client opens to the screens before any world, as a
    // headless run does with `--front-door`; the data files are the ones last chosen there.
    cfg.front_door = !cfg.will_connect() && (cfg.front_door || !cfg.headless);
    let settings = pyreal_client::settings_dir_for(
        &cfg.preferences_file,
        pyreal_desktop::folders::default_settings_dir(
            <pyreal_client::Dereth as pyreal_desktop::Product>::SETTINGS_DIR_NAME,
        ),
    );
    // The device's pipeline cache, kept between runs in the settings folder; a headless run that
    // named no preferences keeps none, as it keeps no settings.
    if !settings.as_os_str().is_empty() && (!cfg.headless || cfg.preferences_named) {
        pyreal_scene::gpu::keep_pipeline_cache_in(
            settings.join("Cache"),
            &format!(
                "{} {}",
                env!("CARGO_PKG_VERSION"),
                option_env!("PYREAL_BUILD_COMMIT").unwrap_or("unknown")
            ),
        );
    }
    // The art pack chosen over the base pack (none by default), or the one `PYREAL_ART_PACK`
    // names.
    let mut packs = pyreal_client::art_packs::ArtPacks::open(&settings);
    // The old Dereth client's profile beside the old launcher's folder (none for a run given its
    // own settings folder).
    let older = pyreal_desktop::folders::legacy_launcher_dir()
        .and_then(|launcher| launcher.parent().map(std::path::Path::to_path_buf))
        .map(|root| {
            root.join(pyreal_desktop::folders::LEGACY_CLIENT_DIR_NAME)
                .join(pyreal_client_runtime::config::PREFERENCES_FILE_NAME)
        });
    let prefs: Vec<&std::path::Path> = std::iter::once(cfg.preferences_file.as_path())
        .chain(older.as_deref())
        .collect();
    cfg.art_pack = packs.start_pack(cfg.art_pack.take(), &prefs);
    let mut door = cfg.front_door.then(|| {
        let found = (!cfg.dat_dir.as_os_str().is_empty()).then(|| cfg.dat_dir.clone());
        let mut door = pyreal_client::front_door::FrontDoor::open(
            &settings,
            pyreal_desktop::folders::legacy_launcher_dir().as_deref(),
            Box::new(pyreal_client::front_door::SystemVault),
            found.as_deref(),
        );
        if let Some(link) = &join {
            door.add_join_link(link);
        }
        if let (Some(modern), classic) = door.dat_dirs() {
            cfg.dat_dir = modern;
            cfg.classic_dat_dir = classic;
        }
        door
    });
    let capture = cfg.capture.clone();
    let headless = cfg.headless;
    let linger = cfg.linger;
    let mut scene = cfg.scene_config();
    // `--explore`: the files the run reads, from the place given.
    let explore = cfg
        .explore
        .map(|place| pyreal_client_runtime::explore::ExploreStart {
            dat_dir: cfg.dat_dir.clone(),
            classic_dat_dir: cfg.classic_dat_dir.clone(),
            era: cfg.era,
            place,
        });
    let want_world = cfg.world;
    let terrain_blending = cfg.terrain_blending;
    let cfg_connect = cfg.will_connect();
    let host = cfg.host.clone();
    let account = cfg.account_name().to_owned();
    // The plugins the player installs, offered with the run's own: before any world only.
    let mut shelf = door.is_some().then(|| {
        pyreal_client::plugin_shelf::PluginShelf::open(
            &settings,
            pyreal_desktop::folders::legacy_launcher_dir().as_deref(),
            cfg.offered_plugin_dirs.clone(),
        )
    });
    let mut plugins = cfg.plugins.then(|| pyreal_client::plugins::DesktopPlugins {
        settings_dir: Some(settings.clone()),
        extra_dirs: cfg.plugin_dirs.clone(),
        offered_dirs: shelf.as_ref().map_or_else(
            || cfg.offered_plugin_dirs.clone(),
            pyreal_client::plugin_shelf::PluginShelf::offered,
        ),
        bundled_dir: std::env::current_exe()
            .ok()
            .and_then(|exe| pyreal_client::plugins::bundled_dir_for(&exe)),
        watch: cfg.plugin_dev,
        policy: cfg.plugin_policy.clone(),
    });

    let nav_published = cfg.nav_published.clone();
    // initialize the client application and its presentation.
    let mut app = pyreal_client::app::App::new(cfg).map_err(fatal)?;
    // The Admin window's weenie list is fetched the same way, when first wanted.
    app.weenie_lists.fetch = Some(std::sync::Arc::new(|url: &str| {
        pyreal_client::plugins::fetch_text(url)
    }));
    // The server's published portal table, fetched off the frame's thread.
    if let Some(source) = nav_published {
        let inbox = std::sync::Arc::clone(&app.published_portals);
        std::thread::spawn(move || match pyreal_client::plugins::fetch_text(&source) {
            Ok(text) => {
                if let Ok(mut slot) = inbox.lock() {
                    *slot = Some(text);
                }
            }
            Err(e) => tracing::warn!("--nav-published: {e}"),
        });
    }
    tracing::info!(
        "{} device, {}x{}",
        app.renderer_mut().adapter_name(),
        app.renderer_mut().size().0,
        app.renderer_mut().size().1
    );
    // On a software rasteriser the compute shader runs on the CPU too, and far more slowly than
    // the CPU compositor it would replace (a minute against six seconds for one Extreme-distance
    // load on WARP). The two are bit-identical, so the CPU one is taken instead.
    if app.renderer_mut().software() && scene.gpu_terrain_merge {
        scene.gpu_terrain_merge = false;
        if terrain_blending == pyreal_client_runtime::render_prefs::TerrainBlending::Gpu {
            tracing::info!(
                "TerrainBlending=gpu on a software device -- composing on the CPU, which is faster there and identical"
            );
        }
    }

    // Initialize the keymap, sound and UI. The keymap and sound come up whatever the switches
    // say, because the pump has somewhere to send a `MSG` and the device is the thing being
    // proved; the screens come up with `--ui`.
    app.start_shell().map_err(fatal)?;
    if let Some(plugins) = &plugins {
        plugins.install(&mut app);
    }

    if let Some(start) = explore.as_ref().filter(|_| want_world) {
        // The sessions a front door starts after it still have their body.
        if door.is_some() {
            app.defer_static_scene(pyreal_client_runtime::scene::SceneConfig {
                character: true,
                ..scene
            });
        }
        app.begin_explore(start).map_err(fatal)?;
    } else if want_world && (cfg_connect || door.is_some()) {
        // With a server the block to build the landscape around is the one the *server*
        // puts the player in, and nothing knows that until the player's `0xF745` arrives. So the
        // scene is deferred until world entry supplies a position and the landscape can load
        // landblocks. `--landblock` still chooses the scenery radius and the rest of the config.
        app.defer_static_scene(scene);
        tracing::info!("the landscape waits for the server to place the player");
    } else if want_world {
        // A missing region or landblock is fatal for the same reason a missing portal dat
        // is: there is nothing to draw and nothing else this build does.
        app.load_static_scene(scene).map_err(fatal)?;
        app.log_load_report(scene);
    } else {
        // The full-screen quad, kept reachable so its regression still runs.
        app.load_first_pixel_scene().map_err(fatal)?;
        tracing::info!(
            "surface 0x{:08X}",
            pyreal_client_runtime::assets::FIRST_PIXEL_SURFACE
        );
    }

    if cfg_connect {
        // The connection step keeps the UI alive until the link reaches Connected; `App::run`
        // owns that frame loop. This branch therefore only reports which server was selected.
        tracing::info!("connecting to {host} as {account}");
    }

    // The one sound that proves the device path end to end: the UI button-press sound out of the UI
    // sound table, played centred on the listener. It is played before the loop rather than from a
    // button because a run without the UI has no buttons to press.
    app.play_startup_sound();

    // run the client frame loop. With `--linger`, a capture is of the world, a moment before the
    // run logs off.
    let mut captured = false;
    let frames = if let Some(mut replay) = replay {
        app.state = pyreal_client_runtime::app::AppState::Running;
        let mut frame = 1;
        loop {
            replay.drain_frame(frame, |event| app.queue_window_event(event));
            if let Some(pad) = replay.drain_pad(frame) {
                app.set_scripted_pad(pad);
            }
            if !app.frame() {
                break;
            }
            if let Some(door) = door.as_mut() {
                door.step(&mut app);
            }
            if let Some(shelf) = shelf.as_mut() {
                shelf.step(&mut app, plugins.as_mut());
            }
            packs.step(&mut app);
            frame += 1;
        }
        app.state = pyreal_client_runtime::app::AppState::ShuttingDown;
        app.frames_drawn()
    } else if let Some(mut live) = live {
        app.state = pyreal_client_runtime::app::AppState::Running;
        loop {
            match live.next_step() {
                Some(input_live::Step::Event(event)) => app.queue_window_event(event),
                Some(input_live::Step::Capture(path)) => {
                    if let Err(e) = app.renderer_mut().capture_png(&path) {
                        tracing::warn!("--input-live capture: {e}");
                    }
                }
                Some(input_live::Step::Quit) => break,
                None => {}
            }
            if !app.frame() {
                break;
            }
            if let Some(door) = door.as_mut() {
                door.step(&mut app);
            }
            if let Some(shelf) = shelf.as_mut() {
                shelf.step(&mut app, plugins.as_mut());
            }
            packs.step(&mut app);
        }
        app.state = pyreal_client_runtime::app::AppState::ShuttingDown;
        app.frames_drawn()
    } else if let Some(door) = door.as_mut() {
        app.state = pyreal_client_runtime::app::AppState::Running;
        let mut files = app.store_generation();
        loop {
            door.step(&mut app);
            if let Some(shelf) = shelf.as_mut() {
                shelf.step(&mut app, plugins.as_mut());
            }
            packs.step(&mut app);
            // A session on other data files gives the plugins those files and its world's policy.
            if app.store_generation() != files {
                files = app.store_generation();
                if let Some(p) = plugins.as_mut() {
                    pyreal_client::plugins::flush(&mut app);
                    p.policy.clone_from(&app.config().plugin_policy);
                    p.install(&mut app);
                }
            }
            if !app.frame() {
                break;
            }
        }
        app.state = pyreal_client_runtime::app::AppState::ShuttingDown;
        app.frames_drawn()
    } else if let (Some(path), true) = (&capture, headless && linger > 0.0) {
        app.run_with_moment((linger - 0.5).max(0.0), |app| {
            match app.renderer_mut().capture_png(path) {
                Ok(()) => {
                    tracing::info!("wrote {} (in the world)", path.display());
                    captured = true;
                }
                Err(e) => tracing::warn!("capture in the world: {e}"),
            }
        })
    } else {
        // `PYREAL_CAPTURE_EVERY=N` with `--capture a.png` also writes a-FRAME.png every N frames.
        let every = std::env::var("PYREAL_CAPTURE_EVERY")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|n| *n > 0 && headless);
        let mut frame = 0u64;
        app.run_each(|app| {
            packs.step(app);
            frame += 1;
            if let (Some(n), Some(path)) = (every, &capture) {
                if frame.is_multiple_of(n) {
                    let stem = path.with_extension("");
                    let at = format!("{}-{frame:04}.png", stem.display());
                    if let Err(e) = app.renderer_mut().capture_png(std::path::Path::new(&at)) {
                        tracing::warn!("capture every: {e}");
                    }
                }
            }
        })
    };
    app.log_run_report(frames, cfg_connect);

    if let Some(path) = capture.filter(|_| !captured) {
        if !headless {
            tracing::warn!("--capture reads the offscreen target and needs --headless");
        }
        app.renderer_mut()
            .capture_png(&path)
            .map_err(|e| format!("capture: {e}"))?;
        tracing::info!("wrote {}", path.display());
    }

    // clean up the client, in the documented order; the log is printed rather
    // than dropped so a windowed run says what it tore down and in what order.
    pyreal_client::plugins::flush(&mut app);
    let log = app.shutdown();
    for (step, outcome) in &log.0 {
        tracing::debug!("cleanup {step:?}: {outcome:?}");
    }
    // A driver reset (a Windows TDR) ends the run at its frame, and the player is told why.
    if pyreal_scene::gpu::device_lost().is_some() {
        return Err(
            "The graphics card stopped responding and its driver reset it, so the game \
            closed. Start it again to keep playing."
                .into(),
        );
    }
    Ok(())
}

#[cfg(not(gpu))]
fn run_with(
    _cfg: Config,
    _replay: Option<input_replay::Replay>,
    _live: Option<input_live::LiveInput>,
    _join: Option<String>,
) -> Result<(), String> {
    Err("pyreal needs a graphics backend: build with --features vulkan, d3d12 or wgpu".into())
}
