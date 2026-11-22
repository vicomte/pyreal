//! The portal walk and the room volumes, on hand-made rooms.

use glam::{Vec3, Vec4};
use pyreal_engine::camera::Camera;
use pyreal_engine::cells::{is_empty, Cell, CellGraph, Portal, Volume, FULL, OUTSIDE};

/// A box room `x0..x1` by `y0..y1`, three metres high, its planes facing in.
fn room(id: u32, (x0, x1): (f32, f32), (y0, y1): (f32, f32), portals: Vec<Portal>) -> Cell {
    Cell {
        id,
        portals,
        volume: Some(Volume {
            planes: vec![
                Vec4::new(1.0, 0.0, 0.0, -x0),
                Vec4::new(-1.0, 0.0, 0.0, x1),
                Vec4::new(0.0, 1.0, 0.0, -y0),
                Vec4::new(0.0, -1.0, 0.0, y1),
                Vec4::new(0.0, 0.0, 1.0, 0.0),
                Vec4::new(0.0, 0.0, -1.0, 3.0),
            ],
            min: Vec3::new(x0, y0, 0.0),
            max: Vec3::new(x1, y1, 3.0),
            center: Vec3::new((x0 + x1) * 0.5, (y0 + y1) * 0.5, 1.5),
        }),
        seen_outside: true,
        reach: f32::INFINITY,
    }
}

/// A doorway across `y`, from x 1 to 3, facing the room around `inside`.
fn door(y: f32, inside: Vec3, to: Option<u32>) -> Portal {
    Portal::new(
        vec![
            Vec3::new(1.0, y, 0.0),
            Vec3::new(3.0, y, 0.0),
            Vec3::new(3.0, y, 2.5),
            Vec3::new(1.0, y, 2.5),
        ],
        inside,
        to,
    )
}

/// Room A (y 0 to 4) opens north into room B (y 4 to 8), which opens north onto the outdoors.
fn house() -> CellGraph {
    let a = Vec3::new(2.0, 2.0, 1.5);
    let b = Vec3::new(2.0, 6.0, 1.5);
    let mut g = CellGraph::default();
    g.add(room(
        0x0100,
        (0.0, 4.0),
        (0.0, 4.0),
        vec![door(4.0, a, Some(0x0101))],
    ));
    g.add(room(
        0x0101,
        (0.0, 4.0),
        (4.0, 8.0),
        vec![door(4.0, b, Some(0x0100)), door(8.0, b, None)],
    ));
    g.link();
    g
}

fn eye(at: Vec3, yaw: f32) -> (glam::Mat4, Vec3) {
    let c = Camera {
        position: at,
        yaw,
        ..Camera::default()
    };
    (c.view_projection(16.0 / 9.0), at)
}

/// A portal running behind the eye still opens to the screen's edge: its corners behind are
/// clipped at the eye, not dropped.
#[test]
fn a_portal_passing_behind_the_eye_opens_to_the_screen_edge() {
    let c = Camera {
        position: Vec3::new(14.2, -21.889, 1.559),
        yaw: -5.914,
        fov_y: 58.7f32.to_radians(),
        ..Camera::default()
    };
    let vp = c.view_projection(16.0 / 9.0);
    let p = Portal::new(
        vec![
            Vec3::new(15.0, -18.333, 0.0),
            Vec3::new(15.0, -21.667, 0.0),
            Vec3::new(15.0, -21.667, 3.0),
            Vec3::new(15.0, -18.333, 3.0),
        ],
        Vec3::new(14.0, -20.0, 1.0),
        None,
    );
    let r = p.screen_rect(&vp);
    assert!(r[0] > 0.5 && r[2] == 1.0, "{r:?}");
    assert_eq!((r[1], r[3]), (-1.0, 1.0));
}

