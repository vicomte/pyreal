//! An object the server makes is drawn as the art pack's model of its setup, as the dats' own
//! statics are. Fixture: the retail dats, a graphics device, and a pack holding one triangle for
//! the academy door's setup.

use pyreal_client::app::App;
use pyreal_client_net::client_session::SessionEvent;
use pyreal_client_runtime::config::Config;
use pyreal_client_runtime::scene::SceneConfig;
use pyreal_primitives::{LocalTime, ObjectId, Quat, Vec3};
use pyreal_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use pyreal_protocol::types::physicsdesc::flags;
use pyreal_protocol::Message;

use crate::common::gpu_lock;

/// The academy door's setup, which the pack draws as a triangle.
const DOOR: u32 = 0x0200_05DA;
const THING: ObjectId = ObjectId(0x7000_0042);

/// A pack in a scratch folder: one model, a triangle, for `DOOR`.
fn pack() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pyreal-pack-models-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a pack folder");
    let mut bin = Vec::new();
    for p in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 2.0]] {
        bin.extend(p.iter().flat_map(|c| c.to_le_bytes()));
    }
    bin.extend([0u16, 1, 2].iter().flat_map(|i| i.to_le_bytes()));
    bin.extend([0, 0]);
    std::fs::write(dir.join("tri.bin"), &bin).expect("the buffer");
    std::fs::write(
        dir.join("tri.gltf"),
        r#"{
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"mesh": 0}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1}]}],
            "buffers": [{"uri": "tri.bin", "byteLength": 44}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 36},
                {"buffer": 0, "byteOffset": 36, "byteLength": 6}
            ],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                    "min": [0, 0, 0], "max": [1, 0, 2]},
                {"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}
            ]
        }"#,
    )
    .expect("the model");
    std::fs::write(
        dir.join("manifest.toml"),
        format!(
            "[pack]\nname = \"door\"\nformat = 1\n\n[[model]]\nid = \"{DOOR:#010X}\"\n\
             author = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n\n[[model.lod]]\n\
             file = \"tri.gltf\"\n"
        ),
    )
    .expect("the manifest");
    dir
}

#[test]
fn an_object_the_server_makes_is_drawn_as_the_packs_model_of_its_setup() {
    let _gpu = gpu_lock();
    let dir = pack();
    let mut app = App::new(Config {
        headless: true,
        width: 320,
        height: 240,
        sound: false,
        ui: false,
        dat_dir: pyreal_dat::testing::dat_dir(),
        base_pack: Some(dir.clone()),
        ..Default::default()
    })
    .expect("a headless App over the retail dats");
    app.load_static_scene(SceneConfig {
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        ..Default::default()
    })
    .expect("real terrain and a body");
    for _ in 0..4 {
        assert!(app.frame());
    }
    let (block, camera) = {
        let w = app.world_state().expect("a world");
        (
            w.viewer_block().expect("the viewer block"),
            w.camera.frame(),
        )
    };
    let mut p = ObjectCreatePayload {
        id: THING,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= flags::SETUP | flags::POSITION;
    p.physicsdesc.setup_id = Some(DOOR);
    p.physicsdesc.position = Some(pyreal_protocol::types::PositionWire {
        objcell_id: (u32::try_from(block.0).expect("a block") << 24)
            | (u32::try_from(block.1).expect("a block") << 16)
            | 1,
        frame: pyreal_protocol::types::Frame {
            origin: pyreal_physics::math::localtoglobal(&camera, Vec3::new(0.0, 8.0, -1.0)).into(),
            orientation: Quat::IDENTITY.into(),
        },
    });
    p.physicsdesc.timestamps.instance = 1;
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemCreateObject::OPCODE,
            body: pyreal_protocol::write_body(&ItemCreateObject(p)).expect("a create"),
        },
        LocalTime(1.0),
    );
    for _ in 0..8 {
        assert!(app.frame());
    }
    let packed = app.renderer().bridge.as_ref().map(|b| b.counts.packed);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(packed, Some(1), "the door is drawn as the pack's triangle");
}
