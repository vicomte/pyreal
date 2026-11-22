//! Writes a setup's parts at the resting placement as JSON: each part's graphics object, its
//! place in the setup, and its triangles as the engine builds them (positions, normals and UVs in
//! the part's own space, with each sub-mesh's surface and its solid colour).
//!
//! Usage: `statue_probe DAT_DIR SETUP` (SETUP as 0x02000A53), JSON to stdout.

use pyreal_assets::{Decode, GfxObj, Surface};
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_engine::asset::mesh::{gfxobj_mesh, Sides};
use pyreal_engine::asset::object::object_parts;
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
    let id = DataId(u32::from_str_radix(a[2].trim_start_matches("0x"), 16).expect("setup"));
    let mut parts = Vec::new();
    for (k, p) in object_parts(&store, id).into_iter().enumerate() {
        let Some(g) = read::<GfxObj>(&store, DbType::GfxObj, p.gfxobj) else {
            continue;
        };
        let mesh = gfxobj_mesh(&g);
        let m = p.transform.to_cols_array();
        let mut subs = Vec::new();
        for s in &mesh.submeshes {
            let surface = s.surface.map_or(0, |d| d.0);
            let colour = s
                .surface
                .and_then(|d| read::<Surface>(&store, DbType::Surface, d))
                .and_then(|r| r.color_value)
                .unwrap_or(0);
            let tris: Vec<String> = mesh.indices
                [s.first_index as usize..(s.first_index + s.index_count) as usize]
                .iter()
                .map(|&i| {
                    let v = &mesh.vertices[i as usize];
                    format!(
                        "[{},{},{},{},{},{},{},{}]",
                        v.position[0],
                        v.position[1],
                        v.position[2],
                        v.normal[0],
                        v.normal[1],
                        v.normal[2],
                        v.uv[0],
                        v.uv[1]
                    )
                })
                .collect();
            subs.push(format!(
                "{{\"surface\":{surface},\"colour\":{colour},\"two_sided\":{},\"corners\":[{}]}}",
                matches!(s.sides, Sides::Both),
                tris.join(",")
            ));
        }
        parts.push(format!(
            "{{\"part\":{k},\"gfxobj\":{},\"transform\":{:?},\"subs\":[{}]}}",
            p.gfxobj.0,
            m,
            subs.join(",")
        ));
    }
    println!("[{}]", parts.join(",\n"));
}