/// From inside A looking through the door, B and the outdoors beyond it are seen, each through
/// a part of the screen; looking away, only A is.
#[test]
fn the_walk_sees_through_doors_and_not_through_walls() {
    let g = house();
    assert_eq!(g.cell_at(Vec3::new(2.0, 1.0, 1.5)), Some(1));
    assert_eq!(g.cell_at(Vec3::new(2.0, 5.0, 1.5)), Some(2));
    assert_eq!(g.cell_at(Vec3::new(2.0, 9.0, 1.5)), None);

    let (vp, at) = eye(Vec3::new(2.0, 1.0, 1.5), 0.0);
    let v = g.visibility(&vp, at, None, 100.0, 1.0);
    assert_eq!(v.eye_cell, Some(1));
    assert_eq!(v.rects[1], FULL);
    assert!(!is_empty(&v.rects[2]) && v.rects[2] != FULL, "{v:?}");
    assert!(v.outside());
    // The outdoors is seen through the far door, inside the near door's rectangle.
    let (o, b) = (v.rects[OUTSIDE as usize], v.rects[2]);
    assert!(
        o[0] >= b[0] && o[2] <= b[2] && o[2] - o[0] < b[2] - b[0],
        "{o:?} in {b:?}"
    );

    let (vp, at) = eye(Vec3::new(2.0, 1.0, 1.5), std::f32::consts::PI);
    let v = g.visibility(&vp, at, None, 100.0, 1.0);
    assert!(is_empty(&v.rects[2]) && !v.outside(), "{v:?}");
}

/// From outdoors the rooms are entered through their outdoor doors, when those face the eye.
#[test]
fn the_outdoors_sees_into_rooms_through_their_doors() {
    let g = house();
    // North of the house, looking south at its door.
    let (vp, at) = eye(Vec3::new(2.0, 14.0, 1.5), std::f32::consts::PI);
    let v = g.visibility(&vp, at, None, 100.0, 1.0);
    assert_eq!(v.eye_cell, None);
    assert_eq!(v.rects[0], FULL);
    assert!(!is_empty(&v.rects[2]) && !is_empty(&v.rects[1]), "{v:?}");
    // Looking north, away from it; and south of the house, where its door faces away.
    let (vp, at) = eye(Vec3::new(2.0, 14.0, 1.5), 0.0);
    assert_eq!(g.visibility(&vp, at, None, 100.0, 1.0).cells_seen(), 0);
    let (vp, at) = eye(Vec3::new(2.0, -6.0, 1.5), 0.0);
    assert_eq!(g.visibility(&vp, at, None, 100.0, 1.0).cells_seen(), 0);
    // Too far away to look into, or past where the building's shell is drawn.
    let (vp, at) = eye(Vec3::new(2.0, 14.0, 1.5), std::f32::consts::PI);
    assert_eq!(g.visibility(&vp, at, None, 3.0, 1.0).cells_seen(), 0);
    let mut short = g.clone();
    short.limit_reach(0x0101, 5.0);
    assert_eq!(short.visibility(&vp, at, None, 100.0, 1.0).cells_seen(), 0);
    assert_eq!(short.visibility(&vp, at, None, 100.0, 0.5).cells_seen(), 2);
}

/// Where a room's walk reaches the outdoors, it goes on into the rooms of the building across the
/// way through their outdoor doors.
#[test]
fn rooms_see_into_the_next_building_through_the_outdoors() {
    let mut g = house();
    let c = Vec3::new(2.0, 12.0, 1.5);
    g.add(room(
        0x0102,
        (0.0, 4.0),
        (10.0, 14.0),
        vec![door(10.0, c, None)],
    ));
    g.link();
    let (vp, at) = eye(Vec3::new(2.0, 1.0, 1.5), 0.0);
    let v = g.visibility(&vp, at, None, 100.0, 1.0);
    assert!(v.outside() && !is_empty(&v.rects[3]), "{v:?}");
    let (vp, at) = eye(Vec3::new(2.0, 1.0, 1.5), std::f32::consts::PI);
    assert!(is_empty(&g.visibility(&vp, at, None, 100.0, 1.0).rects[3]));
}

