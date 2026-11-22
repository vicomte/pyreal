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
                client_offset: None,
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

/// A gem golem's white sparkles (star 0x010010F9) are glints and light nothing: the ice golem
/// 0x020007D8 has no glow lamp. A pale blue star and a white ball are still lights, and a body
/// with coloured lights beside the same sparkles (0x02000A6A) keeps its glow.
#[test]
fn a_gem_golems_sparkles_are_glints_that_light_nothing() {
    use pyreal_engine::glow::glow_of;
    use pyreal_engine::lamps::glows;
    let store = pyreal_dat::testing::open_store_or_fail();
    let glint = |id: u32| {
        glow_of(&store, DataId(id))
            .expect("a light picture")
            .glint()
    };
    assert!(glint(0x0100_10F9));
    assert!(!glint(0x0100_108F), "a pale blue star");
    assert!(!glint(0x0100_1065), "a white ball");
    assert!(glows(&store, DataId(0x0200_07D8)).is_empty());
    assert!(!glows(&store, DataId(0x0200_0A6A)).is_empty());
}

/// A pack that makes the ice golem's pictures (0x050018D0, 0x050018D1) clear crystal.
struct ClearIce;

impl pyreal_engine::asset::material::MaterialOverrides for ClearIce {
    fn maps(
        &self,
        _surface: DataId,
        texture: Option<DataId>,
    ) -> pyreal_engine::asset::material::PackMaps {
        let mut maps = pyreal_engine::asset::material::PackMaps::default();
        if matches!(texture, Some(DataId(0x0500_18D0 | 0x0500_18D1))) {
            maps.matter.clear = Some(0.5);
        }
        maps
    }
}

/// The ice golem 0x020007D7 (the Diamond Golem undressed) is luminous half see-through ice in the
/// dats, which reads as a glowing body; where the pack makes that ice clear crystal it is lit,
/// not a light, and carries no glow lamp.
#[test]
fn a_body_of_clear_crystal_carries_no_glow() {
    use pyreal_engine::asset::material::NoOverrides;
    use pyreal_engine::world::AssetLoader;
    let store = pyreal_dat::testing::open_store_or_fail();
    let golem = DataId(0x0200_07D7);
    let lamps =
        |loader: &mut AssetLoader| loader.lamps_at(&store, golem, &Affine3A::IDENTITY, 0, 0, true);
    assert!(!lamps(&mut AssetLoader::new(Box::new(NoOverrides))).is_empty());
    assert!(lamps(&mut AssetLoader::new(Box::new(ClearIce))).is_empty());
}
