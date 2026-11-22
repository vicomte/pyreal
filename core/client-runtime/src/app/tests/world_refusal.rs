//! A world whose data the client refuses is not played: refused at the start, the client never
//! connects; refused during the data patch, the session ends before character select. Each world a
//! server serves keeps its own folder in the server's folder, and what it keeps reaches the
//! preview spaces drawn before the world and the spells the interface shows.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pyreal_client_contract::pregame::{DisconnectNotice, GamePhase};
use pyreal_client_net::client_session::{DddEvent, SessionEvent};
use pyreal_dat::overlay::OverlayDir;
use pyreal_dat::{DatFile, ModernDat, RetailDatStore};
use pyreal_primitives::DataId;
use pyreal_protocol::admin::{DddBeginDdd, DddData, DddInterrogation, DddOverlayManifest};

use super::*;

/// A picture id the retail portal file holds, which the tests' worlds replace.
const PICTURE: DataId = DataId(0x0600_0001);

fn retail_store() -> Arc<RetailDatStore> {
    Arc::new(pyreal_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are this test's oracle and they are not under {}",
            pyreal_dat::testing::dat_dir().display()
        )
    }))
}

/// Write `bytes` as the picture of the world `world` in the overlay folder `dir`, in a portal
/// container made over `base`.
fn write_picture(dir: &Path, base: &DatFile, world: &str, bytes: &[u8]) {
    let dir = OverlayDir::new(dir).expect("an overlay folder");
    let mut w = dir
        .writer(ModernDat::Portal, base, "client_portal.dat", world, 1)
        .expect("an overlay writer");
    let next = pyreal_dat::overlay::base_iterations(base)
        .last()
        .copied()
        .unwrap_or(0)
        + 1;
    w.put(PICTURE, bytes, 1, next, 1).expect("a record");
    w.add_iteration(next, 1).expect("its revision");
    w.flush(1).expect("written");
}

fn bring_up(cfg: Config, store: &Arc<RetailDatStore>) -> App<NullShell> {
    App::<NullShell>::bring_up_with_store(
        cfg,
        Some(Arc::clone(store)),
        |_| Ok(Platform::headless(64, 64)),
        |_, _, _, _| Ok(Box::new(crate::present::NullPresentation::new(64, 64))),
    )
    .expect("bring-up")
}

/// A scripted run with no interface that connects to a server, its preferences in `settings`.
fn connecting(settings: &Path) -> Config {
    Config {
        headless: true,
        frames: None,
        connect: true,
        sound: false,
        host: "127.0.0.1".into(),
        port: 9000,
        account: "builder".into(),
        vg_password: "password".into(),
        enter_world: true,
        start_char: "Builder".into(),
        preferences_file: settings.join("UserPreferences.ini"),
        dat_dir: PathBuf::from("a directory that does not exist"),
        ..Config::default()
    }
}

/// Where the session stands, as the interfaces read it.
fn phase(app: &mut App<NullShell>) -> GamePhase {
    app.build_host_state();
    app.host_state().phase.clone()
}

fn character_set() -> SessionEvent {
    use pyreal_protocol::login::{CharacterIdentity, LoginCharacterSet};
    SessionEvent::CharacterSet(Box::new(LoginCharacterSet {
        characters: vec![CharacterIdentity {
            gid: pyreal_primitives::ObjectId(0x5000_0001),
            name: "Builder".into(),
            seconds_greyed_out: 0,
        }],
        account: "builder".into(),
        ..LoginCharacterSet::default()
    }))
}

