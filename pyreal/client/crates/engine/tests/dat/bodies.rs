//! An animated body poses its parts where the client's animation code places them.

use std::sync::Arc;

use pyreal_animation::data::AnimAssets;
use pyreal_animation::motion::MovementParameters;
use pyreal_animation::{step_animation, MotionCommand, MotionDriver};
use pyreal_engine::body::{part_affine, Body, QUANTUM};
use pyreal_primitives::{DataId, Frame, LocalTime, Quat, Vec3};
use pyreal_world_data::anim_assets::DatAnimAssets;

/// A drudge and the motion table it moves by.
const DRUDGE: DataId = DataId(0x0200_07DD);
const DRUDGE_MOTIONS: DataId = DataId(0x0900_0008);

/// A drudge walking, 0.37 s in, standing at a turn: each part's transform is the one the client's
/// own motion driver gives the same setup, table and motion stepped the way the world steps an
/// object; and the walk has moved its parts from where they stood when it began.
#[test]
fn an_animated_part_is_placed_where_the_clients_animation_places_it() {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let assets: Arc<dyn AnimAssets> = Arc::new(DatAnimAssets::new(Arc::clone(&store)));
    let half = 0.3f32;
    let frame = Frame::new(
        Vec3::new(10.0, 20.0, 5.0),
        Quat {
            w: pyreal_primitives::num::math::cosf(half),
            x: 0.0,
            y: 0.0,
            z: pyreal_primitives::num::math::sinf(half),
        },
    );
    let walk = MotionCommand::from_name("WalkForward").expect("a walk");
    let mut body = Body::new(Arc::clone(&assets), DRUDGE, None, frame).expect("a drudge");
    assert!(body.driver.set_motion_table(DRUDGE_MOTIONS));
    assert_eq!(body.command(walk), 0, "the walk is taken");
    let start = body.pose();
    body.advance(0.37);
    let pose = body.pose();

    // The client's: a motion driver set up and stepped as the world steps an object.
    let mut client = MotionDriver::new(Arc::clone(&assets));
    assert!(client.set_setup(assets.setup(DRUDGE).expect("the setup")));
    assert!(client.set_motion_table(DRUDGE_MOTIONS));
    assert_eq!(
        client.do_interpreted_motion(walk, &MovementParameters::default()),
        0
    );
    let (mut left, mut clock) = (0.37f64, 0.0f64);
    while left > 1e-9 {
        let q = left.min(QUANTUM);
        clock += q;
        step_animation(&mut client, q, LocalTime(clock));
        left -= q;
    }
    client.update_parts(&frame);
    assert_eq!(pose.len(), client.part_array.parts.len());
    for (k, (got, p)) in pose.iter().zip(&client.part_array.parts).enumerate() {
        let want = part_affine(&p.pos, p.gfxobj_scale);
        assert!(
            got.abs_diff_eq(want, 1e-5),
            "part {k}: {got:?} against {want:?}"
        );
    }
    let moved = start
        .iter()
        .zip(&pose)
        .filter(|(a, b)| a.translation.distance(b.translation) > 0.01)
        .count();
    assert!(moved > 3, "only {moved} parts moved");
}
