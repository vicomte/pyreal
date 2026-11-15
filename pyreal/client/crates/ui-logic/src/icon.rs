//! Icon compositing: an object's or a spell's picture built from several dat surfaces, as the
//! retail icon renderer builds it.

use pyreal_primitives::DataId;

/// A **runtime-generated** UI surface: a dat image with one pixel operation applied to it before it
/// is shown.
///
/// One half of the "generated surface" support — `UiFill` is the other. Two of the
/// client's UI surfaces are not a picture out of the dat and are not a flat fill either: they are a
/// *derivation* of a picture, made in a local render surface at run time.
///
/// For each appearance-page color spot, the client creates a local surface in the UI format,
/// copies the template onto it, replaces black with the spot color, and installs the generated
/// image on the region. The gradient disk instead copies the shade ring with the spot color as a
/// multiply tint.
///
/// The client makes a page of pixels; this rebuild has no CPU page and uploads a texture, so the
/// operation rides on the image reference and the host derives the texture once per distinct
/// `(DataId, SurfaceOp)` pair. That is the same caching the combined-texture builder
/// does for a `(palette, image)` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SurfaceOp {
    /// Behavior: every texel whose **whole 32-bit ARGB value**
    /// equals `from` becomes `to`. An exact match, not a tolerance and not a hue shift: the loop is
    /// `if (*p == from) *p = to;` over the locked surface.
    ReplaceColor { from: u32, to: u32 },
    /// Multiply every channel of every texel
    /// multiplied by `colour`'s, which is what turns the grey shade ring into a coloured one.
    Multiply(u32),
    /// The target indicator's image copy, selector 4:
    /// Normal copy, then an unmasked colorize blit at full strength. Retains the source
    /// alpha/value and replaces hue/saturation using retail's integer colour conversion.
    Colorize(u32),
    /// **Several** dat surfaces blitted into one local surface, which is what an inventory slot's
    /// picture actually is. See [`IconRecipe`].
    Icon(IconRecipe),
}

/// The surface the weenie's icon accessor answers, described rather than drawn.
///
/// The icon accessor does not return a dat image directly. It builds a **32 × 32** local surface
/// in the UI's `PFID_A8R8G8B8` format, so the
/// working format is `PFID_A8R8G8B8`) that the icon renderer composites out of
/// **six** blits into **two** surfaces. A spell slot's picture is the same idea through a different
/// function, the magic system's spell-icon compositor.
///
/// For an object, the client first builds a drag surface from the icon, alpha overlay, and effect
/// color replacement. It then builds the final surface from the item-type background, underlay,
/// and the completed drag surface. For a spell, it draws the power-level background, alpha icon,
/// reversed or normal tint, and an optional fellowship or self overlay in that order.
///
/// # Why this is a recipe and not a flat layer list
///
/// The drag icon is a **separate surface** and the three blits that build it happen *before* it is
/// alpha-blended onto the icon surface as one image. Flattening the six into one ordered list would put
/// the icon's normal blit straight onto the background tile and erase it — the tile is the opaque
/// base, and it is only visible at all because the icon reaches it through the drag surface's own
/// alpha. Two stages is not an implementation detail; it is the picture.
///
/// # Why this rides on [`SurfaceOp`]
///
/// The host keys its texture cache on `(DataId, Option<SurfaceOp>)` and composites once per
/// distinct recipe, as the client's icon data is built once per object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IconRecipe {
    /// The object icon, as the icon renderer builds it.
    Object {
        /// The `0x10000004` icon-background row selected by the lowest set item-type bit plus one —
        /// the opaque base.
        background: Option<DataId>,
        /// The `0x10000005` effect-icon row selected by the lowest set effect bit plus one, with
        /// the client's row `0x21` fallback already applied.
        /// Replaces the drag icon's opaque-white pixels; x86.
        effects: Option<DataId>,
        /// The object's public icon id.
        icon: Option<DataId>,
        /// The object's public icon-overlay id.
        overlay: Option<DataId>,
        /// The object's public icon-underlay id.
        /// Blitted over the type tile, below the drag icon; x86.
        underlay: Option<DataId>,
    },
    /// The spell icon, as the magic system's compositor builds it.
    Spell {
        /// The `0x10000006` spell-background row selected by the spell's power level.
        background: Option<DataId>,
        /// The spell's icon id.
        icon: Option<DataId>,
        /// Row 1 or 2 of spell-overlay table `0x10000007`, selected by bit `0x10` — the
        /// **reversed / non-reversed** wash, applied through `ReplaceColor` and not blitted.
        tint: Option<DataId>,
        /// `0x10000007` row **4** for `FellowshipSpell (0x2000)`, row **3** for
        /// `SelfTargeted (0x8)`, and nothing otherwise.
        overlay: Option<DataId>,
    },
}

