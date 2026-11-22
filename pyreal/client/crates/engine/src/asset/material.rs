//! One material record per surface, and the per-map fallback: every map beyond
//! the dat's own picture is optional, one by one, and a surface with none of them draws from the
//! dat and looks right.
//!
//! | Map | From the pack | Without it |
//! |---|---|---|
//! | Base colour | upscaled picture | the dat's texture, palette applied, as it is |
//! | Normal | normal map | the mesh's own normal |
//! | Height | height map | none: no parallax |
//! | Roughness, metal | maps | one value per surface, from its kind, or a plain default |
//! | Occlusion | map | none |
//! | Metal | map: where a metal is | the whole surface, by its kind |
//! | Emissive | none | the dat's luminous flag and colour |

use std::collections::HashMap;

use pyreal_assets::art_pack::Matter;
use pyreal_assets::fingerprint::Fingerprint;
use pyreal_assets::Surface;
use pyreal_codecs::surface::surface_type as st;
use pyreal_primitives::DataId;

use crate::asset::texture::PaletteSwap;

/// The material flags the shaders branch on. The `HAS_*` bits say which maps exist.
pub mod flags {
    pub const CUTOUT: u32 = 1 << 0;
    pub const TRANSLUCENT: u32 = 1 << 1;
    pub const TWO_SIDED: u32 = 1 << 2;
    pub const WATER: u32 = 1 << 3;
    pub const FOLIAGE: u32 = 1 << 4;
    pub const ADDITIVE: u32 = 1 << 5;
    pub const LUMINOUS: u32 = 1 << 6;
    pub const TERRAIN: u32 = 1 << 7;
    pub const HAS_BASE: u32 = 1 << 8;
    pub const HAS_NORMAL: u32 = 1 << 9;
    pub const HAS_HEIGHT: u32 = 1 << 10;
    pub const HAS_ROUGH_METAL: u32 = 1 << 11;
    pub const HAS_OCCLUSION: u32 = 1 << 12;
    /// A metal map: the surface is a metal only where it says, in red.
    pub const HAS_METAL: u32 = 1 << 13;
    /// The roughness, occlusion and height maps are one packed surface picture.
    pub const PACKED_SURFACE: u32 = 1 << 14;
    /// A level of pictures from round a model (an impostor): the base colour and normal pictures
    /// are atlases of views, laid out as the record's views word says.
    pub const IMPOSTOR: u32 = 1 << 15;
    /// The surface blends by its alpha (`ALPHA` or `INVALPHA`): an additive one is weighed by it,
    /// where a plain additive one adds its whole picture (retail's `ONE / ONE`).
    pub const ALPHA: u32 = 1 << 16;
    /// A cobweb card, left out while its silk is drawn ([`crate::webs`]).
    pub const WEB_CARD: u32 = 1 << 17;
    /// Glacier ice (a pack material's `ice`): what lies behind it seen through it, bent and blue.
    pub const ICE: u32 = 1 << 18;
    /// A shadow's see-through body ([`crate::body::shadow_parts`]): smoke frayed at its edges.
    pub const SMOKE: u32 = 1 << 19;
    /// Ice in its picture's own colour, a creature's body ([`ICE`] with it).
    pub const TINTED_ICE: u32 = 1 << 20;
    /// A basin's water ([`super::is_pool`]): drawn as water over what lies under it while water
    /// is drawn as water ([`crate::fountains`]), else as its picture.
    pub const POOL: u32 = 1 << 21;
    /// A player's face or hair picture: the shader draws skin and hair detail over it from its
    /// face map and edge map, whose texture slots the record's views word holds, and the shared
    /// tile in the metal map's slot ([`crate::face_detail`]).
    pub const FACE: u32 = 1 << 22;
    /// An untextured surface's pack material (`[[solid]]`): its maps laid along the world's three
    /// axes, one repeat over as many metres of the object at its own size as the record's
    /// cut-out word says, so it grows with the object.
    pub const PROJECTED: u32 = 1 << 23;
}

/// How much darker a surface under a film of water is drawn, and the film's roughness.
const WET_DARKER: f32 = 0.55;
const WET_ROUGHNESS: f32 = 0.04;
/// The roughness of a surface only a little wet.
const WET_DAMP: f32 = 0.3;

