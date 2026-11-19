//! The selection's target ring lies on the ground round its object's feet, never on another
//! body or a thing standing in it.

use std::sync::Arc;

use glam::{Affine3A, Vec3};
use pyreal_engine::body::Body;
use pyreal_engine::renderer::{Exposure, Selection};
use pyreal_engine::world::{ground_height, load_area, Area, AssetLoader};
use pyreal_engine::{Camera, SkyLight};
use pyreal_primitives::{DataId, Frame, Quat, Vec3 as DVec3};
use pyreal_world_data::anim_assets::DatAnimAssets;

/// A drudge, a body that moves as a server's creature does.
const DRUDGE: DataId = DataId(0x0200_07DD);
/// A Holtburg door: a static object, upright.
const DOOR: DataId = DataId(0x0200_19FF);

/// Two drudges side by side on Holtburg's ground with a door beside them, the left one selected
/// with a ring wide enough to reach both: the ring tints the ground round its feet, and the other
/// drudge's pixels and the door's face are as they were with no ring.
#[test]
fn the_selection_ring_lies_on_the_ground_and_not_on_what_stands_in_it() {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = super::engine();
    engine.renderer.exposure = Exposure::Manual(1.0);
    let mut loader = AssetLoader::default();
    load_area(
        &mut engine,
        &mut loader,
        &store,
        &region,
        Area {
            center: 0xA9B4,
            radius: 1,
        },
    );
    // Render space is the middle block's own.
    let ground = |x: f32, y: f32| {
        Vec3::new(
            x,
            y,
            ground_height(&store, &region, 0xA9B4, x, y).expect("the ground"),
        )
    };
    let middle = ground(96.0, 90.0);
    let stand = |at: Vec3| {
        Body::new(
            Arc::new(DatAnimAssets::new(Arc::clone(&store))),
            DRUDGE,
            None,
            Frame::new(DVec3::new(at.x, at.y, at.z), Quat::IDENTITY),
        )
        .expect("a drudge")
    };
    let mut selected = stand(middle);
    let mut other = stand(ground(96.9, 90.0));
    assert!(selected.place(&mut engine, &mut loader, &store) > 5);
    assert!(other.place(&mut engine, &mut loader, &store) > 5);
    let door = loader.object_mark();
    let door_at = ground(95.0, 90.2);
    assert!(
        loader.place_object(
            &mut engine,
            &store,
            DOOR,
            Affine3A::from_translation(door_at),
            false
        ) > 0
    );
    let camera = Camera {
        position: middle + Vec3::new(0.0, -5.0, 2.4),
        pitch: -0.4,
        far: 300.0,
        ..Camera::default()
    };
    let light = SkyLight::at(&region, 0, 0.5);
    let size = (240, 160);
    let mut frame = |ring: Option<(Vec3, f32)>| {
        engine.renderer.selection = Selection {
            selected: Some(selected.object),
            hovered: None,
            ring,
            colour: 0,
        };
        let mut rgba = Vec::new();
        for _ in 0..3 {
            rgba = engine
                .render_to_rgba(&camera, &light, size)
                .expect("a frame");
        }
        let objects: Vec<u32> = pyreal_engine::read_texture(
            &engine.gpu,
            engine
                .renderer
                .objects_texture()
                .expect("the objects target"),
            4,
        )
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u32::from_le_bytes(*b))
        .collect();
        (rgba, objects)
    };
    let (bare, objects) = frame(None);
    let (ringed, _) = frame(Some((middle, 1.0)));
    let changed = |i: usize| {
        (0..3)
            .map(|c| bare[i * 4 + c].abs_diff(ringed[i * 4 + c]))
            .max()
            .unwrap_or(0)
    };
    let pixels = |id: u32| -> Vec<usize> {
        (0..objects.len())
            .filter(|&i| objects[i] == id + 1)
            .collect()
    };
    let ground_tinted = (0..objects.len())
        .filter(|&i| objects[i] == 0 && changed(i) > 24)
        .count();
    let other_px = pixels(other.object);
    let door_px = pixels(door);
    let other_tinted = other_px.iter().filter(|&&i| changed(i) > 6).count();
    let door_tinted = door_px.iter().filter(|&&i| changed(i) > 6).count();
    eprintln!(
        "ground tinted {ground_tinted}; other drudge {other_tinted} of {}; door {door_tinted} of {}",
        other_px.len(),
        door_px.len()
    );
    assert!(ground_tinted > 40, "the ring is not on the ground");
    assert!(
        other_px.len() > 100 && door_px.len() > 100,
        "nothing stands in the ring"
    );
    assert_eq!(other_tinted, 0, "the ring climbs the other drudge");
    // At most the door's bottom edge, where it meets the ringed ground.
    assert!(
        door_tinted * 100 < door_px.len(),
        "the ring climbs the door"
    );
}