impl IconRecipe {
    /// The id this composite **is about**, which is what an image's id carries.
    ///
    /// **The object's own icon, not the surface the composite happens to start from.** The two are
    /// different — the bottom blit is the item-type tile — and the icon is the right answer for
    /// both of `did`'s jobs: a consumer that does not understand the recipe draws the object's
    /// picture rather than a bare coloured square, and a reader of a draw list (or of a failing
    /// assertion) sees the id that names the thing in the slot. Dozens of assertions across this
    /// workspace already read `did` off the icon element and mean *"this slot draws that object's
    /// icon"*; two of the quickbar's caught this the moment it was the tile instead.
    ///
    /// The **pixels** do not come from here at all: the host builds them from the whole recipe, so
    /// nothing about the composite depends on which layer this names.
    #[must_use]
    pub const fn base(self) -> Option<DataId> {
        match self {
            Self::Object {
                background, icon, ..
            }
            | Self::Spell {
                background, icon, ..
            } => match icon {
                Some(d) => Some(d),
                None => background,
            },
        }
    }

    /// The *first* of the two surfaces the icon renderer builds,
    /// as a recipe of its own.
    ///
    /// The client makes two 32×32 surfaces and keeps both. The drag surface gets the icon (blit
    /// mode 0), then the overlay (mode 2), then the effects replacement of white. The icon surface
    /// gets the item-type tile (mode 0), then the underlay (mode 1), then the finished drag surface
    /// (mode 1). Each becomes a graphic of its own.
    ///
    /// The item list's drag-icon preparation asks for the object's drag icon by name, so what
    /// follows the cursor is the icon, its overlay and the effects replacement, and
    /// **not** the item-type tile or the custom underlay. Dropping those two is therefore not an
    /// approximation of the drag surface: `composite`'s `based == false` arm blits the finished
    /// drag buffer onto an untouched local with a normal blit, which reproduces the drag surface exactly.
    ///
    /// A `Spell` recipe has no second surface in the client — the spell-icon compositor
    /// makes one — and is returned unchanged.
    #[must_use]
    pub const fn drag_surface(self) -> Self {
        match self {
            Self::Object {
                effects,
                icon,
                overlay,
                ..
            } => Self::Object {
                background: None,
                effects,
                icon,
                overlay,
                underlay: None,
            },
            Self::Spell { .. } => self,
        }
    }

    /// How many of the recipe's surfaces are present. **One** is exactly the plain image and the
    /// host may skip the compositor — but it must still key on the recipe, because the icon alone
    /// and the icon under a tile are different pictures. **Zero** is the client's
    /// null image: nothing at all.
    #[must_use]
    pub fn layer_count(self) -> usize {
        let present = |v: Option<DataId>| usize::from(v.is_some());
        match self {
            Self::Object {
                background,
                effects,
                icon,
                overlay,
                underlay,
            } => {
                present(background)
                    + present(effects)
                    + present(icon)
                    + present(overlay)
                    + present(underlay)
            }
            Self::Spell {
                background,
                icon,
                tint,
                overlay,
            } => present(background) + present(icon) + present(tint) + present(overlay),
        }
    }
}

impl SurfaceOp {
    /// The packed ARGB value of RGBA `(0, 0, 0, 1)` — the colour the colour-spot pass replaces.
    ///
    /// The `from` is built on the stack as four floats and converted, so the alpha byte is **0xFF**
    /// and the match is against opaque black rather than against transparent black.
    pub const OPAQUE_BLACK: u32 = 0xFF00_0000;

    /// The packed ARGB value of RGBA `(1, 1, 1, 1)` — the colour **both** icon composites use
    /// as the replacement source. Opaque white, by the same reasoning as
    /// [`Self::OPAQUE_BLACK`]: the four floats are converted, so the alpha byte is `0xFF`.
    pub const OPAQUE_WHITE: u32 = 0xFFFF_FFFF;

    /// The 32 × 32 `Create(0x20, 0x20, ...)` both composites make. Kept here because it is the
    /// client's constant and not the icon art's: a source surface of another size is clipped to it
    /// ( takes the smaller of the two extents on each axis).
    pub const ICON_EXTENT: u32 = 32;

