//! The creation tables the creation wizard and the barber read, the appearance parts, the
//! preview's animation enums, and the loader that reads the tables from the dat.

use std::rc::Rc;
use std::sync::Arc;

pub use pyreal_chargen::palette::PaletteSample;
use pyreal_chargen::{HERITAGE_OLTHOI, HERITAGE_OLTHOI_ACID};
use pyreal_primitives::DataId;

/// The appearance parts, in the client's enum order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum EParts {
    Invalid = 0,
    Hair = 1,
    Eyes = 2,
    Nose = 3,
    Mouth = 4,
    Skin = 5,
    Headgear = 6,
    Shirt = 7,
    Trousers = 8,
    Footwear = 9,
}

/// The animation enums the preview uses for an ordinary heritage, then the Olthoi (`0x0C`) and
/// OlthoiAcid (`0x0D`) overrides. The pairs are `(running, rest)` followed by the five
/// animation-array entries.
pub const ANIM_ENUMS_DEFAULT: (u32, u32, [u32; 5]) = (
    0x1000_0006,
    0x1000_0005,
    [
        0x1000_0007,
        0x1000_0008,
        0x1000_0009,
        0x1000_000A,
        0x1000_000B,
    ],
);
/// Olthoi: one animation enum for both, and the **same** enum five times in the array.
pub const ANIM_ENUMS_OLTHOI: (u32, u32, [u32; 5]) = (0x1000_0011, 0x1000_0011, [0x1000_0012; 5]);
/// OlthoiAcid, the same shape.
pub const ANIM_ENUMS_OLTHOI_ACID: (u32, u32, [u32; 5]) =
    (0x1000_0013, 0x1000_0013, [0x1000_0014; 5]);

/// The rest animation enum the preview holds a heritage's model in.
#[must_use]
pub const fn rest_animation_enum(heritage_group: u32) -> u32 {
    match heritage_group {
        HERITAGE_OLTHOI => ANIM_ENUMS_OLTHOI.1,
        HERITAGE_OLTHOI_ACID => ANIM_ENUMS_OLTHOI_ACID.1,
        _ => ANIM_ENUMS_DEFAULT.1,
    }
}

/// The two dat tables creation reads, read from `DidMapper 0x25000002` (`UNIQUEDB`) entries
/// **0x0E** `CharGen_CharacterData` and **4** `Weenie_SkillTable`.
#[derive(Debug)]
pub struct CharGenTables {
    /// The world rules and appearance resources.
    pub world: Rc<pyreal_chargen::CreationTables>,
    /// The colour source across the host seam, or `None` where a caller has no dat behind it.
    pub colors: Option<Rc<dyn CgColorSource>>,
    /// The creation texts the world's own files carry (a world from before Throne of Destiny).
    pub texts: pyreal_chargen::CreationTexts,
}

impl std::ops::Deref for CharGenTables {
    type Target = pyreal_chargen::CreationTables;
    fn deref(&self) -> &Self::Target {
        &self.world
    }
}

/// The char-gen state's "get colour from palette" and the averaging loop
/// the appearance page's selection change wraps it in — the seam that gets a colour out of
/// the dat and into the wheel.
///
/// The client asks for a `Palette` as resource type 10 and a `PalSet` as type `0x18`
/// wherever it needs one. This crate never reads the dat, so the host implements this and hands it
/// over on [`CharGenTables`]; it is **lazy** rather than a precomputed table because the set of
/// palettes a wheel needs depends on the style that is currently chosen.
pub trait CgColorSource: std::fmt::Debug {
    /// The ordered palette IDs in a `PalSet`. Barber Start needs the inverse of
    /// the native integral palette lookup: find the incoming concrete palette in this list and
    /// turn
    /// its one-based position back into the shade fraction.
    fn pal_set_palettes(&self, pal_set: DataId) -> Option<Vec<DataId>>;

    /// The colour of one palette entry — the palette's 32-bit colour read of one entry, ARGB.
    /// `None` when the palette is not in the dat, which the client answers with an error log line
    /// and a colour of **0**.
    fn palette_color(&self, palette: DataId, sample: PaletteSample) -> Option<u32>;

