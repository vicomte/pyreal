//! Art packs: models for retail objects, drawn by the high-fidelity presentation and the new
//! engine in place of the objects they replace.
//!
//! **Depends on** `pyreal-primitives`, `pyreal-assets` (the manifest), `pyreal-terrain` (the
//! retail ground a pack's ground is laid with), `pyreal-dat` (the file a pack is packed into) and
//! `pyreal-codecs` (the block codecs). **Used by** the drawn world (`pyreal-scene`, which
//! re-exports it as `art_pack` for the art tool) and the new engine
//! (`pyreal-engine`).
//!
//! **Must never** need a device or reach the client runtime: it opens a pack into plain data,
//! and each renderer uploads it its own way.
//!
//! **A pack is a folder** ([`ArtPack::open_dir`]): `manifest.toml` at its top
//! ([`pyreal_assets::art_pack`]), naming glTF 2.0 models by path, with their pictures beside them.
//! Everything is read through one function of a path ([`ArtPack::open`]). A model that will not
//! read is left out with a line saying why, and the rest of the pack is drawn.
//!
//! **Or the folder packed into one file** ([`ArtPack::open_packed`], [`packed`]): the same files,
//! read through the same paths, each a record, beside every picture already made as it is made
//! here, so the file opens without decoding or compressing a picture. [`ArtPack::open_path`]
//! opens either.
//!
//! **A pack may lay the ground** ([`PackGround`]): a material for each retail terrain type it
//! covers, its pictures packed three to a material and brought to one size, so the presentation
//! holds them all in three arrays.
//!
//! **A pack may draw its own pictures in place of retail ones** ([`TextureMaterial`]): each
//! material names the retail pictures it stands for, and wherever a retail object or room is
//! drawn with one of them, the presentation draws it with the material on the object's own
//! coordinates and surface flags. A packed pack's material pictures are read again from its file
//! as they are drawn ([`ArtPack::material_picture`]), so a pack of upscaled pictures holds only
//! their sizes once open.
//!
//! **Pictures are compressed as they are read** ([`images::compressed`]): a model's pictures in
//! BC7, a ground material's colour and height in BC7, its normal in BC5, and its roughness and
//! occlusion in BC5 at half its size, a quarter of the memory or less of eight bits a channel.
//! A model's picture whose sides are not whole blocks of four is first stretched to the next
//! whole block on each side.
//!
//! **A model with no pictures of its own gets a card** ([`cards`]): drawn from round it as the pack
//! opens, and drawn in place of its shape far off.

pub mod cards;
pub mod fingerprint;
pub mod gltf;
pub mod images;
pub mod materials;
pub mod packed;
pub mod skin;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use pyreal_assets::art_pack::{ArtPackManifest, Credit, DrawsOn, Fingerprint, ManifestGround};

pub use pyreal_assets::art_pack::{Ground, MANIFEST_FILE};

/// The name a pack's manifest gives it, when the manifest reads.
#[must_use]
pub fn manifest_name(text: &str) -> Option<String> {
    ArtPackManifest::parse(text).ok().map(|m| m.name)
}
use pyreal_primitives::DataId;

pub use gltf::VERTEX_FLOATS;
pub use images::Image;
pub use materials::TextureMaterial;

/// A metallic-roughness material, its pictures by index into [`ArtPack::images`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PackMaterial {
    /// The base colour factor, linear, with alpha.
    pub base_color: [f32; 4],
    /// The base colour picture.
    pub base_color_image: Option<u32>,
    /// The tangent-space normal picture.
    pub normal_image: Option<u32>,
    /// The normal picture's strength.
    pub normal_scale: f32,
    /// The metallic-roughness picture.
    pub metal_rough_image: Option<u32>,
    /// The metalness factor.
    pub metallic: f32,
    /// The roughness factor.
    pub roughness: f32,
    /// The occlusion picture.
    pub occlusion_image: Option<u32>,
    /// The occlusion's strength.
    pub occlusion_strength: f32,
    /// The alpha a pixel is cut below, for a cut-out material.
    pub alpha_cutoff: Option<f32>,
    /// Whether both faces are drawn.
    pub double_sided: bool,
}

/// One part of a level of detail: triangles of one material.
#[derive(Debug, Clone, PartialEq)]
pub struct PackPrimitive {
    /// [`VERTEX_FLOATS`] floats a vertex, model space, height up.
    pub vertices: Arc<Vec<f32>>,
    /// Three a triangle, counter-clockwise seen from the front.
    pub indices: Arc<Vec<u32>>,
    /// How it is drawn.
    pub material: PackMaterial,
}

/// One level of detail.
#[derive(Debug, Clone, PartialEq)]
pub struct PackLod {
    /// How far from the eye it is drawn, metres; infinite for a last level with no limit.
    pub distance: f32,
    /// Its parts.
    pub primitives: Vec<PackPrimitive>,
    /// For a level that is pictures of the model seen from round it, how they are laid out
    /// ([`pyreal_assets::art_pack`] says how such a level is drawn).
    pub views: Option<pyreal_assets::art_pack::Views>,
    /// Drawn into the sun's shadows alone, never into the view.
    pub shadows_only: bool,
}

/// One model of a pack.
#[derive(Debug, Clone, PartialEq)]
pub struct PackModel {
    /// The retail object it is drawn in place of.
    pub id: DataId,
    /// Cut-out growth: its cut-out materials (needles, leaves) are drawn from both sides and lit
    /// through from behind.
    pub foliage: bool,
    /// How much the wind moves it: 0 not at all, 1 as much as growth of its height moves.
    pub wind: f32,
    /// Its levels of detail, nearest first.
    pub lods: Vec<PackLod>,
    /// A sphere holding every level, model space: centre and radius.
    pub bounds: ([f32; 3], f32),
    /// The fingerprint of the retail object it was made for, where it names one.
    pub fingerprint: Option<Fingerprint>,
    /// Which records it is drawn on.
    pub draws_on: DrawsOn,
    /// Who made it, with what, and under which license.
    pub credit: Credit,
}

/// One ground material of a pack, its pictures packed for drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct PackGround {
    /// The retail grounds it is drawn for.
    pub grounds: Vec<Ground>,
    /// The base colour, in the display's encoding, with the height in alpha.
    pub albedo: Image,
    /// The normal's east and north parts (-1..1 as 0..1), in red and green, the base colour's
    /// size, or one block where it is flat.
    pub normal: Image,
    /// The roughness and the ambient occlusion, in red and green, half the base colour's size, or
    /// one block where the material names neither.
    pub surface: Image,
    /// How many metres one repeat of the pictures covers.
    pub size: f32,
    /// How strongly the normal picture bends the normal.
    pub normal_scale: f32,
    /// Laid on the steep slopes of its grounds rather than on their flat ground.
    pub steep: bool,
    /// The fingerprint of the retail picture its grounds are laid with in the world it was made
    /// for, where it names one ([`ManifestGround::fingerprint`]).
    pub fingerprint: Option<Fingerprint>,
    /// A grain multiplied over it near the eye, gray in red, with its levels.
    pub detail: Option<Image>,
    /// How many metres one repeat of the detail covers.
    pub detail_size: f32,
    /// How far the detail moves its colour.
    pub detail_strength: f32,
    /// Whether the presentation's pebbles lie on its grounds.
    pub pebbles: bool,
    /// Who made it, with what, and under which license.
    pub credit: Credit,
}

impl PackGround {
    /// The ground material `entry` drawn with `pictures`: its base colour, normal and surface, as
    /// [`ground`] makes them.
    fn with_pictures(entry: &ManifestGround, pictures: [Image; 3]) -> Self {
        let [albedo, normal, surface] = pictures;
        Self {
            grounds: entry.grounds.clone(),
            albedo,
            normal,
            surface,
            size: entry.size,
            normal_scale: entry.normal_scale,
            steep: entry.steep,
            fingerprint: entry.fingerprint,
            detail: None,
            detail_size: entry.detail_size,
            detail_strength: entry.detail_strength,
            pebbles: entry.pebbles,
            credit: entry.credit.clone(),
        }
    }
}

/// What a ground material takes where it names no picture: a level height, a flat normal, a
/// rough surface and no occlusion.
const GROUND_DEFAULTS: [u8; 4] = [128, 128, 230, 255];

/// A ground's three pictures from its base colour with the height in alpha (`albedo`), its
/// normal in red and green (`normal`), both `side` texels a side, and its roughness and
/// occlusion in red and green (`surface`), at half that, with their levels and compressed. A
/// larger normal or surface that is the same at every texel (`constant`) is one block of it.
///
/// The base colour's smaller levels are averaged in the display's encoding, as retail's ground
/// pictures are, so a material far off takes the tone retail's picture takes there.
fn ground_pictures(
    side: u32,
    albedo: Vec<u8>,
    normal: Vec<u8>,
    surface: &[u8],
    constant: [bool; 2],
) -> [Image; 3] {
    let half = (side / 2).max(1);
    let block = |rgba: &[u8]| rgba[..4].repeat(16);
    let (nside, normal) = if constant[0] && side > 4 {
        (4, block(&normal))
    } else {
        (side, normal)
    };
    let (half, surface) = if constant[1] && half > 4 {
        (4, block(surface))
    } else {
        (half, resampled(side, side, surface, half))
    };
    let albedo = Image {
        srgb: true,
        ..images::with_levels(side, side, albedo, false, None)
    };
    [
        images::compressed(albedo, images::BlockFormat::Bc7),
        images::compressed(
            images::with_levels(nside, nside, normal, false, None),
            images::BlockFormat::Bc5,
        ),
        images::compressed(
            images::with_levels(half, half, surface, false, None),
            images::BlockFormat::Bc5,
        ),
    ]
}

/// `rgba` (`w` by `h`) resampled to `size` by `size`, wrapping at its edges (a ground picture
/// repeats), each texel the bilinear mix of the four nearest.
fn resampled(w: u32, h: u32, rgba: &[u8], size: u32) -> Vec<u8> {
    if (w, h) == (size, size) {
        return rgba.to_vec();
    }
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    let n = size.max(1) as usize;
    let mut out = Vec::with_capacity(n * n * 4);
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    // Texel coordinates of pictures far smaller than a float's exact range.
    for y in 0..n {
        let fy = ((y as f32 + 0.5) * h as f32 / n as f32 - 0.5).rem_euclid(h as f32);
        let (y0, ty) = (fy.floor() as usize % h, fy.fract());
        let y1 = (y0 + 1) % h;
        for x in 0..n {
            let fx = ((x as f32 + 0.5) * w as f32 / n as f32 - 0.5).rem_euclid(w as f32);
            let (x0, tx) = (fx.floor() as usize % w, fx.fract());
            let x1 = (x0 + 1) % w;
            for c in 0..4 {
                let at = |xx: usize, yy: usize| f32::from(rgba[(yy * w + xx) * 4 + c]);
                let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * tx;
                let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * tx;
                out.push((top + (bottom - top) * ty + 0.5).clamp(0.0, 255.0) as u8);
            }
        }
    }
    out
}

/// The files of one ground material, as read.
struct GroundFiles {
    base: Vec<u8>,
    normal: Option<Vec<u8>>,
    height: Option<Vec<u8>>,
    roughness: Option<Vec<u8>>,
    occlusion: Option<Vec<u8>>,
}

impl GroundFiles {
    /// The files `entry` names, read through `read`.
    fn read(
        entry: &ManifestGround,
        read: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
    ) -> Result<Self, String> {
        let base = read(&entry.base_color)?;
        let mut channel = |path: &Option<String>| path.as_deref().map(&mut *read).transpose();
        Ok(Self {
            base,
            normal: channel(&entry.normal)?,
            height: channel(&entry.height)?,
            roughness: channel(&entry.roughness)?,
            occlusion: channel(&entry.occlusion)?,
        })
    }
}

/// The ground material `entry` from its `files`, its pictures brought to `size` by `size` texels
/// (its base colour's own size when `size` is `None`).
fn ground(
    entry: &ManifestGround,
    files: &GroundFiles,
    size: Option<u32>,
) -> Result<PackGround, String> {
    let picture = |bytes: &[u8], path: &str| -> Result<(u32, u32, Vec<u8>), String> {
        images::decode_png(bytes).map_err(|e| format!("{path}: {e}"))
    };
    let (w, h, base) = picture(&files.base, &entry.base_color)?;
    if w == 0 || h == 0 {
        return Err(format!("{}: an empty picture", entry.base_color));
    }
    let side = size.unwrap_or(w.max(h));
    let base = resampled(w, h, &base, side);
    let channel =
        |bytes: &Option<Vec<u8>>, path: &Option<String>| -> Result<Option<Vec<u8>>, String> {
            let (Some(bytes), Some(path)) = (bytes, path) else {
                return Ok(None);
            };
            let (w, h, rgba) = picture(bytes, path)?;
            Ok(Some(resampled(w, h, &rgba, side)))
        };
    let normal = channel(&files.normal, &entry.normal)?;
    let height = channel(&files.height, &entry.height)?;
    let roughness = channel(&files.roughness, &entry.roughness)?;
    let occlusion = channel(&files.occlusion, &entry.occlusion)?;
    let texels = (side as usize) * (side as usize);
    let mut albedo = Vec::with_capacity(texels * 4);
    let mut bent = Vec::with_capacity(texels * 4);
    let mut surface = Vec::with_capacity(texels * 4);
    let red = |p: &Option<Vec<u8>>, i: usize, c: usize, default: u8| {
        p.as_ref().map_or(default, |p| p[i * 4 + c])
    };
    for i in 0..texels {
        albedo.extend([
            base[i * 4],
            base[i * 4 + 1],
            base[i * 4 + 2],
            red(&height, i, 0, GROUND_DEFAULTS[0]),
        ]);
        bent.extend([
            red(&normal, i, 0, GROUND_DEFAULTS[0]),
            red(&normal, i, 1, GROUND_DEFAULTS[1]),
            0,
            255,
        ]);
        surface.extend([
            red(&roughness, i, 0, GROUND_DEFAULTS[2]),
            red(&occlusion, i, 0, GROUND_DEFAULTS[3]),
            0,
            255,
        ]);
    }
    Ok(PackGround::with_pictures(
        entry,
        ground_pictures(
            side,
            albedo,
            bent,
            &surface,
            [normal.is_none(), roughness.is_none() && occlusion.is_none()],
        ),
    ))
}

/// The terrain types drawn as water, whose retail pictures keep a wet sheen when the pack has no
/// material for them.
const WATER: [u32; 6] = [16, 17, 18, 19, 20, 22];

