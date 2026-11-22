//! The art pack in the engine: opened by `pyreal-art-pack`, the reader the
//! client's presentation uses, and turned into what the engine draws.
//!
//! - **Materials**: a pack material names the retail pictures it stands for; a surface drawn
//!   with one of them takes the material's maps ([`PackOverrides`]), each optional, and keeps
//!   the dat's for any it lacks. A material's surface map packs roughness, occlusion and height
//!   in red, green and blue ([`flags::PACKED_SURFACE`](crate::asset::material::flags)); its kind
//!   (for its metal) comes from its grit ([`material_kind`]), and its metal map says where it is
//!   a metal ([`flags::HAS_METAL`](crate::asset::material::flags)).
//! - **Models**: a pack model is drawn in place of the retail object of the same id (alongside it
//!   for the few that lie over one, [`lays_over`]), its levels of detail linked as a degrade chain
//!   at the pack's distances.
//! - **Ground**: a pack ground material's picture is laid for the terrain types it covers, at its
//!   own size in metres; its normal and surface, and a steep material's picture, go with it.
//! - **The rest** is the open pack's ([`EnginePack::pack`]): its lights ([`crate::lamps`]),
//!   moons ([`crate::heavens`]) and ground cover ([`crate::cover`]) are drawn.
//!
//! No pack draws from the dats alone, and every surface still looks as retail draws it.

use std::collections::HashMap;
use std::sync::Arc;

use pyreal_art_pack::images::{BlockFormat, Image};
pub use pyreal_art_pack::{lays_over, ArtPack, MaterialPart, PackLod, PackMaterial, PackModel};
use pyreal_assets::art_pack::{DrawsOn, Fingerprint, Ground};
use pyreal_primitives::DataId;

use crate::asset::material::{flags, MaterialOverrides, PackMaps, SurfaceKind, TextureKey};
use crate::asset::mesh::{MeshData, Sides, SubMesh, Vertex};
use crate::asset::texture::{PixelLayout, TexturePixels};

/// The side of the normal and surface pictures a pack material without them is given: flat
/// defaults, which the engine treats as missing.
const FLAT_SIDE: u32 = 4;

/// An open art pack, indexed for the engine.
#[derive(Debug)]
pub struct EnginePack {
    pub pack: Arc<ArtPack>,
    // ORDER-OK: looked up by retail id, never iterated.
    models: HashMap<u32, usize>,
}

impl EnginePack {
    #[must_use]
    pub fn new(pack: Arc<ArtPack>) -> Self {
        let mut models = HashMap::new();
        for (i, m) in pack.models.iter().enumerate() {
            models.entry(m.id.0).or_insert(i);
        }
        Self { pack, models }
    }