    /// The selection path's loop over a `PalSet`: for each palette in the set it reads the colour
    /// for `sample` and sums the red, green and blue bytes, then divides each sum by the palette
    /// count. So a spot is the **mean** of the whole set at one palette offset, per channel, with
    /// integer division. Alpha is not accumulated; the spot is opaque.
    fn pal_set_color(&self, pal_set: DataId, sample: PaletteSample) -> Option<u32>;
}

/// Read the creation tables from `store`: `None` when they are not there or do not decode.
#[must_use]
pub fn load(store: &Arc<pyreal_dat::RetailDatStore>) -> Option<Rc<CharGenTables>> {
    use pyreal_primitives::AssetSource;
    let assets: &dyn AssetSource = &**store;
    let world = pyreal_chargen::CreationTables::read(assets)?;
    let texts = pyreal_chargen::CreationTexts::read(assets, &world.chargen);
    Some(Rc::new(CharGenTables {
        texts,
        world: Rc::new(world),
        colors: Some(Rc::new(DatColorSource {
            store: Arc::clone(store),
            cache: std::cell::RefCell::new(std::collections::BTreeMap::new()),
        })),
    }))
}

/// Palette and palette-set lookup over the dat, with a cache: [`CgColorSource`] over a dat set.
struct DatColorSource {
    store: Arc<pyreal_dat::RetailDatStore>,
    cache: std::cell::RefCell<std::collections::BTreeMap<(DataId, PaletteSample), Option<u32>>>,
}

impl std::fmt::Debug for DatColorSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatColorSource")
            .field("cached", &self.cache.borrow().len())
            .finish()
    }
}

impl DatColorSource {
    fn read_palette_color(&self, palette: DataId, sample: PaletteSample) -> Option<u32> {
        use pyreal_assets::Decode;
        let bytes = pyreal_primitives::AssetSource::read(&*self.store, palette).ok()?;
        let p = pyreal_assets::material::Palette::decode_payload(palette, &bytes).ok()?;
        let layout = pyreal_chargen::palette::PaletteLayout::from_entry_count(p.colors_argb.len())?;
        p.colors_argb.get(sample.index(layout)).copied()
    }
}

impl CgColorSource for DatColorSource {
    fn pal_set_palettes(&self, pal_set: DataId) -> Option<Vec<DataId>> {
        use pyreal_assets::Decode;
        let bytes = pyreal_primitives::AssetSource::read(&*self.store, pal_set).ok()?;
        let set = pyreal_assets::material::PaletteSet::decode_payload(pal_set, &bytes).ok()?;
        Some(set.palette_ids)
    }

    fn palette_color(&self, palette: DataId, sample: PaletteSample) -> Option<u32> {
        if let Some(hit) = self.cache.borrow().get(&(palette, sample)) {
            return *hit;
        }
        let v = self.read_palette_color(palette, sample);
        self.cache.borrow_mut().insert((palette, sample), v);
        v
    }

    fn pal_set_color(&self, pal_set: DataId, sample: PaletteSample) -> Option<u32> {
        // The key space is shared with `palette_color`'s: a `PalSet` id and a `Palette` id never
        // collide.
        if let Some(hit) = self.cache.borrow().get(&(pal_set, sample)) {
            return *hit;
        }
        let v = (|| {
            use pyreal_assets::Decode;
            let bytes = pyreal_primitives::AssetSource::read(&*self.store, pal_set).ok()?;
            let s = pyreal_assets::material::PaletteSet::decode_payload(pal_set, &bytes).ok()?;
            let n = u32::try_from(s.palette_ids.len()).ok()?;
            if n == 0 {
                return None;
            }
            // Each channel summed over the whole set, then divided by its count.
            let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
            for p in &s.palette_ids {
                let c = self.palette_color(*p, sample).unwrap_or(0);
                r += (c >> 16) & 0xFF;
                g += (c >> 8) & 0xFF;
                b += c & 0xFF;
            }
            Some(0xFF00_0000 | ((r / n) << 16) | ((g / n) << 8) | (b / n))
        })();
        self.cache.borrow_mut().insert((pal_set, sample), v);
        v
    }
}
