//! Every examine route -- the examine cursor's pick, the examine target mode, the identify button
//! and the selection-examine key -- sends one `0x00C8 Item_Appraise` and arms the awaited
//! appraisal id and the examine serial, so the reply that answers it is accepted as new.
//! Fixture: the retail dats and the recorded captures. No recording holds a 3D pick, so the routes
//! are driven directly on a headless bench seeded from a recording.

use crate::common::captures_dir;
use crate::common::client_dir;
use crate::common::recorded_sessions;

use std::net::SocketAddr;

use pyreal_client_contract::view::{TargetMode, UiRequest};
use pyreal_client_net::client_session::SessionEvent;
use pyreal_client_runtime::interaction::UiMouseEvent;
use pyreal_client_runtime::net::ClientNetwork;
use pyreal_client_runtime::objects::ObjectStream;
use pyreal_primitives::{LocalTime, ObjectId, ServerTime};

// ---------------------------------------------------------------------------------------------
// Harness: the recorded-capture reader.
// ---------------------------------------------------------------------------------------------

fn have_dats() {
    assert!(
        pyreal_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set PYREAL_TEST_DAT_DIR",
        client_dir().display()
    );
}

use pyreal_client_net::client_session::testing::capture::{self, Datagram as Record};

fn corpus_sessions() -> Vec<String> {
    let dir = captures_dir();
    let mut out: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter(|e| !crate::common::is_unclean_logout_recording(&e.path()))
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                return None;
            }
            p.file_stem().and_then(|x| x.to_str()).map(str::to_owned)
        })
        .collect();
    out.sort();
    // The number is the corpus index's own, so a new recording re-measures this scan instead of
    // failing it.
    assert_eq!(
        out.len(),
        recorded_sessions(),
        "the recorded captures; found {out:?}"
    );
    out
}

fn load(session: &str) -> Vec<Record> {
    capture::shared_session(session).to_vec()
}

fn connection_sequence_number(records: &[Record]) -> u32 {
    pyreal_client_net::recording::connection_sequence_number(records).unwrap_or(0)
}

fn addr(pair: u16) -> SocketAddr {
    capture::peer(pair)
}

// ---------------------------------------------------------------------------------------------
// The four routes
// ---------------------------------------------------------------------------------------------

/// The four examine routes and how this headless fixture drives them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    /// Direct wrapper mouse press/release followed by an injected object-found answer for the
    /// examine search reason. No rendered 3D pick runs here.
    Cursor,
    /// A directly queued examine target mode, direct wrapper click, and injected object-found
    /// answer. No toolbar button hit-test runs here.
    TargetMode,
    /// A directly queued `UiRequest::Examine` for the selected target.
    Button,
    /// Input action `0x1000002B SelectionExamine`.
    Key,
}

const ROUTES: [Route; 4] = [Route::Cursor, Route::TargetMode, Route::Button, Route::Key];

/// Build the direct input event used by the key route. The bench has no OS-key producer, 3D world
/// or network owner: `use_time` receives `world: None` and `net: None`, and the request remains in
/// `Interaction::last_sent` or the pending queue where the test reads its enum value.
fn key(action: u32) -> pyreal_client_runtime::actions::Action {
    pyreal_client_runtime::actions::Action {
        id: pyreal_input::ActionId(action),
        phase: pyreal_client_runtime::actions::ActionPhase::Repeat,
        extent: 1.0,
        repeats: 1,
    }
}

struct Bench {
    inter: pyreal_client_runtime::interaction::Interaction,
    objects: ObjectStream,
    store: pyreal_dat::RetailDatStore,
}