    /// Apply the operation to one 32-bit ARGB texel.
    ///
    /// Little-endian BGRA8 bytes and a `u32` ARGB are the same four bytes in the same order, which
    /// is why the client's `*(ulong*)p == from` and this agree texel for texel.
    #[must_use]
    pub fn apply(self, texel: u32) -> u32 {
        match self {
            Self::ReplaceColor { from, to } => {
                if texel == from {
                    to
                } else {
                    texel
                }
            }
            Self::Multiply(c) => {
                let ch = |sh: u32| ((texel >> sh) & 0xFF) * ((c >> sh) & 0xFF) / 255;
                (ch(24) << 24) | (ch(16) << 16) | (ch(8) << 8) | ch(0)
            }
            Self::Colorize(c) => Self::colorize(texel, c),
            // A composite is a function of **several** surfaces and cannot be a function of one
            // texel. The host recognises this variant before it reaches here; see
            // [`composite`]. Returning the texel unchanged keeps a caller that
            // does not from corrupting the base image, which is what a `todo!()` here would not.
            Self::Icon(_) => texel,
        }
    }

    /// Full-strength, unmasked ARGB colorize. The blit-and-color entry point performs the
    /// preceding Normal copy.
    /// The 1..256 channel representation, integer hue/saturation, and signed nearest
    /// division (ties toward zero) are observable on grey antialiased bracket texels.
    #[must_use]
    pub fn colorize(texel: u32, color: u32) -> u32 {
        let channel = |c: u32, shift: u32| {
            let v = ((c >> shift) & 255u32) as i32;
            v + i32::from(v != 0)
        };
        let (r, g, b) = (channel(color, 16), channel(color, 8), channel(color, 0));
        let max = r.max(g).max(b);
        let delta = max - r.min(g).min(b);
        let value = channel(texel, 16)
            .max(channel(texel, 8))
            .max(channel(texel, 0));
        let (rr, gg, bb) = if delta == 0 {
            (value, value, value)
        } else {
            let hue = if r == max {
                (g - b) * 4096 / delta + if g < b { 24576 } else { 0 }
            } else if g == max {
                (b - r) * 4096 / delta + 8192
            } else {
                (r - g) * 4096 / delta + 16384
            };
            let saturation = delta * 256 / max;
            let chroma = value * saturation / 256;
            let low = value - chroma;
            let sector = hue / 4096;
            let product = (hue - ((sector + 1) / 2) * 8192) * chroma;
            let mut middle = product / 4096;
            if product % 4096 > 2048 {
                middle += 1;
            }
            if product % 4096 < -2048 {
                middle -= 1;
            }
            match sector {
                0 => (value, low + middle, low),
                1 => (low - middle, value, low),
                2 => (low, value, low + middle),
                3 => (low, low - middle, value),
                4 => (low + middle, low, value),
                _ => (value, low, low - middle),
            }
        };
        let pack = |v: i32| u32::try_from(v - i32::from(v != 0)).unwrap_or(0);
        (texel & 0xFF00_0000) | (pack(rr) << 16) | (pack(gg) << 8) | pack(bb)
    }

    /// The pair a per-texel [`Self::apply`] cannot answer: the recipe, when this operation is one.
    #[must_use]
    pub const fn icon_recipe(self) -> Option<IconRecipe> {
        match self {
            Self::Icon(r) => Some(r),
            _ => None,
        }
    }

    /// Normal-copy behavior for `A8R8G8B8 -> A8R8G8B8` at full opacity.
    ///
    /// A source copy from ARGB to ARGB is a plain 32-bit store — all four channels, alpha
    /// included. That is why the drag surface never needs clearing: the icon's own normal blit
    /// initialises every texel of it.
    #[must_use]
    pub const fn blit_normal(_dst: u32, src: u32) -> u32 {
        src
    }