/// What a surface is made of, for its roughness and metal when no map says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SurfaceKind {
    #[default]
    Plain,
    Stone,
    Wood,
    Metal,
    Cloth,
    Earth,
    Foliage,
    Water,
    Snow,
    /// Molten rock: dark stone whose hot texels (red to yellow) glow.
    Molten,
}

impl SurfaceKind {
    pub const ALL: [Self; 10] = [
        Self::Plain,
        Self::Stone,
        Self::Wood,
        Self::Metal,
        Self::Cloth,
        Self::Earth,
        Self::Foliage,
        Self::Water,
        Self::Snow,
        Self::Molten,
    ];

    /// `(roughness, metal)` for a surface of this kind with no map.
    #[must_use]
    pub fn roughness_metal(self) -> (f32, f32) {
        match self {
            Self::Plain => (0.8, 0.0),
            Self::Stone => (0.75, 0.0),
            Self::Wood => (0.7, 0.0),
            Self::Metal => (0.35, 1.0),
            Self::Cloth => (0.95, 0.0),
            Self::Earth => (0.95, 0.0),
            Self::Foliage => (0.7, 0.0),
            Self::Water => (0.06, 0.0),
            Self::Snow => (0.55, 0.0),
            Self::Molten => (0.8, 0.0),
        }
    }

    /// The index the shaders know this kind by.
    #[must_use]
    pub fn index(self) -> u32 {
        Self::ALL
            .iter()
            .position(|&k| k == self)
            .and_then(|i| u32::try_from(i).ok())
            .unwrap_or(0)
    }

    /// The kind a terrain type's name suggests (the region names each one).
    #[must_use]
    pub fn of_terrain_name(name: &str) -> Self {
        let n = name.to_ascii_lowercase();
        if n.contains("water") || n.contains("sea") {
            Self::Water
        } else if n.contains("snow") || n.contains("ice") {
            Self::Snow
        } else if n.contains("volcano") || n.contains("lava") {
            Self::Molten
        } else if n.contains("rock") || n.contains("obsidian") {
            Self::Stone
        } else if n.contains("grass") || n.contains("forest") || n.contains("lush") {
            Self::Foliage
        } else {
            Self::Earth
        }
    }
}

/// Retail's water in a fountain's, a basin's or a tub's bowl (`fountain_survey --water`): about
/// 3,500 placed fountains, basins, troughs, wells and tubs, and no room; and a horned fountain's.
pub const POOL_PICTURES: [u32; 2] = [0x0500_01B6, 0x0500_2921];

/// The melted snowman's puddle (0x02000FF7): retail's one see-through cyan with no picture.
const PUDDLE: (u32, f32) = (0xFF00_FFFF, 0.65);

/// Whether a surface is a basin's or a tub's water: retail's water picture, seen through or not,
/// or the melted snowman's puddle ([`PUDDLE`]).
#[must_use]
pub fn is_pool(surface: &Surface) -> bool {
    let pictured = surface.surface_type & (st::BASE1_IMAGE | st::BASE1_CLIPMAP) != 0
        && surface
            .orig_texture_id
            .is_some_and(|t| POOL_PICTURES.contains(&t.0));
    let puddle = surface.surface_type & st::BASE1_SOLID != 0
        && surface.surface_type & st::TRANSLUCENT != 0
        && surface.color_value == Some(PUDDLE.0)
        && (surface.translucency - PUDDLE.1).abs() < 0.005;
    pictured || puddle
}

/// Retail's pictures of molten rock beside the volcano ground: the dungeons' lava cells, and the
/// smaller picture the 1999 and 2005 files lay there (`docs/ERAS.md`).
const MOLTEN_PICTURES: [u32; 2] = [0x0500_2A45, 0x0500_1824];

/// Pictures molten only in one palette (0: any), by id, the picture's fingerprint and the palette:
/// the 1999 golems' one picture in the magma golem's palette (`docs/ERAS.md`), and end of retail's
/// Magma Golem 0x020007CB, whose cracks glow in place of its core's fire.
const MOLTEN_DRESSES: [(u32, &str, u32); 4] = [
    (0x0500_1310, "picture 128x128 0c1966a006f9ffae", 0x0400_0BA5),
    (0x0500_18B4, "picture 128x128 b2a82a60277c2808", 0),
    (0x0500_18B5, "picture 128x128 b2a902642d3e2420", 0),
    (0x0500_18B6, "picture 64x64 fc0428dad040317f", 0),
];

