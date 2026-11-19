//! A character runs from the moment it enters the world: run is the default, and the walk key
//! (Shift+R) switches it to walking and back. Nothing has to be pressed first, so a character entered
//! without a key going down since the client started runs too.
//!
//! Fixture: a headless App with the shipped key map, logged in through `App::attach_replay_network`'s
//! socket-free endpoint (every blob handed to `Transport::feed` in memory; no datagram leaves the
//! process), a whole world entry with no input before it, then the walk key's physical messages.

use crate::common::client_dir;
use crate::common::sim_app::key;

use pyreal_client::app::App;
use pyreal_client_runtime::config::Config;
use pyreal_client_runtime::net::ClientNetwork;
use pyreal_client_runtime::scene::SceneConfig;
use pyreal_primitives::ObjectId;
use pyreal_protocol::login::LoginEnterGameServerReady;
use pyreal_protocol::objects::{ItemCreateObject, LoginCreatePlayer, ObjectCreatePayload};
use pyreal_protocol::types::physicsdesc::flags;
use pyreal_protocol::types::PositionWire;
use winit::keyboard::KeyCode;

/// Holtburg.
const CELL: u32 = 0xA9B4_001D;
const POS: (f32, f32, f32) = (60.0, 80.0, 42.0);
const PLAYER: ObjectId = ObjectId(0x5000_0001);

/// Queue 9 carries UI messages and queue 10 carries world-object messages.
const UI_QUEUE: u16 = 9;
const SMARTBOX_QUEUE: u16 = 10;

/// The socket-free peer, as `chat::chat_entry_after_login` drives it.
struct Peer {
    crypto: pyreal_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "p710", "unused", 0)
            .expect("a socket-free client network");
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some("127.0.0.1:19000".parse().expect("a literal address")),
        );
        (
            Self {
                crypto: pyreal_transport::CryptoSystem::new(0xDEAD_BEEF),
                sequence: 1,
                blob: 0,
            },
            net,
        )
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
            .expect("one fragment fits");
        let raw = packet
            .serialize(Some(self.crypto.next()))
            .expect("the envelope serialises");
        app.replay_network_mut()
            .expect("an explicitly socket-free endpoint")
            .session
            .transport
            .feed(&raw, None, pyreal_primitives::LocalTime(0.0))
            .expect("the transport accepts its own envelope");
    }
}

/// The player's own `0xF745`, carrying the placement the server chose.
fn player_create() -> Vec<u8> {
    let mut p = ObjectCreatePayload {
        id: PLAYER,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= flags::SETUP | flags::POSITION;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.timestamps.instance = 1;
    p.physicsdesc.position = Some(PositionWire {
        objcell_id: CELL,
        frame: pyreal_protocol::types::Frame {
            origin: pyreal_protocol::types::Vec3 {
                x: POS.0,
                y: POS.1,
                z: POS.2,
            },
            ..Default::default()
        },
    });
    pyreal_protocol::write_blob(&ItemCreateObject(p)).expect("the create encodes")
}

fn frames(app: &mut App, n: u32, what: &str) {
    for i in 0..n {
        assert!(app.frame(), "{what}: frame {i} does not end the client");
    }
}

/// A client in the world after one whole world entry, with no key pressed since it started.
fn in_world() -> App {
    assert!(
        pyreal_dat::testing::have_dats(),
        "the retail dats must be at {} (set PYREAL_TEST_DAT_DIR)",
        client_dir().display()
    );
    let mut app = crate::common::sim_app::new(Config {
        headless: true,
        sound: false,
        ui: true,
        preferences_file: std::env::temp_dir().join("pyreal-run-after-login-not-created/prefs.ini"),
        dat_dir: client_dir(),
        ..Default::default()
    })
    .expect("the application comes up headless");
    app.start_shell().expect("the UI shell comes up");
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("a headless App with no live link");
    app.defer_static_scene(SceneConfig {
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..SceneConfig::default()
    });
    app.replay_network_mut()
        .expect("the endpoint")
        .enter_world(PLAYER, "p710");
    frames(&mut app, 1, "the entry request");
    peer.send(
        &mut app,
        UI_QUEUE,
        pyreal_protocol::write_blob(&LoginEnterGameServerReady).expect("0xF7DF"),
    );
    frames(&mut app, 2, "the server's ready");
    peer.send(
        &mut app,
        SMARTBOX_QUEUE,
        pyreal_protocol::write_blob(&LoginCreatePlayer { player_id: PLAYER }).expect("0xF746"),
    );
    peer.send(&mut app, SMARTBOX_QUEUE, player_create());
    frames(&mut app, 5, "the world entry");
    // The scripted login does not open the game screen by itself; the live client reaches it
    // from character select.
    crate::common::sim_app::game_screen(&mut app);
    frames(&mut app, 3, "the gameplay screen");
    assert_eq!(
        app.probe_mut().objects_mut().world.player,
        Some(PLAYER),
        "the login put a player in the world"
    );
    app
}

/// Run is the default from the world entry on, and Horizon's walk key (Shift+R) switches to walking
/// and back.
#[test]
fn a_character_entered_with_no_key_pressed_runs_and_the_walk_key_switches_it_to_walking() {
    let mut app = in_world();
    assert!(
        app.objects().world.player_system.options.toggle_run(),
        "the character's toggle-run option is on, as a new character's is"
    );
    assert!(
        app.probe().char_input().run,
        "the character runs on entering the world, before any key is pressed"
    );

    walk_key(&mut app, 10_000);
    frames(&mut app, 2, "the walk key pressed");
    assert!(!app.probe().char_input().run, "the walk key walks");

    walk_key(&mut app, 10_200);
    frames(&mut app, 2, "the walk key pressed again");
    assert!(app.probe().char_input().run, "pressed again, it runs");
}

/// Shift+R pressed and released.
fn walk_key(app: &mut App, t: u32) {
    key(app, KeyCode::ShiftLeft, true, t);
    key(app, KeyCode::KeyR, true, t + 10);
    frames(app, 1, "the walk key down");
    key(app, KeyCode::KeyR, false, t + 20);
    key(app, KeyCode::ShiftLeft, false, t + 30);
}