/// The world's own ground an art pack's ground is laid with: retail's blend masks, and retail's
/// pictures for the grounds the pack has no material for, each brought to the size of the pack's
/// materials. Read from the files when a pack is set or the ground changes.
#[derive(Debug, Clone, PartialEq)]
pub struct RetailGround {
    /// A number of its own, so a ground read again is told from the one before.
    pub serial: u64,
    /// The masks, each with its id, in the order the cells' layers name them: the red keeps
    /// what lies under a layer (255) or takes the layer (0), compressed in BC4.
    pub masks: Vec<(DataId, Image)>,
    /// Retail's pictures for the grounds the pack has no material for: the grounds each is drawn
    /// for (terrain types, 32 the road), the picture, its repeats across a cell, and a surface
    /// picture of a level, unbent ground (wet for water).
    pub pictures: Vec<RetailPicture>,
    /// Each retail ground's own picture shrunk to [`BROAD_SIZE`] texels a side (terrain types 0 to
    /// 31, then the road), with its repeats across a cell: its patches of light and dark and of
    /// colour, broader than a fifth of a metre, which the pack's materials are laid out in so they
    /// keep the character of the ground they replace.
    pub broad: Vec<(Image, u32)>,
}

/// A retail picture as read: its width, height and texels, and the same with its levels.
type Decoded = (u32, u32, Vec<u8>, Image);

/// The texels a side of a ground's broad picture ([`RetailGround::broad`]).
pub const BROAD_SIZE: u32 = 64;

/// `image`'s level `size` texels wide, as a picture of its own with its smaller levels; a plain
/// grey picture where it has none.
fn broad_level(image: Option<&Image>, size: u32) -> Image {
    let level = image.and_then(|i| {
        let k = i
            .levels
            .iter()
            .position(|l| l.len() == (size as usize) * (size as usize) * 4)?;
        Some(i.levels[k].as_ref().clone())
    });
    let rgba =
        level.unwrap_or_else(|| [128, 128, 128, 255].repeat((size as usize) * (size as usize)));
    images::with_levels(size, size, rgba, true, None)
}

/// One of retail's ground pictures, laid where the pack has no material, in the pictures of a
/// pack's material ([`PackGround`]).
#[derive(Debug, Clone, PartialEq)]
pub struct RetailPicture {
    /// The retail grounds it is drawn for: terrain types, 32 the road.
    pub grounds: Vec<u8>,
    /// The picture, with a level height in alpha.
    pub albedo: Image,
    /// A flat normal.
    pub normal: Image,
    /// Its roughness and no occlusion.
    pub surface: Image,
    /// Its repeats across a cell.
    pub tiling: u32,
}

impl RetailGround {
    /// The ground `pack` is laid with over the texture-merge land surface `tm`, its pictures read
    /// through `picture` (four bytes a texel, red first).
    #[must_use]
    pub fn of(
        tm: &pyreal_assets::region::TexMerge,
        pack: &ArtPack,
        picture: &mut dyn FnMut(DataId) -> Option<(u32, u32, Vec<u8>)>,
    ) -> Self {
        use pyreal_terrain::consts::ROAD_TERRAIN_TYPE;
        use pyreal_terrain::land::merge::get_terrain_tex;
        let mut masks: Vec<(DataId, Image)> = Vec::new();
        for m in tm
            .corner_terrain_maps
            .iter()
            .chain(&tm.side_terrain_maps)
            .chain(&tm.road_maps)
        {
            if masks.iter().any(|(id, _)| *id == m.tex_gid) {
                continue;
            }
            if let Some((w, h, rgba)) = picture(m.tex_gid) {
                let alpha = rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|t| [t[3], 0, 0, 255])
                    .collect();
                masks.push((
                    m.tex_gid,
                    images::compressed(
                        images::with_levels(w, h, alpha, false, None),
                        images::BlockFormat::Bc4,
                    ),
                ));
            }
        }
        let size = pack.grounds.first().map_or(512, |g| g.albedo.width);
        // Each retail picture once, decoded with its levels.
        let mut decoded: Vec<(DataId, Option<Decoded>)> = Vec::new();
        let mut broad = Vec::with_capacity(33);
        for g in 0..=ROAD_TERRAIN_TYPE {
            let desc = get_terrain_tex(tm, g);
            let got = desc.and_then(|d| {
                if !decoded.iter().any(|(id, _)| *id == d.tex_gid) {
                    let p = picture(d.tex_gid).map(|(w, h, rgba)| {
                        let i = images::with_levels(w, h, rgba.clone(), true, None);
                        (w, h, rgba, i)
                    });
                    decoded.push((d.tex_gid, p));
                }
                decoded
                    .iter()
                    .find(|(id, _)| *id == d.tex_gid)
                    .and_then(|(_, p)| p.as_ref())
            });
            broad.push((
                broad_level(got.map(|(_, _, _, i)| i), BROAD_SIZE),
                desc.map_or(1, |d| d.tex_tiling.max(1)),
            ));
        }
        // Each picture the pack has no material for, once, with the grounds it is drawn for and
        // whether the first of them is water.
        let mut wanted: Vec<(DataId, Vec<u8>, u32, bool)> = Vec::new();
        for g in 0..=ROAD_TERRAIN_TYPE {
            let ground = if g == ROAD_TERRAIN_TYPE {
                Ground::Road
            } else {
                // LINT-OK: a terrain type, 0 to 31.
                #[allow(clippy::cast_possible_truncation)]
                Ground::Type(g as u8)
            };
            if pack.ground(ground).is_some() {
                continue;
            }
            let Some(desc) = get_terrain_tex(tm, g) else {
                continue;
            };
            // LINT-OK: a terrain type or the road, 0 to 32.
            #[allow(clippy::cast_possible_truncation)]
            let number = g as u8;
            if let Some((_, grounds, _, _)) = wanted.iter_mut().find(|(id, ..)| *id == desc.tex_gid)
            {
                grounds.push(number);
                continue;
            }
            wanted.push((
                desc.tex_gid,
                vec![number],
                desc.tex_tiling.max(1),
                WATER.contains(&g),
            ));
        }
        let pictures = images::par_map(wanted, &|(id, grounds, tiling, water)| {
            let (w, h, rgba, _) = decoded
                .iter()
                .find(|(d, _)| *d == id)
                .and_then(|(_, p)| p.as_ref())?;
            let mut albedo = resampled(*w, *h, rgba, size);
            for t in albedo.as_chunks_mut::<4>().0 {
                t[3] = 128;
            }
            let roughness = if water { 100 } else { 220 };
            let texels = (size as usize) * (size as usize);
            let road = grounds.iter().all(|g| u32::from(*g) == ROAD_TERRAIN_TYPE);
            let (flat, surface) = if road {
                road_relief(size, &mut albedo, roughness)
            } else {
                (
                    [128, 128, 0, 255].repeat(texels),
                    [roughness, 255, 0, 255].repeat(texels),
                )
            };
            let [albedo, normal, surface] =
                ground_pictures(size, albedo, flat, &surface, [false; 2]);
            Some(RetailPicture {
                grounds,
                albedo,
                normal,
                surface,
                tiling,
            })
        });
        Self {
            serial: next_serial(),
            masks,
            pictures: pictures.into_iter().flatten().collect(),
            broad,
        }
    }

    /// The index the cells' layers name mask `id` by.
    #[must_use]
    pub fn mask(&self, id: DataId) -> Option<u8> {
        self.masks
            .iter()
            .position(|(m, _)| *m == id)
            .and_then(|i| u8::try_from(i).ok())
    }
}

/// How far out a road's stones are told from its broad patches, texels of a 512 picture.
const ROAD_RELIEF_REACH: usize = 18;
/// How steeply a road's stones bend the light.
const ROAD_RELIEF_BEND: f32 = 5.0;

/// A road's relief, worked out from its own `albedo` (`size` a side), which keeps its colour:
/// its height written into the alpha, the bright stones standing above the dark joints; and the
/// normal and surface pictures it gives, the joints in shade.
fn road_relief(size: u32, albedo: &mut [u8], roughness: u8) -> (Vec<u8>, Vec<u8>) {
    let n = size as usize;
    let light: Vec<f32> = albedo
        .as_chunks::<4>()
        .0
        .iter()
        .map(|t| {
            (0.2126 * f32::from(t[0]) + 0.7152 * f32::from(t[1]) + 0.0722 * f32::from(t[2])) / 255.0
        })
        .collect();
    let reach = (ROAD_RELIEF_REACH * n / 512).max(2);
    // The stones' tops lie nearly level, a little of their grain left: it is the joints that sink.
    let height: Vec<f32> = wrapped_blur(&light, n, 2 * n / 512 + 1)
        .iter()
        .zip(&wrapped_blur(&light, n, reach))
        .map(|(fine, broad)| {
            let d = (fine - broad) * 6.0;
            let t = ((d + 0.7) / 0.6).clamp(0.0, 1.0);
            let joint = t * t * (3.0 - 2.0 * t);
            (joint * 1.7 - 1.0 + (d * 0.15).clamp(-0.15, 0.15)).clamp(-1.0, 1.0)
        })
        .collect();
    let at = |x: usize, y: usize| height[(y % n) * n + x % n];
    let mut normal = Vec::with_capacity(n * n * 4);
    let mut surface = Vec::with_capacity(n * n * 4);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    // LINT-OK: each brought to 0..255 and rounded; a picture's side, far inside f32.
    for y in 0..n {
        for x in 0..n {
            let h = at(x, y);
            // Toward the east, and north: up the picture, against its rows.
            let east = (at(x + n - 1, y) - at(x + 1, y)) * ROAD_RELIEF_BEND * (n as f32 / 512.0);
            let north = (at(x, y + 1) - at(x, y + n - 1)) * ROAD_RELIEF_BEND * (n as f32 / 512.0);
            let byte = |v: f32| ((v.clamp(-0.98, 0.98) * 0.5 + 0.5) * 255.0 + 0.5) as u8;
            normal.extend([byte(east), byte(north), 0, 255]);
            let shade = 0.62 + 0.38 * (h * 0.5 + 0.5);
            surface.extend([roughness, (shade * 255.0 + 0.5) as u8, 0, 255]);
            albedo[(y * n + x) * 4 + 3] = ((h * 0.5 + 0.5) * 255.0 + 0.5) as u8;
        }
    }
    (normal, surface)
}

/// The largest side [`retail_road_maps`] works a road's relief out at.
const ROAD_MAPS_MOST: u32 = 512;

/// A retail road picture's relief for the engine, which keeps drawing the picture itself: worked
/// out by [`road_relief`] from `rgba` (`width` by `height`, four bytes a texel, red first), its
/// normal and its surface (roughness, no occlusion, and the height in blue).
#[must_use]
pub fn retail_road_maps(width: u32, height: u32, rgba: &[u8]) -> [Image; 2] {
    let size = width
        .max(height)
        .clamp(4, ROAD_MAPS_MOST)
        .next_power_of_two();
    let mut albedo = resampled(width, height, rgba, size);
    let (normal, mut surface) = road_relief(size, &mut albedo, 220);
    for (s, a) in surface
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(albedo.as_chunks::<4>().0)
    {
        *s = [s[0], 255, a[3], 255];
    }
    [
        images::compressed(
            images::with_levels(size, size, normal, false, None),
            images::BlockFormat::Bc5,
        ),
        images::with_levels(size, size, surface, false, None),
    ]
}

/// `v`, a picture `n` a side, averaged over `reach` texels each way, its edges wrapping round.
fn wrapped_blur(v: &[f32], n: usize, reach: usize) -> Vec<f32> {
    #[allow(clippy::cast_precision_loss)] // a few texels
    let share = 1.0 / (2 * reach + 1) as f32;
    let along = |from: &[f32], step: usize, row: usize| -> Vec<f32> {
        let mut out = vec![0.0; n * n];
        for r in 0..n {
            let at = |i: usize| from[(r * row + (i % n) * step) % (n * n)];
            let mut sum: f32 = (0..=2 * reach).map(|i| at(i + n * 4 - reach)).sum();
            for i in 0..n {
                out[(r * row + i * step) % (n * n)] = sum * share;
                sum += at(i + reach + 1) - at(i + n * 4 - reach);
            }
        }
        out
    };
    along(&along(v, 1, n), n, 1)
}

/// A body a pack draws in place of a setup's parts ([`skin`]).
#[derive(Debug, Clone, PartialEq)]
pub struct PackSkin {
    /// The setup whose objects it is drawn for.
    pub setup: DataId,
    /// The palettes an object's look must lay one of; none for the setup's others.
    pub palettes: Vec<DataId>,
    /// Each slot it has a clip for, and the clip's place in its model.
    pub clips: Vec<(skin::Slot, usize)>,
    /// Clips for retail animations, by the animation's id.
    pub by_animation: Vec<(DataId, usize)>,
    /// The mesh, its skeleton and its clips.
    pub model: Arc<skin::SkinModel>,
    /// Each primitive's material, its pictures by index into [`ArtPack::images`].
    pub materials: Vec<PackMaterial>,
    /// The fingerprint of the setup record it was made for, where it names one.
    pub fingerprint: Option<Fingerprint>,
    /// Which records it is drawn on.
    pub draws_on: DrawsOn,
    /// Who made it, with what, and under which license.
    pub credit: Credit,
}

impl PackSkin {
    /// The clip it plays for `slot`: its own, or the first of the slot's fallbacks it has.
    #[must_use]
    pub fn clip_for(&self, slot: skin::Slot) -> Option<usize> {
        std::iter::once(slot)
            .chain(slot.fallbacks().iter().copied())
            .find_map(|s| self.clips.iter().find(|c| c.0 == s).map(|c| c.1))
    }
}

/// Queues the picture at `path` (`srgb`, cut-out below `cutoff`, a level of `views`) once,
/// reading its file now for a folder; its place in the pictures.
fn queue_picture(
    reader: &mut Reader<'_>,
    pending: &mut Vec<PendingPicture>,
    by_path: &mut HashMap<(String, bool, bool), u32>,
    pictures: Pictures<'_>,
    path: Option<&str>,
    (srgb, cutoff, views): (bool, Option<f32>, bool),
) -> Result<Option<u32>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    if let Some(i) = by_path.get(&(path.to_owned(), srgb, views)) {
        return Ok(Some(*i));
    }
    let bytes = match pictures {
        Pictures::Made => Some(reader.for_picture(path)?),
        Pictures::Packed(_) => None,
    };
    let i = u32::try_from(pending.len()).map_err(|_| "too many pictures".to_owned())?;
    pending.push(PendingPicture {
        path: path.to_owned(),
        bytes,
        srgb,
        cutoff,
        views,
    });
    by_path.insert((path.to_owned(), srgb, views), i);
    Ok(Some(i))
}

/// An open art pack: its models and their pictures, decoded.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ArtPack {
    /// The pack's name.
    pub name: String,
    /// A number of its own, so a pack opened again is told from the one before.
    pub serial: u64,
    /// The hash of its manifest and every file it read, so a report names exactly this pack.
    pub hash: String,
    /// The models that read.
    pub models: Vec<PackModel>,
    /// The pictures the materials name.
    pub images: Vec<Image>,
    /// The ground materials that read, all their pictures one size.
    pub grounds: Vec<PackGround>,
    /// The materials for retail pictures that read.
    pub materials: Vec<TextureMaterial>,
    /// The grit pictures the materials share.
    pub grits: Vec<Image>,
    /// Each grit picture's path in the pack (`grit/rock.png`): what the material is made of.
    pub grit_names: Vec<String>,
    /// The kinds of plant it lays over the land, whose models read.
    pub ground_cover: Vec<PackGroundCover>,
    /// The lights it gives retail objects.
    pub lights: Vec<pyreal_assets::art_pack::ManifestLight>,
    /// The retail objects the wind moves, drawn with their own parts.
    pub sways: Vec<pyreal_assets::art_pack::ManifestSway>,
    /// The retail moons it draws again as balls.
    pub moons: Vec<PackMoon>,
    /// The bodies it draws in place of setups' parts, in the manifest's order.
    pub skins: Vec<PackSkin>,
    /// The weenies (class ids) whose objects keep their own parts.
    pub keep: Vec<u32>,
    /// Whether an object that has held a launcher keeps its own parts.
    pub keep_launchers: bool,
    /// What would not read, a line each.
    pub problems: Vec<String>,
    /// Where a packed pack's material pictures are read again as they are drawn; none where the
    /// materials hold theirs.
    pub material_source: Option<MaterialSource>,
}

