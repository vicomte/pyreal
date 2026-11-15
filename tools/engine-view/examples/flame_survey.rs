//! Lists every setup Pyreal draws flames for: its flames' kinds and pictures, and how many times
//! the landblocks place it, most placed first.
//!
//! Usage: `flame_survey DAT_DIR`

use std::collections::HashMap;

use pyreal_assets::world::LandblockInfo;
use pyreal_assets::Decode;
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::DataId;

fn read<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let store = RetailDatStore::open_dir(std::path::Path::new(&a[1])).expect("dats");
    // ORDER-OK: counted, then sorted.
    let mut placed: HashMap<u32, (u32, u32)> = HashMap::new();
    for id in store.ids_of(DbType::Lbi) {
        if let Some(info) = read::<LandblockInfo>(&store, DbType::Lbi, id) {
            for o in &info.objects {
                let e = placed.entry(o.id.0).or_insert((0, id.0 >> 16));
                e.0 += 1;
            }
        }
    }
    let mut rows = Vec::new();
    for id in store.ids_of(DbType::Setup) {
        let f = pyreal_engine::flames::examine(&store, id);
        if f.is_empty() {
            continue;
        }
        let (n, block) = placed.get(&id.0).copied().unwrap_or((0, 0));
        let desc: Vec<String> = f
            .iter()
            .map(|s| {
                format!(
                    "{:?}@({:.2},{:.2},{:.2}) {:#010X}",
                    s.kind, s.offset.x, s.offset.y, s.offset.z, s.sprite
                )
            })
            .collect();
        rows.push((n, id.0, block, desc.join("; ")));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (n, id, block, d) in rows {
        println!("{id:#010X} placed {n:5} e.g. {block:#06X}: {d}");
    }
}