/// Whether `texture` drawn in `palettes` is molten rock, where the world's files hold that picture
/// (`print` reads a record's fingerprint from them).
#[must_use]
pub fn molten_dress(
    texture: Option<DataId>,
    palettes: &[DataId],
    print: &dyn Fn(DataId) -> Option<Fingerprint>,
) -> bool {
    let Some(t) = texture else {
        return false;
    };
    MOLTEN_DRESSES.iter().any(|&(id, made_from, palette)| {
        id == t.0
            && (palette == 0 || palettes.contains(&DataId(palette)))
            && Fingerprint::parse(made_from)
                .zip(print(t))
                .is_some_and(|(a, b)| a.matches(&b))
    })
}

/// A surface's kind, keyed by texture id: small, filled at load from what the dats name (the
/// terrain types) and extendable by the art pack. Unknown textures are [`SurfaceKind::Plain`].
#[derive(Debug, Clone, Default)]
pub struct KindTable {
    // ORDER-OK: looked up by texture id, never iterated.
    by_texture: HashMap<u32, SurfaceKind>,
}

impl KindTable {
    pub fn insert(&mut self, texture: DataId, kind: SurfaceKind) {
        self.by_texture.insert(texture.0, kind);
    }

    #[must_use]
    pub fn kind(&self, texture: Option<DataId>) -> SurfaceKind {
        texture
            .and_then(|t| {
                self.by_texture.get(&t.0).copied().or_else(|| {
                    MOLTEN_PICTURES
                        .contains(&t.0)
                        .then_some(SurfaceKind::Molten)
                })
            })
            .unwrap_or_default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_texture.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_texture.is_empty()
    }
}

/// Where a texture's pixels come from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TextureKey {
    /// A dat picture, with its clip-map keying and an optional palette variant.
    Dat {
        id: DataId,
        clip_map: bool,
        swap: Option<PaletteSwap>,
    },
    /// A picture the art pack supplies, by its name in the pack.
    Pack(String),
    /// A dat ground picture in terrain type `terrain_type`'s Classic tint
    /// ([`crate::ground_tint`]).
    ClassicGround { id: DataId, terrain_type: u32 },
    /// End of retail's ground picture beside an older world, untinted: the later files hold other
    /// pictures than the world's under the same ids ([`crate::world::LaterGround`]).
    LaterGround { id: DataId },
    /// End of Retail's room picture beside an older world, tinted toward the older picture by
    /// `fit` under Classic Ground Coloring ([`crate::later_rooms`]).
    LaterPicture {
        id: DataId,
        fit: Option<crate::later_rooms::FitKey>,
    },
    /// A face or hair picture's face map ([`crate::face_detail::face_map`]).
    FaceMap { id: DataId },
    /// A face or hair picture's edge map ([`crate::face_detail::edge_map`]).
    FaceEdges { id: DataId },
    /// The face detail's shared tile ([`crate::face_detail::tile`]).
    FaceTile,
    /// A picture of End of Retail's model beside an older world, with its clip-map keying, tinted
    /// toward the older model by `fit` under Classic Ground Coloring ([`crate::later_models`]).
    LaterModel {
        id: DataId,
        clip_map: bool,
        fit: Option<crate::later_rooms::FitKey>,
    },
}

/// The maps the art pack supplies for one surface. Every one is optional.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PackMaps {
    pub base: Option<TextureKey>,
    pub normal: Option<TextureKey>,
    pub height: Option<TextureKey>,
    pub roughness_metal: Option<TextureKey>,
    pub occlusion: Option<TextureKey>,
    /// Where the surface is a metal, in red; the whole of it where there is none.
    pub metal: Option<TextureKey>,
    /// A kind for the surface, in place of the table's.
    pub kind: Option<SurfaceKind>,
    /// The roughness, occlusion and height maps are one picture holding them in red, green and
    /// blue (a pack material's surface map), not glTF's roughness and metal in green and blue.
    pub packed_surface: bool,
    /// Glacier ice: drawn as ice whatever the surface blends by.
    pub ice: bool,
    /// What it is made of: a metal, how smooth, or clear as ice in its own colour.
    pub matter: Matter,
    /// An untextured surface's material: the metres one repeat of its maps covers on the object at
    /// its own size, laid along the world's axes, its base map a grain over the surface's own colour.
    pub projected: Option<f32>,
}