/// One of a material's pictures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialPart {
    Albedo,
    Normal,
    Surface,
    /// Where it is a metal ([`TextureMaterial::metal`]).
    Metal,
    /// Where it is clear ([`TextureMaterial::clear`]).
    Clear,
}

/// How many materials' pictures [`MaterialSource`] keeps after reading them.
const RECENT_MATERIALS: usize = 8;

/// The packed files a pack's materials' pictures are read from as they are drawn: each
/// material's three (or two) are one record.
#[derive(Clone)]
pub struct MaterialSource(Arc<MaterialFile>);

/// A material's packed file and record key.
type MaterialRecord = (Arc<packed::PackedFile>, String);

struct MaterialFile {
    /// Each material's file and record key, in [`ArtPack::materials`]' order; none for one that
    /// holds its pictures.
    keys: Vec<Option<MaterialRecord>>,
    recent: Mutex<Vec<(usize, Arc<Vec<Image>>)>>,
}

impl MaterialSource {
    fn new(keys: Vec<Option<MaterialRecord>>) -> Option<Self> {
        keys.iter().any(Option::is_some).then(|| {
            Self(Arc::new(MaterialFile {
                keys,
                recent: Mutex::new(Vec::new()),
            }))
        })
    }

    /// Material `i`'s file and record key, if it is read as drawn.
    fn record(&self, i: usize) -> Option<MaterialRecord> {
        self.0.keys.get(i)?.clone()
    }
}

impl std::fmt::Debug for MaterialSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut names: Vec<&str> = Vec::new();
        for (file, _) in self.0.keys.iter().flatten() {
            if !names.contains(&file.name()) {
                names.push(file.name());
            }
        }
        f.debug_tuple("MaterialSource").field(&names).finish()
    }
}

impl PartialEq for MaterialSource {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl MaterialSource {
    /// Material `i`'s pictures as packed: base colour (none where it keeps the retail picture,
    /// `two`), normal, surface.
    fn pictures(&self, i: usize, two: bool) -> Option<Arc<Vec<Image>>> {
        let lock = || self.0.recent.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((_, p)) = lock().iter().find(|(at, _)| *at == i) {
            return Some(Arc::clone(p));
        }
        let (file, key) = self.0.keys.get(i)?.as_ref()?;
        let read = if two {
            file.two(key).map(Vec::from)
        } else {
            file.three(key).map(Vec::from)
        };
        let pictures = Arc::new(read.ok()?);
        let mut recent = lock();
        if recent.len() >= RECENT_MATERIALS {
            recent.remove(0);
        }
        recent.push((i, Arc::clone(&pictures)));
        Some(pictures)
    }
}

/// A retail moon a pack draws again as a ball, its pictures laid longitude by latitude.
#[derive(Debug, Clone, PartialEq)]
pub struct PackMoon {
    /// The retail moon's graphics object.
    pub object: DataId,
    /// Its colour, and its height in alpha.
    pub surface: Image,
    /// What burns on its night side.
    pub lights: Option<Image>,
    /// A layer of cloud over the ground, its cover in alpha.
    pub clouds: Option<Image>,
}

/// One kind of plant a pack lays over the land.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PackGroundCover {
    /// The number its model is known by among the pack's models.
    pub id: DataId,
    /// The retail terrain types it grows on, a bit each.
    pub grounds: u32,
    /// How many plants a square metre.
    pub density: f32,
    /// The smallest and the largest it is scaled by.
    pub scale: (f32, f32),
    /// The steepest slope it grows on, degrees.
    pub steepest: f32,
}

/// Why a pack could not be opened at all, as a sentence.
pub type OpenError = String;

/// A picture of a pack's models, to be made from its file or read as packed.
struct PendingPicture {
    path: String,
    /// The file's bytes, read to make the picture from; none where it is read as packed.
    bytes: Option<Vec<u8>>,
    srgb: bool,
    cutoff: Option<f32>,
    /// A picture of a level of pictures of its model seen from round it.
    views: bool,
}

impl PendingPicture {
    /// What a packed pack keeps the picture under.
    fn key(&self) -> String {
        packed::model_key(&self.path, self.srgb, self.cutoff, self.views)
    }

    /// The picture made from its file's `bytes`: decoded, stretched to whole blocks, given its
    /// levels and compressed.
    fn make(&self, bytes: &[u8]) -> Result<Image, String> {
        let (w, h, rgba) = images::decode_png(bytes).map_err(|e| format!("{}: {e}", self.path))?;
        let (w, h, rgba) = images::to_whole_blocks(w, h, rgba);
        let levels = if self.views {
            images::with_view_levels(w, h, rgba, self.srgb)
        } else {
            images::with_levels(w, h, rgba, self.srgb, self.cutoff)
        };
        Ok(images::compressed(levels, images::BlockFormat::Bc7))
    }
}

/// Where an opening pack's pictures come from.
#[derive(Clone, Copy)]
enum Pictures<'a> {
    /// Made from the files they are made from, as those are read: a folder.
    Made,
    /// Read from a packed file, as they were made when it was packed.
    Packed(&'a packed::PackedFile),
}

/// What opening a pack read and made, kept while it is packed into one file.
#[derive(Default)]
struct Recording {
    /// Every file read for itself (the manifest, the models and their buffers), by path.
    files: BTreeMap<String, Vec<u8>>,
    /// Every picture made, by what a packed pack keeps it under.
    pictures: BTreeMap<String, Vec<Image>>,
}

/// What an opening pack reads its files through: each hashed as it is read, and the files read
/// for themselves kept while the pack is packed.
struct Reader<'r> {
    read: &'r mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
    /// None for a packed pack, which is named by the hash it was packed with.
    hash: Option<ContentHash>,
    kept: Option<BTreeMap<String, Vec<u8>>>,
}

impl Reader<'_> {
    /// The file at `path`, read for itself: a model or one of its buffers.
    fn file(&mut self, path: &str) -> Result<Vec<u8>, String> {
        let bytes = self.for_picture(path)?;
        if let Some(kept) = self.kept.as_mut() {
            kept.insert(path.to_owned(), bytes.clone());
        }
        Ok(bytes)
    }

    /// The file at `path`, read to make a picture from.
    fn for_picture(&mut self, path: &str) -> Result<Vec<u8>, String> {
        let bytes = (self.read)(path)?;
        if let Some(hash) = self.hash.as_mut() {
            hash.part(path, &bytes);
        }
        Ok(bytes)
    }
}

/// A short hash of what a pack read: the first eight bytes of its BLAKE3 hash in hex, each file
/// framed by the lengths of its path and its bytes (the client's report hash, so a report names
/// the same pack by the same hash).
#[derive(Debug, Clone, Default)]
struct ContentHash(blake3::Hasher);

impl ContentHash {
    fn new() -> Self {
        Self::default()
    }

    fn part(&mut self, name: &str, bytes: &[u8]) {
        let len = |n: usize| u64::try_from(n).unwrap_or(u64::MAX).to_le_bytes();
        self.0.update(&len(name.len()));
        self.0.update(name.as_bytes());
        self.0.update(&len(bytes.len()));
        self.0.update(bytes);
    }

