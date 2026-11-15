//! Bitmap fonts: the `Font` dat object, the texture-based font atlas and the batched text path.
//!
//! This module preserves the font byte layout and all eight pixel-positioning rules.
//!
//! **Why [`Font`] is declared here.** `pyreal-primitives` has no font type, so [`Font`] is
//! declared in this crate, with the field names of the decoded font record, for the same reason
//! a surface is.
//!
//! The font path has its **own** half-pixel formula (`2·(p/size) − 1 ∓ 1/size`) — related to, but
//! not identical to, the old UI quad path's pixel rules.

use crate::RenderError;

/// `FontCharDesc` — 11 bytes, packed, no padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FontCharDesc {
    /// The UTF-16 code unit this glyph draws.
    pub unicode: u16,
    /// Left edge of the glyph inside the atlas, in pixels.
    pub offset_x: u16,
    /// Top edge inside the atlas.
    pub offset_y: u16,
    pub width: u8,
    pub height: u8,
    /// Left side bearing, added to the pen before drawing.
    pub horizontal_offset_before: i8,
    /// Right side bearing, added after.
    pub horizontal_offset_after: i8,
    /// Vertical offset from the top of the line to the glyph top.
    pub vertical_offset_before: i8,
}

impl FontCharDesc {
    /// The packed fields occupy 2+2+2+1+1+1+1+1 = 11 bytes.
    pub const SERIALISED_SIZE: usize = 11;

    /// Read one record from its 11 packed bytes, little-endian.
    #[must_use]
    pub fn from_bytes(b: &[u8; Self::SERIALISED_SIZE]) -> Self {
        Self {
            unicode: u16::from_le_bytes([b[0], b[1]]),
            offset_x: u16::from_le_bytes([b[2], b[3]]),
            offset_y: u16::from_le_bytes([b[4], b[5]]),
            width: b[6],
            height: b[7],
            #[allow(clippy::cast_possible_wrap)]
            horizontal_offset_before: b[8] as i8,
            #[allow(clippy::cast_possible_wrap)]
            horizontal_offset_after: b[9] as i8,
            #[allow(clippy::cast_possible_wrap)]
            vertical_offset_before: b[10] as i8,
        }
    }

    /// Rule 1 — **advance** = `horizontal_offset_before + width + horizontal_offset_after`
    /// Integer, accumulated in integer pixels; no
    /// kerning, no sub-pixel positioning, no hinting. **There is no kerning table** — pair kerning
    /// does not exist in this engine.
    #[must_use]
    pub fn advance(&self) -> i32 {
        i32::from(self.horizontal_offset_before)
            + i32::from(self.width)
            + i32::from(self.horizontal_offset_after)
    }
}

/// The `Font` dat object (type 0x0D `.font`), metrics only: the pixels live in two ordinary
/// `0x06xxxxxx` `RenderSurface`s.
#[derive(Debug, Clone, Default)]
pub struct Font {
    /// The line height in pixels.
    pub max_char_height: u32,
    /// Widest glyph.
    pub max_char_width: u32,
    /// Sorted ascending by `unicode` in every retail font.
    pub char_descs: Vec<FontCharDesc>,
    pub num_horizontal_border_pixels: u32,
    pub num_vertical_border_pixels: u32,
    /// Serialised and never read by any traced draw path; its intended use is not established.
    pub baseline_offset: i32,
    pub foreground_surface_data_id: u32,
    /// 0 when the font has no shadow sheet.
    pub background_surface_data_id: u32,
}

/// The `'?'` fallback the client uses for any character outside the map.
pub const MISSING_GLYPH: u16 = 0x3F;

impl Font {
    /// the client falls back to `'?' (0x3F)` for any character
    /// outside the map or with an out-of-range index; if `'?'` is also missing it returns NULL.
    ///
    /// "The `'?'` fallback means a missing glyph renders as a question mark, not as a box or
    /// nothing."
    #[must_use]
    pub fn get_char_desc(&self, ch: u16) -> Option<&FontCharDesc> {
        self.find(ch).or_else(|| self.find(MISSING_GLYPH))
    }

