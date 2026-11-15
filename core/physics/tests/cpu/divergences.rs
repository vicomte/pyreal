//! Places the transcription once disagreed with ACE's port (`Sphere.IntersectsSphere`,
//! `ObjectInfo.ValidateWalkable`). Each test asserts what ACE
//! does; the transcription follows it now.

use std::sync::Arc;

use pyreal_physics::geom::{Plane, Sphere};
use pyreal_physics::transition::collide;
use pyreal_physics::transition::objectinfo::ObjectInfoState;
use pyreal_physics::{
    globals, PhysHandle, PhysicsWorld, SetupGeometry, StaticLandSource, Transition,
    TransitionState, V3,
};
use pyreal_primitives::{CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};

const BLOCK: LandblockId = LandblockId(0xA9B4);
const MOVER_R: f32 = 0.5;

fn cell_at(p: Vec3) -> CellId {
    let mut c = BLOCK.cell(1);
    let mut o = p;
    assert!(pyreal_physics::landdefs::adjust_to_outside(&mut c, &mut o));
    c
}

/// Flat ground at z = 20 round Holtburg's block.
fn flat_world() -> PhysicsWorld {
    let mut land = StaticLandSource::linear();
    for x in 0xA8..=0xAA_u8 {
        for y in 0xB3..=0xB5_u8 {
            land.add_flat_block(LandblockId::new(x, y), 10);
        }
    }
    PhysicsWorld::new(Arc::new(land))
}

fn body_geometry() -> Arc<SetupGeometry> {
    Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, MOVER_R), MOVER_R)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, MOVER_R), 1.0),
        step_up_height: 0.3,
        step_down_height: 0.3,
        radius: MOVER_R,
        height: 1.0,
        ..SetupGeometry::default()
    })
}

/// A static object that is one sphere of radius `r` resting on the ground.
fn dome_geometry(r: f32) -> Arc<SetupGeometry> {
    Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, r), r)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, r), r * 2.0),
        radius: r,
        height: r * 2.0,
        ..SetupGeometry::default()
    })
}

fn spawn(w: &mut PhysicsWorld, at: Vec3) -> PhysHandle {
    let h = w.create(ObjectId(1), body_geometry(), true);
    let cell = cell_at(at);
    w.enter_cell(h, cell);
    if let Some(o) = w.get_mut(h) {
        o.position = Position::new(cell, Frame::new(at, Quat::IDENTITY));
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, false);
    h
}

fn register_static(w: &mut PhysicsWorld, at: Vec3, g: Arc<SetupGeometry>) {
    let h = w.create(ObjectId(9), g, false);
    let cell = cell_at(at);
    w.enter_cell(h, cell);
    let frame = Frame::new(at, Quat::IDENTITY);
    if let Some(o) = w.get_mut(h) {
        o.set_frame(frame);
        o.position = Position::new(cell, frame);
    }
    w.calc_cross_cells(h, true);
}

/// A mover's sphere going from origin `curr` to origin `check`, airborne.
fn mover_transition(mover: PhysHandle, curr: Vec3, check: Vec3) -> Transition {
    let mut t = Transition::default();
    t.sphere_path
        .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, MOVER_R), MOVER_R)], 1.0);
    let from = Position::new(cell_at(curr), Frame::new(curr, Quat::IDENTITY));
    let to = Position::new(cell_at(check), Frame::new(check, Quat::IDENTITY));
    t.sphere_path.init_path(Some(from.cell), Some(from), &to);
    t.sphere_path.set_check_pos(&to, Some(to.cell));
    t.object_info.object = Some(mover);
    t.object_info.step_down = true;
    t.object_info.step_up_height = 0.3;
    t.object_info.step_down_height = 0.3;
    t.sphere_path.walk_interp = 1.0;
    t
}

/// D1a. ACE runs the swept landing (tangent contact plane, `walk_interp`, `Adjusted`) when
/// `SpherePath.Collide` is set, the re-test after `LandOnSphere`. Here the swept landing sits
/// under `check_walkable` (`collide.rs` `sphere_intersects_sphere`) and nothing reads `collide`,
/// so the re-test lands on the sphere again instead. The cylinder dispatcher has ACE's layout.
#[test]
fn d1a_a_collide_armed_retest_lands_the_sphere_on_the_obstacle() {
    let mut w = flat_world();
    let dome_at = Vec3::new(100.0, 100.0, 20.0);
    register_static(&mut w, dome_at, dome_geometry(3.0));
    let curr = Vec3::new(100.0, 100.0, 26.3);
    let check = Vec3::new(100.0, 100.0, 25.9);
    let mover = spawn(&mut w, curr);
    let obstacle = Sphere::new(dome_at.add(Vec3::new(0.0, 0.0, 3.0)), 3.0);
    let mut t = mover_transition(mover, curr, check);
    t.sphere_path.set_collide(Vec3::new(0.0, 0.0, 1.0));
    t.sphere_path.walkable_allowance = globals::LANDING_Z;
    let ctx = w.transition_ctx(Some(mover));
    let r = collide::sphere_intersects_sphere(&ctx, &mut t, &obstacle, false);
    let z = t.sphere_path.check_pos.frame.origin.z;
    assert!(
        r == TransitionState::Adjusted
            && t.collision_info.contact_plane_valid
            && (z - 26.0).abs() < 1e-3
            && (t.sphere_path.walk_interp - 0.75).abs() < 1e-3,
        "ACE: Adjusted, plane (0,0,1), check z 26.0, walk_interp 0.75; got {r:?}, plane {}, z {z}, \
         walk_interp {}",
        t.collision_info.contact_plane_valid,
        t.sphere_path.walk_interp
    );
}

