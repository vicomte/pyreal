//! Dat textures as GPU-ready pixels: the record chain resolved, the picture decoded with
//! `render-cpu`'s codecs, DXT kept as BC blocks, everything else as BGRA8, and a mip chain.

use pyreal_assets::texture_lookup::{LookupError, TextureLookup};
use pyreal_assets::RenderSurface;
use pyreal_codecs::palette::ExpandedPalette;
use pyreal_codecs::pixel_format::PixelFormatId;
use pyreal_codecs::texture::{decode_dat_picture, DatPicture, PictureError};
use pyreal_dat::{ContainerEra, RetailDatStore};
use pyreal_primitives::{DataId, TextureData, TextureFormat};

/// Why a texture did not decode.
#[derive(Debug, thiserror::Error)]
pub enum TextureError {
    #[error(transparent)]
    Lookup(#[from] LookupError),
    #[error("{0}: {1}")]
    Decode(DataId, pyreal_codecs::RenderError),
    #[error("{0} has no pixels the engine can draw")]
    Unusable(DataId),
}

/// How the pixels are laid out on the GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelLayout {
    /// Blue, green, red, alpha; colour in sRGB.
    Bgra8,
    /// DXT1, as BC1 blocks.
    Bc1,
    /// DXT2 and DXT3, as BC2 blocks.
    Bc2,
    /// DXT4 and DXT5, as BC3 blocks.
    Bc3,
    /// An art pack's pictures: red, green, blue, alpha as data, or colour when `srgb`.
    Rgba8 { srgb: bool },
    /// Red alone (a pack's masks).
    Bc4,
    /// Red and green as data (a pack's normals).
    Bc5,
    /// All four channels, colour when `srgb`.
    Bc7 { srgb: bool },
}

impl PixelLayout {
    #[must_use]
    pub fn wgpu_format(self) -> wgpu::TextureFormat {
        use wgpu::TextureFormat as F;
        match self {
            Self::Bgra8 => F::Bgra8UnormSrgb,
            Self::Bc1 => F::Bc1RgbaUnormSrgb,
            Self::Bc2 => F::Bc2RgbaUnormSrgb,
            Self::Bc3 => F::Bc3RgbaUnormSrgb,
            Self::Rgba8 { srgb: true } => F::Rgba8UnormSrgb,
            Self::Rgba8 { srgb: false } => F::Rgba8Unorm,
            Self::Bc4 => F::Bc4RUnorm,
            Self::Bc5 => F::Bc5RgUnorm,
            Self::Bc7 { srgb: true } => F::Bc7RgbaUnormSrgb,
            Self::Bc7 { srgb: false } => F::Bc7RgbaUnorm,
        }
    }

    /// Bytes per 4x4 block, or per texel for [`Self::Bgra8`] and [`Self::Rgba8`].
    #[must_use]
    pub fn block_bytes(self) -> u32 {
        match self {
            Self::Bgra8 | Self::Rgba8 { .. } => 4,
            Self::Bc1 | Self::Bc4 => 8,
            Self::Bc2 | Self::Bc3 | Self::Bc5 | Self::Bc7 { .. } => 16,
        }
    }

    #[must_use]
    pub fn is_block(self) -> bool {
        !matches!(self, Self::Bgra8 | Self::Rgba8 { .. })
    }
}

/// A decoded picture and its mips, level 0 first.
#[derive(Debug, Clone, PartialEq)]
pub struct TexturePixels {
    pub width: u32,
    pub height: u32,
    pub layout: PixelLayout,
    pub levels: Vec<Vec<u8>>,
}

impl TexturePixels {
    /// A one-texel picture.
    #[must_use]
    pub fn solid(bgra: [u8; 4]) -> Self {
        Self {
            width: 1,
            height: 1,
            layout: PixelLayout::Bgra8,
            levels: vec![bgra.to_vec()],
        }
    }

    /// The byte size of every level together.
    #[must_use]
    pub fn byte_size(&self) -> usize {
        self.levels.iter().map(Vec::len).sum()
    }
}

/// A palette built for one variant of a picture: `base` with ranges of it refilled from other
/// palettes, each `(palette, first, count)`. This is a clothing or creature tint, and the
/// palette-shift ground of the dats before Throne of Destiny.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PaletteSwap {
    pub base: DataId,
    pub ranges: Vec<(DataId, u32, u32)>,
    /// Ranges in the wire's eight-entry units of the expanded palette, a count of 0 the whole
    /// (an object description's); otherwise in the base palette's own entries (the ground's).
    pub wire: bool,
}

