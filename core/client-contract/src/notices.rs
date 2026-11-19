//! The inbound **notice** queue — the mirror of [`crate::requests`].
//!
//! `crate::requests` carries UI -> client systems; this carries the other direction, for the one
//! system that needs it: the combat system's magic-action handler raises a magic notice and the
//! UI's spell bar receives it, one frame later. In retail the notice runs inside the input
//! dispatch; nothing here can observe the difference.

use crate::view::MagicNotice;

/// The notice queue the UI drains, as a value.
#[derive(Debug, Default)]
pub struct NoticeInbox {
    magic: Vec<MagicNotice>,
}

impl NoticeInbox {
    /// A private queue, owned by whoever holds it.
    #[must_use]
    pub fn owned() -> Self {
        Self::default()
    }

    /// Append one notice from the magic-action handler's counterpart.
    pub fn emit(&mut self, n: MagicNotice) {
        self.magic.push(n);
    }

    /// Take everything queued so far, in emission order, and leave the queue empty.
    #[must_use]
    pub fn take(&mut self) -> Vec<MagicNotice> {
        std::mem::take(&mut self.magic)
    }

    /// How many notices are queued, without taking them.
    #[must_use]
    pub fn len(&self) -> usize {
        self.magic.len()
    }

    /// Whether the magic-notice queue is empty, without taking it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.magic.is_empty()
    }

    /// Drop everything queued.
    pub fn clear(&mut self) {
        self.magic.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: [`crate::requests`]'s own contract, which this module mirrors — "appended in
    /// dispatch order and drained by the owner". The order is the observable part: two quickslot
    /// keys pressed in one frame must cast in the order they were pressed.
    #[test]
    fn notices_come_back_in_emission_order_and_the_queue_empties() {
        let mut inbox = NoticeInbox::owned();
        assert_eq!(inbox.len(), 0);
        inbox.emit(MagicNotice::CastQuickslotSpell { slot: 0 });
        inbox.emit(MagicNotice::NextSpellSelection);
        inbox.emit(MagicNotice::CastQuickslotSpell { slot: 3 });
        assert_eq!(inbox.len(), 3);
        assert_eq!(
            inbox.take(),
            vec![
                MagicNotice::CastQuickslotSpell { slot: 0 },
                MagicNotice::NextSpellSelection,
                MagicNotice::CastQuickslotSpell { slot: 3 },
            ]
        );
        assert_eq!(inbox.len(), 0, "taking empties it");
        assert!(
            inbox.take().is_empty(),
            "and a second take is not the first take again"
        );
    }
}