/// D1b. ACE answers a `check_walkable` probe that overlaps either sphere with `Collided` and writes
/// nothing. Here it runs the swept landing: `Ok` on a steep tangent, and on a shallow one it moves
/// the check position and overwrites the contact plane, which `walk::check_walkable` does not
/// restore.
#[test]
fn d1b_a_check_walkable_probe_that_overlaps_a_sphere_is_collided() {
    let mut w = flat_world();
    let dome_at = Vec3::new(100.0, 100.0, 20.0);
    register_static(&mut w, dome_at, dome_geometry(3.0));
    let c = dome_at.add(Vec3::new(0.0, 0.0, 3.0));
    let obstacle = Sphere::new(c, 3.0);
    let probe = |w: &PhysicsWorld, mover, check: Vec3, on_walkable: bool| {
        let curr = check.add(Vec3::new(0.0, 0.0, 0.3));
        let mut t = mover_transition(mover, curr, check);
        t.object_info.set(ObjectInfoState::CONTACT, true);
        t.object_info.set(ObjectInfoState::ON_WALKABLE, on_walkable);
        t.sphere_path.check_walkable = true;
        t.sphere_path.walkable_allowance = globals::FLOOR_Z;
        let ctx = w.transition_ctx(Some(mover));
        let r = collide::sphere_intersects_sphere(&ctx, &mut t, &obstacle, false);
        (r, t.collision_info.contact_plane_valid)
    };
    let steep = c.add(Vec3::new(3.45, 0.0, 0.3 - MOVER_R));
    let mover = spawn(&mut w, steep.add(Vec3::new(0.0, 0.0, 0.3)));
    let shallow = c.add(Vec3::new(0.2, 0.0, 3.3 - MOVER_R));
    let got = (
        probe(&w, mover, steep, true),
        probe(&w, mover, shallow, false),
    );
    assert_eq!(
        got,
        (
            (TransitionState::Collided, false),
            (TransitionState::Collided, false)
        ),
        "ACE: Collided both times, no plane written"
    );
}

/// D1c. End to end: a body dropped 0.4 m off the axis of a static one-sphere object. Under ACE it
/// lands on the tangent plane through the collide re-test. Here it falls and stalls until the
/// frames-stationary-fall counter gives it the synthetic plane (stuck bit set by about frame 25).
#[test]
fn d1c_a_body_dropped_on_a_static_sphere_lands_without_the_stuck_plane() {
    let mut w = flat_world();
    let dome_at = Vec3::new(100.0, 100.0, 20.0);
    register_static(&mut w, dome_at, dome_geometry(3.0));
    let h = spawn(&mut w, Vec3::new(100.4, 100.0, 27.0));
    w.set_player(h);
    let mut now = 0.0_f64;
    for _ in 0..45 {
        now += 1.0 / 30.0;
        w.use_time(LocalTime(now), false);
    }
    let o = w.get(h).expect("live");
    assert!(
        !o.transient_state.is_stationary_stuck(),
        "came to rest on the frames-stationary-fall plane at z {}, not by landing",
        o.position.frame.origin.z
    );
}

/// D2. ACE's `ValidateWalkable` returns `Adjusted` unconditionally once the sphere is below the
/// plane and not in a `check_walkable` probe. Here it returns `Ok` when it neither set a plane
/// nor lifted (on walkable, not stepping down, plane too steep), which lets a walking body be
/// slid sideways along a steep ramp's foot where ACE stops it.
#[test]
fn d2_a_sphere_sunk_into_steep_ground_while_walking_is_adjusted() {
    let at = Vec3::new(100.0, 100.0, 20.0);
    let mut t = Transition::default();
    t.sphere_path
        .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, MOVER_R), MOVER_R)], 1.0);
    let pos = Position::new(cell_at(at), Frame::new(at, Quat::IDENTITY));
    t.sphere_path.init_path(Some(pos.cell), Some(pos), &pos);
    t.sphere_path.set_check_pos(&pos, Some(pos.cell));
    t.object_info.set(ObjectInfoState::CONTACT, true);
    t.object_info.set(ObjectInfoState::ON_WALKABLE, true);
    let s = Sphere::new(at.add(Vec3::new(0.0, 0.0, MOVER_R)), MOVER_R);
    // A 60-degree slope with the sphere's low point 0.1 m inside it.
    let n = Vec3::new(0.866_025_4, 0.0, 0.5);
    let plane = Plane {
        normal: n,
        d: -(n.x * at.x + n.y * at.y + n.z * at.z) - 0.1,
    };
    let r = collide::validate_walkable(&mut t, &s, &plane, false, 0.0, pos.cell);
    assert_eq!(r, TransitionState::Adjusted, "ACE: Adjusted");
}
