//! Records a world serves, end to end: a socket-free session is served what Empyrean sends a
//! client that keeps overlays -- the world's overlay manifest (`0xF7EC`), the patch (`0xF7E7`),
//! its records (`0xF7E2`) and the end (`0xF7EA`) -- replacing a retail picture, adding a new one
//! and lowering the level cap. The store then reads the served records over the retail ones, the
//! HUD's tables are read again from them, the engine's picture path decodes the served picture,
//! and an art pack's material for the replaced picture still lays over it.
//!
//! Empyrean's `ddd_handler.rs` (`ddd_interrogation_response_with`) is the order: the manifest
//! ahead of the patch, then the begin message naming every record to download, then the records.

use std::sync::Arc;

use pyreal_client_runtime::ddd::DatTarget;
use pyreal_client_runtime::present::NullPresentation;
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_engine::asset::material::{describe_surface, KindTable, MaterialOverrides, TextureKey};
use pyreal_engine::asset::texture::TextureSource;
use pyreal_engine::pack::PackOverrides;
use pyreal_primitives::{AssetSource as _, DataId};
use pyreal_protocol::admin::{
    DddBeginDdd, DddData, DddOverlayManifest, OverlayFileManifest, OverlayRecord, PatchRevision,
};

/// The world the session is served by.
const WORLD: &str = "Served Records";

/// A picture id no retail record holds.
const NEW_PICTURE: DataId = DataId(0x05FF_F001);

/// The experience table, whose levels are the level cap.
const XP_TABLE: DataId = DataId(0x0E00_0018);

/// `bytes` of a record as the record `id`: its leading id rewritten where it names its own.
fn as_record(id: DataId, from: DataId, bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    if out.get(..4) == Some(&from.raw().to_le_bytes()[..]) {
        out[..4].copy_from_slice(&id.raw().to_le_bytes());
    }
    out
}

/// The retail experience table cut to `levels` levels: a world with a lower level cap.
fn xp_table_with(store: &RetailDatStore, levels: usize) -> Vec<u8> {
    use pyreal_assets::Decode;
    let bytes = store.read(XP_TABLE).expect("the experience table");
    let t =
        pyreal_assets::tables::XpTable::decode_payload_in(store.era_of(XP_TABLE), XP_TABLE, &bytes)
            .expect("it decodes");
    let mut out = XP_TABLE.raw().to_le_bytes().to_vec();
    let n = |v: usize| u32::try_from(v - 1).expect("a count").to_le_bytes();
    for len in [
        t.attribute_xp.len(),
        t.vital_xp.len(),
        t.trained_xp.len(),
        t.specialized_xp.len(),
        levels + 1,
    ] {
        out.extend(n(len));
    }
    for list in [
        &t.attribute_xp,
        &t.vital_xp,
        &t.trained_xp,
        &t.specialized_xp,
    ] {
        out.extend(list.iter().flat_map(|v| v.to_le_bytes()));
    }
    out.extend(t.level_xp[..=levels].iter().flat_map(|v| v.to_le_bytes()));
    out.extend(
        t.level_credits[..=levels]
            .iter()
            .flat_map(|v| v.to_le_bytes()),
    );
    out
}

/// Two pictures (`0x05`) a surface draws, whose pixels differ, and a surface drawing the first.
fn two_pictures(store: &RetailDatStore) -> (DataId, DataId, pyreal_assets::Surface) {
    use pyreal_assets::Decode;
    let src = TextureSource::new(store);
    let mut found: Vec<(DataId, pyreal_assets::Surface)> = Vec::new();
    for id in store.ids_of(DbType::Surface) {
        let Ok(bytes) = store.read_typed(DbType::Surface, id) else {
            continue;
        };
        let Ok(s) = pyreal_assets::Surface::decode_payload_in(store.era_of(id), id, &bytes) else {
            continue;
        };
        let Some(t) = s.orig_texture_id.filter(|t| t.raw() >> 24 == 0x05) else {
            continue;
        };
        if src.decode(t, false, None).is_err() || found.iter().any(|(f, _)| *f == t) {
            continue;
        }
        if let Some((first, surface)) = found.first() {
            let a = src.decode(*first, false, None).expect("decodes");
            let b = src.decode(t, false, None).expect("decodes");
            if a.levels[0] != b.levels[0] {
                return (*first, t, *surface);
            }
        }
        found.push((t, s));
    }
    panic!("no two pictures that differ");
}

