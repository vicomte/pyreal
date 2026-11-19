//! The dats into the engine: textures, materials and meshes loaded once each and cached by id,
//! and an area of landblocks (ground, statics, buildings and their rooms, dungeons, and scenery)
//! placed around a centre block.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use glam::{Affine3A, Vec3};
pub use pyreal_assets::Region;
use pyreal_assets::{CellLandblock, Decode, GfxObj, LandblockInfo, Scene as SceneRecord, Surface};
use pyreal_codecs::surface::surface_type as st;
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::DataId;
use pyreal_terrain::consts::{CELL_SIZE, ROAD_TERRAIN_TYPE};
use pyreal_terrain::land::merge::get_terrain_tex;
use pyreal_terrain::land::mesh::height_table;
use pyreal_terrain::land::water::drawn_words;
use pyreal_terrain::scenery::{generate_scenery, outside_cell_index, SceneryEnv, WithinBlockShape};
use pyreal_world_data::env_cells::{DecodedCell, EnvCellLoader};
use pyreal_world_data::landblock::{landblock_did, lbi_did};

use crate::asset::material::{
    describe_surface, flags, KindTable, MaterialDesc, MaterialOverrides, MaterialRecord,
    NoOverrides, PackMaps, SurfaceKind, TextureKey,
};
use crate::asset::mesh::cell_mesh;
use crate::asset::mesh::{gfxobj_mesh, Sides};
use crate::asset::object::{degrade_chain, frame_affine, near_mode, object_affine, object_parts};
use crate::asset::texture::{PaletteSwap, TexturePixels, TextureSource};
use crate::cells::{Cell, Portal, Volume, OUTSIDE};
use crate::pack::{EnginePack, PackOverrides};
use crate::scene::{Instance, MeshId, PipelineKind};
use crate::terrain::{build_block, GroundTextures};
use crate::Engine;
use pyreal_assets::fingerprint::Fingerprint;

/// A material by surface, sidedness, palette variant, the picture put in place of its own and
/// whether it is a shadow's smoke.
type MaterialKey = (u32, bool, Option<PaletteSwap>, Option<u32>, bool);

/// How a part is dressed: pictures put in place of others (an object description's texture
/// changes, by original texture), the palette its palette-indexed pictures are drawn in, and
/// whether it is drawn as a shadow's smoke.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Dress {
    pub textures: Vec<(DataId, DataId)>,
    pub palette: Option<PaletteSwap>,
    pub smoke: bool,
}

impl Dress {
    /// A part's surface overrides as the animation code left them: its texture changes, and its
    /// shift palette with its sub-palette ranges (in the wire's units).
    #[must_use]
    pub fn of(overrides: Option<&pyreal_animation::parts::SurfaceOverrides>) -> Self {
        let Some(o) = overrides else {
            return Self::default();
        };
        let palette = o.shift_palette.map(|base| PaletteSwap {
            base,
            ranges: o
                .subpalettes
                .iter()
                .map(|r| (r.palette_set, r.offset, r.length))
                .collect(),
            wire: true,
        });
        Self {
            textures: o.texture_maps.clone(),
            palette,
            smoke: false,
        }
    }

    #[must_use]
    pub fn is_plain(&self) -> bool {
        self.textures.is_empty() && self.palette.is_none() && !self.smoke
    }
}

/// The palettes a picture is drawn in, which a pack material may be for alone: those a palette
/// swap's ranges lay over it, else the swap's base, else the picture's own.
fn drawn_in(
    store: &RetailDatStore,
    swap: Option<&PaletteSwap>,
    texture: Option<DataId>,
) -> Vec<DataId> {
    match swap {
        Some(s) if !s.ranges.is_empty() => s.ranges.iter().map(|r| r.0).collect(),
        Some(s) => vec![s.base],
        None => texture
            .and_then(|t| TextureSource::new(store).own_palette(t))
            .into_iter()
            .collect(),
    }
}

/// What the loader has done, for the viewer's report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoadStats {
    pub textures: usize,
    pub texture_failures: usize,
    pub materials: usize,
    pub meshes: usize,
    pub empty_meshes: usize,
    pub blocks: usize,
    pub statics: usize,
    pub buildings: usize,
    pub scenery: usize,
    pub instances: usize,
    /// Rooms placed, their furniture, and cells promised that would not load.
    pub cells: usize,
    pub cell_statics: usize,
    pub cell_failures: u64,
    /// Terrain cells with at least one room volume over them.
    pub cut_cells: usize,
    /// Objects drawn as the art pack's models, and their levels' meshes.
    pub pack_models: usize,
    pub pack_meshes: usize,
    /// Terrain types laid with the pack's ground.
    pub pack_grounds: usize,
    /// Retail surfaces drawn with one of the pack's materials.
    pub pack_surfaces: usize,
    /// Retail pictures and objects the pack has entries for, none made from this world's record
    /// of them (`docs/ERAS.md`), drawn as the world's files hold them.
    pub pack_unmatched: usize,
}

/// The caches between the dats and the engine: each texture, material and mesh is loaded once.
pub struct AssetLoader {
    pub kinds: KindTable,
    overrides: Box<dyn MaterialOverrides>,
    /// The art pack, when one is drawn.
    pack: Option<Arc<EnginePack>>,
    /// Each pack model's near level, by retail id, and how the wind sways it.
    pack_meshes: HashMap<u32, Option<(MeshId, u32)>>,
    /// Each pack body's materials, by its place in the pack, made once for every object.
    // ORDER-OK: looked up by place.
    skin_draws: HashMap<usize, Vec<Option<(u32, PipelineKind)>>>,
    /// The fingerprint of each world picture and object the pack was asked about, by id, and of
    /// each end-of-retail ground picture an older world lays ([`LaterGround`]) apart from those.
    prints: Mutex<HashMap<(u32, bool), Option<Fingerprint>>>,
    /// End of retail's ground beside an older world, read once.
    later_ground: Option<Option<Arc<LaterGround>>>,
    /// Where a pack model's level of pictures takes over from its last level of shape: its
    /// shape's own end where it has one, else this many times where that level began; zero
    /// keeps the shape to the horizon.
    pub cards_from: f32,
    /// Each pack model's level of pictures, by retail id, for the far trees.
    pack_cards: HashMap<u32, Option<MeshId>>,
    /// Each terrain type's pack ground (picture slot, metres a repeat), when the pack has one.
    pack_grounds: HashMap<u32, Option<(u32, f32)>>,
    /// Each placed setup's basin water faces ([`crate::fountains::pool_faces`]) and where its
    /// streams meet them ([`crate::fountains::spills`]), read once.
    // ORDER-OK: looked up by setup, never iterated.
    pool_faces: HashMap<
        u32,
        (
            Vec<crate::fountains::PoolFace>,
            Vec<crate::fountains::Spill>,
        ),
    >,
    /// The forges and fire pits of the room whose objects are being placed, each where
    /// ([`crate::flames::firebox_of`]); none outdoors, where a body of fire shows faintly by day
    /// and darkens the night round it as the eye adjusts to it.
    fireboxes: Vec<(u32, Affine3A)>,
    /// Each terrain type's ground colour (linear) once its picture is laid, for the cover's tint.
    ground_colours: [Option<[f32; 3]>; crate::cover::GROUND_TYPES],
    // ORDER-OK: these caches are looked up by key and never iterated.
    textures: HashMap<TextureKey, Option<u32>>,
    materials: HashMap<MaterialKey, Option<(u32, u32)>>,
    meshes: HashMap<u32, Option<MeshId>>,
    /// Dressed variants of a graphics object's mesh.
    dressed: HashMap<(u32, Dress), Option<MeshId>>,
    /// A part's near mesh dressed: its pictures' mean linear luminance and their weight.
    lumas: HashMap<(u32, Dress), Option<(f32, f32)>>,
    /// Each loaded mesh's sub-meshes, which a variant draws with other materials.
    submeshes: HashMap<u32, Vec<crate::asset::mesh::SubMesh>>,
    /// Whether a picture is palette-indexed, which a palette variant can change.
    palettised: HashMap<u32, bool>,
    cell_meshes: HashMap<(u32, u16, Vec<u32>), Option<MeshId>>,
    /// Meshes whose degrade levels are linked.
    linked: std::collections::HashSet<u32>,
    /// The cut volumes written so far.
    cuts: Vec<Cut>,
    /// The static instances placed since this was last taken ([`Self::take_placed`]), so a
    /// block's can be taken out again.
    placed: Vec<u32>,
    /// Pictures decoded ahead by the land's planners.
    pub prefetch: Arc<Prefetch>,
    terrain_material: Option<u32>,
    next_object: u32,
    /// Each object id's lamps.
    pub lamps: crate::lamps::LampCache,
    /// The cobweb cards' pictures and silk.
    webs: crate::webs::Weaver,
    /// Each cobweb mesh's other webs, by graphics object and variant ([`Self::web_variant`]).
    // ORDER-OK: looked up by key, never iterated in an order that matters.
    web_variants: HashMap<(u32, u32), Option<MeshId>>,
    pub stats: LoadStats,
    /// Dressed variants made so far, which the stream frees after when over its budget.
    pub dressed_made: u64,
}

impl std::fmt::Debug for AssetLoader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssetLoader")
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

impl Default for AssetLoader {
    fn default() -> Self {
        Self::new(Box::new(NoOverrides))
    }
}

fn decode<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

/// Where a pack model's level of pictures takes over: this many times where its last level of
/// shape began.
pub const CARDS_FROM: f32 = 1.0;
/// The least distance a level of shape is taken to begin at, metres a metre of scale.
const CARD_LEAST: f32 = 60.0;

/// The pipeline a material draws with.
#[must_use]
pub fn pipeline_for(material_flags: u32, sides: Sides) -> PipelineKind {
    if material_flags & flags::ICE != 0 {
        PipelineKind::Ice
    } else if material_flags & flags::POOL != 0 {
        PipelineKind::Pool
    } else if material_flags & flags::ADDITIVE != 0 {
        PipelineKind::Additive
    } else if material_flags & flags::TRANSLUCENT != 0 {
        PipelineKind::Translucent
    } else if material_flags & flags::TERRAIN != 0 {
        PipelineKind::Terrain
    } else if sides == Sides::Both {
        PipelineKind::TwoSided
    } else {
        PipelineKind::Opaque
    }
}

impl AssetLoader {
    /// A loader whose materials take `overrides`' maps where it has them.
    #[must_use]
    pub fn new(overrides: Box<dyn MaterialOverrides>) -> Self {
        Self {
            kinds: KindTable::default(),
            overrides,
            pack: None,
            pack_meshes: HashMap::new(),
            skin_draws: HashMap::new(),
            cards_from: CARDS_FROM,
            pack_cards: HashMap::new(),
            prints: Mutex::new(HashMap::new()),
            later_ground: None,
            pack_grounds: HashMap::new(),
            pool_faces: HashMap::new(),
            fireboxes: Vec::new(),
            ground_colours: [None; crate::cover::GROUND_TYPES],
            textures: HashMap::new(),
            materials: HashMap::new(),
            meshes: HashMap::new(),
            dressed: HashMap::new(),
            lumas: HashMap::new(),
            submeshes: HashMap::new(),
            palettised: HashMap::new(),
            cell_meshes: HashMap::new(),
            linked: std::collections::HashSet::new(),
            cuts: Vec::new(),
            placed: Vec::new(),
            prefetch: Arc::new(Prefetch::default()),
            dressed_made: 0,
            terrain_material: None,
            next_object: 0,
            lamps: crate::lamps::LampCache::default(),
            webs: crate::webs::Weaver::default(),
            web_variants: HashMap::new(),
            stats: LoadStats::default(),
        }
    }

    /// A loader drawing `pack`: its materials' maps on the retail surfaces they stand for, its
    /// models in place of the retail objects, and its ground.
    #[must_use]
    pub fn with_pack(pack: EnginePack) -> Self {
        let pack = Arc::new(pack);
        let mut loader = Self::new(Box::new(PackOverrides(Arc::clone(&pack.pack))));
        loader.pack = Some(pack);
        loader
    }

    /// Forgets what was placed (the rooms' cut volumes, the counts), keeping every mesh, picture
    /// and material loaded: for [`Engine::clear_world`].
    pub fn clear_world(&mut self) {
        self.cuts.clear();
        self.placed.clear();
        self.lumas.clear();
        self.stats = LoadStats::default();
    }

    /// Forgets the meshes, materials and textures [`crate::evict`] freed, so they are loaded again
    /// when next asked for.
    pub fn forget(
        &mut self,
        meshes: &std::collections::HashSet<u32>,
        materials: &std::collections::HashSet<u32>,
        textures: &std::collections::HashSet<u32>,
    ) {
        let mesh_gone = |m: &Option<MeshId>| m.is_some_and(|m| meshes.contains(&m.0));
        self.meshes.retain(|_, m| !mesh_gone(m));
        self.dressed.retain(|_, m| !mesh_gone(m));
        // A freed part's dressed brightness is worked out again if it comes back.
        let meshes_kept = &self.meshes;
        self.lumas.retain(|(g, _), _| meshes_kept.contains_key(g));
        self.cell_meshes.retain(|_, m| !mesh_gone(m));
        self.pack_cards.retain(|_, m| !mesh_gone(m));
        self.web_variants.retain(|_, m| !mesh_gone(m));
        self.pack_meshes
            .retain(|_, m| !m.is_some_and(|(m, _)| meshes.contains(&m.0)));
        self.submeshes.retain(|m, _| !meshes.contains(m));
        self.linked.retain(|m| !meshes.contains(m));
        self.materials
            .retain(|_, m| !m.is_some_and(|(m, _)| materials.contains(&m)));
        self.webs.forget(materials);
        self.textures.retain(|k, t| {
            let gone = t.is_some_and(|t| textures.contains(&t));
            if let (
                true,
                TextureKey::Dat {
                    id,
                    clip_map,
                    swap: None,
                },
            ) = (gone, k)
            {
                self.prefetch.forget(*id, *clip_map);
            }
            !gone
        });
        self.pack_grounds
            .retain(|_, g| !g.is_some_and(|(t, _)| textures.contains(&t)));
    }

