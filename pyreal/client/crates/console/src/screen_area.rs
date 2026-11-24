//! The Mac's usable screen area: a screen less its menu bar and Dock, which winit does not expose.

use objc2::encode::{Encode, Encoding};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::NSString;

/// `CGPoint` and `CGSize` alike: two `CGFloat`s.
#[repr(C)]
#[derive(Clone, Copy)]
struct Pair {
    a: f64,
    b: f64,
}

/// `NSRect`, which is `CGRect`: an origin and a size.
#[repr(C)]
#[derive(Clone, Copy)]
struct NSRect {
    #[allow(dead_code)]
    origin: Pair,
    size: Pair,
}

// SAFETY: both are `#[repr(C)]` with the fields and order of `CGPoint`/`CGSize` and `CGRect`.
unsafe impl Encode for NSRect {
    const ENCODING: Encoding = Encoding::Struct(
        "CGRect",
        &[
            Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]),
            Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]),
        ],
    );
}

/// The usable width and height, in points, of the screen whose display id is `display` (a
/// `CGDirectDisplayID`), or of the main screen when none has it.
pub fn usable_points(display: u32) -> Option<(f64, f64)> {
    // SAFETY: `NSScreen`'s class methods and `deviceDescription`, `objectForKey:`,
    // `unsignedIntValue` and `visibleFrame` are AppKit's own, read only, and answer an object, nil
    // or a plain value. Nothing is retained past the call.
    unsafe {
        let screens: Option<Retained<AnyObject>> = msg_send![class!(NSScreen), screens];
        let key = NSString::from_str("NSScreenNumber");
        let mut found: Option<Retained<AnyObject>> = None;
        if let Some(screens) = screens {
            let count: usize = msg_send![&screens, count];
            for i in 0..count {
                let screen: Retained<AnyObject> = msg_send![&screens, objectAtIndex: i];
                let about: Option<Retained<AnyObject>> = msg_send![&screen, deviceDescription];
                let id: Option<Retained<AnyObject>> = match about {
                    Some(about) => msg_send![&about, objectForKey: &*key],
                    None => None,
                };
                let id: Option<u32> = id.map(|id| msg_send![&id, unsignedIntValue]);
                if id == Some(display) {
                    found = Some(screen);
                    break;
                }
            }
        }
        let screen = match found {
            Some(s) => s,
            None => msg_send![class!(NSScreen), mainScreen],
        };
        let area: NSRect = msg_send![&screen, visibleFrame];
        (area.size.a > 0.0 && area.size.b > 0.0).then_some((area.size.a, area.size.b))
    }
}