    /// Opens the pack at `path` (a folder, or the file it was packed into).
    ///
    /// # Errors
    /// The pack will not open.
    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        Ok(Self::new(Arc::new(ArtPack::open_path(path)?)))
    }

    /// Opens the pack at `path` drawn over `base` ([`ArtPack::over`]), or alone without one.
    ///
    /// # Errors
    /// The pack will not open.
    pub fn open_over(path: &std::path::Path, base: Option<&ArtPack>) -> Result<Self, String> {
        let pack = ArtPack::open_path(path)?;
        Ok(Self::new(Arc::new(match base {
            Some(base) => pack.over(base),
            None => pack,
        })))
    }

    /// The pack's first model for retail object `id`.
    #[must_use]
    pub fn model(&self, id: DataId) -> Option<&PackModel> {
        self.models.get(&id.0).map(|&i| &self.pack.models[i])
    }

    /// The pack's model drawn in place of retail object `id`, whose fingerprint `record` gives
    /// when asked ([`ArtPack::model_for`]).
    #[must_use]
    pub fn model_for(
        &self,
        id: DataId,
        record: impl FnOnce() -> Option<Fingerprint>,
    ) -> Option<&PackModel> {
        if !self.models.contains_key(&id.0) {
            return None;
        }
        self.pack
            .model_for(id, record)
            .map(|i| &self.pack.models[i])
    }

    /// The pixels of the picture `key` names, or `None` when it names none of the pack's.
    #[must_use]
    pub fn pixels(&self, key: &str) -> Option<TexturePixels> {
        let (what, rest) = key.split_once(':')?;
        let (index, part) = rest.split_once(':').unwrap_or((rest, ""));
        let i: usize = index.parse().ok()?;
        let p = &self.pack;
        let material = match (what, part) {
            ("material", "albedo") => Some((MaterialPart::Albedo, true)),
            ("material", "normal") => Some((MaterialPart::Normal, false)),
            ("material", "surface") => Some((MaterialPart::Surface, false)),
            ("material", "metal") => Some((MaterialPart::Metal, false)),
            _ => None,
        };
        if let Some((part, srgb)) = material {
            return image_pixels(&p.material_picture(i, part)?, srgb);
        }
        let (image, srgb) = match (what, part) {
            ("image", "colour") => {
                let image = p.images.get(i)?;
                if matted(image) {
                    return filled_pixels(image);
                }
                (image, true)
            }
            ("image", _) => (p.images.get(i)?, false),
            ("ground", "albedo") => (&p.grounds.get(i)?.albedo, true),
            ("ground", "normal") => (&p.grounds.get(i)?.normal, false),
            ("ground", "surface") => (&p.grounds.get(i)?.surface, false),
            ("ground", "detail") => (p.grounds.get(i)?.detail.as_ref()?, false),
            _ => return None,
        };
        image_pixels(image, srgb)
    }

    /// The pack's ground material for `ground` (flat or `steep`), by index: the first drawn on
    /// `record`, the fingerprint of the picture the world's region lays that ground with.
    #[must_use]
    pub fn ground_index(
        &self,
        ground: Ground,
        steep: bool,
        record: Option<&Fingerprint>,
    ) -> Option<usize> {
        self.pack.grounds.iter().position(|g| {
            g.steep == steep
                && g.grounds.contains(&ground)
                && DrawsOn::Matching.draws(g.fingerprint.as_ref(), record)
        })
    }
}

/// A pack picture as the engine uploads it; `srgb` reads colour as colour.
#[must_use]
pub fn image_pixels(image: &Image, srgb: bool) -> Option<TexturePixels> {
    let layout = match image.format {
        None => PixelLayout::Rgba8 { srgb },
        Some(BlockFormat::Bc4) => PixelLayout::Bc4,
        Some(BlockFormat::Bc5) => PixelLayout::Bc5,
        Some(BlockFormat::Bc7) => PixelLayout::Bc7 { srgb },
    };
    if image.levels.is_empty() || image.width == 0 || image.height == 0 {
        return None;
    }
    Some(TexturePixels {
        width: image.width,
        height: image.height,
        layout,
        levels: image.levels.iter().map(|l| l.as_ref().clone()).collect(),
    })
}

/// A level's texels as red, green, blue, alpha bytes.
fn decoded(image: &Image, k: usize) -> Option<(u32, u32, Vec<u8>)> {
    let (w, h) = ((image.width >> k).max(1), (image.height >> k).max(1));
    let level = image.levels.get(k)?;
    let rgba = match image.format {
        Some(f) => pyreal_codecs::block_codec::decode(f, w, h, level),
        None => level.to_vec(),
    };
    (rgba.len() >= (w * h * 4) as usize).then_some((w, h, rgba))
}

/// Whether a cut-out colour picture's cut-away texels hold a colour of their own, unlike its
/// leaves' (a sheet of leaves on a pale ground): filtering mixes that colour into each edge as a
/// rim that shimmers as the plant sways. Read from its first level no wider than 256.
fn matted(image: &Image) -> bool {
    let Some(k) = image
        .levels
        .iter()
        .enumerate()
        .position(|(k, _)| (image.width >> k) <= 256 || k + 1 == image.levels.len())
    else {
        return false;
    };
    let Some((_, _, rgba)) = decoded(image, k) else {
        return false;
    };
    let texels = rgba.as_chunks::<4>().0;
    let cut = texels.iter().filter(|t| t[3] < 8).count();
    if cut * 50 < texels.len() || cut == texels.len() {
        return false;
    }
    let most = texels.iter().map(|t| t[3]).max().unwrap_or(0);
    let mean = |keep: &dyn Fn(u8) -> bool| {
        crate::asset::texture::mean_linear(
            texels
                .iter()
                .map(|t| [t[0], t[1], t[2], u8::from(keep(t[3])) * 255]),
            255,
        )
    };
    let (Some(ground), Some(leaves)) = (mean(&|a| a < 8), mean(&|a| a >= most / 2 + most / 4))
    else {
        return false;
    };
    ground
        .iter()
        .zip(&leaves)
        .any(|(g, l)| (g - l).abs() > 0.06)
}