/// A world whose folder the client refuses when it starts is never connected to, for each reason
/// the client can tell then: the folder's world is on the blocklist, it holds another world's
/// data, it cannot be read. The session's end says why, the run with no interface stops, and no
/// connection can be attached after. The same folder, not refused, is connected to, and so is one
/// made for other game files, whose overlay is thrown away unread.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_world_refused_at_the_start_is_never_connected_to_for_each_local_reason() {
    let store = retail_store();
    let scratch = pyreal_dat::testing::ScratchDir::new("refused-at-start").expect("scratch");
    type Make = fn(&Path, &RetailDatStore);
    let cases: [(&str, Make, &str); 3] = [
        (
            "blocked",
            |dir, store| {
                write_picture(dir, store.portal(), "Elsewhere", b"elsewhere");
                let settings = dir.parent().unwrap().join("blocked-settings");
                std::fs::create_dir_all(&settings).unwrap();
                std::fs::write(
                    settings.join(crate::world_overlay::BLOCKLIST_FILE),
                    "Elsewhere\n",
                )
                .unwrap();
            },
            "\"Elsewhere\" is on your overlay blocklist",
        ),
        (
            "other-world",
            |dir, store| {
                write_picture(dir, store.portal(), "Here", b"here");
                let dir = OverlayDir::new(dir).unwrap();
                let mut w = dir
                    .writer(
                        ModernDat::Cell,
                        store.cell(),
                        "client_cell_1.dat",
                        "There",
                        1,
                    )
                    .unwrap();
                w.put(DataId(0xA9B4_FFFF), b"there", 1, 1, 1).unwrap();
                w.flush(1).unwrap();
            },
            "holds another world's data (\"There\")",
        ),
        (
            "unreadable",
            |dir, _| {
                std::fs::create_dir_all(dir).unwrap();
                std::fs::write(
                    dir.join(pyreal_dat::overlay::container_name(ModernDat::Portal)),
                    b"not an overlay at all",
                )
                .unwrap();
            },
            "its folder could not be read",
        ),
    ];
    for (name, make, says) in cases {
        let folder = scratch.path().join(name);
        make(&folder, &store);
        let settings = scratch.path().join(format!("{name}-settings"));
        std::fs::create_dir_all(&settings).unwrap();
        let cfg = Config {
            overlay_dat_dir: Some(folder),
            ..connecting(&settings)
        };
        let mut app = bring_up(cfg, &store);
        assert!(app.link.is_none(), "{name}: never connected");
        let text = app.world_refused().map(str::to_owned);
        assert!(
            text.as_deref().is_some_and(|t| t.contains(says)),
            "{name}: {text:?}"
        );
        assert!(!app.store.has_overlay(), "{name}: nothing of it is read");
        assert_eq!(
            phase(&mut app),
            GamePhase::Disconnected(DisconnectNotice::OverlayRefused(text.unwrap())),
            "{name}"
        );
        let host = app.host_state();
        assert!(
            host.has_packet_controller && !host.connected && !host.patch_finished,
            "{name}: the screens show a run that connects, not connected, its data not checked"
        );
        app.process_logon_event_queue(&mut NullShell, Vec::new());
        assert_eq!(app.script, EnterWorldScript::Done, "{name}: the run stops");
        let net = crate::net::ClientNetwork::new("127.0.0.1", 9000, "builder", "password", 1)
            .expect("an endpoint");
        assert!(
            app.attach_replay_network(net).is_err(),
            "{name}: no connection after"
        );
        assert_eq!(app.host_state().entered_character, None, "{name}");
    }

    // Not refused, the same kind of folder is connected to.
    let folder = scratch.path().join("taken");
    write_picture(&folder, store.portal(), "Taken", b"taken");
    let settings = scratch.path().join("taken-settings");
    std::fs::create_dir_all(&settings).unwrap();
    let app = bring_up(
        Config {
            overlay_dat_dir: Some(folder),
            ..connecting(&settings)
        },
        &store,
    );
    assert_eq!(app.world_refused(), None);
    assert!(app.link.is_some(), "connected");
    assert_eq!(
        app.store.read_portal(PICTURE).expect("the picture"),
        b"taken"
    );

    // Made for other game files (a portal container over the language file), the folder is
    // connected to, its overlay thrown away unread, and the world patches these files afresh.
    let folder = scratch.path().join("other-files");
    write_picture(&folder, store.local(), "Mine", b"mine");
    let settings = scratch.path().join("other-files-settings");
    std::fs::create_dir_all(&settings).unwrap();
    let app = bring_up(
        Config {
            overlay_dat_dir: Some(folder.clone()),
            ..connecting(&settings)
        },
        &store,
    );
    assert_eq!(app.world_refused(), None);
    assert!(app.link.is_some(), "connected");
    assert!(!app.store.has_overlay(), "nothing of it is read");
    assert!(
        OverlayDir::new(&folder).unwrap().containers().is_empty(),
        "its containers are gone"
    );
}

/// A world's manifest naming a record of the portal file whose bytes are `sha256`, over the
/// player's portal file as `base_fingerprint` names it.
fn manifest(world: &str, base_fingerprint: [u8; 32], sha256: [u8; 32]) -> DddOverlayManifest {
    DddOverlayManifest {
        world_key: world.into(),
        total_bytes: 0,
        files: vec![pyreal_protocol::admin::OverlayFileManifest {
            dat_file_type: 0,
            dat_file_id: 1,
            base_name: "client_portal.dat".into(),
            base_fingerprint,
            base_records: None,
            base_iterations: 0,
            exact_iterations: false,
            revisions: Vec::new(),
            records: vec![pyreal_protocol::admin::OverlayRecord {
                id: PICTURE.raw(),
                iteration: 1,
                size: 5,
                sha256,
            }],
            tombstones: Vec::new(),
        }],
    }
}

/// The world's picture, sent during the patch.
fn picture(bytes: &[u8], iteration: u32) -> DddData {
    DddData {
        dat_file_type: 0,
        dat_file_id: 1,
        resource_type: 6,
        resource_id: PICTURE.raw(),
        iteration,
        compressed: 0,
        version: 1,
        data_size: u32::try_from(bytes.len() + 4).unwrap(),
        data: bytes.to_vec(),
    }
}