    /// The static instances placed since the last call.
    pub fn take_placed(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.placed)
    }

    /// The object number the next placement takes.
    #[must_use]
    pub fn object_mark(&self) -> u32 {
        self.next_object
    }

    /// Places a static instance, kept for [`Self::take_placed`].
    fn add_instance(&mut self, engine: &mut Engine, instance: Instance) -> bool {
        match engine.scene.add_instance(instance) {
            Ok(id) => {
                self.placed.push(id);
                true
            }
            Err(_) => false,
        }
    }

    /// [`Self::with_pack`] for a pack already opened and shared.
    #[must_use]
    pub fn with_shared_pack(pack: Arc<EnginePack>) -> Self {
        let mut loader = Self::new(Box::new(PackOverrides(Arc::clone(&pack.pack))));
        loader.pack = Some(pack);
        loader
    }

    /// Colour the ground as `coloring` says from the next land laid
    /// ([`crate::ground_tint::set_coloring`]), its cover's colours taken again; whether it changed.
    pub fn set_ground_coloring(&mut self, coloring: crate::ground_tint::GroundColoring) -> bool {
        if !crate::ground_tint::set_coloring(coloring) {
            return false;
        }
        self.ground_colours = [None; crate::cover::GROUND_TYPES];
        true
    }

    /// The art pack drawn, if any: its lights, moons and cover are read from here.
    #[must_use]
    pub fn pack(&self) -> Option<&EnginePack> {
        self.pack.as_deref()
    }

    /// The pack's body for an object built on `setup` (its record read from `store` for its
    /// fingerprint) whose look lays `palettes`, and its place in the pack.
    #[must_use]
    pub fn skin_for(
        &self,
        store: Option<&RetailDatStore>,
        setup: DataId,
        palettes: &[DataId],
    ) -> Option<(usize, &pyreal_art_pack::PackSkin)> {
        let pack = &self.pack.as_ref()?.pack;
        let skin = pack.skin_for(setup, || self.print_of(store, setup), palettes)?;
        let at = pack.skins.iter().position(|k| std::ptr::eq(k, skin))?;
        Some((at, skin))
    }

    /// Whether the pack keeps an object of weenie class `wcid` as it is, `launcher` when it has
    /// held a launcher.
    #[must_use]
    pub fn keeps(&self, wcid: u32, launcher: bool) -> bool {
        self.pack
            .as_ref()
            .is_some_and(|p| p.pack.keep.contains(&wcid) || (launcher && p.pack.keep_launchers))
    }

    /// A mesh of its own for one object's body, the pack's `skin`th, posed at rest with the
    /// pack's materials; pinned, so it stays until freed.
    pub fn skin_mesh(&mut self, engine: &mut Engine, skin: usize) -> Option<MeshId> {
        let pack = Arc::clone(&self.pack.as_ref()?.pack);
        let body = pack.skins.get(skin)?;
        let model = &body.model;
        let globals = model.rig.globals(&model.rig.rest(), &[]);
        let mut posed = Vec::new();
        model.pose(&model.rig.joint_matrices(&globals), &mut posed);
        let mut mesh = crate::skin::rest_mesh(model, &posed);
        if mesh.indices.is_empty() {
            return None;
        }
        // Room for the limbs as they move.
        mesh.radius *= crate::skin::REACH;
        if !self.skin_draws.contains_key(&skin) {
            let lod = crate::pack::PackLod {
                distance: f32::INFINITY,
                primitives: model
                    .primitives
                    .iter()
                    .zip(&body.materials)
                    .map(|(p, m)| pyreal_art_pack::PackPrimitive {
                        vertices: Arc::new(Vec::new()),
                        indices: Arc::new(p.indices.clone()),
                        material: *m,
                    })
                    .collect(),
                views: None,
                shadows_only: false,
            };
            let draws = self.pack_materials(engine, &lod, &mesh, false);
            self.skin_draws.insert(skin, draws);
        }
        let draws = self.skin_draws.get(&skin)?;
        let id = engine
            .scene
            .add_mesh(&engine.gpu, &mesh, |k| draws.get(k).copied().flatten())
            .ok()?;
        engine.scene.pin(id);
        Some(id)
    }

    /// The texture slot of the pack picture `key`, uploading it the first time.
    fn pack_texture(&mut self, engine: &mut Engine, key: &TextureKey) -> Option<u32> {
        if let Some(slot) = self.textures.get(key) {
            return *slot;
        }
        let slot = match key {
            TextureKey::Pack(name) => self
                .pack
                .as_ref()
                .and_then(|p| p.pixels(name))
                .and_then(|px| engine.materials.add_texture(&engine.gpu, &px).ok()),
            TextureKey::Dat { .. }
            | TextureKey::ClassicGround { .. }
            | TextureKey::LaterGround { .. } => None,
        };
        if slot.is_some() {
            self.stats.textures += 1;
        } else {
            self.stats.texture_failures += 1;
        }
        self.textures.insert(key.clone(), slot);
        slot
    }

    /// The texture slot of `key`, decoding and uploading it the first time. `None` when it will
    /// not decode (the material then drops that map).
    pub fn texture(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        key: &TextureKey,
    ) -> Option<u32> {
        if let Some(slot) = self.textures.get(key) {
            return *slot;
        }
        let slot = match key {
            TextureKey::Dat { id, clip_map, swap } => {
                let decoded = swap
                    .is_none()
                    .then(|| self.prefetch.take(*id, *clip_map))
                    .flatten();
                let pixels = decoded.or_else(|| {
                    let src = TextureSource::new(store);
                    let palette = swap.as_ref().and_then(|s| src.compose(s));
                    src.decode(*id, *clip_map, palette.as_ref())
                        .ok()
                        .map(Arc::new)
                });
                if swap.is_none() {
                    self.prefetch.note(*id, *clip_map);
                }
                pixels.and_then(|p| engine.materials.add_texture(&engine.gpu, &p).ok())
            }
            TextureKey::Pack(name) => self
                .pack
                .as_ref()
                .and_then(|p| p.pixels(name))
                .and_then(|px| engine.materials.add_texture(&engine.gpu, &px).ok()),
            TextureKey::ClassicGround { id, terrain_type } => crate::ground_tint::tint_for(
                crate::ground_tint::GroundColoring::Classic,
                *terrain_type,
            )
            .and_then(|tint| {
                TextureSource::new(store)
                    .decode_with(*id, |w, h, bgra| {
                        crate::ground_tint::apply(tint, w, h, bgra)
                    })
                    .ok()
            })
            .and_then(|px| engine.materials.add_texture(&engine.gpu, &px).ok()),
            TextureKey::LaterGround { id } => TextureSource::new(store)
                .decode(*id, false, None)
                .ok()
                .and_then(|px| engine.materials.add_texture(&engine.gpu, &px).ok()),
        };
        if slot.is_some() {
            self.stats.textures += 1;
        } else {
            self.stats.texture_failures += 1;
        }
        self.textures.insert(key.clone(), slot);
        slot
    }

    /// The material of `surface` drawn `two_sided` or not, and its flags.
    pub fn material(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        surface: DataId,
        two_sided: bool,
    ) -> Option<(u32, u32)> {
        self.material_variant(engine, store, surface, two_sided, None)
    }

    /// [`Self::material`] with its picture drawn in the palette `swap` describes (a clothing or
    /// creature tint), each variant made once and cached by the swap.
    pub fn material_variant(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        surface: DataId,
        two_sided: bool,
        swap: Option<PaletteSwap>,
    ) -> Option<(u32, u32)> {
        self.material_with(engine, store, surface, two_sided, swap, None, false)
    }

    /// [`Self::material`] dressed as `dress` says: the surface's original picture replaced where
    /// a texture change names it, and the palette applied where the picture it ends up with is
    /// palette-indexed (a DXT picture is the same in every palette, so it shares one material).
    pub fn material_dressed(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        surface: DataId,
        two_sided: bool,
        dress: &Dress,
    ) -> Option<(u32, u32)> {
        if dress.is_plain() {
            return self.material(engine, store, surface, two_sided);
        }
        let orig = self.surface_texture(store, surface);
        let texture = orig.and_then(|o| {
            dress
                .textures
                .iter()
                .find(|(old, _)| *old == o)
                .map(|(_, new)| *new)
        });
        let effective = texture.or(orig);
        let swap = dress
            .palette
            .clone()
            .filter(|_| effective.is_some_and(|t| self.is_palettised(store, t)));
        self.material_with(
            engine,
            store,
            surface,
            two_sided,
            swap,
            texture,
            dress.smoke,
        )
    }

    /// Part `gfxobj`'s near mesh dressed as `dress`: the mean linear luminance of what its
    /// opaque surfaces show and their triangles, worked out once a dress.
    pub fn dress_luma(
        &mut self,
        store: &RetailDatStore,
        gfxobj: DataId,
        dress: &Dress,
    ) -> Option<(f32, f32)> {
        let key = (gfxobj.0, dress.clone());
        if let Some(l) = self.lumas.get(&key) {
            return *l;
        }
        let made = self.measure_luma(store, gfxobj, dress);
        self.lumas.insert(key, made);
        made
    }

    fn measure_luma(
        &mut self,
        store: &RetailDatStore,
        gfxobj: DataId,
        dress: &Dress,
    ) -> Option<(f32, f32)> {
        let near = crate::asset::object::near_mesh(store, gfxobj)?;
        let obj = decode::<GfxObj>(store, DbType::GfxObj, near)?;
        let mesh = gfxobj_mesh(&obj);
        let src = TextureSource::new(store);
        let (mut sum, mut weight) = (0.0f32, 0.0f32);
        for s in &mesh.submeshes {
            let Some(rec) = s
                .surface
                .and_then(|f| decode::<Surface>(store, DbType::Surface, f))
            else {
                continue;
            };
            let ty = rec.surface_type;
            if ty & (st::TRANSLUCENT | st::ADDITIVE | st::ALPHA | st::INVALPHA) != 0 {
                continue;
            }
            let rgb = if ty & (st::BASE1_IMAGE | st::BASE1_CLIPMAP) != 0 {
                let Some(orig) = rec.orig_texture_id else {
                    continue;
                };
                let texture = dress
                    .textures
                    .iter()
                    .find(|(old, _)| *old == orig)
                    .map_or(orig, |(_, new)| *new);
                let palette = dress
                    .palette
                    .as_ref()
                    .filter(|_| self.is_palettised(store, texture))
                    .and_then(|p| src.compose(p));
                let clip = ty & st::BASE1_CLIPMAP != 0;
                src.average_dressed(texture, clip, palette.as_ref()).ok()
            } else {
                rec.color_value.map(|argb| {
                    let [b, g, r, _] = argb.to_le_bytes();
                    [r, g, b].map(crate::asset::material::srgb_to_linear)
                })
            };
            let Some([r, g, b]) = rgb else { continue };
            #[allow(clippy::cast_precision_loss)]
            let w = (s.index_count / 3).max(1) as f32;
            sum += (0.2126 * r + 0.7152 * g + 0.0722 * b) * w;
            weight += w;
        }
        (weight > 0.0).then(|| (sum / weight, weight))
    }

    fn surface_texture(&self, store: &RetailDatStore, surface: DataId) -> Option<DataId> {
        decode::<Surface>(store, DbType::Surface, surface).and_then(|s| s.orig_texture_id)
    }

    fn is_palettised(&mut self, store: &RetailDatStore, texture: DataId) -> bool {
        *self
            .palettised
            .entry(texture.0)
            .or_insert_with(|| TextureSource::new(store).is_palettised(texture))
    }

    /// The fingerprint of the world's picture or object `id`, worked out once; with no `store`,
    /// only one already worked out.
    pub(crate) fn print_of(
        &self,
        store: Option<&RetailDatStore>,
        id: DataId,
    ) -> Option<Fingerprint> {
        self.print_in(store, id, false)
    }

    /// [`Self::print_of`] for a picture of `store`, which is end of retail's files beside an older
    /// world when `later` ([`LaterGround`]): those hold other pictures under the world's ids.
    fn print_in(
        &self,
        store: Option<&RetailDatStore>,
        id: DataId,
        later: bool,
    ) -> Option<Fingerprint> {
        let mut prints = self
            .prints
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(p) = prints.get(&(id.0, later)) {
            return *p;
        }
        let store = store?;
        let print = pyreal_art_pack::fingerprint::of_record(store, id);
        prints.insert((id.0, later), print);
        print
    }

    /// End of retail's ground beside the older world `store`, read the first time it is asked for.
    fn later_ground(&mut self, store: &RetailDatStore) -> Option<Arc<LaterGround>> {
        self.later_ground
            .get_or_insert_with(|| LaterGround::of(store).map(Arc::new))
            .clone()
    }

    /// Whether the pack has entries for picture `texture` and none drew on `maps`.
    fn unmatched(&self, texture: Option<DataId>, maps: &PackMaps) -> bool {
        *maps == PackMaps::default()
            && texture.is_some_and(|t| {
                self.pack
                    .as_ref()
                    .is_some_and(|p| p.pack.material_of(t).is_some())
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn material_with(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        surface: DataId,
        two_sided: bool,
        swap: Option<PaletteSwap>,
        texture: Option<DataId>,
        smoke: bool,
    ) -> Option<(u32, u32)> {
        let key = (surface.0, two_sided, swap, texture.map(|t| t.0), smoke);
        if let Some(m) = self.materials.get(&key) {
            return *m;
        }
        let made = decode::<Surface>(store, DbType::Surface, surface).and_then(|mut s| {
            if texture.is_some() {
                s.orig_texture_id = texture;
            }
            let palettes = drawn_in(store, key.2.as_ref(), s.orig_texture_id);
            let mut pack = self
                .overrides
                .maps_for(surface, s.orig_texture_id, &palettes, &|t| {
                    self.print_of(Some(store), t)
                });
            if self.unmatched(s.orig_texture_id, &pack) {
                self.stats.pack_unmatched += 1;
            }
            if crate::asset::material::molten_dress(s.orig_texture_id, &palettes, &|t| {
                self.print_of(Some(store), t)
            }) {
                pack.kind = Some(SurfaceKind::Molten);
            }
            if key.2.is_some() {
                // A tinted picture is the dat's; the pack's picture would lose the tint.
                pack.base = None;
            }
            if pack != PackMaps::default() {
                self.stats.pack_surfaces += 1;
            }
            let mut desc = describe_surface(&s, two_sided, key.2.clone(), &self.kinds, pack);
            // A shadow's smoke is drawn sorted with the translucent; what already blends stays.
            if smoke && desc.flags & (flags::TRANSLUCENT | flags::ICE) == 0 {
                desc.flags |= flags::SMOKE | flags::TRANSLUCENT;
            }
            self.add_material(engine, store, &desc)
        });
        self.materials.insert(key, made);
        made
    }

    fn add_material(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        desc: &MaterialDesc,
    ) -> Option<(u32, u32)> {
        let record = MaterialRecord::new(desc, |k| self.texture(engine, store, k));
        let flags = record.flags;
        let index = engine.materials.add_material(record).ok()?;
        self.stats.materials += 1;
        Some((index, flags))
    }

    /// The material every block of ground draws with; its cells carry their own pictures.
    pub fn terrain_material(&mut self, engine: &mut Engine) -> Option<u32> {
        if self.terrain_material.is_none() {
            let (roughness, metal) = SurfaceKind::Earth.roughness_metal();
            let record = MaterialRecord {
                base_color: [1.0; 4],
                textures: [crate::asset::material::NO_TEXTURE; 6],
                flags: flags::TERRAIN,
                kind: SurfaceKind::Earth.index(),
                roughness,
                metal,
                ..MaterialRecord::default()
            };
            self.terrain_material = engine.materials.add_material(record).ok();
            if let Some(m) = self.terrain_material {
                engine.materials.pin_material(m);
            }
        }
        self.terrain_material
    }

    /// The mesh of graphics object `id`, loading it the first time. `foliage` marks its clip-mapped
    /// pictures as foliage when nothing has said what they are (scenery's leaves).
    pub fn mesh(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        id: DataId,
        foliage: bool,
    ) -> Option<MeshId> {
        if let Some(m) = self.meshes.get(&id.0) {
            return *m;
        }
        let made = self.load_mesh(engine, store, id, foliage, 0);
        if made.is_some() {
            self.stats.meshes += 1;
        } else {
            self.stats.empty_meshes += 1;
        }
        self.meshes.insert(id.0, made);
        made
    }

    fn load_mesh(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        id: DataId,
        foliage: bool,
        variant: u32,
    ) -> Option<MeshId> {
        let obj = decode::<GfxObj>(store, DbType::GfxObj, id)?;
        let mut mesh = gfxobj_mesh(&obj);
        if mesh.indices.is_empty() {
            return None;
        }
        let webs = self.weave_webs(engine, store, &mut mesh, id, variant);
        let mut draws = Vec::with_capacity(mesh.submeshes.len());
        for (k, s) in mesh.submeshes.iter().enumerate() {
            if let Some((silk, material, cards)) = &webs {
                if k == *silk {
                    draws.push(Some((*material, PipelineKind::Web)));
                    continue;
                }
                if let (true, Some(surface)) = (cards.contains(&k), s.surface) {
                    let m = self.web_card(engine, store, surface, s.sides == Sides::Both);
                    draws.push(m.map(|(index, f)| (index, pipeline_for(f, s.sides))));
                    continue;
                }
            }
            let Some(surface) = s.surface else {
                draws.push(None);
                continue;
            };
            if foliage {
                if let Some(rec) = decode::<Surface>(store, DbType::Surface, surface) {
                    let tex = rec.orig_texture_id;
                    if rec.surface_type & st::BASE1_CLIPMAP != 0
                        && self.kinds.kind(tex) == SurfaceKind::Plain
                    {
                        if let Some(t) = tex {
                            self.kinds.insert(t, SurfaceKind::Foliage);
                        }
                    }
                }
            }
            let m = self.material(engine, store, surface, s.sides == Sides::Both);
            draws.push(m.map(|(index, f)| (index, pipeline_for(f, s.sides))));
        }
        let id = engine
            .scene
            .add_mesh(&engine.gpu, &mesh, |k| draws.get(k).copied().flatten())
            .ok()?;
        self.submeshes.insert(id.0, mesh.submeshes);
        Some(id)
    }

    /// Spins webs to fit `mesh`'s cobweb cards ([`crate::webs`]), as variant `variant`: the
    /// silk's sub-mesh and material, and the cards' sub-meshes.
    fn weave_webs(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        mesh: &mut crate::asset::mesh::MeshData,
        id: DataId,
        variant: u32,
    ) -> Option<(usize, u32, Vec<usize>)> {
        let textures: Vec<Option<DataId>> = mesh
            .submeshes
            .iter()
            .map(|s| {
                s.surface
                    .and_then(|f| self.surface_texture(store, f))
                    .filter(|t| crate::webs::is_cobweb(*t))
            })
            .collect();
        let texture = textures.iter().flatten().next().copied()?;
        let cards: Vec<usize> = (0..textures.len())
            .filter(|&k| textures[k] == Some(texture))
            .collect();
        let picture = self.webs.picture(store, texture)?;
        let seed = id.0 ^ variant.wrapping_mul(0x9E37_79B9);
        let (woven, silk) = crate::webs::weave(mesh, &cards, &picture, texture.0, seed)?;
        let material = match self.webs.silk {
            Some(m) => m,
            None => {
                let m = engine
                    .materials
                    .add_material(crate::webs::Weaver::silk_record())
                    .ok()?;
                self.webs.silk = Some(m);
                m
            }
        };
        self.webs.woven.insert(id.0);
        *mesh = woven;
        Some((silk, material, cards))
    }

    /// The mesh of graphics object `id` placed at `at`: `mesh`, or for a cobweb one of its
    /// other webs, chosen by where it stands, so neighbouring webs differ.
    fn web_variant(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        id: DataId,
        mesh: MeshId,
        at: &Affine3A,
        foliage: bool,
    ) -> MeshId {
        if !self.webs.woven.contains(&id.0) {
            return mesh;
        }
        let p = at.translation * 4.0;
        let spot = [p.x, p.y, p.z].map(|v| pyreal_primitives::num::floor_to_i32(v).cast_unsigned());
        let variant = crate::webs::variant(spot);
        if variant == 0 {
            return mesh;
        }
        if let Some(m) = self.web_variants.get(&(id.0, variant)) {
            return m.unwrap_or(mesh);
        }
        let made = self.load_mesh(engine, store, id, foliage, variant);
        self.web_variants.insert((id.0, variant), made);
        made.unwrap_or(mesh)
    }

    /// A cobweb card's material: its own, left out while the silk is drawn.
    fn web_card(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        surface: DataId,
        two_sided: bool,
    ) -> Option<(u32, u32)> {
        let key = (surface.0, two_sided);
        if let Some(m) = self.webs.cards.get(&key) {
            return *m;
        }
        let made = decode::<Surface>(store, DbType::Surface, surface).and_then(|s| {
            let palettes = drawn_in(store, None, s.orig_texture_id);
            let pack = self
                .overrides
                .maps_for(surface, s.orig_texture_id, &palettes, &|t| {
                    self.print_of(Some(store), t)
                });
            let mut desc = describe_surface(&s, two_sided, None, &self.kinds, pack);
            desc.flags |= flags::WEB_CARD;
            self.add_material(engine, store, &desc)
        });
        self.webs.cards.insert(key, made);
        made
    }

    /// The mesh of graphics object `id` dressed as `dress` says: its own mesh's vertices drawn
    /// with the dressed materials, made once per dress.
    pub fn mesh_dressed(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        id: DataId,
        dress: &Dress,
    ) -> Option<MeshId> {
        let base = self.mesh(engine, store, id, false)?;
        if dress.is_plain() {
            return Some(base);
        }
        let key = (id.0, dress.clone());
        if let Some(m) = self.dressed.get(&key) {
            return *m;
        }
        let subs = self.submeshes.get(&base.0).cloned().unwrap_or_default();
        let draws: Vec<Option<(u32, PipelineKind)>> = subs
            .iter()
            .map(|s| {
                let m = self.material_dressed(
                    engine,
                    store,
                    s.surface?,
                    s.sides == Sides::Both,
                    dress,
                )?;
                Some((m.0, pipeline_for(m.1, s.sides)))
            })
            .collect();
        let made = engine
            .scene
            .add_mesh_variant(base, &subs, |k| draws.get(k).copied().flatten())
            .ok();
        if let Some(m) = made {
            self.submeshes.insert(m.0, subs);
            self.dressed_made += 1;
        }
        self.dressed.insert(key, made);
        made
    }

    /// Links `mesh` (graphics object `gfxobj`'s near level) to its coarser levels, once.
    fn link_degrades(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        mesh: MeshId,
        gfxobj: DataId,
        foliage: bool,
    ) {
        self.link_degrades_dressed(engine, store, mesh, gfxobj, foliage, &Dress::default());
    }

    /// [`Self::link_degrades`] with every level dressed as `dress` says.
    pub fn link_degrades_dressed(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        mesh: MeshId,
        gfxobj: DataId,
        foliage: bool,
        dress: &Dress,
    ) {
        if !self.linked.insert(mesh.0) {
            return;
        }
        engine.scene.set_billboard(mesh, near_mode(store, gfxobj));
        let mut at = mesh;
        for level in degrade_chain(store, gfxobj) {
            let next = level.mesh.and_then(|g| {
                if dress.is_plain() {
                    self.mesh(engine, store, g, foliage)
                } else {
                    self.mesh_dressed(engine, store, g, dress)
                }
            });
            engine.scene.set_lod(at, next, level.switch);
            if let Some(n) = next {
                engine.scene.set_billboard(n, level.mode);
            }
            match next {
                Some(n) => at = n,
                None => break,
            }
        }
    }

    /// Places object `id` (a setup or a graphics object) at `at` outdoors; the instances it made.
    pub fn place_object(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        id: DataId,
        at: Affine3A,
        foliage: bool,
    ) -> usize {
        self.place_object_in(engine, store, id, at, foliage, OUTSIDE)
    }

    /// Object `id`'s lamps placed at `at` in cell slot `cell`: the pack's where it gives the object
    /// some there, else the dats'.
    pub fn place_lamps(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        id: DataId,
        at: &Affine3A,
        cell: u32,
    ) {
        let retail = self.lamps.sources(store, id).to_vec();
        let sources = self
            .pack
            .as_ref()
            .and_then(|p| crate::lamps::pack_sources(&p.pack, id, cell == OUTSIDE, &retail))
            .unwrap_or(retail)
            .into_iter()
            .map(|s| crate::lamps::fixture(s, cell != OUTSIDE));
        // The object number the placement about to be made takes.
        let object = self.next_object;
        let placed: Vec<crate::lamps::Lamp> = sources
            .map(|s| crate::lamps::place(&s, at, cell, object))
            .collect();
        let flames = engine.flames.sources(store, id).to_vec();
        let (carded, beds): (Vec<_>, Vec<_>) = flames.iter().partition(|f| f.kind.carded());
        engine.flames.placed.extend(
            carded
                .iter()
                .map(|f| crate::flames::place(f, at, cell, object, &placed)),
        );
        // A hearth's fire is a body of fire flickering with its lamp, and throws its own light;
        // so is a forge's or a fire pit's.
        let mut glows = Vec::new();
        for f in beds
            .iter()
            .filter(|f| f.kind == crate::flames::FlameKind::Hearth)
        {
            let lamp = crate::flames::hearth_lamp_of(at.transform_point3(f.offset), &placed);
            engine.flames.hearths.push(crate::body_fire::hearth(
                f, at, cell, object, lamp.0, lamp.1,
            ));
            glows.push(crate::body_fire::hearth_lamp(f, at, cell, object, lamp));
        }
        for f in &beds {
            let Some((b, there)) = crate::flames::firebox_of(f, at, &self.fireboxes) else {
                continue;
            };
            let flicker = crate::flames::flicker_of(at.transform_point3(f.offset), &placed);
            let (fire, glow) = crate::body_fire::in_firebox(f, b, &there, cell, object, flicker);
            engine.flames.hearths.push(fire);
            glows.push(glow);
        }
        engine
            .particles
            .flamed
            .extend(carded.iter().map(|f| f.sprite));
        engine
            .particles
            .puffed
            .extend(beds.iter().map(|f| f.sprite));
        // A wreath's fire is a body of fire whirling round its object, flickering on its lamp's
        // seed. It brings its own light, so the lamp made up for its puffs, inside its object, goes.
        let mut placed = placed;
        for f in beds
            .iter()
            .filter(|f| f.kind == crate::flames::FlameKind::Wreath)
        {
            let base = at.transform_point3(f.offset);
            let (seed, _) = crate::flames::flicker_of(base, &placed);
            engine
                .flames
                .hearths
                .push(crate::body_fire::wreath(f, at, cell, object, seed));
            placed.retain(|l| {
                l.kind != crate::lamps::LampKind::Flame || l.position.distance(base) > 0.5
            });
        }
        engine.lamps.extend(placed);
        engine.lamps.extend(glows);
    }

    /// Object `id`'s lamps at `at` in cell slot `cell`, belonging to object number `object`: the
    /// pack's where it gives the object some there, else the dats', else a creature's glow (an
    /// acid creature's glow in place of its own). For an object that moves, whose lamps are
    /// placed again each frame.
    pub fn lamps_at(
        &mut self,
        store: &RetailDatStore,
        id: DataId,
        at: &Affine3A,
        cell: u32,
        object: u32,
        creature: bool,
    ) -> Vec<crate::lamps::Lamp> {
        let retail = self.lamps.sources(store, id).to_vec();
        let sources = self
            .pack
            .as_ref()
            .and_then(|p| crate::lamps::pack_sources(&p.pack, id, cell == OUTSIDE, &retail))
            .unwrap_or(retail);
        let sources = if creature
            && (sources.is_empty() || self.lamps.glows(store, id).iter().any(crate::lamps::acid))
        {
            self.lamps.glows(store, id).to_vec()
        } else {
            sources
        };
        sources
            .iter()
            .map(|s| crate::lamps::place(s, at, cell, object))
            .collect()
    }

    /// Object `id`'s default script started at `at` in cell slot `cell`, as a particle host,
    /// when the engine has the animation assets and the script starts emitters; the placement
    /// about to be made takes the host's object number.
    pub fn place_particles(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        id: DataId,
        at: &Affine3A,
        cell: u32,
    ) -> Option<usize> {
        let assets = engine.particles.assets.clone()?;
        let sprites = engine.particles.setup_sprites(id);
        let animated = pyreal_dat::divine_type(id) == Some(DbType::Setup)
            && assets
                .setup(id)
                .is_some_and(|s| s.default_animation.is_some());
        let (pools, spills) = self
            .pool_faces
            .entry(id.0)
            .or_insert_with(|| {
                let faces = crate::fountains::pool_faces(store, id);
                let spills = crate::fountains::spills(store, id, &faces);
                (faces, spills)
            })
            .clone();
        let still = sprites.is_empty() && !animated;
        if still && spills.is_empty() {
            return None;
        }
        for g in sprites {
            crate::particles::prepare_sprite(engine, self, store, g);
        }
        let (scale, turn, t) = at.to_scale_rotation_translation();
        let frame = pyreal_primitives::Frame::new(
            pyreal_primitives::Vec3::new(t.x, t.y, t.z),
            pyreal_primitives::Quat {
                w: turn.w,
                x: turn.x,
                y: turn.y,
                z: turn.z,
            },
        );
        let object = self.next_object;
        let flames = engine.flames.sources(store, id);
        let wreath = flames
            .iter()
            .any(|f| f.kind == crate::flames::FlameKind::Wreath);
        let flamed = flames.iter().any(|f| f.kind.carded());
        let puffed = flames.iter().any(|f| !f.kind.carded());
        let hearth = flames.iter().any(|f| {
            f.kind == crate::flames::FlameKind::Hearth
                || crate::flames::firebox_of(f, at, &self.fireboxes).is_some()
        });
        let k = if still {
            engine.particles.hold(id, frame, scale.x, cell, object)
        } else {
            engine.particles.spawn(id, frame, scale.x, cell, object)
        }
        .then(|| engine.particles.hosts.len() - 1)?;
        engine.particles.hosts[k].flamed = flamed;
        engine.particles.hosts[k].puffed = puffed;
        engine.particles.hosts[k].pools = pools;
        engine.particles.hosts[k].spills = spills;
        engine.particles.hosts[k].hearth = hearth || wreath;
        engine.particles.hosts[k].animated.then_some(k)
    }

    /// A new object number, for selection.
    pub fn next_object(&mut self) -> u32 {
        let object = self.next_object;
        self.next_object += 1;
        object
    }

    /// The pack's model for retail object `id` (a setup or a graphics object), its levels linked:
    /// what a server's object or one of its parts is drawn as in place of the files' own.
    pub fn pack_object(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        id: DataId,
    ) -> Option<MeshId> {
        self.pack.as_ref()?;
        self.pack_mesh(engine, Some(store), id).map(|(m, _)| m)
    }

    /// The near level of the pack's model for retail object `id`, its coarser levels linked at
    /// the pack's distances and its level of pictures (an impostor) past its last level of
    /// shape ([`Self::cards_from`]); made once. With it, how the wind sways it.
    fn pack_mesh(
        &mut self,
        engine: &mut Engine,
        store: Option<&RetailDatStore>,
        id: DataId,
    ) -> Option<(MeshId, u32)> {
        if let Some(m) = self.pack_meshes.get(&id.0) {
            return *m;
        }
        let pack = Arc::clone(self.pack.as_ref()?);
        let Some(model) = pack.model_for(id, || self.print_of(store, id)) else {
            if pack.model(id).is_some() {
                self.stats.pack_unmatched += 1;
                self.pack_meshes.insert(id.0, None);
            }
            return None;
        };
        // Levels drawn as geometry; a level of shadows alone is skipped, and a level of pictures
        // from round the model ends the chain.
        let mut lods: Vec<&crate::pack::PackLod> = model
            .lods
            .iter()
            .take_while(|l| l.views.is_none())
            .filter(|l| !l.shadows_only)
            .collect();
        let card = (self.cards_from > 0.0)
            .then(|| model.lods.iter().find(|l| l.views.is_some()))
            .flatten()
            .and_then(|l| self.pack_lod(engine, l, model.foliage));
        // An open-ended last level the card takes over where it begins would never be drawn
        // alone: the level before crossfades to the card itself.
        let n = lods.len();
        if card.is_some()
            && self.cards_from <= 1.0
            && n > 1
            && !lods[n - 1].distance.is_finite()
            && lods[n - 2].distance >= CARD_LEAST
        {
            lods.pop();
        }
        let mut meshes = Vec::with_capacity(lods.len());
        for lod in &lods {
            meshes.push(self.pack_lod(engine, lod, model.foliage));
        }
        if let Some(c) = card {
            engine.scene.set_billboard(c, crate::scene::IMPOSTOR_MODE);
        }
        self.pack_cards.insert(id.0, card);
        let first = meshes.first().copied().flatten();
        let ends = model.lods.iter().all(|l| l.views.is_none());
        for (k, lod) in lods.iter().enumerate() {
            let Some(at) = meshes[k] else { continue };
            match meshes.get(k + 1) {
                Some(next) => engine.scene.set_lod(at, *next, lod.distance),
                None if card.is_some() => {
                    let begun = k
                        .checked_sub(1)
                        .map_or(CARD_LEAST, |j| lods[j].distance.max(CARD_LEAST));
                    let from = if lod.distance.is_finite() {
                        lod.distance
                    } else {
                        begun * self.cards_from
                    };
                    engine.scene.set_lod(at, card, from);
                }
                None if ends && lod.distance.is_finite() => {
                    engine.scene.set_lod(at, None, lod.distance);
                }
                None => {}
            }
        }
        if first.is_some() {
            self.stats.pack_models += 1;
        }
        // The wind's hold, by the near level's height.
        let height = meshes
            .first()
            .copied()
            .flatten()
            .and_then(|m| engine.scene.mesh(m))
            .map_or(0.0, |m| m.center.z + m.radius);
        let sway = if model.wind > 0.0 && height > 0.05 {
            crate::scene::sway(model.wind, height)
        } else {
            0
        };
        let out = first.map(|f| (f, sway));
        self.pack_meshes.insert(id.0, out);
        out
    }

    /// The level of pictures of the pack's model for retail object `id`, when it has one.
    fn far_card(&mut self, engine: &mut Engine, id: DataId) -> Option<MeshId> {
        if !self.pack_cards.contains_key(&id.0) {
            self.pack_mesh(engine, None, id)?;
        }
        self.pack_cards.get(&id.0).copied().flatten()
    }

    /// One level of a pack model in the pools, each primitive with its own material.
    fn pack_lod(
        &mut self,
        engine: &mut Engine,
        lod: &crate::pack::PackLod,
        foliage: bool,
    ) -> Option<MeshId> {
        let mesh = crate::pack::lod_mesh(lod);
        if mesh.indices.is_empty() {
            return None;
        }
        let draws = self.pack_materials(engine, lod, &mesh, foliage);
        let id = engine
            .scene
            .add_mesh(&engine.gpu, &mesh, |k| draws.get(k).copied().flatten())
            .ok()?;
        self.stats.pack_meshes += 1;
        Some(id)
    }

    /// A level of ground cover in the pools, drawn by the cover pass rather than the cull's
    /// buckets: each primitive's material is written into its vertices, and its index range
    /// (pool-absolute) and material handed back. `pictures` stands in for the level's pictures
    /// (a card baked here, not in the pack): its colour and normal slots.
    pub fn cover_level(
        &mut self,
        engine: &mut Engine,
        lod: &crate::pack::PackLod,
        foliage: bool,
        pictures: Option<(u32, u32)>,
    ) -> Option<Vec<(u32, u32, u32)>> {
        let mut mesh = crate::pack::lod_mesh(lod);
        if mesh.indices.is_empty() {
            return None;
        }
        if let Some((colour, normal)) = pictures {
            // The baked card's pictures are no pack image: name them for the cache.
            for p in &lod.primitives {
                for (i, slot, part) in [
                    (p.material.base_color_image, colour, "colour"),
                    (p.material.normal_image, normal, "data"),
                ] {
                    if let Some(i) = i {
                        self.textures
                            .insert(TextureKey::Pack(format!("image:{i}:{part}")), Some(slot));
                    }
                }
            }
        }
        let draws = self.pack_materials(engine, lod, &mesh, foliage);
        let mut first_vertex = 0;
        for (k, p) in lod.primitives.iter().enumerate() {
            let n = p.vertices.len() / pyreal_art_pack::VERTEX_FLOATS;
            let material = draws.get(k).copied().flatten().map_or(0, |d| d.0);
            let colours = p
                .vertices
                .as_chunks::<{ pyreal_art_pack::VERTEX_FLOATS }>()
                .0;
            for (v, f) in mesh.vertices[first_vertex..first_vertex + n]
                .iter_mut()
                .zip(colours)
            {
                // Its vertex colour, as the card baked from it takes it.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let shade = (f[12..15].iter().sum::<f32>() / 3.0 * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u32;
                v.extra = [material, shade];
            }
            first_vertex += n;
        }
        let id = engine.scene.add_mesh(&engine.gpu, &mesh, |_| None).ok()?;
        // The cover pass draws it, never an instance: kept from eviction, with its materials.
        engine.scene.pin(id);
        for (m, _) in draws.iter().flatten() {
            engine.materials.pin_material(*m);
        }
        let base = u32::try_from(engine.scene.mesh(id)?.first_index).ok()?;
        self.stats.pack_meshes += 1;
        Some(
            mesh.submeshes
                .iter()
                .zip(&draws)
                .filter_map(|(s, d)| d.map(|(m, _)| (base + s.first_index, s.index_count, m)))
                .collect(),
        )
    }

    /// Each primitive of a pack level's material and the pipeline it draws with.
    fn pack_materials(
        &mut self,
        engine: &mut Engine,
        lod: &crate::pack::PackLod,
        mesh: &crate::asset::mesh::MeshData,
        foliage: bool,
    ) -> Vec<Option<(u32, PipelineKind)>> {
        let mut draws = Vec::with_capacity(lod.primitives.len());
        for (p, sub) in lod.primitives.iter().zip(&mesh.submeshes) {
            let (mut f, keys) = crate::pack::model_material(&p.material, foliage);
            let mut slot = |k: &Option<TextureKey>, bit: u32| -> u32 {
                match k.as_ref().and_then(|k| self.pack_texture(engine, k)) {
                    Some(s) => s,
                    None => {
                        f &= !bit;
                        crate::asset::material::NO_TEXTURE
                    }
                }
            };
            let textures = [
                slot(&keys[0], flags::HAS_BASE),
                slot(&keys[1], flags::HAS_NORMAL),
                crate::asset::material::NO_TEXTURE,
                slot(&keys[2], flags::HAS_ROUGH_METAL),
                slot(&keys[3], flags::HAS_OCCLUSION),
                crate::asset::material::NO_TEXTURE,
            ];
            let kind = if f & flags::FOLIAGE != 0 {
                SurfaceKind::Foliage
            } else {
                SurfaceKind::Plain
            };
            let views = lod.views.map_or(0, |v| {
                f |= flags::IMPOSTOR;
                v.count.min(255) | (v.columns.min(255) << 8)
            });
            let record = MaterialRecord {
                base_color: p.material.base_color,
                emissive: [0.0; 4],
                textures,
                flags: f,
                kind: kind.index(),
                roughness: p.material.roughness,
                metal: p.material.metallic,
                alpha_cutoff: p.material.alpha_cutoff.unwrap_or(0.5),
                pad: views,
            };
            let material = engine.materials.add_material(record).ok();
            self.stats.materials += usize::from(material.is_some());
            draws.push(material.map(|m| (m, pipeline_for(f, sub.sides))));
        }
        draws
    }

    /// An animated static's parts (setup `id`, whose host is `host`): instances moved each frame
    /// to where its animation puts them ([`crate::particles::Particles::pose`]).
    fn place_animated(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        id: DataId,
        host: usize,
        foliage: bool,
        cell: u32,
    ) -> usize {
        let object = self.next_object;
        self.next_object += 1;
        let Some(setup) = decode::<pyreal_assets::Setup>(store, DbType::Setup, id) else {
            return 0;
        };
        let mut placed = Vec::new();
        for (k, &g) in setup.parts.iter().enumerate() {
            let Some(gfx) = crate::asset::object::near_mesh(store, g) else {
                continue;
            };
            let Some(mesh) = self.mesh(engine, store, gfx, foliage) else {
                continue;
            };
            self.link_degrades(engine, store, mesh, gfx, foliage);
            let Some(p) = engine.particles.hosts[host].driver.part_array.parts.get(k) else {
                continue;
            };
            let transform = crate::body::part_affine(&p.pos, p.gfxobj_scale);
            if let Ok(i) = engine.scene.add_instance(Instance {
                mesh,
                transform,
                previous: transform,
                object,
                cell,
                sway: 0,
            }) {
                self.placed.push(i);
                placed.push((k, i));
            }
        }
        let n = placed.len();
        engine.particles.hosts[host].instances = placed;
        self.stats.instances += n;
        n
    }

    /// [`Self::place_object`] seen through cell slot `cell`.
    pub fn place_object_in(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        id: DataId,
        at: Affine3A,
        foliage: bool,
        cell: u32,
    ) -> usize {
        self.place_lamps(engine, store, id, &at, cell);
        let animated = self.place_particles(engine, store, id, &at, cell);
        if let Some((mesh, sway)) = self
            .pack
            .is_some()
            .then(|| self.pack_mesh(engine, Some(store), id))
            .flatten()
        {
            let object = self.next_object();
            let placed = self.add_instance(
                engine,
                Instance {
                    sway: if cell == OUTSIDE { sway } else { 0 },
                    ..Instance::at(mesh, at, object, cell)
                },
            );
            self.stats.instances += usize::from(placed);
            if !crate::pack::lays_over(id) {
                return usize::from(placed);
            }
        }
        if let Some(host) = animated {
            return self.place_animated(engine, store, id, host, foliage, cell);
        }
        let object = self.next_object;
        self.next_object += 1;
        let mut n = 0;
        for part in object_parts(store, id) {
            let Some(mesh) = self.mesh(engine, store, part.gfxobj, foliage) else {
                continue;
            };
            let transform = at * part.transform;
            let mesh = self.web_variant(engine, store, part.gfxobj, mesh, &transform, foliage);
            self.link_degrades(engine, store, mesh, part.gfxobj, foliage);
            if self.add_instance(
                engine,
                Instance {
                    mesh,
                    transform,
                    previous: transform,
                    object,
                    cell,
                    sway: 0,
                },
            ) {
                n += 1;
            }
        }
        self.stats.instances += n;
        n
    }
}

impl AssetLoader {
    /// The mesh of a room: its cell structure against its surface list, its portals left open.
    /// Rooms of one building type share it.
    fn room_mesh(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        d: &DecodedCell,
    ) -> Option<MeshId> {
        let key = (
            d.cell.environment.0,
            d.cell.cell_struct,
            d.cell.surfaces.iter().map(|s| s.0).collect::<Vec<_>>(),
        );
        if let Some(m) = self.cell_meshes.get(&key) {
            return *m;
        }
        let mesh = cell_mesh(&d.structure, &d.cell.surfaces);
        let made = (!mesh.indices.is_empty())
            .then(|| {
                let draws: Vec<Option<(u32, PipelineKind)>> = mesh
                    .submeshes
                    .iter()
                    .map(|s| {
                        let m = self.material(engine, store, s.surface?, s.sides == Sides::Both)?;
                        Some((m.0, pipeline_for(m.1, s.sides)))
                    })
                    .collect();
                engine
                    .scene
                    .add_mesh(&engine.gpu, &mesh, |k| draws.get(k).copied().flatten())
                    .ok()
            })
            .flatten();
        self.cell_meshes.insert(key, made);
        made
    }

    /// Places one room of a block whose south-west corner is at `origin`: its mesh, its furniture,
    /// and its volume and portals in the engine's cell graph; its slot.
    pub fn place_cell(
        &mut self,
        engine: &mut Engine,
        store: &RetailDatStore,
        d: &DecodedCell,
        origin: Vec3,
    ) -> u32 {
        let at_block = Affine3A::from_translation(origin);
        let at = at_block * frame_affine(&d.cell.frame);
        let cell = room_cell(d, origin);
        let volume = cell.volume.clone();
        let slot = engine.cells.add(cell);
        if let Some(v) = &volume {
            let mut words = Vec::new();
            v.write(f32::INFINITY, &mut words);
            if let Ok(first) = engine.scene.add_cut_volume(&engine.gpu, &words) {
                self.cuts.push(Cut {
                    min: v.min,
                    max: v.max,
                    first,
                    len: u32::try_from(words.len()).unwrap_or(0),
                    slot,
                });
            }
        }
        if let Some(mesh) = self.room_mesh(engine, store, d) {
            let object = self.next_object;
            self.next_object += 1;
            if self.add_instance(
                engine,
                Instance {
                    mesh,
                    transform: at,
                    previous: at,
                    object,
                    cell: slot,
                    sway: 0,
                },
            ) {
                self.stats.instances += 1;
            }
        }
        self.stats.cells += 1;
        let first = self.placed.len();
        self.fireboxes = fireboxes_of(
            d.cell
                .static_objects
                .iter()
                .map(|o| (o.id, at_block * object_affine(&o.frame, 1.0))),
        );
        for o in d.cell.static_objects.iter().filter(|o| o.id.0 != 0) {
            let placed = at_block * object_affine(&o.frame, 1.0);
            self.place_object_in(engine, store, o.id, placed, false, slot);
            self.stats.cell_statics += 1;
        }
        self.fireboxes.clear();
        // Furniture reaching past the room's walls is seen through the rooms it reaches too.
        for &id in &self.placed[first..] {
            let Some((center, radius)) = engine.scene.instance(id).and_then(|i| {
                let m = engine.scene.mesh(i.mesh)?;
                let t = i.transform.matrix3;
                let scale = t
                    .x_axis
                    .length()
                    .max(t.y_axis.length())
                    .max(t.z_axis.length());
                Some((i.transform.transform_point3(m.center), m.radius * scale))
            }) else {
                continue;
            };
            let seen_through = engine.cells.furniture_slot(slot, center, radius);
            engine.scene.set_cell(id, seen_through);
        }
        slot
    }

    /// The first words of the room volumes whose boxes overlap `lo..hi`.
    fn cuts_over(&self, lo: Vec3, hi: Vec3) -> Vec<u32> {
        self.cuts
            .iter()
            .filter(|c| c.min.cmple(hi).all() && c.max.cmpge(lo).all())
            .map(|c| c.first)
            .collect()
    }

    /// Writes the reach of each room volume in `slots` (every one for `None`): its building's,
    /// once the buildings are placed.
    fn write_cut_reaches(&self, engine: &Engine, slots: Option<&[u32]>) {
        for c in &self.cuts {
            if slots.is_some_and(|s| !s.contains(&c.slot)) {
                continue;
            }
            let reach = engine.cells.cell(c.slot).map_or(f32::INFINITY, |r| r.reach);
            let word: Vec<u8> = [c.max.x, c.max.y, c.max.z, reach.min(f32::MAX)]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            engine
                .scene
                .cuts
                .write(&engine.gpu.queue, u64::from(c.first) + 1, &word);
        }
    }

    /// Takes the cut volumes of the rooms in `slots` out, freeing their words; their boxes.
    fn remove_cuts(&mut self, engine: &mut Engine, slots: &[u32]) -> Vec<(Vec3, Vec3)> {
        let mut gone = Vec::new();
        self.cuts.retain(|c| {
            if slots.contains(&c.slot) {
                engine.scene.cuts.free(u64::from(c.first), u64::from(c.len));
                gone.push((c.min, c.max));
                false
            } else {
                true
            }
        });
        gone
    }

    /// Moves every cut volume by `delta`, its words written again from its room's volume.
    pub(crate) fn translate_cuts(&mut self, engine: &Engine, delta: Vec3) {
        for c in &mut self.cuts {
            c.min += delta;
            c.max += delta;
            let Some(cell) = engine.cells.cell(c.slot) else {
                continue;
            };
            let Some(v) = cell.volume.as_ref() else {
                continue;
            };
            let mut words = Vec::new();
            v.write(cell.reach, &mut words);
            let bytes: Vec<u8> = words
                .iter()
                .flatten()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            engine
                .scene
                .cuts
                .write(&engine.gpu.queue, u64::from(c.first), &bytes);
        }
    }
}

/// Dat pictures decoded ahead of need by the land's planner threads (decoding is most of what
/// placing a block new to the journey costs), taken by the loader as it meets them. The planners
/// skip what the loader has (it notes each picture it loads, and forgets what it frees).
#[derive(Debug, Default)]
pub struct Prefetch {
    // ORDER-OK: looked up by picture; iterated only to drop the old ones.
    /// Each picture with the age it was decoded at.
    ready: Mutex<HashMap<(u32, bool), Held>>,
    known: Mutex<std::collections::HashSet<(u32, bool)>>,
    /// Bumped at each switch of the square ([`Self::drop_unused`]).
    age: std::sync::atomic::AtomicU64,
}

/// A picture decoded ahead: the age it was decoded at, and its pixels.
type Held = (u64, Arc<TexturePixels>);

/// Pictures held ready at most.
const PREFETCH_HELD: usize = 256;

impl Prefetch {
    /// Decodes picture `id` ahead, unless it is loaded or held already; whether it did.
    pub fn decode(&self, store: &RetailDatStore, id: DataId, clip_map: bool) -> bool {
        let key = (id.0, clip_map);
        {
            let Ok(mut known) = self.known.lock() else {
                return false;
            };
            if !known.insert(key) {
                return false;
            }
        }
        let Ok(p) = TextureSource::new(store).decode(id, clip_map, None) else {
            return false;
        };
        if let Ok(mut ready) = self.ready.lock() {
            if ready.len() < PREFETCH_HELD {
                let age = self.age.load(std::sync::atomic::Ordering::Relaxed);
                ready.insert(key, (age, Arc::new(p)));
                return true;
            }
        }
        // Too many held: the loader decodes it itself.
        if let Ok(mut known) = self.known.lock() {
            known.remove(&key);
        }
        false
    }

    /// The pictures of `surfaces`, decoded ahead.
    pub fn surfaces(&self, store: &RetailDatStore, surfaces: impl IntoIterator<Item = DataId>) {
        for s in surfaces {
            let Some(rec) = decode::<Surface>(store, DbType::Surface, s) else {
                continue;
            };
            if let Some(t) = rec.orig_texture_id {
                self.decode(store, t, rec.surface_type & st::BASE1_CLIPMAP != 0);
            }
        }
    }

    /// The pictures `plan`'s objects, rooms and scenery draw, decoded ahead; `seen` holds the
    /// graphics objects already looked through on this thread.
    pub fn block(
        &self,
        store: &RetailDatStore,
        plan: &BlockPlan,
        seen: &mut std::collections::HashSet<u32>,
    ) {
        let mut objects: Vec<DataId> = Vec::new();
        if let Some(info) = &plan.info {
            objects.extend(info.objects.iter().map(|o| o.id));
            objects.extend(info.buildings.iter().map(|b| b.id));
        }
        for d in &plan.cells {
            self.surfaces(store, d.cell.surfaces.iter().copied());
            objects.extend(
                d.cell
                    .static_objects
                    .iter()
                    .map(|o| o.id)
                    .filter(|i| i.0 != 0),
            );
        }
        objects.extend(plan.scenery.iter().map(|s| s.0));
        for id in objects {
            for part in object_parts(store, id) {
                let chain = degrade_chain(store, part.gfxobj);
                let gfx = std::iter::once(part.gfxobj).chain(chain.iter().filter_map(|l| l.mesh));
                for g in gfx {
                    if !seen.insert(g.0) {
                        continue;
                    }
                    if let Some(obj) = decode::<GfxObj>(store, DbType::GfxObj, g) {
                        self.surfaces(store, obj.surfaces.iter().copied());
                    }
                }
            }
        }
    }

    /// Takes picture `id` when it was decoded ahead.
    fn take(&self, id: DataId, clip_map: bool) -> Option<Arc<TexturePixels>> {
        self.ready
            .lock()
            .ok()?
            .remove(&(id.0, clip_map))
            .map(|r| r.1)
    }

    /// The loader has picture `id`.
    fn note(&self, id: DataId, clip_map: bool) {
        if let Ok(mut k) = self.known.lock() {
            k.insert((id.0, clip_map));
        }
    }

    /// The loader freed picture `id`.
    fn forget(&self, id: DataId, clip_map: bool) {
        if let Ok(mut k) = self.known.lock() {
            k.remove(&(id.0, clip_map));
        }
    }

    /// At a switch of the square: drops the pictures decoded ahead two switches ago and never
    /// taken (their blocks were left behind, or the art pack draws those surfaces).
    pub fn drop_unused(&self) {
        let now = self.age.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let Ok(mut ready) = self.ready.lock() else {
            return;
        };
        let old: Vec<(u32, bool)> = ready
            .iter()
            .filter(|(_, (age, _))| *age + 2 <= now)
            .map(|(k, _)| *k)
            .collect();
        if let Ok(mut known) = self.known.lock() {
            for k in &old {
                known.remove(k);
                ready.remove(k);
            }
        }
    }

    /// Pictures held ready.
    #[must_use]
    pub fn held(&self) -> usize {
        self.ready.lock().map_or(0, |r| r.len())
    }
}

/// One room volume the ground is cut by: its box, its words in the cut pool, and its room.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Cut {
    min: Vec3,
    max: Vec3,
    first: u32,
    len: u32,
    slot: u32,
}

/// A room of a block whose south-west corner is at `origin`, for the cell graph: its volume and its
/// portals in render space, each portal facing into the room.
#[must_use]
pub fn room_cell(d: &DecodedCell, origin: Vec3) -> Cell {
    let at = Affine3A::from_translation(origin) * frame_affine(&d.cell.frame);
    let local = |v: &pyreal_assets::SwVertex| {
        at.transform_point3(Vec3::new(v.position.x, v.position.y, v.position.z))
    };
    let vertices = &d.structure.vertex_array.vertices;
    let volume = room_volume(&d.structure.cell_bsp, &at);
    // Portals face the room: its volume's middle, else its vertices'.
    #[allow(clippy::cast_precision_loss)]
    let center = match &volume {
        Some(v) => v.center,
        None if vertices.is_empty() => at.translation.into(),
        None => vertices.iter().map(local).sum::<Vec3>() / vertices.len() as f32,
    };
    let block = d.id.0 & 0xFFFF_0000;
    let portals = d
        .cell
        .portals
        .iter()
        .filter_map(|p| {
            let poly = d.structure.polygons.get(usize::from(p.polygon_id))?;
            let points: Vec<Vec3> = poly
                .vertex_ids
                .iter()
                .filter_map(|&i| vertices.get(usize::from(i)))
                .map(local)
                .collect();
            let to = (p.other_cell_id != 0xFFFF_FFFF).then_some(block | (p.other_cell_id & 0xFFFF));
            (points.len() >= 3).then(|| Portal::new(points, center, to))
        })
        .collect();
    Cell {
        id: d.id.0,
        portals,
        volume,
        seen_outside: d.cell.flags & 1 != 0,
        reach: f32::INFINITY,
    }
}

/// How far an object is drawn before its every part has degraded to nothing (in degrade metres);
/// infinite when one never does.
#[must_use]
pub fn draw_reach(store: &RetailDatStore, id: DataId) -> f32 {
    object_parts(store, id)
        .iter()
        .map(|p| match degrade_chain(store, p.gfxobj).last() {
            Some(l) if l.mesh.is_none() => l.switch,
            _ => f32::INFINITY,
        })
        .fold(0.0, f32::max)
}

/// A building's walls as the cover keeps off them: the box, in its own frame, of its parts'
/// vertices within two metres of its lowest (so eaves and porch roofs do not push the plants
/// back).
#[must_use]
pub fn walls_of(store: &RetailDatStore, id: DataId) -> Option<(glam::Vec2, glam::Vec2)> {
    let mut points = Vec::new();
    for p in object_parts(store, id) {
        let Some(g) = decode::<GfxObj>(store, DbType::GfxObj, p.gfxobj) else {
            continue;
        };
        points.extend(g.vertex_array.vertices.iter().map(|v| {
            p.transform
                .transform_point3(Vec3::new(v.position.x, v.position.y, v.position.z))
        }));
    }
    let floor = points.iter().map(|v| v.z).fold(f32::INFINITY, f32::min);
    let (mut low, mut high) = (glam::Vec2::splat(f32::MAX), glam::Vec2::splat(f32::MIN));
    for v in points.iter().filter(|v| v.z <= floor + 2.0) {
        low = low.min(v.truncate());
        high = high.max(v.truncate());
    }
    (low.x <= high.x).then_some((low, high))
}

/// The radius of ground object `id` (a setup or a model) clears of cover at scale one
/// ([`crate::cover::clearing_radius`]): a setup's cylinders, else its bounding radius about its
/// spheres, else its physics polygons' reach at its foot when they stand in a compact patch (a
/// wall or a fence is no disc).
#[must_use]
pub fn ground_radius(store: &RetailDatStore, id: DataId) -> Option<f32> {
    let setup = (id.0 >> 24 == 0x02)
        .then(|| decode::<pyreal_assets::Setup>(store, DbType::Setup, id))
        .flatten();
    if let Some(s) = &setup {
        if !s.cylspheres.is_empty() || !s.spheres.is_empty() {
            let cylinders = s
                .cylspheres
                .iter()
                .map(|c| (Vec3::new(c.low_pt.x, c.low_pt.y, c.low_pt.z), c.radius));
            let bounding = (!s.spheres.is_empty()).then_some(s.radius);
            return crate::cover::clearing_radius(cylinders, bounding, 1.0);
        }
    }
    let mut points = Vec::new();
    for p in object_parts(store, id) {
        let Some(g) = decode::<GfxObj>(store, DbType::GfxObj, p.gfxobj) else {
            continue;
        };
        let corners = g.physics_polygons.iter().flat_map(|q| &q.vertex_ids);
        points.extend(
            corners
                .filter_map(|&v| g.vertex_array.vertices.get(usize::from(v)))
                .map(|v| {
                    p.transform.transform_point3(Vec3::new(
                        v.position.x,
                        v.position.y,
                        v.position.z,
                    ))
                }),
        );
    }
    let floor = points.iter().map(|v| v.z).fold(f32::INFINITY, f32::min);
    let foot: Vec<glam::Vec2> = points
        .iter()
        .filter(|v| v.z <= floor + crate::cover::CLEARING_BASE)
        .map(|v| v.truncate())
        .collect();
    let (low, high) = foot.iter().fold(
        (glam::Vec2::splat(f32::MAX), glam::Vec2::splat(f32::MIN)),
        |(l, h), v| (l.min(*v), h.max(*v)),
    );
    let size = high - low;
    if foot.is_empty() || size.max_element() > 2.0 * size.min_element().max(0.1) {
        return None;
    }
    let reach = foot.iter().map(|v| v.length()).fold(0.0, f32::max);
    crate::cover::clearing_radius([], Some(reach), 1.0)
}

/// A room's convex volume in render space: its cell BSP's chain of planes moved by `at`.
#[must_use]
pub fn room_volume(bsp: &pyreal_assets::common::BspTree, at: &Affine3A) -> Option<Volume> {
    let planes: Vec<glam::Vec4> = bsp
        .nodes
        .iter()
        .filter_map(|n| n.plane)
        .map(|p| {
            let n = at.transform_vector3(Vec3::new(p.normal.x, p.normal.y, p.normal.z));
            n.extend(p.d - n.dot(at.translation.into()))
        })
        .collect();
    if planes.is_empty() {
        return None;
    }
    // The box of the volume: the corners where three planes meet inside all the others.
    let (mut lo, mut hi) = (Vec3::MAX, Vec3::MIN);
    let (mut sum, mut corners) = (Vec3::ZERO, 0u16);
    for (i, a) in planes.iter().enumerate() {
        for (j, b) in planes.iter().enumerate().skip(i + 1) {
            for c in planes.iter().skip(j + 1) {
                let m = glam::Mat3::from_cols(a.truncate(), b.truncate(), c.truncate()).transpose();
                if m.determinant().abs() < 1e-6 {
                    continue;
                }
                let p = m.inverse() * -Vec3::new(a.w, b.w, c.w);
                if planes.iter().all(|q| q.truncate().dot(p) + q.w >= -1e-3) {
                    lo = lo.min(p);
                    hi = hi.max(p);
                    sum += p;
                    corners = corners.saturating_add(1);
                }
            }
        }
    }
    (corners > 0).then(|| Volume {
        planes,
        min: lo,
        max: hi,
        center: sum / f32::from(corners),
    })
}

/// The ground textures of one area, through the loader.
struct Ground<'a> {
    loader: &'a mut AssetLoader,
    engine: &'a mut Engine,
    store: &'a RetailDatStore,
    region: &'a Region,
    later: Option<&'a LaterGround>,
}

