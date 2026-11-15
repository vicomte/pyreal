//! Copying text to the host clipboard and reading it back.
//!
//! The interface copies on a Copy and reads the clipboard on a paste key; the host supplies the
//! clipboard ([`crate::platform::host::Host::clipboard`]): on the desktop the crate the workspace
//! confines the clipboard's unsafe to. **This file contains no unsafe
//! code.** [`HostClipboard`] is the seam, so the routing is tested headless against
//! [`FakeClipboard`].

/// Why the host's clipboard refused, in the host's own words. Its `Debug` is those words alone, so
/// the warning a failure logs reads as the host's error would.
#[derive(Clone, PartialEq, Eq)]
pub struct ClipboardError(pub String);

impl std::fmt::Debug for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The host's clipboard, behind a seam so the routing can be tested without a window station.
pub trait HostClipboard {
    /// Send text as `CF_UNICODETEXT`, preserving its UTF-16 representation.
    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError>;
    /// `Ok(None)` when no `CF_UNICODETEXT` is on offer.
    fn get_text(&mut self) -> Result<Option<String>, ClipboardError>;
    /// `GetClipboardSequenceNumber`, or `None` where there is no clipboard to poll.
    fn sequence_number(&mut self) -> Option<u32>;
}

/// No clipboard at all: nothing is sent anywhere and the sequence number is `None`, so the mirror
/// is never refreshed. What a host with no clipboard supplies.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoClipboard;

impl HostClipboard for NoClipboard {
    fn set_text(&mut self, _text: &str) -> Result<(), ClipboardError> {
        Ok(())
    }
    fn get_text(&mut self) -> Result<Option<String>, ClipboardError> {
        Ok(None)
    }
    fn sequence_number(&mut self) -> Option<u32> {
        None
    }
}

/// An in-memory stand-in, so the routing above can be asserted without a window station.
///
/// Deliberately in the library and not in a test file: the clipboard tests and any later
/// caller need the same one, and it also documents what the trait's contract is.
#[derive(Debug, Default)]
pub struct FakeClipboard {
    /// What `set_text` was handed, in order. The assertion target.
    pub sent: Vec<String>,
    /// What `get_text` will answer.
    pub contents: Option<String>,
    /// What `sequence_number` will answer.
    pub seq: Option<u32>,
    /// How many times the host was actually read.
    pub reads: u32,
}

impl HostClipboard for FakeClipboard {
    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.sent.push(text.to_string());
        self.contents = Some(text.to_string());
        // A real `SetClipboardData` bumps the sequence number; the fake must too, or a test would
        // never see the write-then-refresh ordering the real one has.
        self.seq = Some(self.seq.unwrap_or(0) + 1);
        Ok(())
    }
    fn get_text(&mut self) -> Result<Option<String>, ClipboardError> {
        self.reads += 1;
        Ok(self.contents.clone())
    }
    fn sequence_number(&mut self) -> Option<u32> {
        self.seq
    }
}
