//! Losing window focus releases every held control: with Jump held the charge is released and
//! lateral movement still works. Fixture: a headless `App` on the default landblock with an animated body,
//! the player created and unhidden from the early-inventory-and-casting recording, and window
//! events fed through `Pump` to the input manager as `App::run_event_loop` does.
//!
//! # What focus loss does
//!
//! The input manager takes one `timeGetTime()` stamp and then does **three** things, in order:
//!
//! 1. **A synthetic zero-extent release per held control.** While the active-control map is not
//!    empty it unlinks one entry, clears that key's meta-mode bit, finds the `ActionState` for the
//!    recorded action, builds an `InputEvent` with extent 0.0 at the one stamp, and fires it down
//!    the ordinary action-event path. It synthesises an event; it does not reach into the
//!    consumers' state.
//! 2. **Keyboard-focus cleanup**, once the active-control hash is empty. The command interpreter
//!    clears keyboard commands from all three command lists and invokes its two further cleanup
//!    hooks.
//! 3. **The mouse-look release.** When mouse look is active and a device exists, it fires a button
//!    release for control 1, subcontrol 0, on the virtual device with activation `0x80` (`ANALOG`)
//!    and the same timestamp. The hold that put the client into mouse look therefore cannot survive
//!    the focus loss.
//!
//! # Where the client does it
//!
//! `Pump::map_window_event` turns `Focused(false)` into `WM_ACTIVATEAPP`/`WM_ACTIVATE`/
//! `WM_KILLFOCUS`, and `pyreal_input`'s `WM_KILLFOCUS` arm calls
//! `InputShell::release_pressed_keys`, which walks its `active_controls` and calls
//! `deactivate_action_key`: a real `extent: 0.0, start: false` event on the ordinary queue.
//! `App::apply_input_actions` routes it to the movement command handler's jump release arm. The
//! same arm clears the input manager's `in_mouse_look`; `App::mouse_look` is a second, app-level mouse-look latch, and focus loss releases it too.
//!
//! `winit::event::WindowEvent::MouseInput` and `CursorMoved` both carry a `DeviceId` that cannot
//! be constructed without `unsafe`, which this crate forbids, so the mouse half of
//! `note_flycam_input` is reached through `App::mouse_look_button` and `App::cursor_moved`, the
//! same shape `App::flycam_key` has for the keyboard half.

use crate::common::sim_app::{body, frames, key, movement_key, unhide_player as unhide_the_player};

use pyreal_client_net::client_session::{testing::Corpus, SessionEvent};
use pyreal_primitives::num::math;
use pyreal_primitives::{LocalTime, ObjectId};
use pyreal_protocol::{Message, Opcode};
use {
    pyreal_client::app::App, pyreal_client_runtime::config::Config,
    pyreal_client_runtime::scene::SceneConfig, pyreal_desktop::pump::Pump,
    pyreal_world_data::landblock::DEFAULT_LANDBLOCK,
};

fn setup() -> App {
    let mut app = crate::common::sim_app::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: 640,
        height: 480,
        dat_dir: pyreal_dat::testing::dat_dir(),
        ..Default::default()
    })
    .expect("required DATs and simulated world");
    app.start_shell().unwrap();
    crate::common::sim_app::game_screen(&mut app);
    frames(&mut app, 4);
    app.load_static_scene(SceneConfig {
        landblock: DEFAULT_LANDBLOCK,
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..Default::default()
    })
    .expect("real terrain and animated body");
    frames(&mut app, 60);
    let corpus = Corpus::shared("early-inventory-and-casting");
    let player = corpus.blobs.iter().find(|b| b.opcode == 0xf746).unwrap();
    let id = ObjectId(u32::from_le_bytes(player.payload[4..8].try_into().unwrap()));
    let row = corpus
        .blobs
        .iter()
        .find(|b| b.opcode == 0xf745 && b.payload[4..8] == id.0.to_le_bytes())
        .unwrap();
    let mut create = pyreal_protocol::objects::ItemCreateObject::read(
        &mut pyreal_protocol::Reader::new(&row.payload[4..]),
    )
    .unwrap();
    let p = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position();
    create.0.physicsdesc.position = Some(pyreal_protocol::types::PositionWire {
        objcell_id: p.cell.0,
        frame: pyreal_protocol::types::Frame {
            origin: pyreal_protocol::types::Vec3 {
                x: p.frame.origin.x,
                y: p.frame.origin.y,
                z: p.frame.origin.z,
            },
            orientation: pyreal_protocol::types::Quat {
                w: p.frame.rotation.w,
                x: p.frame.rotation.x,
                y: p.frame.rotation.y,
                z: p.frame.rotation.z,
            },
        },
    });
    create.0.physicsdesc.bitfield |= pyreal_protocol::types::physicsdesc::flags::POSITION;
    app.probe_mut()
        .objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(id), LocalTime(1.0));
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: pyreal_protocol::write_body(&create).unwrap(),
        },
        LocalTime(1.0),
    );
    unhide_the_player(&mut app, corpus, id);
    frames(&mut app, 60);
    let c = app.world_state().unwrap().character.as_ref().unwrap();
    assert!(c.on_ground());
    assert!(
        !c.world
            .get(c.handle)
            .expect("the local collision body")
            .state()
            .is_hidden(),
        "premise: the login create arrives with HIDDEN_PS and a hidden body never \
         advances its animation offset, so the recorded unhide has to have reached it"
    );
    assert!(!app.objects().world.combat.jump_pending);
    app
}