    /// The three-channel alpha blit.
    ///
    /// ```text
    /// a = src >> 24;                       // the alpha modifier is 1.0 at every call site here
    /// if (a == 0) leave dst alone;
    /// if (a == 255) dst = (dst & 0xFF000000) | (src & 0x00FFFFFF);   // three channels, not four
    /// else per channel c:  d - (((d * (a+1)) >> 8) - ((s * (a+1)) >> 8))   saturated to 0..255,
    ///      and dst's own alpha byte is left where it is.
    /// ```
    ///
    /// The two separate shifts are the client's and are **not** algebraically `d + ((s-d)*k >> 8)`;
    /// they round apart by one on many texels, which is why they are transcribed rather than
    /// simplified. This is the mode that lets the background tile show around an icon's edge.
    #[must_use]
    pub fn blit_3alpha(dst: u32, src: u32) -> u32 {
        let a = src >> 24;
        if a == 0 {
            return dst;
        }
        if a == 255 {
            return (dst & 0xFF00_0000) | (src & 0x00FF_FFFF);
        }
        let k = i64::from(a + 1);
        let ch = |sh: u32| -> u32 {
            let d = i64::from((dst >> sh) & 0xFF);
            let s = i64::from((src >> sh) & 0xFF);
            let v = d - (((d * k) >> 8) - ((s * k) >> 8));
            u32::try_from(v.clamp(0, 255)).unwrap_or(0)
        };
        (dst & 0xFF00_0000) | (ch(16) << 16) | (ch(8) << 8) | ch(0)
    }

    /// The four-channel alpha blit.
    ///
    /// The same lerp when the destination is opaque, the source outright when the destination is
    /// transparent, and the non-premultiplied *over* when it is neither:
    ///
    /// ```text
    /// a = src >> 24;  if (a == 0) leave dst alone;  if (a == 255) dst = src;
    /// da = dst >> 24; if (da == 0) dst = src;
    /// else if (da == 255) the blit_3alpha lerp, and the alpha byte stays 255;
    /// else { k = a + 1; dk = da + 1;
    ///        outA = dk + k - ((dk * k) >> 8);
    ///        per channel: round(d - ((d - s) * k) / outA), saturated;
    ///        alpha byte = outA - 1; }
    /// ```
    #[must_use]
    pub fn blit_4alpha(dst: u32, src: u32) -> u32 {
        let a = src >> 24;
        if a == 0 {
            return dst;
        }
        if a == 255 {
            return src;
        }
        let da = dst >> 24;
        if da == 0 {
            return src;
        }
        if da == 255 {
            // The alpha lane of the same lerp with d = 255 cannot move below 255 for any s, so the
            // result is opaque and only the three colour channels change.
            return 0xFF00_0000 | (Self::blit_3alpha(dst, src) & 0x00FF_FFFF);
        }
        let k = a + 1;
        let dk = da + 1;
        let out_a = dk + k - ((dk * k) >> 8);
        let ch = |sh: u32| -> u32 {
            let d = f64::from((dst >> sh) & 0xFF);
            let s = f64::from((src >> sh) & 0xFF);
            let v = (d - (d - s) * f64::from(k) / f64::from(out_a)).round();
            u32::try_from(pyreal_primitives::num::to_i32_f64(v).clamp(0, 255)).unwrap_or(0)
        };
        ((out_a - 1) << 24) | (ch(16) << 16) | (ch(8) << 8) | ch(0)
    }
}

