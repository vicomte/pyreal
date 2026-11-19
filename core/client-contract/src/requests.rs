//! The outbound request queue.
//!
//! A UI appends requests to its `Outbox`, an ordinary value it owns, in execution order, and the
//! host drains it at fixed points of the frame.

use crate::view::UiRequest;

/// The request queue a panel appends to, as a value.
#[derive(Debug, Default)]
pub struct Outbox(Vec<UiRequest>);

impl Outbox {
    /// An empty queue, owned by whoever holds it.
    #[must_use]
    pub fn owned() -> Self {
        Self(Vec::new())
    }

    /// Append one request. Called from panel message handlers.
    pub fn emit(&mut self, r: UiRequest) {
        self.0.push(r);
    }

    /// Take everything queued so far, in emission order, and leave the queue empty.
    #[must_use]
    pub fn take(&mut self) -> Vec<UiRequest> {
        std::mem::take(&mut self.0)
    }

    /// Extract only local `PlayerModule` placement writes, preserving every other request in
    /// order. The host consumes these before PM restoration, and drops old writes when a new
    /// authoritative `PlayerDescription` replaces that module. They must not wait for the next
    /// interaction frame.
    #[must_use]
    pub fn take_placement_updates(&mut self) -> Vec<UiRequest> {
        let mut placements = Vec::new();
        let mut others = Vec::new();
        for request in std::mem::take(&mut self.0) {
            if is_local_module_write(&request) {
                placements.push(request);
            } else {
                others.push(request);
            }
        }
        self.0 = others;
        placements
    }

    /// Drop everything queued. Used by tests between cases, and by the host on a mode switch, where
    /// the client's equivalent is that the objects that would have made the calls no longer exist.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// How many requests are queued, without taking them.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the queue is empty, without taking it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[must_use]
pub fn is_numeric_placement_update(request: &UiRequest) -> bool {
    matches!(
        request,
        UiRequest::SetChatWindowOption {
            property: 0x1000_0086..=0x1000_008A,
            ..
        }
    )
}

/// Every request whose whole effect is a write into the retained `PlayerModule` — the placement
/// blob **and** the Chat Options page's two kinds of write.
///
/// They belong together because they are the same code in the client:
/// the chat-window option write and the general option write both end in the player module's own
/// changed hook, which raises a **local** notice
/// and sets the 480-second deferred-save flag. None of the three puts a byte on the wire, and all
/// three have to land before the frame's `PlayerModule` read-back or the page would show the
/// value it just replaced.
#[must_use]
pub fn is_local_module_write(request: &UiRequest) -> bool {
    is_numeric_placement_update(request)
        || matches!(
            request,
            UiRequest::SetChatWindowFilter { .. } | UiRequest::SetChatOpacity { .. }
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyreal_primitives::ObjectId;

    /// Requests come back in the order they were appended.
    #[test]
    fn requests_come_back_in_emission_order_and_the_queue_empties() {
        let mut out = Outbox::owned();
        out.emit(UiRequest::Select(ObjectId(1)));
        out.emit(UiRequest::Use(ObjectId(2)));
        assert_eq!(out.len(), 2);
        let got = out.take();
        assert_eq!(
            got,
            vec![UiRequest::Select(ObjectId(1)), UiRequest::Use(ObjectId(2))]
        );
        assert_eq!(out.len(), 0);
        assert!(out.take().is_empty());
    }
}
