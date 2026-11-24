//! The desktop's native console attachment, error box, read-only display queries, the splash
//! painted before there is a graphics device, and the Mac's open-URL events.
//!
//! **Depends on** no other workspace crate (`objc2` on the Mac). **Used by** the client
//! (`pyreal-client`) and the desktop host (`pyreal-desktop`).
//!
//! **Must never** own or close a native handle, retain a caller's buffer or reach the client
//! runtime. Unsafe calls are confined to console attachment, the modal error box, the
//! read-only caret/monitor/screen queries, one picture painted on a window with GDI and the Mac's
//! open-URL handler. Each call returns plain values and keeps its buffers local.
//!
//! Under the windows subsystem Windows creates no console, and this crate never makes one: a player
//! who starts the client from Explorer or from the launcher sees the game's window and nothing
//! else. [`attach`] only borrows a console that already exists:
//!
//! | how the client was started | what [`attach`] does | what the user sees |
//! |---|---|---|
//! | from a terminal | `AttachConsole(ATTACH_PARENT_PROCESS)` | output in that terminal |
//! | from Explorer, or from the launcher | `AttachConsole` fails; nothing else is called | no console |
//! | with `--no-console` | nothing is called | no console |
//!
//! So a developer who wants the client's stderr starts it from a terminal (or passes `--log-file`
//! for the same lines in a file); nobody else is shown a console window.

#![deny(unsafe_code)]

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;

#[cfg(windows)]
#[allow(unsafe_code)]
mod message_box;

#[cfg(windows)]
#[allow(unsafe_code)]
mod desktop_queries;

#[cfg(windows)]
#[allow(unsafe_code)]
mod window_paint;

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod open_url;

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod screen_area;

/// The usable width and height of the screen whose display id is `display`, less its menu bar
/// and Dock, in points. `None` off the Mac.
#[must_use]
pub fn usable_screen_points(display: u32) -> Option<(f64, f64)> {
    #[cfg(target_os = "macos")]
    {
        screen_area::usable_points(display)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = display;
        None
    }
}

/// A picture as GDI takes it: rows from the top, four bytes a texel in the order blue, green,
/// red and one unused.
#[derive(Debug, Clone, Copy)]
pub struct Picture<'a> {
    pub width: u32,
    pub height: u32,
    pub bgra: &'a [u8],
}

/// Paint `picture` on the window `window` (its native handle) as large as it goes whole, centred,
/// the rest of the window `round` (red, green, blue): what a window shows before there is a
/// graphics device to draw with. Whether it was painted; nothing is painted off Windows, where the
/// window waits for the device.
pub fn paint_picture(window: isize, picture: &Picture<'_>, round: (u8, u8, u8)) -> bool {
    #[cfg(windows)]
    {
        window_paint::paint(window, picture, round)
    }
    #[cfg(not(windows))]
    {
        let _ = (window, picture, round);
        false
    }
}

/// Where a picture of size `picture` stands in an area of size `area` drawn as large as it goes
/// whole and centred: left, top, width and height.
#[must_use]
pub fn contained(picture: (u32, u32), area: (u32, u32)) -> (i32, i32, i32, i32) {
    let (pw, ph) = (u64::from(picture.0.max(1)), u64::from(picture.1.max(1)));
    let (aw, ah) = (u64::from(area.0), u64::from(area.1));
    // The wider of the two is held by its height, the narrower by its width.
    let (w, h) = if aw * ph > ah * pw {
        (ah * pw / ph, ah)
    } else {
        (aw, aw * ph / pw)
    };
    let i = |v: u64| i32::try_from(v).unwrap_or(i32::MAX);
    (i((aw - w) / 2), i((ah - h) / 2), i(w), i(h))
}

#[cfg(test)]
mod picture_tests {
    #[test]
    fn a_picture_stands_whole_and_centred() {
        assert_eq!(super::contained((800, 600), (800, 600)), (0, 0, 800, 600));
        assert_eq!(
            super::contained((800, 600), (1600, 1200)),
            (0, 0, 1600, 1200)
        );
        // Wider than the picture: bars either side.
        assert_eq!(
            super::contained((800, 600), (1920, 1080)),
            (240, 0, 1440, 1080)
        );
        // Taller: bars above and below.
        assert_eq!(super::contained((800, 600), (600, 800)), (0, 175, 600, 450));
    }
}

/// Start receiving the links (`pyreal://...`) the system opens with this application; on the Mac
/// before the event loop first runs, so the link it was launched with is caught. Elsewhere a link
/// arrives on the command line and this does nothing.
pub fn receive_open_urls() {
    #[cfg(target_os = "macos")]
    open_url::install();
}

/// The links opened with this application since the last call, oldest first.
#[must_use]
pub fn take_open_urls() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        open_url::take()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

/// The system's caret interval in milliseconds; 530 on hosts without the native query.
#[must_use]
pub fn caret_blink_millis() -> u32 {
    #[cfg(windows)]
    {
        desktop_queries::caret_blink_millis()
    }
    #[cfg(not(windows))]
    {
        530
    }
}

/// The usable monitor rectangle, without owning the supplied opaque handle.
#[must_use]
pub fn monitor_work_area(handle: isize) -> Option<(i32, i32, i32, i32)> {
    #[cfg(windows)]
    {
        desktop_queries::monitor_work_area(handle)
    }
    #[cfg(not(windows))]
    {
        let _ = handle;
        None
    }
}

/// Show the operating system's modal error box and return when the player has closed it.
///
/// This is the other thing the client says outside its own window: the box it shows when its
/// first connection fails, before it has a screen to say it on. `style` is the box's style bits
/// (the buttons, the icon, top-most); the box has no owner window.
///
/// Off Windows nothing is shown: the caller has already logged the same text (on stderr), which
/// is the only surface such a build has.
pub fn error_box(caption: &str, text: &str, style: u32) {
    #[cfg(windows)]
    {
        message_box::error_box(caption, text, style);
    }
    #[cfg(not(windows))]
    {
        let _ = (caption, text, style);
    }
}

/// What [`attach`] found, or made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Console {
    /// `AttachConsole(ATTACH_PARENT_PROCESS)` succeeded: the client is writing to the terminal it
    /// was started from. Note that the shell does **not** wait for a windows-subsystem process, so
    /// output arrives after the prompt has come back. That is the shell's behaviour and not
    /// something the client can change from this side; a launcher that waits on the process
    /// (`Start-Process -PassThru` + `WaitForExit`) rather than on a pipe sees all of it.
    Parent,
    /// The process already had a console, so neither call was needed. Unreachable while the binary
    /// is linked for the windows subsystem, and correct if it ever is not.
    Existing,
    /// There was no console to borrow -- the Explorer and launcher case -- or this is not
    /// Windows. Output goes nowhere; the run is otherwise unaffected, which is why this is a value
    /// and not an error. No console is created in its place.
    None,
}

/// Borrow the console of whatever started us, if it has one. A console is never created.
///
/// Call this **first thing in `main`**, before anything writes to stdout or stderr: `std` resolves
/// the standard handles when it writes, and a write that happens first goes nowhere.
///
/// Never called when `--no-console` was given -- that switch is precisely "do not call this".
#[must_use]
pub fn attach() -> Console {
    #[cfg(windows)]
    {
        windows::attach()
    }
    #[cfg(not(windows))]
    {
        // The non-Windows build of the client exists to keep `cargo check` honest on a non-Windows
        // host; it cannot open a device or a window either. The process keeps whatever streams it
        // was started with, which on a Unix shell is the terminal, so there is nothing to do and
        // nothing is lost.
        Console::None
    }
}
