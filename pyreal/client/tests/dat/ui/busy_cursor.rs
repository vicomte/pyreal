//! The pointer is the hourglass while anything the player asked for is still waiting on its
//! answer: an examine, a swing the server commenced, and the portal space. Each answer takes its part down, and with nothing
//! waiting the pointer is the default arrow again.
//! Fixture: the retail dats' cursors; a headless `App` in gameplay with no network. The waiting is
//! the claim, so the app's stand-in server is turned off and every answer is given by hand.

use crate::common::app::app_in_gameplay_unanswered;
use crate::common::client_dir;

use pyreal_client::app::App;
use pyreal_client_net::client_session::SessionEvent;
use pyreal_primitives::{DataId, ObjectId};
use {pyreal_client_shell::cursor::cursor_enum, pyreal_client_shell::cursor::UICURSOR_GROUP};

const PLAYER: ObjectId = ObjectId(0x5000_000A);
const SWORD: ObjectId = ObjectId(0x8000_0101);

fn cursor(key: u32) -> DataId {
    assert!(
        pyreal_dat::testing::have_dats(),
        "the shipped cursors are this file's oracle: no dats at {} -- set PYREAL_TEST_DAT_DIR",
        client_dir().display()
    );
    let store = pyreal_dat::RetailDatStore::open_dir(&client_dir()).expect("open the retail dats");
    pyreal_client_runtime::assets::enum_did(&store, UICURSOR_GROUP, key)
        .expect("the shipped cursor")
}

fn settle(app: &mut App) {
    for _ in 0..2 {
        app.frame();
    }
}

/// Each waiting request puts the hourglass up and its answer takes it down.
///
/// Falsified by a pointer that reads only the portal space (the examine and the swing would leave
/// the arrow up), and by a raise with no matching lowering (the arrow would never come back).
#[test]
fn the_hourglass_is_up_while_a_request_waits_and_down_once_it_is_answered() {
    let (wait, arrow) = (cursor(cursor_enum::WAIT), cursor(cursor_enum::DEFAULT));
    let mut app = app_in_gameplay_unanswered(3, Some(PLAYER));

    settle(&mut app);
    assert_eq!(
        app.objects().world.magic.busy_count,
        0,
        "nothing is asked yet"
    );
    assert_eq!(app.current_cursor_did(), Some(arrow), "nothing waits");

    // An examine.
    let mut req = pyreal_client_model::RecordingRequests::default();
    app.probe_mut()
        .objects_mut()
        .world
        .examine_object(&mut req, SWORD);
    settle(&mut app);
    assert_eq!(
        app.current_cursor_did(),
        Some(wait),
        "waiting on the examine"
    );
    let mut sink = pyreal_client_model::RecordingSink::default();
    app.probe_mut()
        .objects_mut()
        .world
        .set_appraise_info(SWORD, Default::default(), &mut sink);
    settle(&mut app);
    assert_eq!(
        app.current_cursor_did(),
        Some(arrow),
        "the examine answered"
    );

    // A swing the server commenced.
    app.probe_mut().objects_mut().world.handle_commence_attack();
    settle(&mut app);
    assert_eq!(
        app.current_cursor_did(),
        Some(wait),
        "the swing is outstanding"
    );
    app.probe_mut().objects_mut().world.handle_attack_done(
        &mut req,
        0,
        true,
        pyreal_primitives::LocalTime(0.0),
    );
    settle(&mut app);
    assert_eq!(app.current_cursor_did(), Some(arrow), "the swing is over");

    // The portal space: the player appears with its position unsettled, which starts the tunnel.
    app.teleport
        .apply_events(&[SessionEvent::PlayerCreated(PLAYER)]);
    settle(&mut app);
    assert_eq!(app.current_cursor_did(), Some(wait), "in portal space");
    app.teleport.step_world_view(false);
    for _ in 0..900 {
        app.frame();
    }
    assert_eq!(
        app.teleport.anim.state,
        pyreal_client_contract::teleport::TeleportAnimState::Off,
        "the world has faded back in"
    );
    assert_eq!(app.objects().world.magic.busy_count, 0);
    assert_eq!(app.current_cursor_did(), Some(arrow), "out of portal space");

    app.shutdown();
}