    fn find(&self, ch: u16) -> Option<&FontCharDesc> {
        // Records are stored sorted ascending by unicode in every retail font, so a binary search
        // is the intended lookup; the client builds a dense character map for O(1) and falls back
        // to a linear scan when it is absent. Either way the answer is the same record.
        self.char_descs
            .binary_search_by_key(&ch, |d| d.unicode)
            .ok()
            .map(|i| &self.char_descs[i])
            .or_else(|| self.char_descs.iter().find(|d| d.unicode == ch))
    }

    /// True only when the character is in *this* font's map. It
    /// is what drives per-character font fallback.
    #[must_use]
    pub fn contains_char(&self, ch: u16) -> bool {
        self.find(ch).is_some()
    }

    /// The character's advance width.
    #[must_use]
    pub fn get_char_width_a(&self, ch: u16) -> i32 {
        self.get_char_desc(ch).map_or(0, FontCharDesc::advance)
    }

    /// The lowest and highest `unicode` present.
    #[must_use]
    pub fn unicode_range(&self) -> Option<(u16, u16)> {
        let min = self.char_descs.iter().map(|d| d.unicode).min()?;
        let max = self.char_descs.iter().map(|d| d.unicode).max()?;
        Some((min, max))
    }
}

/// One baked glyph in the atlas.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GlyphUv {
    pub u0: f32,
    pub v0: f32,
    /// Rule 7 — `(x + width)/256`, **exclusive**, with no half-texel inset (the sampler is `POINT`,
    /// so no inset is needed). Divided by the *glyph texture's* width, which is 256 only for a
    /// [`FontAtlas::build`] bake; [`FontAtlas::from_glyph_sheet`] divides by the sheet's own width.
    pub u1: f32,
    /// `(y + max_character_height)/256`, likewise exclusive — and `(y + height)/sheetHeight` for
    /// a sheet, because the client's source rectangle is `height` tall.
    pub v1: f32,
    pub width: u8,
    /// A baked cell is `max_char_height` tall — that is what the font-texture setup records and what
    /// the text draw uses. A **sheet** entry is `height` tall, the glyph's own, because
    /// the rectangle builder builds both rectangles from `height`.
    pub height: u32,
    pub horizontal_offset_before: i8,
    pub horizontal_offset_after: i8,
    pub vertical_offset_before: i8,
}

/// The eight `(dx, dy)` offsets of the text element's **`0x9000`** outline arm, in the client's
/// own iteration order.
///
/// The client loops `dy` from -1 to 1 on the outside and `dx` from -1 to 1 on the inside,
/// skipping `(0, 0)`, and draws the character at `(x + dx, y + dy)` in the outline colour with
/// flags `0x9000`.
///
/// The order is submission order and a text batch has no other ordering, so
/// it is pinned rather than left to a nested loop's convenience. The centre is skipped because
/// the foreground pass draws it.
pub const OUTLINE_NEIGHBOURHOOD: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// The atlas edge, in texels: the client creates a 256×256 `PFID_A8R8G8B8` texture, one level.
pub const ATLAS_SIZE: u32 = 256;
/// `0.00390625 = 1/256` is the texel scale.
pub const TEXEL_SCALE: f32 = 1.0 / 256.0;
/// Rule 7 — the atlas gutter is 1 pixel horizontally and 1 pixel vertically
/// (`row height = max_char_height + 1`).
pub const ATLAS_GUTTER: u32 = 1;
/// the font-texture setup bakes only the printable-ASCII range.
pub const FIRST_BAKED_CHAR: u16 = 0x20;
/// The clamp is `<= 0x7F`, one past the last printable character.
pub const LAST_BAKED_CHAR: u16 = 0x7F;

/// A glyph sheet: one of the `Font`'s two `RenderSurface`s, as BGRA8.
#[derive(Debug, Clone, Copy)]
pub struct GlyphSheet<'a> {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes.
    pub bgra: &'a [u8],
}

