//! Lists a setup's lights and parts, with each part's surfaces, their types and pictures.
//!
//! Usage: `setup_probe DAT_DIR SETUP...` (SETUP as 0x020005DA).

use pyreal_assets::{Decode, GfxObj, Setup, Surface, SurfaceTexture};
use pyreal_dat::{DbType, RetailDatStore};
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
    for s in &a[2..] {
        let id = DataId(u32::from_str_radix(s.trim_start_matches("0x"), 16).expect("setup"));
        let Some(setup) = read::<Setup>(&store, DbType::Setup, id) else {
            println!("{s}: no setup");
            continue;
        };
        println!("setup {:#010X}: {} parts", id.0, setup.parts.len());
        for (k, l) in &setup.lights {
            println!("  light {k}: {l:?}");
        }
        for (i, p) in setup.parts.iter().enumerate() {
            let Some(g) = read::<GfxObj>(&store, DbType::GfxObj, *p) else {
                println!("  part {i} {:#010X}: unread", p.0);
                continue;
            };
            println!("  part {i} {:#010X}: {} polygons", p.0, g.polygons.len());
            for sid in &g.surfaces {
                let Some(sf) = read::<Surface>(&store, DbType::Surface, *sid) else {
                    continue;
                };
                let pic = sf.orig_texture_id.and_then(|t| {
                    read::<SurfaceTexture>(&store, DbType::SurfaceTexture, t)
                        .map(|st| (t, st.source_levels.first().copied()))
                });
                println!(
                    "    surface {:#010X} type {:#x} trans {} lum {} diff {} texture {:?} palette {:?}",
                    sid.0,
                    sf.surface_type,
                    sf.translucency,
                    sf.luminosity,
                    sf.diffuse,
                    pic.map(|(t, l)| (format!("{:#010X}", t.0), l.map(|l| format!("{:#010X}", l.0)))),
                    sf.orig_palette_id.map(|p| format!("{:#010X}", p.0))
                );
            }
        }
    }
}