/// A world whose data the client refuses during the data patch ends the session there, for each
/// reason only the server can tell: the world is on the blocklist, its overlay was made for other
/// game files, a record is not the one its manifest names. The rest of the patch is not taken,
/// the patch never finishes, character select is never reached, the scripted entry stops, and no
/// character goes in; nothing of the world is left in its folder.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_world_refused_during_the_patch_ends_the_session_before_character_select() {
    let store = retail_store();
    let base = pyreal_dat::overlay::fingerprint(store.portal());
    let good = pyreal_dat::overlay::record_hash(b"good!");
    let scratch = pyreal_dat::testing::ScratchDir::new("refused-at-patch").expect("scratch");
    let cases: [(&str, Vec<DddEvent>, &str); 3] = [
        (
            "blocked",
            vec![DddEvent::OverlayManifest(Box::new(manifest(
                "Bad", base, good,
            )))],
            "\"Bad\" is on your overlay blocklist",
        ),
        (
            "other-files",
            vec![DddEvent::OverlayManifest(Box::new(manifest(
                "Good", [0xAB; 32], good,
            )))],
            "made for other game files than yours",
        ),
        (
            "bad-record",
            vec![
                // Ahead of the manifest, and newer than the installed file's own record.
                DddEvent::Data(Box::new(picture(b"forge", 100_000))),
                DddEvent::OverlayManifest(Box::new(manifest("Good", base, good))),
            ],
            "the server sent data that does not match it",
        ),
    ];
    for (name, events, says) in cases {
        let settings = scratch.path().join(format!("{name}-settings"));
        std::fs::create_dir_all(&settings).unwrap();
        std::fs::write(settings.join(crate::world_overlay::BLOCKLIST_FILE), "Bad\n").unwrap();
        let folder = scratch.path().join(name);
        let cfg = Config {
            connect: false,
            overlay_dat_dir: Some(folder.clone()),
            ..connecting(&settings)
        };
        let mut app = bring_up(cfg, &store);
        let net = crate::net::ClientNetwork::new("127.0.0.1", 9000, "builder", "password", 1)
            .expect("an endpoint");
        assert!(app.attach_replay_network(net).is_ok());
        let mut shell = NullShell;
        app.process_logon_event_queue(&mut shell, vec![character_set()]);
        app.process_logon_event_queue(
            &mut shell,
            vec![SessionEvent::Ddd(DddEvent::Interrogation(
                DddInterrogation::default(),
            ))],
        );
        assert!(
            !app.host_state().patch_finished,
            "{name}: the patch is under way"
        );
        // The rest of the patch comes in the same frame, and is not taken.
        let mut batch: Vec<SessionEvent> = events.into_iter().map(SessionEvent::Ddd).collect();
        batch.extend([
            SessionEvent::Ddd(DddEvent::Begin(DddBeginDdd::default())),
            SessionEvent::Ddd(DddEvent::Data(Box::new(picture(b"good!", 1)))),
            SessionEvent::Ddd(DddEvent::End),
        ]);
        app.process_logon_event_queue(&mut shell, batch);
        let text = app.world_refused().map(str::to_owned);
        assert!(
            text.as_deref().is_some_and(|t| t.contains(says)),
            "{name}: {text:?}"
        );
        assert!(!app.host_state().patch_finished, "{name}");
        assert_eq!(
            phase(&mut app),
            GamePhase::Disconnected(DisconnectNotice::OverlayRefused(text.unwrap())),
            "{name}"
        );
        assert_eq!(app.script, EnterWorldScript::Done, "{name}");
        app.process_logon_event_queue(&mut shell, vec![character_set()]);
        assert_eq!(app.host_state().entered_character, None, "{name}");
        assert!(
            !matches!(
                phase(&mut app),
                GamePhase::CharacterSelect | GamePhase::EnteringWorld
            ),
            "{name}"
        );
        assert!(
            OverlayDir::new(&folder).unwrap().containers().is_empty(),
            "{name}: nothing of the world is written"
        );
    }
}

