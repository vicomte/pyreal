//! Lists the pictures worn items draw on players: for each clothing table and player body, the
//! parts it swaps, their surfaces' pictures after its texture swaps, and the share of each
//! picture's texels that each palette range of the item's template tints; writes each picture as a
//! PNG in that template and its palette indices beside it.
//!
//! Usage: `armor_survey DAT_DIR OUT_DIR CLOTHING:TEMPLATE...`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use pyreal_assets::{CharGen, ClothingTable, Decode, GfxObj, Surface};
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_engine::asset::texture::{PaletteSwap, PixelLayout, TextureSource};
use pyreal_primitives::DataId;

fn read<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

fn palset(store: &RetailDatStore, id: DataId) -> Vec<DataId> {
    store
        .read_typed(DbType::PalSet, id)
        .ok()
        .and_then(|b| {
            pyreal_assets::material::PaletteSet::decode_payload_in(store.era_of(id), id, &b).ok()
        })
        .map(|p| p.palette_ids)
        .unwrap_or_default()
}

fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) {
    let f = std::fs::File::create(path).expect("png");
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .and_then(|mut w| w.write_image_data(rgba))
        .expect("png");
}

fn hex(s: &str) -> u32 {
    u32::from_str_radix(s.trim_start_matches("0x"), 16).expect("hex")
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let store = RetailDatStore::open_dir(std::path::Path::new(&a[1])).expect("dats");
    let out = std::path::PathBuf::from(&a[2]);
    std::fs::create_dir_all(out.join("pics")).expect("out");
    let src = TextureSource::new(&store);
    let cg: CharGen = read(&store, DbType::CharGen, DataId(0x0E00_0002)).expect("chargen");
    // Player bodies: setup -> (name, base palette).
    let mut bodies: BTreeMap<u32, (String, DataId)> = BTreeMap::new();
    for key in &cg.heritage_order {
        let Some(g) = cg.heritage_groups.get(key) else {
            continue;
        };
        for sx in g.sexes.values() {
            let who = format!("{}-{}", g.name.replace(' ', ""), sx.name).to_lowercase();
            bodies
                .entry(sx.setup.0)
                .and_modify(|e| {
                    e.0.push('+');
                    e.0.push_str(&who);
                })
                .or_insert((who, sx.base_palette));
        }
    }
    let mut parts = std::fs::File::create(out.join("parts.tsv")).expect("tsv");
    writeln!(parts, "clothing\tsetup\tbodies\tpart\tgfxobj\tsurface\tsurface_type\torig\tpicture\twidth\theight\tformat").ok();
    let mut ranges = std::fs::File::create(out.join("ranges.tsv")).expect("tsv");
    writeln!(
        ranges,
        "clothing\ttemplate\tpicture\teffect\tpalset\tpalettes\tranges\tshare"
    )
    .ok();
    let mut tpl = std::fs::File::create(out.join("templates.tsv")).expect("tsv");
    writeln!(tpl, "clothing\ttemplate\ticon\teffects").ok();
    let mut written = BTreeSet::new();
    for spec in &a[3..] {
        let (c, t) = spec.split_once(':').unwrap_or((spec, "0"));
        let cid = DataId(hex(c));
        let want = t.parse::<u32>().unwrap_or(0);
        let Some(table) = read::<ClothingTable>(&store, DbType::Clothing, cid) else {
            println!("{:#010X}: no table", cid.0);
            continue;
        };
        for (k, p) in &table.palette_templates {
            let effs: Vec<String> = p
                .subpalette_effects
                .iter()
                .map(|e| {
                    let r: Vec<String> = e
                        .ranges
                        .iter()
                        .map(|r| format!("{}+{}", r.offset, r.length))
                        .collect();
                    format!(
                        "{:#010X}[{}]{}",
                        e.palette_set.0,
                        palset(&store, e.palette_set).len(),
                        r.join(",")
                    )
                })
                .collect();
            writeln!(
                tpl,
                "{:#010X}\t{k}\t{:#010X}\t{}",
                cid.0,
                p.icon.0,
                effs.join(" ")
            )
            .ok();
        }
        let template = if table.palette_templates.contains_key(&want) {
            want
        } else {
            table.palette_templates.keys().next().copied().unwrap_or(0)
        };
        let effects = table
            .palette_templates
            .get(&template)
            .map(|p| p.subpalette_effects.clone())
            .unwrap_or_default();
        for (setup, base) in &table.clothing_bases {
            let Some((who, base_palette)) = bodies.get(&setup.0) else {
                continue;
            };
            // The template's palettes at a middle shade, as the item's own colour.
            let mut swap = PaletteSwap {
                base: *base_palette,
                ranges: Vec::new(),
                wire: true,
            };
            let mut eff_pals = Vec::new();
            for e in &effects {
                let set = palset(&store, e.palette_set);
                let pal = set.get(set.len() / 2).copied().unwrap_or(e.palette_set);
                eff_pals.push(set.len());
                for r in &e.ranges {
                    swap.ranges.push((pal, r.offset / 8, r.length / 8));
                }
            }
            let pal = src.compose(&swap);
            for eff in base {
                let swaps: BTreeMap<u32, u32> = eff
                    .texture_effects
                    .iter()
                    .map(|t| (t.old_texture.0, t.new_texture.0))
                    .collect();
                let Some(gfx) = read::<GfxObj>(&store, DbType::GfxObj, eff.object_id) else {
                    continue;
                };
                for s in &gfx.surfaces {
                    let Some(surf) = read::<Surface>(&store, DbType::Surface, *s) else {
                        continue;
                    };
                    let Some(orig) = surf.orig_texture_id else {
                        continue;
                    };
                    let pic = DataId(swaps.get(&orig.0).copied().unwrap_or(orig.0));
                    let Ok((_rsid, rs, bytes)) = src.resolve(pic) else {
                        continue;
                    };
                    writeln!(
                        parts,
                        "{:#010X}\t{:#010X}\t{who}\t{}\t{:#010X}\t{:#010X}\t{:#X}\t{:#010X}\t{:#010X}\t{}\t{}\t{}",
                        cid.0, setup.0, eff.part_num, eff.object_id.0, s.0, surf.surface_type,
                        orig.0, pic.0, rs.width, rs.height, rs.format
                    )
                    .ok();
                    let n = (rs.width * rs.height) as usize;
                    let idx: Vec<u16> = match (rs.format, rs.payload(&bytes)) {
                        (41, Some(p)) => p[..n].iter().map(|&b| u16::from(b)).collect(),
                        (101, Some(p)) => p[..n * 2]
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|b| u16::from_le_bytes(*b))
                            .collect(),
                        _ => Vec::new(),
                    };
                    if !idx.is_empty() && written.insert((cid.0, pic.0)) {
                        for (i, e) in effects.iter().enumerate() {
                            let inr = |v: u16| {
                                e.ranges.iter().any(|r| {
                                    u32::from(v) >= r.offset && u32::from(v) < r.offset + r.length
                                })
                            };
                            let c = idx.iter().filter(|v| inr(**v)).count();
                            let r: Vec<String> = e
                                .ranges
                                .iter()
                                .map(|r| format!("{}+{}", r.offset, r.length))
                                .collect();
                            writeln!(
                                ranges,
                                "{:#010X}\t{template}\t{:#010X}\t{i}\t{:#010X}\t{}\t{}\t{:.3}",
                                cid.0,
                                pic.0,
                                e.palette_set.0,
                                eff_pals[i],
                                r.join(","),
                                c as f64 / n as f64
                            )
                            .ok();
                        }
                    }
                    let name = format!("{:08X}_{:08X}", pic.0, cid.0);
                    if out.join("pics").join(format!("{name}.png")).exists() {
                        continue;
                    }
                    if let Ok(px) = src.decode(pic, false, pal.as_ref()) {
                        if px.layout == PixelLayout::Bgra8 {
                            let mut rgba = px.levels[0].clone();
                            for c in rgba.as_chunks_mut::<4>().0 {
                                c.swap(0, 2);
                            }
                            write_png(
                                &out.join("pics").join(format!("{name}.png")),
                                px.width,
                                px.height,
                                &rgba,
                            );
                        }
                    }
                    if !idx.is_empty()
                        && !out.join("pics").join(format!("{:08X}.idx", pic.0)).exists()
                    {
                        let mut b = Vec::with_capacity(8 + n * 2);
                        b.extend(rs.width.to_le_bytes());
                        b.extend(rs.height.to_le_bytes());
                        for v in idx {
                            b.extend(v.to_le_bytes());
                        }
                        std::fs::write(out.join("pics").join(format!("{:08X}.idx", pic.0)), b).ok();
                    }
                }
            }
        }
    }
}
