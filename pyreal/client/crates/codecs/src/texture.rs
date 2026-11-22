//! Texture decode and the format remap.
//!
//! Palette and surface records are described in `docs/formats/13-palette-and-surfaces.md`. This
//! module preserves the client's image copying.
//!
//! Everything here produces `pyreal_primitives::TextureData`, whose `Bgra8` is D3D9's `A8R8G8B8` in memory
//! order (B, G, R, A bytes) — the format the client's `X8R8G8B8`/`A8R8G8B8` surfaces already are.

use pyreal_primitives::{TextureData, TextureFormat};

use crate::palette::ExpandedPalette;
use crate::pixel_format::{PixelFormatDesc, PixelFormatId};
use crate::RenderError;

/// A source image, tagged by how it must be decoded.
#[derive(Debug, Clone, Copy)]
pub enum SourcePixels<'a> {
    /// A D3D format (not palettised or landscape): the client memcpys it row by row to D3D.
    Direct {
        format: PixelFormatId,
        bits: &'a [u8],
        pitch: usize,
    },
    /// `PFID_P8` — 8-bit palettised.
    Palettised8 {
        indices: &'a [u8],
        palette: &'a ExpandedPalette,
        clip_map: bool,
    },
    /// `PFID_INDEX16` — 16-bit palettised, indexing straight into the 2048-entry table.
    Palettised16 {
        indices: &'a [u16],
        palette: &'a ExpandedPalette,
        clip_map: bool,
    },
    /// `PFID_DXT1..DXT5` — copied verbatim; the GPU decodes them.
    Dxt {
        format: PixelFormatId,
        blocks: &'a [u8],
    },
    /// `PFID_CUSTOM_RAW_JPEG`.
    Jpeg(&'a [u8]),
    /// `PFID_CUSTOM_LSCAPE_R8G8B8` — 24-bit BGR-ordered landscape texture.
    LandscapeRgb(&'a [u8]),
    /// `PFID_CUSTOM_LSCAPE_ALPHA` — 8-bit alpha-only landscape mask.
    LandscapeAlpha(&'a [u8]),
}

/// Decode one source image into something the GPU accepts.
///
/// The block-compressed arm passes the blocks through untouched, which is what
/// the retail texture loader does ("copied verbatim ... no row loop, because DXT rows are 4-pixel
/// blocks"). Everything else lands as BGRA8.
///
/// # Errors
/// [`RenderError::ShortSourceData`] when the payload is smaller than the dimensions require,
/// [`RenderError::UnsupportedFormat`] for a format the format decoder rejects, and
/// [`RenderError::Jpeg`] for a stream the client itself would refuse.
pub fn decode_surface(
    src: SourcePixels<'_>,
    width: u32,
    height: u32,
) -> Result<TextureData, RenderError> {
    // **`PFID_CUSTOM_RAW_JPEG` is the one format whose header carries no extent**, so its arm runs
    // before the zero-extent check and answers with the stream's own dimensions.
    //
    // In the JPEG variant described in [`docs/formats/14-textures.md`], `width`, `height` and
    // `bitsPerPixel` are **0 in the header** — they come from the JPEG. The record's width and
    // height fields are documented as "0 for `PFID_CUSTOM_RAW_JPEG`". The client agrees:
    // it sets the decoded bitmap width and height **from the
    // JPEG** rather than from the record. All 79 shipped JPEG surfaces have a 0×0 header, so
    // requiring the header to match the stream rejected every one of them — and requiring a
    // non-zero extent rejected them before the arm was even reached.
    //
    // `0x06007576`, a character-management background, is one such JPEG surface.
    if let SourcePixels::Jpeg(bytes) = src {
        let img = crate::jpeg::decode(bytes)?;
        // A header that *does* carry an extent must still agree with the stream; the client would
        // create a surface of one size and blit another into it.
        if (width != 0 && width != img.width) || (height != 0 && height != img.height) {
            return Err(RenderError::Jpeg(format!(
                "stream is {}x{}; the surface header says {width}x{height}",
                img.width, img.height
            )));
        }
        let n = img.width as usize * img.height as usize;
        // The decode lands as 24-bit BGR (PFID_R8G8B8); widen to BGRA with an opaque alpha.
        let mut out = vec![0u8; n * 4];
        for (i, px) in img.bgr.as_chunks::<3>().0.iter().enumerate() {
            out[i * 4] = px[0];
            out[i * 4 + 1] = px[1];
            out[i * 4 + 2] = px[2];
            out[i * 4 + 3] = 0xFF;
        }
        return Ok(TextureData {
            width: img.width,
            height: img.height,
            format: TextureFormat::Bgra8,
            levels: vec![out],
        });
    }

    if width == 0 || height == 0 {
        return Err(RenderError::BadDimensions {
            width,
            height,
            reason: "a surface must have a non-zero extent",
        });
    }
    let n = width as usize * height as usize;

    let (format, bits) = match src {
        SourcePixels::Dxt { format, blocks } => {
            let desc = PixelFormatDesc::for_format(format)
                .ok_or(RenderError::UnsupportedFormat(format))?;
            if !desc.is_compressed() {
                return Err(RenderError::UnsupportedFormat(format));
            }
            let expected = desc.image_bytes(width, height);
            if blocks.len() < expected {
                return Err(RenderError::ShortSourceData {
                    format,
                    width,
                    height,
                    expected,
                    actual: blocks.len(),
                });
            }
            let tf = match format {
                PixelFormatId::Dxt1 => TextureFormat::Bc1,
                PixelFormatId::Dxt2 => TextureFormat::Bc2Premultiplied,
                PixelFormatId::Dxt3 => TextureFormat::Bc2,
                PixelFormatId::Dxt4 => TextureFormat::Bc3Premultiplied,
                PixelFormatId::Dxt5 => TextureFormat::Bc3,
                _ => unreachable!("compressed source descriptor only accepts DXT1-5"),
            };
            return Ok(TextureData {
                width,
                height,
                format: tf,
                levels: vec![blocks[..expected].to_vec()],
            });
        }
        // Handled above: a JPEG surface's extent comes from the stream, not from the header.
        SourcePixels::Jpeg(_) => unreachable!("the JPEG arm returns before the format match"),
        SourcePixels::Palettised8 {
            indices,
            palette,
            clip_map,
        } => {
            if indices.len() < n {
                return Err(RenderError::ShortSourceData {
                    format: PixelFormatId::P8,
                    width,
                    height,
                    expected: n,
                    actual: indices.len(),
                });
            }
            let mut out = vec![0u8; n * 4];
            for (i, idx) in indices[..n].iter().enumerate() {
                let argb = palette.expand(u16::from(*idx), clip_map);
                out[i * 4..i * 4 + 4].copy_from_slice(&argb.to_le_bytes());
            }
            (TextureFormat::Bgra8, out)
        }
        SourcePixels::Palettised16 {
            indices,
            palette,
            clip_map,
        } => {
            if indices.len() < n {
                return Err(RenderError::ShortSourceData {
                    format: PixelFormatId::Index16,
                    width,
                    height,
                    expected: n * 2,
                    actual: indices.len() * 2,
                });
            }
            let mut out = vec![0u8; n * 4];
            for (i, idx) in indices[..n].iter().enumerate() {
                let argb = palette.expand(*idx, clip_map);
                out[i * 4..i * 4 + 4].copy_from_slice(&argb.to_le_bytes());
            }
            (TextureFormat::Bgra8, out)
        }
        SourcePixels::LandscapeRgb(bits) => {
            // PFID_CUSTOM_LSCAPE_R8G8B8 shares R8G8B8's descriptor: 24 bits, R = 0x00FF0000, so the
            // bytes are B, G, R. The loader expands it to X8R8G8B8.
            let expected = n * 3;
            if bits.len() < expected {
                return Err(RenderError::ShortSourceData {
                    format: PixelFormatId::CustomLscapeR8G8B8,
                    width,
                    height,
                    expected,
                    actual: bits.len(),
                });
            }
            let mut out = vec![0u8; n * 4];
            for (i, px) in bits[..expected].as_chunks::<3>().0.iter().enumerate() {
                out[i * 4] = px[0];
                out[i * 4 + 1] = px[1];
                out[i * 4 + 2] = px[2];
                out[i * 4 + 3] = 0xFF;
            }
            (TextureFormat::Bgra8, out)
        }
        SourcePixels::LandscapeAlpha(bits) => {
            // PFID_CUSTOM_LSCAPE_ALPHA has the same flags/bpp as PFID_A8: alpha only.
            if bits.len() < n {
                return Err(RenderError::ShortSourceData {
                    format: PixelFormatId::CustomLscapeAlpha,
                    width,
                    height,
                    expected: n,
                    actual: bits.len(),
                });
            }
            let mut out = vec![0u8; n * 4];
            for (i, a) in bits[..n].iter().enumerate() {
                out[i * 4 + 3] = *a;
            }
            (TextureFormat::Bgra8, out)
        }
        SourcePixels::Direct {
            format,
            bits,
            pitch,
        } => {
            let desc = PixelFormatDesc::for_format(format)
                .ok_or(RenderError::UnsupportedFormat(format))?;
            let row = desc.row_bytes(width);
            let pitch = if pitch == 0 { row } else { pitch };
            // An extent too large to count cannot be held either.
            let expected = pitch
                .checked_mul(height as usize - 1)
                .and_then(|n| n.checked_add(row))
                .unwrap_or(usize::MAX);
            if bits.len() < expected {
                return Err(RenderError::ShortSourceData {
                    format,
                    width,
                    height,
                    expected,
                    actual: bits.len(),
                });
            }
            let mut out = vec![0u8; n * 4];
            for y in 0..height as usize {
                let src_row = &bits[y * pitch..y * pitch + row];
                for x in 0..width as usize {
                    let argb = read_pixel(&desc, src_row, x)?;
                    let d = (y * width as usize + x) * 4;
                    out[d..d + 4].copy_from_slice(&argb.to_le_bytes());
                }
            }
            (TextureFormat::Bgra8, out)
        }
    };

    Ok(TextureData {
        width,
        height,
        format,
        levels: vec![bits],
    })
}

/// The pixel unpack: read one pixel through the source
/// descriptor's masks and rescale each channel to 8 bits.
///
/// The client's rescale is "unpack with the source masks/offsets/max, rescale each channel to the
/// destination's max, repack". For an 8-bit destination that is `value * 255 / max`, which for the
/// power-of-two channel widths in the table is exactly bit replication.
fn read_pixel(desc: &PixelFormatDesc, row: &[u8], x: usize) -> Result<u32, RenderError> {
    let bpp = desc.bits_per_pixel;
    let raw: u32 = match bpp {
        8 => u32::from(row[x]),
        16 => u32::from(u16::from_le_bytes([row[x * 2], row[x * 2 + 1]])),
        24 => {
            let b = &row[x * 3..x * 3 + 3];
            u32::from(b[0]) | (u32::from(b[1]) << 8) | (u32::from(b[2]) << 16)
        }
        32 => u32::from_le_bytes([row[x * 4], row[x * 4 + 1], row[x * 4 + 2], row[x * 4 + 3]]),
        _ => return Err(RenderError::UnsupportedFormat(desc.format)),
    };
    let chan = |mask: u32, default: u32| -> u32 {
        if mask == 0 {
            return default;
        }
        let shift = mask.trailing_zeros();
        let max = u64::from(mask >> shift);
        let v = u64::from((raw & mask) >> shift);
        // v * 255 / max, exact for every width in the table; widened for a 32-bit channel.
        u32::try_from((v * 255 + max / 2) / max).unwrap_or(255)
    };
    let r = chan(desc.red_mask, 0);
    let g = chan(desc.green_mask, 0);
    let b = chan(desc.blue_mask, 0);
    // A format with no alpha channel reads as fully opaque, which is what an X8 surface means.
    let a = chan(desc.alpha_mask, 255);
    Ok((a << 24) | (r << 16) | (g << 8) | b)
}

/// The byte size of a 4x4 block, or `None` for an uncompressed or unsupported format.
#[must_use]
pub const fn block_bytes(format: TextureFormat) -> Option<usize> {
    match format {
        TextureFormat::Bc1 => Some(8),
        TextureFormat::Bc2
        | TextureFormat::Bc2Premultiplied
        | TextureFormat::Bc3
        | TextureFormat::Bc3Premultiplied => Some(16),
        _ => None,
    }
}

/// Decode a block-compressed chain to BGRA8 on the CPU, for a device with no BC support.
///
/// # Errors
/// Rejects unsupported formats and source levels shorter than their declared extent.
pub fn decode_block_chain(t: &TextureData) -> Result<TextureData, RenderError> {
    let id = crate::texture::block_source_format(t.format)
        .ok_or(RenderError::Unsupported("a texture format with no decode"))?;
    let (mut w, mut h) = (t.width, t.height);
    let mut levels = Vec::with_capacity(t.levels.len());
    for bits in &t.levels {
        levels.push(crate::dxt::decode(id, bits, w, h)?);
        w = crate::mip::half(w);
        h = crate::mip::half(h);
    }
    Ok(TextureData {
        width: t.width,
        height: t.height,
        format: TextureFormat::Bgra8,
        levels,
    })
}

/// A dat `RenderSurface`'s stored picture: its format, size and payload, and whether it is from
/// the files before Throne of Destiny.
#[derive(Debug, Clone, Copy)]
pub struct DatPicture<'a> {
    pub format: PixelFormatId,
    pub width: u32,
    pub height: u32,
    pub payload: &'a [u8],
    pub classic: bool,
}

/// Why a dat picture would not decode.
#[derive(Debug)]
pub enum PictureError {
    /// A palette-indexed picture with no palette, or colour planes shorter than the picture.
    Incomplete,
    /// The payload itself would not decode.
    Decode(RenderError),
}

/// A dat picture decoded as the client draws it, in either era's files: `palette` for a
/// palette-indexed one, `clip_map` keying its first colours clear. DXT blocks stay blocks.
///
/// # Errors
/// [`PictureError`].
pub fn decode_dat_picture(
    pic: DatPicture<'_>,
    palette: Option<&ExpandedPalette>,
    clip_map: bool,
) -> Result<TextureData, PictureError> {
    let DatPicture {
        format,
        width,
        height,
        payload,
        classic,
    } = pic;
    // A 256-colour picture of the older files indexes its 256-entry palette, which the expanded
    // palette holds eight times over: colour `i` is entry `8i`. `INDEX16`'s indices are widened
    // into an owned buffer, which `Palettised16` borrows.
    if format == PixelFormatId::Index16 || (format == PixelFormatId::P8 && classic) {
        let palette = palette.ok_or(PictureError::Incomplete)?;
        let indices: Vec<u16> = if format == PixelFormatId::P8 {
            payload.iter().map(|&i| u16::from(i) << 3).collect()
        } else {
            payload
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect()
        };
        let src = SourcePixels::Palettised16 {
            indices: &indices,
            palette,
            clip_map,
        };
        return decode_surface(src, width, height).map_err(PictureError::Decode);
    }
    // The older files store a landscape picture as three colour planes.
    if format == PixelFormatId::CustomLscapeR8G8B8 && classic {
        let bgr = interleave_rgb_planes(payload, width, height).ok_or(PictureError::Incomplete)?;
        return decode_surface(SourcePixels::LandscapeRgb(&bgr), width, height)
            .map_err(PictureError::Decode);
    }
    let src = match format {
        PixelFormatId::Dxt1
        | PixelFormatId::Dxt2
        | PixelFormatId::Dxt3
        | PixelFormatId::Dxt4
        | PixelFormatId::Dxt5 => SourcePixels::Dxt {
            format,
            blocks: payload,
        },
        PixelFormatId::CustomRawJpeg => SourcePixels::Jpeg(payload),
        PixelFormatId::CustomLscapeR8G8B8 => SourcePixels::LandscapeRgb(payload),
        PixelFormatId::CustomLscapeAlpha => SourcePixels::LandscapeAlpha(payload),
        PixelFormatId::P8 => SourcePixels::Palettised8 {
            indices: payload,
            palette: palette.ok_or(PictureError::Incomplete)?,
            clip_map,
        },
        other => SourcePixels::Direct {
            format: other,
            bits: payload,
            pitch: 0,
        },
    };
    decode_surface(src, width, height).map_err(PictureError::Decode)
}

/// Three colour planes, red, green then blue (the older files' landscape pictures), as one
/// picture of blue, green, red pixels; `None` when the planes are short.
#[must_use]
pub fn interleave_rgb_planes(planes: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let n = width as usize * height as usize;
    let (r, rest) = planes.split_at_checked(n)?;
    let (g, rest) = rest.split_at_checked(n)?;
    let b = rest.get(..n)?;
    let mut out = Vec::with_capacity(n * 3);
    for i in 0..n {
        out.extend_from_slice(&[b[i], g[i], r[i]]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    /// Three planes become one picture whose pixels are blue, green, red, each taken from the
    /// same place in the three planes; short planes are refused.
    #[test]
    fn three_colour_planes_interleave_into_blue_green_red_pixels() {
        let planes = [1, 2, 10, 20, 100, 200];
        assert_eq!(
            super::interleave_rgb_planes(&planes, 2, 1),
            Some(vec![100, 10, 1, 200, 20, 2])
        );
        assert_eq!(super::interleave_rgb_planes(&planes[..5], 2, 1), None);
    }

    #[test]
    fn block_chain_decode_keeps_cropped_levels_and_premultiplied_formats() {
        let colour = [0, 248, 0, 0, 0, 0, 0, 0];
        for (format, alpha, bytes) in [
            (TextureFormat::Bc1, vec![], 8),
            (TextureFormat::Bc2, vec![255; 8], 16),
            (TextureFormat::Bc2Premultiplied, vec![255; 8], 16),
            (TextureFormat::Bc3, vec![255, 255, 0, 0, 0, 0, 0, 0], 16),
            (
                TextureFormat::Bc3Premultiplied,
                vec![255, 255, 0, 0, 0, 0, 0, 0],
                16,
            ),
        ] {
            assert_eq!(block_bytes(format), Some(bytes));
            let block: Vec<_> = alpha.into_iter().chain(colour).collect();
            let source = TextureData {
                width: 5,
                height: 3,
                format,
                levels: vec![block.repeat(2), block.clone(), block],
            };
            let decoded = decode_block_chain(&source).unwrap();
            assert_eq!(
                (decoded.width, decoded.height, decoded.format),
                (5, 3, TextureFormat::Bgra8)
            );
            assert_eq!(
                decoded.levels,
                [15, 2, 1].map(|pixels| [0, 0, 255, 255].repeat(pixels))
            );
            assert_eq!(source.format, format);
        }
    }

    #[test]
    fn block_chain_decode_reports_the_first_short_level_and_refuses_uncompressed_input() {
        let mut source = TextureData {
            width: 5,
            height: 3,
            format: TextureFormat::Bc1,
            levels: vec![vec![0; 16], vec![]],
        };
        assert!(matches!(
            decode_block_chain(&source),
            Err(RenderError::ShortSourceData {
                width: 2,
                height: 1,
                expected: 8,
                actual: 0,
                ..
            })
        ));
        source.format = TextureFormat::Bgra8;
        assert_eq!(block_bytes(source.format), None);
        assert!(matches!(
            decode_block_chain(&source),
            Err(RenderError::Unsupported(_))
        ));
    }
    use super::*;

    /// A header whose extent overflows the byte count is short data, not an arithmetic panic.
    #[test]
    fn an_extent_too_large_to_count_is_short_data() {
        let r = std::panic::catch_unwind(|| {
            decode_surface(
                SourcePixels::Direct {
                    format: PixelFormatId::A8R8G8B8,
                    bits: &[0; 16],
                    pitch: 0,
                },
                u32::MAX,
                u32::MAX,
            )
            .is_err()
        });
        assert_eq!(r.ok(), Some(true));
    }

    /// A 32-bit depth texel, whose one channel fills the dword, unpacks without overflowing.
    #[test]
    fn a_full_dword_channel_unpacks_without_overflow() {
        let r = std::panic::catch_unwind(|| {
            decode_surface(
                SourcePixels::Direct {
                    format: PixelFormatId::D32,
                    bits: &[0xFF; 4],
                    pitch: 0,
                },
                1,
                1,
            )
            .map(|t| t.levels[0].clone())
            .ok()
        });
        assert_eq!(r.ok().flatten(), Some(vec![0, 0, 0, 0xFF]));
    }

    /// The JPEG variant in [`docs/formats/14-textures.md`] stores zero header `width`,
    /// `height` and `bitsPerPixel`; they come from the JPEG. This matches the field
    /// table's "0 for `PFID_CUSTOM_RAW_JPEG`" rows at +0x08 and +0x0C and the worked example
    /// `0x06005F18` ("header `width = 0, height = 0, format = 500`"). All 79 shipped JPEG surfaces
    /// have a 0×0 header.
    ///
    /// An arm that required the header to equal the stream, or ran only after a non-zero-extent
    /// check, would decode **no retail JPEG surface at all**. The live counterpart is
    /// `pyreal_scene::textures`'s `the_retail_jpeg_surface_the_character_screen_uses_decodes`,
    /// which reads `0x06007576` out of `client_portal.dat`.
    #[test]
    fn a_jpeg_surface_takes_its_extent_from_the_stream_because_the_header_carries_none() {
        let bytes = crate::jpeg::tests::bytes(crate::jpeg::tests::RED_8X8_BASELINE);
        let t = decode_surface(SourcePixels::Jpeg(&bytes), 0, 0)
            .expect("a 0x0 header is the shipped form and must decode");
        assert_eq!(
            (t.width, t.height),
            (8, 8),
            "the extent comes from the stream"
        );
        assert_eq!(t.format, TextureFormat::Bgra8);
        assert_eq!(t.levels[0].len(), 8 * 8 * 4);

        // A header that *does* carry an extent must still agree with the stream: the client would
        // otherwise create a surface of one size and blit another into it.
        assert!(decode_surface(SourcePixels::Jpeg(&bytes), 8, 8).is_ok());
        assert!(matches!(
            decode_surface(SourcePixels::Jpeg(&bytes), 16, 16),
            Err(RenderError::Jpeg(_))
        ));
    }

    fn ramp_palette() -> ExpandedPalette {
        let src: Vec<u32> = (0..256u32)
            .map(|i| 0xFF00_0000 | (i << 16) | (i << 8) | i)
            .collect();
        ExpandedPalette::from_dat(&src).unwrap()
    }

    // Oracle: the client's indexed-image copy --
    //   idx = (src format == PFID_INDEX16) ? ((uint16*)srcRow)[x] : ((uint8*)srcRow)[x]
    //   dst32[x] = (bClipMap && idx <= 7) ? 0x00000000 : palette->ARGB[idx]
    // plus the 8x replication during palette loading. Getting the replication wrong "shifts every
    // colour in every indexed texture".
    #[test]
    fn palettised_sources_expand_through_the_2048_entry_table() {
        let pal = ramp_palette();
        // `PixelFormatId::P8` indexes the *expanded* table directly: the texel copy reads
        //   idx = ((uint8*)srcRow)[x]; dst32[x] = palette->ARGB[idx]
        // with no scaling, so an 8-bit index can only reach entries 0..255 -- which, after the 8x
        // replication, is the first 32 authored colours, each of them eight times. That is a
        // consequence of the two documented behaviours together, not an extra rule, and it is why
        // the game's own index images are INDEX16 rather than `PixelFormatId::P8`.
        let indices: Vec<u8> = vec![0, 1, 8, 9, 255];
        let t = decode_surface(
            SourcePixels::Palettised8 {
                indices: &indices,
                palette: &pal,
                clip_map: false,
            },
            5,
            1,
        )
        .unwrap();
        assert_eq!(t.format, TextureFormat::Bgra8);
        assert_eq!(t.levels[0].len(), 20);
        // BGRA order: entry 0xFF010101 is B=1, G=1, R=1, A=255.
        assert_eq!(&t.levels[0][0..4], &[0, 0, 0, 255], "index 0 -> slot 0");
        assert_eq!(
            &t.levels[0][4..8],
            &[0, 0, 0, 255],
            "index 1 still lands in slot 0"
        );
        assert_eq!(
            &t.levels[0][8..12],
            &[1, 1, 1, 255],
            "index 8 is the first of slot 1"
        );
        assert_eq!(
            &t.levels[0][12..16],
            &[1, 1, 1, 255],
            "index 9 shares slot 1"
        );
        assert_eq!(
            &t.levels[0][16..20],
            &[31, 31, 31, 255],
            "index 255 -> slot 31"
        );

        // INDEX16 goes straight into the 2048-entry table: index 8 is the *second* source colour,
        // and index 9 is the same colour again (the 8x replication).
        let indices: Vec<u16> = vec![0, 7, 8, 9, 2047, 16];
        let t = decode_surface(
            SourcePixels::Palettised16 {
                indices: &indices,
                palette: &pal,
                clip_map: false,
            },
            6,
            1,
        )
        .unwrap();
        assert_eq!(&t.levels[0][0..4], &[0, 0, 0, 255], "index 0");
        assert_eq!(&t.levels[0][4..8], &[0, 0, 0, 255], "index 7 shares slot 0");
        assert_eq!(&t.levels[0][8..12], &[1, 1, 1, 255], "index 8 is slot 1");
        assert_eq!(
            &t.levels[0][12..16],
            &[1, 1, 1, 255],
            "index 9 shares slot 1"
        );
        assert_eq!(
            &t.levels[0][16..20],
            &[255, 255, 255, 255],
            "index 2047 is slot 255"
        );
        assert_eq!(&t.levels[0][20..24], &[2, 2, 2, 255], "index 16 is slot 2");
    }

    // Oracle: the same function's clip-map term -- "Palette indices 0-7 are the transparent range
    // for clip-mapped surfaces ... which is what the alpha test (reference 100/255) then cuts away."
    #[test]
    fn a_clip_map_makes_indices_zero_to_seven_transparent_black() {
        let pal = ramp_palette();
        let indices: Vec<u16> = vec![0, 7, 8];
        let t = decode_surface(
            SourcePixels::Palettised16 {
                indices: &indices,
                palette: &pal,
                clip_map: true,
            },
            3,
            1,
        )
        .unwrap();
        assert_eq!(&t.levels[0][0..4], &[0, 0, 0, 0]);
        assert_eq!(&t.levels[0][4..8], &[0, 0, 0, 0]);
        assert_eq!(&t.levels[0][8..12], &[1, 1, 1, 255], "index 8 survives");
        // The same data without the clip-map flag keeps index 0 as an ordinary colour with alpha.
        let t = decode_surface(
            SourcePixels::Palettised16 {
                indices: &indices,
                palette: &pal,
                clip_map: false,
            },
            3,
            1,
        )
        .unwrap();
        assert_eq!(t.levels[0][3], 255);
    }

    // The original direct-format path copies source bits into the locked surface. Here those
    // pixels are widened to BGRA8 for a modern GPU, using the observed channel masks.
    #[test]
    fn direct_formats_decode_through_their_documented_masks() {
        // A1R5G5B5: alpha 0x8000, red 0x7C00, green 0x03E0, blue 0x001F.
        // alpha 0x8000 | red 31 | green 0 | blue 31 -- opaque magenta.
        let px: u16 = 0x8000 | (31 << 10) | 31;
        let t = decode_surface(
            SourcePixels::Direct {
                format: PixelFormatId::A1R5G5B5,
                bits: &px.to_le_bytes(),
                pitch: 0,
            },
            1,
            1,
        )
        .unwrap();
        assert_eq!(&t.levels[0][..4], &[255, 0, 255, 255]);

        // X1R5G5B5 has no alpha mask, so it reads as fully opaque even with the top bit clear.
        let px: u16 = 31 << 10;
        let t = decode_surface(
            SourcePixels::Direct {
                format: PixelFormatId::X1R5G5B5,
                bits: &px.to_le_bytes(),
                pitch: 0,
            },
            1,
            1,
        )
        .unwrap();
        assert_eq!(t.levels[0][3], 255, "an X-format surface is opaque");
        assert_eq!(t.levels[0][2], 255, "red is saturated");

        // A8R8G8B8 round-trips byte for byte, because Bgra8 *is* A8R8G8B8's memory order.
        let src = [0x11u8, 0x22, 0x33, 0x44]; // B, G, R, A
        let t = decode_surface(
            SourcePixels::Direct {
                format: PixelFormatId::A8R8G8B8,
                bits: &src,
                pitch: 0,
            },
            1,
            1,
        )
        .unwrap();
        assert_eq!(&t.levels[0][..4], &src);

        // A8 is alpha-only: RGB reads as zero.
        let t = decode_surface(
            SourcePixels::Direct {
                format: PixelFormatId::A8,
                bits: &[0x7F],
                pitch: 0,
            },
            1,
            1,
        )
        .unwrap();
        assert_eq!(&t.levels[0][..4], &[0, 0, 0, 0x7F]);

        // R5G6B5's green channel is six bits: 0x3F must reach 255, not 252.
        let px: u16 = 0x07E0;
        let t = decode_surface(
            SourcePixels::Direct {
                format: PixelFormatId::R5G6B5,
                bits: &px.to_le_bytes(),
                pitch: 0,
            },
            1,
            1,
        )
        .unwrap();
        assert_eq!(t.levels[0][1], 255);
    }

    // Oracle: the same table -- "row by row when the source pitch differs from the destination
    // pitch". A surface whose rows are padded must not smear.
    #[test]
    fn a_source_pitch_wider_than_the_row_is_honoured() {
        // Two rows of two A8 pixels each, with two bytes of padding per row.
        let bits = [1u8, 2, 0xEE, 0xEE, 3, 4, 0xEE, 0xEE];
        let t = decode_surface(
            SourcePixels::Direct {
                format: PixelFormatId::A8,
                bits: &bits,
                pitch: 4,
            },
            2,
            2,
        )
        .unwrap();
        let alphas: Vec<u8> = t.levels[0]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3])
            .collect();
        assert_eq!(alphas, vec![1, 2, 3, 4]);
    }

    // The observed landscape RGB format is 24-bit BGR (red bit mask = 0x00FF0000), expanded
    // to X8R8G8B8. The landscape alpha format is an 8-bit alpha-only mask with the same
    // flags and bit depth as PFID_A8.
    #[test]
    fn the_landscape_formats_decode_as_their_documented_twins() {
        let bits = [0x10u8, 0x20, 0x30, 0x40, 0x50, 0x60];
        let t = decode_surface(SourcePixels::LandscapeRgb(&bits), 2, 1).unwrap();
        assert_eq!(&t.levels[0][..4], &[0x10, 0x20, 0x30, 0xFF]);
        assert_eq!(&t.levels[0][4..8], &[0x40, 0x50, 0x60, 0xFF]);

        let t = decode_surface(SourcePixels::LandscapeAlpha(&[0x00, 0x80, 0xFF]), 3, 1).unwrap();
        let alphas: Vec<u8> = t.levels[0]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3])
            .collect();
        assert_eq!(alphas, vec![0x00, 0x80, 0xFF]);
        // ..and the colour channels stay zero, as an alpha-only surface has no colour.
        assert_eq!(&t.levels[0][..3], &[0, 0, 0]);
    }

    // Oracle: the retail texture loader's DXT row -- "copied verbatim (flags & 4: the whole image
    // size in one block copy; no row loop)". The renderer must pass the blocks to the GPU, not
    // expand them.
    #[test]
    fn dxt_sources_pass_through_as_blocks() {
        let blocks = vec![0xABu8; 8];
        let t = decode_surface(
            SourcePixels::Dxt {
                format: PixelFormatId::Dxt1,
                blocks: &blocks,
            },
            4,
            4,
        )
        .unwrap();
        assert_eq!(t.format, TextureFormat::Bc1);
        assert_eq!(t.levels[0], blocks);
        // DXT2 retains source premultiplication; layout/bytes stay BC2. DXT4/5
        // provenance remains a separate BC3 follow-up, unchanged here.
        let blocks: Vec<u8> = (0..16).collect();
        for (f, want) in [
            (PixelFormatId::Dxt2, TextureFormat::Bc2Premultiplied),
            (PixelFormatId::Dxt3, TextureFormat::Bc2),
            (PixelFormatId::Dxt4, TextureFormat::Bc3Premultiplied),
            (PixelFormatId::Dxt5, TextureFormat::Bc3),
        ] {
            let t = decode_surface(
                SourcePixels::Dxt {
                    format: f,
                    blocks: &blocks,
                },
                4,
                4,
            )
            .unwrap();
            assert_eq!(t.format, want, "{f:?}");
            assert_eq!(
                t.levels,
                std::slice::from_ref(&blocks),
                "{f:?} is never converted/recompressed here"
            );
        }
    }

    // Oracle: "Parsers return Result; they do not panic on malformed input, because
    // they will meet malformed input."
    #[test]
    fn short_or_impossible_input_is_an_error_not_a_panic() {
        let pal = ramp_palette();
        assert!(matches!(
            decode_surface(
                SourcePixels::Palettised8 {
                    indices: &[0],
                    palette: &pal,
                    clip_map: false
                },
                4,
                4
            ),
            Err(RenderError::ShortSourceData { .. })
        ));
        assert!(matches!(
            decode_surface(SourcePixels::LandscapeRgb(&[0, 0]), 4, 4),
            Err(RenderError::ShortSourceData { .. })
        ));
        assert!(matches!(
            decode_surface(
                SourcePixels::Direct {
                    format: PixelFormatId::Other(27),
                    bits: &[0; 64],
                    pitch: 0
                },
                4,
                4
            ),
            Err(RenderError::UnsupportedFormat(_))
        ));
        assert!(matches!(
            decode_surface(SourcePixels::LandscapeAlpha(&[]), 0, 4),
            Err(RenderError::BadDimensions { .. })
        ));
        // A non-compressed format offered on the Dxt arm is a caller error, not a decode.
        assert!(matches!(
            decode_surface(
                SourcePixels::Dxt {
                    format: PixelFormatId::A8,
                    blocks: &[0; 64]
                },
                4,
                4
            ),
            Err(RenderError::UnsupportedFormat(_))
        ));
    }
}

