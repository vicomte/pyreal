//! `/loc`'s location line: `Your location is: 0x%08X [%f %f %f] %f %f %f %f` -- the cell, the
//! origin xyz, then the rotation quaternion wxyz, each to six decimals. The expected line is
//! formatted here, independently of the client's formatter, and checked against a line read off a
//! retail screenshot in Holtburg.
//!
//! Fixture: a headless App with a static scene and its body teleported to the screenshot's pose.
use crate::common::client_dir_or_workspace_client as client_dir;

use pyreal_client::app::App;
use pyreal_client_runtime::config::Config;
use pyreal_client_runtime::scene::SceneConfig;
use pyreal_primitives::{CellId, Frame, Position, Quat, Vec3};

/// The pose read off a retail screenshot: Holtburg, cell `0xA9B4002A`.
fn holtburg() -> Position {
    Position::new(
        CellId(0xA9B4_002A),
        Frame::new(
            Vec3 {
                x: 133.168_5,
                y: 27.490_582,
                z: 94.005_005,
            },
            Quat {
                w: -0.991_022,
                x: 0.0,
                y: 0.0,
                z: -0.133_696,
            },
        ),
    )
}

/// Load a local static scene, attach/teleport its body and put the interface on its game screen. This supplies
/// the body directly rather than replaying world entry. DATs and application creation must
/// succeed; frames may be zero for the immediate transcript-format check.
fn app_with_body_at(at: Position, frames: u32) -> App {
    let d = client_dir();
    assert!(
        pyreal_dat::testing::have_dats(),
        "the retail dats are required at {} -- set PYREAL_TEST_DAT_DIR",
        d.display()
    );
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        dat_dir: d,
        ..Config::default()
    };
    let mut app = crate::common::sim_app::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    let block = at.cell.landblock();
    app.load_static_scene(SceneConfig {
        landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..SceneConfig::default()
    })
    .expect("the static scene and the body load");
    {
        let c = app
            .probe_mut()
            .world_state_mut()
            .unwrap()
            .character
            .as_mut()
            .unwrap();
        c.land().load_block_cells(block);
        c.teleport(at);
    }
    crate::common::sim_app::game_screen(&mut app);
    for _ in 0..frames {
        app.frame();
    }
    app
}

fn body_position(app: &App) -> Position {
    app.world_state()
        .and_then(|w| w.character.as_ref())
        .expect("a body")
        .position()
}

/// An independent spelling of the location format: eight uppercase hexadecimal cell
/// digits, origin xyz and quaternion wxyz, each float widened from f32 and printed to six
/// decimals. The helper supplies the prefix; the output comparison trims leading and trailing
/// newlines.
fn retail_loc_line(p: &Position) -> String {
    let o = p.frame.origin;
    let q = p.frame.rotation;
    format!(
        "Your location is: 0x{:08X} [{:.6} {:.6} {:.6}] {:.6} {:.6} {:.6} {:.6}",
        p.cell.0,
        f64::from(o.x),
        f64::from(o.y),
        f64::from(o.z),
        f64::from(q.w),
        f64::from(q.x),
        f64::from(q.y),
        f64::from(q.z)
    )
}

/// Teleport to the recorded screenshot pose with zero subsequent frames. Compare the local
/// expected-line formatter and the production position formatter with exact transcript literals,
/// pinning cell, seven float fields, quaternion w-first order and six decimals. This test does
/// not submit the command or read the screenshot image.
#[test]
fn the_screenshot_position_renders_as_the_screenshot_line() {
    let app = app_with_body_at(holtburg(), 0);
    let at = body_position(&app);
    assert_eq!(
        retail_loc_line(&at),
        "Your location is: 0xA9B4002A [133.168503 27.490582 94.005005] -0.991022 0.000000 0.000000 -0.133696",
        "the position transcript read from a retail Holtburg screenshot"
    );
    assert_eq!(
        pyreal_client_runtime::interaction::position_to_string(&at),
        "0xA9B4002A [133.168503 27.490582 94.005005] -0.991022 0.000000 0.000000 -0.133696"
    );
}
