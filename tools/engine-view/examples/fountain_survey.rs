//! Lists every placed setup whose default script throws a jet of water up (an endless emitter
//! thrown up and pulled back down), with its emitters, its translucent surfaces and the height of
//! their level faces, and how many times the landblocks and the dungeons' cells place it.
//!
//! Usage: `fountain_survey DAT_DIR`, `fountain_survey DAT_DIR SETUP` for every place one setup
//! stands, `fountain_survey DAT_DIR --uses PICTURE` for the surfaces, models and rooms drawn
//! with a picture, or `fountain_survey DAT_DIR --water OUT_DIR` for the pictures on level faces
//! held below a model's rim (water in a basin, tub, barrel, trough or well), each written to
//! `OUT_DIR` as a PNG.

use std::collections::HashMap;

use pyreal_assets::hook::HookData;
use pyreal_assets::world::{EnvCell, LandblockInfo, ParticleEmitterInfo};
use pyreal_assets::{Decode, GfxObj, PhysicsScript, Setup, Surface};
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::DataId;

fn read<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

fn placed(store: &RetailDatStore, mut each: impl FnMut(u32, u32, pyreal_primitives::Vec3)) {
    for id in store.ids_of(DbType::Lbi) {
        if let Some(info) = read::<LandblockInfo>(store, DbType::Lbi, id) {
            for o in &info.objects {
                each(o.id.0, id.0, o.frame.origin);
            }
        }
    }
    for id in store.ids_of(DbType::Cell) {
        if id.0 & 0xFFFF < 0x100 {
            continue;
        }
        if let Some(cell) = read::<EnvCell>(store, DbType::Cell, id) {
            for o in &cell.static_objects {
                each(o.id.0, id.0, o.frame.origin);
            }
        }
    }
}