thread_local! {
    /// The per-user cache the per-world tests keep their servers' folders in.
    static CACHE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// The disk, with the per-user cache in [`CACHE`].
fn keep_cache_in(dir: &Path) {
    CACHE.with(|c| *c.borrow_mut() = Some(dir.to_path_buf()));
    crate::platform::files::install(crate::platform::files::FileHost {
        cache_dir: || CACHE.with(|c| c.borrow().clone()),
        ..crate::platform::files::DISK
    });
}

/// A run keeping a folder per server starts on the folder of the world the server served last,
/// and its answer names that world. When the server names itself as another world, that world's
/// folder takes its place: the files read through it, the next answer is that world's, and the
/// next run starts on it; a world new to the server has an empty folder, and its answer names no
/// world. A server that patches it anyway, as the world the answer named, is refused: what it sent
/// is not the world's.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_world_naming_itself_as_another_is_read_and_answered_for_from_its_own_folder() {
    use pyreal_protocol::admin::DddInterrogationResponse;
    let store = retail_store();
    let scratch = pyreal_dat::testing::ScratchDir::new("each-world").expect("scratch");
    keep_cache_in(&scratch.path().join("cache"));
    let settings = scratch.path().join("settings");
    std::fs::create_dir_all(&settings).unwrap();
    let root = scratch.path().join("cache/overlays/127.0.0.1-9000");
    write_picture(
        &crate::world_overlay::world_folder(&root, "Alpha"),
        store.portal(),
        "Alpha",
        b"alpha",
    );
    write_picture(
        &crate::world_overlay::world_folder(&root, "Beta"),
        store.portal(),
        "Beta",
        b"beta!",
    );
    crate::world_overlay::remember_world(&root, "Alpha").unwrap();

    let start = |switch_to: &str| {
        let mut app = bring_up(connecting(&settings), &store);
        // The test speaks for the server: the run's connection is one it feeds.
        app.link = Some(crate::net::NetLink::replay(
            crate::net::ClientNetwork::new("127.0.0.1", 9000, "builder", "password", 1)
                .expect("an endpoint"),
        ));
        assert_eq!(
            crate::world_overlay::laid_world(&app.store).as_deref(),
            Some("Alpha"),
            "the world the server served last"
        );
        let mut shell = NullShell;
        app.process_logon_event_queue(
            &mut shell,
            vec![
                SessionEvent::Ddd(DddEvent::Interrogation(DddInterrogation::default())),
                SessionEvent::Ddd(DddEvent::OverlayManifest(Box::new(DddOverlayManifest {
                    world_key: switch_to.into(),
                    ..DddOverlayManifest::default()
                }))),
            ],
        );
        app
    };
    let answer = |app: &App<NullShell>| {
        crate::app::ddd_interrogation_response(
            &app.store,
            0,
            true,
            crate::world_overlay::laid_world(&app.store).as_deref(),
        )
    };

    // Beta names itself: its folder is read, the next answer is Beta's, and Beta is noted.
    let mut app = start("Beta");
    assert_eq!(app.world_refused(), None, "{:?}", app.ddd().notices());
    assert_eq!(app.store.read_portal(PICTURE).unwrap(), b"beta!");
    assert_eq!(
        answer(&app).world(),
        Some(pyreal_dat::overlay::world_hash("Beta"))
    );
    assert_eq!(
        app.ddd().dir(),
        Some(crate::world_overlay::world_folder(&root, "Beta").as_path())
    );
    assert_eq!(
        crate::world_overlay::last_world(&root).as_deref(),
        Some("Beta")
    );
    // The server asks again, and the patch goes on as Beta's.
    let mut shell = NullShell;
    app.process_logon_event_queue(
        &mut shell,
        vec![
            SessionEvent::Ddd(DddEvent::Interrogation(DddInterrogation::default())),
            SessionEvent::Ddd(DddEvent::Begin(DddBeginDdd::default())),
            SessionEvent::Ddd(DddEvent::End),
        ],
    );
    assert_eq!(app.world_refused(), None, "{:?}", app.ddd().notices());
    assert!(app.host_state().patch_finished);
    assert_eq!(app.store.read_portal(PICTURE).unwrap(), b"beta!");

    // A world new to the server: an empty folder of its own, and an answer naming no world.
    crate::world_overlay::remember_world(&root, "Alpha").unwrap();
    let app = start("Gamma");
    assert_eq!(app.world_refused(), None);
    assert!(!app.store.has_overlay());
    let r = answer(&app);
    assert_eq!(r.world(), None);
    assert_eq!(r.flags & DddInterrogationResponse::FLAG_WORLD, 0);
    assert_eq!(
        app.ddd().dir(),
        Some(crate::world_overlay::world_folder(&root, "Gamma").as_path())
    );

    // A server that patches Beta as the world the answer named is refused.
    crate::world_overlay::remember_world(&root, "Alpha").unwrap();
    let mut app = start("Beta");
    app.process_logon_event_queue(
        &mut shell,
        vec![SessionEvent::Ddd(DddEvent::Begin(DddBeginDdd::default()))],
    );
    let text = app.world_refused().map(str::to_owned);
    assert!(
        text.as_deref().is_some_and(|t| t.contains("Connect again")),
        "{text:?}"
    );
    assert_eq!(app.script, EnterWorldScript::Done);
    assert_eq!(
        crate::world_overlay::last_world(&root).as_deref(),
        Some("Beta"),
        "the next run starts on Beta's folder"
    );
    assert_eq!(
        OverlayDir::new(&crate::world_overlay::world_folder(&root, "Alpha"))
            .unwrap()
            .world_key()
            .as_deref(),
        Some("Alpha"),
        "Alpha's folder keeps Alpha's data"
    );
}

/// A part only a world serves: a retail mesh under a graphics-object id retail lacks.
const SERVED_PART: DataId = DataId(0x0100_FFF0);