/// End of retail's region and files beside an older world: its pictures lay the older world's
/// terrain types, which Ground Coloring tints toward 1999 as it does end of retail's own.
#[derive(Debug)]
pub struct LaterGround {
    store: RetailDatStore,
    region: Region,
}

impl LaterGround {
    /// `None` on a later world, or on an older one with no later files beside it.
    #[must_use]
    pub fn of(store: &RetailDatStore) -> Option<Self> {
        if store.era() != pyreal_dat::ContainerEra::Classic {
            return None;
        }
        let store = store.modern_files()?;
        let region = pyreal_world_data::landblock::load_region(&store).ok()?;
        Some(Self { store, region })
    }
}

/// The picture terrain type `terrain_type` is laid with and the files that hold it: end of
/// retail's beside an older world (`later`, and true), else the world's own.
fn ground_picture<'a>(
    store: &'a RetailDatStore,
    region: &'a Region,
    later: Option<&'a LaterGround>,
    terrain_type: u32,
) -> Option<(
    &'a RetailDatStore,
    &'a pyreal_assets::region::TerrainDesc,
    bool,
)> {
    let desc = |r: &'a Region| {
        r.land_surf
            .tex_merge
            .as_ref()
            .and_then(|tm| get_terrain_tex(tm, terrain_type))
    };
    if let Some(l) = later {
        if let Some(d) = desc(&l.region) {
            return Some((&l.store, d, true));
        }
    }
    Some((store, desc(region)?, false))
}