/// A font's glyph texture plus the per-glyph metrics — built either of the two ways the client
/// draws text.
///
/// [`FontAtlas::build`] is the 256×256 texture-font bake: the **GPU path**, used by the debug
/// overlay, which refuses outright any font whose printable-ASCII range will not fit
/// the 256×256 atlas. [`FontAtlas::from_glyph_sheet`] is the **CPU path**'s
/// source -- the font's own foreground `RenderSurface`, addressed by the client's own source
/// rectangle. It bakes nothing, so it has no size limit and no ASCII requirement, and it is what
/// the UI text path reads.
#[derive(Debug, Clone)]
pub struct FontAtlas {
    /// The glyph texture, BGRA8: `texture_size.0 * texture_size.1 * 4` bytes.
    pub pixels: Vec<u8>,
    /// The glyph texture's size in texels — `(256, 256)` for a bake, the sheet's own size for
    /// [`FontAtlas::from_glyph_sheet`]. Every `u`/`v` in [`GlyphUv`] is a fraction of it.
    pub texture_size: (u32, u32),
    /// `(unicode, its cell)`, sorted ascending by the code unit. The list is already in the order
    /// the atlas lookup expects, so a lookup is the client's binary search.
    characters: Vec<(u16, GlyphUv)>,
    /// `max_character_height` = `font.max_char_height`.
    pub max_character_height: u32,
    /// `vertical_spacing`, defaulted to `font.max_char_height`.
    pub vertical_spacing: u32,
    /// `horizontal_spacing`, defaulted to the maximum `GetCharWidthA` over the baked range. Used
    /// only in fixed-pitch mode.
    pub horizontal_spacing: i32,
    /// Horizontal dilation — how far the **outline** blit's rectangles
    /// are dilated horizontally. Carried here because a caller holding only the atlas cannot
    /// otherwise build the client's dilated rectangle.
    pub num_horizontal_border_pixels: u32,
    /// Vertical dilation. See
    /// [`Self::num_horizontal_border_pixels`]; the outline glyph is the one place the two are
    /// *not* used symmetrically.
    pub num_vertical_border_pixels: u32,
    /// The font's **background** (outline) sheet, BGRA8 at [`Self::texture_size`], when it has
    /// one.
    ///
    /// It is a second glyph texture addressed by the *same* [`GlyphUv`] metrics: the retail
    /// sheets are authored with the glyph pre-fattened in place, so the outline blit reads the
    /// same `(offset_x, offset_y)` origin, only dilated. Measured over the shipped set: **37 of
    /// 49** fonts have one, every one of them is the *same size* as its foreground sheet, and for
    /// every one of them the dilated source rectangle of every glyph stays inside the sheet.
    ///
    /// `None` for the other 12, which is the `0x9000` arm's precondition — see
    /// [`OUTLINE_NEIGHBOURHOOD`].
    ///
    /// **This is the payload, not the fact.** A caller that uploads the sheet to the GPU wants to
    /// *move* the pixels out rather than clone 8 MiB, so [`FontAtlas::take_outline_pixels`] exists
    /// and [`Self::has_outline_sheet`] is answered by [`Self::outline_sheet`] instead. Reading the
    /// fact off `outline_pixels.is_some()` after such a move silently selects the wrong
    /// the character draw arm, which is exactly what happened the first time this was written.
    outline_pixels: Option<Vec<u8>>,
    /// Whether the font named a background sheet — **independent of whether the pixels are still
    /// here**. See [`Self::outline_pixels`].
    outline_sheet: bool,
}

