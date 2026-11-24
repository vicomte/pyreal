//! Lists every setup whose default script plays one of the given emitters, and how many times the
//! landblocks, the dungeon cells and the generated scenery place each, with an example block.
//!
//! Usage: `vent_survey DAT_DIR EMITTER...` (each as 0x320002D0).

use std::collections::{BTreeMap, BTreeSet};

use pyreal_assets::hook::HookData;
use pyreal_assets::world::{EnvCell, LandblockInfo};
use pyreal_assets::{Decode, PhysicsScript, Setup};
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::DataId;

fn read<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

/// The emitters a script creates, following the scripts it calls.
fn emitters(store: &RetailDatStore, id: DataId, seen: &mut BTreeSet<u32>, out: &mut BTreeSet<u32>) {
    if id.0 == 0 || !seen.insert(id.0) {
        return;
    }
    let Some(s) = read::<PhysicsScript>(store, DbType::PhysicsScript, id) else {
        return;
    };
    for step in &s.script_data {
        match &step.hook.data {
            HookData::CreateParticle(c) => {
                out.insert(c.emitter_info_id.0);
            }
            HookData::CallPes(c) => emitters(store, c.pes, seen, out),
            _ => {}
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&a[1]);
    let store = if pyreal_dat::holds_modern_dats(dir) {
        RetailDatStore::open_dir(dir)
    } else {
        RetailDatStore::open_classic_dir(dir)
    }
    .expect("dats");
    let wanted: BTreeSet<u32> = a[2..]
        .iter()
        .map(|e| u32::from_str_radix(e.trim_start_matches("0x"), 16).expect("emitter"))
        .collect();
    let mut setups = BTreeMap::new();
    for id in store.ids_of(DbType::Setup) {
        let Some(s) = read::<Setup>(&store, DbType::Setup, id) else {
            continue;
        };
        let mut out = BTreeSet::new();
        emitters(&store, s.default_script_id, &mut BTreeSet::new(), &mut out);
        let hit: Vec<u32> = out.intersection(&wanted).copied().collect();
        if !hit.is_empty() {
            setups.insert(id.0, hit);
        }
    }
    // Per setup: (landblock statics, cell statics, scenery, an example block).
    let mut placed: BTreeMap<u32, (u32, u32, u32, u32)> = BTreeMap::new();
    for id in store.ids_of(DbType::Lbi) {
        if let Some(info) = read::<LandblockInfo>(&store, DbType::Lbi, id) {
            for o in info.objects.iter().filter(|o| setups.contains_key(&o.id.0)) {
                let e = placed.entry(o.id.0).or_insert((0, 0, 0, id.0 >> 16));
                e.0 += 1;
            }
        }
    }
    for id in store.ids_of(DbType::Cell) {
        if id.0 & 0xFFFF < 0x100 {
            continue;
        }
        if let Some(cell) = read::<EnvCell>(&store, DbType::Cell, id) {
            for o in cell
                .static_objects
                .iter()
                .filter(|o| setups.contains_key(&o.id.0))
            {
                let e = placed.entry(o.id.0).or_insert((0, 0, 0, id.0 >> 16));
                e.1 += 1;
            }
        }
    }
    let region = pyreal_world_data::landblock::load_region(&store).expect("region");
    let mut planner = pyreal_engine::world::Planner::new(&region);
    for bx in 0..255 {
        for by in 0..255 {
            for (id, _) in planner.scenery(&store, &region, (bx, by)) {
                if setups.contains_key(&id.0) {
                    let block = u32::try_from((bx << 8) | by).unwrap_or(0);
                    let e = placed.entry(id.0).or_insert((0, 0, 0, block));
                    e.2 += 1;
                }
            }
        }
    }
    for (id, hit) in &setups {
        let (lb, cells, scenery, block) = placed.get(id).copied().unwrap_or_default();
        let hit: Vec<String> = hit.iter().map(|e| format!("{e:#010X}")).collect();
        println!(
            "{id:#010X} emitters [{}] landblocks {lb} cells {cells} scenery {scenery} e.g. {block:#06X}",
            hit.join(" ")
        );
    }
}
