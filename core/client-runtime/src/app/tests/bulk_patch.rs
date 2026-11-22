//! The patch stream as the frame loop applies it (CD-044): its records wait for the world's
//! manifest, and a frame applies only its share of them.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pyreal_client_contract::pregame::DddEvent as UiDdd;
use pyreal_client_net::client_session::{DddEvent, SessionEvent};
use pyreal_dat::RetailDatStore;
use pyreal_primitives::DataId;
use pyreal_protocol::admin::{
    DddBeginDdd, DddData, DddInterrogation, DddOverlayManifest, OverlayFileManifest, OverlayRecord,
    PatchRevision,
};

use super::*;

/// The most one wait may take: a guard against a hang, never the expected time.
const GUARD: Duration = Duration::from_secs(60);
/// The first of the pictures the tests' world adds.
const FIRST: u32 = 0x0600_E000;

fn retail_store() -> Arc<RetailDatStore> {
    Arc::new(pyreal_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are this test's oracle and they are not under {}",
            pyreal_dat::testing::dat_dir().display()
        )
    }))
}

/// A scripted run with no interface, keeping the world's records in an overlay at `folder`.
fn app(store: &Arc<RetailDatStore>, settings: &Path, folder: PathBuf) -> App<NullShell> {
    let cfg = Config {
        headless: true,
        frames: None,
        connect: false,
        sound: false,
        host: "127.0.0.1".into(),
        port: 9000,
        account: "builder".into(),
        vg_password: "password".into(),
        enter_world: true,
        start_char: "Builder".into(),
        preferences_file: settings.join("UserPreferences.ini"),
        dat_dir: PathBuf::from("a directory that does not exist"),
        overlay_dat_dir: Some(folder),
        ..Config::default()
    };
    let mut app = App::<NullShell>::bring_up_with_store(
        cfg,
        Some(Arc::clone(store)),
        |_| Ok(Platform::headless(64, 64)),
        |_, _, _, _| Ok(Box::new(crate::present::NullPresentation::new(64, 64))),
    )
    .expect("bring-up");
    let net = crate::net::ClientNetwork::new("127.0.0.1", 9000, "builder", "password", 1)
        .expect("an endpoint");
    assert!(app.attach_replay_network(net).is_ok());
    app
}

/// One frame of the server's `events`.
fn frame(app: &mut App<NullShell>, events: Vec<DddEvent>) {
    let events = events.into_iter().map(SessionEvent::Ddd).collect();
    app.process_logon_event_queue(&mut NullShell, events);
}

/// The world the tests' server patches: `n` pictures after [`FIRST`], over the player's portal.
fn world(store: &RetailDatStore, n: u32) -> (DddOverlayManifest, DddBeginDdd, Vec<DddData>) {
    let records: Vec<DddData> = (0..n)
        .map(|i| {
            let data = vec![u8::try_from(i % 251).unwrap(); 64 + i as usize];
            DddData {
                dat_file_type: 0,
                dat_file_id: 1,
                resource_type: 6,
                resource_id: FIRST + i,
                iteration: 1,
                compressed: 0,
                version: 1,
                data_size: u32::try_from(data.len() + 4).unwrap(),
                data,
            }
        })
        .collect();
    let manifest = DddOverlayManifest {
        world_key: "a streamed world".into(),
        total_bytes: 0,
        files: vec![OverlayFileManifest {
            dat_file_type: 0,
            dat_file_id: 1,
            base_name: "client_portal.dat".into(),
            base_fingerprint: pyreal_dat::overlay::fingerprint(store.portal()),
            base_records: None,
            base_iterations: 0,
            exact_iterations: false,
            revisions: Vec::new(),
            records: records
                .iter()
                .map(|d| OverlayRecord {
                    id: d.resource_id,
                    iteration: 1,
                    size: u32::try_from(d.data.len()).unwrap(),
                    sha256: pyreal_dat::overlay::record_hash(&d.data),
                })
                .collect(),
            tombstones: Vec::new(),
        }],
    };
    let next = pyreal_dat::overlay::base_iterations(store.portal())
        .last()
        .copied()
        .unwrap_or(0)
        + 1;
    let begin = DddBeginDdd {
        data_expected: 0,
        revisions: vec![PatchRevision {
            dat_file_type: 0,
            dat_file_id: 1,
            iteration: next,
            ids_to_download: records.iter().map(|d| d.resource_id).collect(),
            ids_to_purge: Vec::new(),
        }],
    };
    (manifest, begin, records)
}

/// A stand-in stream server: on the client's hello it sends each of `records` and the end.
fn serve(records: &[DddData]) -> (SocketAddr, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let frames: Vec<Vec<u8>> = records
        .iter()
        .map(|d| {
            let body = pyreal_protocol::write_body(d).unwrap();
            let mut f = u32::try_from(body.len()).unwrap().to_le_bytes().to_vec();
            f.extend_from_slice(&body);
            f
        })
        .collect();
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut hello = [0u8; 24];
        s.read_exact(&mut hello).unwrap();
        for f in &frames {
            s.write_all(f).unwrap();
        }
        s.write_all(&0u32.to_le_bytes()).unwrap();
    });
    (addr, server)
}

