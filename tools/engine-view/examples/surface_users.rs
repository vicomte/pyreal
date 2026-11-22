//! Lists the graphics objects that draw any of the given surfaces, and the setups that use them.
//!
//! Usage: `surface_users DAT_DIR SURFACE...` (SURFACE as 0x080006F7).

use pyreal_assets::{Decode, GfxObj, Setup};
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::DataId;

fn read<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
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
    let wanted: Vec<u32> = a[2..]
        .iter()
        .map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).expect("surface"))
        .collect();
    let mut users = Vec::new();
    for id in store.ids_of(DbType::GfxObj) {
        let Some(g) = read::<GfxObj>(&store, DbType::GfxObj, id) else {
            continue;
        };
        if g.surfaces.iter().any(|s| wanted.contains(&s.0)) {
            users.push(id.0);
        }
    }
    for id in store.ids_of(DbType::Setup) {
        let Some(s) = read::<Setup>(&store, DbType::Setup, id) else {
            continue;
        };
        let parts: Vec<String> = s
            .parts
            .iter()
            .filter(|p| users.contains(&p.0))
            .map(|p| format!("{:#010X}", p.0))
            .collect();
        if !parts.is_empty() {
            println!("setup {:#010X}: {}", id.0, parts.join(" "));
        }
    }
    let list: Vec<String> = users.iter().map(|u| format!("{u:#010X}")).collect();
    println!("{} graphics objects: {}", users.len(), list.join(" "));
}
