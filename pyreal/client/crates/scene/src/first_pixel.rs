//! The first-pixel picture: one retail surface decoded to RGBA, drawn over the whole frame while
//! no world is loaded.

use pyreal_assets::{Decode, RenderSurface};
use pyreal_codecs::pixel_format::PixelFormatId;
use pyreal_codecs::{decode_surface, SourcePixels};
use pyreal_primitives::{AssetSource, DataId};

/// Surface `id`'s first level as tightly packed RGBA rows, with its size.
///
/// # Errors
/// The surface is missing, will not decode, or is palettised (it needs a palette this picture
/// has none of).
pub fn decode(assets: &dyn AssetSource, id: DataId) -> Result<(u32, u32, Vec<u8>), String> {
    let bytes = assets
        .read(id)
        .map_err(|e| format!("reading {id:?}: {e}"))?;
    let rs = RenderSurface::decode_payload_in(assets.container_era_of(id), id, &bytes)
        .map_err(|e| format!("decoding {id:?}: {e}"))?;
    let payload = rs
        .payload(&bytes)
        .ok_or_else(|| format!("{id:?} has a truncated payload"))?;
    let format = PixelFormatId::from_raw(rs.format);
    let source = match format {
        PixelFormatId::Dxt1
        | PixelFormatId::Dxt2
        | PixelFormatId::Dxt3
        | PixelFormatId::Dxt4
        | PixelFormatId::Dxt5 => SourcePixels::Dxt {
            format,
            blocks: payload,
        },
        other => SourcePixels::Direct {
            format: other,
            bits: payload,
            pitch: 0,
        },
    };
    let decoded = decode_surface(source, rs.width, rs.height).map_err(|e| e.to_string())?;
    let levels =
        crate::gpu::rgba_levels(&decoded).ok_or_else(|| format!("{id:?} decodes to no picture"))?;
    let first = levels
        .into_iter()
        .next()
        .ok_or_else(|| format!("{id:?} has no level"))?;
    Ok((decoded.width, decoded.height, first))
}
