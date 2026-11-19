//! Lists the objects a dungeon or building cell places: id, where, its parts, lights, script and
//! the flames Pyreal draws for it.
//!
//! Usage: `cell_probe DAT_DIR CELL...` (each as 0x667901D8), or `cell_probe DAT_DIR BLOCK` (as
//! 0x6679) for every cell of the block's objects that burn, or `cell_probe DAT_DIR --where
//! SETUP...` for every cell and landblock that places one of the setups.

use pyreal_assets::world::{EnvCell, LandblockInfo};
use pyreal_assets::{Decode, Setup};
use pyreal_dat::{divine_type, DbType, RetailDatStore};
use pyreal_primitives::DataId;

fn read<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let store = {
        // End of Retail's files, or an older portal.dat and cell.dat.
        let dir = std::path::Path::new(&a[1]);
        if pyreal_dat::holds_modern_dats(dir) {
            RetailDatStore::open_dir(dir)
        } else {
            RetailDatStore::open_classic_dir(dir)
        }
        .expect("dats")
    };
    if a[2] == "--where" {
        let setups: Vec<u32> = a[3..]
            .iter()
            .map(|c| u32::from_str_radix(c.trim_start_matches("0x"), 16).expect("setup"))
            .collect();
        placed(&store, &setups);
        return;
    }
    let asked: Vec<u32> = a[2..]
        .iter()
        .map(|c| u32::from_str_radix(c.trim_start_matches("0x"), 16).expect("cell"))
        .collect();
    let burning = asked.len() == 1 && asked[0] <= 0xFFFF;
    let cells: Vec<u32> = if burning {
        store
            .ids_of(DbType::Cell)
            .into_iter()
            .map(|id| id.0)
            .filter(|id| id >> 16 == asked[0] && id & 0xFFFF >= 0x100)
            .collect()
    } else {
        asked
    };
    for c in cells {
        let id = DataId(c);
        let Some(cell) = read::<EnvCell>(&store, DbType::Cell, id) else {
            println!("{id:?}: no cell");
            continue;
        };
        if !burning {
            println!(
                "cell {:#010X} at {:?} env {:#010X}",
                id.0, cell.frame.origin, cell.environment.0
            );
        }
        for o in &cell.static_objects {
            if burning && pyreal_engine::flames::examine(&store, o.id).is_empty() {
                continue;
            }
            if burning {
                print!("cell {:#010X}", id.0);
            }
            print!(
                "  object {:#010X} at {:?} rot {:?}",
                o.id.0, o.frame.origin, o.frame.rotation
            );
            if divine_type(o.id) == Some(DbType::Setup) {
                if let Some(s) = read::<Setup>(&store, DbType::Setup, o.id) {
                    let parts: Vec<String> =
                        s.parts.iter().map(|p| format!("{:#010X}", p.0)).collect();
                    print!(
                        " parts [{}] script {:#010X} lights {}",
                        parts.join(" "),
                        s.default_script_id.0,
                        s.lights.len()
                    );
                }
                for f in pyreal_engine::flames::examine(&store, o.id) {
                    print!(
                        " flame {:?} at {:?} sprite {:#010X}",
                        f.kind, f.offset, f.sprite
                    );
                }
                for l in pyreal_engine::lamps::examine(&store, o.id) {
                    print!(
                        " lamp {:?} at {:?} colour {:?} intensity {} falloff {} {:?}",
                        l.kind, l.offset, l.colour, l.intensity, l.falloff, l.flicker
                    );
                }
            }
            println!();
        }
    }
}

/// Each placement of `setups`: the cell (or landblock) and the block-local frame.
fn placed(store: &RetailDatStore, setups: &[u32]) {
    for id in store.ids_of(DbType::Cell) {
        if id.0 & 0xFFFF < 0x100 {
            continue;
        }
        let Some(cell) = read::<EnvCell>(store, DbType::Cell, id) else {
            continue;
        };
        for o in cell
            .static_objects
            .iter()
            .filter(|o| setups.contains(&o.id.0))
        {
            println!(
                "{:#010X} in cell {:#010X} at {:?} rot {:?} floor {}",
                o.id.0, id.0, o.frame.origin, o.frame.rotation, cell.frame.origin.z
            );
        }
    }
    for id in store.ids_of(DbType::Lbi) {
        let Some(info) = read::<LandblockInfo>(store, DbType::Lbi, id) else {
            continue;
        };
        for o in info.objects.iter().filter(|o| setups.contains(&o.id.0)) {
            println!(
                "{:#010X} in block {:#06X} at {:?} rot {:?}",
                o.id.0,
                id.0 >> 16,
                o.frame.origin,
                o.frame.rotation
            );
        }
    }
}
