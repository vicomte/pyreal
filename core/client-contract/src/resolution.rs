//! The Resolution list: the common window sizes an interface offers, and what it shows for the
//! window as it is.
//!
//! Sizes are in the window's own units, the ones `Display.Resolution` is kept in: pixels on
//! Windows and Linux, points on a Mac (where a Retina screen draws two pixels to the point).
//! Full screen is borderless at the screen's own size, so the list is for a window only.

/// The common window sizes, smallest first.
pub const COMMON_SIZES: [(u32, u32); 12] = [
    (1024, 768),
    (1280, 720),
    (1280, 800),
    (1366, 768),
    (1440, 900),
    (1600, 900),
    (1680, 1050),
    (1920, 1080),
    (1920, 1200),
    (2560, 1440),
    (2560, 1600),
    (3840, 2160),
];

/// The window as the Resolution row reads it, in the window's own units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowSizes {
    /// The window's size now.
    pub size: (u32, u32),
    /// The largest window the screen it is on holds, less the taskbar (or the menu bar and Dock)
    /// and the window's frame, where that is known.
    pub usable: Option<(u32, u32)>,
    /// Whether the window is full screen, by the client or by the window system's own button.
    pub full_screen: bool,
}

/// The common sizes a window on a screen leaving `usable` holds; every one when that is not known.
#[must_use]
pub fn offered(usable: Option<(u32, u32)>) -> Vec<(u32, u32)> {
    COMMON_SIZES
        .into_iter()
        .filter(|(w, h)| usable.is_none_or(|(uw, uh)| *w <= uw && *h <= uh))
        .collect()
}

/// A size as the list shows it.
#[must_use]
pub fn label((w, h): (u32, u32)) -> String {
    format!("{w} \u{d7} {h}")
}

/// What the Resolution row offers for `window`: the names in its list, and which of them is the
/// window's size. A size not in the list (a dragged one, or an old saved one) is last, as
/// "Custom", and full screen is the screen's size alone.
#[must_use]
pub fn choices(window: &WindowSizes) -> (Vec<String>, Vec<(u32, u32)>, usize) {
    if window.full_screen {
        return (vec![label(window.size)], vec![window.size], 0);
    }
    let mut sizes = offered(window.usable);
    let at = sizes
        .iter()
        .position(|s| *s == window.size)
        .unwrap_or_else(|| {
            sizes.push(window.size);
            sizes.len() - 1
        });
    let names = sizes
        .iter()
        .enumerate()
        .map(|(i, s)| {
            if i == at && !COMMON_SIZES.contains(s) {
                format!("Custom ({})", label(*s))
            } else {
                label(*s)
            }
        })
        .collect();
    (names, sizes, at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_sizes_the_usable_area_holds_are_offered() {
        // A 1920x1080 screen less a 48-pixel taskbar and a window frame.
        let offered = offered(Some((1904, 1001)));
        assert_eq!(offered.last(), Some(&(1600, 900)));
        assert!(offered.iter().all(|(w, h)| *w <= 1904 && *h <= 1001));
        assert!(!offered.contains(&(1920, 1080)));
        // A MacBook Air's 1470x956 points less the menu bar, the Dock and the title bar.
        assert_eq!(
            super::offered(Some((1470, 840))),
            vec![(1024, 768), (1280, 720), (1280, 800), (1366, 768)]
        );
        assert_eq!(super::offered(None), COMMON_SIZES.to_vec());
    }

    #[test]
    fn a_dragged_size_shows_as_custom_and_a_listed_one_is_chosen() {
        let usable = Some((2544, 1381));
        let (names, sizes, at) = choices(&WindowSizes {
            size: (1600, 900),
            usable,
            full_screen: false,
        });
        assert_eq!(names[at], "1600 \u{d7} 900");
        assert_eq!(sizes.len(), offered(usable).len());
        let (names, sizes, at) = choices(&WindowSizes {
            size: (1503, 877),
            usable,
            full_screen: false,
        });
        assert_eq!(at, sizes.len() - 1);
        assert_eq!(names[at], "Custom (1503 \u{d7} 877)");
        assert_eq!(sizes[at], (1503, 877));
    }

    #[test]
    fn full_screen_offers_the_screens_size_alone() {
        let (names, sizes, at) = choices(&WindowSizes {
            size: (2560, 1440),
            usable: Some((2544, 1381)),
            full_screen: true,
        });
        assert_eq!(
            (names, sizes, at),
            (vec!["2560 \u{d7} 1440".to_owned()], vec![(2560, 1440)], 0)
        );
    }

    /// A size the adapter does not list is kept as `1366x768` and read back the same, so an old
    /// `Display.Resolution` line and a new one load alike.
    #[test]
    fn a_window_size_is_kept_in_the_preference_files_own_form() {
        use crate::options::store;
        store::clear_display_choices();
        let packed = store::mode_desc(1366, 768);
        let text =
            store::convert_to_string(store::DISPLAY_RESOLUTION, &crate::PrefValue::Int(packed));
        assert_eq!(text, "1366x768");
        assert_eq!(
            store::display_choice(store::DISPLAY_RESOLUTION, &text),
            Some(packed)
        );
        assert_eq!(
            store::display_choice(store::DISPLAY_RESOLUTION, "1024x768"),
            Some(store::mode_desc(1024, 768))
        );
    }
}