impl FontAtlas {
    /// The client's font-texture setup.
    ///
    /// ```text
    /// require min_unicode_char <= 0x20 and max_unicode_char >= 0x7E   // must cover printable ASCII
    /// clamp first_char to >= 0x20 and last_char to <= 0x7F
    /// scratch = 256x256 A8R8G8B8, filled RGBA(1,1,1,0)            // white, fully transparent
    /// x = 0; y = 0
    /// for ch = first .. last:
    ///     d = font.get_char_desc(ch)
    ///     if (x + d->width > 255) { y += max_character_height + 1; x = 0; if (y > 255) fail }
    ///     draw ch into scratch at (x, y), colours 0xFFFFFFFF / 0xFF000000, flags 0x100
    /// .. record U0/V0/U1/V1 ...
    ///     x += d->width + 1                                      // 1 px gutter horizontally
    /// ```
    ///
    /// # Errors
    /// [`RenderError::FontAtlasOverflow`] when the glyphs do not fit, which is what the client
    /// returns false for — "which is why the GPU path is only used for the small debug font and for
    /// UI text drawn in Latin scripts".
    pub fn build(font: &Font, foreground: GlyphSheet<'_>) -> Result<Self, RenderError> {
        let (min_u, max_u) = font
            .unicode_range()
            .ok_or(RenderError::Unsupported("font has no glyphs"))?;
        // The client's `require`: the font must cover printable ASCII.
        if min_u > FIRST_BAKED_CHAR || max_u < 0x7E {
            return Err(RenderError::Unsupported(
                "bitmap font requires a font covering printable ASCII (0x20..=0x7E)",
            ));
        }
        let first = min_u.max(FIRST_BAKED_CHAR);
        let last = max_u.min(LAST_BAKED_CHAR);

        // Fill white and fully transparent, as the client does: RGBA(1,1,1,0).
        let mut pixels = vec![0xFFu8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize];
        for px in pixels.as_chunks_mut::<4>().0 {
            px[3] = 0;
        }

        let max_h = font.max_char_height;
        let mut horizontal_spacing = 0i32;
        let mut characters: Vec<(u16, GlyphUv)> = Vec::with_capacity(usize::from(last - first) + 1);
        let mut x = 0u32;
        let mut y = 0u32;
        for ch in first..=last {
            let d = font.get_char_desc(ch).copied().unwrap_or_default();
            horizontal_spacing = horizontal_spacing.max(d.advance());
            let w = u32::from(d.width);
            // `if (x + d->width > 255)` -- note the 255, not 256: a glyph ending exactly on the
            // last column still wraps.
            if x + w > ATLAS_SIZE - 1 {
                y += max_h + ATLAS_GUTTER;
                x = 0;
                if y > ATLAS_SIZE - 1 {
                    return Err(RenderError::FontAtlasOverflow);
                }
            }
            blit_glyph(&mut pixels, &d, foreground, x, y);
            #[allow(clippy::cast_precision_loss)]
            characters.push((
                ch,
                GlyphUv {
                    u0: x as f32 * TEXEL_SCALE,
                    v0: y as f32 * TEXEL_SCALE,
                    // U1 = (x + width - 1 + 1)/256 -- the exclusive right edge.
                    u1: (x + w) as f32 * TEXEL_SCALE,
                    // V1 = (y + max_character_height - 1 + 1)/256.
                    v1: (y + max_h) as f32 * TEXEL_SCALE,
                    width: d.width,
                    height: max_h,
                    horizontal_offset_before: d.horizontal_offset_before,
                    horizontal_offset_after: d.horizontal_offset_after,
                    vertical_offset_before: d.vertical_offset_before,
                },
            ));
            x += w + ATLAS_GUTTER;
        }

        Ok(Self {
            pixels,
            texture_size: (ATLAS_SIZE, ATLAS_SIZE),
            characters,
            max_character_height: max_h,
            vertical_spacing: max_h,
            horizontal_spacing,
            num_horizontal_border_pixels: font.num_horizontal_border_pixels,
            num_vertical_border_pixels: font.num_vertical_border_pixels,
            // The client bakes with flags `0x100`, which is the copy-alpha flag and
            // has no `0x4000`, so the bake never reads the background sheet. The debug overlay
            // therefore has no outline, and this stays `None` by transcription rather than by
            // omission.
            outline_pixels: None,
            outline_sheet: false,
        })
    }

    /// The **CPU path**: the font's own foreground glyph sheet, used as the glyph texture directly.
    ///
    /// The client draws UI text by blitting out of the
    /// `Font`'s two `RenderSurface`s rather than out of a baked atlas, and its rectangle builder
    /// builds the pair of rectangles it blits between:
    ///
    /// ```text
    /// src = (offset_x - hBorder, offset_y - vBorder,
    ///        offset_x + width + hBorder, offset_y + height + vBorder)
    /// dst = (penX - hBorder, penY - vBorder, penX + width + hBorder, penY + height + hBorder)
    /// ```
    ///
    /// and the character draw passes **0** for the border on the foreground call — only the background
    /// (shadow) call passes `num_vertical_border_pixels`. So the foreground source rectangle is
    /// exactly `(offset_x, offset_y, width, height)`, which is what this records, and the
    /// entry's height is the glyph's own `height` rather than `max_char_height`.
    ///
    /// **This is the one that has no 256×256 limit**, and the reason the client can draw a 48-pixel
    /// display font that the font-texture setup refuses: three retail fonts — `0x40000013`,
    /// `0x40000014` and `0x40000024` — need more than 256 rows for printable ASCII alone, and the
    /// shipped char-gen layouts name two of them.
    ///
    /// # Errors
    /// [`RenderError::Unsupported`] when the font has no glyphs at all, or when the sheet's byte
    /// count does not match its stated size.
    pub fn from_glyph_sheet(font: &Font, sheet: GlyphSheet<'_>) -> Result<Self, RenderError> {
        Self::from_glyph_sheets(font, sheet, None)
    }