/// The source decoder for a block-compressed GPU format, preserving premultiplied alpha.
#[must_use]
pub const fn block_source_format(
    format: pyreal_primitives::TextureFormat,
) -> Option<crate::PixelFormatId> {
    use crate::PixelFormatId as P;
    use pyreal_primitives::TextureFormat as T;
    match format {
        T::Bc1 => Some(P::Dxt1),
        T::Bc2 => Some(P::Dxt3),
        T::Bc2Premultiplied => Some(P::Dxt2),
        T::Bc3 => Some(P::Dxt5),
        T::Bc3Premultiplied => Some(P::Dxt4),
        _ => None,
    }
}

#[cfg(test)]
mod block_format_tests {
    use super::*;
    #[test]
    fn compressed_formats_select_the_matching_dxt_alpha_convention() {
        use crate::PixelFormatId as P;
        use pyreal_primitives::TextureFormat as T;
        for (gpu, source) in [
            (T::Bc1, P::Dxt1),
            (T::Bc2, P::Dxt3),
            (T::Bc2Premultiplied, P::Dxt2),
            (T::Bc3, P::Dxt5),
            (T::Bc3Premultiplied, P::Dxt4),
        ] {
            assert_eq!(block_source_format(gpu), Some(source));
        }
        assert_eq!(block_source_format(T::Bgra8), None);
    }
}
