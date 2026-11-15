//! The claims a scenario asserts: each scenario declares the ids it asserts, and its test fails
//! unless it asserted exactly those.

use std::cell::RefCell;
use std::collections::BTreeSet;

/// One scenario, as its file's `ALL` lists it: its name, the claim ids it asserts, and the function.
pub type Scenario = (&'static str, &'static [&'static str], fn());

thread_local! {
    /// The ids noted since [`record`] began on this thread, or `None` when nothing is recording.
    ///
    /// **Per thread, and only while a scenario runs.** A process-global set cannot say *which* scenario asserted an id, which is
    /// what makes a declaration checkable; and the `cpu` binary runs its tests in parallel, so a
    /// shared set would have mixed two scenarios' assertions together.
    static RECORDING: RefCell<Option<BTreeSet<&'static str>>> = const { RefCell::new(None) };
}

/// Run `f` and answer with the claim ids it asserted.
///
/// # Panics
/// Panics when a recording is already open on this thread: recordings do not nest, and a nested
/// one would hand its ids to the wrong scenario.
pub fn record(f: impl FnOnce()) -> BTreeSet<&'static str> {
    /// Ends the recording however `f` ends -- a scenario that fails its own assertion unwinds, and
    /// with `--test-threads=1` the next scenario runs on this same thread.
    struct Open;
    impl Drop for Open {
        fn drop(&mut self) {
            RECORDING.with(|r| *r.borrow_mut() = None);
        }
    }

    RECORDING.with(|r| {
        let mut slot = r.borrow_mut();
        assert!(
            slot.is_none(),
            "a scenario is already recording on this thread"
        );
        *slot = Some(BTreeSet::new());
    });
    let open = Open;
    f();
    let ids = RECORDING
        .with(|r| r.borrow_mut().take())
        .unwrap_or_default();
    drop(open);
    ids
}

/// Run one scenario of `all` by name, and assert that the ids it asserted are **exactly** the ones
/// its entry declares.
///
/// Every `#[test] fn scenario_*` in a scenario file calls this.
///
/// # Panics
/// Panics when `name` is in no entry of `all`, and when the two sets differ.
pub fn run_scenario(all: &[Scenario], name: &str) {
    let (_, declared, f) = all.iter().find(|(n, _, _)| *n == name).unwrap_or_else(|| {
        panic!("no scenario named {name:?} is listed in this file's ALL, so it cannot be checked")
    });
    let asserted = record(*f);
    let declared: BTreeSet<&'static str> = declared.iter().copied().collect();
    let undeclared: Vec<&str> = asserted.difference(&declared).copied().collect();
    let unasserted: Vec<&str> = declared.difference(&asserted).copied().collect();
    assert!(
        undeclared.is_empty() && unasserted.is_empty(),
        "the scenario {name} asserted {asserted:?} and its ALL entry declares {declared:?}: it \
         asserted {undeclared:?} without declaring them, and declares {unasserted:?} without \
         asserting them"
    );
}

/// Record `id` as asserted. Called by `HeadlessClient::assert_behaviour`.
///
/// Outside a [`record`] it counts nothing.
pub fn note_asserted(id: &'static str) {
    RECORDING.with(|r| {
        if let Some(open) = r.borrow_mut().as_mut() {
            open.insert(id);
        }
    });
}