impl Ground<'_> {
    fn kind_of(&self, terrain_type: u32) -> SurfaceKind {
        if terrain_type == ROAD_TERRAIN_TYPE {
            return SurfaceKind::Stone;
        }
        self.region
            .terrain_types
            .get(terrain_type as usize)
            .map_or(SurfaceKind::Earth, |t| {
                SurfaceKind::of_terrain_name(&t.terrain_name)
            })
    }
}

impl Ground<'_> {
    /// Terrain type `terrain_type`'s ground colour, read once by `read`, for the cover's tint.
    fn note_colour(&mut self, terrain_type: u32, read: impl FnOnce(&Self) -> Option<[f32; 3]>) {
        let at = terrain_type as usize;
        if at >= crate::cover::GROUND_TYPES || self.loader.ground_colours[at].is_some() {
            return;
        }
        let Some(colour) = read(self) else {
            return;
        };
        self.loader.ground_colours[at] = Some(colour);
        if let Some(cover) = self.engine.renderer.cover.as_mut() {
            cover.set_ground(&self.engine.gpu, terrain_type, colour);
        }
    }

    /// A pack material's normal and surface (with its height) for the retail ground picture
    /// `texture` in `slot`, as the picture's companions, where the pack has one.
    fn pack_material_maps(
        &mut self,
        slot: u32,
        texture: DataId,
        store: &RetailDatStore,
        later: bool,
    ) {
        let Some(pack) = self.loader.pack.as_ref().map(Arc::clone) else {
            return;
        };
        let maps =
            PackOverrides(Arc::clone(&pack.pack)).maps_for(texture, Some(texture), &[], &|t| {
                self.loader.print_in(Some(store), t, later)
            });
        if maps.normal.is_none() && maps.height.is_none() {
            return;
        }
        let none = crate::asset::material::NO_TEXTURE;
        let mut load = |k: &Option<TextureKey>| {
            k.as_ref()
                .and_then(|k| self.loader.texture(self.engine, self.store, k))
                .unwrap_or(none)
        };
        let normal = load(&maps.normal);
        let surface = load(&maps.height);
        self.engine
            .materials
            .set_companions(slot, [normal, surface, none, 0]);
        if surface != none && maps.packed_surface {
            self.engine
                .materials
                .set_height_in(slot, crate::materials::height_in::SURFACE);
        }
    }

    /// A normal and surface (with its height) worked out from the retail road picture `texture`
    /// in `slot`, as the picture's companions, where the pack gave it none.
    fn road_maps(&mut self, slot: u32, texture: DataId, store: &RetailDatStore) {
        let none = crate::asset::material::NO_TEXTURE;
        if self
            .engine
            .materials
            .companions(slot)
            .is_none_or(|c| c[0] != none || c[1] != none)
        {
            return;
        }
        let Ok((w, h, bgra)) = TextureSource::new(store).bgra(texture) else {
            return;
        };
        let rgba: Vec<u8> = bgra
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|t| [t[2], t[1], t[0], t[3]])
            .collect();
        let [normal, surface] = pyreal_art_pack::retail_road_maps(w, h, &rgba);
        let mut upload = |image| {
            crate::pack::image_pixels(&image, false)
                .and_then(|p| self.engine.materials.add_texture(&self.engine.gpu, &p).ok())
        };
        let (Some(normal), Some(surface)) = (upload(normal), upload(surface)) else {
            return;
        };
        self.loader.stats.textures += 2;
        self.engine
            .materials
            .set_companions(slot, [normal, surface, none, 0]);
        self.engine
            .materials
            .set_height_in(slot, crate::materials::height_in::SURFACE);
    }

    /// The fingerprint of the picture the region lays `terrain_type` with, which a pack ground
    /// must match to be drawn for it.
    fn ground_record(&self, terrain_type: u32) -> Option<Fingerprint> {
        let (store, desc, later) =
            ground_picture(self.store, self.region, self.later, terrain_type)?;
        self.loader.print_in(Some(store), desc.tex_gid, later)
    }

    /// The pack's ground for `terrain_type`: its picture's slot and metres a repeat, with its
    /// normal, surface and steep slopes' picture set as the picture's companions.
    fn pack_ground(&mut self, terrain_type: u32) -> Option<(u32, f32)> {
        if let Some(g) = self.loader.pack_grounds.get(&terrain_type) {
            return *g;
        }
        let record = self.ground_record(terrain_type);
        let made = (|| {
            let pack = Arc::clone(self.loader.pack.as_ref()?);
            let ground = if terrain_type == ROAD_TERRAIN_TYPE {
                pyreal_assets::art_pack::Ground::Road
            } else {
                pyreal_assets::art_pack::Ground::Type(u8::try_from(terrain_type).ok()?)
            };
            let mut laid = |steep: bool| -> Option<(u32, f32)> {
                let i = pack.ground_index(ground, steep, record.as_ref())?;
                let key = |part: &str| TextureKey::Pack(format!("ground:{i}:{part}"));
                let albedo = self
                    .loader
                    .texture(self.engine, self.store, &key("albedo"))?;
                let normal = self.loader.texture(self.engine, self.store, &key("normal"));
                let surface = self
                    .loader
                    .texture(self.engine, self.store, &key("surface"));
                let none = crate::asset::material::NO_TEXTURE;
                self.engine.materials.set_companions(
                    albedo,
                    [normal.unwrap_or(none), surface.unwrap_or(none), none, 0],
                );
                self.engine
                    .materials
                    .set_height_in(albedo, crate::materials::height_in::ALPHA);
                let g = &pack.pack.grounds[i];
                if g.detail.is_some() {
                    if let Some(d) = self.loader.texture(self.engine, self.store, &key("detail")) {
                        self.engine.materials.set_detail(
                            albedo,
                            d,
                            g.detail_size.max(0.01),
                            g.detail_strength,
                        );
                    }
                }
                Some((albedo, g.size.max(0.01)))
            };
            let flat = laid(false)?;
            if let Some((steep, size)) = laid(true) {
                let none = crate::asset::material::NO_TEXTURE;
                let normal = self.loader.texture(
                    self.engine,
                    self.store,
                    &TextureKey::Pack(format!(
                        "ground:{}:normal",
                        pack.ground_index(ground, false, record.as_ref())?
                    )),
                );
                let surface = self.loader.texture(
                    self.engine,
                    self.store,
                    &TextureKey::Pack(format!(
                        "ground:{}:surface",
                        pack.ground_index(ground, false, record.as_ref())?
                    )),
                );
                self.engine.materials.set_companions(
                    flat.0,
                    [
                        normal.unwrap_or(none),
                        surface.unwrap_or(none),
                        steep,
                        size.to_bits(),
                    ],
                );
            }
            self.loader.stats.pack_grounds += 1;
            Some(flat)
        })();
        self.loader.pack_grounds.insert(terrain_type, made);
        made
    }
}