    fn finish(&self) -> String {
        self.0.finalize().as_bytes()[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

/// The next pack's serial.
fn next_serial() -> u64 {
    static SERIAL: AtomicU64 = AtomicU64::new(1);
    SERIAL.fetch_add(1, Ordering::Relaxed)
}

/// A sphere holding every vertex of `lods`.
fn bounds(lods: &[PackLod]) -> ([f32; 3], f32) {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    let positions = || {
        lods.iter()
            .flat_map(|l| &l.primitives)
            .flat_map(|p| p.vertices.as_chunks::<VERTEX_FLOATS>().0.iter())
    };
    for v in positions() {
        for c in 0..3 {
            lo[c] = lo[c].min(v[c]);
            hi[c] = hi[c].max(v[c]);
        }
    }
    if lo[0] > hi[0] {
        return ([0.0; 3], 0.0);
    }
    let centre = [0, 1, 2].map(|c| (lo[c] + hi[c]) * 0.5);
    let radius = positions()
        .map(|v| {
            let d = [0, 1, 2].map(|c| v[c] - centre[c]);
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        })
        .fold(0.0f32, f32::max);
    (centre, radius)
}

impl ArtPack {
    /// The pack at `path`: the folder of one ([`Self::open_dir`]), or the file it was packed into
    /// ([`Self::open_packed`]).
    ///
    /// # Errors
    /// As theirs.
    pub fn open_path(path: &Path) -> Result<Self, OpenError> {
        if path.is_dir() {
            Self::open_dir(path)
        } else {
            Self::open_packed(path)
        }
    }

    /// The pack packed into the file at `path` ([`packed`]).
    ///
    /// # Errors
    /// The file is not a packed pack, was packed for another layout or manifest format, or is
    /// damaged: a record that will not read, or is not the bytes it was packed with. A damaged
    /// pack is refused whole.
    pub fn open_packed(path: &Path) -> Result<Self, OpenError> {
        let file = Arc::new(packed::PackedFile::open(path)?);
        Self::from_packed_file(&file, Some(&file))
    }

    /// The pack in the packed file `file`, opened as [`Self::open`] opens a folder's, its
    /// pictures read as they were made when it was packed.
    ///
    /// # Errors
    /// As [`Self::open_packed`].
    pub fn from_packed(file: &packed::PackedFile) -> Result<Self, OpenError> {
        Self::from_packed_file(file, None)
    }

    /// [`Self::from_packed`], its materials' pictures let go once read and read again from
    /// `lazy` as they are drawn, when given.
    fn from_packed_file(
        file: &packed::PackedFile,
        lazy: Option<&Arc<packed::PackedFile>>,
    ) -> Result<Self, OpenError> {
        let manifest = String::from_utf8(file.file(MANIFEST_FILE)?)
            .map_err(|_| format!("{}: its manifest is not text", file.name()))?;
        let pack = Self::open_from(
            &manifest,
            &mut |path| file.file(path),
            Pictures::Packed(file),
            None,
            lazy,
        );
        match file.damage() {
            Some(e) => Err(e),
            None => pack,
        }
    }

    /// The pack in the folder `dir`.
    ///
    /// # Errors
    /// The manifest will not read, or names no model that reads.
    pub fn open_dir(dir: &Path) -> Result<Self, OpenError> {
        let manifest = std::fs::read_to_string(dir.join(MANIFEST_FILE))
            .map_err(|e| format!("{}: {e}", dir.join(MANIFEST_FILE).display()))?;
        Self::open(&manifest, &mut |path| {
            std::fs::read(dir.join(path)).map_err(|e| format!("{path}: {e}"))
        })
    }

    /// The pack whose manifest is `manifest`, every file it names read through `read` by its
    /// path relative to the pack.
    ///
    /// # Errors
    /// The manifest will not read, or names no model that reads.
    pub fn open(
        manifest: &str,
        read_file: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
    ) -> Result<Self, OpenError> {
        Self::open_from(manifest, read_file, Pictures::Made, None, None)
    }

    /// The pack whose manifest is `manifest_text`, its files read through `read_file` and its
    /// pictures made or read as `pictures` says; what it read and made kept in `recording`; its
    /// materials' pictures let go once read, to be read again from `lazy`, when given.
    fn open_from(
        manifest_text: &str,
        read_file: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
        pictures: Pictures<'_>,
        mut recording: Option<&mut Recording>,
        lazy: Option<&Arc<packed::PackedFile>>,
    ) -> Result<Self, OpenError> {
        // Every file read is hashed as it is read, after the manifest.
        let mut reader = Reader {
            read: read_file,
            hash: matches!(pictures, Pictures::Made).then(ContentHash::new),
            kept: recording.is_some().then(BTreeMap::new),
        };
        if let Some(hash) = reader.hash.as_mut() {
            hash.part(MANIFEST_FILE, manifest_text.as_bytes());
        }
        if let Some(kept) = reader.kept.as_mut() {
            kept.insert(MANIFEST_FILE.to_owned(), manifest_text.as_bytes().to_vec());
        }
        let manifest = ArtPackManifest::parse(manifest_text).map_err(|e| e.to_string())?;
        let mut problems = Vec::new();
        let mut models = Vec::new();
        // Each picture once, by path and whether it holds colour, read here and made below.
        let mut pending: Vec<PendingPicture> = Vec::new();
        let mut by_path: HashMap<(String, bool, bool), u32> = HashMap::new();
        for entry in &manifest.models {
            let label = gltf::label(entry.id);
            let mut lods = Vec::with_capacity(entry.lods.len());
            let mut failed = None;
            for lod in &entry.lods {
                let read_model = reader
                    .file(&lod.file)
                    .and_then(|json| gltf::decode(&json, &lod.file, &mut |p| reader.file(p)));
                match read_model {
                    Ok(primitives) => lods.push((lod, primitives)),
                    Err(e) => {
                        failed = Some(e);
                        break;
                    }
                }
            }
            if let Some(e) = failed {
                problems.push(format!("{label}: {e}"));
                continue;
            }
            let mut picture = |path: &Option<String>,
                               srgb: bool,
                               cutoff: Option<f32>,
                               views: bool|
             -> Result<Option<u32>, String> {
                queue_picture(
                    &mut reader,
                    &mut pending,
                    &mut by_path,
                    pictures,
                    path.as_deref(),
                    (srgb, cutoff, views),
                )
            };
            let mut built = Vec::with_capacity(lods.len());
            let mut broken = None;
            for (lod, primitives) in lods {
                let mut parts = Vec::with_capacity(primitives.len());
                let views = lod.views.is_some();
                for p in primitives {
                    let m = &p.material;
                    let material = (|| -> Result<PackMaterial, String> {
                        Ok(PackMaterial {
                            base_color: m.base_color,
                            base_color_image: picture(
                                &m.base_color_image,
                                true,
                                m.alpha_cutoff,
                                views,
                            )?,
                            normal_image: picture(&m.normal_image, false, None, views)?,
                            normal_scale: m.normal_scale,
                            metal_rough_image: picture(&m.metal_rough_image, false, None, views)?,
                            metallic: m.metallic,
                            roughness: m.roughness,
                            occlusion_image: picture(&m.occlusion_image, false, None, views)?,
                            occlusion_strength: m.occlusion_strength,
                            alpha_cutoff: m.alpha_cutoff,
                            double_sided: m.double_sided,
                        })
                    })();
                    match material {
                        Ok(material) => parts.push(PackPrimitive {
                            vertices: Arc::new(p.vertices),
                            indices: Arc::new(p.indices),
                            material,
                        }),
                        Err(e) => {
                            broken = Some(e);
                            break;
                        }
                    }
                }
                if broken.is_some() {
                    break;
                }
                built.push(PackLod {
                    distance: lod.distance.unwrap_or(f32::INFINITY),
                    primitives: parts,
                    views: lod.views,
                    shadows_only: lod.shadows_only,
                });
            }
            if let Some(e) = broken {
                problems.push(format!("{label}: {e}"));
                continue;
            }
            models.push(PackModel {
                id: entry.id,
                foliage: entry.foliage,
                wind: entry.wind,
                bounds: bounds(&built),
                lods: built,
                fingerprint: entry.fingerprint,
                draws_on: entry.draws_on,
                credit: entry.credit.clone(),
            });
        }
        // The bodies, their pictures queued with the models'.
        let mut skins: Vec<PackSkin> = Vec::with_capacity(manifest.skins.len());
        for entry in &manifest.skins {
            let label = format!("body {:#010X}", entry.setup.0);
            let made = reader
                .file(&entry.file)
                .and_then(|json| gltf::decode_skin(&json, &entry.file, &mut |p| reader.file(p)));
            let model = match made {
                Ok(m) => m,
                Err(e) => {
                    problems.push(format!("{label}: {e}"));
                    continue;
                }
            };
            let mut mats = Vec::with_capacity(model.primitives.len());
            let mut broken = None;
            for p in &model.primitives {
                let m = &p.material;
                let mut q = |path: &Option<String>, srgb: bool, cutoff: Option<f32>| {
                    queue_picture(
                        &mut reader,
                        &mut pending,
                        &mut by_path,
                        pictures,
                        path.as_deref(),
                        (srgb, cutoff, false),
                    )
                };
                let made = (|| -> Result<PackMaterial, String> {
                    Ok(PackMaterial {
                        base_color: m.base_color,
                        base_color_image: q(&m.base_color_image, true, m.alpha_cutoff)?,
                        normal_image: q(&m.normal_image, false, None)?,
                        normal_scale: m.normal_scale,
                        metal_rough_image: q(&m.metal_rough_image, false, None)?,
                        metallic: m.metallic,
                        roughness: m.roughness,
                        occlusion_image: q(&m.occlusion_image, false, None)?,
                        occlusion_strength: m.occlusion_strength,
                        alpha_cutoff: m.alpha_cutoff,
                        double_sided: m.double_sided,
                    })
                })();
                match made {
                    Ok(m) => mats.push(m),
                    Err(e) => {
                        broken = Some(e);
                        break;
                    }
                }
            }
            if let Some(e) = broken {
                problems.push(format!("{label}: {e}"));
                continue;
            }
            let mut clips = Vec::new();
            for (slot, name) in &entry.clips {
                match (skin::Slot::named(slot), model.clip(name)) {
                    (Some(s), Some(c)) => clips.push((s, c)),
                    _ => problems.push(format!("{label}: no animation {name:?} for {slot}")),
                }
            }
            let by_animation = entry
                .by_animation
                .iter()
                .filter_map(|(id, name)| Some((*id, model.clip(name)?)))
                .collect();
            skins.push(PackSkin {
                setup: entry.setup,
                palettes: entry.palettes.clone(),
                clips,
                by_animation,
                model: Arc::new(model),
                materials: mats,
                fingerprint: entry.fingerprint,
                draws_on: entry.draws_on,
                credit: entry.credit.clone(),
            });
        }
        // The pictures made (decoded, given their levels and compressed) or read as packed, on
        // every thread at once; a model whose picture will not make is left out.
        let keys: Vec<String> = match recording {
            Some(_) => pending.iter().map(PendingPicture::key).collect(),
            None => Vec::new(),
        };
        let decoded = images::par_map(pending, &|p| match (pictures, &p.bytes) {
            (Pictures::Packed(file), _) => file.picture(&p.key()),
            (Pictures::Made, Some(bytes)) => p.make(bytes),
            (Pictures::Made, None) => Err(format!("{}: not read", p.path)),
        });
        if let Some(r) = recording.as_deref_mut() {
            for (key, made) in keys.into_iter().zip(&decoded) {
                if let Ok(image) = made {
                    r.pictures.insert(key, vec![image.clone()]);
                }
            }
        }
        let failed: HashMap<u32, String> = (0u32..)
            .zip(&decoded)
            .filter_map(|(i, d)| d.as_ref().err().map(|e| (i, e.clone())))
            .collect();
        models.retain(|m: &PackModel| {
            let Some(e) = m
                .lods
                .iter()
                .flat_map(|l| &l.primitives)
                .flat_map(|p| {
                    let m = &p.material;
                    [
                        m.base_color_image,
                        m.normal_image,
                        m.metal_rough_image,
                        m.occlusion_image,
                    ]
                })
                .flatten()
                .find_map(|i| failed.get(&i))
            else {
                return true;
            };
            problems.push(format!("{}: {e}", gltf::label(m.id)));
            false
        });
        skins.retain(|k| {
            let Some(e) = k
                .materials
                .iter()
                .flat_map(|m| {
                    [
                        m.base_color_image,
                        m.normal_image,
                        m.metal_rough_image,
                        m.occlusion_image,
                    ]
                })
                .flatten()
                .find_map(|i| failed.get(&i))
            else {
                return true;
            };
            problems.push(format!("body {:#010X}: {e}", k.setup.0));
            false
        });
        let mut images: Vec<Image> = decoded
            .into_iter()
            .map(|d| d.unwrap_or_else(|_| images::with_levels(1, 1, vec![0; 4], false, None)))
            .collect();
        // Cards for the models with none of their own: read as packed, or worked out here.
        let skip = manifest.ground_cover.iter().map(|c| c.id).collect();
        let packed_card = |id| match pictures {
            Pictures::Packed(file) => file.two(&packed::card_key(id)).ok(),
            Pictures::Made => None,
        };
        let made = cards::add_cards(&mut models, &mut images, &skip, &packed_card);
        if let Some(r) = recording.as_deref_mut() {
            for (id, made) in made {
                r.pictures.insert(packed::card_key(id), made.to_vec());
            }
        }
        if models.is_empty() && !manifest.models.is_empty() {
            return Err(format!(
                "no model of the pack would read: {}",
                problems.join("; ")
            ));
        }
        // Every ground at the first one's size, so they are drawn from one array: the files read
        // here, the pictures made on every thread at once (or read as packed).
        let made: Vec<(&ManifestGround, Result<PackGround, String>)> = match pictures {
            Pictures::Made => {
                let files: Vec<(&ManifestGround, Result<GroundFiles, String>)> = manifest
                    .grounds
                    .iter()
                    .map(|entry| {
                        let files = GroundFiles::read(entry, &mut |p| reader.for_picture(p));
                        (entry, files)
                    })
                    .collect();
                let side = files.iter().find_map(|(_, f)| {
                    let (w, h) = images::png_size(&f.as_ref().ok()?.base).ok()?;
                    Some(w.max(h))
                });
                images::par_map(files, &|(entry, f)| {
                    (entry, f.and_then(|f| ground(entry, &f, side)))
                })
            }
            Pictures::Packed(file) => {
                images::par_map(manifest.grounds.iter().collect(), &|entry| {
                    let made = file.three(&packed::ground_key(entry));
                    (entry, made.map(|p| PackGround::with_pictures(entry, p)))
                })
            }
        };
        if let Some(r) = recording.as_deref_mut() {
            for (entry, g) in &made {
                if let Ok(g) = g {
                    let made = vec![g.albedo.clone(), g.normal.clone(), g.surface.clone()];
                    r.pictures.insert(packed::ground_key(entry), made);
                }
            }
        }
        let mut grounds: Vec<PackGround> = Vec::with_capacity(made.len());
        // The detail grains, one each however many grounds share it.
        let mut details: BTreeMap<&str, Option<Image>> = BTreeMap::new();
        for (entry, g) in made {
            match g {
                Ok(mut g) => {
                    if let Some(path) = entry.detail.as_deref() {
                        g.detail = details
                            .entry(path)
                            .or_insert_with(|| {
                                let made = match pictures {
                                    Pictures::Made => reader
                                        .for_picture(path)
                                        .and_then(|bytes| materials::detail(&bytes, path)),
                                    Pictures::Packed(file) => {
                                        file.picture(&packed::detail_key(path))
                                    }
                                };
                                match made {
                                    Ok(image) => {
                                        if let Some(r) = recording.as_deref_mut() {
                                            r.pictures.insert(
                                                packed::detail_key(path),
                                                vec![image.clone()],
                                            );
                                        }
                                        Some(image)
                                    }
                                    Err(e) => {
                                        problems.push(format!("detail {path}: {e}"));
                                        None
                                    }
                                }
                            })
                            .clone();
                    }
                    grounds.push(g);
                }
                Err(e) => problems.push(format!("ground {:?}: {e}", entry.grounds)),
            }
        }
        // The materials for retail pictures: the files read here, the pictures made on every
        // thread at once (or read as packed).
        let made: Vec<(_, Result<TextureMaterial, String>)> = match pictures {
            Pictures::Made => {
                let files: Vec<_> = manifest
                    .materials
                    .iter()
                    .map(|entry| {
                        let files =
                            materials::MaterialFiles::read(entry, &mut |p| reader.for_picture(p));
                        (entry, files)
                    })
                    .collect();
                images::par_map(files, &|(entry, f)| {
                    (entry, f.and_then(|f| materials::material(entry, &f)))
                })
            }
            Pictures::Packed(file) => {
                images::par_map(manifest.materials.iter().collect(), &|entry| {
                    let key = packed::material_key(entry);
                    let made = if entry.keep_retail_picture {
                        file.two(&key)
                            .map(|[n, s]| materials::with_pictures(entry, None, n, s))
                    } else {
                        file.three(&key)
                            .map(|[a, n, s]| materials::with_pictures(entry, Some(a), n, s))
                    };
                    // Read again as drawn: only the pictures' sizes are kept.
                    let made = made.map(|mut m| {
                        if lazy.is_some() {
                            for image in m.albedo.iter_mut().chain([&mut m.normal, &mut m.surface])
                            {
                                image.levels = Vec::new();
                            }
                        }
                        m
                    });
                    (entry, made)
                })
            }
        };
        if let Some(r) = recording.as_deref_mut() {
            for (entry, m) in &made {
                if let Ok(m) = m {
                    let mut made: Vec<Image> = m.albedo.iter().cloned().collect();
                    made.extend([m.normal.clone(), m.surface.clone()]);
                    r.pictures.insert(packed::material_key(entry), made);
                }
            }
        }
        let mut retextured: Vec<TextureMaterial> = Vec::with_capacity(made.len());
        let mut named: Vec<&pyreal_assets::art_pack::ManifestMaterial> =
            Vec::with_capacity(made.len());
        for (entry, m) in made {
            match m {
                Ok(m) => {
                    retextured.push(m);
                    named.push(entry);
                }
                Err(e) => problems.push(format!(
                    "material {}: {e}",
                    entry
                        .textures
                        .iter()
                        .chain(entry.solid.iter().flat_map(|sd| sd.surfaces.iter()))
                        .map(|t| format!("{:#010X}", t.0))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            }
        }
        let material_source = lazy.and_then(|file| {
            MaterialSource::new(
                named
                    .iter()
                    .map(|e| Some((Arc::clone(file), packed::material_key(e))))
                    .collect(),
            )
        });
        // The metal maps, each held by its material.
        for (m, entry) in retextured.iter_mut().zip(&named) {
            let Some(path) = entry.metal_map.as_deref() else {
                continue;
            };
            let made = match pictures {
                Pictures::Made => reader
                    .for_picture(path)
                    .and_then(|bytes| materials::metal(&bytes, path)),
                Pictures::Packed(file) => file.picture(&packed::metal_key(path)),
            };
            match made {
                Ok(image) => {
                    if let Some(r) = recording.as_deref_mut() {
                        r.pictures
                            .insert(packed::metal_key(path), vec![image.clone()]);
                    }
                    m.metal = Some(image);
                }
                Err(e) => problems.push(format!("metal map {path}: {e}")),
            }
        }
        // The clear maps, each held by its material.
        for (m, entry) in retextured.iter_mut().zip(&named) {
            let Some(path) = entry.clear_map.as_deref() else {
                continue;
            };
            let made = match pictures {
                Pictures::Made => reader
                    .for_picture(path)
                    .and_then(|bytes| materials::clear(&bytes, path)),
                Pictures::Packed(file) => file.picture(&packed::clear_key(path)),
            };
            match made {
                Ok(image) => {
                    if let Some(r) = recording.as_deref_mut() {
                        r.pictures
                            .insert(packed::clear_key(path), vec![image.clone()]);
                    }
                    m.clear = Some(image);
                }
                Err(e) => problems.push(format!("clear map {path}: {e}")),
            }
        }
        // The grit pictures, one each however many materials share it.
        let mut grits: Vec<Image> = Vec::new();
        let mut grit_names: Vec<String> = Vec::new();
        let mut grit_at: BTreeMap<&str, Option<usize>> = BTreeMap::new();
        for (m, entry) in retextured.iter_mut().zip(named) {
            let Some(path) = entry.grit.as_deref() else {
                continue;
            };
            let at = *grit_at.entry(path).or_insert_with(|| {
                let made = match pictures {
                    Pictures::Made => reader
                        .for_picture(path)
                        .and_then(|bytes| materials::grit(&bytes, path)),
                    Pictures::Packed(file) => file.picture(&packed::grit_key(path)),
                };
                match made {
                    Ok(image) => {
                        if let Some(r) = recording.as_deref_mut() {
                            r.pictures
                                .insert(packed::grit_key(path), vec![image.clone()]);
                        }
                        grits.push(image);
                        grit_names.push(path.to_owned());
                        Some(grits.len() - 1)
                    }
                    Err(e) => {
                        problems.push(format!("grit {path}: {e}"));
                        None
                    }
                }
            });
            m.grit = at;
        }
        // The moons' pictures, made as a model's colour is (or read as packed).
        let mut moons = Vec::with_capacity(manifest.moons.len());
        for entry in &manifest.moons {
            let mut made = |path: &str| -> Result<Image, String> {
                let image = match pictures {
                    Pictures::Made => reader.for_picture(path).and_then(|bytes| {
                        PendingPicture {
                            path: path.to_owned(),
                            bytes: None,
                            srgb: true,
                            cutoff: None,
                            views: false,
                        }
                        .make(&bytes)
                    }),
                    Pictures::Packed(file) => file.picture(&packed::moon_key(path)),
                }?;
                if let Some(r) = recording.as_deref_mut() {
                    r.pictures
                        .insert(packed::moon_key(path), vec![image.clone()]);
                }
                Ok(image)
            };
            let moon = made(&entry.surface).and_then(|surface| {
                let lights = entry.lights.as_deref().map(&mut made).transpose()?;
                let clouds = entry.clouds.as_deref().map(&mut made).transpose()?;
                Ok(PackMoon {
                    object: entry.object,
                    surface,
                    lights,
                    clouds,
                })
            });
            match moon {
                Ok(m) => moons.push(m),
                Err(e) => problems.push(format!("moon {:#010X}: {e}", entry.object.0)),
            }
        }
        // A packed pack is named by the hash of the folder it was packed from.
        let hash = match (pictures, &reader.hash) {
            (Pictures::Packed(file), _) => file.hash().to_owned(),
            (Pictures::Made, hash) => hash.as_ref().map(ContentHash::finish).unwrap_or_default(),
        };
        if let Some(r) = recording {
            r.files = reader.kept.take().unwrap_or_default();
        }
        let ground_cover = manifest
            .ground_cover
            .iter()
            .filter(|c| models.iter().any(|m| m.id == c.id))
            .map(|c| PackGroundCover {
                id: c.id,
                grounds: c.grounds.iter().fold(0, |bits, g| bits | 1 << g),
                density: c.density,
                scale: c.scale,
                steepest: c.steepest,
            })
            .collect();
        Ok(Self {
            name: manifest.name,
            serial: next_serial(),
            hash,
            models,
            images,
            grounds,
            materials: retextured,
            grits,
            grit_names,
            ground_cover,
            lights: manifest.lights,
            sways: manifest.sways,
            moons,
            skins,
            keep: manifest.keep,
            keep_launchers: manifest.keep_launchers,
            problems,
            material_source,
        })
    }

    /// A pack with nothing in it, which replaces nothing: what the Reflective Water box draws
    /// under when no pack is set, the world's ground laid with retail's own pictures alone.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            name: "Reflective Water".to_owned(),
            serial: next_serial(),
            ..Self::default()
        }
    }

    /// This pack drawn over `base`: its models, materials, grounds, kinds of cover, lights and
    /// moons replace the base's for what they stand for, and what it lacks draws from the base.
    #[must_use]
    pub fn over(&self, base: &Self) -> Self {
        let grit_at = self.grits.len();

        // The base's models the top lacks, and only the pictures they draw with, after the top's.
        let mut models = self.models.clone();
        let mut images = self.images.clone();
        let mut moved: HashMap<u32, u32> = HashMap::new();
        // A top entry with no fingerprint is for the record the base's first entry of its id was
        // made for, so the base's entries for other eras' records still draw.
        for m in &mut models {
            if m.fingerprint.is_none() {
                m.fingerprint = base
                    .models
                    .iter()
                    .find(|b| b.id == m.id)
                    .and_then(|b| b.fingerprint);
            }
        }
        let replaced: HashSet<(DataId, Option<Fingerprint>)> =
            models.iter().map(|m| (m.id, m.fingerprint)).collect();
        for m in base
            .models
            .iter()
            .filter(|m| !replaced.contains(&(m.id, m.fingerprint)))
        {
            let mut m = m.clone();
            for p in m.lods.iter_mut().flat_map(|l| &mut l.primitives) {
                let mat = &mut p.material;
                for i in [
                    &mut mat.base_color_image,
                    &mut mat.normal_image,
                    &mut mat.metal_rough_image,
                    &mut mat.occlusion_image,
                ] {
                    *i = i.and_then(|at| {
                        let picture = base.images.get(at as usize)?;
                        Some(*moved.entry(at).or_insert_with(|| {
                            images.push(picture.clone());
                            u32::try_from(images.len() - 1).unwrap_or(u32::MAX)
                        }))
                    });
                }
            }
            models.push(m);
        }

        // A setup the top has a body for, for the same record, draws only the top's bodies.
        let mut skins = self.skins.clone();
        let bodied: HashSet<(DataId, Option<Fingerprint>)> = self
            .skins
            .iter()
            .map(|k| (k.setup, k.fingerprint))
            .collect();
        for k in base
            .skins
            .iter()
            .filter(|k| !bodied.contains(&(k.setup, k.fingerprint)))
        {
            let mut k = k.clone();
            for mat in &mut k.materials {
                for i in [
                    &mut mat.base_color_image,
                    &mut mat.normal_image,
                    &mut mat.metal_rough_image,
                    &mut mat.occlusion_image,
                ] {
                    *i = i.and_then(|at| {
                        let picture = base.images.get(at as usize)?;
                        Some(*moved.entry(at).or_insert_with(|| {
                            images.push(picture.clone());
                            u32::try_from(images.len() - 1).unwrap_or(u32::MAX)
                        }))
                    });
                }
            }
            skins.push(k);
        }
        let mut keep = self.keep.clone();
        keep.extend(base.keep.iter().filter(|w| !self.keep.contains(w)));

        let mut materials = self.materials.clone();
        let mut records: Vec<Option<MaterialRecord>> = (0..self.materials.len())
            .map(|i| self.material_source.as_ref().and_then(|s| s.record(i)))
            .collect();
        // As for the models: a top entry with no fingerprint takes the base's first one's.
        for m in &mut materials {
            let prints = (0..m.textures.len())
                .map(|k| {
                    m.fingerprint(k).or_else(|| {
                        let t = m.textures[k];
                        base.materials.iter().find_map(|b| {
                            let at = b.textures.iter().position(|&u| u == t)?;
                            Some(b.fingerprint(at))
                        })?
                    })
                })
                .collect();
            m.fingerprints = prints;
        }
        let covered: HashSet<(DataId, Option<Fingerprint>)> = materials
            .iter()
            .flat_map(|m| (0..m.textures.len()).map(|k| (m.textures[k], m.fingerprint(k))))
            .collect();
        // A top solid replaces the base's for the same surface and colour.
        let laid_solid: HashSet<(DataId, u32)> = materials
            .iter()
            .filter_map(|m| m.solid.as_ref())
            .flat_map(|sd| sd.surfaces.iter().map(|s| (*s, sd.colour)))
            .collect();
        for (i, m) in base.materials.iter().enumerate() {
            if let Some(sd) = &m.solid {
                let surfaces: Vec<DataId> = sd
                    .surfaces
                    .iter()
                    .copied()
                    .filter(|s| !laid_solid.contains(&(*s, sd.colour)))
                    .collect();
                if surfaces.is_empty() {
                    continue;
                }
                let mut m = m.clone();
                m.solid = Some(pyreal_assets::art_pack::Solid {
                    surfaces,
                    ..sd.clone()
                });
                m.grit = m.grit.map(|g| g + grit_at);
                materials.push(m);
                records.push(base.material_source.as_ref().and_then(|s| s.record(i)));
                continue;
            }
            let kept: Vec<(DataId, Option<Fingerprint>)> = (0..m.textures.len())
                .map(|k| (m.textures[k], m.fingerprint(k)))
                .filter(|t| {
                    !m.palettes.is_empty() || !m.laid_palettes.is_empty() || !covered.contains(t)
                })
                .collect();
            if kept.is_empty() {
                continue;
            }
            let mut m = m.clone();
            m.textures = kept.iter().map(|t| t.0).collect();
            m.fingerprints = kept.iter().map(|t| t.1).collect();
            m.grit = m.grit.map(|g| g + grit_at);
            materials.push(m);
            records.push(base.material_source.as_ref().and_then(|s| s.record(i)));
        }

        // A top ground replaces the base's for the same ground made from the same picture.
        let mut grounds = self.grounds.clone();
        let laid: HashSet<(Ground, bool, Option<Fingerprint>)> = self
            .grounds
            .iter()
            .flat_map(|g| g.grounds.iter().map(|&k| (k, g.steep, g.fingerprint)))
            .collect();
        for g in &base.grounds {
            let kinds: Vec<Ground> = g
                .grounds
                .iter()
                .copied()
                .filter(|&k| !laid.contains(&(k, g.steep, g.fingerprint)))
                .collect();
            if !kinds.is_empty() {
                grounds.push(PackGround {
                    grounds: kinds,
                    ..g.clone()
                });
            }
        }

        let mut ground_cover = self.ground_cover.clone();
        let grown = self.ground_cover.iter().fold(0, |m, c| m | c.grounds);
        ground_cover.extend(base.ground_cover.iter().filter_map(|c| {
            let grounds = c.grounds & !grown;
            (grounds != 0).then_some(PackGroundCover { grounds, ..*c })
        }));

        let lit: HashSet<DataId> = self.lights.iter().map(|l| l.object).collect();
        let mut lights = self.lights.clone();
        lights.extend(
            base.lights
                .iter()
                .filter(|l| !lit.contains(&l.object))
                .cloned(),
        );
        let swayed: HashSet<DataId> = self.sways.iter().map(|w| w.object).collect();
        let mut sways = self.sways.clone();
        sways.extend(base.sways.iter().filter(|w| !swayed.contains(&w.object)));
        let mooned: HashSet<DataId> = self.moons.iter().map(|m| m.object).collect();
        let mut moons = self.moons.clone();
        moons.extend(
            base.moons
                .iter()
                .filter(|m| !mooned.contains(&m.object))
                .cloned(),
        );

        Self {
            name: self.name.clone(),
            serial: next_serial(),
            hash: format!("{}+{}", self.hash, base.hash),
            models,
            images,
            grounds,
            materials,
            grits: self.grits.iter().chain(&base.grits).cloned().collect(),
            grit_names: self
                .grit_names
                .iter()
                .chain(&base.grit_names)
                .cloned()
                .collect(),
            ground_cover,
            lights,
            sways,
            moons,
            skins,
            keep,
            keep_launchers: self.keep_launchers || base.keep_launchers,
            problems: self
                .problems
                .iter()
                .chain(&base.problems)
                .cloned()
                .collect(),
            material_source: MaterialSource::new(records),
        }
    }

    /// The body drawn for an object built on `setup`, whose record's fingerprint `record` gives
    /// when asked, and whose look lays `palettes`: of the setup's bodies drawn on the record, those
    /// naming a fingerprint first, and in each the first naming one of the palettes, else one
    /// naming none.
    #[must_use]
    pub fn skin_for(
        &self,
        setup: DataId,
        record: impl FnOnce() -> Option<Fingerprint>,
        palettes: &[DataId],
    ) -> Option<&PackSkin> {
        let rows: Vec<&PackSkin> = self.skins.iter().filter(|k| k.setup == setup).collect();
        if rows.is_empty() {
            return None;
        }
        let known = rows
            .iter()
            .any(|k| k.fingerprint.is_some())
            .then(record)
            .flatten();
        let drawn: Vec<&PackSkin> = rows
            .into_iter()
            .filter(|k| k.draws_on.draws(k.fingerprint.as_ref(), known.as_ref()))
            .collect();
        [true, false].into_iter().find_map(|printed| {
            let tier = drawn.iter().filter(|k| k.fingerprint.is_some() == printed);
            tier.clone()
                .find(|k| k.palettes.iter().any(|p| palettes.contains(p)))
                .or_else(|| tier.clone().find(|k| k.palettes.is_empty()))
                .copied()
        })
    }

    /// Whether the pack has a model for `id`.
    #[must_use]
    pub fn has(&self, id: DataId) -> bool {
        self.models.iter().any(|m| m.id == id)
    }

    /// How much the wind moves retail object `id` drawn with its own parts (`[[sway]]`), if the
    /// pack says.
    #[must_use]
    pub fn sway(&self, id: DataId) -> Option<f32> {
        self.sways.iter().find(|w| w.object == id).map(|w| w.wind)
    }

    /// Material `i`'s picture `part`, with its levels: held, or read again from the packed file.
    #[must_use]
    pub fn material_picture(&self, i: usize, part: MaterialPart) -> Option<Image> {
        let m = self.materials.get(i)?;
        let held = match part {
            MaterialPart::Albedo => m.albedo.as_ref()?,
            MaterialPart::Normal => &m.normal,
            MaterialPart::Surface => &m.surface,
            // Always held: small, and never read again.
            MaterialPart::Metal => return m.metal.clone(),
            MaterialPart::Clear => return m.clear.clone(),
        };
        if !held.levels.is_empty() {
            return Some(held.clone());
        }
        let pictures = self
            .material_source
            .as_ref()?
            .pictures(i, m.albedo.is_none())?;
        let at = match (part, m.albedo.is_some()) {
            (MaterialPart::Albedo, _) => 0,
            (MaterialPart::Normal, true) => 1,
            (MaterialPart::Surface, true) => 2,
            (MaterialPart::Normal, false) => 0,
            (MaterialPart::Surface, false) => 1,
            (MaterialPart::Metal | MaterialPart::Clear, _) => return None,
        };
        pictures.get(at).cloned()
    }

    /// The index of the pack's first material for the retail picture `texture`, if it has one.
    #[must_use]
    pub fn material_of(&self, texture: DataId) -> Option<usize> {
        self.materials
            .iter()
            .position(|m| m.textures.contains(&texture))
    }

    /// The material laid on the untextured surface `surface` of solid colour `colour` (ARGB),
    /// and the metres one repeat of it covers, if the pack has one.
    #[must_use]
    pub fn solid_for(&self, surface: DataId, colour: u32) -> Option<(usize, f32)> {
        self.materials.iter().enumerate().find_map(|(i, m)| {
            let sd = m.solid.as_ref()?;
            (sd.colour == colour && sd.surfaces.contains(&surface)).then_some((i, sd.size))
        })
    }

    /// The index of the pack's material drawn on the retail picture `texture` drawn in
    /// `palettes`, which an object's look lays over it where `laid` (else they are the picture's
    /// own), whose fingerprint `record` gives when asked (`docs/ERAS.md`): of those drawn on the
    /// record, the first whose laid palettes the look lays, else the first that names one of the
    /// palettes, else the first that names none. `record` is asked at most once, and only when a
    /// material for `texture` names a fingerprint.
    #[must_use]
    pub fn material_for(
        &self,
        texture: DataId,
        palettes: &[DataId],
        laid: bool,
        record: impl FnOnce() -> Option<Fingerprint>,
    ) -> Option<usize> {
        let mut record = Some(record);
        let mut known: Option<Option<Fingerprint>> = None;
        let mut drawn = |m: &TextureMaterial| {
            let Some(k) = m.textures.iter().position(|&t| t == texture) else {
                return false;
            };
            let made_from = m.fingerprint(k);
            if made_from.is_some() && known.is_none() {
                known = Some(record.take().and_then(|f| f()));
            }
            m.draws_on
                .draws(made_from.as_ref(), known.flatten().as_ref())
        };
        let (mut first_named, mut first_plain) = (None, None);
        for (i, m) in self.materials.iter().enumerate() {
            let by_look = laid && m.laid_palettes.iter().any(|p| palettes.contains(p));
            let named = m.palettes.iter().any(|p| palettes.contains(p));
            let plain = m.palettes.is_empty() && m.laid_palettes.is_empty();
            if !(by_look || named || plain) || !drawn(m) {
                continue;
            }
            if by_look {
                return Some(i);
            }
            if named {
                first_named.get_or_insert(i);
            } else {
                first_plain.get_or_insert(i);
            }
        }
        first_named.or(first_plain)
    }

    /// The index of the pack's model drawn in place of the retail object `id`, whose fingerprint
    /// `record` gives when asked, as [`Self::material_for`] chooses a material.
    #[must_use]
    pub fn model_for(
        &self,
        id: DataId,
        record: impl FnOnce() -> Option<Fingerprint>,
    ) -> Option<usize> {
        let mut record = Some(record);
        let mut known: Option<Option<Fingerprint>> = None;
        self.models.iter().position(|m| {
            if m.id != id {
                return false;
            }
            if m.fingerprint.is_some() && known.is_none() {
                known = Some(record.take().and_then(|f| f()));
            }
            m.draws_on
                .draws(m.fingerprint.as_ref(), known.flatten().as_ref())
        })
    }

    /// The pack's material for `ground`, if it has one.
    #[must_use]
    pub fn ground(&self, ground: Ground) -> Option<&PackGround> {
        self.grounds
            .iter()
            .find(|g| !g.steep && g.grounds.contains(&ground))
    }

    /// The material laid on `ground`'s steep slopes, if the pack has one.
    #[must_use]
    pub fn steep_ground(&self, ground: Ground) -> Option<&PackGround> {
        self.grounds
            .iter()
            .find(|g| g.steep && g.grounds.contains(&ground))
    }

    /// How many triangles every level of every model holds.
    #[must_use]
    pub fn triangles(&self) -> usize {
        self.models
            .iter()
            .flat_map(|m| &m.lods)
            .flat_map(|l| &l.primitives)
            .map(|p| p.indices.len() / 3)
            .sum()
    }
}

/// Whether a pack's model for `id` lies over its retail object instead of replacing it: a bed of
/// embers in a retail basin or fire pit.
#[must_use]
pub fn lays_over(id: DataId) -> bool {
    matches!(id.0, 0x0200_0285 | 0x0200_0354)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pack_over_the_base_replaces_what_it_stands_for() {
        let image = |w| Image {
            width: w,
            height: w,
            srgb: true,
            levels: Vec::new(),
            format: None,
        };
        let model = |id, picture| PackModel {
            id: DataId(id),
            foliage: false,
            wind: 0.0,
            lods: vec![PackLod {
                distance: f32::INFINITY,
                primitives: vec![PackPrimitive {
                    vertices: Arc::new(Vec::new()),
                    indices: Arc::new(Vec::new()),
                    material: PackMaterial {
                        base_color: [1.0; 4],
                        base_color_image: Some(picture),
                        normal_image: None,
                        normal_scale: 1.0,
                        metal_rough_image: None,
                        metallic: 0.0,
                        roughness: 1.0,
                        occlusion_image: None,
                        occlusion_strength: 1.0,
                        alpha_cutoff: None,
                        double_sided: false,
                    },
                }],
                views: None,
                shadows_only: false,
            }],
            bounds: ([0.0; 3], 1.0),
            fingerprint: None,
            draws_on: Default::default(),
            credit: Credit::default(),
        };
        let sway = |id, wind| pyreal_assets::art_pack::ManifestSway {
            object: DataId(id),
            wind,
        };
        let material = |textures: &[u32], grit| TextureMaterial {
            textures: textures.iter().map(|&t| DataId(t)).collect(),
            albedo: None,
            normal: image(4),
            surface: image(4),
            grit,
            metal: None,
            clear: None,
            grit_repeat: [1.0; 2],
            damp_floors: false,
            slime: false,
            ice: false,
            sheen: 0.0,
            matter: Default::default(),
            fingerprints: Vec::new(),
            palettes: Vec::new(),
            laid_palettes: Vec::new(),
            draws_on: Default::default(),
            solid: None,
            credit: Credit::default(),
        };
        let cover = |grounds| PackGroundCover {
            id: DataId(9),
            grounds,
            density: 1.0,
            scale: (1.0, 1.0),
            steepest: 30.0,
        };
        let base = ArtPack {
            hash: "b".into(),
            models: vec![model(1, 0), model(2, 1)],
            images: vec![image(8), image(16)],
            materials: vec![material(&[10, 11], Some(0)), material(&[12], None)],
            grits: vec![image(4)],
            ground_cover: vec![cover(0b011)],
            sways: vec![sway(5, 1.0), sway(6, 1.0)],
            ..ArtPack::default()
        };
        let top = ArtPack {
            name: "Top".into(),
            hash: "t".into(),
            models: vec![model(2, 0)],
            images: vec![image(32)],
            materials: vec![material(&[11], Some(0))],
            grits: vec![image(4)],
            ground_cover: vec![cover(0b010)],
            sways: vec![sway(6, 0.5)],
            ..ArtPack::default()
        };
        let both = top.over(&base);
        assert_eq!((both.name.as_str(), both.hash.as_str()), ("Top", "t+b"));
        assert_eq!(both.sway(DataId(5)), Some(1.0));
        assert_eq!(
            both.sway(DataId(6)),
            Some(0.5),
            "the top's sway replaces the base's"
        );
        assert_eq!(both.sways.len(), 2);
        let ids: Vec<u32> = both.models.iter().map(|m| m.id.0).collect();
        assert_eq!(ids, [2, 1], "the top's model replaces the base's");
        let picture = |m: &PackModel| m.lods[0].primitives[0].material.base_color_image;
        assert_eq!(picture(&both.models[0]), Some(0));
        assert_eq!(
            picture(&both.models[1]),
            Some(1),
            "the base's pictures follow the top's"
        );
        assert_eq!(
            both.images.len(),
            2,
            "a replaced model's pictures are left out"
        );
        assert_eq!(both.images[1].width, 8);
        let textures: Vec<Vec<u32>> = both
            .materials
            .iter()
            .map(|m| m.textures.iter().map(|t| t.0).collect())
            .collect();
        assert_eq!(textures, [vec![11], vec![10], vec![12]]);
        assert_eq!(
            both.materials[1].grit,
            Some(1),
            "the base's grit follows the top's"
        );
        assert_eq!(both.material_of(DataId(11)), Some(0));
        let grounds: Vec<u32> = both.ground_cover.iter().map(|c| c.grounds).collect();
        assert_eq!(grounds, [0b010, 0b001]);
    }

    #[test]
    fn each_era_draws_the_entry_made_from_its_own_record() {
        let flat = Image {
            width: 4,
            height: 4,
            srgb: false,
            levels: Vec::new(),
            format: None,
        };
        let picture = |hash: u64| Fingerprint::Picture {
            width: 64,
            height: 64,
            hash,
        };
        let (eor, older, other) = (picture(0), picture(u64::MAX), picture(0xFFFF_FFFF));
        let material = |print: Option<Fingerprint>, draws_on| TextureMaterial {
            textures: vec![DataId(0x0500_0001)],
            albedo: None,
            normal: flat.clone(),
            surface: flat.clone(),
            grit: None,
            metal: None,
            clear: None,
            grit_repeat: [1.0; 2],
            damp_floors: false,
            slime: false,
            ice: false,
            sheen: 0.0,
            matter: Default::default(),
            fingerprints: vec![print],
            palettes: Vec::new(),
            laid_palettes: Vec::new(),
            draws_on,
            solid: None,
            credit: Credit::default(),
        };
        let pick = |pack: &ArtPack, record: Fingerprint| {
            pack.material_for(DataId(0x0500_0001), &[], false, || Some(record))
        };
        let base = ArtPack {
            hash: "b".into(),
            materials: vec![
                material(Some(eor), DrawsOn::Matching),
                material(Some(older), DrawsOn::Matching),
            ],
            ..ArtPack::default()
        };
        assert_eq!(pick(&base, eor), Some(0));
        assert_eq!(pick(&base, older), Some(1));
        assert_eq!(
            pick(&base, other),
            None,
            "no entry was made from this record"
        );
        assert_eq!(
            base.material_for(DataId(0x0500_0001), &[], false, || None),
            Some(0),
            "a record of no known fingerprint takes the first"
        );
        assert_eq!(
            base.material_for(DataId(0x0500_0002), &[], false, || Some(eor)),
            None
        );

        // A chosen pack's entry naming no fingerprint stands for the base's first, and the base's
        // entry for the older record still draws there.
        let top = ArtPack {
            name: "Top".into(),
            hash: "t".into(),
            materials: vec![material(None, DrawsOn::Matching)],
            ..ArtPack::default()
        };
        let both = top.over(&base);
        assert_eq!(both.materials.len(), 2);
        assert_eq!(both.materials[0].fingerprint(0), Some(eor));
        assert_eq!(pick(&both, eor), Some(0), "the top's");
        assert_eq!(pick(&both, older), Some(1), "the base's older one");

        let any = ArtPack {
            materials: vec![
                material(Some(eor), DrawsOn::Nothing),
                material(Some(eor), DrawsOn::Any),
            ],
            ..ArtPack::default()
        };
        assert_eq!(
            pick(&any, other),
            Some(1),
            "drawn on any record, the first on none"
        );
    }

    #[test]
    fn a_material_for_a_palette_is_drawn_in_that_palette_before_one_for_any() {
        let flat = Image {
            width: 4,
            height: 4,
            srgb: false,
            levels: Vec::new(),
            format: None,
        };
        let material = |palettes: &[u32], laid: &[u32]| TextureMaterial {
            textures: vec![DataId(0x0500_1310)],
            albedo: None,
            normal: flat.clone(),
            surface: flat.clone(),
            grit: None,
            metal: None,
            clear: None,
            grit_repeat: [1.0; 2],
            damp_floors: false,
            slime: false,
            ice: false,
            sheen: 0.0,
            matter: Default::default(),
            fingerprints: Vec::new(),
            draws_on: DrawsOn::Matching,
            palettes: palettes.iter().map(|&p| DataId(p)).collect(),
            laid_palettes: laid.iter().map(|&p| DataId(p)).collect(),
            solid: None,
            credit: Credit::default(),
        };
        let pack = ArtPack {
            materials: vec![
                material(&[], &[]),
                material(&[0x0400_0BA1, 0x0400_0B9F], &[]),
                material(&[0x0400_0BA3], &[]),
            ],
            ..ArtPack::default()
        };
        let pick = |palettes: &[u32]| {
            let palettes: Vec<DataId> = palettes.iter().map(|&p| DataId(p)).collect();
            pack.material_for(DataId(0x0500_1310), &palettes, false, || None)
        };
        assert_eq!(pick(&[0x0400_0B9F]), Some(1));
        assert_eq!(pick(&[0x0400_0BA3]), Some(2));
        assert_eq!(
            pick(&[0x0400_0BA6]),
            Some(0),
            "a palette no material names takes the plain one"
        );
        assert_eq!(pick(&[]), Some(0));
        let only_named = ArtPack {
            materials: vec![material(&[0x0400_0BA3], &[])],
            ..ArtPack::default()
        };
        assert_eq!(
            only_named.material_for(DataId(0x0500_1310), &[DataId(0x0400_0BA6)], false, || None),
            None
        );
        // A creature that wears its picture's own palette (a crystal shard in the world's
        // crystal's blue) takes the material laid for it; the world's crystal, drawn in that palette
        // as its own, keeps the picture's.
        let own = 0x0400_0BEF;
        let laid = ArtPack {
            materials: vec![material(&[own], &[]), material(&[0x0400_0BF3], &[own])],
            ..ArtPack::default()
        };
        let pick = |palettes: &[u32], by_look| {
            let palettes: Vec<DataId> = palettes.iter().map(|&p| DataId(p)).collect();
            laid.material_for(DataId(0x0500_1310), &palettes, by_look, || None)
        };
        assert_eq!(pick(&[own], true), Some(1));
        assert_eq!(pick(&[own], false), Some(0));
        assert_eq!(pick(&[0x0400_0BF3], false), Some(1));
        assert_eq!(pick(&[0x0400_0BA6], true), None);
    }

    #[test]
    fn a_road_stands_up_from_its_own_picture_and_keeps_its_colour() {
        // A pale stone with a dark joint across it, every 16 texels.
        let n = 64usize;
        let mut albedo = Vec::with_capacity(n * n * 4);
        for y in 0..n {
            for _ in 0..n {
                let v = if y % 16 < 2 { 40 } else { 200 };
                albedo.extend([v, v, v, 128]);
            }
        }
        let colour: Vec<u8> = albedo.chunks(4).map(|t| t[0]).collect();
        let (normal, surface) = road_relief(64, &mut albedo, 220);
        assert_eq!(colour, albedo.chunks(4).map(|t| t[0]).collect::<Vec<u8>>());
        let at = |y: usize| (y * n) * 4;
        // The joint lies below the stone, in shade, and the light bends at its edge alone.
        assert!(albedo[at(1) + 3] < albedo[at(9) + 3]);
        assert!(surface[at(1) + 1] < surface[at(9) + 1]);
        assert!(normal[at(2) + 1].abs_diff(128) > 4, "{}", normal[at(2) + 1]);
        // A picture with nothing in it stays level.
        let mut plain = [90u8, 90, 90, 128].repeat(n * n);
        let (normal, _) = road_relief(64, &mut plain, 220);
        assert!(normal
            .chunks(4)
            .all(|t| t[0].abs_diff(128) < 2 && t[1].abs_diff(128) < 2));
    }

    #[test]
    fn a_retail_roads_maps_keep_its_height_in_the_surface_and_add_no_shade() {
        // Pale stones with a dark joint every 16 rows, 48 texels wide: worked out at 64.
        let (w, h) = (48u32, 64u32);
        let rgba: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let v = if (i / w) % 16 < 2 { 40 } else { 200 };
                [v, v, v, 255]
            })
            .collect();
        let [normal, surface] = retail_road_maps(w, h, &rgba);
        assert_eq!((surface.width, normal.width), (64, 64));
        assert_eq!(normal.format, Some(images::BlockFormat::Bc5));
        let top = &surface.levels[0];
        let at = |y: usize| y * 64 * 4;
        assert!(top[at(1) + 2] < top[at(9) + 2], "the joint lies lower");
        assert!(top
            .as_chunks::<4>()
            .0
            .iter()
            .all(|t| t[0] == 220 && t[1] == 255));
    }

    /// A pack of one model with two levels, a triangle each, its picture shared.
    fn files() -> HashMap<String, Vec<u8>> {
        let mut bin = Vec::new();
        for p in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 3.0, 0.0]] {
            for c in p {
                bin.extend(c.to_le_bytes());
            }
        }
        let gltf = r#"{"asset": {"version": "2.0"}, "scenes": [{"nodes": [0]}],
            "nodes": [{"mesh": 0}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "material": 0}]}],
            "materials": [{"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}}],
            "textures": [{"source": 0}], "images": [{"uri": "bark.png"}],
            "buffers": [{"uri": "tri.bin", "byteLength": 36}],
            "bufferViews": [{"buffer": 0, "byteLength": 36}],
            "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}]}"#;
        let mut png_bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png_bytes, 1, 1);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().expect("header");
            w.write_image_data(&[200, 100, 50, 255]).expect("data");
        }
        let mut f = HashMap::new();
        f.insert("tree/lod0.gltf".to_owned(), gltf.as_bytes().to_vec());
        f.insert("tree/lod1.gltf".to_owned(), gltf.as_bytes().to_vec());
        f.insert("tree/tri.bin".to_owned(), bin);
        f.insert("tree/bark.png".to_owned(), png_bytes);
        f
    }

    const MANIFEST: &str = r#"
