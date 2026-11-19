//! Lists the objects a landblock places within a radius of a point: id, where, its parts, lights
//! and default script.
//!
//! Usage: `place_probe DAT_DIR BLOCK X Y RADIUS` (BLOCK as 0xAAB1), or `place_probe DAT_DIR BLOCK
//! SETUP` for where the block places one setup. A last `--scenery` lists the block's generated
//! scenery within the radius too.

use pyreal_assets::world::LandblockInfo;
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
    let block = u32::from_str_radix(a[2].trim_start_matches("0x"), 16).expect("block");
    let only = a[3]
        .strip_prefix("0x")
        .map(|h| u32::from_str_radix(h, 16).expect("setup"));
    let (x, y, r): (f32, f32, f32) = if only.is_some() {
        (0.0, 0.0, f32::MAX)
    } else {
        (
            a[3].parse().expect("x"),
            a[4].parse().expect("y"),
            a[5].parse().expect("radius"),
        )
    };
    let id = DataId((block << 16) | 0xFFFE);
    let Some(info) = read::<LandblockInfo>(&store, DbType::Lbi, id) else {
        println!("{id:?}: no landblock info");
        return;
    };
    let near = |o: pyreal_primitives::Vec3| ((o.x - x).powi(2) + (o.y - y).powi(2)).sqrt() <= r;
    for b in info.buildings.iter().filter(|b| near(b.frame.origin)) {
        println!("building {:#010X} at {:?}", b.id.0, b.frame.origin);
    }
    for o in info
        .objects
        .iter()
        .filter(|o| near(o.frame.origin) && only.is_none_or(|s| s == o.id.0))
    {
        print!(
            "object {:#010X} at {:?} rot {:?}",
            o.id.0, o.frame.origin, o.frame.rotation
        );
        if divine_type(o.id) == Some(DbType::Setup) {
            if let Some(s) = read::<Setup>(&store, DbType::Setup, o.id) {
                let parts: Vec<String> = s.parts.iter().map(|p| format!("{:#010X}", p.0)).collect();
                print!(
                    " parts [{}] script {:#010X} phs {:#010X} lights {}",
                    parts.join(" "),
                    s.default_script_id.0,
                    s.default_phstable_id.0,
                    s.lights.len()
                );
                for (k, l) in &s.lights {
                    print!(" [{k}: {l:?}]");
                }
            }
        }
        println!();
    }
    if a.last().is_some_and(|l| l == "--scenery") {
        let region = pyreal_world_data::landblock::load_region(&store).expect("region");
        let (bx, by) = (
            i32::try_from(block >> 8).unwrap_or(0),
            i32::try_from(block & 0xFF).unwrap_or(0),
        );
        let mut planner = pyreal_engine::world::Planner::new(&region);
        for (id, at) in planner.scenery(&store, &region, (bx, by)) {
            let o = at.translation;
            if !near(pyreal_primitives::Vec3 {
                x: o.x,
                y: o.y,
                z: o.z,
            }) {
                continue;
            }
            print!("scenery {:#010X} at {o:?}", id.0);
            if divine_type(id) == Some(DbType::Setup) {
                if let Some(s) = read::<Setup>(&store, DbType::Setup, id) {
                    print!(
                        " script {:#010X} lights {}",
                        s.default_script_id.0,
                        s.lights.len()
                    );
                }
            }
            println!();
        }
    }
}