impl GroundTextures for Ground<'_> {
    fn ground(&mut self, terrain_type: u32) -> Option<(u32, f32, u32)> {
        let kind = self.kind_of(terrain_type);
        if let Some((slot, size)) = self.pack_ground(terrain_type) {
            self.note_colour(terrain_type, |g| {
                let pack = g.loader.pack.as_ref()?;
                let ground =
                    pyreal_assets::art_pack::Ground::Type(u8::try_from(terrain_type).ok()?);
                let i = pack.ground_index(ground, false, g.ground_record(terrain_type).as_ref())?;
                crate::pack::image_mean(&pack.pack.grounds.get(i)?.albedo, 0)
            });
            return Some((slot, -size, kind.index()));
        }
        let (store, desc, later) =
            ground_picture(self.store, self.region, self.later, terrain_type)?;
        let tint = classic_tint(store, terrain_type, desc.tex_gid);
        self.note_colour(terrain_type, |_| {
            let source = TextureSource::new(store);
            match tint {
                Some(t) => {
                    let px = source
                        .decode_with(desc.tex_gid, |w, h, bgra| {
                            crate::ground_tint::apply(t, w, h, bgra)
                        })
                        .ok()?;
                    let texels = px.levels.first()?.as_chunks::<4>().0.iter();
                    crate::asset::texture::mean_linear(texels.map(|p| [p[2], p[1], p[0], 255]), 0)
                }
                None => source.average_linear(desc.tex_gid).ok(),
            }
        });
        let kind = self.kind_of(terrain_type);
        // The world's own objects may name an id the later files hold other pictures under.
        if !later {
            self.loader.kinds.insert(desc.tex_gid, kind);
        }
        let key = match tint {
            Some(_) => TextureKey::ClassicGround {
                id: desc.tex_gid,
                terrain_type,
            },
            None if later => TextureKey::LaterGround { id: desc.tex_gid },
            None => TextureKey::Dat {
                id: desc.tex_gid,
                clip_map: false,
                swap: None,
            },
        };
        let slot = self.loader.texture(self.engine, store, &key)?;
        self.pack_material_maps(slot, desc.tex_gid, store, later);
        if terrain_type == ROAD_TERRAIN_TYPE {
            self.road_maps(slot, desc.tex_gid, store);
        }
        Some((
            slot,
            crate::terrain::tiling_f32(desc.tex_tiling.max(crate::terrain::LEAST_GROUND_REPEATS)),
            kind.index(),
        ))
    }

    fn alpha(&mut self, id: DataId) -> Option<u32> {
        let key = TextureKey::Dat {
            id,
            clip_map: false,
            swap: None,
        };
        self.loader.texture(self.engine, self.store, &key)
    }

    fn shifted(&mut self, texture: DataId, swap: PaletteSwap, kind_of: u32) -> Option<(u32, u32)> {
        let key = TextureKey::Dat {
            id: texture,
            clip_map: false,
            swap: Some(swap),
        };
        let slot = self.loader.texture(self.engine, self.store, &key)?;
        Some((slot, self.kind_of(kind_of).index()))
    }

    fn own_palette(&mut self, texture: DataId) -> Option<DataId> {
        TextureSource::new(self.store).own_palette(texture)
    }
}