    /// [`Self::from_glyph_sheet`] plus the font's **background** (outline) sheet, which is the
    /// second texture the client blits from when its flags carry
    /// `0x4000` — the `0x7000` outline pass of the text element's glyph loop.
    ///
    /// The outline sheet must be the same size as the foreground one, because the two are
    /// addressed by the same `(offset_x, offset_y)`; that is true of all 37 shipped fonts that
    /// have one, and a mismatch is refused rather than silently mis-sampled.
    ///
    /// # Errors
    /// [`RenderError::Unsupported`] as [`Self::from_glyph_sheet`], and additionally when the
    /// outline sheet's size differs from the foreground sheet's.
    pub fn from_glyph_sheets(
        font: &Font,
        sheet: GlyphSheet<'_>,
        outline: Option<GlyphSheet<'_>>,
    ) -> Result<Self, RenderError> {
        if font.char_descs.is_empty() {
            return Err(RenderError::Unsupported("font has no glyphs"));
        }
        let want = (sheet.width as usize) * (sheet.height as usize) * 4;
        if sheet.bgra.len() < want {
            return Err(RenderError::Unsupported(
                "the glyph sheet is smaller than its stated size",
            ));
        }
        let outline_pixels = match outline {
            None => None,
            Some(o) => {
                if o.width != sheet.width || o.height != sheet.height {
                    return Err(RenderError::Unsupported(
                        "the outline sheet is not the same size as the foreground sheet",
                    ));
                }
                if o.bgra.len() < want {
                    return Err(RenderError::Unsupported(
                        "the outline sheet is smaller than its stated size",
                    ));
                }
                Some(o.bgra[..want].to_vec())
            }
        };
        #[allow(clippy::cast_precision_loss)]
        let (sw, sh) = (
            1.0f32 / sheet.width.max(1) as f32,
            1.0f32 / sheet.height.max(1) as f32,
        );
        let max_h = font.max_char_height;
        let mut horizontal_spacing = 0i32;
        let mut characters: Vec<(u16, GlyphUv)> = Vec::with_capacity(font.char_descs.len());
        for d in &font.char_descs {
            horizontal_spacing = horizontal_spacing.max(d.advance());
            let (x, y) = (u32::from(d.offset_x), u32::from(d.offset_y));
            let (w, h) = (u32::from(d.width), u32::from(d.height));
            #[allow(clippy::cast_precision_loss)]
            characters.push((
                d.unicode,
                GlyphUv {
                    u0: x as f32 * sw,
                    v0: y as f32 * sh,
                    // Exclusive at the far edge and no half-texel inset, exactly as rule 7 says --
                    // the rule is about the sampler, not about the atlas.
                    u1: (x + w) as f32 * sw,
                    v1: (y + h) as f32 * sh,
                    width: d.width,
                    height: h,
                    horizontal_offset_before: d.horizontal_offset_before,
                    horizontal_offset_after: d.horizontal_offset_after,
                    vertical_offset_before: d.vertical_offset_before,
                },
            ));
        }
        // The client uses a dense-map lookup; here it is a binary search, so
        // the records must be sorted. They are in every retail font; this makes it true.
        characters.sort_unstable_by_key(|e| e.0);
        characters.dedup_by_key(|e| e.0);
        Ok(Self {
            pixels: sheet.bgra[..want].to_vec(),
            texture_size: (sheet.width, sheet.height),
            characters,
            max_character_height: max_h,
            vertical_spacing: max_h,
            horizontal_spacing,
            num_horizontal_border_pixels: font.num_horizontal_border_pixels,
            num_vertical_border_pixels: font.num_vertical_border_pixels,
            outline_sheet: outline_pixels.is_some(),
            outline_pixels,
        })
    }

