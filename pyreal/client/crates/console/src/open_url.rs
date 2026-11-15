//! The Mac's open-URL Apple Event (`kInternetEventClass`/`kAEGetURL`), which winit does not
//! expose: a handler installed when the application is about to finish launching, so the link it
//! was launched with arrives too, and kept for the life of the process.

use std::sync::Mutex;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{class, define_class, msg_send, sel, ClassType};
use objc2_foundation::NSString;

/// The links delivered and not yet taken.
static LINKS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// `kInternetEventClass` and `kAEGetURL`, both `'GURL'`.
const GURL: u32 = u32::from_be_bytes(*b"GURL");
/// `keyDirectObject`, the event's URL.
const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and this class does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[name = "PyrealOpenUrlReceiver"]
    struct Receiver;

    impl Receiver {
        #[unsafe(method(willFinishLaunching:))]
        fn will_finish_launching(&self, _note: &AnyObject) {
            self.listen();
        }

        #[unsafe(method(handleGetURLEvent:withReplyEvent:))]
        fn get_url(&self, event: &AnyObject, _reply: &AnyObject) {
            // SAFETY: `event` is an `NSAppleEventDescriptor`; both selectors are its own and return
            // an object or nil.
            let text: Option<Retained<NSString>> = unsafe {
                let param: Option<Retained<AnyObject>> =
                    msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT];
                match param {
                    Some(p) => msg_send![&p, stringValue],
                    None => None,
                }
            };
            if let (Some(text), Ok(mut links)) = (text, LINKS.lock()) {
                links.push(text.to_string());
            }
        }
    }
);

impl Receiver {
    /// Make this the open-URL handler.
    fn listen(&self) {
        // SAFETY: the shared manager always exists; the handler outlives the process (it is never
        // released) and its selector takes (event, reply) as the manager calls it.
        unsafe {
            let manager: Retained<AnyObject> =
                msg_send![class!(NSAppleEventManager), sharedAppleEventManager];
            let () = msg_send![
                &manager,
                setEventHandler: self,
                andSelector: sel!(handleGetURLEvent:withReplyEvent:),
                forEventClass: GURL,
                andEventID: GURL
            ];
        }
    }
}

/// Install the handler now and again when the application is about to finish launching (AppKit
/// installs its own then). Call once, on the main thread, before the event loop first runs.
pub fn install() {
    // SAFETY: `new` on an NSObject subclass with no ivars; the default notification centre takes
    // the observer, selector, a constant name and a nil object.
    unsafe {
        let receiver: Retained<Receiver> = msg_send![Receiver::class(), new];
        receiver.listen();
        let center: Retained<AnyObject> = msg_send![class!(NSNotificationCenter), defaultCenter];
        let name = NSString::from_str("NSApplicationWillFinishLaunchingNotification");
        let () = msg_send![
            &center,
            addObserver: &*receiver,
            selector: sel!(willFinishLaunching:),
            name: &*name,
            object: std::ptr::null::<AnyObject>()
        ];
        // Kept for the process: the manager and the centre do not retain it.
        std::mem::forget(receiver);
    }
}

/// The links delivered since the last call, oldest first.
pub fn take() -> Vec<String> {
    LINKS
        .lock()
        .map(|mut links| std::mem::take(&mut *links))
        .unwrap_or_default()
}