/// Compose the drag and base icon surfaces against decoded dat surfaces rather than a locked
/// render surface.
///
/// The client makes a 32 × 32 `A8R8G8B8` local surface, blits into it, and hands the result to its
/// texture cache. This does the same on the CPU and hands the result to the texture
/// uploader.
///
/// # What is faithful here and what is not
///
/// * The **order** and the **modes** are the client's, step for step, and they are
///   the part that a reader can get wrong in a way that still looks plausible: an overlay under the
///   base icon, or the background tile above it, draws a picture that is merely different rather
///   than obviously broken.
/// * The **blend arithmetic** is the client's three-channel and four-channel alpha blits — see
///   [`SurfaceOp::blit_3alpha`] and [`SurfaceOp::blit_4alpha`].
/// * The **extent** is the client's 32 × 32 surface. A source larger than that is
///   clipped, a smaller one covers its own corner and leaves the rest, which is
///   the colouring blit taking the smaller of the two extents on each axis.
/// * **Not faithful:** a source that does not decode is skipped rather than aborting the whole
///   composite. The client's icon renderer guards every blit with a null asset check and does the same.
///
/// Returns `None` when nothing in the recipe decoded, which the caller counts as a decode failure
/// exactly as it counts a missing plain image.
#[must_use]
pub fn composite(
    recipe: IconRecipe,
    fetch: &dyn Fn(DataId) -> Option<pyreal_primitives::TextureData>,
) -> Option<pyreal_primitives::TextureData> {
    let n = SurfaceOp::ICON_EXTENT;
    let mut got_any = false;
    // A freshly created local surface is not cleared by the client either; every recipe whose first
    // blit is present overwrites all of it with a plain blit, and one whose first blit is absent
    // is a picture the client would leave uninitialised. Transparent black is the honest stand-in
    // and it is what the alpha-blended layers above expect.
    let mut main = vec![0u32; (n * n) as usize];

    let load = |id: Option<DataId>| -> Option<Vec<u32>> {
        let d = fetch(id?)?;
        if d.format != pyreal_primitives::TextureFormat::Bgra8 {
            return None;
        }
        let src = d.levels.first()?;
        let mut out = vec![0u32; (n * n) as usize];
        for y in 0..n.min(d.height) {
            for x in 0..n.min(d.width) {
                let at = ((y * d.width + x) * 4) as usize;
                let px = src.get(at..at + 4)?;
                out[(y * n + x) as usize] = u32::from_le_bytes([px[0], px[1], px[2], px[3]]);
            }
        }
        Some(out)
    };

    // `(dst, src, mode, 1.0)` over the whole 32x32.
    fn blit(dst: &mut [u32], src: &[u32], f: fn(u32, u32) -> u32) {
        for (d, s) in dst.iter_mut().zip(src.iter()) {
            *d = f(*d, *s);
        }
    }
    // `(dst, &white, src)` — the *same texel* of the source,
    // not a constant colour, wherever the destination is exactly opaque white.
    fn replace_from(dst: &mut [u32], src: &[u32]) {
        for (d, s) in dst.iter_mut().zip(src.iter()) {
            if *d == SurfaceOp::OPAQUE_WHITE {
                *d = *s;
            }
        }
    }

    match recipe {
        IconRecipe::Object {
            background,
            effects,
            icon,
            overlay,
            underlay,
        } => {
            // The drag-icon pass uses `UIEffectIcons`, not the custom underlay.
            // ---- drag icon: icon, custom overlay, effects by colour replacement --------------
            let mut drag = vec![0u32; (n * n) as usize];
            if let Some(p) = load(icon) {
                got_any = true;
                blit(&mut drag, &p, SurfaceOp::blit_normal);
            }
            if let Some(p) = load(overlay) {
                got_any = true;
                blit(&mut drag, &p, SurfaceOp::blit_4alpha);
            }
            if let Some(p) = load(effects) {
                got_any = true;
                replace_from(&mut drag, &p);
            }
            // The based-icon pass uses the custom underlay, not `UIEffectIcons`.
            // ---- icon: type tile, custom underlay, then the drag surface --------------------
            let mut based = false;
            if let Some(p) = load(background) {
                got_any = true;
                based = true;
                blit(&mut main, &p, SurfaceOp::blit_normal);
            }
            if let Some(p) = load(underlay) {
                got_any = true;
                blit(&mut main, &p, SurfaceOp::blit_3alpha);
            }
            // **One deliberate divergence, and it is in a case the client cannot reach.**
            // The three-channel alpha blit preserves the *destination's* alpha byte, so blitting the drag surface
            // onto a surface nothing has written leaves alpha 0 — an invisible icon. The client
            // blits a `0x10000004` row into the icon surface first every single time (all 34 rows of the
            // mapper resolve, which the backpack grid tests assert), so it never meets this; a
            // rebuild whose mapper lookup failed for one type would silently blank that cell
            // instead of drawing the icon. A plain blit for the un-based case keeps the failure visible as "no tile" rather than "no icon".
            blit(
                &mut main,
                &drag,
                if based {
                    SurfaceOp::blit_3alpha
                } else {
                    SurfaceOp::blit_normal
                },
            );
        }
        IconRecipe::Spell {
            background,
            icon,
            tint,
            overlay,
        } => {
            if let Some(p) = load(background) {
                got_any = true;
                blit(&mut main, &p, SurfaceOp::blit_normal);
            }
            if let Some(p) = load(icon) {
                got_any = true;
                blit(&mut main, &p, SurfaceOp::blit_4alpha);
            }
            if let Some(p) = load(tint) {
                got_any = true;
                replace_from(&mut main, &p);
            }
            if let Some(p) = load(overlay) {
                got_any = true;
                blit(&mut main, &p, SurfaceOp::blit_4alpha);
            }
        }
    }
    if !got_any {
        return None;
    }
    let mut bytes = Vec::with_capacity(main.len() * 4);
    for t in &main {
        bytes.extend_from_slice(&t.to_le_bytes());
    }
    Some(pyreal_primitives::TextureData {
        width: n,
        height: n,
        format: pyreal_primitives::TextureFormat::Bgra8,
        levels: vec![bytes],
    })
}
