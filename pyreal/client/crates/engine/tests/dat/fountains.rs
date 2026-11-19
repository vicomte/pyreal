//! Fountains and basins: retail's basin water and jets, told apart from what is not theirs.

use std::sync::Arc;

use pyreal_animation::data::AnimAssets;
use pyreal_assets::{Decode, Surface};
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_engine::asset::material::{describe_surface, flags, is_pool, KindTable, PackMaps};
use pyreal_engine::fountains::{is_jet, is_spray, pool_faces, spills};
use pyreal_engine::scene::PipelineKind;
use pyreal_engine::world::pipeline_for;
use pyreal_primitives::DataId;
use pyreal_world_data::anim_assets::DatAnimAssets;

fn decode<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

/// The academies' fountain (0x0200027B): its basin's water is one level face at 0.40 m drawn as
/// water, its jet and spray are the fountain's, and the two-tier fountain's spill streams are jets.
#[test]
fn the_academy_fountain_has_a_basin_of_water_and_a_jet() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let faces = pool_faces(&store, DataId(0x0200_027B));
    assert_eq!(faces.len(), 1, "{faces:?}");
    assert!((faces[0].z - 0.40).abs() < 0.01);
    assert!(faces[0].max.x - faces[0].min.x > 1.0);

    let water: Surface = decode(&store, DbType::Surface, DataId(0x0800_01A1)).expect("surface");
    assert!(is_pool(&water));
    let d = describe_surface(
        &water,
        false,
        None,
        &KindTable::default(),
        PackMaps::default(),
    );
    assert_ne!(d.flags & flags::POOL, 0);
    assert_eq!(
        pipeline_for(d.flags, pyreal_engine::asset::mesh::Sides::One),
        PipelineKind::Pool
    );
    // The fountain's wood is no water.
    let wood: Surface = decode(&store, DbType::Surface, DataId(0x0800_0102)).expect("surface");
    assert!(!is_pool(&wood));

    let assets = DatAnimAssets::new(Arc::new(store));
    let jet = assets.emitter_info(DataId(0x3200_0227)).expect("jet");
    assert!(is_jet(&jet));
    let spray = assets.emitter_info(DataId(0x3200_000B)).expect("spray");
    assert!(!is_jet(&spray) && is_spray(&spray));
    for lip in [0x3200_060E, 0x3200_060F] {
        assert!(
            is_jet(&assets.emitter_info(DataId(lip)).expect("lip")),
            "{lip:#X}"
        );
    }
    // A waterfall's sheet is not a fountain's.
    assert!(!is_jet(
        &assets.emitter_info(DataId(0x3200_04A4)).expect("sheet")
    ));
}

/// The arrival hall's wall fountain (0x02000115) runs its water down upright faces into its basin,
/// its tub (0x0200016C) and the horned fountain (0x0200123A) hold water retail draws opaque: all
/// of it is drawn as water.
#[test]
fn a_wall_fountains_stream_and_a_tubs_water_are_water() {
    let store = pyreal_dat::testing::open_store_or_fail();
    for id in [0x0800_019F, 0x0800_0262, 0x0800_083D] {
        let s: Surface = decode(&store, DbType::Surface, DataId(id)).expect("surface");
        assert!(is_pool(&s), "{id:#X}");
        let d = describe_surface(&s, false, None, &KindTable::default(), PackMaps::default());
        assert_ne!(d.flags & flags::POOL, 0, "{id:#X}");
    }
    let tub = pool_faces(&store, DataId(0x0200_016C));
    assert_eq!(tub.len(), 1, "{tub:?}");
    let basin = pool_faces(&store, DataId(0x0200_0115));
    assert!(basin.iter().any(|f| (f.z - 0.40).abs() < 0.01), "{basin:?}");
    // Its stream's three faces ring the basin as one, at their foot at the back.
    let feet = spills(&store, DataId(0x0200_0115), &basin);
    assert_eq!(feet.len(), 1, "{feet:?}");
    assert!(
        feet.iter()
            .all(|s| (s.at.z - 0.40).abs() < 0.01 && s.at.x < -0.2),
        "{feet:?}"
    );
    assert!(spills(&store, DataId(0x0200_016C), &tub).is_empty());
}
