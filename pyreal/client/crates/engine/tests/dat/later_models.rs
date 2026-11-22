//! End of Retail's torch pillars burn ([`pyreal_engine::flames::BURNS_AS`]), and the melted
//! snowman's puddle is water.

use pyreal_engine::flames::{examine, FlameKind};
use pyreal_primitives::DataId;

/// Both pillars burn one torch flame at the niche in the pillar's face, the one on a base too,
/// though End of Retail gave it no script.
#[test]
fn both_torch_pillars_burn_at_their_niche() {
    let eor = pyreal_dat::testing::open_store_or_fail();
    for id in [0x0200_039C_u32, 0x0200_039D] {
        let f = examine(&eor, DataId(id));
        assert_eq!(f.len(), 1, "{id:#010X} burns once: {f:?}");
        assert_eq!(f[0].kind, FlameKind::Torch, "{id:#010X}");
        assert!(
            (1.8..2.3).contains(&f[0].offset.z),
            "{id:#010X} burns at {}",
            f[0].offset
        );
    }
}

/// The melted snowman's puddle is drawn as water, and it is the only surface of its kind.
#[test]
fn the_snowman_puddle_is_water_and_alone() {
    use pyreal_assets::Surface;
    use pyreal_dat::DbType;
    use pyreal_engine::asset::material::is_pool;
    let eor = pyreal_dat::testing::open_store_or_fail();
    let solid: Vec<_> = eor
        .ids_of(DbType::Surface)
        .into_iter()
        .filter(|&id| {
            super::decode::<Surface>(&eor, DbType::Surface, id)
                .is_some_and(|s| s.orig_texture_id.is_none() && is_pool(&s))
        })
        .collect();
    assert_eq!(solid, [DataId(0x0800_0B3C)]);
}
