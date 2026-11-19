use glam::Affine3A;
use pyreal_engine::lamps::{examine, fixture, place, LampKind};
use pyreal_primitives::DataId;

/// Dungeon 0x7F03's steam vent 0x02000AB4 and floor lights 0x02000801 (blue) and 0x02000815
/// (magenta) light their rooms at retail's colour and intensity 100: placed in a room each is a
/// mood light, as bright as a warm light of the same intensity and less paled than a portal's.
/// Outdoors the same light stays capped as a portal's or spell's.
#[test]
fn a_dungeons_coloured_lights_keep_retails_colour_in_a_room() {
    let store = pyreal_dat::testing::open_store_or_fail();
    for id in [0x0200_0AB4, 0x0200_0801, 0x0200_0815] {
        let sources = examine(&store, DataId(id));
        let s = sources
            .iter()
            .find(|s| s.kind == LampKind::Authored)
            .unwrap_or_else(|| panic!("{id:#010X} has an authored light"));
        assert!(s.intensity >= 100.0, "{id:#010X}: {}", s.intensity);
        let room = place(&fixture(*s, true), &Affine3A::IDENTITY, 1, 0);
        let outside = place(&fixture(*s, false), &Affine3A::IDENTITY, 0, 0);
        let warm = place(
            &pyreal_engine::lamps::LampSource {
                colour: glam::Vec3::new(1.0, 0.6, 0.3),
                ..*s
            },
            &Affine3A::IDENTITY,
            1,
            0,
        );
        assert_eq!(room.kind, LampKind::Mood, "{id:#010X}");
        assert_eq!(outside.kind, LampKind::Authored, "{id:#010X}");
        assert!(
            (room.candela - warm.candela).abs() < 1e-3 * warm.candela,
            "{id:#010X}"
        );
        assert!(outside.candela * 10.0 < room.candela, "{id:#010X}");
        let spread = |c: glam::Vec3| (c.max_element() - c.min_element()) / c.max_element();
        assert!(
            spread(room.colour) > spread(outside.colour) + 0.2,
            "{id:#010X}: {} vs {}",
            room.colour,
            outside.colour
        );
    }
}
