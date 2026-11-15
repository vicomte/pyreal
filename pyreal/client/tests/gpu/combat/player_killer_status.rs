//! Live `PlayerKillerStatus` (Int 134) updates reach the local PWD predicates: the private
//! `0x02CD` and public `0x02CE` int updates set the PK and PKLite bits that attack eligibility
//! and the radar read, with fellowship colours overriding the PK base. The stored Int 134 and the PWD bits agree, and a PK pair
//! passes the local pre-send attack gate.
//! Fixture: a headless `App` in gameplay with a socket-free replay endpoint fed synthetic
//! datagrams, and long-solo-play's recorded player description.

#![cfg(gpu)]

use crate::common::client_dir_required as client_dir;
use crate::common::gpu_lock;

use pyreal_client_model::combat::{AttackHeight, CombatMode};
use pyreal_client_model::{RecordingRequests, Request, StatKey, StatType, StatValue};
use pyreal_client_net::client_session::testing::{Corpus, Direction};
use pyreal_client_net::client_session::SessionEvent;
use pyreal_primitives::{LocalTime, ObjectId, Quat, ServerTime, Vec3};
use pyreal_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use pyreal_protocol::qualities as wire;
use pyreal_protocol::Opcode;
use pyreal_ui_logic::radar::{get_blip_color, semantic};
use {
    pyreal_client::app::App, pyreal_client_runtime::config::Config,
    pyreal_client_runtime::net::ClientNetwork, pyreal_client_runtime::scene::SceneConfig,
};
use {pyreal_rules::weenie::bitfield, pyreal_rules::weenie::item_type};

const PLAYER: ObjectId = ObjectId(0x5000_0001);
const TARGET: ObjectId = ObjectId(0x5000_0022);
const PLAYER_KILLER_STATUS: u32 = 134;

struct Peer {
    crypto: pyreal_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
    stamp: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net =
            ClientNetwork::new("127.0.0.1:19100", 7304, "player-killer-status", "unused", 0)
                .expect("a socket-free replay net");
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some("127.0.0.1:19100".parse().expect("addr")),
        );
        (
            Self {
                crypto: pyreal_transport::CryptoSystem::new(0xDEAD_BEEF),
                sequence: 1,
                blob: 0,
                stamp: 0,
            },
            net,
        )
    }

    fn event(&mut self, app: &mut App, inner: Vec<u8>) {
        self.stamp += 1;
        let mut blob = 0xF7B0_u32.to_le_bytes().to_vec();
        blob.extend_from_slice(&PLAYER.0.to_le_bytes());
        blob.extend_from_slice(&self.stamp.to_le_bytes());
        blob.extend_from_slice(&inner);
        self.send(app, 9, blob);
    }

    fn send(&mut self, app: &mut App, queue: u16, bytes: Vec<u8>) {
        self.sequence += 1;
        self.blob += 1;
        let mut packet = pyreal_transport::OutPacket::new(pyreal_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_fragment(pyreal_transport::Fragment::new(
                pyreal_transport::FragmentHeader {
                    blob_id_low: self.blob,
                    blob_id_high: 0x8000_0000,
                    num_frags: 1,
                    blob_frag_size: 0,
                    blob_num: 0,
                    queue_id: queue,
                },
                bytes,
            ))
            .expect("one fragment");
        let raw = packet
            .serialize(Some(self.crypto.next()))
            .expect("a datagram");
        app.replay_network_mut()
            .expect("the replay endpoint")
            .session
            .transport
            .feed(&raw, None, LocalTime(0.0))
            .expect("fed only to the socket-free endpoint");
    }
}

fn settle(app: &mut App) {
    for _ in 0..6 {
        app.frame();
    }
}

fn player_description() -> pyreal_protocol::login::LoginPlayerDescription {
    let corpus = Corpus::shared("long-solo-play");
    let payload = corpus
        .blobs
        .iter()
        .find(|b| {
            b.dir == Direction::ServerToClient
                && b.opcode == 0xF7B0
                && b.payload.len() >= 16
                && u32::from_le_bytes(b.payload[12..16].try_into().expect("four bytes")) == 0x0013
        })
        .map(|b| b.payload.clone())
        .expect("long-solo-play carries 0x0013");
    pyreal_protocol::read_body::<pyreal_protocol::login::LoginPlayerDescription>(&payload[16..])
        .expect("the recorded player description decodes")
}