/// High-Res Textures: whether a two-level image draws its `client_highres.dat` level where the
/// store has that file open. Set before a dat set's first texture is made; on by default.
static HIGH_RES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Sets High-Res Textures ([`high_res_textures`]) for the textures made after it.
pub fn set_high_res_textures(on: bool) {
    HIGH_RES.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Whether a two-level image draws its high-res level where the store has the file.
#[must_use]
pub fn high_res_textures() -> bool {
    HIGH_RES.load(std::sync::atomic::Ordering::Relaxed)
}

/// The texture detail the lookups are made at: 0, the highest, with High-Res Textures on.
#[must_use]
pub fn texture_detail() -> u32 {
    u32::from(!high_res_textures())
}

/// The texture records of one dat set, decoded.
#[derive(Debug)]
pub struct TextureSource<'a> {
    lookup: TextureLookup<'a>,
}

impl<'a> TextureSource<'a> {
    /// A two-level image resolves to its high-res level where the store has `client_highres.dat`
    /// and High-Res Textures is on, and to its original art otherwise.
    #[must_use]
    pub fn new(store: &'a RetailDatStore) -> Self {
        Self {
            lookup: TextureLookup::new(store, texture_detail()),
        }
    }

    /// The record chain's end: the picture's id, header and record bytes.
    ///
    /// # Errors
    /// A missing or malformed record in the chain.
    pub fn resolve(&self, id: DataId) -> Result<(DataId, RenderSurface, Vec<u8>), TextureError> {
        Ok(self.lookup.resolve(id)?)
    }

    /// One palette record's colours, as the dat holds them.
    #[must_use]
    pub fn palette_colours(&self, id: DataId) -> Option<Vec<u32>> {
        self.lookup.palette(id).ok().map(|p| p.colors_argb)
    }

    /// The palette `swap` describes, or `None` when its base will not read. Each range is read
    /// from the same entries of its palette, both expanded to 2,048 entries; a range past the
    /// end is refused whole, as the client's `Modify` refuses it.
    #[must_use]
    pub fn compose(&self, swap: &PaletteSwap) -> Option<ExpandedPalette> {
        if !swap.wire {
            let mut colours = self.palette_colours(swap.base)?;
            for &(id, first, count) in &swap.ranges {
                let Some(src) = self.palette_colours(id) else {
                    continue;
                };
                let start = first as usize;
                let end = (start + count as usize).min(colours.len()).min(src.len());
                if start < end {
                    colours[start..end].copy_from_slice(&src[start..end]);
                }
            }
            return ExpandedPalette::from_dat(&colours);
        }
        let mut out = ExpandedPalette::from_dat(&self.palette_colours(swap.base)?)?;
        for &(id, offset, length) in &swap.ranges {
            let Some(src) = self
                .palette_colours(id)
                .and_then(|c| ExpandedPalette::from_dat(&c))
            else {
                continue;
            };
            let start = offset as usize * pyreal_codecs::palette::PALETTE_REPLICATION;
            if let (Some(tail), Ok(o), Ok(l)) = (
                src.0.get(start..),
                u16::try_from(offset),
                u16::try_from(length),
            ) {
                let _ = out.apply_subpalette(o, l, tail);
            }
        }
        Some(out)
    }

    /// Whether picture `id` is palette-indexed, so a palette variant draws it differently.
    #[must_use]
    pub fn is_palettised(&self, id: DataId) -> bool {
        self.resolve(id).is_ok_and(|(_, rs, _)| {
            matches!(
                PixelFormatId::from_raw(rs.format),
                PixelFormatId::P8 | PixelFormatId::Index16
            )
        })
    }

    /// The palette a picture names for itself, when it is palette-indexed.
    #[must_use]
    pub fn own_palette(&self, id: DataId) -> Option<DataId> {
        self.resolve(id).ok()?.1.default_palette_id
    }

    /// The picture `id` (a surface, image texture or render surface id) with its mips. `clip_map`
    /// keys a palette-indexed picture's first colours to transparent; `palette` replaces a
    /// palette-indexed picture's own palette.
    ///
    /// # Errors
    /// A broken record chain, or pixels that will not decode.
    pub fn decode(
        &self,
        id: DataId,
        clip_map: bool,
        palette: Option<&ExpandedPalette>,
    ) -> Result<TexturePixels, TextureError> {
        let data = self.decode_level0(id, clip_map, palette)?;
        with_mips(id, &data)
    }