/// What a scenery placement asks of an object: its collision shape's outline.
fn within_block_shape(store: &RetailDatStore, id: DataId) -> Option<WithinBlockShape> {
    let mut stats = pyreal_world_data::setup::SetupPartStats::default();
    let g = pyreal_world_data::env_cells::static_geometry(store, id, &mut stats)?;
    Some(WithinBlockShape {
        physics_mesh: g.caches_physics_bsp(),
        cylinders: g.cyl_spheres.iter().map(|c| (c.low_pt, c.radius)).collect(),
        has_spheres: !g.spheres.is_empty(),
        sorting_sphere: (g.sorting_sphere.center, g.sorting_sphere.radius),
    })
}

/// The landblocks around a centre, in the engine's render space: metres from the centre block's
/// south-west corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub center: u16,
    /// Blocks out from the centre on each side.
    pub radius: i32,
}

impl Area {
    /// The centre block's grid coordinates.
    #[must_use]
    pub fn center_block(&self) -> (i32, i32) {
        (i32::from(self.center >> 8), i32::from(self.center & 0xFF))
    }

    /// Where render space's origin (the centre block's south-west corner) lies in the world, metres.
    #[must_use]
    pub fn world_origin(&self) -> glam::Vec2 {
        let (cx, cy) = self.center_block();
        #[allow(clippy::cast_precision_loss)]
        glam::Vec2::new((cx * 192) as f32, (cy * 192) as f32)
    }

    /// Where block `(bx, by)`'s south-west corner sits in render space.
    #[must_use]
    pub fn block_origin(&self, bx: i32, by: i32) -> Vec3 {
        let (cx, cy) = (i32::from(self.center >> 8), i32::from(self.center & 0xFF));
        #[allow(clippy::cast_precision_loss)]
        Vec3::new(((bx - cx) * 192) as f32, ((by - cy) * 192) as f32, 0.0)
    }

    /// The blocks of the area that exist in the world grid.
    #[must_use]
    pub fn blocks(&self) -> Vec<(i32, i32)> {
        let (cx, cy) = (i32::from(self.center >> 8), i32::from(self.center & 0xFF));
        let mut out = Vec::new();
        for bx in cx - self.radius..=cx + self.radius {
            for by in cy - self.radius..=cy + self.radius {
                if (0..=0xFE).contains(&bx) && (0..=0xFE).contains(&by) {
                    out.push((bx, by));
                }
            }
        }
        out
    }
}

/// The ground's height at block-local `(x, y)` of `block`, on the retail triangles.
#[must_use]
pub fn ground_height(
    store: &RetailDatStore,
    region: &Region,
    block: u16,
    x: f32,
    y: f32,
) -> Option<f32> {
    let lb = decode::<CellLandblock>(store, DbType::LandBlock, landblock_did(block))?;
    let (bx, by) = (i32::from(block >> 8), i32::from(block & 0xFF));
    let land = pyreal_terrain::land::mesh::generate_landblock(
        &lb,
        region,
        bx,
        by,
        1,
        pyreal_terrain::land::mesh::Direction::InViewerBlock,
    );
    let plane = pyreal_terrain::scenery::find_terrain_poly(&land, outside_cell_index(x, y), x, y)?;
    pyreal_terrain::land::mesh::plane_set_height(plane, x, y)
}

fn block_id(bx: i32, by: i32) -> u16 {
    u16::try_from((bx << 8) | by).unwrap_or(0)
}

/// The far terrain's field `radius` blocks out around `area` (whose exact cells it leaves alone):
/// the dats' heights and each terrain type's colour, with no device, so it can be built away from
/// the frame.
#[must_use]
pub fn far_field(
    store: &RetailDatStore,
    region: &Region,
    area: Area,
    radius: i32,
) -> crate::far::FarField {
    far_field_round(store, region, area, area.center_block(), radius)
}

/// [`far_field`] round block `centre`, the exact cells covering `area.radius` blocks round it,
/// in `area`'s render space.
#[must_use]
pub fn far_field_round(
    store: &RetailDatStore,
    region: &Region,
    area: Area,
    (cx, cy): (i32, i32),
    radius: i32,
) -> crate::far::FarField {
    let (x, y) = (
        crate::far::near_blocks(cx, area.radius),
        crate::far::near_blocks(cy, area.radius),
    );
    let near = (
        area.block_origin(x.0, y.0).truncate(),
        area.block_origin(x.1, y.1).truncate(),
    );
    let later = LaterGround::of(store);
    let mut colour = |terrain_type: u32| -> [u8; 4] {
        let picture = ground_picture(store, region, later.as_ref(), terrain_type).and_then(
            |(store, d, _)| {
                let src = TextureSource::new(store);
                match classic_tint(store, terrain_type, d.tex_gid) {
                    Some(t) => tinted_average(&src, d.tex_gid, t),
                    None => src.average(d.tex_gid).ok(),
                }
            },
        );
        let [r, g, b] = picture.unwrap_or_else(|| {
            let name = region
                .terrain_types
                .get(terrain_type as usize)
                .map_or("", |t| t.terrain_name.as_str());
            kind_colour(if terrain_type == ROAD_TERRAIN_TYPE {
                SurfaceKind::Stone
            } else {
                SurfaceKind::of_terrain_name(name)
            })
        });
        [r, g, b, 255]
    };
    crate::far::FarField::build(
        store,
        region,
        block_id(cx, cy),
        area.block_origin(cx, cy),
        radius,
        near,
        &mut colour,
    )
}

/// Puts `field` on the GPU as the far terrain, its finest level drawn out to `first_range` metres.
pub fn set_far(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    field: crate::far::FarField,
    first_range: f32,
) {
    let material = loader.terrain_material(engine).unwrap_or(0);
    engine.far = Some(crate::far::FarTerrain::new(
        &engine.gpu,
        &engine.renderer.frame_layout,
        field,
        first_range,
        material,
        engine.far.as_ref().map(crate::far::FarTerrain::pipelines),
    ));
}

/// Builds the far terrain `radius` blocks out around `area` (whose exact cells it leaves alone),
/// its finest level drawn out to `first_range` metres.
pub fn load_far(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    region: &Region,
    area: Area,
    radius: i32,
    first_range: f32,
) {
    let field = far_field(store, region, area, radius);
    set_far(engine, loader, field, first_range);
}

/// Terrain type `terrain_type`'s Classic tint for its picture `picture`, when the coloring in
/// force tints it: end of retail's pictures alone, the older files' being their own.
fn classic_tint(
    store: &RetailDatStore,
    terrain_type: u32,
    picture: DataId,
) -> Option<crate::ground_tint::Tint> {
    crate::ground_tint::tint_for(crate::ground_tint::coloring(), terrain_type)
        .filter(|_| store.era_of(picture) == pyreal_dat::ContainerEra::Modern)
}

/// The mean sRGB colour of `picture` in `tint`.
fn tinted_average(
    src: &TextureSource,
    picture: DataId,
    tint: crate::ground_tint::Tint,
) -> Option<[u8; 3]> {
    let px = src
        .decode_with(picture, |w, h, bgra| {
            crate::ground_tint::apply(tint, w, h, bgra)
        })
        .ok()?;
    let texels = px.levels.first()?.as_chunks::<4>().0;
    let mut sum = [0u64; 3];
    for t in texels {
        for (s, c) in sum.iter_mut().zip([t[2], t[1], t[0]]) {
            *s += u64::from(c);
        }
    }
    let n = (texels.len() as u64).max(1);
    Some(sum.map(|s| u8::try_from(s / n).unwrap_or(u8::MAX)))
}

/// A plain colour for ground the dats give no single picture (the older dats shift their
/// ground's palette per cell).
fn kind_colour(kind: SurfaceKind) -> [u8; 3] {
    match kind {
        SurfaceKind::Water => [40, 70, 90],
        SurfaceKind::Snow => [220, 225, 230],
        SurfaceKind::Stone | SurfaceKind::Molten => [110, 105, 95],
        SurfaceKind::Foliage => [80, 100, 45],
        _ => [115, 100, 70],
    }
}

/// How far the far trees reach past the loaded blocks at their full count, blocks.
pub const FAR_TREE_BLOCKS: i32 = 8;
/// The most rings of far trees, thinned past [`FAR_TREE_BLOCKS`] ([`far_tree_keep`]): about the
/// old presentation's six kilometres.
pub const FAR_TREE_MOST: i32 = 24;

/// The rings of far trees past a square of `radius` blocks with far terrain to `far`: as many as
/// the far terrain reaches, at least [`FAR_TREE_BLOCKS`] and a third of it, at most
/// [`FAR_TREE_MOST`].
#[must_use]
pub fn far_tree_rings(radius: i32, far: i32) -> i32 {
    let past = far - radius;
    past.min(FAR_TREE_BLOCKS.max(past / 3)).min(FAR_TREE_MOST)
}

/// The share of a far block's trees kept `ring` blocks past the loaded square: all within
/// [`FAR_TREE_BLOCKS`], thinning past it with the square of the distance, so each ring holds
/// about as many trees on the screen.
#[must_use]
pub fn far_tree_keep(ring: i32) -> f32 {
    if ring <= FAR_TREE_BLOCKS {
        1.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        let k = FAR_TREE_BLOCKS as f32 / ring as f32;
        k * k
    }
}
/// How far a far tree is sunk into the ground, metres: the far rings' coarser heights never
/// leave its foot floating.
const FAR_TREE_SINK: f32 = 1.5;

/// What a block's planning reads beyond the dats: the height table, the rooms' decoder and a
/// memo of the objects' outlines and reaches. One per thread that plans.
pub struct Planner {
    table: [f32; 256],
    cells: EnvCellLoader,
    // ORDER-OK: memos, looked up by id and never iterated.
    shapes: std::cell::RefCell<HashMap<u32, Option<WithinBlockShape>>>,
    reaches: HashMap<u32, f32>,
    walls: HashMap<u32, Option<(glam::Vec2, glam::Vec2)>>,
    grounds: HashMap<u32, Option<f32>>,
    /// Landscape Smoothing: each block's drawn offset worked out, with its knobs and a memo of
    /// the models' footprints.
    smooth: Option<pyreal_terrain::land::drawn::Knobs>,
    footprints: HashMap<u32, Option<[f32; 4]>>,
}

impl std::fmt::Debug for Planner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Planner").finish_non_exhaustive()
    }
}

/// Records `around` with their terrain words as drawn ([`drawn_words`]).
fn drawn_around(
    around: &[((i32, i32), Option<CellLandblock>)],
) -> Vec<((i32, i32), Option<CellLandblock>)> {
    around
        .iter()
        .map(|(b, r)| {
            let r = r.as_ref().map(|lb| CellLandblock {
                terrain: drawn_words(&lb.terrain, &lb.height),
                ..lb.clone()
            });
            (*b, r)
        })
        .collect()
}

/// One block's records, decoded and worked out with no device: what placing it takes.
#[derive(Debug, Clone)]
pub struct BlockPlan {
    pub block: (i32, i32),
    pub record: Option<CellLandblock>,
    /// Its own record and its eight neighbours', for the ground's normals across its edges.
    pub around: Vec<((i32, i32), Option<CellLandblock>)>,
    pub info: Option<LandblockInfo>,
    /// Its rooms, and how many it promised that would not load.
    pub cells: Vec<DecodedCell>,
    pub cell_failures: u64,
    /// Its generated scenery: each object and where it stands in the block.
    pub scenery: Vec<(DataId, Affine3A)>,
    /// Each building's draw reach, in its landblock information's order.
    pub reaches: Vec<f32>,
    /// What the land field takes from it.
    pub land: Option<crate::land::BlockLand>,
    /// Its drawn offset, with Landscape Smoothing.
    pub drawn: Option<Arc<pyreal_terrain::land::drawn::BlockField>>,
}

impl Planner {
    #[must_use]
    pub fn new(region: &Region) -> Self {
        Self {
            table: height_table(region),
            cells: EnvCellLoader::new(),
            shapes: std::cell::RefCell::new(HashMap::new()),
            reaches: HashMap::new(),
            walls: HashMap::new(),
            grounds: HashMap::new(),
            smooth: None,
            footprints: HashMap::new(),
        }
    }

    /// The same, with Landscape Smoothing at strength `level` (0 off, 1 Low, 2 Medium, 3 High):
    /// each block's drawn offset worked out with it.
    #[must_use]
    pub fn smoothing(mut self, level: u8) -> Self {
        self.smooth = pyreal_terrain::land::drawn::Knobs::smoothing(level);
        self
    }

    /// Block `(bx, by)`'s drawn offset from its records `around` and the buildings and statics
    /// about it (`info` its own).
    fn drawn_field(
        &mut self,
        store: &RetailDatStore,
        (bx, by): (i32, i32),
        around: &[((i32, i32), Option<CellLandblock>)],
        info: Option<&LandblockInfo>,
    ) -> Option<pyreal_terrain::land::drawn::BlockField> {
        let knobs = self.smooth.as_ref()?;
        let infos: Vec<((i32, i32), Option<LandblockInfo>)> = around
            .iter()
            .map(|(b, _)| {
                let i = if *b == (bx, by) {
                    info.cloned()
                } else if (0..=0xFE).contains(&b.0) && (0..=0xFE).contains(&b.1) {
                    decode::<LandblockInfo>(store, DbType::Lbi, lbi_did(block_id(b.0, b.1)))
                } else {
                    None
                };
                (*b, i)
            })
            .collect();
        let pins = crate::drawn::pins(store, (bx, by), &infos, &mut self.footprints, knobs);
        let src = crate::drawn::Around {
            records: around,
            table: &self.table,
        };
        Some(crate::drawn::block_field(&src, (bx, by), &pins, knobs))
    }