    /// Whether this font has a background sheet, i.e. which arm of the text element's outline
    /// pass applies: `true` selects the single dilated `0x7000` draw
    /// (the outline glyph), `false` the eight `0x9000` draws over
    /// [`OUTLINE_NEIGHBOURHOOD`].
    ///
    /// Stays true after [`Self::take_outline_pixels`] has moved the pixels to a GPU texture.
    #[must_use]
    pub fn has_outline_sheet(&self) -> bool {
        self.outline_sheet
    }

    /// Move the outline sheet's pixels out, for a caller uploading them as a texture.
    ///
    /// [`Self::has_outline_sheet`] keeps answering `true` afterwards: the atlas still *describes*
    /// a font with an outline sheet, it simply no longer holds a resident copy of it. Returns
    /// `None` on the second call and for a font with no background sheet.
    pub fn take_outline_pixels(&mut self) -> Option<Vec<u8>> {
        self.outline_pixels.take()
    }

    /// The outline sheet's pixels, while they are still resident.
    #[must_use]
    pub fn outline_pixels(&self) -> Option<&[u8]> {
        self.outline_pixels.as_deref()
    }

    /// `1/width`, `1/height` of the glyph texture — the texel step each `u`/`v` moves by.
    #[must_use]
    pub fn texel_scale(&self) -> (f32, f32) {
        #[allow(clippy::cast_precision_loss)]
        (
            1.0 / self.texture_size.0.max(1) as f32,
            1.0 / self.texture_size.1.max(1) as f32,
        )
    }

    /// Every code unit this glyph texture can draw, ascending.
    #[must_use]
    pub fn baked_chars(&self) -> Vec<u16> {
        self.characters.iter().map(|e| e.0).collect()
    }

    /// The debug-font setup overrides `horizontal_spacing = 8` and
    /// `vertical_spacing = 14` — the fixed-pitch debug console metrics.
    pub fn set_debug_console_metrics(&mut self) {
        self.horizontal_spacing = 8;
        self.vertical_spacing = 14;
    }

    /// The baked glyph for a character, with the `'?'` fallback.
    #[must_use]
    pub fn glyph(&self, ch: char) -> Option<GlyphUv> {
        let code = u16::try_from(u32::from(ch)).ok()?;
        self.lookup(code).or_else(|| self.lookup(MISSING_GLYPH))
    }

    fn lookup(&self, code: u16) -> Option<GlyphUv> {
        self.characters
            .binary_search_by_key(&code, |e| e.0)
            .ok()
            .map(|i| self.characters[i].1)
    }

    /// Text width: the sum of `horizontal_spacing` per character when `fixed_pitch`, otherwise
    /// the sum of `before + width + after`.
    #[must_use]
    pub fn compute_text_width(&self, text: &str, fixed_pitch: bool) -> i32 {
        text.chars()
            .filter_map(|c| self.glyph(c))
            .map(|g| {
                if fixed_pitch {
                    self.horizontal_spacing
                } else {
                    i32::from(g.horizontal_offset_before)
                        + i32::from(g.width)
                        + i32::from(g.horizontal_offset_after)
                }
            })
            .sum()
    }
}

/// Copy one glyph's source rectangle out of the foreground sheet into the atlas.
///
/// The destination rectangle is `(x, y, width, height)` sampled from
/// `(offset_x, offset_y, width, height)` in the sheet.
/// The background (shadow) sheet is not composited here.
fn blit_glyph(atlas: &mut [u8], d: &FontCharDesc, sheet: GlyphSheet<'_>, x: u32, y: u32) {
    for row in 0..u32::from(d.height) {
        let sy = u32::from(d.offset_y) + row;
        let dy = y + row;
        if sy >= sheet.height || dy >= ATLAS_SIZE {
            break;
        }
        for col in 0..u32::from(d.width) {
            let sx = u32::from(d.offset_x) + col;
            let dx = x + col;
            if sx >= sheet.width || dx >= ATLAS_SIZE {
                break;
            }
            let s = ((sy * sheet.width + sx) * 4) as usize;
            let t = ((dy * ATLAS_SIZE + dx) * 4) as usize;
            if s + 4 <= sheet.bgra.len() {
                atlas[t..t + 4].copy_from_slice(&sheet.bgra[s..s + 4]);
            }
        }
    }
}

