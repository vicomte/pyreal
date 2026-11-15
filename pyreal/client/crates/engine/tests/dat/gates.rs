//! Portals in the world: which setups are drawn as a vortex, and their colours, in each era.

use std::sync::Arc;

use pyreal_animation::data::{AnimAssets, ParticleType};
use pyreal_animation::hooks::HookKind;
use pyreal_animation::MotionDriver;
use pyreal_dat::RetailDatStore;
use pyreal_engine::gates::{is_portal, SCRIPTS};
use pyreal_engine::particles::sprite_of;
use pyreal_primitives::DataId;
use pyreal_world_data::anim_assets::DatAnimAssets;

/// The setups an era's dats hold of the portals', each naming the script it is told by.
fn held(store: RetailDatStore) -> Vec<u32> {
    let assets = DatAnimAssets::new(Arc::new(store));
    SCRIPTS
        .iter()
        .filter_map(|&(script, setup, _)| {
            let s = assets.setup(DataId(setup))?;
            assert_eq!(s.default_script, Some(DataId(script)), "{setup:#X}");
            Some(setup)
        })
        .collect()
}

/// Every portal setup names its listed script wherever it is: all of them at the end of retail,
/// the seven colours and the shadow in February 2005, the seven colours in 1999.
#[test]
fn the_portal_setups_are_told_by_their_scripts_in_every_era() {
    assert_eq!(
        held(pyreal_dat::testing::open_store_or_fail()).len(),
        SCRIPTS.len()
    );
    assert!(held(pyreal_dat::testing::open_classic_store_or_fail()).len() >= 8);
    assert!(held(pyreal_dat::testing::open_launch_store_or_fail()).len() >= 7);
}

/// A portal is one; Dark Vapor (whose whirl is the shadow portal's) and a broken portal are not.
#[test]
fn only_the_portals_are_drawn_as_a_vortex() {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let assets: Arc<dyn AnimAssets> = Arc::new(DatAnimAssets::new(store));
    let driver = |setup: u32| {
        let mut d = MotionDriver::new(Arc::clone(&assets));
        assert!(d.set_setup(assets.setup(DataId(setup)).expect("the setup")));
        d
    };
    assert!(is_portal(&driver(0x0200_01B3)));
    assert!(is_portal(&driver(0x0200_08FD)));
    assert!(!is_portal(&driver(0x0200_0FEF)));
    assert!(!is_portal(&driver(0x0200_0F2E)));
}

/// Each colour's glow picture is of its colour: the portals keep their own colours, read from
/// their pictures.
#[test]
fn each_portals_glow_is_its_own_colour() {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let assets = DatAnimAssets::new(Arc::clone(&store));
    let glow = |setup: u32| {
        let script = assets.setup(DataId(setup)).and_then(|s| s.default_script);
        let steps = assets.script(script.expect("its script")).expect("decodes");
        let e = steps
            .steps
            .iter()
            .find_map(|s| match &s.hook.kind {
                HookKind::CreateParticle(c) => assets
                    .emitter_info(c.emitter_info_id)
                    .filter(|e| e.particle_type == ParticleType::ParabolicLvgagr),
                _ => None,
            })
            .expect("a glow");
        pyreal_engine::puffs::picture(&store, sprite_of(&e)).1
    };
    let purple = glow(0x0200_01B3);
    assert!(purple.z > purple.x && purple.x > purple.y * 4.0, "{purple}");
    let blue = glow(0x0200_05D2);
    assert!(blue.z > blue.x * 3.0 && blue.z > blue.y * 3.0, "{blue}");
    let green = glow(0x0200_05D3);
    assert!(
        green.y > green.x * 3.0 && green.y > green.z * 3.0,
        "{green}"
    );
    let orange = glow(0x0200_05D4);
    assert!(orange.x > orange.y * 2.0 && orange.y > orange.z, "{orange}");
    let red = glow(0x0200_05D5);
    assert!(red.x > red.y * 5.0 && red.x > red.z * 5.0, "{red}");
    let yellow = glow(0x0200_05D6);
    assert!(
        yellow.y > yellow.z * 5.0 && yellow.x > yellow.z * 5.0,
        "{yellow}"
    );
    let white = glow(0x0200_06F4);
    assert!(white.max_element() - white.min_element() < 0.05, "{white}");
}
