//! The waterfalls are told by their emitters, and nothing else is.

use std::collections::BTreeSet;
use std::sync::Arc;

use pyreal_animation::data::AnimAssets;
use pyreal_animation::hooks::HookKind;
use pyreal_assets::world::LandblockInfo;
use pyreal_dat::DbType;
use pyreal_engine::falls::{is_mist, is_sheet};
use pyreal_world_data::anim_assets::DatAnimAssets;

/// Of every setup the landblocks place, those whose default script throws a waterfall's water
/// are the four waterfall setups, and the one whose script makes its mist is 0x020003C5.
#[test]
fn the_waterfalls_are_told_by_their_emitters() {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let assets = DatAnimAssets::new(Arc::clone(&store));
    let mut placed = BTreeSet::new();
    for id in store.ids_of(DbType::Lbi) {
        if let Some(info) = super::decode::<LandblockInfo>(&store, DbType::Lbi, id) {
            placed.extend(info.objects.iter().map(|o| o.id));
        }
    }
    let (mut sheets, mut mists) = (Vec::new(), Vec::new());
    for id in placed {
        let Some(script) = assets.setup(id).and_then(|s| s.default_script) else {
            continue;
        };
        let Some(script) = assets.script(script) else {
            continue;
        };
        let infos: Vec<_> = script
            .steps
            .iter()
            .filter_map(|step| match &step.hook.kind {
                HookKind::CreateParticle(c) | HookKind::CreateBlockingParticle(c) => {
                    assets.emitter_info(c.emitter_info_id)
                }
                _ => None,
            })
            .collect();
        if infos.iter().any(|e| is_sheet(e)) {
            sheets.push(id.0);
        }
        if infos.iter().any(|e| is_mist(e)) {
            mists.push(id.0);
        }
    }
    assert_eq!(sheets, [0x0200_03E4, 0x0200_0859, 0x0200_085A, 0x0200_0B4E]);
    assert_eq!(mists, [0x0200_03C5]);
}