    /// Block `(bx, by)`'s plan.
    pub fn block(
        &mut self,
        store: &RetailDatStore,
        region: &Region,
        (bx, by): (i32, i32),
    ) -> BlockPlan {
        let id = block_id(bx, by);
        let record = decode::<CellLandblock>(store, DbType::LandBlock, landblock_did(id));
        let mut around = Vec::with_capacity(9);
        for dx in -1..=1 {
            for dy in -1..=1 {
                let (nx, ny) = (bx + dx, by + dy);
                let r = if (dx, dy) == (0, 0) {
                    record.clone()
                } else if (0..=0xFE).contains(&nx) && (0..=0xFE).contains(&ny) {
                    decode::<CellLandblock>(
                        store,
                        DbType::LandBlock,
                        landblock_did(block_id(nx, ny)),
                    )
                } else {
                    None
                };
                around.push(((nx, ny), r));
            }
        }
        let info = decode::<LandblockInfo>(store, DbType::Lbi, lbi_did(id));
        let before = self.cells.stats;
        let cells = self.cells.load_block(store, id);
        let after = self.cells.stats;
        let cell_failures = (after.missing + after.undecodable + after.no_environment)
            - (before.missing + before.undecodable + before.no_environment);
        let reaches = info
            .iter()
            .flat_map(|i| &i.buildings)
            .map(|b| {
                *self
                    .reaches
                    .entry(b.id.0)
                    .or_insert_with(|| draw_reach(store, b.id))
            })
            .collect();
        let mut scenery = record
            .as_ref()
            .map(|lb| self.scenery_of(store, region, lb, info.as_ref(), (bx, by)))
            .unwrap_or_default();
        // What the statics and the scenery clear of cover, the scenery where it grew.
        let statics = info
            .iter()
            .flat_map(|i| &i.objects)
            .map(|o| (o.id, object_affine(&o.frame, 1.0)));
        let grown = scenery.iter().map(|(id, at)| (*id, *at));
        let clearings = statics
            .chain(grown)
            .filter_map(|(id, at)| {
                let r = (*self
                    .grounds
                    .entry(id.0)
                    .or_insert_with(|| ground_radius(store, id)))?;
                let r = r * at.x_axis.length();
                (crate::cover::CLEARING_LEAST..=crate::cover::CLEARING_MOST)
                    .contains(&r)
                    .then(|| Vec3::from(at.translation).extend(r))
            })
            .collect();
        let drawn = record
            .as_ref()
            .and_then(|_| self.drawn_field(store, (bx, by), &drawn_around(&around), info.as_ref()))
            .map(Arc::new);
        if let (Some(field), Some(knobs)) = (drawn.as_ref(), self.smooth.as_ref()) {
            let src = crate::drawn::Around {
                records: &around,
                table: &self.table,
            };
            for (_, at) in &mut scenery {
                let foot = Vec3::from(at.translation);
                at.translation.z += crate::drawn::scenery_lift(field, &src, (bx, by), foot, knobs);
            }
        }
        let footprints = info
            .iter()
            .flat_map(|i| &i.buildings)
            .filter_map(|b| {
                let (low, high) = (*self
                    .walls
                    .entry(b.id.0)
                    .or_insert_with(|| walls_of(store, b.id)))?;
                let x = object_affine(&b.frame, 1.0).x_axis;
                let axis = glam::Vec2::new(x.x, x.y).normalize_or(glam::Vec2::X);
                Some(crate::land::Footprint {
                    at: glam::Vec2::new(b.frame.origin.x, b.frame.origin.y),
                    axis,
                    low,
                    high,
                })
            })
            .collect();
        let land = record.as_ref().map(|lb| crate::land::BlockLand {
            footprints,
            clearings,
            block: (bx, by),
            heights: std::array::from_fn(|i| self.table[usize::from(lb.height[i])]),
            terrain: drawn_words(&lb.terrain, &lb.height),
            buildings: info
                .iter()
                .flat_map(|i| &i.buildings)
                .map(|b| {
                    let cell = |v: f32| pyreal_primitives::num::floor_to_i32(v / 24.0).clamp(0, 7);
                    usize::try_from(cell(b.frame.origin.x) * 8 + cell(b.frame.origin.y))
                        .unwrap_or(0)
                })
                .collect(),
            drawn: drawn
                .as_deref()
                .map(|f| Arc::new(crate::drawn::vertex_offsets(f))),
        });
        BlockPlan {
            block: (bx, by),
            record,
            around,
            info,
            cells,
            cell_failures,
            scenery,
            reaches,
            land,
            drawn,
        }
    }

    /// Block `(bx, by)`'s generated scenery alone (the far trees'): each object and where it
    /// stands in the block.
    /// Block `(bx, by)`'s scenery for the far trees: each object's foot lowered to the far
    /// terrain where its coarser `levels` ([`crate::far::levels_near`]) cut the ground under it.
    pub fn far_trees(
        &mut self,
        store: &RetailDatStore,
        region: &Region,
        (bx, by): (i32, i32),
        levels: std::ops::RangeInclusive<u32>,
    ) -> Vec<(DataId, Affine3A)> {
        let id = block_id(bx, by);
        let Some(lb) = decode::<CellLandblock>(store, DbType::LandBlock, landblock_did(id)) else {
            return Vec::new();
        };
        let info = decode::<LandblockInfo>(store, DbType::Lbi, lbi_did(id));
        let mut out = self.scenery_of(store, region, &lb, info.as_ref(), (bx, by));
        let h = |x: usize, y: usize| self.table[usize::from(lb.height[x.min(8) * 9 + y.min(8)])];
        for (_, at) in &mut out {
            let (x, y) = (at.translation.x, at.translation.y);
            // A level's vertices are every 2^k-th of the block's, from its corner.
            for k in levels.clone() {
                let s = 1usize << k.min(3);
                #[allow(clippy::cast_precision_loss)]
                let cell = 24.0 * s as f32;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (gx, gy) = (
                    ((x / cell).floor().max(0.0) as usize * s).min(8 - s),
                    ((y / cell).floor().max(0.0) as usize * s).min(8 - s),
                );
                #[allow(clippy::cast_precision_loss)]
                let (fx, fy) = (
                    ((x - 24.0 * gx as f32) / cell).clamp(0.0, 1.0),
                    ((y - 24.0 * gy as f32) / cell).clamp(0.0, 1.0),
                );
                let mut low = h(gx, gy) * (1.0 - fx) * (1.0 - fy)
                    + h(gx + s, gy) * fx * (1.0 - fy)
                    + h(gx, gy + s) * (1.0 - fx) * fy
                    + h(gx + s, gy + s) * fx * fy;
                if self.smooth.is_some() {
                    // Landscape Smoothing's far rings: the Catmull-Rom surface (far.wgsl), from
                    // this block's vertices.
                    let (wx, wy) = (catmull(fx), catmull(fy));
                    let idx = |g: usize, k: usize| (g + k * s).saturating_sub(s).min(8);
                    let mut c = 0.0;
                    for (b, wb) in wy.iter().enumerate() {
                        for (a, wa) in wx.iter().enumerate() {
                            c += wa * wb * h(idx(gx, a), idx(gy, b));
                        }
                    }
                    low = low.min(c);
                }
                at.translation.z = at.translation.z.min(low);
            }
        }
        out
    }

    pub fn scenery(
        &mut self,
        store: &RetailDatStore,
        region: &Region,
        (bx, by): (i32, i32),
    ) -> Vec<(DataId, Affine3A)> {
        let id = block_id(bx, by);
        let Some(lb) = decode::<CellLandblock>(store, DbType::LandBlock, landblock_did(id)) else {
            return Vec::new();
        };
        let info = decode::<LandblockInfo>(store, DbType::Lbi, lbi_did(id));
        self.scenery_of(store, region, &lb, info.as_ref(), (bx, by))
    }

    fn scenery_of(
        &self,
        store: &RetailDatStore,
        region: &Region,
        lb: &CellLandblock,
        info: Option<&LandblockInfo>,
        (bx, by): (i32, i32),
    ) -> Vec<(DataId, Affine3A)> {
        let land = pyreal_terrain::land::mesh::generate_landblock_with_table(
            lb,
            region,
            &self.table,
            bx,
            by,
            1,
            pyreal_terrain::land::mesh::Direction::InViewerBlock,
        );
        let building_cells: Vec<u16> = info
            .iter()
            .flat_map(|i| &i.buildings)
            .map(|b| outside_cell_index(b.frame.origin.x, b.frame.origin.y))
            .collect();
        let env = SceneryEnv {
            scenes: &|id| decode::<SceneRecord>(store, DbType::Scene, id),
            has_building: &|cell| building_cells.contains(&cell),
            shape: &|id| {
                self.shapes
                    .borrow_mut()
                    .entry(id.0)
                    .or_insert_with(|| within_block_shape(store, id))
                    .clone()
            },
        };
        generate_scenery(lb, &land, region, bx, by, &env)
            .into_iter()
            .map(|p| (p.gfxobj, object_affine(&p.frame, p.scale)))
            .collect()
    }
}

/// Catmull-Rom weights at `t`.
fn catmull(t: f32) -> [f32; 4] {
    let (t2, t3) = (t * t, t * t * t);
    [
        0.5 * (-t3 + 2.0 * t2 - t),
        0.5 * (3.0 * t3 - 5.0 * t2 + 2.0),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2),
    ]
}

/// The pack's trees of one far block as cards alone (each scenery object the pack has a level of
/// pictures for), `scenery` as [`Planner::scenery`] gives it, the block's corner at `origin`; the
/// instances it placed. A share `keep` of them stands (the same ones on every visit), each wider
/// by the fourth root of what was left out.
pub fn place_far_trees(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    scenery: &[(DataId, Affine3A)],
    origin: Vec3,
    keep: f32,
) -> usize {
    if loader.pack.is_none() {
        return 0;
    }
    let at_block = Affine3A::from_translation(origin - Vec3::Z * FAR_TREE_SINK);
    let wider = Affine3A::from_scale(Vec3::splat(1.0 / keep.clamp(0.01, 1.0).sqrt().sqrt()));
    let mut placed = 0;
    for (gfxobj, local) in scenery {
        if keep < 1.0 && far_tree_hash(local.translation.truncate()) >= keep {
            continue;
        }
        let local = &(*local * wider);
        let Some(card) = loader.far_card(engine, *gfxobj) else {
            continue;
        };
        let object = loader.next_object();
        if loader.add_instance(
            engine,
            Instance::at(card, at_block * *local, object, OUTSIDE),
        ) {
            placed += 1;
        }
    }
    loader.stats.instances += placed;
    placed
}

/// A far tree's lot by where it stands in its block (to the decimetre), in `[0, 1)`.
fn far_tree_hash(at: glam::Vec2) -> f32 {
    let (x, y) = (
        pyreal_primitives::num::to_i32((at.x * 10.0).round()),
        pyreal_primitives::num::to_i32((at.y * 10.0).round()),
    );
    #[allow(clippy::cast_sign_loss)]
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343) ^ (y as u32).wrapping_mul(0xD816_3841);
    h = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    #[allow(clippy::cast_precision_loss)]
    let v = (h >> 8) as f32 / 16_777_216.0;
    v
}

/// The pack's trees on the far rings: for each block up to `rings` blocks past `area`, retail's
/// scenery generated as the client generates it, each object the pack has a level of pictures for
/// placed as that card alone, its foot on the far terrain whose first level is drawn to
/// `first_range` metres. The near blocks keep their own.
pub fn load_far_trees(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    region: &Region,
    area: Area,
    rings: i32,
    first_range: f32,
) -> usize {
    if loader.pack.is_none() || rings <= 0 {
        return 0;
    }
    let mut planner = Planner::new(region);
    let (cx, cy) = area.center_block();
    let outer = area.radius + rings;
    let mut placed = 0;
    for bx in cx - outer..=cx + outer {
        for by in cy - outer..=cy + outer {
            let near = (bx - cx).abs() <= area.radius && (by - cy).abs() <= area.radius;
            if near || !(0..=0xFE).contains(&bx) || !(0..=0xFE).contains(&by) {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let d = glam::Vec2::new((bx - cx) as f32, (by - cy) as f32).length() * 192.0;
            let levels = crate::far::levels_near(first_range, d, 192.0);
            let scenery = planner.far_trees(store, region, (bx, by), levels);
            let ring = (bx - cx).abs().max((by - cy).abs()) - area.radius;
            placed += place_far_trees(
                engine,
                loader,
                &scenery,
                area.block_origin(bx, by),
                far_tree_keep(ring),
            );
        }
    }
    placed
}

/// The pack's ground cover kinds made ready for the cover pass: each kind's three levels of shape
/// in the pools, and a card baked from its near level (sixteen views round it) for the distance.
/// The roads it keeps off are `region`'s road alpha maps, read from `store`.
pub fn load_cover(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    region: &Region,
) {
    let Some(pack) = loader.pack.as_ref().map(Arc::clone) else {
        return;
    };
    let mut kinds = Vec::new();
    for (k, c) in pack.pack.ground_cover.iter().enumerate() {
        let Some(model) = pack.model(c.id) else {
            continue;
        };
        let shapes: Vec<&crate::pack::PackLod> = model
            .lods
            .iter()
            .filter(|l| l.views.is_none() && !l.shadows_only)
            .take(crate::cover::LEVELS - 1)
            .collect();
        let Some(near) = shapes.first() else {
            continue;
        };
        let mut draws: [Vec<(u32, u32, u32)>; crate::cover::LEVELS] = Default::default();
        let mut ends = [0.0; crate::cover::LEVELS];
        let mut last = 0.0f32;
        for l in 0..crate::cover::LEVELS - 1 {
            // A kind with fewer levels keeps its last one on.
            let lod = shapes.get(l).copied().unwrap_or(shapes[shapes.len() - 1]);
            last = if lod.distance.is_finite() {
                lod.distance
            } else {
                last.max(10.0) * 1.5
            };
            ends[l] = last;
            if l < shapes.len() {
                draws[l] = loader
                    .cover_level(engine, lod, model.foliage, None)
                    .unwrap_or_default();
            } else {
                draws[l] = draws[l - 1].clone();
            }
        }
        ends[crate::cover::LEVELS - 1] = last * crate::cover::CARD_REACH;
        let mut albedo = 0.0;
        if let Some(card) = bake_card(&pack, near, model.foliage) {
            albedo = crate::pack::image_mean(&card.colour, 128)
                .map_or(0.0, |[r, g, b]| 0.2126 * r + 0.7152 * g + 0.0722 * b);
            let colour = crate::pack::image_pixels(&card.colour, true)
                .and_then(|p| engine.materials.add_texture(&engine.gpu, &p).ok());
            let normal = crate::pack::image_pixels(&card.normal, false)
                .and_then(|p| engine.materials.add_texture(&engine.gpu, &p).ok());
            for t in [colour, normal].into_iter().flatten() {
                engine.materials.pin_texture(t);
            }
            if let (Some(colour), Some(normal)) = (colour, normal) {
                let mut lod = card.lod;
                // Names the cache keeps for the baked pictures, past any the pack has.
                let at = u32::MAX - 2 * u32::try_from(k).unwrap_or(0) - 1;
                for p in &mut lod.primitives {
                    p.material.base_color_image = Some(at);
                    p.material.normal_image = Some(at - 1);
                }
                draws[crate::cover::LEVELS - 1] = loader
                    .cover_level(engine, &lod, model.foliage, Some((colour, normal)))
                    .unwrap_or_default();
            }
        }
        let (centre, radius) = model.bounds;
        kinds.push(crate::cover::CoverKind {
            id: c.id.0,
            grounds: c.grounds,
            density: c.density,
            scale: c.scale,
            steepest: c.steepest,
            height: (centre[2] + radius).max(0.05),
            radius: radius + (centre[0] * centre[0] + centre[1] * centre[1]).sqrt(),
            wind: model.wind,
            albedo,
            ends,
            draws,
        });
    }
    if !kinds.is_empty() {
        let source = TextureSource::new(store);
        let roads = crate::cover::road_fields(region.land_surf.tex_merge.as_ref(), &mut |id| {
            source.alpha(id, false).ok()
        });
        let mut cover =
            crate::cover::Cover::new(&engine.gpu, &engine.renderer.cover_pipelines, kinds, &roads);
        // The grounds whose pack material keeps the pebbles off, as the world's region lays them.
        let bare = |ground: pyreal_assets::art_pack::Ground, terrain_type: u32| {
            let record = region
                .land_surf
                .tex_merge
                .as_ref()
                .and_then(|tm| get_terrain_tex(tm, terrain_type))
                .and_then(|d| loader.print_of(Some(store), d.tex_gid));
            pack.ground_index(ground, false, record.as_ref())
                .is_some_and(|i| !pack.pack.grounds[i].pebbles)
        };
        let types = (0u8..32)
            .filter(|&t| bare(pyreal_assets::art_pack::Ground::Type(t), u32::from(t)))
            .fold(0u32, |m, t| m | 1 << t);
        let road = bare(pyreal_assets::art_pack::Ground::Road, ROAD_TERRAIN_TYPE);
        cover.keep_pebbles_off(types, road);
        if let Some((slot, per_m)) = stone_picture(engine, loader, store, region) {
            cover.set_stone_picture(slot, per_m);
        }
        for (t, c) in (0u32..).zip(&loader.ground_colours) {
            if let Some(c) = c {
                cover.set_ground(&engine.gpu, t, *c);
            }
        }
        engine.renderer.set_cover(&engine.gpu, cover);
    }
}

/// The picture the cover's stones are drawn with: the region's bare rock ground as the land draws
/// it (the pack's where it has one), pinned with its companions, and its repeats a metre.
fn stone_picture(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    region: &Region,
) -> Option<(u32, f32)> {
    pinned_ground(engine, loader, store, region, "BarrenRock", 0)
}

/// Gives the bed under water drawn as water the region's mud ground as the land draws it (the
/// pack's where it has one), once.
pub fn set_bed_picture(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    region: &Region,
) {
    if engine.renderer.bed.is_none() {
        engine.renderer.bed = pinned_ground(engine, loader, store, region, "MudRichDirt", 5);
    }
}

/// The region's ground named `name` (terrain type `fallback` where it names none) as the land
/// draws it, pinned with its companions so eviction never frees it, and its repeats a metre.
fn pinned_ground(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    region: &Region,
    name: &str,
    fallback: usize,
) -> Option<(u32, f32)> {
    let terrain_type = region
        .terrain_types
        .iter()
        .position(|t| t.terrain_name.eq_ignore_ascii_case(name))
        .unwrap_or(fallback);
    let later = loader.later_ground(store);
    let (slot, tiling, _) = Ground {
        loader,
        engine: &mut *engine,
        store,
        region,
        later: later.as_deref(),
    }
    .ground(u32::try_from(terrain_type).ok()?)?;
    let none = crate::asset::material::NO_TEXTURE;
    let companions = engine.materials.companions(slot).unwrap_or([none; 8]);
    for t in [slot, companions[0], companions[1]] {
        if t != none {
            engine.materials.pin_texture(t);
        }
    }
    // A layer's tiling: repeats a cell, or (below nought) metres a repeat.
    let per_m = if tiling >= 0.0 {
        tiling / CELL_SIZE
    } else {
        -1.0 / tiling
    };
    Some((slot, per_m))
}

/// A card of a pack level, drawn from round it with its base colour pictures (`pack`'s).
fn bake_card(
    pack: &EnginePack,
    lod: &crate::pack::PackLod,
    foliage: bool,
) -> Option<pyreal_art_pack::cards::Card> {
    use pyreal_art_pack::cards::{bake_parts, Part, Texels};
    let pictures: Vec<Option<Texels>> = lod
        .primitives
        .iter()
        .map(|p| {
            let image = pack
                .pack
                .images
                .get(p.material.base_color_image? as usize)?;
            let level = image.levels.first()?;
            let rgba = match image.format {
                Some(f) => pyreal_codecs::block_codec::decode(f, image.width, image.height, level),
                None => level.to_vec(),
            };
            Texels::new(image.width, image.height, rgba)
        })
        .collect();
    let parts: Vec<Part<'_>> = lod
        .primitives
        .iter()
        .zip(&pictures)
        .map(|(p, t)| Part {
            vertices: &p.vertices,
            indices: &p.indices,
            picture: t.as_ref(),
            base_color: p.material.base_color,
            cut: p.material.alpha_cutoff,
            double_sided: p.material.double_sided,
            roughness: p.material.roughness,
        })
        .collect();
    bake_parts(&parts, foliage)
}

/// A block's ground as it was placed: its mesh (drawn once an instance is made of it), its cells'
/// records in the terrain cell pool, and each cell's box and list of the room volumes cutting it.
#[derive(Debug, Clone)]
pub struct PlacedGround {
    pub mesh: MeshId,
    pub transform: Affine3A,
    pub cells_first: u32,
    pub records: Vec<crate::terrain::CellRecord>,
    pub boxes: Vec<(Vec3, Vec3)>,
}

impl PlacedGround {
    /// The block's ground as an instance.
    #[must_use]
    pub fn instance(&self) -> Instance {
        Instance::at(self.mesh, self.transform, u32::MAX, OUTSIDE)
    }

    /// Its box.
    #[must_use]
    pub fn bounds(&self) -> (Vec3, Vec3) {
        self.boxes
            .iter()
            .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), b| {
                (lo.min(b.0), hi.max(b.1))
            })
    }

    /// The textures its cells lay.
    pub fn textures(&self) -> impl Iterator<Item = u32> + '_ {
        self.records
            .iter()
            .flat_map(|r| &r.layers)
            .flat_map(|l| [l.texture, l.alpha])
    }

    /// Moves it by `delta`.
    pub fn translate(&mut self, delta: Vec3) {
        self.transform.translation += glam::Vec3A::from(delta);
        for b in &mut self.boxes {
            b.0 += delta;
            b.1 += delta;
        }
    }

    /// Frees its cut lists, its cell records and its mesh (its instance, if any, is gone).
    pub fn free(&self, engine: &mut Engine) {
        for r in &self.records {
            if r.cut_count > 0 {
                engine
                    .scene
                    .cut_lists
                    .free(u64::from(r.cut_first), u64::from(r.cut_count));
            }
        }
        engine
            .scene
            .free_terrain_cells(self.cells_first, self.records.len() as u64);
        engine.scene.free_mesh(self.mesh);
        engine.renderer.rays.forget(self.mesh);
    }
}