/// Open the stream to `records`' server, and wait until it has read every one of them.
fn streamed(app: &mut App<NullShell>, records: &[DddData]) {
    let (addr, server) = serve(records);
    app.bulk = Some(crate::bulk_patch::BulkDownload::start(addr, [5; 16]).expect("the stream"));
    server.join().unwrap();
    let bytes: usize = records.iter().map(|d| d.data.len()).sum();
    let since = Instant::now();
    while app.bulk.as_ref().is_some_and(|b| b.queued() < bytes) {
        assert!(since.elapsed() < GUARD, "the stream brings its records");
        std::thread::yield_now();
    }
}

/// The records the patch screen was told of since the last look.
fn downloaded(app: &mut App<NullShell>) -> usize {
    std::mem::take(&mut app.pending_ddd)
        .iter()
        .filter(|e| matches!(e, UiDdd::DataDownloaded { .. }))
        .count()
}

/// The stream's records that arrive before the world's manifest are not applied: they wait,
/// however many frames go by, and are applied once it is in. The patch then completes with every
/// record in the world's overlay and nothing refused.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_streamed_patch_whose_manifest_comes_after_the_stream_waits_for_it_and_is_kept() {
    let store = retail_store();
    let scratch = pyreal_dat::testing::ScratchDir::new("bulk-late-manifest").expect("scratch");
    let (manifest, begin, records) = world(&store, 40);
    let settings = scratch.path().join("settings");
    std::fs::create_dir_all(&settings).unwrap();
    let mut app = app(&store, &settings, scratch.path().join("overlay"));
    frame(
        &mut app,
        vec![DddEvent::Interrogation(DddInterrogation::default())],
    );
    // The begin and the stream come; the manifest, sent ahead of them, is still on its way.
    frame(&mut app, vec![DddEvent::Begin(begin)]);
    let _ = downloaded(&mut app);
    streamed(&mut app, &records);
    for _ in 0..5 {
        app.apply_bulk_records();
        assert_eq!(
            downloaded(&mut app),
            0,
            "nothing is applied before the manifest"
        );
    }

    frame(
        &mut app,
        vec![DddEvent::OverlayManifest(Box::new(manifest))],
    );
    let mut applied = 0;
    let since = Instant::now();
    while app.bulk.is_some() {
        assert!(since.elapsed() < GUARD, "the stream is applied");
        app.apply_bulk_records();
        applied += downloaded(&mut app);
    }
    assert_eq!(applied, records.len());
    assert_eq!(app.ddd().pending(), 0, "every record is applied");
    frame(&mut app, vec![DddEvent::End]);
    assert!(app.host_state().patch_finished);
    assert_eq!(app.world_refused(), None, "{:?}", app.ddd().notices());
    for d in &records {
        assert_eq!(
            app.store.read_portal(DataId(d.resource_id)).unwrap(),
            d.data,
            "{:#010X}",
            d.resource_id
        );
    }
}

/// A frame applies the stream's records only for its share of the frame, by the clock it is
/// given, and at least one: with every record already read and a clock that moves a millisecond
/// at each look, a 3 ms share applies three records a frame until none are left.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_streamed_patch_is_applied_a_frames_share_at_a_time() {
    let store = retail_store();
    let scratch = pyreal_dat::testing::ScratchDir::new("bulk-frame-share").expect("scratch");
    let (manifest, begin, records) = world(&store, 10);
    let settings = scratch.path().join("settings");
    std::fs::create_dir_all(&settings).unwrap();
    let mut app = app(&store, &settings, scratch.path().join("overlay"));
    let ticks = Arc::new(AtomicU64::new(0));
    let clock = Arc::clone(&ticks);
    app.patch_clock = crate::bulk_patch::WorkClock::new(move || {
        Duration::from_millis(clock.fetch_add(1, Ordering::Relaxed))
    });
    app.patch_share = Duration::from_millis(3);
    frame(
        &mut app,
        vec![DddEvent::Interrogation(DddInterrogation::default())],
    );
    frame(
        &mut app,
        vec![
            DddEvent::OverlayManifest(Box::new(manifest)),
            DddEvent::Begin(begin),
        ],
    );
    let _ = downloaded(&mut app);
    streamed(&mut app, &records);
    let mut per_frame = Vec::new();
    let since = Instant::now();
    while app.bulk.is_some() {
        assert!(since.elapsed() < GUARD, "the stream is applied");
        app.apply_bulk_records();
        per_frame.push(downloaded(&mut app));
    }
    // A frame that found the stream's end not yet in applies none.
    per_frame.retain(|&n| n > 0);
    assert_eq!(
        per_frame,
        [3, 3, 3, 1],
        "three a frame, the last frame what is left"
    );
    assert_eq!(app.ddd().pending(), 0, "every record is applied");
}