/// The art pack's hook into the material table: given a surface and its dat texture, the maps the
/// pack has for it ([`NoOverrides`] without a pack; `crate::pack::PackOverrides` with one).
pub trait MaterialOverrides {
    fn maps(&self, surface: DataId, texture: Option<DataId>) -> PackMaps;

    /// The pack's `[[solid]]` maps for the untextured surface `surface` of solid colour `colour`
    /// (ARGB), laid along the world's axes ([`PackMaps::projected`]).
    fn solid_maps(&self, _surface: DataId, _colour: u32) -> PackMaps {
        PackMaps::default()
    }

    /// [`Self::maps`] for a world whose pictures' fingerprints `record` gives, so the pack's
    /// entry for that era's picture is drawn (`docs/ERAS.md`), the picture drawn in `palettes`.
    fn maps_for(
        &self,
        surface: DataId,
        texture: Option<DataId>,
        _palettes: &[DataId],
        _record: &dyn Fn(DataId) -> Option<pyreal_assets::fingerprint::Fingerprint>,
    ) -> PackMaps {
        self.maps(surface, texture)
    }
}

/// No art pack: every surface draws from the dats.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoOverrides;

impl MaterialOverrides for NoOverrides {
    fn maps(&self, _surface: DataId, _texture: Option<DataId>) -> PackMaps {
        PackMaps::default()
    }
}

/// One material, before its textures have slots: what [`describe_surface`] decides.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialDesc {
    /// Linear RGB and alpha: the solid colour of an untextured surface, white otherwise.
    pub base_color: [f32; 4],
    /// How much of the base colour the surface gives off by itself (the dat's luminosity).
    pub emissive: f32,
    pub roughness: f32,
    pub metal: f32,
    pub kind: SurfaceKind,
    pub flags: u32,
    pub base: Option<TextureKey>,
    pub normal: Option<TextureKey>,
    pub height: Option<TextureKey>,
    pub roughness_metal: Option<TextureKey>,
    pub occlusion: Option<TextureKey>,
    pub metal_map: Option<TextureKey>,
    /// Clear ice in its own colour ([`flags::TINTED_ICE`]): the share of the world behind seen
    /// through it, and its index of refraction.
    pub clear: [f32; 2],
    /// How many times brighter its picture is drawn where its metal map says it is a metal.
    pub brighten: f32,
    /// A face or hair picture's face map, edge map and the shared tile ([`flags::FACE`]).
    pub face: Option<[TextureKey; 3]>,
    /// The metres one repeat of a projected material covers ([`flags::PROJECTED`]).
    pub projected: Option<f32>,
}

/// The solid colour (ARGB) of a surface drawn with no picture, `None` for one with a picture.
#[must_use]
pub fn untextured_colour(surface: &Surface) -> Option<u32> {
    (surface.surface_type & (st::BASE1_IMAGE | st::BASE1_CLIPMAP) == 0)
        .then_some(surface.color_value)
        .flatten()
}

/// An sRGB byte as linear light.
#[must_use]
pub fn srgb_to_linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        pyreal_primitives::num::math::powf((c + 0.055) / 1.055, 2.4)
    }
}

