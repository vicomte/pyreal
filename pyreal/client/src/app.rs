//! The application on the desktop: the client shell's [`pyreal_client_shell::app::App`] on the
//! [`Desktop`](crate::Desktop) host.
//!
//! Shared application types are named through the runtime.

use pyreal_client_runtime::app::{Platform, StartupError};

use pyreal_client_runtime::config::Config;

/// The application, on the desktop.
pub type App = pyreal_client_shell::app::App<crate::Desktop>;

/// The front end, on the desktop.
pub type ClientShell = pyreal_client_shell::app::ClientShell<crate::Desktop>;

/// The runtime's application with the desktop's front end in it.
pub type CoreApp = pyreal_client_shell::app::CoreApp<crate::Desktop>;

/// The windowed platform: the window and its event loop, the system clock and pacer, and the OS
/// error box.
///
/// # Errors
/// [`StartupError::Device`] when the event loop or the window cannot be created.
pub fn open_platform(
    cfg: &Config,
    events: pyreal_client_shell::platform::window::WindowEvents,
) -> Result<Platform, StartupError> {
    pyreal_desktop::launch::open_platform::<crate::Pyreal>(cfg, events)
}