/// Places the rooms of `plan` (a block whose south-west corner is at `origin`) with their
/// furniture, and links their portals; their slots.
pub fn place_rooms(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    plan: &BlockPlan,
    origin: Vec3,
) -> Vec<u32> {
    let mut slots = Vec::with_capacity(plan.cells.len());
    place_rooms_run(
        engine,
        loader,
        store,
        plan,
        origin,
        0..plan.cells.len(),
        &mut slots,
    );
    slots
}

/// [`place_rooms`] a run at a time: rooms `range`, their slots added to `slots`; the portals are
/// linked once the last is placed.
pub fn place_rooms_run(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    plan: &BlockPlan,
    origin: Vec3,
    range: std::ops::Range<usize>,
    slots: &mut Vec<u32>,
) {
    let last = range.end >= plan.cells.len();
    for d in plan.cells.get(range).unwrap_or_default() {
        slots.push(loader.place_cell(engine, store, d, origin));
    }
    if last {
        engine.cells.link_slots(slots);
        loader.stats.cell_failures += plan.cell_failures;
    }
}

/// Takes the rooms in `slots` out of the cell graph and their volumes out of the cut pool; the
/// boxes of the volumes taken.
pub fn remove_rooms(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    slots: &[u32],
) -> Vec<(Vec3, Vec3)> {
    let gone = loader.remove_cuts(engine, slots);
    for &s in slots {
        engine.cells.remove(s);
    }
    gone
}

/// Builds `plan`'s ground (a block whose south-west corner is at `origin`): its mesh in the pools
/// and its cells' records, cut by the room volumes placed so far. No instance is made of it yet.
pub fn place_ground(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    region: &Region,
    plan: &BlockPlan,
    origin: Vec3,
) -> Option<PlacedGround> {
    let lb = plan.record.as_ref()?;
    let table = height_table(region);
    let (bx, by) = plan.block;
    let heights = |gx: i32, gy: i32| -> Option<f32> {
        let (nbx, nby) = (gx.div_euclid(8), gy.div_euclid(8));
        let (vx, vy) = (gx.rem_euclid(8), gy.rem_euclid(8));
        let (_, r) = plan.around.iter().find(|(k, _)| *k == (nbx, nby))?;
        let r = r.as_ref()?;
        let i = usize::try_from(vx * 9 + vy).ok()?;
        Some(table[usize::from(r.height[i])])
    };
    let material = loader.terrain_material(engine)?;
    let ground = {
        let later = loader.later_ground(store);
        let mut g = Ground {
            loader: &mut *loader,
            engine: &mut *engine,
            store,
            region,
            later: later.as_deref(),
        };
        build_block(
            lb,
            region,
            &table,
            bx,
            by,
            &heights,
            0,
            &mut g,
            plan.drawn.as_deref(),
        )
    };
    let boxes: Vec<(Vec3, Vec3)> = ground
        .cell_vertices
        .iter()
        .map(|r| {
            ground
                .mesh
                .vertices
                .get(r.clone())
                .unwrap_or_default()
                .iter()
                .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), v| {
                    let p = origin + Vec3::from(v.position);
                    (lo.min(p), hi.max(p))
                })
        })
        .collect();
    let mut records = ground.cells.clone();
    for r in &mut records {
        r.cut_first = 0;
        r.cut_count = 0;
    }
    let mut cell_bytes = Vec::new();
    for c in &records {
        c.write(&mut cell_bytes);
    }
    let first = engine
        .scene
        .add_terrain_cells(&engine.gpu, &cell_bytes)
        .ok()?;
    let mut mesh = ground.mesh.clone();
    for v in &mut mesh.vertices {
        v.extra[0] += first;
    }
    let Ok(id) = engine.scene.add_mesh(&engine.gpu, &mesh, |_| {
        Some((material, PipelineKind::Terrain))
    }) else {
        engine.scene.free_terrain_cells(first, records.len() as u64);
        return None;
    };
    let mut placed = PlacedGround {
        mesh: id,
        transform: Affine3A::from_translation(origin),
        cells_first: first,
        records,
        boxes,
    };
    cut_ground(engine, loader, &mut placed);
    loader.stats.blocks += 1;
    Some(placed)
}

/// Works out again which room volumes cut each of `ground`'s cells (rooms came or went), and
/// writes its records.
pub fn cut_ground(engine: &mut Engine, loader: &mut AssetLoader, ground: &mut PlacedGround) {
    for r in &mut ground.records {
        if r.cut_count > 0 {
            engine
                .scene
                .cut_lists
                .free(u64::from(r.cut_first), u64::from(r.cut_count));
        }
        r.cut_first = 0;
        r.cut_count = 0;
    }
    for (rec, (lo, hi)) in ground.records.iter_mut().zip(&ground.boxes) {
        let list = loader.cuts_over(*lo - 0.1, *hi + 0.1);
        if list.is_empty() {
            continue;
        }
        if let Ok(first) = engine.scene.add_cut_list(&engine.gpu, &list) {
            rec.cut_first = first;
            rec.cut_count = u32::try_from(list.len()).unwrap_or(0);
            loader.stats.cut_cells += 1;
        }
    }
    let mut bytes = Vec::new();
    for c in &ground.records {
        c.write(&mut bytes);
    }
    engine
        .scene
        .terrain_cells
        .write(&engine.gpu.queue, u64::from(ground.cells_first), &bytes);
}

/// Places `plan`'s statics and buildings' shells (a block whose south-west corner is at
/// `origin`), limiting how far its rooms (`rooms`, placed first) are looked into to their
/// buildings' reach.
pub fn place_statics(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    plan: &BlockPlan,
    origin: Vec3,
    rooms: &[u32],
) {
    place_statics_run(
        engine,
        loader,
        store,
        plan,
        origin,
        rooms,
        0..plan.statics(),
    );
}

impl BlockPlan {
    /// Its statics and buildings, together.
    #[must_use]
    pub fn statics(&self) -> usize {
        self.info
            .as_ref()
            .map_or(0, |i| i.objects.len() + i.buildings.len())
    }
}

/// [`place_statics`] a run at a time: statics `range` of [`BlockPlan::statics`] (its objects,
/// then its buildings); the rooms' reaches are written once the last is placed.
pub fn place_statics_run(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    plan: &BlockPlan,
    origin: Vec3,
    rooms: &[u32],
    range: std::ops::Range<usize>,
) {
    let Some(info) = &plan.info else {
        return;
    };
    let last = range.end >= plan.statics();
    let at_block = Affine3A::from_translation(origin);
    let base = u32::from(block_id(plan.block.0, plan.block.1)) << 16;
    let buildings: Vec<_> = info
        .buildings
        .iter()
        .map(|b| (b.id.0, b.frame.origin))
        .collect();
    for i in range {
        if let Some(o) = info.objects.get(i) {
            let hosts = engine.particles.hosts.len();
            loader.place_object(
                engine,
                store,
                o.id,
                at_block * object_affine(&o.frame, 1.0),
                false,
            );
            if let Some(part) = crate::beacons::is_beacon(o.id.0, o.frame.origin, &buildings) {
                for h in engine.particles.hosts.iter_mut().skip(hosts) {
                    h.beacon = Some(part);
                }
            }
            loader.stats.statics += 1;
            continue;
        }
        let k = i - info.objects.len();
        let Some(b) = info.buildings.get(k) else {
            continue;
        };
        // Its rooms are looked into only while its shell is drawn.
        let reach = plan.reaches.get(k).copied().unwrap_or(f32::INFINITY);
        for p in &b.portals {
            for &c in p.stab_list.iter().chain(std::iter::once(&p.other_cell_id)) {
                engine.cells.limit_reach(base | u32::from(c), reach);
            }
        }
        loader.place_object(
            engine,
            store,
            b.id,
            at_block * object_affine(&b.frame, 1.0),
            false,
        );
        loader.stats.buildings += 1;
    }
    if last {
        loader.write_cut_reaches(engine, Some(rooms));
    }
}

/// The forges and fire pits among a room's `objects` (each placed where), for
/// [`crate::flames::firebox_of`].
fn fireboxes_of(objects: impl Iterator<Item = (DataId, Affine3A)>) -> Vec<(u32, Affine3A)> {
    objects
        .filter(|(id, _)| crate::flames::holds_fire(id.0))
        .map(|(id, at)| (id.0, at))
        .collect()
}

/// Places `plan`'s scenery `range` (a block whose south-west corner is at `origin`).
pub fn place_scenery(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    plan: &BlockPlan,
    origin: Vec3,
    range: std::ops::Range<usize>,
) {
    let at_block = Affine3A::from_translation(origin);
    for (gfxobj, local) in plan.scenery.get(range).unwrap_or_default() {
        loader.place_object(engine, store, *gfxobj, at_block * *local, true);
        loader.stats.scenery += 1;
    }
}

/// The land field of the blocks `lands` lay, `blocks` blocks a side from block `first`, its first
/// vertex at `origin` in render space, with its water and shore field worked out.
#[must_use]
pub fn land_field(
    lands: &[&crate::land::BlockLand],
    first: (i32, i32),
    blocks: usize,
    origin: glam::Vec2,
) -> crate::land::LandField {
    let mut land = crate::land::LandField::new(first, (blocks, blocks), origin);
    for b in lands {
        land.add_block(b);
    }
    land.finish(&crate::water::WaterTable::default());
    land.shore = Some(std::sync::Arc::new(crate::shore::ShoreField::build(&land)));
    land
}

/// Puts `land` on the GPU beside the ground: the water's surface, the cover's and the wet ground's
/// reading of it.
pub fn set_land(engine: &mut Engine, land: crate::land::LandField) {
    engine.renderer.set_land(&engine.gpu, &land);
    engine.land = Some(land);
}

/// Places `area` into `engine`: its rooms first, then each block's ground with the rooms' volumes
/// cut out of it, its statics and buildings' shells from its landblock information, and its
/// generated scenery; then the land field, the cover and the ground's material.
pub fn load_area(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    region: &Region,
    area: Area,
) {
    let mut planner = Planner::new(region);
    let plans: Vec<BlockPlan> = area
        .blocks()
        .into_iter()
        .map(|b| planner.block(store, region, b))
        .collect();
    let rooms: Vec<Vec<u32>> = plans
        .iter()
        .map(|p| {
            place_rooms(
                engine,
                loader,
                store,
                p,
                area.block_origin(p.block.0, p.block.1),
            )
        })
        .collect();
    for (p, rooms) in plans.iter().zip(&rooms) {
        let origin = area.block_origin(p.block.0, p.block.1);
        if let Some(g) = place_ground(engine, loader, store, region, p, origin) {
            let _ = engine.scene.add_instance(g.instance());
        }
        place_statics(engine, loader, store, p, origin, rooms);
        place_scenery(engine, loader, store, p, origin, 0..p.scenery.len());
    }
    let _ = loader.take_placed();
    engine.renderer.world_origin = area.world_origin();
    if engine.renderer.cover.is_none() {
        load_cover(engine, loader, store, region);
    }
    let (cx, cy) = area.center_block();
    let blocks = usize::try_from(area.radius * 2 + 1).unwrap_or(1);
    let lands: Vec<&crate::land::BlockLand> =
        plans.iter().filter_map(|p| p.land.as_ref()).collect();
    let land = land_field(
        &lands,
        (cx - area.radius, cy - area.radius),
        blocks,
        area.block_origin(cx - area.radius, cy - area.radius)
            .truncate(),
    );
    set_land(engine, land);
    if let Some(m) = loader.terrain_material(engine) {
        engine.renderer.terrain_material = m;
    }
    set_bed_picture(engine, loader, store, region);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A part's dressed brightness goes with its freed mesh and with a cleared world, so the memo
    /// stays bounded by what is loaded.
    #[test]
    fn dressed_brightness_is_let_go_with_its_mesh_and_the_world() {
        let mut loader = AssetLoader::default();
        let dress = Dress::default();
        loader.meshes.insert(7, Some(MeshId(70)));
        loader.meshes.insert(8, Some(MeshId(80)));
        loader.lumas.insert((7, dress.clone()), Some((0.5, 1.0)));
        loader.lumas.insert((8, dress.clone()), Some((0.25, 1.0)));
        let none = std::collections::HashSet::new();
        loader.forget(&std::iter::once(70).collect(), &none, &none);
        assert!(
            !loader.lumas.contains_key(&(7, dress.clone())),
            "freed with its mesh"
        );
        assert!(
            loader.lumas.contains_key(&(8, dress.clone())),
            "kept while its mesh is"
        );
        loader.clear_world();
        assert!(loader.lumas.is_empty(), "cleared with the world");
    }
}