#[test]
fn a_served_record_replaces_a_retail_one_a_new_one_is_added_and_the_pack_still_lays_over_them() {
    use pyreal_client::app::App;
    use pyreal_client_runtime::config::Config;
    use pyreal_client_runtime::net::ClientNetwork;

    let dir = pyreal_dat::testing::dat_dir();
    assert!(pyreal_dat::testing::have_dats(), "set PYREAL_TEST_DAT_DIR");
    let retail = pyreal_dat::testing::open_store_or_fail();
    let (replaced, served_as, surface) = two_pictures(&retail);
    let picture = |id: DataId| retail.read(id).expect("a retail picture");
    let replacement = as_record(replaced, served_as, &picture(served_as));
    let added = as_record(NEW_PICTURE, served_as, &picture(served_as));
    let xp = xp_table_with(&retail, 200);
    assert!(!retail.portal().contains(NEW_PICTURE), "the new id is new");

    let scratch = pyreal_dat::testing::ScratchDir::new("served-records").expect("a scratch folder");
    let overlay = scratch.path().join("overlay");
    std::fs::create_dir_all(&overlay).expect("the overlay folder");
    let mut app = App::with_presentation(
        Config {
            headless: true,
            sound: false,
            ui: true,
            preferences_file: scratch.path().join("prefs.ini"),
            dat_dir: dir.clone(),
            overlay_dat_dir: Some(overlay.clone()),
            ..Default::default()
        },
        Box::new(NullPresentation::new(800, 600)),
    )
    .expect("the application comes up with a null presentation");
    app.start_shell().expect("the UI shell comes up");
    let level_cap = app.hud().era.level_cap;
    assert!(level_cap > 200, "retail's cap is {level_cap}");

    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "served", "unused", 0)
        .expect("a socket-free network client");
    net.session.transport.add_connection(
        0xB,
        0,
        1,
        0xDEAD_BEEF,
        0x1234_5678,
        Some("127.0.0.1:19000".parse().expect("a literal address")),
    );
    let mut crypto = pyreal_transport::CryptoSystem::new(0xDEAD_BEEF);
    app.attach_replay_network(net)
        .map_err(|_| ())
        .expect("the endpoint attaches");
    // Each blob on queue 5, in 448-byte fragments a packet each, as a server sends a large one.
    let (mut seq, mut blob) = (1u32, 0u32);
    let mut feed = |app: &mut App, bytes: Vec<u8>| {
        blob += 1;
        let chunks: Vec<&[u8]> = bytes.chunks(448).collect();
        for (n, chunk) in chunks.iter().enumerate() {
            seq += 1;
            let mut packet = pyreal_transport::OutPacket::new(pyreal_transport::ProtoHeader {
                seq_id: seq,
                rec_id: 0xB,
                interval: 0x100,
                iteration: 1,
                ..Default::default()
            });
            packet
                .add_fragment(pyreal_transport::Fragment::new(
                    pyreal_transport::FragmentHeader {
                        blob_id_low: blob,
                        blob_id_high: 0x8000_0000,
                        num_frags: u16::try_from(chunks.len()).expect("fragments"),
                        blob_frag_size: 0,
                        blob_num: u16::try_from(n).expect("a fragment"),
                        queue_id: 5,
                    },
                    chunk.to_vec(),
                ))
                .expect("one fragment fits");
            let raw = packet
                .serialize(Some(crypto.next()))
                .expect("the envelope serialises");
            app.replay_network_mut()
                .expect("a socket-free endpoint")
                .session
                .transport
                .feed(&raw, None, pyreal_primitives::LocalTime(0.0))
                .expect("the transport takes its own envelope");
        }
        for _ in 0..2 {
            assert!(app.frame());
        }
    };

    // The world's manifest: one portal revision, the three records and their hashes.
    let base = app
        .ddd()
        .target()
        .and_then(|t| t.base(DatTarget::Portal))
        .expect("the portal file the patch is made against")
        .clone();
    let iteration = base
        .iteration_list()
        .expect("a list")
        .last()
        .copied()
        .unwrap_or(0)
        + 1;
    let records: [(DataId, &[u8]); 3] = [
        (replaced, &replacement),
        (NEW_PICTURE, &added),
        (XP_TABLE, &xp),
    ];
    let manifest = DddOverlayManifest {
        world_key: WORLD.into(),
        total_bytes: 0,
        files: vec![OverlayFileManifest {
            dat_file_type: 0,
            dat_file_id: 1,
            base_name: "client_portal.dat".into(),
            base_fingerprint: pyreal_dat::overlay::fingerprint(&base),
            base_records: None,
            base_iterations: 0,
            exact_iterations: false,
            revisions: vec![iteration],
            records: records
                .iter()
                .map(|(id, b)| OverlayRecord {
                    id: id.raw(),
                    iteration,
                    size: u32::try_from(b.len()).expect("fits"),
                    sha256: pyreal_dat::overlay::record_hash(b),
                })
                .collect(),
            tombstones: Vec::new(),
        }],
    };
    feed(
        &mut app,
        pyreal_protocol::write_blob(&manifest).expect("0xF7EC"),
    );
    assert_eq!(app.ddd().refused(), None, "{:?}", app.ddd().notices());
    feed(
        &mut app,
        pyreal_protocol::write_blob(&DddBeginDdd {
            data_expected: 0,
            revisions: vec![PatchRevision {
                dat_file_type: 0,
                dat_file_id: 1,
                iteration,
                ids_to_download: records.iter().map(|(id, _)| id.raw()).collect(),
                ids_to_purge: Vec::new(),
            }],
        })
        .expect("0xF7E7"),
    );
    for (id, bytes) in records {
        feed(
            &mut app,
            pyreal_protocol::write_blob(&DddData {
                dat_file_type: 0,
                dat_file_id: 1,
                resource_type: 0,
                resource_id: id.raw(),
                iteration,
                compressed: 0,
                version: 1,
                data_size: u32::try_from(bytes.len() + 4).expect("fits"),
                data: bytes.to_vec(),
            })
            .expect("0xF7E2"),
        );
    }
    assert_eq!(app.ddd().applied(), 3, "{:?}", app.ddd().notices());
    let before = app.store_generation();
    feed(
        &mut app,
        pyreal_protocol::write_blob(&pyreal_protocol::admin::DddEndDdd).expect("0xF7EA"),
    );
    assert!(
        app.store_generation() > before,
        "the store was opened again"
    );

    // The store reads the served records over the retail ones; the retail files are as they were.
    let store = Arc::clone(&app.store);
    assert_eq!(store.read(replaced).expect("reads"), replacement);
    assert_eq!(store.read(NEW_PICTURE).expect("reads"), added);
    assert_eq!(retail.read(replaced).expect("reads"), picture(replaced));
    assert_eq!(
        app.hud().era.level_cap,
        200,
        "the HUD's tables were read again from the served ones"
    );

    // The engine's picture path decodes the served picture, under the replaced id and the new one.
    let served = TextureSource::new(&store);
    let was = TextureSource::new(&retail);
    let want = was.decode(served_as, false, None).expect("decodes").levels;
    assert_eq!(
        served
            .decode(replaced, false, None)
            .expect("decodes")
            .levels,
        want
    );
    assert_eq!(
        served
            .decode(NEW_PICTURE, false, None)
            .expect("decodes")
            .levels,
        want
    );
    assert_ne!(
        was.decode(replaced, false, None).expect("decodes").levels,
        want
    );

    // A pack's material for the replaced picture lays over the served record; the new picture,
    // which the pack doesn't know, draws from the served data.
    let image = |side: u32| pyreal_art_pack::Image {
        width: side,
        height: side,
        srgb: false,
        levels: vec![Arc::new(vec![128; (side * side * 4) as usize])],
        format: None,
    };
    let material = pyreal_art_pack::TextureMaterial {
        textures: vec![replaced],
        albedo: Some(image(64)),
        normal: image(4),
        surface: image(4),
        grit: None,
        grit_repeat: [1.0, 1.0],
        damp_floors: false,
        slime: false,
        ice: false,
        sheen: 0.0,
        matter: Default::default(),
        fingerprints: Vec::new(),
        palettes: Vec::new(),
        draws_on: Default::default(),
        credit: Default::default(),
    };
    let pack = PackOverrides(Arc::new(pyreal_art_pack::ArtPack {
        materials: vec![material],
        ..Default::default()
    }));
    let kinds = KindTable::default();
    let over = describe_surface(
        &surface,
        false,
        None,
        &kinds,
        pack.maps(DataId(0), Some(replaced)),
    );
    assert!(
        matches!(&over.base, Some(TextureKey::Pack(k)) if k.starts_with("material:0")),
        "{:?}",
        over.base
    );
    let mut new_surface = surface;
    new_surface.orig_texture_id = Some(NEW_PICTURE);
    let own = describe_surface(
        &new_surface,
        false,
        None,
        &kinds,
        pack.maps(DataId(0), Some(NEW_PICTURE)),
    );
    assert!(
        matches!(own.base, Some(TextureKey::Dat { id, .. }) if id == NEW_PICTURE),
        "{:?}",
        own.base
    );
}