/// The files the front door opens carry no world's records. Signing in to a world whose records
/// were kept at its last session reads them before its characters (whatever dat set the world's
/// switches name), and the preview spaces read them: a character dressed in served parts is not
/// drawn from the retail files alone.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_worlds_served_records_reach_the_previews_drawn_before_sign_in() {
    let store = retail_store();
    let scratch = pyreal_dat::testing::ScratchDir::new("served-backdrop").expect("scratch");
    keep_cache_in(&scratch.path().join("cache"));
    let settings = scratch.path().join("settings");
    std::fs::create_dir_all(&settings).unwrap();
    let root = scratch.path().join("cache/overlays/127.0.0.1-9000");
    assert!(store.read_portal(SERVED_PART).is_err(), "retail lacks it");
    let mut part = store
        .read_portal(DataId(0x0100_0001))
        .expect("a retail mesh");
    part[..4].copy_from_slice(&SERVED_PART.0.to_le_bytes());
    {
        let base = store.portal();
        let dir = OverlayDir::new(&crate::world_overlay::world_folder(&root, "Served"))
            .expect("an overlay folder");
        let mut w = dir
            .writer(ModernDat::Portal, base, "client_portal.dat", "Served", 1)
            .expect("an overlay writer");
        let next = pyreal_dat::overlay::base_iterations(base)
            .last()
            .copied()
            .unwrap_or(0)
            + 1;
        w.put(SERVED_PART, &part, 1, next, 1).expect("a record");
        w.add_iteration(next, 1).expect("its revision");
        w.flush(1).expect("written");
    }
    crate::world_overlay::remember_world(&root, "Served").unwrap();

    // The front door: no session, the world switch naming the modern set, the files read at once.
    let mut app = bring_up(
        Config {
            connect: false,
            world_base: Some(pyreal_dat::ContainerEra::Modern),
            ..connecting(&settings)
        },
        &store,
    );
    app.use_data_files(&pyreal_dat::testing::dat_dir(), None)
        .expect("the retail files");
    assert!(!app.store.has_overlay());

    let start = SessionStart {
        host: "127.0.0.1".into(),
        port: 9000,
        account: "builder".into(),
        password: "password".into(),
        dat_dir: pyreal_dat::testing::dat_dir(),
        classic_dat_dir: None,
        world: vec![("world-base".into(), "modern".into())],
        plugin_policy: None,
    };
    let net = crate::net::ClientNetwork::new("127.0.0.1", 9000, "builder", "password", 1)
        .expect("an endpoint");
    app.begin_session_on(&mut NullShell, &start, net)
        .expect("the session");
    assert_eq!(
        crate::world_overlay::laid_world(&app.store).as_deref(),
        Some("Served"),
        "the world's records are read before its characters"
    );
    assert_eq!(app.store.read_portal(SERVED_PART).unwrap(), part);
    assert_eq!(
        app.anim_assets.store().read_portal(SERVED_PART).unwrap(),
        part,
        "the preview spaces' assets read them"
    );
}

/// Spells only a world serves, and a picture for their icon.
const SERVED_SPELL: u32 = 0x7FF0;
const PATCHED_SPELL: u32 = 0x7FF1;
const SERVED_ICON: DataId = DataId(0x0600_FFF0);

/// A spell table's string as its records keep it: nibble-swapped, padded to four bytes.
fn put_spell_string(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&u16::try_from(text.len()).unwrap().to_le_bytes());
    out.extend(text.bytes().map(|b| b.rotate_left(4)));
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

/// `template` as a spell named `name`, its icon `icon` and its components keyed to its new name.
fn encode_spell(
    template: &pyreal_assets::tables::SpellBase,
    name: &str,
    description: &str,
    icon: u32,
) -> Vec<u8> {
    let mut out = Vec::new();
    put_spell_string(&mut out, name);
    put_spell_string(&mut out, description);
    let t = template;
    for v in [t.school, icon, t.category, t.bitfield] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&t.base_mana.to_le_bytes());
    out.extend_from_slice(&t.base_range_constant.to_le_bytes());
    out.extend_from_slice(&t.base_range_mod.to_le_bytes());
    out.extend_from_slice(&t.power.to_le_bytes());
    out.extend_from_slice(&t.spell_economy_mod.to_le_bytes());
    out.extend_from_slice(&t.formula_version.to_le_bytes());
    out.extend_from_slice(&t.component_loss.to_le_bytes());
    out.extend_from_slice(&t.meta_spell_type.to_le_bytes());
    out.extend_from_slice(&t.meta_spell_id.to_le_bytes());
    if let Some((d, a, b)) = t.duration {
        out.extend_from_slice(&d.to_le_bytes());
        out.extend_from_slice(&a.to_le_bytes());
        out.extend_from_slice(&b.to_le_bytes());
    }
    if let Some(l) = t.portal_lifetime {
        out.extend_from_slice(&l.to_le_bytes());
    }
    let key = pyreal_assets::tables::spell_component_key(name.as_bytes(), description.as_bytes());
    for raw in t.raw_comps {
        let slot = if raw == 0 {
            0
        } else {
            raw.wrapping_sub(t.comp_key).wrapping_add(key)
        };
        out.extend_from_slice(&slot.to_le_bytes());
    }
    for v in [t.caster_effect, t.target_effect, t.fizzle_effect] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&t.recovery_interval.to_le_bytes());
    out.extend_from_slice(&t.recovery_amount.to_le_bytes());
    out.extend_from_slice(&t.display_order.to_le_bytes());
    out.extend_from_slice(&t.non_component_target_type.to_le_bytes());
    out.extend_from_slice(&t.mana_mod.to_le_bytes());
    out
}

/// The retail spell table with `spells` added ahead of its own, as a world serves it.
fn served_spell_table(
    retail: &[u8],
    template: &pyreal_assets::tables::SpellBase,
    spells: &[(u32, &str, &str)],
) -> Vec<u8> {
    let mut out = retail[..8].to_vec();
    let count = u16::from_le_bytes([out[4], out[5]]) + u16::try_from(spells.len()).unwrap();
    out[4..6].copy_from_slice(&count.to_le_bytes());
    for (id, name, description) in spells {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend(encode_spell(template, name, description, SERVED_ICON.0));
    }
    out.extend_from_slice(&retail[8..]);
    out
}