fn space(app: &mut App, down: bool, time: u32) {
    key(app, winit::keyboard::KeyCode::Space, down, time);
}

fn player_description(app: &mut App, name: &str) {
    let corpus = Corpus::shared(name);
    let row = corpus
        .blobs
        .iter()
        .find(|b| b.opcode == 0xf7b0 && b.payload[12..16] == 0x13_u32.to_le_bytes())
        .expect("0013 player description");
    let desc = pyreal_protocol::read_body::<pyreal_protocol::login::LoginPlayerDescription>(
        &row.payload[16..],
    )
    .unwrap();
    let event = SessionEvent::PlayerDescription(Box::new(desc));
    app.probe_mut()
        .objects_mut()
        .apply_event(&event, LocalTime(1.0));
    app.apply_hud_events(std::slice::from_ref(&event));
    app.probe_mut()
        .apply_interaction_events(std::slice::from_ref(&event));
}

/// Exactly the pair `App::run_event_loop` performs for one `WindowEvent`: the direct arm first,
/// then every `Win32Message` the pump maps it to, fed to the input manager.
fn window_event(app: &mut App, event: &pyreal_input::host::HostEvent, time: u32) {
    app.note_flycam_input(event);
    let mut pump = Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    for m in pump.map_window_event(event, time) {
        app.input_manager_mut().unwrap().on_message(m);
    }
}

fn lose_focus(app: &mut App, time: u32) {
    window_event(app, &pyreal_input::host::HostEvent::Focused(false), time);
}

// -------------------------------------------------------------------------------------------
// 1. The per-control releases, and the movement state they must leave behind.
// -------------------------------------------------------------------------------------------

/// **A focus loss with Jump held must release the jump, not strand the charge.**
///
/// `standing_longjump` gates the three lateral movement commands and no turn, so a client that
/// kept it across an alt-tab would be able to turn and unable to translate, with no way out.
#[test]
fn focus_loss_with_jump_held_releases_the_charge_and_leaves_lateral_movement_working() {
    let mut app = setup();
    player_description(&mut app, "early-inventory-and-casting");

    space(&mut app, true, 700_000);
    frames(&mut app, 4);
    // The setup was alive enough to fail: the charge really is up and really is a standing one.
    assert!(
        app.objects().world.combat.jump_pending,
        "the charge started"
    );
    assert!(
        body(&app).driver().movement.interp.standing_longjump,
        "and it is the standing charge that gates lateral movement"
    );
    let before = app.probe().jump_counts();

    lose_focus(&mut app, 700_100);
    frames(&mut app, 4);

    assert!(
        !app.objects().world.combat.jump_pending,
        "the synthesized zero-extent release must reach the combat controller's jump action"
    );
    assert!(
        !body(&app).driver().movement.interp.standing_longjump,
        "the charge is gone, so the lateral gate must be down"
    );
    assert_eq!(
        app.probe().jump_counts().0,
        before.0 + 1,
        "the release increments the jump counter"
    );

    // And the gate really is down: the body translates again, through the real command owner.
    frames(&mut app, 70);
    assert!(body(&app).on_ground(), "the jump finished before this step");
    let start = body(&app).position().frame.origin;
    movement_key(
        &mut app,
        pyreal_client_runtime::actions::movement::action::MOVE_FORWARD,
        true,
        700_400,
    );
    frames(&mut app, 40);
    let moved = body(&app).position().frame.origin;
    assert!(
        math::hypotf(moved.x - start.x, moved.y - start.y) > 0.25,
        "lateral movement is still locked after the focus loss: {start:?} -> {moved:?}, \
         longjump={} pending={}",
        body(&app).driver().movement.interp.standing_longjump,
        app.objects().world.combat.jump_pending,
    );
    movement_key(
        &mut app,
        pyreal_client_runtime::actions::movement::action::MOVE_FORWARD,
        false,
        700_600,
    );
    frames(&mut app, 10);
    app.shutdown();
}