/// Rule 3 — **line height** is the maximum over the glyphs on the line, so a
/// line of one small glyph is short.
#[must_use]
pub fn line_height(fonts_on_line: &[&Font]) -> u32 {
    fonts_on_line
        .iter()
        .map(|f| f.max_char_height)
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic font covering printable ASCII with a 6x8 cell, laid out left to right in a
    /// 640x8 sheet. Metrics are chosen so every rule can be distinguished from every other.
    fn test_font() -> Font {
        let mut char_descs = Vec::new();
        for (i, ch) in (0x20u16..=0x7Eu16).enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let x = (i * 6) as u16;
            char_descs.push(FontCharDesc {
                unicode: ch,
                offset_x: x,
                offset_y: 0,
                width: 5,
                height: 8,
                horizontal_offset_before: 1,
                horizontal_offset_after: 2,
                vertical_offset_before: 1,
            });
        }
        Font {
            max_char_height: 8,
            max_char_width: 5,
            char_descs,
            num_horizontal_border_pixels: 0,
            num_vertical_border_pixels: 0,
            baseline_offset: 6,
            foreground_surface_data_id: 0x0600_0001,
            background_surface_data_id: 0,
        }
    }

    fn test_sheet(font: &Font) -> (u32, u32, Vec<u8>) {
        let w = 6 * 96u32;
        let h = 8u32;
        let mut bgra = vec![0u8; (w * h * 4) as usize];
        // Paint each glyph's cell with its index so a blit can be traced back to its source.
        for (i, d) in font.char_descs.iter().enumerate() {
            for row in 0..u32::from(d.height) {
                for col in 0..u32::from(d.width) {
                    let x = u32::from(d.offset_x) + col;
                    let p = ((row * w + x) * 4) as usize;
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        bgra[p] = i as u8;
                        bgra[p + 1] = col as u8;
                        bgra[p + 2] = row as u8;
                        bgra[p + 3] = 0xFF;
                    }
                }
            }
        }
        (w, h, bgra)
    }

    /// A font whose line box is taller than its glyphs, so `max_char_height` and `height` can be
    /// told apart. Cells stay 5x8 in the 576x8 sheet [`test_sheet`] paints.
    fn tall_line_font() -> Font {
        Font {
            max_char_height: 12,
            ..test_font()
        }
    }

    #[test]
    fn the_sheet_path_addresses_the_fonts_own_source_rectangle() {
        let font = tall_line_font();
        let (w, h, bgra) = test_sheet(&font);
        let atlas = FontAtlas::from_glyph_sheet(
            &font,
            GlyphSheet {
                width: w,
                height: h,
                bgra: &bgra,
            },
        )
        .unwrap();
        assert_eq!(
            atlas.texture_size,
            (w, h),
            "the glyph texture is the sheet itself"
        );
        assert_eq!(atlas.pixels.len(), (w * h * 4) as usize);
        assert_eq!(atlas.max_character_height, 12);

        // 'A' is the 33rd printable character, so its cell starts at x = 33*6 = 198.
        let d = font.get_char_desc(u16::from(b'A')).copied().unwrap();
        assert_eq!((d.offset_x, d.offset_y, d.width, d.height), (198, 0, 5, 8));
        let g = atlas.glyph('A').unwrap();
        #[allow(clippy::cast_precision_loss)]
        let (sw, sh) = (1.0f32 / w as f32, 1.0f32 / h as f32);
        assert!(
            (g.u0 - 198.0 * sw).abs() < 1e-7,
            "U0 = offset_x / sheetWidth"
        );
        assert!((g.v0 - 0.0).abs() < 1e-7);
        // Exclusive at the far edge and no half-texel inset -- rule 7 is about the POINT sampler,
        // not about the 256x256 atlas, so it survives the change of denominator.
        assert!(
            (g.u1 - 203.0 * sw).abs() < 1e-7,
            "U1 = (offset_x + width) / sheetWidth"
        );
        assert!(
            (g.v1 - 8.0 * sh).abs() < 1e-7,
            "V1 = (offset_y + height) / sheetHeight"
        );
        // The cell is the glyph's own height, which is what retail's character-rectangle builder measures; a baked
        // cell is max_char_height instead, and the two differ for this font.
        assert_eq!(g.height, 8, "height, not max_char_height");
        assert_eq!(g.width, 5);
        // The bearings are the font's own, untouched.
        assert_eq!(
            (
                g.horizontal_offset_before,
                g.horizontal_offset_after,
                g.vertical_offset_before
            ),
            (1, 2, 1)
        );
        // Every character of the font is addressable, not just a baked ASCII window.
        assert_eq!(atlas.baked_chars().len(), font.char_descs.len());
        assert_eq!(atlas.texel_scale(), (sw, sh));
    }

    /// **The atlas setup's `return false` is faithful and stays; UI text does not go through
    /// it.** A font too tall for 256 rows is
    /// refused by the bake and drawn by the sheet.
    #[test]
    fn the_sheet_path_takes_a_font_the_256x256_bake_refuses() {
        // The shape of the shipped `0x40000013`: max_char_height 46, glyphs up to 50 wide. Five of
        // those fit one 256-pixel row, so 95 printable characters need 19 rows of 47 pixels --
        // 893, well past the atlas's 256.
        let mut font = test_font();
        font.max_char_height = 46;
        font.max_char_width = 50;
        for (i, d) in font.char_descs.iter_mut().enumerate() {
            d.width = 50;
            d.height = 46;
            #[allow(clippy::cast_possible_truncation)]
            {
                d.offset_x = (i * 51) as u16;
            }
            d.offset_y = 0;
        }
        let (w, h) = (51u32 * 96, 46u32);
        let bgra = vec![0xFFu8; (w * h * 4) as usize];
        let err = FontAtlas::build(
            &font,
            GlyphSheet {
                width: w,
                height: h,
                bgra: &bgra,
            },
        )
        .unwrap_err();
        assert!(matches!(err, RenderError::FontAtlasOverflow), "{err:?}");

        let atlas = FontAtlas::from_glyph_sheet(
            &font,
            GlyphSheet {
                width: w,
                height: h,
                bgra: &bgra,
            },
        )
        .unwrap();
        for ch in 0x20u8..=0x7E {
            assert!(
                atlas.glyph(ch as char).is_some(),
                "{ch:#04X} draws from the sheet"
            );
        }
        // ..and a font with no glyphs at all is still refused, rather than yielding a texture
        // nothing can be drawn from.
        let empty = Font::default();
        assert!(matches!(
            FontAtlas::from_glyph_sheet(
                &empty,
                GlyphSheet {
                    width: 1,
                    height: 1,
                    bgra: &[0; 4]
                }
            ),
            Err(RenderError::Unsupported(_))
        ));
        // ..as is a sheet that does not carry the pixels it claims to.
        assert!(matches!(
            FontAtlas::from_glyph_sheet(
                &font,
                GlyphSheet {
                    width: 64,
                    height: 64,
                    bgra: &[0; 4]
                }
            ),
            Err(RenderError::Unsupported(_))
        ));
    }

    /// Oracle: the lookup -- the `'?'` fallback is the sheet path's too, and a
    /// character the font does have is found however sparse its map is. The shipped unicode fonts
    /// carry 20 609 records spread over 0x0000..0xFFFC, which a dense array indexed by
    /// `ch - first` would answer wrongly.
    #[test]
    fn the_sheet_path_keeps_the_question_mark_fallback_over_a_sparse_map() {
        let mut font = test_font();
        font.char_descs.push(FontCharDesc {
            unicode: 0x4E00,
            offset_x: 0,
            offset_y: 0,
            width: 5,
            height: 8,
            ..FontCharDesc::default()
        });
        let (w, h, bgra) = test_sheet(&font);
        let atlas = FontAtlas::from_glyph_sheet(
            &font,
            GlyphSheet {
                width: w,
                height: h,
                bgra: &bgra,
            },
        )
        .unwrap();
        assert!(
            atlas.glyph('\u{4E00}').is_some(),
            "a sparse high code unit is addressable"
        );
        assert_ne!(atlas.glyph('\u{4E00}'), atlas.glyph('?'));
        // ..and one the font does not have still falls back to '?'.
        assert_eq!(atlas.glyph('\u{4E01}'), atlas.glyph('?'));
    }
}
