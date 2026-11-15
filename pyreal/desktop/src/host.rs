//! The desktop host: what the client shell is given on a desktop -- the `winit` window and the
//! system clock, the operating system's time zone and URL launch, the `cpal` sound output, the
//! system clipboard, the system cursors and the window's hold on the pointer -- for the product
//! `P`.

use std::marker::PhantomData;

use pyreal_client_runtime::app::{Platform, StartupError};
use pyreal_client_runtime::config::Config;
use pyreal_client_shell::cursor::CursorImages;
use pyreal_client_shell::platform::host::Host;

use crate::Product;

/// The desktop, as the client shell's host, for product `P`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Desktop<P: Product>(PhantomData<P>);

impl<P: Product> Host for Desktop<P> {
    const BUILD_ID: &'static str = P::BUILD_ID;
    const BUILD_COMMIT: &'static str = P::BUILD_COMMIT;

    type Clipboard = crate::clipboard::SystemClipboard;

    fn open_platform(
        cfg: &Config,
        events: pyreal_client_shell::platform::window::WindowEvents,
    ) -> Result<Platform, StartupError> {
        crate::launch::open_platform::<P>(cfg, events)
    }

    fn local_utc_offset_secs(unix_secs: i64) -> i32 {
        crate::platform::local_utc_offset_secs(unix_secs)
    }

    fn launch_uri(url: &str) -> i32 {
        crate::launch::launch_uri(url)
    }

    fn install_default_output() {
        crate::audio::install_default_output();
    }

    fn clipboard() -> Self::Clipboard {
        crate::clipboard::SystemClipboard
    }

    fn cursor_images(window: Option<isize>) -> Box<dyn CursorImages> {
        P::cursor_images(window)
    }

    fn caret_blink_secs() -> f64 {
        crate::platform::window::caret_blink_time_seconds()
    }

    fn pointer(window: Option<isize>) -> Box<dyn pyreal_client_shell::pointer::HostPointer> {
        crate::cursor::desktop_pointer(window)
    }

    fn gamepad() -> Box<dyn pyreal_client_shell::gamepad::HostGamepad> {
        Box::new(crate::gamepad::DesktopGamepad::default())
    }
}