/// The material of one surface drawn `two_sided` or not, from its record, the kind table and
/// whatever the pack supplies, with each missing map taking its fallback. `swap` is the palette
/// variant its picture is drawn with, if any.
#[must_use]
pub fn describe_surface(
    surface: &Surface,
    two_sided: bool,
    swap: Option<PaletteSwap>,
    kinds: &KindTable,
    pack: PackMaps,
) -> MaterialDesc {
    let ty = surface.surface_type;
    let textured = ty & (st::BASE1_IMAGE | st::BASE1_CLIPMAP) != 0;
    let texture = surface.orig_texture_id.filter(|_| textured);
    let clip_map = ty & st::BASE1_CLIPMAP != 0;
    let clear = pack.matter.clear;
    let ice = pack.ice || clear.is_some();
    let translucent = !ice && ty & st::TRANSLUCENT != 0;
    let additive = !ice && ty & st::ADDITIVE != 0;
    let alpha = !ice && ty & (st::ALPHA | st::INVALPHA) != 0;
    let blended = translucent || additive || alpha;
    let mut kind = pack.kind.unwrap_or_else(|| kinds.kind(texture));
    if pack.matter.metal.is_some() {
        kind = SurfaceKind::Metal;
    }
    let (mut roughness, metal) = kind.roughness_metal();
    if let Some(g) = pack.matter.gloss {
        roughness = 1.0 - g;
    }

    let mut flags = 0;
    // Base colour: the pack's picture, else the dat's, else the record's solid colour.
    let base = pack
        .base
        .or_else(|| texture.map(|id| TextureKey::Dat { id, clip_map, swap }));
    let mut base_color = [1.0, 1.0, 1.0, 1.0];
    // A projected material's grain multiplies the surface's own colour.
    let grain = pack.projected.is_some() && texture.is_none();
    if base.is_some() && !grain {
        flags |= flags::HAS_BASE;
    } else if let Some(argb) = surface.color_value {
        let [b, g, r, _] = argb.to_le_bytes();
        base_color = [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b), 1.0];
        if grain && base.is_some() {
            flags |= flags::HAS_BASE;
        }
    }
    if grain {
        flags |= flags::PROJECTED;
    }
    if translucent {
        base_color[3] = (1.0 - surface.translucency).clamp(0.0, 1.0);
    }
    if clip_map && !blended {
        flags |= flags::CUTOUT;
    }
    if blended {
        flags |= flags::TRANSLUCENT;
    }
    if additive {
        flags |= flags::ADDITIVE;
    }
    if alpha {
        flags |= flags::ALPHA;
    }
    if ice {
        flags |= flags::ICE;
    }
    if clear.is_some() {
        flags |= flags::TINTED_ICE;
    }
    if !ice && !additive && is_pool(surface) {
        flags |= flags::POOL;
    }
    // A metal's dark picture brightened toward what a metal mirrors; with a metal map, only
    // where it is a metal (the record's `emissive.w`).
    let brighten = pack.matter.metal.unwrap_or(1.0);
    if pack.metal.is_none() {
        for c in &mut base_color[..3] {
            *c *= brighten;
        }
    }
    // Under a film of water: darker, as smooth as a puddle.
    if let Some(w) = pack.matter.wet {
        for c in &mut base_color[..3] {
            *c *= 1.0 - WET_DARKER * w;
        }
        roughness = roughness.min(WET_DAMP + (WET_ROUGHNESS - WET_DAMP) * w);
    }
    if two_sided {
        flags |= flags::TWO_SIDED;
    }
    match kind {
        SurfaceKind::Water => flags |= flags::WATER,
        SurfaceKind::Foliage => flags |= flags::FOLIAGE,
        _ => {}
    }
    for (map, bit) in [
        (&pack.normal, flags::HAS_NORMAL),
        (&pack.height, flags::HAS_HEIGHT),
        (&pack.roughness_metal, flags::HAS_ROUGH_METAL),
        (&pack.occlusion, flags::HAS_OCCLUSION),
        (&pack.metal, flags::HAS_METAL),
    ] {
        if map.is_some() {
            flags |= bit;
        }
    }
    if pack.packed_surface {
        flags |= flags::PACKED_SURFACE;
    }
    // Emissive: the pack's map, else the dat's luminosity over the base colour (retail's material
    // emissive, the luminous flag or not).
    let emissive = if surface.luminosity > 0.0 {
        flags |= flags::LUMINOUS;
        surface.luminosity.min(1.0)
    } else {
        0.0
    };
    MaterialDesc {
        base_color,
        emissive,
        roughness,
        metal,
        kind,
        flags,
        base,
        normal: pack.normal,
        height: pack.height,
        roughness_metal: pack.roughness_metal,
        occlusion: pack.occlusion,
        metal_map: pack.metal,
        clear: [clear.unwrap_or(0.0), pack.matter.refraction],
        brighten: if flags & flags::HAS_METAL != 0 {
            brighten
        } else {
            1.0
        },
        face: None,
        projected: pack.projected.filter(|_| grain),
    }
}

/// A material record as the shaders read it (80 bytes, std430).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MaterialRecord {
    pub base_color: [f32; 4],
    /// Luminosity, clear ice's clearness and refraction, and a metal map's brightening.
    pub emissive: [f32; 4],
    /// Texture slots: base, normal, height, roughness/metal, occlusion, metal map.
    pub textures: [u32; 6],
    pub flags: u32,
    pub kind: u32,
    pub roughness: f32,
    pub metal: f32,
    pub alpha_cutoff: f32,
    /// An impostor's views: how many, and (shifted by 8) how many to a row of its atlas.
    pub pad: u32,
}

/// The bytes of one material record.
pub const MATERIAL_RECORD_SIZE: u64 = 80;