/// Put `records` in the portal container of the world `world` in `root`, as revision `iteration`
/// past the retail file's.
fn serve(root: &Path, base: &DatFile, world: &str, iteration: u32, records: &[(DataId, &[u8])]) {
    let dir = OverlayDir::new(&crate::world_overlay::world_folder(root, world))
        .expect("an overlay folder");
    let mut w = dir
        .writer(ModernDat::Portal, base, "client_portal.dat", world, 1)
        .expect("an overlay writer");
    let next = pyreal_dat::overlay::base_iterations(base)
        .last()
        .copied()
        .unwrap_or(0)
        + iteration;
    for (id, bytes) in records {
        w.put(*id, bytes, 1, next, 1).expect("a record");
    }
    w.add_iteration(next, 1).expect("its revision");
    w.flush(1).expect("written");
}

/// A world's served spell table (spells retail lacks, with their own names, descriptions, icons
/// and components) is what the interface and the model read once the world's records are: kept
/// from its last session, they replace the retail table read at the front door before the
/// characters are listed; patched in the session, they replace it again.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_worlds_served_spells_replace_the_retail_table_read_before_sign_in() {
    use pyreal_assets::Decode as _;
    let store = retail_store();
    let scratch = pyreal_dat::testing::ScratchDir::new("served-spells").expect("scratch");
    keep_cache_in(&scratch.path().join("cache"));
    let settings = scratch.path().join("settings");
    std::fs::create_dir_all(&settings).unwrap();
    let root = scratch.path().join("cache/overlays/127.0.0.1-9000");

    let id = crate::hud::SPELL_TABLE;
    let retail = store.read_portal(id).expect("the retail spell table");
    let table = pyreal_assets::tables::SpellTable::decode_payload_in(store.era_of(id), id, &retail)
        .expect("decodes");
    assert!(!table.spells.contains_key(&SERVED_SPELL), "retail lacks it");
    let (_, template) = table
        .spells
        .iter()
        .find(|(_, s)| s.comps.len() >= 4 && s.meta_spell_type == 1)
        .expect("an enchantment with components");
    let mut icon = store.read_portal(DataId(template.icon)).expect("its icon");
    icon[..4].copy_from_slice(&SERVED_ICON.0.to_le_bytes());
    let served = served_spell_table(
        &retail,
        template,
        &[(
            SERVED_SPELL,
            "Ethereal Frost",
            "A spell only this world knows.",
        )],
    );
    serve(
        &root,
        store.portal(),
        "Served",
        1,
        &[(id, &served), (SERVED_ICON, &icon)],
    );
    crate::world_overlay::remember_world(&root, "Served").unwrap();

    // The front door reads the retail table.
    let mut app = bring_up(
        Config {
            connect: false,
            world_base: Some(pyreal_dat::ContainerEra::Modern),
            ..connecting(&settings)
        },
        &store,
    );
    app.use_data_files(&pyreal_dat::testing::dat_dir(), None)
        .expect("the retail files");
    assert!(app.hud.spell_entry(SERVED_SPELL).is_none());

    let start = SessionStart {
        host: "127.0.0.1".into(),
        port: 9000,
        account: "builder".into(),
        password: "password".into(),
        dat_dir: pyreal_dat::testing::dat_dir(),
        classic_dat_dir: None,
        world: vec![("world-base".into(), "modern".into())],
        plugin_policy: None,
    };
    let net = crate::net::ClientNetwork::new("127.0.0.1", 9000, "builder", "password", 1)
        .expect("an endpoint");
    app.begin_session_on(&mut NullShell, &start, net)
        .expect("the session");
    let entry = app
        .hud
        .spell_entry(SERVED_SPELL)
        .expect("the served spell, before the characters");
    assert_eq!(entry.name, "Ethereal Frost");
    assert_eq!(entry.icon, Some(SERVED_ICON));
    assert_eq!(app.store.read_portal(SERVED_ICON).unwrap(), icon);
    let base = &app.hud.spell_table.as_ref().unwrap().spells[&SERVED_SPELL];
    assert_eq!(base.description, "A spell only this world knows.");
    assert_eq!(base.comps, template.comps, "its components");
    assert!(
        app.objects
            .world
            .magic
            .spell_table
            .as_ref()
            .unwrap()
            .spells
            .contains_key(&SERVED_SPELL),
        "the model's table, which casting and enchantments read"
    );

    // A patch in the session adds another: read again, the model's copies with it.
    app.objects.world.magic.spell_beneficial = [(SERVED_SPELL, true)].into();
    let patched = served_spell_table(
        &retail,
        template,
        &[
            (
                SERVED_SPELL,
                "Ethereal Frost",
                "A spell only this world knows.",
            ),
            (PATCHED_SPELL, "Ethereal Thaw", "Patched in."),
        ],
    );
    serve(&root, store.portal(), "Served", 2, &[(id, &patched)]);
    app.ddd_invalidation = Some(crate::ddd::DddSummary {
        applied: 1,
        changed: vec![crate::ddd::DatTarget::Portal],
        ..Default::default()
    });
    app.invalidate_after_ddd();
    assert_eq!(
        app.hud.spell_entry(PATCHED_SPELL).map(|e| e.name),
        Some("Ethereal Thaw".to_owned())
    );
    assert!(app
        .objects
        .world
        .magic
        .spell_table
        .as_ref()
        .unwrap()
        .spells
        .contains_key(&PATCHED_SPELL));
    assert!(app
        .objects
        .world
        .magic
        .spell_beneficial
        .contains_key(&PATCHED_SPELL));
}

