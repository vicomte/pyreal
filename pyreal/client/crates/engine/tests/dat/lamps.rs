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

/// A room open to the sky (the swamp hut 0xBB630100, seen from outdoors at noon) lights its lamps at
/// the outdoors' own scale: lit to retail's clamp (its green light of intensity 100) it keeps
/// retail's ratio to a sunlit face beside it, in every day group at every hour, where the sky's
/// scale lit it about twice that. Its fill keeps the sky's.
#[test]
fn a_room_open_to_the_sky_keeps_retails_ratio_to_a_sunlit_face() {
    use pyreal_engine::renderer::{indoor_physical, room_lamp_scale, ROOM_LUX};
    use pyreal_engine::sky::{sky_lux, SkyLight, SCENE_LUX};
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let groups = region.sky_info.as_ref().map_or(0, |s| s.day_groups.len());
    assert!(groups > 9, "the region has its day groups");
    let luma = |c: glam::Vec3| c.dot(glam::Vec3::new(0.2126, 0.7152, 0.0722));
    let linear = |c: glam::Vec3| c.min(glam::Vec3::ONE).powf(2.2);
    for group in 0..groups {
        for k in 0..48 {
            let light = SkyLight::at(&region, group, k as f32 / 48.0);
            let whole = room_lamp_scale(&light, true);
            // A sunlit face as retail sums it, and as the outdoors lights it.
            let summed = luma(linear(light.ambient + light.sun));
            let sunlit = luma(light.sun_light + light.sky_light);
            assert!(whole > 0.0, "group {group} hour {k}");
            assert!(
                whole * summed <= sunlit * 1.001,
                "group {group} hour {k}: {whole} times {summed} over {sunlit}"
            );
            assert!(whole <= sky_lux(light.daylight) / SCENE_LUX * 1.001);
            assert!((room_lamp_scale(&light, false) - ROOM_LUX / SCENE_LUX).abs() < 1e-6);
        }
    }
    let hut = SkyLight::at(&region, 9, 0.515);
    let sky = sky_lux(hut.daylight) / SCENE_LUX;
    let whole = room_lamp_scale(&hut, true);
    assert!(whole < sky * 0.6, "{whole} of {sky}");
    // The room's fill stays over the open sky's light, as retail's does.
    assert!(luma(indoor_physical(&hut, true, true)) > luma(hut.sky_light));
}

/// An outdoor lamp by day sits on the sunlit ground as retail's did: retail sums a lamp with the
/// sun and sky and clamps, so a sunlit face takes little more and a shaded one is lifted toward
/// it. By night the brightest lamp keeps its light. Every day group at every hour; the brightest
/// outdoor lamp (`outdoor_lamp` and `outdoor_lamp_scale` in lamp_types.wgsl) a metre and two off.
#[test]
fn an_outdoor_lamp_by_day_keeps_retails_clamp_and_its_night_light() {
    use pyreal_engine::renderer::{lamp_ceiling, lamps_held, DAY_GLOW};
    use pyreal_engine::sky::{sky_lux, SkyLight, SCENE_LUX};
    let lamp_types = include_str!("../../src/shaders/lamp_types.wgsl");
    assert!(lamp_types.contains("const OUTDOOR_GAIN: f32 = 12.0;"));
    assert!(lamp_types.contains("const OUTDOOR_CORE: f32 = 0.6;"));
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let groups = region.sky_info.as_ref().map_or(0, |s| s.day_groups.len());
    assert!(groups > 9, "the region has its day groups");
    let luma = |c: glam::Vec3| c.dot(glam::Vec3::new(0.2126, 0.7152, 0.0722));
    let (mut nights, mut noons) = (0, 0);
    for group in 0..groups {
        for k in 0..48 {
            let light = SkyLight::at(&region, group, k as f32 / 48.0);
            let ceiling = lamp_ceiling(&light, light.sun_light, light.sky_light);
            let scale = 12.0 * sky_lux(light.daylight) / SCENE_LUX;
            let (near, off) = (scale / (1.0 + 0.36), scale / (4.0 + 0.36));
            let sunlit = luma(light.sun_light + light.sky_light);
            let shade = luma(light.sky_light);
            let at = format!("group {group} hour {k}");
            for lamp in [near, off] {
                for own in [shade, sunlit] {
                    let held = lamps_held(lamp, own, ceiling);
                    assert!(
                        held <= lamp && own + held <= own.max(ceiling) * 1.001,
                        "{at}"
                    );
                }
            }
            if light.daylight < 0.05 {
                nights += 1;
                assert!(
                    lamps_held(near, sunlit, ceiling) >= near * 0.99,
                    "{at}: {near}"
                );
            }
            if light.daylight > 0.9999 {
                noons += 1;
                // Retail's white, a sunlit face's light over retail's clamped sum of it.
                let summed = luma((light.ambient + light.sun).min(glam::Vec3::ONE).powf(2.2));
                assert!(
                    ceiling <= sunlit / summed * DAY_GLOW * 1.001,
                    "{at}: {ceiling}"
                );
                // A lamp two metres off lit the sunlit ground past twice over; now it keeps under
                // a fifth of that, and lifts the shade a metre off most of the way to the sun.
                assert!(off > sunlit, "{at}: {off} on {sunlit}");
                assert!(
                    lamps_held(off, sunlit, ceiling) < off * 0.2,
                    "{at}: {off} on {sunlit}"
                );
                let lifted = shade + lamps_held(near, shade, ceiling);
                assert!(lifted >= sunlit * 0.8, "{at}: {near} on {shade}");
            }
        }
    }
    assert!(nights > 0 && noons > 0, "{nights} nights, {noons} noons");
}
