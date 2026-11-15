//! The body's scripts and ramps run on the frame's clock: after the recorded session's scripts and
//! hides on the player, in order, its parts are drawn and every fade has finished, none held by a
//! clock that never advances.

use crate::common::sim_app::{app_with_recorded_body, frames};
use pyreal_client_net::client_session::{
    testing::{Corpus, Direction},
    SessionEvent,
};
use pyreal_primitives::LocalTime;
use pyreal_protocol::Opcode;

#[test]
fn the_recorded_body_is_drawn_after_its_scripts() {
    let mut app = app_with_recorded_body();
    let player = app.objects().player().expect("the player");
    let corpus = Corpus::load("early-inventory-and-casting")
        .unwrap()
        .unwrap();
    let mut played = 0;
    for row in corpus.blobs.iter().filter(|r| {
        r.dir == Direction::ServerToClient
            && (r.opcode == Opcode::EFFECTS_PLAY_SCRIPT_TYPE.0
                || r.opcode == Opcode::EFFECTS_PLAY_SCRIPT_ID.0
                || r.opcode == Opcode::ITEM_SET_STATE.0)
            && r.payload[4..8] == player.0.to_le_bytes()
    }) {
        app.probe_mut().objects_mut().apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode(row.opcode),
                body: row.payload[4..].to_vec(),
            },
            LocalTime(1.0),
        );
        frames(&mut app, 30);
        played += 1;
    }
    eprintln!("{played} scripts played on the player");
    assert!(
        played > 0,
        "the recording plays no script on the player, so nothing below is tested"
    );
    frames(&mut app, 600);
    let c = app.world_state().unwrap().character.as_ref().unwrap();
    let d = c.driver();
    let parts = &d.part_array.parts;
    assert!(!parts.is_empty(), "the body has parts");
    let hidden: Vec<(usize, f32)> = parts
        .iter()
        .enumerate()
        .filter(|(_, p)| p.gfxobj_id.0 != 0)
        .filter_map(|(k, p)| {
            let t = p.material.map_or(0.0, |m| m.translucency);
            (p.no_draw() || t >= 0.5).then_some((k, t))
        })
        .collect();
    assert!(hidden.is_empty(), "parts left hidden or faded: {hidden:?}");
    // Every fade the scripts started has run its course on the frame's clock.
    assert!(
        d.fp_hooks.is_empty(),
        "{} ramps never finished",
        d.fp_hooks.len()
    );
}