/// The portal and cell iterations `app`'s answer to the server's question carries.
fn answered_iterations(app: &App<NullShell>) -> (i32, i32) {
    let world = crate::world_overlay::laid_world(&app.store);
    let r = crate::app::ddd_interrogation_response(
        &app.store,
        0,
        app.ddd.keeps_overlay(),
        world.as_deref(),
    );
    let of = |id: u32| {
        r.iters_with_keys
            .iter()
            .find(|l| l.dat_file_id == id)
            .map_or(-1, |l| l.iterations.iterations)
    };
    (of(1), of(2))
}

/// A world on 1999 files signed in to after one on the later files, in one run, answers with the
/// 1999 files' iterations, with the later files kept beside them for the ground.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail, February 2005 and 1999 dats: --features retail-dats"
)]
fn a_1999_world_after_a_later_one_answers_with_the_1999_files_iterations() {
    let store = retail_store();
    let classic = pyreal_dat::testing::classic_dat_dir().expect("the February 2005 dats");
    let launch = pyreal_dat::testing::launch_dat_dir().expect("the 1999 dats");
    let scratch = pyreal_dat::testing::ScratchDir::new("launch-after-later").expect("scratch");
    keep_cache_in(&scratch.path().join("cache"));
    let settings = scratch.path().join("settings");
    std::fs::create_dir_all(&settings).unwrap();
    // The 2005 world plays the later files with an overlay of its own over them.
    let root = scratch.path().join("cache/overlays/127.0.0.1-9010");
    write_picture(
        &crate::world_overlay::world_folder(&root, "Local 2005"),
        store.portal(),
        "Local 2005",
        b"theirs",
    );
    crate::world_overlay::remember_world(&root, "Local 2005").unwrap();

    let mut app = bring_up(
        Config {
            connect: false,
            ..connecting(&settings)
        },
        &store,
    );
    app.use_data_files(&pyreal_dat::testing::dat_dir(), Some(&classic))
        .expect("the retail files");
    let start = |port: u16, era: &str, pair: &Path| SessionStart {
        host: "127.0.0.1".into(),
        port,
        account: "builder".into(),
        password: "password".into(),
        dat_dir: pyreal_dat::testing::dat_dir(),
        classic_dat_dir: Some(pair.to_path_buf()),
        world: vec![("era".into(), era.into())],
        plugin_policy: None,
    };
    let net = |port: u16| {
        crate::net::ClientNetwork::new("127.0.0.1", port, "builder", "password", 1)
            .expect("an endpoint")
    };
    app.begin_session_on(
        &mut NullShell,
        &start(9010, "infiltration", &classic),
        net(9010),
    )
    .expect("the 2005 session");
    assert_eq!(
        answered_iterations(&app).1,
        982,
        "the 2005 world's own, the later files"
    );
    app.end_session(&mut NullShell);

    app.begin_session_on(&mut NullShell, &start(9020, "launch", &launch), net(9020))
        .expect("the 1999 session");
    assert_eq!(answered_iterations(&app), (102, 67), "the 1999 files");
    assert!(
        app.store.modern_files().is_some(),
        "the later files stay beside them for the ground"
    );
}

/// A 2005 world kept at port 9010 when it played the later files, under its old name, and a run
/// on the later files with the February 2005 pair beside them that connects to it.
fn moved_2005_world(name: &str) -> (pyreal_dat::testing::ScratchDir, PathBuf, Config) {
    let store = retail_store();
    let classic = pyreal_dat::testing::classic_dat_dir().expect("the February 2005 dats");
    let scratch = pyreal_dat::testing::ScratchDir::new(name).expect("scratch");
    keep_cache_in(&scratch.path().join("cache"));
    let settings = scratch.path().join("settings");
    std::fs::create_dir_all(&settings).unwrap();
    let root = scratch.path().join("cache/overlays/127.0.0.1-9010");
    write_picture(
        &crate::world_overlay::world_folder(&root, "Local"),
        store.portal(),
        "Local",
        b"local",
    );
    crate::world_overlay::remember_world(&root, "Local").unwrap();
    let cfg = Config {
        port: 9010,
        dat_dir: pyreal_dat::testing::dat_dir(),
        classic_dat_dir: Some(classic),
        era: Some(pyreal_primitives::EraId::Infiltration),
        ..connecting(&settings)
    };
    (scratch, root, cfg)
}