fn setup() -> (App, Peer) {
    let mut app = App::new(Config {
        ui: true,
        headless: true,
        sound: false,
        width: 800,
        height: 600,
        dat_dir: client_dir(),
        preferences_file: std::env::temp_dir()
            .join("pyreal-player-killer-status-not-created/prefs.ini"),
        ..Config::default()
    })
    .expect("an application");
    app.start_shell().expect("the shell starts");
    crate::common::app::game_screen(&mut app);
    app.load_static_scene(SceneConfig {
        character: false,
        cell_statics: false,
        mesh_collision: false,
        land_radius: 1,
        scenery_radius: 0,
        particles: false,
        ..SceneConfig::default()
    })
    .expect("the retail scene loads");
    for _ in 0..4 {
        app.frame();
    }
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("the replay endpoint attaches");

    let scene = app.world_state().expect("the retail scene");
    let camera = scene.camera.frame();
    let block = scene.viewer_block().expect("the viewer block");
    for (id, x) in [(PLAYER, -2.0), (TARGET, 2.0)] {
        let mut p = ObjectCreatePayload {
            id,
            ..Default::default()
        };
        p.physicsdesc.bitfield |= pyreal_protocol::types::physicsdesc::flags::SETUP
            | pyreal_protocol::types::physicsdesc::flags::POSITION;
        p.physicsdesc.setup_id = Some(0x0200_0001);
        p.physicsdesc.position = Some(pyreal_protocol::types::PositionWire {
            objcell_id: ((block.0 as u32) << 24) | ((block.1 as u32) << 16) | 1,
            frame: pyreal_protocol::types::Frame {
                origin: pyreal_physics::math::localtoglobal(&camera, Vec3::new(x, 18.0, -1.0))
                    .into(),
                orientation: Quat::IDENTITY.into(),
            },
        });
        p.physicsdesc.timestamps.instance = 1;
        peer.send(
            &mut app,
            10,
            pyreal_protocol::write_blob(&ItemCreateObject(p)).expect("create"),
        );
        app.frame();
    }
    app.probe_mut().objects_mut().world.player = Some(PLAYER);
    for (id, name) in [(PLAYER, "Local PK"), (TARGET, "Target PK")] {
        let w = app
            .probe_mut()
            .objects_mut()
            .world
            .weenie_mut(id)
            .expect("created weenie");
        w.pwd.name = name.to_owned();
        w.pwd.obj_type = item_type::CREATURE;
        w.pwd.bitfield = bitfield::PLAYER;
        w.pwd.radar_enum = Some(4);
    }

    let desc = SessionEvent::PlayerDescription(Box::new(player_description()));
    app.probe_mut()
        .objects_mut()
        .apply_event(&desc, LocalTime(0.0));
    app.apply_hud_events(std::slice::from_ref(&desc));
    app.probe_mut()
        .apply_interaction_events(std::slice::from_ref(&desc));
    app.probe_mut()
        .objects_mut()
        .world
        .player_system
        .set_option(14, true, ServerTime(0.0));
    app.probe_mut().objects_mut().world.set_selected_object(
        Some(TARGET),
        false,
        &mut pyreal_client_model::NullSink,
    );
    settle(&mut app);
    (app, peer)
}

fn private_int(sequence: u8, value: i32) -> Vec<u8> {
    let mut out = Opcode::QUALITIES_PRIVATE_UPDATE_INT
        .0
        .to_le_bytes()
        .to_vec();
    out.extend_from_slice(
        &pyreal_protocol::write_body(&wire::QualitiesPrivateUpdateInt(wire::PrivateUpdate {
            sequence,
            property_id: PLAYER_KILLER_STATUS,
            value,
        }))
        .expect("private Int update"),
    );
    out
}

fn public_int(object: ObjectId, sequence: u8, value: i32) -> Vec<u8> {
    let mut out = Opcode::QUALITIES_UPDATE_INT.0.to_le_bytes().to_vec();
    out.extend_from_slice(
        &pyreal_protocol::write_body(&wire::QualitiesUpdateInt(wire::PublicUpdate {
            sequence,
            object,
            property_id: PLAYER_KILLER_STATUS,
            value,
        }))
        .expect("public Int update"),
    );
    out
}