impl Bench {
    /// Fill the game tables from recorded server events rather than inventing an object id. The
    /// selection scans each recording until it finds the first inventory item with a live object,
    /// falling back to the first non-player live object, and stops as soon as a target is found.
    /// This supplies the object-found handler's `game.weenie(id).is_some()` precondition; it is not
    /// a complete corpus inventory census.
    ///
    /// Returns the bench and the object it will examine, which is one the recording created.
    fn new() -> (Self, ObjectId) {
        have_dats();
        let mut b = Self {
            inter: pyreal_client_runtime::interaction::Interaction::new(),
            objects: ObjectStream::new(),
            store: pyreal_dat::RetailDatStore::open_dir(&client_dir()).expect("the retail dats"),
        };
        let mut target = None;
        let mut tried = Vec::new();
        for session in corpus_sessions() {
            let records = load(&session);
            let csn = connection_sequence_number(&records);
            let mut net =
                ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn).expect("net");
            let mut entered = false;
            let mut peak = 0usize;
            let mut saw_player = false;
            for r in &records {
                let now = LocalTime(r.t);
                if !r.c2s {
                    net.feed(&r.raw, addr(r.pair), now);
                }
                net.tick(now);
                let _ = net.take_outgoing();
                let events = b.objects.pump(&mut net, now);
                for e in &events {
                    if let SessionEvent::CharacterSet(set) = e {
                        if !entered {
                            if let Some(c) = set.characters.first() {
                                let account = set.account.clone();
                                net.enter_world(c.gid, &account);
                                entered = true;
                            }
                        }
                    }
                }
                peak = peak.max(b.objects.world.tables.weenies.len());
                saw_player |= b.objects.world.player.is_some();
                // Any object the recording created that is not the player itself -- the pick's
                // own precondition is that the id names a known object, and nothing more.
                if let Some(p) = b.objects.world.player {
                    if let Some(id) = b
                        .objects
                        .world
                        .inventory(p)
                        .and_then(|inv| {
                            inv.items
                                .iter()
                                .copied()
                                .find(|i| b.objects.world.weenie(*i).is_some())
                        })
                        .or_else(|| {
                            b.objects
                                .world
                                .tables
                                .weenies
                                .iter()
                                .map(|(id, _)| id)
                                .find(|id| *id != p)
                        })
                    {
                        target = Some(id);
                        break;
                    }
                }
            }
            // The denominator distinguishes a replay that found no target from one that never
            // ran. Peak is used because each log-off resets the world tables.
            tried.push(format!(
                "{session}: saw_player={saw_player} peak_weenies={peak}"
            ));
            if target.is_some() {
                break;
            }
        }
        let target = target.unwrap_or_else(|| {
            panic!("no capture puts an object in the tables to examine -- {tried:?}")
        });
        // The replay may have left requests of its own in the outbox; this bench measures what the
        // *gesture* sends, so start from a clean one.
        b.inter.take_pending_requests();
        b.inter.last_sent.clear();
        (b, target)
    }

    fn use_time(&mut self, actions: Vec<pyreal_client_runtime::actions::Action>, now: f64) {
        pyreal_client_runtime::interaction::use_time(
            &mut self.inter,
            &self.store,
            None,
            &mut self.objects,
            None,
            actions,
            false,
            (800, 600),
            LocalTime(now),
        );
    }

    /// Every pending or last-sent `Request::Appraise` this bench has produced, as its target.
    ///
    /// Both halves of the outbox: `use_time` step 4 drains `Interaction::outbox` into `last_sent`,
    /// so a request made *during* a `use_time` is in the second and one made after it (the two
    /// routes that finish in `on_world_object_found`) is still in the first.
    fn appraises(&self) -> Vec<ObjectId> {
        self.inter
            .last_sent
            .iter()
            .chain(self.inter.pending_requests())
            .filter_map(|r| match r {
                pyreal_client_model::Request::Appraise(a) => Some(a.target),
                _ => None,
            })
            .collect()
    }

    fn awaiting(&self) -> Option<ObjectId> {
        self.objects.world.appraisal.examining
    }

    fn serial(&self) -> u64 {
        self.objects.world.appraisal.examine_serial
    }

    /// Drive one route at `target`. The callers inspect request enums, awaiting id and serial.
    fn drive(&mut self, route: Route, target: ObjectId, now: f64) {
        use pyreal_client_contract::actions::ui as action;
        match route {
            Route::Cursor => {
                // Drive a direct right press/release at the same coordinates. In the original
                // frame order, mouse release and the later object-found notice share a frame. The
                // notice is raised outside the player-present guard, so inserting `use_time`
                // between gesture and injected answer supplies object id zero and clears the search
                // reason. That older fixture shape could observe only a reason that survived a
                // frame. This bench instead reads the armed reason before it directly supplies the
                // found id; it does not run the 3D pick or claim an OS mouse producer.
                for start in [true, false] {
                    self.inter.wrapper_mouse(
                        UiMouseEvent {
                            action: action::SECONDARY_CLICK,
                            start,
                            x: 400,
                            y: 300,
                            over: None,
                        },
                        (800, 600),
                        true,
                    );
                }
                // The gesture armed the pick. Assert that, rather than assume it: the whole route
                // dies quietly if the release is read as a camera drag.
                assert_eq!(
                    format!("{:?}", self.inter.search_reason()),
                    "Examine",
                    "the direct right-button release must arm the examine search reason"
                );
                assert!(
                    self.inter.pick.looking_for_object(),
                    "and the world-object lookup remains armed for the injected answer"
                );
                // Supply the object-found answer that the omitted rendered pick would have raised.
                self.inter
                    .on_world_object_found(target, &mut self.objects.world, ServerTime(now));
            }
            Route::TargetMode => {
                // Queue examine target mode directly, then send a direct left press which arms
                // targeted use. The object-found answer below completes the route.
                self.inter
                    .queue(vec![], vec![UiRequest::SetTargetMode(TargetMode::Examine)]);
                self.use_time(vec![], now);
                // Drive only the direct wrapper input before supplying the answer.
                self.inter.wrapper_mouse(
                    UiMouseEvent {
                        action: action::PRIMARY_CLICK,
                        start: true,
                        x: 400,
                        y: 300,
                        over: None,
                    },
                    (800, 600),
                    true,
                );
                assert_eq!(
                    format!("{:?}", self.inter.search_reason()),
                    "TargetedUse",
                    "a direct left click with examine target mode arms targeted-use search"
                );
                assert!(self.inter.pick.looking_for_object(), "and armed the pick");
                self.inter
                    .on_world_object_found(target, &mut self.objects.world, ServerTime(now));
            }
            Route::Button => {
                self.inter.queue(vec![], vec![UiRequest::Examine(target)]);
                self.use_time(vec![], now);
            }
            Route::Key => {
                // Selection-examine reads the selected object, so select this recorded target
                // before supplying the direct input event.
                self.inter.queue(vec![], vec![UiRequest::Select(target)]);
                self.use_time(vec![], now);
                assert_eq!(self.objects.world.selected, Some(target));
                self.use_time(
                    vec![key(
                        pyreal_client_contract::actions::mapped::SELECTION_EXAMINE.0,
                    )],
                    now + 0.1,
                );
            }
        }
    }
}