/// Furniture in a room the walk does not reach is still seen where it reaches into a room the walk
/// does (stairs climbing into the next room); furniture within its room's walls is not.
#[test]
fn furniture_reaching_into_a_seen_room_is_seen_through_it() {
    // The house, with room C (x 4 to 8, y 4 to 8) east of B through a doorway at x 4, y 5 to 7.
    let (a, b, c) = (
        Vec3::new(2.0, 2.0, 1.5),
        Vec3::new(2.0, 6.0, 1.5),
        Vec3::new(6.0, 6.0, 1.5),
    );
    let side = |inside: Vec3, to: u32| {
        Portal::new(
            vec![
                Vec3::new(4.0, 5.0, 0.0),
                Vec3::new(4.0, 7.0, 0.0),
                Vec3::new(4.0, 7.0, 2.5),
                Vec3::new(4.0, 5.0, 2.5),
            ],
            inside,
            Some(to),
        )
    };
    let mut g = CellGraph::default();
    g.add(room(
        0x0100,
        (0.0, 4.0),
        (0.0, 4.0),
        vec![door(4.0, a, Some(0x0101))],
    ));
    g.add(room(
        0x0101,
        (0.0, 4.0),
        (4.0, 8.0),
        vec![
            door(4.0, b, Some(0x0100)),
            door(8.0, b, None),
            side(b, 0x0102),
        ],
    ));
    let home = g.add(room(0x0102, (4.0, 8.0), (4.0, 8.0), vec![side(c, 0x0101)]));
    let inside = g.furniture_slot(home, Vec3::new(6.0, 6.0, 1.0), 0.5);
    let reaching = g.furniture_slot(home, Vec3::new(5.0, 6.0, 1.0), 1.5);
    assert_eq!(inside, home);
    assert_ne!(reaching, home);
    g.link();
    // From A looking north, B is seen through its door and C's doorway lies outside it.
    let (vp, at) = eye(Vec3::new(2.0, 1.0, 1.5), 0.0);
    let v = g.visibility(&vp, at, None, 100.0, 1.0);
    assert!(
        !is_empty(&v.rects[2]) && is_empty(&v.rects[home as usize]),
        "{v:?}"
    );
    assert_eq!(v.rects[reaching as usize], v.rects[2]);
    assert_eq!(g.rooms_seen(&v), 2);
    // Looking south, neither.
    let (vp, at) = eye(Vec3::new(2.0, 1.0, 1.5), std::f32::consts::PI);
    assert!(is_empty(
        &g.visibility(&vp, at, None, 100.0, 1.0).rects[reaching as usize]
    ));
}

/// A report's cell wins over the volumes: an outdoor cell id means outdoors, a room's means that
/// room.
#[test]
fn a_named_cell_places_the_eye() {
    let g = house();
    let at = Vec3::new(2.0, 1.0, 1.5);
    assert_eq!(g.eye_cell(at, None), Some(1));
    assert_eq!(g.eye_cell(at, Some(0xA9B4_0031)), None);
    assert_eq!(g.eye_cell(at, Some(0x0101)), Some(2));
}

/// The cut buffer's words: the box with the plane count's bits, then the planes.
#[test]
fn a_volume_writes_its_box_then_its_planes() {
    let g = house();
    let v = g.cell(1).and_then(|c| c.volume.clone()).expect("a volume");
    let mut words = Vec::new();
    v.write(f32::INFINITY, &mut words);
    assert_eq!(words.len(), 2 + 6);
    assert_eq!(words[0][3].to_bits(), 6);
    assert_eq!(words[1], [4.0, 4.0, 3.0, f32::MAX]);
    assert!(v.contains(Vec3::new(1.0, 1.0, 0.0), 0.0));
    assert!(!v.contains(Vec3::new(1.0, 1.0, -0.1), 0.05));
}

/// A lamp lights its own room and the rooms its light reaches through doorways, never one beyond
/// a wall, however near; one whose light reaches the outdoors lights everywhere.
#[test]
fn a_lamps_light_goes_through_doorways_and_not_walls() {
    let mut g = house();
    // Room C, east of A beyond a wall, with no way through.
    let c = g.add(room(0x0102, (4.5, 8.5), (0.0, 4.0), Vec::new()));
    g.link();
    let (a, b) = (g.slot_of(0x0100).unwrap(), g.slot_of(0x0101).unwrap());
    let rooms = |slot, at, range| g.lamp_rooms(slot, at, range, 24);
    // By A's door: A and B.
    assert_eq!(rooms(a, Vec3::new(2.0, 3.0, 2.0), 3.0), Some(vec![a, b]));
    // At the far end of A, out of reach of its door: A alone.
    assert_eq!(rooms(a, Vec3::new(2.0, 0.5, 2.0), 3.0), Some(vec![a]));
    // In C, half a metre beyond A's east wall: C alone.
    assert_eq!(rooms(c, Vec3::new(5.0, 2.0, 2.0), 6.0), Some(vec![c]));
    // In B by its door to the outdoors: everywhere.
    assert_eq!(rooms(b, Vec3::new(2.0, 7.0, 2.0), 3.0), None);
    // Outdoors: everywhere.
    assert_eq!(rooms(OUTSIDE, Vec3::ZERO, 3.0), None);
}