    /// A picture's mean colour, sRGB bytes `[r, g, b]`: how it looks from so far away that it is
    /// one texel.
    ///
    /// # Errors
    /// As [`Self::decode`].
    pub fn average(&self, id: DataId) -> Result<[u8; 3], TextureError> {
        let mut data = self.decode_level0(id, false, None)?;
        if data.format != TextureFormat::Bgra8 {
            data = pyreal_codecs::texture::decode_block_chain(&data)
                .map_err(|e| TextureError::Decode(id, e))?;
        }
        let px = data.levels.first().ok_or(TextureError::Unusable(id))?;
        let mut sum = [0u64; 3];
        for p in px.as_chunks::<4>().0 {
            for (s, &c) in sum.iter_mut().zip([p[2], p[1], p[0]].iter()) {
                *s += u64::from(c);
            }
        }
        let n = (px.len() as u64 / 4).max(1);
        Ok(sum.map(|s| u8::try_from(s / n).unwrap_or(u8::MAX)))
    }

    /// A picture's size and texels, blue, green, red and alpha, rows from the top.
    ///
    /// # Errors
    /// As [`Self::decode`].
    pub fn bgra(&self, id: DataId) -> Result<(u32, u32, Vec<u8>), TextureError> {
        self.bgra_in(id, None)
    }

    /// As [`Self::bgra`], a palette-indexed picture drawn in `palette` in place of its own.
    ///
    /// # Errors
    /// As [`Self::decode`].
    pub fn bgra_in(
        &self,
        id: DataId,
        palette: Option<&ExpandedPalette>,
    ) -> Result<(u32, u32, Vec<u8>), TextureError> {
        let mut data = self.decode_level0(id, false, palette)?;
        if data.format != TextureFormat::Bgra8 {
            data = pyreal_codecs::texture::decode_block_chain(&data)
                .map_err(|e| TextureError::Decode(id, e))?;
        }
        Ok((
            data.width,
            data.height,
            data.levels.first().cloned().unwrap_or_default(),
        ))
    }

    /// The picture `id` as BGRA8 with `change` laid over its first level (given its width and
    /// height), and its mips made from that.
    ///
    /// # Errors
    /// As [`Self::decode`].
    pub fn decode_with(
        &self,
        id: DataId,
        change: impl Fn(u32, u32, &mut [u8]),
    ) -> Result<TexturePixels, TextureError> {
        self.decode_keyed_with(id, false, change)
    }

    /// [`Self::decode_with`] with a clip map's keying.
    ///
    /// # Errors
    /// As [`Self::decode`].
    pub fn decode_keyed_with(
        &self,
        id: DataId,
        clip_map: bool,
        change: impl Fn(u32, u32, &mut [u8]),
    ) -> Result<TexturePixels, TextureError> {
        let mut data = self.decode_level0(id, clip_map, None)?;
        if data.format != TextureFormat::Bgra8 {
            data = pyreal_codecs::texture::decode_block_chain(&data)
                .map_err(|e| TextureError::Decode(id, e))?;
        }
        data.levels.truncate(1);
        let (w, h) = (data.width, data.height);
        change(
            w,
            h,
            data.levels.first_mut().ok_or(TextureError::Unusable(id))?,
        );
        with_mips(id, &data)
    }

    /// [`Self::average`] taken in linear light.
    ///
    /// # Errors
    /// As [`Self::decode`].
    pub fn average_linear(&self, id: DataId) -> Result<[f32; 3], TextureError> {
        self.average_dressed(id, false, None)
    }