/// A matted picture ([`matted`]) as RGBA8, each level's edge and cut-away texels coloured as its
/// nearest leaves (pushed down and pulled back up its own pyramid), its alpha kept.
fn filled_pixels(image: &Image) -> Option<TexturePixels> {
    let mut levels = Vec::with_capacity(image.levels.len());
    for k in 0..image.levels.len() {
        let (w, h, mut rgba) = decoded(image, k)?;
        rgba.truncate((w * h * 4) as usize);
        fill_matte(&mut rgba, w as usize, h as usize);
        levels.push(rgba);
    }
    Some(TexturePixels {
        width: image.width,
        height: image.height,
        layout: PixelLayout::Rgba8 { srgb: true },
        levels,
    })
}

/// Colours every texel of `rgba` (`w` by `h`) not well inside a leaf (alpha under three quarters
/// of the level's most) as the leaves about it: a push-pull fill, the known colours summed down a
/// pyramid by halves and each unknown texel taking the first level over it that knows any.
fn fill_matte(rgba: &mut [u8], w: usize, h: usize) {
    let most = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|t| t[3])
        .max()
        .unwrap_or(0);
    if most < 8 {
        return;
    }
    let inside = u16::from(most) * 3 / 4;
    let mut pyramid: Vec<(usize, usize, Vec<[f32; 4]>)> = Vec::new();
    let base: Vec<[f32; 4]> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|t| {
            if u16::from(t[3]) >= inside {
                [f32::from(t[0]), f32::from(t[1]), f32::from(t[2]), 1.0]
            } else {
                [0.0; 4]
            }
        })
        .collect();
    pyramid.push((w, h, base));
    while let Some((pw, ph, prev)) = pyramid.last().filter(|(pw, ph, _)| *pw > 1 || *ph > 1) {
        let (nw, nh) = (pw.div_ceil(2), ph.div_ceil(2));
        let mut next = vec![[0.0f32; 4]; nw * nh];
        for y in 0..*ph {
            for x in 0..*pw {
                let s = prev[y * pw + x];
                let d = &mut next[(y / 2) * nw + x / 2];
                for (a, b) in d.iter_mut().zip(s) {
                    *a += b;
                }
            }
        }
        pyramid.push((nw, nh, next));
    }
    // Each level's texels that know nothing take their parent's colour, from the top down.
    let mut above: Option<(usize, Vec<[f32; 3]>)> = None;
    for (lw, _, level) in pyramid.iter().rev() {
        let resolved: Vec<[f32; 3]> = level
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if c[3] > 0.0 {
                    [c[0] / c[3], c[1] / c[3], c[2] / c[3]]
                } else if let Some((uw, up)) = &above {
                    up[(i / lw / 2) * uw + (i % lw) / 2]
                } else {
                    [128.0; 3]
                }
            })
            .collect();
        above = Some((*lw, resolved));
    }
    let Some((_, colours)) = above else { return };
    for (t, c) in rgba.as_chunks_mut::<4>().0.iter_mut().zip(colours) {
        if u16::from(t[3]) < inside {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            for (b, v) in t.iter_mut().zip(c) {
                *b = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// A colour picture's mean in linear light, of its texels whose alpha is at least `cut`, read from
/// its first level no wider than 256.
#[must_use]
pub fn image_mean(image: &Image, cut: u8) -> Option<[f32; 3]> {
    let k = image
        .levels
        .iter()
        .enumerate()
        .position(|(k, _)| (image.width >> k) <= 256 || k + 1 == image.levels.len())?;
    let (w, h) = ((image.width >> k).max(1), (image.height >> k).max(1));
    let level = image.levels.get(k)?;
    let rgba = match image.format {
        Some(f) => pyreal_codecs::block_codec::decode(f, w, h, level),
        None => level.to_vec(),
    };
    crate::asset::texture::mean_linear(rgba.as_chunks::<4>().0.iter().copied(), cut)
}

/// Whether a material's map is a real one rather than the flat default it was given.
fn real(image: &Image) -> bool {
    image.width > FLAT_SIDE || image.height > FLAT_SIDE
}

/// What a pack material for a retail picture is made of, for the metal its surface map does not
/// hold: by its sheen (cloth), else by the grit laid over it (`grit/metal.png` and so on).
#[must_use]
pub fn material_kind(pack: &ArtPack, material: usize) -> Option<SurfaceKind> {
    let m = pack.materials.get(material)?;
    if m.sheen > 0.0 {
        return Some(SurfaceKind::Cloth);
    }
    let grit = pack.grit_names.get(m.grit?)?;
    let name = grit.rsplit('/').next().unwrap_or(grit);
    match name.split('.').next().unwrap_or(name) {
        "metal" => Some(SurfaceKind::Metal),
        "rock" | "stone" | "clay" | "plaster" => Some(SurfaceKind::Stone),
        "fibre" => Some(SurfaceKind::Wood),
        "lava" | "magma" => Some(SurfaceKind::Molten),
        _ => None,
    }
}

/// The art pack's maps for retail surfaces: a surface whose picture a pack material stands for
/// takes that material's base colour, normal and surface maps, each only where the pack has it.
#[derive(Debug, Clone)]
pub struct PackOverrides(pub Arc<ArtPack>);

impl MaterialOverrides for PackOverrides {
    fn maps(&self, surface: DataId, texture: Option<DataId>) -> PackMaps {
        self.maps_for(surface, texture, &[], &|_| None)
    }

    fn maps_for(
        &self,
        _surface: DataId,
        texture: Option<DataId>,
        palettes: &[DataId],
        record: &dyn Fn(DataId) -> Option<Fingerprint>,
    ) -> PackMaps {
        let Some(i) = texture.and_then(|t| self.0.material_for(t, palettes, || record(t))) else {
            return PackMaps::default();
        };
        let m = &self.0.materials[i];
        let key = |part: &str| Some(TextureKey::Pack(format!("material:{i}:{part}")));
        let surface = real(&m.surface).then(|| key("surface")).flatten();
        PackMaps {
            base: m.albedo.as_ref().and_then(|_| key("albedo")),
            normal: real(&m.normal).then(|| key("normal")).flatten(),
            height: surface.clone(),
            roughness_metal: surface.clone(),
            occlusion: surface.clone(),
            metal: m.metal.as_ref().and_then(|_| key("metal")),
            kind: material_kind(&self.0, i),
            packed_surface: surface.is_some(),
            ice: m.ice,
            matter: m.matter,
            projected: None,
        }
    }

    fn solid_maps(&self, surface: DataId, colour: u32) -> PackMaps {
        let Some((i, size)) = self.0.solid_for(surface, colour) else {
            return PackMaps::default();
        };
        let m = &self.0.materials[i];
        let key = |part: &str| Some(TextureKey::Pack(format!("material:{i}:{part}")));
        let surface = real(&m.surface).then(|| key("surface")).flatten();
        PackMaps {
            base: m.albedo.as_ref().and_then(|_| key("albedo")),
            normal: real(&m.normal).then(|| key("normal")).flatten(),
            // Laid along the world's axes, it has no depth to map.
            height: None,
            roughness_metal: surface.clone(),
            occlusion: surface.clone(),
            metal: None,
            kind: material_kind(&self.0, i),
            packed_surface: surface.is_some(),
            ice: false,
            matter: m.matter,
            projected: Some(size),
        }
    }
}

/// The trunk each tree of `pack` drawn on any world stands on, by retail object id, for collision
/// with what is drawn ([`pyreal_world_data::env_cells::set_drawn_trunks`]): in each of
/// [`pyreal_world_data::env_cells::TRUNK_BANDS`], the middle and median radius of its nearest
/// level's solid parts where a plane through the middle of the band cuts them, so a leaning or
/// tapering trunk is followed. A band the plane finds nothing in takes the band under it; a model
/// with nothing solid in its lowest band (a bush, a flower) has none and keeps retail's collision.
/// Measured once, when the pack is put in use; four short cylinders a tree, so collision stays cheap.
#[must_use]
pub fn drawn_trunks(
    pack: &ArtPack,
) -> std::collections::BTreeMap<u32, pyreal_world_data::env_cells::DrawnTrunk> {
    use pyreal_world_data::env_cells::{DrawnTrunk, TRUNK_BANDS};
    let mut out = std::collections::BTreeMap::new();
    for model in &pack.models {
        if model.draws_on != DrawsOn::Any {
            continue;
        }
        let Some(lod) = model
            .lods
            .iter()
            .find(|l| l.views.is_none() && !l.shadows_only)
        else {
            continue;
        };
        let solid: Vec<&pyreal_art_pack::PackPrimitive> = lod
            .primitives
            .iter()
            .filter(|p| p.material.alpha_cutoff.is_none() && p.material.base_color[3] >= 1.0)
            .collect();
        let mut bands = [(0.0, 0.0, 0.0); 4];
        for (k, &(b0, b1)) in TRUNK_BANDS.iter().enumerate() {
            match trunk_ring(&solid, (b0 + b1) / 2.0) {
                Some(r) => bands[k] = r,
                None if k > 0 => bands[k] = bands[k - 1],
                None => {}
            }
        }
        if bands[0].2 > 0.0 {
            out.insert(model.id.0, DrawnTrunk { bands });
        }
    }
    out
}

/// Where a plane at height `h` cuts `solid`'s triangles: the cut's middle and median radius, or
/// `None` for a cut too small to be a trunk.
fn trunk_ring(solid: &[&pyreal_art_pack::PackPrimitive], h: f32) -> Option<(f32, f32, f32)> {
    use pyreal_art_pack::VERTEX_FLOATS;
    let mut cut = Vec::new();
    for p in solid {
        let v = p.vertices.as_chunks::<VERTEX_FLOATS>().0;
        for t in p.indices.as_chunks::<3>().0 {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let (Some(a), Some(b)) = (v.get(a as usize), v.get(b as usize)) else {
                    continue;
                };
                if (a[2] - h) * (b[2] - h) >= 0.0 {
                    continue;
                }
                let f = (h - a[2]) / (b[2] - a[2]);
                cut.push((a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f));
            }
        }
    }
    if cut.len() < 3 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let n = cut.len() as f32;
    let x = cut.iter().map(|c| c.0).sum::<f32>() / n;
    let y = cut.iter().map(|c| c.1).sum::<f32>() / n;
    let mut d: Vec<f32> = cut
        .iter()
        .map(|c| pyreal_primitives::num::math::hypotf(c.0 - x, c.1 - y))
        .collect();
    d.sort_by(f32::total_cmp);
    let radius = d[d.len() / 2];
    (radius > 0.02).then_some((x, y, radius))
}

/// One level of a pack model as an engine mesh: its primitives' triangles, one sub-mesh each in
/// order, with positions, normals and texture coordinates (tangents and colours are not drawn).
#[must_use]
pub fn lod_mesh(lod: &PackLod) -> MeshData {
    use pyreal_art_pack::VERTEX_FLOATS;
    let mut mesh = MeshData::default();
    for (k, p) in lod.primitives.iter().enumerate() {
        let base = u32::try_from(mesh.vertices.len()).unwrap_or(0);
        for v in p.vertices.as_chunks::<VERTEX_FLOATS>().0 {
            mesh.vertices.push(Vertex {
                position: [v[0], v[1], v[2]],
                normal: [v[3], v[4], v[5]],
                uv: [v[10], v[11]],
                extra: [0, 0],
            });
        }
        let first = u32::try_from(mesh.indices.len()).unwrap_or(0);
        mesh.indices.extend(p.indices.iter().map(|i| i + base));
        mesh.submeshes.push(SubMesh {
            surface_index: u16::try_from(k).unwrap_or(u16::MAX),
            surface: None,
            sides: if p.material.double_sided {
                Sides::Both
            } else {
                Sides::One
            },
            first_index: first,
            index_count: u32::try_from(p.indices.len()).unwrap_or(0),
        });
    }
    mesh.fit_bounds();
    mesh
}

/// What a pack model's material asks of the engine's material record: the flags and the keys of
/// its pictures. `foliage` marks a cut-out material of growth.
#[must_use]
pub fn model_material(m: &PackMaterial, foliage: bool) -> (u32, [Option<TextureKey>; 4]) {
    let image = |i: Option<u32>, colour: bool| {
        i.map(|i| {
            TextureKey::Pack(format!(
                "image:{i}:{}",
                if colour { "colour" } else { "data" }
            ))
        })
    };
    let mut f = 0;
    let keys = [
        image(m.base_color_image, true),
        image(m.normal_image, false),
        image(m.metal_rough_image, false),
        image(m.occlusion_image, false),
    ];
    for (k, bit) in keys.iter().zip([
        flags::HAS_BASE,
        flags::HAS_NORMAL,
        flags::HAS_ROUGH_METAL,
        flags::HAS_OCCLUSION,
    ]) {
        if k.is_some() {
            f |= bit;
        }
    }
    if m.alpha_cutoff.is_some() {
        f |= flags::CUTOUT;
        if foliage {
            f |= flags::FOLIAGE;
        }
    }
    if m.double_sided {
        f |= flags::TWO_SIDED;
    }
    (f, keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyreal_art_pack::{PackLod, PackMaterial, PackPrimitive, VERTEX_FLOATS};

    /// A twelve-sided trunk 3 m tall leaning `lean` metres east a metre, `r` round, as one level.
    fn leaning_trunk(lean: f32, r: f32) -> PackModel {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let sides = 12u32;
        for ring in 0..=6u32 {
            #[allow(clippy::cast_precision_loss)]
            let z = ring as f32 * 0.5;
            for s in 0..sides {
                #[allow(clippy::cast_precision_loss)]
                let a = s as f32 / sides as f32 * std::f32::consts::TAU;
                let mut v = [0.0; VERTEX_FLOATS];
                v[0] = lean * z + r * pyreal_primitives::num::math::cosf(a);
                v[1] = r * pyreal_primitives::num::math::sinf(a);
                v[2] = z;
                vertices.extend_from_slice(&v);
            }
        }
        for ring in 0..6u32 {
            for s in 0..sides {
                let (a, b) = (ring * sides + s, ring * sides + (s + 1) % sides);
                indices.extend_from_slice(&[a, b, a + sides, b, b + sides, a + sides]);
            }
        }
        PackModel {
            id: DataId(0x0200_071F),
            foliage: true,
            wind: 0.0,
            lods: vec![PackLod {
                distance: f32::INFINITY,
                primitives: vec![PackPrimitive {
                    vertices: Arc::new(vertices),
                    indices: Arc::new(indices),
                    material: PackMaterial {
                        base_color: [1.0; 4],
                        base_color_image: None,
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
            bounds: ([0.0, 0.0, 1.5], 3.0),
            fingerprint: None,
            draws_on: DrawsOn::Any,
            credit: pyreal_assets::art_pack::Credit::default(),
        }
    }

    #[test]
    fn a_leaning_trunk_is_measured_in_bands_that_follow_it() {
        let solid: Vec<&PackPrimitive> = Vec::new();
        assert!(trunk_ring(&solid, 0.9).is_none());
        let model = leaning_trunk(0.4, 0.5);
        let prims: Vec<&PackPrimitive> = model.lods[0].primitives.iter().collect();
        let low = trunk_ring(&prims, 0.3).expect("a ring at the foot");
        let high = trunk_ring(&prims, 2.1).expect("a ring higher up");
        assert!(
            (low.0 - 0.12).abs() < 0.05 && (high.0 - 0.84).abs() < 0.05,
            "{low:?} {high:?}"
        );
        assert!(
            (low.2 - 0.5).abs() < 0.05 && (high.2 - 0.5).abs() < 0.05,
            "{low:?} {high:?}"
        );
    }
}