/// The world's manifest, its overlay made over `base` as the file `base_name`.
fn manifest_over(world: &str, base_name: &str, base: &DatFile) -> DddOverlayManifest {
    DddOverlayManifest {
        world_key: world.into(),
        total_bytes: 0,
        files: vec![pyreal_protocol::admin::OverlayFileManifest {
            dat_file_type: 0,
            dat_file_id: 1,
            base_name: base_name.into(),
            base_fingerprint: pyreal_dat::overlay::fingerprint(base),
            base_records: None,
            base_iterations: 0,
            exact_iterations: false,
            revisions: Vec::new(),
            records: Vec::new(),
            tombstones: Vec::new(),
        }],
    }
}

/// A world moved from the later files to the February 2005 ones, under a new name, is answered
/// for from the 2005 files when it names itself and asks again: the folder kept from its old name
/// opened the later files, and the world's overlay lies over the 2005 ones. The patch then goes on.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail and February 2005 dats: --features retail-dats"
)]
fn a_world_moved_to_the_2005_files_is_answered_for_from_them_when_it_asks_again() {
    let (_scratch, root, cfg) = moved_2005_world("moved-2005");
    let classic = RetailDatStore::open_classic_dir(cfg.classic_dat_dir.as_deref().unwrap())
        .expect("the February 2005 dats");
    let mut app = bring_up(cfg, &retail_store());
    app.link = Some(crate::net::NetLink::replay(
        crate::net::ClientNetwork::new("127.0.0.1", 9010, "builder", "password", 1)
            .expect("an endpoint"),
    ));
    assert_eq!(
        app.store.era(),
        pyreal_dat::ContainerEra::Modern,
        "the old name's folder"
    );
    let renamed = manifest_over("Local 2005", "portal.dat", classic.portal());
    let mut shell = NullShell;
    app.process_logon_event_queue(
        &mut shell,
        vec![
            SessionEvent::Ddd(DddEvent::Interrogation(DddInterrogation::default())),
            SessionEvent::Ddd(DddEvent::OverlayManifest(Box::new(renamed.clone()))),
        ],
    );
    assert_eq!(app.world_refused(), None, "{:?}", app.ddd().notices());
    assert_eq!(app.store.era(), pyreal_dat::ContainerEra::Classic);
    assert_eq!(answered_iterations(&app), (2112, 1593), "the 2005 files");
    assert!(
        app.store.modern_files().is_some(),
        "the later files beside them"
    );
    assert_eq!(
        app.ddd().dir(),
        Some(crate::world_overlay::world_folder(&root, "Local 2005").as_path())
    );
    // The server asks again, and its manifest ahead of the patch names the files now open.
    app.process_logon_event_queue(
        &mut shell,
        vec![
            SessionEvent::Ddd(DddEvent::Interrogation(DddInterrogation::default())),
            SessionEvent::Ddd(DddEvent::OverlayManifest(Box::new(renamed))),
            SessionEvent::Ddd(DddEvent::Begin(DddBeginDdd::default())),
            SessionEvent::Ddd(DddEvent::End),
        ],
    );
    assert_eq!(app.world_refused(), None, "{:?}", app.ddd().notices());
    assert!(app.host_state().patch_finished);
}

/// A sign-in in the same run opens the files a fresh run would: once a world's folder changed
/// under the files open, the next session's files follow the folder it starts on.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail and February 2005 dats: --features retail-dats"
)]
fn a_sign_in_after_the_worlds_folder_changed_opens_the_files_a_fresh_run_would() {
    let (_scratch, root, cfg) = moved_2005_world("moved-folder");
    let store = retail_store();
    let start = SessionStart {
        host: "127.0.0.1".into(),
        port: 9010,
        account: "builder".into(),
        password: "password".into(),
        dat_dir: cfg.dat_dir.clone(),
        classic_dat_dir: cfg.classic_dat_dir.clone(),
        world: vec![("era".into(), "infiltration".into())],
        plugin_policy: None,
    };
    let net = || {
        crate::net::ClientNetwork::new("127.0.0.1", 9010, "builder", "password", 1)
            .expect("an endpoint")
    };
    let mut app = bring_up(
        Config {
            connect: false,
            ..cfg
        },
        &store,
    );
    app.begin_session_on(&mut NullShell, &start, net())
        .expect("the session");
    assert_eq!(
        app.store.era(),
        pyreal_dat::ContainerEra::Modern,
        "the old name's folder"
    );
    // The world names itself anew, over the files open, and its new folder is empty.
    let mut shell = NullShell;
    app.process_logon_event_queue(
        &mut shell,
        vec![
            SessionEvent::Ddd(DddEvent::Interrogation(DddInterrogation::default())),
            SessionEvent::Ddd(DddEvent::OverlayManifest(Box::new(manifest_over(
                "Local 2005",
                "client_portal.dat",
                store.portal(),
            )))),
        ],
    );
    assert_eq!(
        app.ddd().dir(),
        Some(crate::world_overlay::world_folder(&root, "Local 2005").as_path())
    );
    app.end_session(&mut shell);
    app.begin_session_on(&mut shell, &start, net())
        .expect("the next session");
    assert_eq!(
        app.store.era(),
        pyreal_dat::ContainerEra::Classic,
        "the era's files"
    );
    assert_eq!(answered_iterations(&app), (2112, 1593));
}
