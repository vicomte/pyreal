//! Prints emitters and GfxObjs in full: an emitter's every field, a GfxObj's surfaces (type,
//! translucency, luminosity, picture), its vertices' extent, and writes each picture as a PNG.
//!
//! Usage: `sprite_probe DAT_DIR OUT_DIR ID...` (0x32… emitters, 0x01… GfxObjs, 0x08… surfaces).

use pyreal_assets::world::ParticleEmitterInfo;
use pyreal_assets::{Decode, GfxObj, Surface};
use pyreal_dat::{divine_type, DbType, RetailDatStore};
use pyreal_engine::asset::texture::TextureSource;
use pyreal_primitives::DataId;

fn read<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

fn surface(store: &RetailDatStore, out: &std::path::Path, id: DataId) {
    let Some(s) = read::<Surface>(store, DbType::Surface, id) else {
        println!("  surface {:#010X} will not read", id.0);
        return;
    };
    println!(
        "  surface {:#010X} type {:#X} trans {:.2} lum {:.2} diffuse {:.2} colour {:?} texture {:?} palette {:?}",
        id.0,
        s.surface_type,
        s.translucency,
        s.luminosity,
        s.diffuse,
        s.color_value.map(|c| format!("{c:#010X}")),
        s.orig_texture_id.map(|t| format!("{:#010X}", t.0)),
        s.orig_palette_id.map(|t| format!("{:#010X}", t.0))
    );
    let Some(t) = s.orig_texture_id else { return };
    let src = TextureSource::new(store);
    match src.bgra(t) {
        Ok((w, h, px)) => {
            let rgba: Vec<u8> = px
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[2], p[1], p[0], p[3]])
                .collect();
            let (mut a_min, mut a_max, mut sum) = (255u8, 0u8, [0u64; 4]);
            for p in rgba.as_chunks::<4>().0 {
                a_min = a_min.min(p[3]);
                a_max = a_max.max(p[3]);
                for (s, c) in sum.iter_mut().zip(p) {
                    *s += u64::from(*c);
                }
            }
            let n = u64::from(w * h).max(1);
            println!(
                "    picture {:#010X} {w}x{h} alpha {a_min}..{a_max} mean rgba {:?}",
                t.0,
                sum.map(|s| s / n)
            );
            let path = out.join(format!("{:08X}.png", t.0));
            let file = std::fs::File::create(&path).expect("png");
            let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header()
                .and_then(|mut wr| wr.write_image_data(&rgba))
                .expect("png");
        }
        Err(e) => println!("    picture {:#010X}: {e}", t.0),
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let store = RetailDatStore::open_dir(std::path::Path::new(&a[1])).expect("dats");
    let out = std::path::Path::new(&a[2]);
    std::fs::create_dir_all(out).expect("out dir");
    for s in &a[3..] {
        let id = DataId(u32::from_str_radix(s.trim_start_matches("0x"), 16).expect("hex"));
        match divine_type(id) {
            Some(DbType::ParticleEmitter) => {
                if let Some(e) = read::<ParticleEmitterInfo>(&store, DbType::ParticleEmitter, id) {
                    println!("emitter {e:#?}");
                }
            }
            Some(DbType::GfxObj) => {
                let Some(g) = read::<GfxObj>(&store, DbType::GfxObj, id) else {
                    println!("gfxobj {:#010X} will not read", id.0);
                    continue;
                };
                let vs: Vec<_> = g.vertex_array.vertices.iter().map(|v| v.position).collect();
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for v in &vs {
                    for (k, c) in [v.x, v.y, v.z].into_iter().enumerate() {
                        lo[k] = lo[k].min(c);
                        hi[k] = hi[k].max(c);
                    }
                }
                println!(
                    "gfxobj {:#010X} flags {:#X} polys {} verts {} extent {lo:?}..{hi:?} degrade {:?}",
                    id.0,
                    g.flags,
                    g.polygons.len(),
                    vs.len(),
                    g.did_degrade
                );
                for p in &g.polygons {
                    println!(
                        "  poly stippling {:?} sides {:?} pos_surface {} neg_surface {} verts {}",
                        p.stippling,
                        p.sides_type,
                        p.pos_surface,
                        p.neg_surface,
                        p.vertex_ids.len()
                    );
                }
                for sid in &g.surfaces {
                    surface(&store, out, *sid);
                }
            }
            Some(DbType::Surface) => surface(&store, out, id),
            t => println!("{:#010X}: {t:?} not probed", id.0),
        }
    }
}