[pack]
name = "Test"
format = 1
[[model]]
id = "0x02000258"
foliage = true
author = "a"
tool = "t"
license = "CC0-1.0"
[[model.lod]]
file = "tree/lod0.gltf"
distance = 30
[[model.lod]]
file = "tree/lod1.gltf"
[[model]]
id = "0x020005C9"
author = "a"
tool = "t"
license = "CC0-1.0"
[[model.lod]]
file = "rock/missing.gltf"
"#;

    #[test]
    fn a_pack_reads_its_models_through_one_function_of_a_path_and_leaves_out_what_will_not_read() {
        let files = files();
        let mut asked: Vec<String> = Vec::new();
        let pack = ArtPack::open(MANIFEST, &mut |path| {
            asked.push(path.to_owned());
            files
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{path}: missing"))
        })
        .expect("the pack opens");
        assert_eq!(pack.name, "Test");
        assert!(pack.has(DataId(0x0200_0258)));
        assert!(!pack.has(DataId(0x0200_05C9)), "its model would not read");
        assert_eq!(pack.problems.len(), 1);
        assert!(
            pack.problems[0].contains("0x020005C9"),
            "{:?}",
            pack.problems
        );
        let tree = &pack.models[0];
        assert!(tree.foliage);
        // Its card takes over past the cards' distance, its two pictures after the pack's.
        assert_eq!(
            tree.lods.iter().map(|l| l.distance).collect::<Vec<_>>(),
            [30.0, cards::cards_from(), f32::INFINITY]
        );
        assert_eq!(tree.lods[2].views, Some(cards::VIEWS));
        // Both levels name the same picture: it is read once.
        assert_eq!(pack.images.len(), 1 + 2);
        assert_eq!(asked.iter().filter(|p| *p == "tree/bark.png").count(), 1);
        assert_eq!(
            tree.lods[1].primitives[0].material.base_color_image,
            Some(0)
        );
        assert!(pack.images[0].srgb);
        // One texel: stretched to a whole block and compressed.
        assert_eq!(pack.images[0].format, Some(images::BlockFormat::Bc7));
        assert_eq!((pack.images[0].width, pack.images[0].height), (4, 4));
        // The triangle, and its card's two.
        assert_eq!(pack.triangles(), 2 + 2);
        // The triangle stands three metres high, upright.
        let (centre, radius) = tree.bounds;
        assert!((centre[2] - 1.5).abs() < 1e-6, "{centre:?}");
        assert!(radius > 1.5 && radius < 1.6, "{radius}");
        // A manifest whose every model fails opens nothing.
        let none = ArtPack::open(
            &MANIFEST.replace("tree/lod0.gltf", "nowhere.gltf"),
            &mut |p| {
                if p.starts_with("tree/") {
                    files.get(p).cloned().ok_or_else(|| p.to_owned())
                } else {
                    Err(format!("{p}: missing"))
                }
            },
        );
        assert!(none.is_err());
        // A model whose picture will not decode is left out with a line naming the picture.
        let mut broken = files.clone();
        broken.insert("tree/bark.png".to_owned(), b"not a png".to_vec());
        let e = ArtPack::open(MANIFEST, &mut |p| {
            broken
                .get(p)
                .cloned()
                .ok_or_else(|| format!("{p}: missing"))
        })
        .expect_err("no model reads");
        assert!(e.contains("0x02000258: tree/bark.png"), "{e}");
    }

    fn png_of(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut bytes, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().expect("header");
            w.write_image_data(rgba).expect("data");
        }
        bytes
    }

    #[test]
    fn a_clear_map_keeps_where_it_is_clear_and_where_it_is_metal() {
        // Red clear, green metal: a clear texel, a plain solid one and a metal one, twice over.
        let texels = [
            [255, 0, 0, 255],
            [0, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 255, 0, 255],
        ];
        let rgba: Vec<u8> = texels.iter().cycle().take(16).flatten().copied().collect();
        let map = materials::clear(&png_of(4, 4, &rgba), "c.png").expect("a clear map");
        assert_eq!(map.format, Some(images::BlockFormat::Bc5));
        assert_eq!((map.width, map.height), (4, 4));
    }

    #[test]
    fn a_material_that_keeps_the_retail_picture_neither_reads_nor_draws_its_base_color() {
        let mut files = HashMap::new();
        files.insert(
            "m/wall_n.png".to_owned(),
            png_of(8, 8, &[160, 128, 230, 255].repeat(64)),
        );
        let manifest = r#"
[pack]
name = "Relief"
format = 1
[[material]]
texture = "0x0500287C"
base_color = "m/fallback.png"
keep_retail_picture = true
normal = "m/wall_n.png"
damp = 1
sheen = 0.6
author = "a"
tool = "t"
license = "CC0-1.0"
"#;
        // The fallback colour is not in the folder: it is never read.
        let pack = ArtPack::open(manifest, &mut |path| {
            files
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{path}: missing"))
        })
        .expect("the pack opens");
        assert_eq!(pack.materials.len(), 1, "{:?}", pack.problems);
        let wall = &pack.materials[0];
        assert!(wall.albedo.is_none());
        assert!((wall.sheen - 0.6).abs() < 1e-6, "its sheen is kept");
        // Sized by the picture it has.
        assert_eq!((wall.normal.width, wall.surface.width), (8, 4));
        // Without the key the base color is read, and drawn.
        let plain = manifest.replace("keep_retail_picture = true\n", "");
        let pack = ArtPack::open(&plain, &mut |path| {
            files
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{path}: missing"))
        })
        .expect("the pack opens");
        assert!(pack.materials.is_empty());
        assert!(
            pack.problems[0].contains("m/fallback.png"),
            "{:?}",
            pack.problems
        );
        // Keeping the retail picture with no picture of its own leaves nothing to draw.
        let nothing = manifest.replace("normal = \"m/wall_n.png\"\n", "");
        let pack = ArtPack::open(&nothing, &mut |_| Err("missing".to_owned())).expect("opens");
        assert!(pack.materials.is_empty() && pack.problems.len() == 1);
    }

    #[test]
    fn a_solid_is_found_by_its_surface_and_colour_and_kept_under_a_pack_over_it() {
        let mut files = HashMap::new();
        files.insert(
            "m/grain.png".to_owned(),
            png_of(8, 8, &[240, 240, 240, 255].repeat(64)),
        );
        let solid = |surface: &str| {
            format!(
                "[pack]\nname = \"S\"\nformat = 1\n[[solid]]\nsurface = \"{surface}\"\n\
                 colour = \"0xFF320000\"\nsize = 0.5\nbase_color = \"m/grain.png\"\n\
                 author = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n"
            )
        };
        let open = |manifest: &str| {
            ArtPack::open(manifest, &mut |path| {
                files
                    .get(path)
                    .cloned()
                    .ok_or_else(|| format!("{path}: missing"))
            })
            .expect("the pack opens")
        };
        let base = open(&solid("0x080006F7"));
        assert_eq!(
            base.solid_for(DataId(0x0800_06F7), 0xFF32_0000),
            Some((0, 0.5))
        );
        // Another colour under the same id is another era's surface.
        assert_eq!(base.solid_for(DataId(0x0800_06F7), 0xFF00_0032), None);
        assert_eq!(
            base.material_for(DataId(0x0800_06F7), &[], false, || None),
            None
        );
        // A pack over the base keeps the base's solid where it has none of its own.
        let top = open(&solid("0x08000203"));
        let both = top.over(&base);
        assert!(both.solid_for(DataId(0x0800_06F7), 0xFF32_0000).is_some());
        assert!(both.solid_for(DataId(0x0800_0203), 0xFF32_0000).is_some());
        assert_eq!(top.over(&top).materials.len(), 1);
    }

    #[test]
    fn a_material_with_only_a_base_color_keeps_its_flat_bend_and_surface_one_block_across() {
        let mut files = HashMap::new();
        files.insert(
            "m/wall.png".to_owned(),
            png_of(16, 16, &[90, 60, 30, 255].repeat(256)),
        );
        let manifest = r#"
[pack]
name = "Pictures"
format = 1
[[material]]
texture = "0x0500287C"
base_color = "m/wall.png"
author = "a"
tool = "t"
license = "CC0-1.0"
"#;
        let pack = ArtPack::open(manifest, &mut |path| {
            files
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{path}: missing"))
        })
        .expect("the pack opens");
        assert_eq!(pack.materials.len(), 1, "{:?}", pack.problems);
        let wall = &pack.materials[0];
        let albedo = wall.albedo.as_ref().expect("a base colour");
        assert_eq!((albedo.width, albedo.height), (16, 16));
        assert_eq!((wall.normal.width, wall.normal.height), (4, 4));
        assert_eq!((wall.surface.width, wall.surface.height), (4, 4));
    }

    #[test]
    fn a_materials_maps_keep_their_own_size_under_a_larger_base_color() {
        let mut files = HashMap::new();
        files.insert(
            "m/wall.png".to_owned(),
            png_of(32, 32, &[90, 60, 30, 255].repeat(32 * 32)),
        );
        files.insert(
            "m/wall_n.png".to_owned(),
            png_of(16, 8, &[150, 110, 255, 255].repeat(16 * 8)),
        );
        files.insert(
            "m/wall_r.png".to_owned(),
            png_of(16, 16, &[60, 60, 60, 255].repeat(16 * 16)),
        );
        let manifest = r#"
[pack]
name = "Pictures"
format = 1
[[material]]
texture = "0x0500287C"
base_color = "m/wall.png"
normal = "m/wall_n.png"
roughness = "m/wall_r.png"
author = "a"
tool = "t"
license = "CC0-1.0"
"#;
        let open = |manifest: &str| {
            ArtPack::open(manifest, &mut |path| {
                files
                    .get(path)
                    .cloned()
                    .ok_or_else(|| format!("{path}: missing"))
            })
            .expect("the pack opens")
        };
        let pack = open(manifest);
        let wall = &pack.materials[0];
        assert_eq!(wall.albedo.as_ref().map(|a| a.width), Some(32));
        // Neither map is stretched to the base colour; the surface is half the base colour's.
        assert_eq!((wall.normal.width, wall.normal.height), (16, 8));
        assert_eq!((wall.surface.width, wall.surface.height), (16, 16));
        // With no normal of its own, its normal is one block, which the engine draws as none.
        let pack = open(&manifest.replace("normal = \"m/wall_n.png\"\n", ""));
        let wall = &pack.materials[0];
        assert_eq!((wall.normal.width, wall.normal.height), (4, 4));
        assert_eq!((wall.surface.width, wall.surface.height), (16, 16));
        // With no surface picture, its even surface is half its normal's size.
        let pack = open(&manifest.replace("roughness = \"m/wall_r.png\"\n", ""));
        let wall = &pack.materials[0];
        assert_eq!((wall.surface.width, wall.surface.height), (8, 4));
    }

    #[test]
    fn a_material_for_a_retail_picture_keeps_its_alpha_and_is_read_as_the_retail_picture_is() {
        let mut files = HashMap::new();
        // An eight by eight picture whose left half is cut away (alpha 0), as a retail cut-out
        // picture is; a normal picture leaning toward the picture's right.
        let mut base = Vec::new();
        for _ in 0..8 {
            for x in 0..8 {
                base.extend([120, 90, 60, if x < 4 { 0 } else { 255 }]);
            }
        }
        files.insert("m/wall.png".to_owned(), png_of(8, 8, &base));
        files.insert(
            "m/wall_n.png".to_owned(),
            png_of(8, 8, &[160, 128, 230, 255].repeat(64)),
        );
        files.insert(
            "m/wall_r.png".to_owned(),
            png_of(8, 8, &[50, 0, 0, 255].repeat(64)),
        );
        files.insert(
            "m/wall_h.png".to_owned(),
            png_of(8, 8, &[77, 0, 0, 255].repeat(64)),
        );
        // A picture whose sides are not whole blocks is stretched to them.
        files.insert(
            "m/floor.png".to_owned(),
            png_of(6, 5, &[10, 200, 30, 255].repeat(30)),
        );
        let manifest = r#"
[pack]
name = "Materials"
format = 1
[[material]]
texture = ["0x0500287C", "0x0500287D"]
base_color = "m/wall.png"
normal = "m/wall_n.png"
roughness = "m/wall_r.png"
height = "m/wall_h.png"
normal_scale = 2.0
damp = 0.5
author = "a"
tool = "t"
license = "CC0-1.0"
[[material]]
texture = "0x05002752"
base_color = "m/floor.png"
author = "a"
tool = "t"
license = "CC0-1.0"
[[material]]
texture = "0x05001CB4"
base_color = "m/missing.png"
author = "a"
tool = "t"
license = "CC0-1.0"
"#;
        let pack = ArtPack::open(manifest, &mut |path| {
            files
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{path}: missing"))
        })
        .expect("the pack opens");
        assert_eq!(pack.materials.len(), 2, "{:?}", pack.problems);
        assert!(
            pack.problems[0].contains("0x05001CB4"),
            "{:?}",
            pack.problems
        );
        assert_eq!(pack.material_of(DataId(0x0500_287D)), Some(0));
        assert_eq!(pack.material_of(DataId(0x0500_2752)), Some(1));
        assert_eq!(pack.material_of(DataId(0x0500_1CB4)), None);
        let wall = &pack.materials[0];
        // Read as stored, as the retail picture is, and compressed in blocks.
        let albedo = wall.albedo.as_ref().expect("a base color");
        assert!(!albedo.srgb);
        assert_eq!(albedo.format, Some(images::BlockFormat::Bc7));
        let texels =
            pyreal_codecs::block_codec::decode(images::BlockFormat::Bc7, 8, 8, &albedo.levels[0]);
        // The cut-away half keeps its alpha, the rest its color.
        for y in 0..8 {
            for x in 0..8 {
                let t = &texels[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4];
                if x < 4 {
                    assert!(t[3] < 8, "{x},{y}: {t:?}");
                } else {
                    assert!(t[3] > 247, "{x},{y}: {t:?}");
                    assert!(t[0].abs_diff(120) <= 4 && t[2].abs_diff(60) <= 4, "{t:?}");
                }
            }
        }
        // The normal leans twice as far as its picture: 160 is 0.25 of the way; bent, 0.5.
        let normal = pyreal_codecs::block_codec::decode(
            images::BlockFormat::Bc5,
            8,
            8,
            &wall.normal.levels[0],
        );
        let x = f32::from(normal[0]) / 255.0 * 2.0 - 1.0;
        assert!((x - 0.5).abs() < 0.04, "{x}");
        // The roughness, occlusion, height and dampness at half the size; occlusion none where it
        // names none.
        assert_eq!((wall.surface.width, wall.surface.height), (4, 4));
        let surface = pyreal_codecs::block_codec::decode(
            images::BlockFormat::Bc7,
            4,
            4,
            &wall.surface.levels[0],
        );
        assert!(
            surface[0].abs_diff(50) <= 3
                && surface[1] > 250
                && surface[2].abs_diff(77) <= 3
                && surface[3].abs_diff(128) <= 3,
            "{:?}",
            &surface[..4]
        );
        let floor = &pack.materials[1];
        let floor_color = floor.albedo.as_ref().expect("a base color");
        assert_eq!((floor_color.width, floor_color.height), (8, 8));
    }

    #[test]
    fn a_ground_material_packs_its_pictures_three_to_a_material_at_one_size() {
        let mut files = HashMap::new();
        files.insert(
            "g/a.png".to_owned(),
            png_of(2, 2, &[200, 100, 50, 255].repeat(4)),
        );
        files.insert(
            "g/a_h.png".to_owned(),
            png_of(2, 2, &[90, 0, 0, 255].repeat(4)),
        );
        files.insert(
            "g/a_n.png".to_owned(),
            png_of(2, 2, &[140, 120, 255, 255].repeat(4)),
        );
        // The second material's pictures are larger: they are brought to the first's size.
        files.insert(
            "g/b.png".to_owned(),
            png_of(4, 4, &[10, 20, 30, 255].repeat(16)),
        );
        files.insert(
            "g/b_r.png".to_owned(),
            png_of(4, 4, &[60, 0, 0, 255].repeat(16)),
        );
        let manifest = r#"
[pack]
name = "Ground"
format = 1
[[terrain]]
ground = [1, 9]
base_color = "g/a.png"
height = "g/a_h.png"
normal = "g/a_n.png"
size = 4
author = "a"
tool = "t"
license = "CC0-1.0"
[[terrain]]
ground = "road"
base_color = "g/b.png"
roughness = "g/b_r.png"
size = 8
author = "a"
tool = "t"
license = "CC0-1.0"
[[terrain]]
ground = 3
base_color = "g/missing.png"
size = 8
author = "a"
tool = "t"
license = "CC0-1.0"
"#;
        let pack = ArtPack::open(manifest, &mut |p| {
            files.get(p).cloned().ok_or_else(|| format!("{p}: missing"))
        })
        .expect("a pack of grounds alone opens");
        assert!(pack.models.is_empty());
        assert_eq!(pack.grounds.len(), 2, "{:?}", pack.problems);
        assert!(
            pack.problems[0].contains("missing.png"),
            "{:?}",
            pack.problems
        );
        // Two texels a side are not whole blocks: the pictures stay four bytes a texel.
        let grass = pack.ground(Ground::Type(9)).expect("type 9");
        assert_eq!(&grass.albedo.levels[0][..4], &[200, 100, 50, 90]);
        assert_eq!(&grass.normal.levels[0][..4], &[140, 120, 0, 255]);
        // The roughness and occlusion at half the size: one texel.
        assert_eq!((grass.surface.width, grass.surface.height), (1, 1));
        assert_eq!(&grass.surface.levels[0][..4], &[230, 255, 0, 255]);
        assert!(grass.albedo.srgb && !grass.normal.srgb && !grass.surface.srgb);
        assert_eq!(grass.albedo.format, None);
        let road = pack.ground(Ground::Road).expect("the road");
        assert_eq!((road.albedo.width, road.albedo.height), (2, 2));
        assert_eq!(&road.albedo.levels[0][..4], &[10, 20, 30, 128]);
        assert_eq!(&road.normal.levels[0][..4], &[128, 128, 0, 255]);
        assert_eq!(&road.surface.levels[0][..4], &[60, 255, 0, 255]);
        assert!(pack.ground(Ground::Type(3)).is_none());
    }

    #[test]
    fn a_ground_materials_pictures_are_compressed_in_blocks_and_read_back_closely() {
        use images::BlockFormat;
        // Eight texels a side: whole blocks.
        let mut files = HashMap::new();
        let ramp = |c: [u8; 3]| -> Vec<u8> {
            (0..64u8)
                .flat_map(|k| {
                    [
                        c[0].saturating_add(k),
                        c[1],
                        c[2].saturating_add(k / 2),
                        255,
                    ]
                })
                .collect()
        };
        files.insert("g/a.png".to_owned(), png_of(8, 8, &ramp([100, 140, 20])));
        files.insert("g/a_h.png".to_owned(), png_of(8, 8, &ramp([60, 0, 0])));
        files.insert("g/a_n.png".to_owned(), png_of(8, 8, &ramp([110, 150, 255])));
        files.insert("g/a_r.png".to_owned(), png_of(8, 8, &ramp([180, 0, 0])));
        let manifest = r#"
[pack]
name = "Ground"
format = 1
[[terrain]]
ground = 1
base_color = "g/a.png"
height = "g/a_h.png"
normal = "g/a_n.png"
roughness = "g/a_r.png"
size = 4
author = "a"
tool = "t"
license = "CC0-1.0"
"#;
        let pack = ArtPack::open(manifest, &mut |p| {
            files.get(p).cloned().ok_or_else(|| format!("{p}: missing"))
        })
        .expect("the pack opens");
        let g = pack.ground(Ground::Type(1)).expect("type 1");
        assert_eq!(g.albedo.format, Some(BlockFormat::Bc7));
        assert_eq!(g.normal.format, Some(BlockFormat::Bc5));
        assert_eq!(g.surface.format, Some(BlockFormat::Bc5));
        assert_eq!((g.surface.width, g.albedo.width), (4, 8));
        // A quarter of four bytes a texel: two by two blocks of sixteen bytes; the half-size
        // surface one block; every level down to one texel, a block each.
        assert_eq!(g.albedo.levels[0].len(), 4 * 16);
        assert_eq!(g.albedo.levels.len(), 4);
        assert_eq!(g.surface.levels[0].len(), 16);
        let decode = |i: &Image| {
            pyreal_codecs::block_codec::decode(
                i.format.expect("blocks"),
                i.width,
                i.height,
                &i.levels[0],
            )
        };
        let close = |a: &[u8], b: &[u8], channels: &[usize]| {
            a.chunks(4)
                .zip(b.chunks(4))
                .all(|(p, q)| channels.iter().all(|c| p[*c].abs_diff(q[*c]) <= 6))
        };
        let base = ramp([100, 140, 20]);
        let height = ramp([60, 0, 0]);
        let want: Vec<u8> = base
            .chunks(4)
            .zip(height.chunks(4))
            .flat_map(|(c, h)| [c[0], c[1], c[2], h[0]])
            .collect();
        assert!(close(&decode(&g.albedo), &want, &[0, 1, 2, 3]));
        let normal = ramp([110, 150, 255]);
        assert!(close(&decode(&g.normal), &normal, &[0, 1]));
        // The roughness in red, nothing of the occlusion named: whole in green.
        let surface = decode(&g.surface);
        assert!(surface.chunks(4).all(|t| t[1] >= 254));
        // Larger, a ground with no normal or surface picture keeps each as one block of it.
        files.insert(
            "g/b.png".to_owned(),
            png_of(16, 16, &ramp([1, 2, 3]).repeat(4)),
        );
        let plain = format!("{manifest}[[terrain]]\nground = 2\nbase_color = \"g/b.png\"\nsize = 4\nauthor = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n");
        let pack = ArtPack::open(&plain, &mut |p| {
            files.get(p).cloned().ok_or_else(|| format!("{p}: missing"))
        })
        .expect("the pack opens");
        let g = pack.ground(Ground::Type(2)).expect("type 2");
        assert_eq!((g.normal.width, g.surface.width), (4, 4));
        assert_eq!(&decode(&g.normal)[..2], &[128, 128]);
        assert_eq!(&decode(&g.surface)[..2], &[230, 255]);
        let g = pack.ground(Ground::Type(1)).expect("type 1");
        assert_eq!((g.normal.width, g.surface.width), (8, 4));
    }

    #[test]
    fn the_retail_ground_holds_retails_masks_and_its_pictures_where_the_pack_has_no_material() {
        use pyreal_assets::region::{CodeTexture, TerrainDesc, TexMerge};
        let desc = |t: u32, gid: u32| TerrainDesc {
            terrain_type: t,
            tex_gid: DataId(gid),
            tex_tiling: 2,
            max_vert_bright: 0,
            min_vert_bright: 0,
            max_vert_saturate: 0,
            min_vert_saturate: 0,
            max_vert_hue: 0,
            min_vert_hue: 0,
            detail_tex_tiling: 0,
            detail_tex_gid: DataId(0),
        };
        let code = |c: u32, gid: u32| CodeTexture {
            code: c,
            tex_gid: DataId(gid),
        };
        let tm = TexMerge {
            base_tex_size: 16,
            corner_terrain_maps: vec![code(8, 0x0500_0100), code(8, 0x0500_0101)],
            side_terrain_maps: vec![code(9, 0x0500_0200)],
            // The road's first mask is a corner mask too: it is held once.
            road_maps: vec![code(8, 0x0500_0100), code(10, 0x0500_0300)],
            terrain_desc: vec![
                desc(0, 0x0600_0000),
                desc(1, 0x0600_0001),
                desc(16, 0x0600_0016),
                desc(22, 0x0600_0016),
                desc(32, 0x0600_0020),
            ],
        };
        let mut pack = ArtPack {
            name: String::new(),
            serial: 0,
            hash: String::new(),
            models: Vec::new(),
            images: Vec::new(),
            grounds: Vec::new(),
            materials: Vec::new(),
            grits: Vec::new(),
            grit_names: Vec::new(),
            material_source: None,
            ground_cover: Vec::new(),
            lights: Vec::new(),
            sways: Vec::new(),
            moons: Vec::new(),
            skins: Vec::new(),
            keep: Vec::new(),
            keep_launchers: false,
            problems: Vec::new(),
        };
        let one = images::with_levels(4, 4, vec![9; 64], true, None);
        pack.grounds.push(PackGround {
            grounds: vec![Ground::Type(1), Ground::Road],
            albedo: one.clone(),
            normal: one.clone(),
            surface: one,
            size: 4.0,
            normal_scale: 1.0,
            steep: false,
            fingerprint: None,
            detail: None,
            detail_size: 1.0,
            detail_strength: 1.0,
            pebbles: true,
            credit: Credit::default(),
        });
        let mut asked = Vec::new();
        let ground = RetailGround::of(&tm, &pack, &mut |id| {
            asked.push(id.0);
            Some((2, 2, vec![id.0.to_le_bytes()[0]; 16]))
        });
        assert_eq!(
            ground.masks.iter().map(|(id, _)| id.0).collect::<Vec<_>>(),
            [0x0500_0100, 0x0500_0101, 0x0500_0200, 0x0500_0300]
        );
        assert_eq!(ground.mask(DataId(0x0500_0300)), Some(3));
        assert_eq!(ground.mask(DataId(0x0500_0999)), None);
        // Type 1 and the road have the pack's materials. Every other type is drawn with the
        // first description's picture unless it has one of its own; the water's is shared.
        let by_picture: Vec<Vec<u8>> = ground.pictures.iter().map(|p| p.grounds.clone()).collect();
        assert_eq!(by_picture[1], [16, 22]);
        assert!(by_picture[0].contains(&0) && by_picture[0].contains(&31));
        assert!(!by_picture.iter().flatten().any(|g| *g == 1 || *g == 32));
        // Brought to the pack's size and compressed, a level height, a flat normal, a wet sheen on
        // the water (its roughness and occlusion at half the size, two texels: not compressed).
        let water = &ground.pictures[1];
        assert_eq!((water.albedo.width, water.tiling), (4, 2));
        assert_eq!(water.albedo.format, Some(images::BlockFormat::Bc7));
        let albedo = pyreal_codecs::block_codec::decode(
            images::BlockFormat::Bc7,
            4,
            4,
            &water.albedo.levels[0],
        );
        assert!(albedo.chunks(4).all(|t| t[3].abs_diff(128) <= 1));
        let normal = pyreal_codecs::block_codec::decode(
            images::BlockFormat::Bc5,
            4,
            4,
            &water.normal.levels[0],
        );
        assert!(normal.chunks(4).all(|t| t[0] == 128 && t[1] == 128));
        assert_eq!(water.surface.width, 2);
        assert_eq!(&water.surface.levels[0][..2], &[100, 255]);
        assert_eq!(ground.pictures[0].surface.levels[0][0], 220);
        // A mask's alpha is kept in its red (two texels a side: not compressed).
        assert_eq!(&ground.masks[1].1.levels[0][..4], &[0x01, 0, 0, 255]);
        // Each picture read once.
        assert_eq!(asked.iter().filter(|a| **a == 0x0600_0016).count(), 1);
        // A broad picture for every ground, the pack's too, at its repeats; a 2 by 2 picture has
        // no level 64 texels wide, so it is plain grey.
        assert_eq!(ground.broad.len(), 33);
        assert!(ground
            .broad
            .iter()
            .all(|(i, k)| i.width == BROAD_SIZE && *k == 2));
        assert_eq!(&ground.broad[1].0.levels[0][..4], &[128, 128, 128, 255]);
    }

    /// A pack's `[[skin]]` opens with its mesh, skeleton and clips; its slots find their clips or
    /// their fallbacks'; a pack over another keeps its own bodies for a setup and takes the
    /// base's for the rest; and `[keep]` comes through.
    #[test]
    fn a_pack_opens_its_bodies_and_stacks_them() {
        let (json, bin) = skin::write_gltf(&skin::sample_body(0.5, [1.0; 4]), "body.bin");
        let manifest = |setup: &str, extra: &str| {
            format!(
                "[pack]\nname = \"Bodies\"\nformat = 1\n{extra}\n[[skin]]\nsetup = \"{setup}\"\n\
                 file = \"b/body.gltf\"\nclips = {{ walk = \"Sway\", melee_med = \"Nod\" }}\n\
                 author = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n"
            )
        };
        let mut read = |p: &str| match p {
            "b/body.gltf" => Ok(json.as_bytes().to_vec()),
            "b/body.bin" => Ok(bin.clone()),
            other => Err(format!("no {other}")),
        };
        let top = ArtPack::open(&manifest("0x02000001", "[keep]\nweenies = [7]"), &mut read)
            .expect("opens");
        assert!(top.problems.is_empty(), "{:?}", top.problems);
        let body = top
            .skin_for(DataId(0x0200_0001), || None, &[])
            .expect("a body");
        assert_eq!(body.model.rig.parts(), [0]);
        assert_eq!(
            body.clip_for(skin::Slot::Run),
            body.clip_for(skin::Slot::Walk)
        );
        assert_eq!(body.clip_for(skin::Slot::MeleeHigh), Some(1));
        assert_eq!(body.clip_for(skin::Slot::Idle), None);
        let base = ArtPack::open(&manifest("0x02000002", ""), &mut read).expect("opens");
        let both = top.over(&base);
        assert_eq!(both.skins.len(), 2);
        assert!(both.skin_for(DataId(0x0200_0002), || None, &[]).is_some());
        assert_eq!(both.keep, [7]);
        let same = top.over(&ArtPack::open(&manifest("0x02000001", ""), &mut read).unwrap());
        assert_eq!(same.skins.len(), 1, "the top's body wins for its setup");
    }
}