/// The translucent surfaces of a part, each with the heights of its level faces.
fn pools(store: &RetailDatStore, part: DataId) -> Vec<String> {
    let Some(g) = read::<GfxObj>(store, DbType::GfxObj, part) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (k, sid) in g.surfaces.iter().enumerate() {
        let Some(s) = read::<Surface>(store, DbType::Surface, *sid) else {
            continue;
        };
        if s.surface_type & 0x10 == 0 {
            continue;
        }
        let mut levels: Vec<String> = Vec::new();
        for p in g
            .polygons
            .iter()
            .filter(|p| usize::from(p.pos_surface) == k)
        {
            let zs: Vec<f32> = p
                .vertex_ids
                .iter()
                .filter_map(|&v| g.vertex_array.vertices.get(usize::from(v)))
                .map(|v| v.position.z)
                .collect();
            let (lo, hi) = zs
                .iter()
                .fold((f32::MAX, f32::MIN), |(a, b), &z| (a.min(z), b.max(z)));
            let tag = if hi - lo < 0.01 {
                format!("{lo:.2}")
            } else {
                format!("{lo:.2}..{hi:.2}")
            };
            if !levels.contains(&tag) {
                levels.push(tag);
            }
        }
        out.push(format!(
            "{:#010X} type {:#X} trans {:.2} lum {:.2} pic {:#010X} at [{}]",
            sid.0,
            s.surface_type,
            s.translucency,
            s.luminosity,
            s.orig_texture_id.map_or(0, |t| t.0),
            levels.join(" ")
        ));
    }
    out
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let store = RetailDatStore::open_dir(std::path::Path::new(&a[1])).expect("dats");
    if a.get(2).map(String::as_str) == Some("--uses") {
        uses(
            &store,
            u32::from_str_radix(a[3].trim_start_matches("0x"), 16).expect("picture"),
        );
        return;
    }
    if a.get(2).map(String::as_str) == Some("--in") {
        let ids: Vec<u32> = a[3..]
            .iter()
            .map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).expect("setup"))
            .collect();
        held_in(&store, &ids);
        return;
    }
    if a.get(2).map(String::as_str) == Some("--faces") {
        let id = u32::from_str_radix(a[3].trim_start_matches("0x"), 16).expect("setup");
        faces(&store, id);
        return;
    }
    if a.get(2).map(String::as_str) == Some("--water") {
        water(&store, std::path::Path::new(&a[3]));
        return;
    }
    if let Some(only) = a.get(2) {
        let only = u32::from_str_radix(only.trim_start_matches("0x"), 16).expect("setup");
        placed(&store, |id, at, o| {
            if id == only {
                println!("{at:#010X} {o:?}");
            }
        });
        return;
    }
    // ORDER-OK: counted, then sorted.
    let mut count: HashMap<u32, (u32, u32)> = HashMap::new();
    placed(&store, |id, at, _| {
        count.entry(id).or_insert((0, at)).0 += 1;
    });
    let mut rows = Vec::new();
    for (&id, &(n, at)) in &count {
        let Some(setup) = read::<Setup>(&store, DbType::Setup, DataId(id)) else {
            continue;
        };
        let Some(script) =
            read::<PhysicsScript>(&store, DbType::PhysicsScript, setup.default_script_id)
        else {
            continue;
        };
        let mut desc = Vec::new();
        let mut jet = false;
        for step in &script.script_data {
            let HookData::CreateParticle(c) = &step.hook.data else {
                continue;
            };
            let Some(e) =
                read::<ParticleEmitterInfo>(&store, DbType::ParticleEmitter, c.emitter_info_id)
            else {
                continue;
            };
            let gid = if e.hw_gfxobj_id.0 != 0 {
                e.hw_gfxobj_id
            } else {
                e.gfxobj_id
            };
            let endless = e.total_particles <= 0 && e.total_seconds <= 0.0;
            let up = e.a.z * e.max_a;
            let pull = e.b.z * e.max_b;
            if endless && up >= 1.0 && pull < 0.0 && e.final_scale <= 1.5 {
                jet = true;
            }
            desc.push(format!(
                "{:#010X} part {} at {:?} type {} life {:.1} rate {:.3} scale {:.2}->{:.2} a {:?}x{:.2}..{:.2} b {:?}x{:.2}..{:.2} c {:?} pic {:#010X}",
                e.id.0,
                c.part_index,
                c.offset.origin,
                e.particle_type,
                e.lifespan,
                e.birthrate,
                e.start_scale,
                e.final_scale,
                (e.a.x, e.a.y, e.a.z),
                e.min_a,
                e.max_a,
                (e.b.x, e.b.y, e.b.z),
                e.min_b,
                e.max_b,
                (e.c.x, e.c.y, e.c.z),
                gid.0
            ));
        }
        if jet {
            for p in &setup.parts {
                for s in pools(&store, *p) {
                    desc.push(format!("part {:#010X} pool {s}", p.0));
                }
            }
            rows.push((n, id, at, desc.join("\n    ")));
        }
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (n, id, at, d) in rows {
        println!("{id:#010X} placed {n:5} e.g. {at:#010X}:\n    {d}");
    }
}

