//! Pyreal, the Asheron's Call client: the window, the graphics device, the renderer, the interface
//! and the sound device, plugged into the runtime's frame loop.
//!
//! **Depends on** the client shell (`pyreal-client-shell`), scene (`pyreal-scene`) and desktop
//! host (`pyreal-desktop`), with renderer backend selection (`dereth-render`), host input replay
//! (`pyreal-input`), shared types (`pyreal-primitives`) and
//! console (`pyreal-console`) and shared application state (`pyreal-client-runtime`), and the plugin
//! host and its engines (`pyreal-plugin-api`, `pyreal-plugin-host`, `pyreal-plugin-lua`,
//! `pyreal-plugin-wasm`), the game data plugins read (`pyreal-dat`) and the player's settings the
//! plugin host follows (`pyreal-client-contract`), what it knows before it connects
//! (`pyreal-launch`), the status ping that names a world's era (`pyreal-transport`) and the
//! screens before the world it fills (`pyreal-horizon`).
//! **Used by** nothing but the client's test kit (`pyreal-testkit`).
//!
//! **Must never** do another crate's work: if something here starts decoding an asset, transforming
//! a vertex or parsing a message, that work belongs elsewhere and the move is to widen that crate's
//! API. It is the one crate permitted to depend on many subsystems at once, which is exactly why
//! nothing but the test kit may depend on it. It keeps the workspace's `forbid(unsafe_code)`.
//!
//! The frame loop is the runtime's and the application is the client shell's, generic over its
//! host. The host is the desktop's ([`pyreal_desktop`]), and what this crate adds is the product
//! itself, [`Dereth`]: its name, its settings folder, its icon, with [`app::App`] and
//! [`app::ClientShell`] specialized on the [`Desktop`] host.
//!
//! ```text
//! pyreal -- --headless --frames 1 --capture out.png
//! ```

pub mod app;
// The art packs: which one is drawn, chosen on the Display page.
pub mod art_packs;

// The HUD, with the desktop's platform answers.
pub mod hud;
// Plugins on the desktop: the plugins folder, the stores, and the host in the frame.
pub mod plugins;
// The plugins the player installs: the Plugins screen's shelf.
pub mod plugin_shelf;
// The screens before any world: worlds, accounts, data files, sessions in process.
pub mod front_door;
// The list of known worlds: its copy, read off the frame thread.
pub mod world_directory;
// The Linux desktop's entry.
#[cfg(any(test, target_os = "linux"))]
pub mod desktop_entry;

/// The client, as a product on the desktop host: Pyreal.
#[derive(Debug, Clone, Copy, Default)]
pub struct Dereth;

/// The application's identity: the macOS bundle's identifier and the Linux desktop's app id.
pub const APP_ID: &str = "com.jacobjohnston.Pyreal";

impl pyreal_desktop::Product for Dereth {
    const BINARY_NAME: &'static str = "pyreal";
    const BUILD_ID: &'static str = concat!("pyreal ", env!("CARGO_PKG_VERSION"));
    const BUILD_COMMIT: &'static str = match option_env!("PYREAL_BUILD_COMMIT") {
        Some(commit) => commit,
        None => "unknown",
    };
    const SETTINGS_DIR_NAME: &'static str = pyreal_desktop::folders::CLIENT_DIR_NAME;
    const TITLE: &'static str = "Pyreal";
    const ICON_PNG: &'static [u8] = include_bytes!("../assets/pyreal-256.png");
    const APP_ID: &'static str = APP_ID;
    /// Dragged on macOS (client divergence CD-resizable-window); elsewhere the window keeps the
    /// size its resolution names, as the original's does.
    const RESIZABLE: bool = cfg!(target_os = "macos");
    const LOGIN_SIZE: Option<(u32, u32)> = Some(pyreal_client_runtime::config::FORCED_LOGIN_SIZE);
    /// As a Mac's windows go (client divergence CD-mac-window-goes-by-points).
    const SIZE_IN_POINTS: bool = cfg!(target_os = "macos");
    const ASKS_FOR_DATS: bool = true;

    /// The desktop entry and the icon it names, under the user's data folder, each written only
    /// when it is missing or its content differs. A failure is logged: the client runs with a
    /// generic icon rather than not at all.
    #[cfg(target_os = "linux")]
    fn before_window() {
        let var = |v: &str| std::env::var_os(v);
        let (Some(home), Ok(me)) = (desktop_entry::data_home(&var), std::env::current_exe()) else {
            return;
        };
        let exec = desktop_entry::entry_target(&var, me);
        let files = desktop_entry::files(&home, &exec, APP_ID, Self::ICON_PNG);
        match desktop_entry::write_changed(&files) {
            Ok(written) => {
                for path in written {
                    tracing::debug!("desktop entry written: {}", path.display());
                }
            }
            Err(e) => tracing::warn!("desktop entry: {e}"),
        }
    }
}

/// The desktop, as the client shell's host, for the Dereth client.
pub type Desktop = pyreal_desktop::Desktop<Dereth>;

/// What `pyreal --version` prints: the version, and the commit and target a release build
/// was made from (`unknown` in a build that did not say).
#[must_use]
pub fn version_text() -> String {
    format!(
        "pyreal {}\ncommit: {}\ntarget: {}\n",
        env!("CARGO_PKG_VERSION"),
        option_env!("PYREAL_BUILD_COMMIT").unwrap_or("unknown"),
        option_env!("PYREAL_BUILD_TARGET").unwrap_or("unknown"),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_version_text_names_the_client_and_its_version_first() {
        let text = super::version_text();
        let first = text.lines().next().unwrap();
        assert_eq!(first, format!("pyreal {}", env!("CARGO_PKG_VERSION")));
        assert!(text.lines().any(|l| l.starts_with("target: ")), "{text}");
    }
}

/// The folder the client keeps its own files in (`art-packs.txt`, `worlds.json`, the plugins and
/// their stores): the folder of the preferences file it runs with (`-prefs`, or a portable one
/// beside it), else `default`, the settings folder.
#[must_use]
pub fn settings_dir_for(
    preferences_file: &std::path::Path,
    default: Option<std::path::PathBuf>,
) -> std::path::PathBuf {
    if preferences_file.as_os_str().is_empty() {
        return default.unwrap_or_default();
    }
    std::path::absolute(preferences_file)
        .ok()
        .and_then(|f| f.parent().map(std::path::Path::to_path_buf))
        .or(default)
        .unwrap_or_default()
}

#[cfg(test)]
mod settings_dir_tests {
    use std::path::{Path, PathBuf};

    /// The client's own files are kept beside the preferences it runs with: a `-prefs` file's or
    /// a portable one's folder, else the settings folder.
    #[test]
    fn the_client_s_files_are_kept_beside_its_preferences() {
        let default = Some(PathBuf::from("/home/p/.config/Pyreal"));
        assert_eq!(
            super::settings_dir_for(Path::new("/games/ac/UserPreferences.ini"), default.clone()),
            // On Windows a rooted path takes the current drive.
            std::path::absolute("/games/ac").unwrap()
        );
        assert_eq!(
            super::settings_dir_for(Path::new(""), default.clone()),
            PathBuf::from("/home/p/.config/Pyreal"),
            "a headless run with no file"
        );
        let here = super::settings_dir_for(Path::new("UserPreferences.ini"), default);
        assert_eq!(here, std::env::current_dir().unwrap(), "a portable file");
    }
}
