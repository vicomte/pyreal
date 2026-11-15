//! The fingerprint of a world's record ([`Fingerprint`]), worked out the one way both the client
//! and `pyreal-art fingerprint` work it out, so a pack's entries match the records they were made
//! from in whichever era's files (`docs/ERAS.md`).

use pyreal_assets::fingerprint::Fingerprint;
use pyreal_assets::texture_lookup::TextureLookup;
use pyreal_codecs::palette::ExpandedPalette;
use pyreal_codecs::pixel_format::PixelFormatId;
use pyreal_codecs::texture::{decode_block_chain, decode_dat_picture, DatPicture};
use pyreal_dat::{ContainerEra, DbType, RetailDatStore};
use pyreal_primitives::{DataId, TextureFormat};

/// The fingerprint of the picture (`0x05...`), setup or graphics object `id` in `store`, when it
/// reads. A palette-indexed picture is seen in its own palette.
#[must_use]
pub fn of_record(store: &RetailDatStore, id: DataId) -> Option<Fingerprint> {
    match pyreal_dat::divine_type(id)? {
        DbType::Setup | DbType::GfxObj => Fingerprint::of_model(store, id),
        _ => {
            let (w, h, bgra) = pixels(store, id, false)?;
            Some(Fingerprint::of_picture(w, h, &bgra))
        }
    }
}

/// Picture `id`'s size and texels, blue, green, red and alpha, rows from the top, decoded as the
/// engine decodes it in either era's files; `clip_map` keys a palette-indexed picture's first
/// colours to clear, as a surface that cuts out draws it.
#[must_use]
pub fn pixels(store: &RetailDatStore, id: DataId, clip_map: bool) -> Option<(u32, u32, Vec<u8>)> {
    let lookup = TextureLookup::new(store, 1);
    let (rsid, rs, bytes) = lookup.resolve(id).ok()?;
    let payload = rs.payload(&bytes)?;
    let format = PixelFormatId::from_raw(rs.format);
    let classic = lookup.era_of(rsid) == ContainerEra::Classic;
    let palette = rs
        .default_palette_id
        .and_then(|p| lookup.palette(p).ok())
        .and_then(|p| ExpandedPalette::from_dat(&p.colors_argb));
    let pic = DatPicture {
        format,
        width: rs.width,
        height: rs.height,
        payload,
        classic,
    };
    let data = decode_dat_picture(pic, palette.as_ref(), clip_map).ok()?;
    let data = if data.format == TextureFormat::Bgra8 {
        data
    } else {
        decode_block_chain(&data).ok()?
    };
    Some((data.width, data.height, data.levels.into_iter().next()?))
}

#[cfg(test)]
mod sweep {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use pyreal_assets::texture_lookup::TextureLookup;
    use pyreal_codecs::pixel_format::{PixelFormatDesc, PixelFormatId};
    use pyreal_dat::{ContainerEra, DbType, RetailDatStore};

    fn dir(var: &str) -> Option<PathBuf> {
        std::env::var_os(var)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }

    /// Every picture of every era decodes to texels the size its header gives, and every header
    /// but raw JPEG's carries the byte count its format and extent require. Ignored: run with
    /// `--ignored picture_sweep`; the eras are `PYREAL_TEST_DAT_DIR`, `PYREAL_TEST_PRETOD_DAT_DIR`
    /// and `PYREAL_TEST_LAUNCH_DAT_DIR`.
    #[test]
    #[ignore = "decodes every picture of all three eras; slow"]
    fn picture_sweep_every_picture_of_every_era_decodes() {
        let mut eras = Vec::new();
        if let Some(d) = dir("PYREAL_TEST_DAT_DIR") {
            let s = RetailDatStore::open_dir(&d).expect("end of retail opens");
            let _ = s.grant_highres();
            eras.push(("end of retail", s));
        }
        for (name, var) in [
            ("february 2005", "PYREAL_TEST_PRETOD_DAT_DIR"),
            ("october 1999", "PYREAL_TEST_LAUNCH_DAT_DIR"),
        ] {
            if let Some(d) = dir(var) {
                eras.push((name, RetailDatStore::open_classic_dir(&d).expect("opens")));
            }
        }
        assert!(!eras.is_empty(), "no era's dats are set");
        let mut bad = Vec::new();
        for (name, s) in &eras {
            let lookup = TextureLookup::new(s, 1);
            // Before Throne of Destiny a `0x05` texture carries its own pixels.
            let mut ids = s.ids_of(DbType::RenderSurface);
            if s.era() == ContainerEra::Classic {
                ids.extend(s.ids_of(DbType::SurfaceTexture));
            }
            let mut formats: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
            for id in ids {
                let Ok((_, rs, _)) = lookup.resolve(id) else {
                    bad.push(format!("{name} {id}: does not resolve"));
                    continue;
                };
                let row = formats.entry(rs.format).or_insert((0, 0));
                row.0 += 1;
                let f = PixelFormatId::from_raw(rs.format);
                if f != PixelFormatId::CustomRawJpeg {
                    if let Some(d) = PixelFormatDesc::for_format(f) {
                        let want = u64::from(d.bits_per_pixel)
                            * u64::from(rs.width)
                            * u64::from(rs.height)
                            / 8;
                        if want != u64::from(rs.image_size) {
                            bad.push(format!(
                                "{name} {id}: {}x{} {f:?} holds {} bytes, the format needs {want}",
                                rs.width, rs.height, rs.image_size
                            ));
                        }
                    }
                }
                match super::pixels(s, id, false) {
                    Some((w, h, px)) if px.len() == w as usize * h as usize * 4 => {
                        if f != PixelFormatId::CustomRawJpeg && (w, h) != (rs.width, rs.height) {
                            bad.push(format!("{name} {id}: decoded {w}x{h}"));
                        }
                    }
                    Some((w, h, px)) => {
                        bad.push(format!("{name} {id}: {w}x{h} with {} bytes", px.len()));
                    }
                    None => {
                        row.1 += 1;
                        if row.1 <= 3 {
                            bad.push(format!(
                                "{name} {id}: {}x{} {f:?} did not decode",
                                rs.width, rs.height
                            ));
                        }
                    }
                }
            }
            eprintln!("{name}: format -> (count, failed) {formats:?}");
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }
}