/// The surfaces drawn with `picture`, the models and rooms that use them (a model's level faces'
/// heights) and how many times the setups holding each model are placed.
fn uses(store: &RetailDatStore, picture: u32) {
    let surfaces: Vec<DataId> = store
        .ids_of(DbType::Surface)
        .into_iter()
        .filter(|&id| {
            read::<Surface>(store, DbType::Surface, id)
                .is_some_and(|s| s.orig_texture_id.is_some_and(|t| t.0 == picture))
        })
        .collect();
    for &s in &surfaces {
        if let Some(x) = read::<Surface>(store, DbType::Surface, s) {
            println!(
                "surface {:#010X} type {:#X} trans {:.2} lum {:.2}",
                s.0, x.surface_type, x.translucency, x.luminosity
            );
        }
    }
    let mut models = Vec::new();
    for id in store.ids_of(DbType::GfxObj) {
        let Some(g) = read::<GfxObj>(store, DbType::GfxObj, id) else {
            continue;
        };
        if g.surfaces.iter().any(|s| surfaces.contains(s)) {
            models.push(id.0);
        }
    }
    // ORDER-OK: counted, then printed in model order.
    let mut setups: HashMap<u32, Vec<u32>> = HashMap::new();
    for id in store.ids_of(DbType::Setup) {
        if let Some(s) = read::<Setup>(store, DbType::Setup, id) {
            for p in &s.parts {
                if models.contains(&p.0) {
                    setups.entry(p.0).or_default().push(id.0);
                }
            }
        }
    }
    // ORDER-OK: counted, then looked up.
    let mut count: HashMap<u32, u32> = HashMap::new();
    placed(store, |id, _, _| *count.entry(id).or_insert(0) += 1);
    for m in &models {
        let holders = setups.get(m).cloned().unwrap_or_default();
        let n: u32 = holders
            .iter()
            .map(|s| count.get(s).copied().unwrap_or(0))
            .sum();
        println!("model {m:#010X} placed {n} in setups {:X?}", holders);
        for line in pools(store, DataId(*m)) {
            println!("    {line}");
        }
    }
    let mut cells = 0;
    for id in store.ids_of(DbType::Cell) {
        if id.0 & 0xFFFF < 0x100 {
            continue;
        }
        if let Some(c) = read::<EnvCell>(store, DbType::Cell, id) {
            if c.surfaces.iter().any(|s| surfaces.contains(s)) {
                cells += 1;
            }
        }
    }
    println!("dungeon cells drawing it {cells}");
}

/// The pictures on level faces held below their model's rim, by how often the setups holding
/// them are placed, with their mean colour; each written to `out` as `PICTURE.png`.
fn water(store: &RetailDatStore, out: &std::path::Path) {
    // ORDER-OK: counted, then looked up.
    let mut count: HashMap<u32, u32> = HashMap::new();
    placed(store, |id, _, _| *count.entry(id).or_insert(0) += 1);
    // ORDER-OK: gathered, then sorted.
    let mut holders: HashMap<u32, Vec<u32>> = HashMap::new();
    for id in store.ids_of(DbType::Setup) {
        if let Some(s) = read::<Setup>(store, DbType::Setup, id) {
            for p in &s.parts {
                holders.entry(p.0).or_default().push(id.0);
            }
        }
    }
    // picture -> (placed, models, surfaces)
    // ORDER-OK: gathered, then sorted.
    let mut found: HashMap<u32, (u32, Vec<u32>, Vec<u32>)> = HashMap::new();
    for id in store.ids_of(DbType::GfxObj) {
        let Some(g) = read::<GfxObj>(store, DbType::GfxObj, id) else {
            continue;
        };
        for (sid, _, _) in held(&g) {
            let Some(s) = read::<Surface>(store, DbType::Surface, sid) else {
                continue;
            };
            let Some(pic) = s.orig_texture_id else {
                continue;
            };
            let n: u32 = holders
                .get(&id.0)
                .map(|h| h.iter().map(|s| count.get(s).copied().unwrap_or(0)).sum())
                .unwrap_or(0);
            let e = found.entry(pic.0).or_default();
            e.0 += n;
            if !e.1.contains(&id.0) {
                e.1.push(id.0);
            }
            if !e.2.contains(&sid.0) {
                e.2.push(sid.0);
            }
        }
    }
    let source = pyreal_engine::asset::texture::TextureSource::new(store);
    let mut rows: Vec<_> = found.into_iter().collect();
    rows.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(&b.0)));
    std::fs::create_dir_all(out).expect("out dir");
    for (pic, (n, models, surfaces)) in rows {
        let Ok((w, h, bgra)) = source.bgra(DataId(pic)) else {
            continue;
        };
        let avg = source.average(DataId(pic)).unwrap_or([0; 3]);
        println!(
            "{pic:#010X} placed {n:5} avg {:?} models {} surfaces {:X?} e.g. {:#010X}",
            avg,
            models.len(),
            surfaces,
            models[0]
        );
        let rgba: Vec<u8> = bgra
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], 255])
            .collect();
        let file = std::fs::File::create(out.join(format!("{pic:08X}.png"))).expect("png");
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut wr| wr.write_image_data(&rgba))
            .expect("png");
    }
}