/// Both halves, asserted separately for each route: the `Item_Appraise` request, and the awaited
/// appraisal id that lets the reply open the panel. A test of the request alone would pass for a
/// route that never arms the panel. The routes are counted, so "the route never ran" and "the route
/// ran and did not arm" are different answers.
#[test]
fn every_examine_route_reaches_examine_object_and_sends_the_same_request() {
    let mut armed = 0;
    for (i, route) in ROUTES.into_iter().enumerate() {
        let (mut b, target) = Bench::new();
        assert!(
            b.objects.world.weenie(target).is_some(),
            "{route:?}: the object the gesture names is one the recording created"
        );
        assert_eq!(
            b.awaiting(),
            None,
            "{route:?}: nothing is awaited before the gesture"
        );
        assert_eq!(b.serial(), 0);

        b.drive(route, target, 1.0 + i as f64);

        // Half one: exactly one pending/last-sent Item_Appraise request enum naming the target.
        // This station does not serialize or inspect wire bytes.
        assert_eq!(
            b.appraises(),
            vec![target],
            "{route:?}: exactly one Item_Appraise, for the object the gesture named"
        );
        // Half two: the awaited appraisal id and the examine serial.
        assert_eq!(
            b.awaiting(),
            Some(target),
            "{route:?}: the examine notice must set the awaiting appraisal id; otherwise the \
             reply is not accepted as new and the panel's visible flag is never set"
        );
        assert_eq!(b.serial(), 1, "{route:?}: one notice, not two");
        armed += 1;
    }
    assert_eq!(
        armed, 4,
        "all four examine routes arm the awaited appraisal id"
    );
}

/// The examine operation returns immediately for id zero, and
/// `ObjectId(0)` is exactly what a pick that found nothing carries. Without the guard the panel
/// would arm on empty air and the next unrelated reply would be refused for the wrong reason.
#[test]
fn examining_the_null_object_arms_nothing_and_sends_nothing() {
    let (mut b, _) = Bench::new();
    b.inter.queue(vec![], vec![UiRequest::Examine(ObjectId(0))]);
    b.use_time(vec![], 1.0);
    assert_eq!(b.appraises(), Vec::<ObjectId>::new());
    assert_eq!(b.awaiting(), None);
    assert_eq!(b.serial(), 0);
}