    /// [`Self::average_linear`] of the picture as `clip_map` and `palette` draw it, over the
    /// texels that show.
    ///
    /// # Errors
    /// As [`Self::decode`].
    pub fn average_dressed(
        &self,
        id: DataId,
        clip_map: bool,
        palette: Option<&ExpandedPalette>,
    ) -> Result<[f32; 3], TextureError> {
        let mut data = self.decode_level0(id, clip_map, palette)?;
        if data.format != TextureFormat::Bgra8 {
            data = pyreal_codecs::texture::decode_block_chain(&data)
                .map_err(|e| TextureError::Decode(id, e))?;
        }
        let px = data.levels.first().ok_or(TextureError::Unusable(id))?;
        let rgb = px
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| [p[2], p[1], p[0], if clip_map { p[3] } else { 255 }]);
        mean_linear(rgb, if clip_map { 128 } else { 0 }).ok_or(TextureError::Unusable(id))
    }

    /// A picture's size and alpha, one byte a texel, rows from the top.
    ///
    /// # Errors
    /// As [`Self::decode`].
    pub fn alpha(&self, id: DataId, clip_map: bool) -> Result<(u32, u32, Vec<u8>), TextureError> {
        let mut data = self.decode_level0(id, clip_map, None)?;
        if data.format != TextureFormat::Bgra8 {
            data = pyreal_codecs::texture::decode_block_chain(&data)
                .map_err(|e| TextureError::Decode(id, e))?;
        }
        let px = data.levels.first().ok_or(TextureError::Unusable(id))?;
        Ok((
            data.width,
            data.height,
            px.as_chunks::<4>().0.iter().map(|p| p[3]).collect(),
        ))
    }

    fn decode_level0(
        &self,
        id: DataId,
        clip_map: bool,
        shift: Option<&ExpandedPalette>,
    ) -> Result<TextureData, TextureError> {
        let (rsid, rs, bytes) = self.resolve(id)?;
        let payload = rs.payload(&bytes).ok_or(TextureError::Unusable(rsid))?;
        let format = PixelFormatId::from_raw(rs.format);
        let own = rs
            .default_palette_id
            .and_then(|p| self.palette_colours(p))
            .and_then(|c| ExpandedPalette::from_dat(&c));
        let palettised = matches!(format, PixelFormatId::P8 | PixelFormatId::Index16);
        let palette = match shift {
            Some(p) if palettised => Some(p),
            _ => own.as_ref(),
        };
        let pic = DatPicture {
            format,
            width: rs.width,
            height: rs.height,
            payload,
            classic: self.lookup.era_of(rsid) == ContainerEra::Classic,
        };
        decode_dat_picture(pic, palette, clip_map).map_err(|e| match e {
            PictureError::Incomplete => TextureError::Unusable(rsid),
            PictureError::Decode(e) => TextureError::Decode(rsid, e),
        })
    }
}

/// The mean of sRGB texels `[r, g, b, a]` in linear light, of those whose alpha is at least `cut`.
#[must_use]
pub fn mean_linear(texels: impl Iterator<Item = [u8; 4]>, cut: u8) -> Option<[f32; 3]> {
    let mut sum = [0.0f64; 3];
    let mut n = 0u32;
    for t in texels.filter(|t| t[3] >= cut) {
        for (s, &c) in sum.iter_mut().zip(&t[..3]) {
            *s += f64::from(crate::asset::material::srgb_to_linear(c));
        }
        n += 1;
    }
    #[allow(clippy::cast_possible_truncation)]
    (n > 0).then(|| sum.map(|s| (s / f64::from(n)) as f32))
}

/// A decoded level 0 with its mips: BC blocks as the dat holds them with the client's four-level
/// chain (or expanded to BGRA8 when the picture is not a whole number of blocks), BGRA8 with a
/// full box-filtered chain.
///
/// # Errors
/// Pixels shorter than their size says.
pub fn with_mips(id: DataId, data: &TextureData) -> Result<TexturePixels, TextureError> {
    use pyreal_codecs::mip;
    let fail = |e| TextureError::Decode(id, e);
    if data.width == 0 || data.height == 0 || data.levels.is_empty() {
        return Err(TextureError::Unusable(id));
    }
    let layout = match data.format {
        TextureFormat::Bc1 => Some(PixelLayout::Bc1),
        TextureFormat::Bc2 | TextureFormat::Bc2Premultiplied => Some(PixelLayout::Bc2),
        TextureFormat::Bc3 | TextureFormat::Bc3Premultiplied => Some(PixelLayout::Bc3),
        _ => None,
    };
    if let Some(layout) = layout {
        if data.width.is_multiple_of(4) && data.height.is_multiple_of(4) {
            let chain = mip::compressed_system_chain(data)
                .map_err(fail)?
                .unwrap_or_else(|| data.clone());
            return Ok(TexturePixels {
                width: data.width,
                height: data.height,
                layout,
                levels: chain.levels,
            });
        }
        let expanded = pyreal_codecs::texture::decode_block_chain(data).map_err(fail)?;
        return with_mips(id, &expanded);
    }
    if data.format != TextureFormat::Bgra8 {
        return Err(TextureError::Unusable(id));
    }
    let mut levels = vec![data.levels[0].clone()];
    let (mut w, mut h) = (data.width, data.height);
    while w > 1 || h > 1 {
        let (next, nw, nh) =
            mip::box_filter_bgra8(levels.last().map_or(&[][..], Vec::as_slice), w, h)
                .map_err(fail)?;
        levels.push(next);
        (w, h) = (nw, nh);
    }
    Ok(TexturePixels {
        width: data.width,
        height: data.height,
        layout: PixelLayout::Bgra8,
        levels,
    })
}