/// A texture slot that holds nothing.
pub const NO_TEXTURE: u32 = u32::MAX;

impl MaterialRecord {
    /// The record for `desc`, with each map's texture slot from `slot` ([`NO_TEXTURE`] where the
    /// map is missing or will not load, whose `HAS_*` flag then goes too).
    pub fn new(desc: &MaterialDesc, mut slot: impl FnMut(&TextureKey) -> Option<u32>) -> Self {
        let mut flags = desc.flags;
        let mut take = |map: &Option<TextureKey>, bit: u32| -> u32 {
            match map.as_ref().and_then(&mut slot) {
                Some(s) => s,
                None => {
                    flags &= !bit;
                    NO_TEXTURE
                }
            }
        };
        let textures = [
            take(&desc.base, flags::HAS_BASE),
            take(&desc.normal, flags::HAS_NORMAL),
            take(&desc.height, flags::HAS_HEIGHT),
            take(&desc.roughness_metal, flags::HAS_ROUGH_METAL),
            take(&desc.occlusion, flags::HAS_OCCLUSION),
            take(&desc.metal_map, flags::HAS_METAL),
        ];
        // A face's two slots share the views word, which only an impostor reads otherwise.
        // The shared tile takes the metal map's slot, which a face has none for.
        let mut pad = 0;
        let mut textures = textures;
        if flags & flags::FACE != 0 {
            match desc
                .face
                .as_ref()
                .map(|[m, e, t]| (slot(m), slot(e), slot(t)))
            {
                Some((Some(m), Some(e), Some(t)))
                    if m < 0x1_0000 && e < 0x1_0000 && flags & flags::HAS_METAL == 0 =>
                {
                    pad = m | (e << 16);
                    textures[5] = t;
                }
                _ => flags &= !flags::FACE,
            }
        }
        Self {
            base_color: desc.base_color,
            // Clear ice keeps its clearness and refraction beside its luminosity, and a metal map
            // its brightening.
            emissive: [desc.emissive, desc.clear[0], desc.clear[1], desc.brighten],
            textures,
            flags,
            kind: desc.kind.index(),
            roughness: desc.roughness,
            metal: desc.metal,
            // An untextured surface never cuts out: a projected one's repeat takes the word.
            alpha_cutoff: desc
                .projected
                .filter(|_| flags & flags::PROJECTED != 0)
                .unwrap_or(0.5),
            pad,
        }
    }

    /// Appends the record in the shaders' layout.
    pub fn write(&self, out: &mut Vec<u8>) {
        let f = |out: &mut Vec<u8>, v: f32| out.extend_from_slice(&v.to_le_bytes());
        let u = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
        for v in self.base_color {
            f(out, v);
        }
        for v in self.emissive {
            f(out, v);
        }
        for v in self.textures {
            u(out, v);
        }
        u(out, self.flags);
        u(out, self.kind);
        f(out, self.roughness);
        f(out, self.metal);
        f(out, self.alpha_cutoff);
        u(out, self.pad);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_1999_golem_picture_is_molten_only_in_the_magma_palette_and_only_as_1999_drew_it() {
        let golem = Some(DataId(0x0500_1310));
        let as_1999 = |_| Fingerprint::parse("picture 128x128 0c1966a006f9ffae");
        let other = |_| Fingerprint::parse("picture 128x128 f3e6995ff9060051");
        let magma = [DataId(0x0400_0BA5)];
        assert!(molten_dress(golem, &magma, &as_1999));
        assert!(!molten_dress(golem, &[DataId(0x0400_0B9E)], &as_1999));
        assert!(!molten_dress(golem, &magma, &other));
        assert!(!molten_dress(golem, &magma, &|_| None));
        assert!(!molten_dress(Some(DataId(0x0500_1311)), &magma, &as_1999));
    }

    #[test]
    fn the_magma_golems_pictures_are_molten_in_any_palette_as_end_of_retail_drew_them() {
        let core = Some(DataId(0x0500_18B4));
        let as_eor = |_| Fingerprint::parse("picture 128x128 b2a82a60277c2808");
        let other = |_| Fingerprint::parse("picture 128x128 f3e6995ff9060051");
        assert!(molten_dress(core, &[], &as_eor));
        assert!(molten_dress(core, &[DataId(0x0400_0B9E)], &as_eor));
        assert!(!molten_dress(core, &[], &other));
        assert!(!molten_dress(core, &[], &|_| None));
    }
}
