//! Freeing what nothing draws any more: meshes, materials and textures loaded on a journey and
//! left behind with the blocks that placed them.
//!
//! A pass marks what is drawn (every instance's mesh, its degrade chain and the mesh a variant
//! shares its ranges with; the pinned meshes and the particles' pictures; the placed ground's
//! meshes, drawn or still waiting for its square; their materials and the pinned ones; those
//! materials' textures, the ground's pictures and their companions), and frees
//! the rest: pool ranges back to their allocators (merged with their free neighbours), mesh,
//! material and texture slots to their free lists, and the loader's caches of them, so a place
//! visited again loads them again. It runs when the pools pass their [`Budget`].

use std::collections::HashSet;

use crate::asset::material::NO_TEXTURE;
use crate::scene::{MeshId, LOD_NOTHING};
use crate::world::{AssetLoader, PlacedGround};
use crate::Engine;

/// How full the pools may grow before what nothing draws is freed: shares of each pool, and
/// texture bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budget {
    pub vertices: f32,
    pub indices: f32,
    pub meshes: f32,
    pub materials: f32,
    pub textures: f32,
    pub texture_bytes: u64,
}

impl Default for Budget {
    /// A quarter of each pool, and 768 MiB of pictures: room for several squares' worth, so a
    /// place left a little way back is still loaded on return.
    fn default() -> Self {
        Self::share(0.25, 768 << 20)
    }
}

impl Budget {
    /// `share` of every pool and `texture_bytes` of pictures.
    #[must_use]
    pub fn share(share: f32, texture_bytes: u64) -> Self {
        Self {
            vertices: share,
            indices: share,
            meshes: share,
            materials: share,
            textures: share,
            texture_bytes,
        }
    }
}

/// How full the pools are.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PoolUse {
    pub vertices: u64,
    pub vertex_capacity: u64,
    /// The largest free run of vertices and of indices: how fragmented the pools are.
    pub largest_vertex_run: u64,
    pub indices: u64,
    pub index_capacity: u64,
    pub largest_index_run: u64,
    pub meshes: usize,
    pub materials: usize,
    pub textures: usize,
    pub texture_bytes: u64,
    pub instances: u32,
}

impl PoolUse {
    #[must_use]
    pub fn of(engine: &Engine) -> Self {
        let s = &engine.scene;
        Self {
            vertices: s.vertices.used(),
            vertex_capacity: s.vertices.capacity(),
            largest_vertex_run: s.vertices.largest_free(),
            indices: s.indices.used(),
            index_capacity: s.indices.capacity(),
            largest_index_run: s.indices.largest_free(),
            meshes: s.live_meshes().count(),
            materials: engine.materials.live_materials(),
            textures: engine.materials.live_textures(),
            texture_bytes: engine.materials.texture_bytes(),
            instances: s.instance_count(),
        }
    }

    /// Whether any pool is past `budget`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn over(&self, engine: &Engine, budget: &Budget) -> bool {
        let share = |used: u64, cap: u64| used as f32 / cap.max(1) as f32;
        share(self.vertices, self.vertex_capacity) > budget.vertices
            || share(self.indices, self.index_capacity) > budget.indices
            || share(self.meshes as u64, engine.scene.sizes.meshes) > budget.meshes
            || share(
                self.materials as u64,
                u64::from(crate::materials::MAX_MATERIALS),
            ) > budget.materials
            || share(
                self.textures as u64,
                u64::from(crate::materials::MAX_TEXTURES),
            ) > budget.textures
            || self.texture_bytes > budget.texture_bytes
    }
}

/// What a pass freed.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Evicted {
    pub meshes: usize,
    pub materials: usize,
    pub textures: usize,
    pub ms: f64,
}

/// Frees every mesh, material and texture nothing draws (see the module header); `ground` is the
/// placed ground, whose meshes and pictures are kept whether or not its square is drawn yet.
pub fn evict<'g>(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    ground: impl IntoIterator<Item = &'g PlacedGround>,
) -> Evicted {
    let started = std::time::Instant::now();
    let ground: Vec<&PlacedGround> = ground.into_iter().collect();
    // Meshes.
    let mut stack: Vec<u32> = engine
        .scene
        .instances()
        .map(|(_, i)| i.mesh.0)
        .chain(ground.iter().map(|g| g.mesh.0))
        .chain(engine.scene.pinned().map(|m| m.0))
        .chain(
            engine
                .particles
                .sprites
                .values()
                .flatten()
                .map(|s| s.mesh.0),
        )
        .collect();
    let mut meshes: HashSet<u32> = HashSet::new();
    while let Some(m) = stack.pop() {
        if !meshes.insert(m) {
            continue;
        }
        let Some(mesh) = engine.scene.mesh(MeshId(m)) else {
            continue;
        };
        if mesh.lod_next < LOD_NOTHING {
            stack.push(mesh.lod_next);
        }
        if let Some(s) = mesh.shares {
            stack.push(s.0);
        }
    }
    // Materials.
    let (pinned_textures, pinned_materials) = engine.materials.pinned();
    let mut materials: HashSet<u32> = pinned_materials.into_iter().collect();
    for &m in &meshes {
        if let Some(mesh) = engine.scene.mesh(MeshId(m)) {
            materials.extend(mesh.draws.iter().map(|d| d.material));
        }
    }
    for s in engine.particles.sprites.values().flatten() {
        materials.extend(s.draws.iter().map(|d| d.material));
    }
    // Textures, and their companions.
    let mut textures: HashSet<u32> = pinned_textures.into_iter().collect();
    for &m in &materials {
        if let Some(r) = engine.materials.material(m) {
            textures.extend(r.textures.iter().copied().filter(|&t| t != NO_TEXTURE));
        }
    }
    textures.extend(
        ground
            .iter()
            .flat_map(|g| g.textures())
            .filter(|&t| t != NO_TEXTURE),
    );
    let mut stack: Vec<u32> = textures.iter().copied().collect();
    while let Some(t) = stack.pop() {
        if let Some(c) = engine.materials.companions(t) {
            for &k in &c[..3] {
                if k != NO_TEXTURE && textures.insert(k) {
                    stack.push(k);
                }
            }
        }
    }
    // Free the rest.
    let gone_meshes: HashSet<u32> = engine
        .scene
        .live_meshes()
        .map(|m| m.0)
        .filter(|m| !meshes.contains(m))
        .collect();
    for &m in &gone_meshes {
        engine.scene.free_mesh(MeshId(m));
        engine.renderer.rays.forget(MeshId(m));
    }
    let gone_materials: HashSet<u32> = (0..u32::try_from(engine.materials.material_count())
        .unwrap_or(0))
        .filter(|&m| engine.materials.material_live(m) && !materials.contains(&m))
        .collect();
    for &m in &gone_materials {
        engine.materials.free_material(m);
    }
    let gone_textures: HashSet<u32> = (0..u32::try_from(engine.materials.texture_count())
        .unwrap_or(0))
        .filter(|&t| engine.materials.texture_live(t) && !textures.contains(&t))
        .collect();
    for &t in &gone_textures {
        engine.materials.free_texture(t);
    }
    loader.forget(&gone_meshes, &gone_materials, &gone_textures);
    Evicted {
        meshes: gone_meshes.len(),
        materials: gone_materials.len(),
        textures: gone_textures.len(),
        ms: started.elapsed().as_secs_f64() * 1e3,
    }
}
