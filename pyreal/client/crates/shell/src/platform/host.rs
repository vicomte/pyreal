//! The host: what a desktop window gives the shell, as one type the shell is generic over.
//!
//! Every item is an associated function or type, so a host is a choice made at compile time and
//! costs nothing at run time: the desktop client names its host once (`pyreal_client::Desktop`),
//! and a headless run or a test names [`NullHost`].

use pyreal_client_runtime::app::{Platform, StartupError};
use pyreal_client_runtime::config::Config;

use super::window::WindowEvents;
use crate::clipboard::HostClipboard;
use crate::cursor::{CursorImages, PortableCursors};

/// What the shell needs from the platform under it.
pub trait Host: 'static {
    /// The program's name and version, as `@version` prints them after "Client version" (for
    /// example `pyreal-client 0.1.3`).
    const BUILD_ID: &'static str;

    /// The commit the program was built from, as `@report` names it; `unknown` when the build
    /// did not say.
    const BUILD_COMMIT: &'static str = "unknown";

    /// The host clipboard the shell mirrors each frame.
    type Clipboard: HostClipboard;

    /// The windowed platform: the window and its event loop, the clock, the pacer and the error
    /// box. `events` is the queue the window fills and the shell routes.
    ///
    /// # Errors
    /// [`StartupError::Device`] when the window cannot be made.
    fn open_platform(cfg: &Config, events: WindowEvents) -> Result<Platform, StartupError>;

    /// The shift from UTC the host's `localtime` would apply to `unix_secs`, in seconds, for
    /// every local date the client draws.
    fn local_utc_offset_secs(unix_secs: i64) -> i32;

    /// Hand `url` to the host to open: `33` when it was handed on, as the desktop launch answers,
    /// `0` when it was not.
    fn launch_uri(url: &str) -> i32;

    /// Install the host's sound output as the mixer's default endpoint. Idempotent.
    fn install_default_output();

    /// The clipboard, for one front end's life.
    fn clipboard() -> Self::Clipboard;

    /// The cursor images, put on `window` (the native handle, `None` without one).
    fn cursor_images(window: Option<isize>) -> Box<dyn CursorImages>;

    /// The host's current text caret blink interval, in seconds.
    fn caret_blink_secs() -> f64 {
        pyreal_client_contract::window_proc::caret_blink_time_seconds_from_millis(530)
    }

    /// The host's hold on the pointer during a camera drag, over `window` (the native handle,
    /// `None` without one), for one front end's life. A host with none never holds the pointer:
    /// every drag keeps it shown.
    fn pointer(_window: Option<isize>) -> Box<dyn crate::pointer::HostPointer> {
        Box::new(crate::pointer::NoPointerHold)
    }

    /// The host's pad, for one front end's life, asked for only when the front end has a window;
    /// a host with none reads nothing.
    fn gamepad() -> Box<dyn crate::gamepad::HostGamepad> {
        Box::new(crate::gamepad::NoGamepad)
    }
}

/// The host with nothing under it: no window, UTC, nothing launched, no sound, an empty
/// clipboard and cursors that are resolved and never installed. What a headless run and the
/// shell's own tests stand on.
#[derive(Debug, Clone, Copy, Default)]
pub struct NullHost;

impl Host for NullHost {
    const BUILD_ID: &'static str = concat!("pyreal-client ", env!("CARGO_PKG_VERSION"));

    type Clipboard = crate::clipboard::NoClipboard;

    fn open_platform(_cfg: &Config, _events: WindowEvents) -> Result<Platform, StartupError> {
        Err(StartupError::Device {
            cause: "this host has no window".to_string(),
        })
    }

    fn local_utc_offset_secs(_unix_secs: i64) -> i32 {
        0
    }

    fn launch_uri(_url: &str) -> i32 {
        0
    }

    fn install_default_output() {}

    fn clipboard() -> Self::Clipboard {
        crate::clipboard::NoClipboard
    }

    fn cursor_images(_window: Option<isize>) -> Box<dyn CursorImages> {
        Box::new(PortableCursors)
    }
}