fn status(app: &App, id: ObjectId) -> (bool, bool) {
    let w = app.objects().world.weenie(id).expect("weenie");
    (w.is_pk(), w.is_pk_lite())
}

#[test]
fn live_pk_updates_drive_attack_eligibility_and_the_radar_from_the_same_pwd_bits() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    assert_eq!(
        status(&app, PLAYER),
        (false, false),
        "the local baseline is NPK"
    );
    assert_eq!(
        status(&app, TARGET),
        (false, false),
        "the target baseline is NPK"
    );
    assert!(
        !app.objects().world.object_is_attackable(TARGET),
        "two NPK players cannot attack"
    );

    // The local player's private form has no second dispatcher; the target's public form is the
    // ordinary object-update route. Both must land on the PWD predicates retail calls.
    peer.event(&mut app, private_int(1, 4));
    peer.event(&mut app, public_int(TARGET, 1, 4));
    settle(&mut app);
    assert_eq!(
        status(&app, PLAYER),
        (true, false),
        "private Int134 updates local IsPK"
    );
    assert_eq!(
        status(&app, TARGET),
        (true, false),
        "public Int134 updates target IsPK"
    );
    assert!(
        app.objects().world.object_is_attackable(TARGET),
        "both PK now pass the local gate"
    );
    assert_eq!(
        app.objects()
            .world
            .player_qualities()
            .expect("player descriptor")
            .get(StatKey::new(StatType::Int, PLAYER_KILLER_STATUS)),
        Some(StatValue::Int(4)),
        "the quality store and PWD mirror agree"
    );

    let target_blip = app
        .hud()
        .radar
        .iter()
        .find(|b| b.id == TARGET)
        .expect("target radar row");
    assert_eq!(
        get_blip_color(Some(target_blip)).hex,
        semantic::PLAYER_KILLER.hex,
        "a non-fellow PK uses the native red base color"
    );
    let mut fellow_control = *target_blip;
    fellow_control.is_fellow = true;
    assert_eq!(
        get_blip_color(Some(&fellow_control)).hex,
        semantic::FELLOWSHIP.hex,
        "native fellowship color overrides the PK base; this is not a PK-state defect"
    );

    app.probe_mut().objects_mut().world.combat.combat_mode = CombatMode::Melee;
    app.probe_mut()
        .objects_mut()
        .world
        .combat
        .requested_attack_power = 0.5;
    let mut requests = RecordingRequests::default();
    let refusal = app.probe_mut().objects_mut().world.execute_attack(
        &mut requests,
        AttackHeight::Medium,
        false,
        true,
    );
    assert_eq!(
        refusal, None,
        "the local attack path does not refuse the live PK pair"
    );
    assert!(
        matches!(
            requests.0.as_slice(),
            [Request::TargetedMeleeAttack(a)] if a.target == TARGET
        ),
        "the eligible pair reaches the exact targeted-melee request: {:?}",
        requests.0
    );

    // A public update naming the player is first accepted by Hud. The boundary sees the shared
    // stamp as stale, so this arm shows the first consumer itself mirrors instead of depending on
    // the second one.
    peer.event(&mut app, public_int(PLAYER, 2, 0));
    settle(&mut app);
    assert_eq!(
        status(&app, PLAYER),
        (false, false),
        "public Int134 clears the local PK bits too"
    );
    assert!(
        !app.objects().world.object_is_attackable(TARGET),
        "the attack gate follows the clear"
    );

    peer.event(&mut app, private_int(3, 0x40));
    peer.event(&mut app, public_int(TARGET, 2, 0x40));
    settle(&mut app);
    assert_eq!(
        status(&app, PLAYER),
        (false, true),
        "private Int134 also moves to PKLite"
    );
    assert_eq!(
        status(&app, TARGET),
        (false, true),
        "the target moves to PKLite"
    );
    assert!(
        app.objects().world.object_is_attackable(TARGET),
        "matching PKLite also passes"
    );
    app.shutdown();
}