/// A model's surfaces drawn on level faces held below its rim: each surface, the faces' height
/// and their area.
fn held(g: &GfxObj) -> Vec<(DataId, f32, f32)> {
    let vs = &g.vertex_array.vertices;
    let (lo, hi) = vs.iter().fold((f32::MAX, f32::MIN), |(a, b), v| {
        (a.min(v.position.z), b.max(v.position.z))
    });
    let mut out = Vec::new();
    for (k, sid) in g.surfaces.iter().enumerate() {
        let mut area = 0.0;
        let mut level = None;
        for p in g
            .polygons
            .iter()
            .filter(|p| usize::from(p.pos_surface) == k)
        {
            let pts: Vec<_> = p
                .vertex_ids
                .iter()
                .filter_map(|&v| vs.get(usize::from(v)))
                .map(|v| v.position)
                .collect();
            let (a, b) = pts
                .iter()
                .fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(v.z), b.max(v.z)));
            if pts.len() < 3 || b - a > 0.01 {
                continue;
            }
            for w in 1..pts.len() - 1 {
                let (ux, uy) = (pts[w].x - pts[0].x, pts[w].y - pts[0].y);
                let (vx, vy) = (pts[w + 1].x - pts[0].x, pts[w + 1].y - pts[0].y);
                area += 0.5 * (ux * vy - uy * vx).abs();
            }
            level = Some(a);
        }
        if let Some(z) = level {
            if area <= 12.0 && z >= lo + 0.1 && z <= hi - 0.03 {
                out.push((*sid, z, area));
            }
        }
    }
    out
}

/// Each part of `setups` with the surfaces it holds below its rim, their picture and mean colour.
fn held_in(store: &RetailDatStore, setups: &[u32]) {
    let source = pyreal_engine::asset::texture::TextureSource::new(store);
    for &id in setups {
        let Some(setup) = read::<Setup>(store, DbType::Setup, DataId(id)) else {
            println!("{id:#010X} unreadable");
            continue;
        };
        println!("{id:#010X}");
        for p in &setup.parts {
            let Some(g) = read::<GfxObj>(store, DbType::GfxObj, *p) else {
                continue;
            };
            for (sid, z, area) in held(&g) {
                let Some(s) = read::<Surface>(store, DbType::Surface, sid) else {
                    continue;
                };
                let pic = s.orig_texture_id.map_or(0, |t| t.0);
                let avg = source.average(DataId(pic)).unwrap_or([0; 3]);
                println!(
                    "    part {:#010X} surface {:#010X} type {:#X} trans {:.2} pic {pic:#010X} avg {avg:?} at {z:.2} area {area:.2}",
                    p.0, sid.0, s.surface_type, s.translucency
                );
            }
        }
    }
}

/// Setup `id`'s water faces, part by part: each face's corners on its part's model.
fn faces(store: &RetailDatStore, id: u32) {
    let Some(setup) = read::<Setup>(store, DbType::Setup, DataId(id)) else {
        return;
    };
    let frames = setup
        .placement_frames
        .values()
        .next()
        .map(|p| p.frames.clone());
    for (k, p) in setup.parts.iter().enumerate() {
        let Some(g) = read::<GfxObj>(store, DbType::GfxObj, *p) else {
            continue;
        };
        let f = frames.as_ref().and_then(|f| f.get(k));
        println!("part {k} {:#010X} frame {f:?}", p.0);
        for poly in &g.polygons {
            let Some(sid) = g.surfaces.get(usize::from(poly.pos_surface)) else {
                continue;
            };
            let Some(s) = read::<Surface>(store, DbType::Surface, *sid) else {
                continue;
            };
            if !pyreal_engine::asset::material::is_pool(&s) {
                continue;
            }
            let at: Vec<String> = poly
                .vertex_ids
                .iter()
                .filter_map(|&v| g.vertex_array.vertices.get(usize::from(v)))
                .map(|v| {
                    format!(
                        "({:.3} {:.3} {:.3})",
                        v.position.x, v.position.y, v.position.z
                    )
                })
                .collect();
            println!("    {:#010X} {}", sid.0, at.join(" "));
        }
    }
}
