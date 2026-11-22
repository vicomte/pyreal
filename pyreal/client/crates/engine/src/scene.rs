//! The retained scene: meshes in the shared vertex and index pools, instances in the instance
//! pool, and the bucket layout the cull pass compacts visible instances into.
//!
//! A bucket is one draw of one mesh with one pipeline (a sub-mesh). Every bucket owns a slice of
//! the visible list as long as its mesh's instance count, and one `draw_indexed_indirect`
//! argument record whose instance count the cull pass fills. The layout changes only when the
//! instance set does; the CPU never sees what was culled.
//!
//! Instances keep their slot for life. Static ones fill the pool from the bottom; dynamic ones
//! (an animated body's parts) live in a region at its top, so the transforms that change every
//! frame go up in one write. A moved instance keeps the transform it was drawn with last frame,
//! for motion vectors; a removed one's slot is written dead and reused.
//!
//! The layout is kept as instances come and go: each mesh's count of instances (through their
//! degrade chains) is kept as they are placed and removed, and each mesh's buckets hold half as
//! many again, so a world streamed in block by block lays its buckets out again only when a mesh
//! outgrows its share or a new mesh is drawn. A freed mesh's slot and pool ranges are reused.

use glam::{Affine3A, Vec3};

use crate::asset::mesh::{MeshData, VERTEX_SIZE};
use crate::pool::Pool;
use crate::{gpu::Gpu, EngineError};

/// Elements of each pool, fixed at start-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolSizes {
    pub vertices: u64,
    pub indices: u64,
    pub instances: u64,
    /// Entries of the visible list: instances times their meshes' draws.
    pub visible: u64,
    pub buckets: u64,
    pub meshes: u64,
    pub terrain_cells: u64,
    /// Words (four floats each) of the cell volumes the ground is cut by.
    pub cuts: u64,
    /// Entries of the terrain cells' lists of cut volumes.
    pub cut_lists: u64,
    /// Cell slots, the outside's included.
    pub cells: u64,
    /// Of `instances`, the slots at the top kept for dynamic instances.
    pub dynamic_instances: u64,
}

impl Default for PoolSizes {
    /// 256 MiB of vertices, 128 MiB of indices, 256k instances.
    fn default() -> Self {
        Self {
            vertices: 8 << 20,
            indices: 32 << 20,
            instances: 256 << 10,
            visible: 2 << 20,
            buckets: 64 << 10,
            meshes: 64 << 10,
            terrain_cells: 64 << 10,
            cuts: 256 << 10,
            cut_lists: 256 << 10,
            cells: 64 << 10,
            dynamic_instances: 32 << 10,
        }
    }
}

/// The sun's cascades and the weather's shelter map (the last), each with its own visible list
/// and draw arguments.
pub const SHADOW_LISTS: u64 = 5;

/// Which pipeline draws a bucket; buckets are drawn grouped in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PipelineKind {
    Opaque,
    TwoSided,
    Terrain,
    Translucent,
    Additive,
    /// A cobweb's silk ([`crate::webs`]), drawn with the translucent.
    Web,
    /// Glacier ice, drawn after the water over what lies behind it, writing its depth.
    Ice,
    /// A basin's water ([`crate::fountains`]): drawn with the ice as water while water is drawn
    /// as water, else with the translucent.
    Pool,
}

impl PipelineKind {
    pub const ALL: [Self; 8] = [
        Self::Opaque,
        Self::TwoSided,
        Self::Terrain,
        Self::Translucent,
        Self::Additive,
        Self::Web,
        Self::Ice,
        Self::Pool,
    ];

    /// Drawn into the G-buffer (rather than forward, after lighting).
    #[must_use]
    pub fn deferred(self) -> bool {
        matches!(self, Self::Opaque | Self::TwoSided | Self::Terrain)
    }
}

/// One draw of a mesh: its indices in the index pool and what draws them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Draw {
    pub first_index: u32,
    pub index_count: u32,
    pub material: u32,
    pub pipeline: PipelineKind,
}

/// A mesh in the pools.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    pub first_vertex: u64,
    pub vertex_count: u64,
    pub first_index: u64,
    pub index_count: u64,
    pub draws: Vec<Draw>,
    pub center: Vec3,
    pub radius: f32,
    /// Its triangles' mean edge, model units; zero when it has none.
    pub edge: f32,
    /// The coarser mesh drawn from [`Self::lod_switch`] metres on ([`LOD_NONE`] when there is
    /// none, [`LOD_NOTHING`] when nothing is drawn there).
    pub lod_next: u32,
    pub lod_switch: f32,
    /// How it turns to the eye where its degrade chain draws it: the dats' `degrade_mode`
    /// ([`BILLBOARD_NONE`] for none, 2 a full card, 3 to 5 about the part's x, y or z).
    pub billboard: u32,
    /// The mesh whose pool ranges it draws (a variant's base); `None` when it owns its own.
    pub shares: Option<MeshId>,
    /// Whether it is in the pools; a freed mesh's slot waits to be reused.
    pub live: bool,
}

/// Bytes a mesh takes in the cull pass's mesh table (`cull.wgsl` `MeshInfo`).
const MESH_INFO_SIZE: u64 = 48;

/// The mean length of `mesh`'s triangles' edges, model units.
fn mean_edge(mesh: &MeshData) -> f32 {
    let at = |i: u32| {
        mesh.vertices
            .get(i as usize)
            .map(|v| Vec3::from(v.position))
    };
    let (mut sum, mut n) = (0.0f64, 0u32);
    for &[i, j, k] in mesh.indices.as_chunks::<3>().0 {
        if let (Some(a), Some(b), Some(c)) = (at(i), at(j), at(k)) {
            sum += f64::from(a.distance(b) + b.distance(c) + c.distance(a));
            n += 3;
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    let mean = if n == 0 {
        0.0
    } else {
        (sum / f64::from(n)) as f32
    };
    mean
}

/// A mesh drawn with the part's own frame.
pub const BILLBOARD_NONE: u32 = 1;
/// An instance's sway word for a posed body ([`crate::skin`]): its vertices carry where they
/// stood last frame, for motion.
pub const SKINNED: u32 = 0x8000_0000;
/// An impostor: turned about its upright axis to face the eye, its picture the view nearest.
pub const IMPOSTOR_MODE: u32 = 6;
/// Where a visible entry's material word keeps the drawn level's billboard mode.
pub const BILLBOARD_SHIFT: u32 = 24;

/// Entries at the end of the visible list that hold this frame's translucent draws, sorted far
/// to near.
pub const SORTED_ENTRIES: u64 = 16 << 10;

/// No coarser level.
pub const LOD_NONE: u32 = u32::MAX;
/// Past the switch, nothing is drawn.
pub const LOD_NOTHING: u32 = u32::MAX - 1;
/// Levels one instance's chain may walk.
pub const LOD_STEPS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MeshId(pub u32);

/// One placed mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Instance {
    pub mesh: MeshId,
    pub transform: Affine3A,
    /// Last frame's transform, for motion vectors.
    pub previous: Affine3A,
    /// The object it belongs to, for selection later.
    pub object: u32,
    /// The cell slot it is seen through ([`crate::cells::OUTSIDE`] outdoors).
    pub cell: u32,
    /// How the wind sways it: half floats of how much (the pack's `wind`) and its height,
    /// [`sway`]; zero for not at all.
    pub sway: u32,
}

/// An [`Instance::sway`] word: half floats of how much the wind sways it, and its height.
#[must_use]
pub fn sway(amount: f32, height: f32) -> u32 {
    let h = |v: f32| u32::from(half_bits(v));
    h(amount) | (h(height) << 16)
}

/// A float's half-precision bits (positive, finite values; rounded toward zero).
fn half_bits(v: f32) -> u16 {
    let b = v.clamp(0.0, 65_000.0).to_bits();
    let exp = i32::try_from((b >> 23) & 0xFF).unwrap_or(0) - 127 + 15;
    if exp <= 0 {
        return 0;
    }
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let out = ((exp as u32) << 10) | ((b >> 13) & 0x3FF);
    #[allow(clippy::cast_possible_truncation)]
    let out = out as u16;
    out
}

/// The cell slot's bits of an instance's cell word; the top byte is its fade.
pub const CELL_BITS: u32 = 0x00FF_FFFF;

/// An instance's cell word: cell slot `cell` and how far the object is faded out, `translucency`
/// 0 (opaque) to 1 (gone). A faded object is drawn screen-door (a share of its pixels each frame,
/// a different share the next), which the temporal resolve blends into see-through.
#[must_use]
pub fn faded(cell: u32, translucency: f32) -> u32 {
    let t = pyreal_primitives::num::to_i32(translucency.clamp(0.0, 1.0) * 255.0);
    let t = u32::try_from(t).unwrap_or(0).min(255);
    (cell & CELL_BITS) | (t << 24)
}

/// The bytes of one instance record.
pub const INSTANCE_SIZE: u64 = 112;
/// The bytes of one indirect argument record.
pub const ARGS_SIZE: u64 = 20;

fn rows(out: &mut Vec<u8>, a: &Affine3A) {
    let m = a.matrix3;
    let t = a.translation;
    for r in 0..3 {
        let row = [m.x_axis[r], m.y_axis[r], m.z_axis[r], t[r]];
        for v in row {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
}

/// The mesh word of an empty slot: the cull pass skips it.
pub const DEAD: u32 = u32::MAX;

impl Instance {
    /// An instance of `mesh` standing still at `transform`.
    #[must_use]
    pub fn at(mesh: MeshId, transform: Affine3A, object: u32, cell: u32) -> Self {
        Self {
            mesh,
            transform,
            previous: transform,
            object,
            cell,
            sway: 0,
        }
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        rows(out, &self.transform);
        rows(out, &self.previous);
        for v in [self.mesh.0, self.object, self.cell, self.sway] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }

    /// An empty slot's record.
    pub fn write_dead(out: &mut Vec<u8>) {
        out.extend(std::iter::repeat_n(0, 96));
        for v in [DEAD, u32::MAX, 0, 0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
}

fn write_slot(slot: Option<&Instance>, out: &mut Vec<u8>) {
    match slot {
        Some(i) => i.write(out),
        None => Instance::write_dead(out),
    }
}

/// The slots of the instance pool the cull pass walks: the static ones used, then the dynamic
/// ones from their region's base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CullRange {
    pub static_count: u32,
    pub dynamic_base: u32,
    pub dynamic_count: u32,
}

impl CullRange {
    /// Invocations the cull pass runs.
    #[must_use]
    pub fn total(&self) -> u32 {
        self.static_count + self.dynamic_count
    }
}

/// One draw of the ground, for the depth prepass: its indices and its instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroundDraw {
    pub first_index: u32,
    pub index_count: u32,
    pub instance: u32,
}

/// Whether a sphere lies wholly off the part of the screen `r` covers (the cull pass's test).
fn outside_rect(r: &crate::cells::Rect, view_proj: &glam::Mat4, center: Vec3, radius: f32) -> bool {
    if *r == crate::cells::FULL {
        return false;
    }
    let (r0, r1, r3) = (view_proj.row(0), view_proj.row(1), view_proj.row(3));
    [
        r0 - r3 * r[0],
        r3 * r[2] - r0,
        r1 - r3 * r[1],
        r3 * r[3] - r1,
    ]
    .iter()
    .any(|p| p.truncate().dot(center) + p.w < -radius * p.truncate().length())
}

/// One bucket of the layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bucket {
    pub mesh: MeshId,
    pub draw: Draw,
    /// Where its slice of the visible list starts, and how long it is.
    pub list_base: u32,
    pub capacity: u32,
}

/// What the scene holds, for the viewer's report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SceneCounts {
    pub meshes: usize,
    pub instances: usize,
    pub buckets: usize,
    /// Triangles of every instance, before culling.
    pub triangles: u64,
    pub vertices_used: u64,
    pub indices_used: u64,
    pub visible_entries: u64,
}

/// The GPU-resident scene.
#[derive(Debug)]
pub struct Scene {
    pub sizes: PoolSizes,
    pub vertices: Pool,
    pub indices: Pool,
    pub instances: Pool,
    pub terrain_cells: Pool,
    /// The cell volumes the ground is cut by, and each terrain cell's list of them.
    pub cuts: Pool,
    pub cut_lists: Pool,
    /// Per cell slot, the screen rectangle the portal walk saw it through.
    pub cell_rects: wgpu::Buffer,
    /// Per mesh: bounding sphere, and its buckets' range in `mesh_buckets`.
    pub mesh_table: wgpu::Buffer,
    pub mesh_buckets: wgpu::Buffer,
    /// Per bucket: its list base and material.
    pub bucket_table: wgpu::Buffer,
    /// The indirect arguments the cull pass fills, and the template they are reset from.
    pub args: wgpu::Buffer,
    pub args_template: wgpu::Buffer,
    /// The compacted visible list: `(instance, material)` per entry; then, each as long, the
    /// lists of the sun's cascades and the shelter map ([`SHADOW_LISTS`]).
    pub visible: wgpu::Buffer,
    /// The cascades' and the shelter map's draw arguments, one bucket table each, and their
    /// template.
    pub shadow_args: wgpu::Buffer,
    pub shadow_args_template: wgpu::Buffer,
    meshes: Vec<Mesh>,
    /// Freed mesh slots, reused first.
    free_meshes: Vec<u32>,
    /// Per mesh: live instances drawing it somewhere in their degrade chains, and the share of
    /// the visible list its buckets hold in the layout.
    chain_count: Vec<u32>,
    capacity: Vec<u32>,
    /// A degrade link changed under placed instances: the counts are taken again.
    recount: bool,
    /// Times the bucket layout was made.
    layouts: u64,
    /// Meshes kept whatever draws them (the previews', the cover's).
    pinned: std::collections::BTreeSet<u32>,
    /// Bumped whenever a static instance is placed, moved or taken out.
    static_epoch: u64,
    /// Static slots, and the dynamic region's from its base; `None` is free.
    placed: Vec<Option<Instance>>,
    dynamic: Vec<Option<Instance>>,
    free_static: Vec<u32>,
    free_dynamic: Vec<u32>,
    alive: usize,
    buckets: Vec<Bucket>,
    ground: Vec<GroundDraw>,
    /// Instances with a translucent or additive draw somewhere in their degrade chain, and those
    /// that draw ground.
    translucent: std::collections::BTreeSet<u32>,
    ground_instances: std::collections::BTreeSet<u32>,
    ground_dirty: bool,
    /// The bucket layout must be made again.
    dirty: bool,
    /// Static slots from here on are not written yet; below it, `stale` lists the rewritten.
    instances_written: usize,
    stale: Vec<u32>,
    /// The dynamic slots to write, `lo..hi`.
    stale_dynamic: Option<(u32, u32)>,
    /// Instances moved this frame, whose previous transforms settle once it is drawn.
    moved: Vec<u32>,
    terrain_used: u64,
}

impl Scene {
    /// Allocates every pool once.
    ///
    /// # Errors
    /// A pool of 4 GiB or more.
    pub fn new(gpu: &Gpu, sizes: PoolSizes) -> Result<Self, EngineError> {
        use wgpu::BufferUsages as U;
        let device = &gpu.device;
        if sizes.dynamic_instances > sizes.instances {
            return Err(EngineError::Device(
                "more dynamic instances than instances".into(),
            ));
        }
        let buffer = |label, size: u64, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: usage | U::COPY_DST,
                mapped_at_creation: false,
            })
        };
        // The pools the rays are traced through feed the ray-tracing structures, and the bounce
        // light's rays read their hits' texture coordinates from them.
        let blas = if gpu.rays() {
            U::BLAS_INPUT | U::STORAGE
        } else {
            U::empty()
        };
        Ok(Self {
            vertices: Pool::new(
                device,
                "vertex pool",
                VERTEX_SIZE,
                sizes.vertices,
                U::VERTEX | blas,
            )?,
            indices: Pool::new(device, "index pool", 4, sizes.indices, U::INDEX | blas)?,
            instances: Pool::new(
                device,
                "instance pool",
                INSTANCE_SIZE,
                sizes.instances,
                U::STORAGE,
            )?,
            terrain_cells: Pool::new(
                device,
                "terrain cells",
                crate::terrain::CELL_RECORD_SIZE,
                sizes.terrain_cells,
                U::STORAGE,
            )?,
            cuts: Pool::new(device, "cut volumes", 16, sizes.cuts, U::STORAGE)?,
            cut_lists: Pool::new(device, "cut lists", 4, sizes.cut_lists, U::STORAGE)?,
            cell_rects: buffer("cell rectangles", sizes.cells * 16, U::STORAGE),
            mesh_table: buffer("mesh table", sizes.meshes * MESH_INFO_SIZE, U::STORAGE),
            mesh_buckets: buffer("mesh buckets", sizes.buckets * 4, U::STORAGE),
            bucket_table: buffer("bucket table", sizes.buckets * 8, U::STORAGE),
            args: buffer(
                "draw arguments",
                sizes.buckets * ARGS_SIZE,
                U::STORAGE | U::INDIRECT,
            ),
            args_template: buffer(
                "draw argument template",
                sizes.buckets * ARGS_SIZE,
                U::COPY_SRC,
            ),
            visible: buffer(
                "visible list",
                sizes.visible * 8 * (1 + SHADOW_LISTS),
                U::STORAGE,
            ),
            shadow_args: buffer(
                "shadow draw arguments",
                sizes.buckets * ARGS_SIZE * SHADOW_LISTS,
                U::STORAGE | U::INDIRECT,
            ),
            shadow_args_template: buffer(
                "shadow draw argument template",
                sizes.buckets * ARGS_SIZE * SHADOW_LISTS,
                U::COPY_SRC,
            ),
            sizes,
            meshes: Vec::new(),
            free_meshes: Vec::new(),
            chain_count: Vec::new(),
            capacity: Vec::new(),
            recount: false,
            layouts: 0,
            pinned: std::collections::BTreeSet::new(),
            static_epoch: 0,
            placed: Vec::new(),
            dynamic: Vec::new(),
            free_static: Vec::new(),
            free_dynamic: Vec::new(),
            alive: 0,
            buckets: Vec::new(),
            ground: Vec::new(),
            translucent: std::collections::BTreeSet::new(),
            ground_instances: std::collections::BTreeSet::new(),
            ground_dirty: false,
            dirty: true,
            instances_written: 0,
            stale: Vec::new(),
            stale_dynamic: None,
            moved: Vec::new(),
            terrain_used: 0,
        })
    }

    /// Puts `mesh` into the pools, one draw per sub-mesh with the material and pipeline
    /// `classify` gives it (`None` leaves the sub-mesh out).
    ///
    /// # Errors
    /// A pool is full.
    pub fn add_mesh(
        &mut self,
        gpu: &Gpu,
        mesh: &MeshData,
        mut classify: impl FnMut(usize) -> Option<(u32, PipelineKind)>,
    ) -> Result<MeshId, EngineError> {
        crate::frame_work::note(|w| w.meshes += 1);
        let id = self.next_mesh_id()?;
        let nv = mesh.vertices.len() as u64;
        let ni = mesh.indices.len() as u64;
        let first_vertex = self.vertices.alloc(nv)?;
        let first_index = match self.indices.alloc(ni) {
            Ok(i) => i,
            Err(e) => {
                self.vertices.free(first_vertex, nv);
                return Err(e);
            }
        };
        let mut bytes = Vec::with_capacity(mesh.vertices.len() * 32);
        for v in &mesh.vertices {
            v.write(&mut bytes);
        }
        self.vertices.write(&gpu.queue, first_vertex, &bytes);
        // Indices are stored pool-absolute, so every draw's base vertex is zero.
        let base = u32::try_from(first_vertex).unwrap_or(0);
        let idx: Vec<u8> = mesh
            .indices
            .iter()
            .flat_map(|i| (i + base).to_le_bytes())
            .collect();
        self.indices.write(&gpu.queue, first_index, &idx);
        let fi = u32::try_from(first_index).unwrap_or(0);
        let draws = mesh
            .submeshes
            .iter()
            .enumerate()
            .filter_map(|(k, s)| {
                classify(k).map(|(material, pipeline)| Draw {
                    first_index: fi + s.first_index,
                    index_count: s.index_count,
                    material,
                    pipeline,
                })
            })
            .collect();
        self.put_mesh(
            id,
            Mesh {
                first_vertex,
                vertex_count: nv,
                first_index,
                index_count: ni,
                draws,
                center: Vec3::from(mesh.center),
                radius: mesh.radius,
                edge: mean_edge(mesh),
                lod_next: LOD_NONE,
                lod_switch: f32::MAX,
                billboard: BILLBOARD_NONE,
                shares: None,
                live: true,
            },
        );
        Ok(id)
    }

    /// A slot for a new mesh: a freed one, else the next.
    fn next_mesh_id(&mut self) -> Result<MeshId, EngineError> {
        if let Some(&id) = self.free_meshes.last() {
            return Ok(MeshId(id));
        }
        let id = u32::try_from(self.meshes.len()).unwrap_or(u32::MAX);
        if u64::from(id) >= self.sizes.meshes {
            return Err(EngineError::PoolFull("mesh table", 1));
        }
        Ok(MeshId(id))
    }

    fn put_mesh(&mut self, id: MeshId, mesh: Mesh) {
        let i = id.0 as usize;
        if i < self.meshes.len() {
            self.free_meshes.pop();
            self.meshes[i] = mesh;
            self.chain_count[i] = 0;
        } else {
            self.meshes.push(mesh);
            self.chain_count.push(0);
            self.capacity.push(0);
        }
    }

    /// Takes mesh `id` out of the pools: its ranges (when it owns them) are freed and its slot
    /// is reused. Nothing may draw it any more: no instance, no other mesh's degrade link and no
    /// variant of it.
    /// Writes `bytes` (vertices in the pool's layout) over the start of `mesh`'s own vertices;
    /// whether they fit.
    pub fn write_vertices(&self, queue: &wgpu::Queue, mesh: MeshId, bytes: &[u8]) -> bool {
        let Some(m) = self.mesh(mesh).filter(|m| m.live && m.shares.is_none()) else {
            return false;
        };
        if bytes.len() as u64 > m.vertex_count * VERTEX_SIZE {
            return false;
        }
        self.vertices.write(queue, m.first_vertex, bytes);
        true
    }

    /// Whether a live instance draws `mesh` anywhere in its degrade chain; `None` while the counts
    /// wait to be taken again.
    #[must_use]
    pub fn drawn_by_any(&self, mesh: MeshId) -> Option<bool> {
        if self.recount {
            return None;
        }
        Some(self.chain_count.get(mesh.0 as usize).copied().unwrap_or(0) > 0)
    }

    pub fn free_mesh(&mut self, id: MeshId) {
        let Some(m) = self.meshes.get_mut(id.0 as usize) else {
            return;
        };
        if !m.live {
            return;
        }
        if m.shares.is_none() {
            self.vertices.free(m.first_vertex, m.vertex_count);
            self.indices.free(m.first_index, m.index_count);
        }
        m.live = false;
        m.draws.clear();
        m.lod_next = LOD_NONE;
        m.lod_switch = f32::MAX;
        m.billboard = BILLBOARD_NONE;
        m.shares = None;
        m.vertex_count = 0;
        m.index_count = 0;
        self.pinned.remove(&id.0);
        if self.capacity[id.0 as usize] > 0 {
            self.dirty = true;
        }
        self.free_meshes.push(id.0);
    }

    /// Keeps `mesh` (and what it degrades to) whatever draws it, until [`Self::unpin`].
    pub fn pin(&mut self, mesh: MeshId) {
        self.pinned.insert(mesh.0);
    }

    pub fn unpin(&mut self, mesh: MeshId) {
        self.pinned.remove(&mesh.0);
    }

    /// The pinned meshes.
    pub fn pinned(&self) -> impl Iterator<Item = MeshId> + '_ {
        self.pinned.iter().map(|&m| MeshId(m))
    }

    /// Every live mesh's id.
    pub fn live_meshes(&self) -> impl Iterator<Item = MeshId> + '_ {
        self.meshes
            .iter()
            .enumerate()
            .filter(|(_, m)| m.live)
            .map(|(i, _)| MeshId(u32::try_from(i).unwrap_or(u32::MAX)))
    }

    /// A mesh drawing `base`'s vertices and indices with other materials (a palette or texture
    /// variant): one draw per sub-mesh of `submeshes` (`base`'s own, in order) as `classify`
    /// gives it.
    ///
    /// # Errors
    /// The mesh table is full.
    pub fn add_mesh_variant(
        &mut self,
        base: MeshId,
        submeshes: &[crate::asset::mesh::SubMesh],
        mut classify: impl FnMut(usize) -> Option<(u32, PipelineKind)>,
    ) -> Result<MeshId, EngineError> {
        let id = self.next_mesh_id()?;
        let Some(b) = self.meshes.get(base.0 as usize).filter(|b| b.live) else {
            return Err(EngineError::PoolFull("mesh table", 0));
        };
        let owner = b.shares.unwrap_or(base);
        let fi = u32::try_from(b.first_index).unwrap_or(0);
        let draws = submeshes
            .iter()
            .enumerate()
            .filter_map(|(k, s)| {
                classify(k).map(|(material, pipeline)| Draw {
                    first_index: fi + s.first_index,
                    index_count: s.index_count,
                    material,
                    pipeline,
                })
            })
            .collect();
        let mesh = Mesh {
            draws,
            lod_next: LOD_NONE,
            lod_switch: f32::MAX,
            billboard: BILLBOARD_NONE,
            shares: Some(owner),
            ..b.clone()
        };
        self.put_mesh(id, mesh);
        Ok(id)
    }

    /// `mesh` turns to the eye as degrade mode `mode` says, where its chain draws it. A mesh
    /// keeps the first mode it is given.
    pub fn set_billboard(&mut self, mesh: MeshId, mode: u32) {
        if let Some(m) = self.meshes.get_mut(mesh.0 as usize) {
            if m.billboard == BILLBOARD_NONE && (2..=IMPOSTOR_MODE).contains(&mode) {
                m.billboard = mode;
                if self.capacity[mesh.0 as usize] > 0 {
                    self.dirty = true;
                }
            }
        }
    }

    /// From `switch` metres on, `mesh` draws as `next` (`None`: nothing). A mesh keeps the first
    /// link it is given.
    pub fn set_lod(&mut self, mesh: MeshId, next: Option<MeshId>, switch: f32) {
        if let Some(m) = self.meshes.get_mut(mesh.0 as usize) {
            if m.lod_next == LOD_NONE && next != Some(mesh) {
                m.lod_next = next.map_or(LOD_NOTHING, |n| n.0);
                m.lod_switch = switch;
                if self.chain_count[mesh.0 as usize] > 0 {
                    self.recount = true;
                }
                self.dirty = true;
            }
        }
    }

    /// `mesh` and the coarser meshes it degrades to, in order.
    #[must_use]
    pub fn lod_chain(&self, mesh: MeshId) -> Vec<MeshId> {
        let mut out = vec![mesh];
        let mut at = mesh.0;
        for _ in 0..LOD_STEPS {
            let Some(m) = self.meshes.get(at as usize) else {
                break;
            };
            if m.lod_next >= LOD_NOTHING || out.contains(&MeshId(m.lod_next)) {
                break;
            }
            at = m.lod_next;
            out.push(MeshId(at));
        }
        out
    }

    /// Takes every instance, the ground's cells and the rooms' cut volumes out, keeping the meshes:
    /// the world is placed again from nothing (somewhere else) and finds its meshes loaded.
    pub fn clear_world(&mut self) {
        self.placed.clear();
        self.static_epoch += 1;
        self.dynamic.clear();
        self.free_static.clear();
        self.free_dynamic.clear();
        self.alive = 0;
        self.buckets.clear();
        self.ground.clear();
        self.translucent.clear();
        self.ground_instances.clear();
        self.ground_dirty = false;
        self.chain_count.iter_mut().for_each(|c| *c = 0);
        self.recount = false;
        self.dirty = true;
        self.instances_written = 0;
        self.stale.clear();
        self.stale_dynamic = None;
        self.moved.clear();
        self.terrain_cells.reset();
        self.cuts.reset();
        self.cut_lists.reset();
        self.terrain_used = 0;
    }

    /// Where the dynamic region starts in the instance pool.
    #[must_use]
    pub fn dynamic_base(&self) -> u32 {
        u32::try_from(self.instances.capacity() - self.sizes.dynamic_instances).unwrap_or(0)
    }

    /// Places a static instance; its slot, which is its id for life.
    ///
    /// # Errors
    /// The static part of the instance pool is full.
    pub fn add_instance(&mut self, instance: Instance) -> Result<u32, EngineError> {
        let id = if let Some(id) = self.free_static.pop() {
            self.placed[id as usize] = Some(instance);
            self.stale.push(id);
            id
        } else {
            if self.placed.len() as u64 >= u64::from(self.dynamic_base()) {
                return Err(EngineError::PoolFull("instance pool", 1));
            }
            self.placed.push(Some(instance));
            u32::try_from(self.placed.len() - 1).unwrap_or(u32::MAX)
        };
        self.alive += 1;
        self.static_epoch += 1;
        self.count_in(id, instance.mesh);
        Ok(id)
    }

    /// Counts a placed instance into its chain's meshes and the translucent and ground sets.
    fn count_in(&mut self, id: u32, mesh: MeshId) {
        let mut translucent = false;
        for m in self.lod_chain(mesh) {
            let k = m.0 as usize;
            let Some(c) = self.chain_count.get_mut(k) else {
                continue;
            };
            *c += 1;
            if *c > self.capacity[k] {
                self.dirty = true;
            }
            translucent |= self.meshes[k].draws.iter().any(|d| !d.pipeline.deferred());
        }
        if translucent {
            self.translucent.insert(id);
        }
        if self.draws_ground(mesh) {
            self.ground_instances.insert(id);
            self.ground_dirty = true;
        }
    }

    /// Counts a removed instance out again.
    fn count_out(&mut self, id: u32, mesh: MeshId) {
        for m in self.lod_chain(mesh) {
            if let Some(c) = self.chain_count.get_mut(m.0 as usize) {
                *c = c.saturating_sub(1);
            }
        }
        self.translucent.remove(&id);
        if self.ground_instances.remove(&id) {
            self.ground_dirty = true;
        }
    }

    fn draws_ground(&self, mesh: MeshId) -> bool {
        self.meshes
            .get(mesh.0 as usize)
            .is_some_and(|m| m.draws.iter().any(|d| d.pipeline == PipelineKind::Terrain))
    }

    /// Places an instance in the dynamic region, for one that moves every frame; its slot.
    ///
    /// # Errors
    /// The dynamic region is full.
    pub fn add_dynamic(&mut self, instance: Instance) -> Result<u32, EngineError> {
        let k = if let Some(k) = self.free_dynamic.pop() {
            self.dynamic[k as usize] = Some(instance);
            k
        } else {
            if self.dynamic.len() as u64 >= self.sizes.dynamic_instances {
                return Err(EngineError::PoolFull("dynamic instances", 1));
            }
            self.dynamic.push(Some(instance));
            u32::try_from(self.dynamic.len() - 1).unwrap_or(u32::MAX)
        };
        self.alive += 1;
        let id = self.dynamic_base() + k;
        self.count_in(id, instance.mesh);
        self.stale_id(id);
        Ok(id)
    }

    fn slot_mut(&mut self, id: u32) -> Option<&mut Option<Instance>> {
        let base = self.dynamic_base();
        if id >= base {
            self.dynamic.get_mut((id - base) as usize)
        } else {
            self.placed.get_mut(id as usize)
        }
    }

    /// The instance in slot `id`.
    #[must_use]
    pub fn instance(&self, id: u32) -> Option<&Instance> {
        let base = self.dynamic_base();
        if id >= base {
            self.dynamic.get((id - base) as usize)?.as_ref()
        } else {
            self.placed.get(id as usize)?.as_ref()
        }
    }

    /// Marks slot `id` to be written at the next upload.
    fn stale_id(&mut self, id: u32) {
        let base = self.dynamic_base();
        if id >= base {
            let k = id - base;
            self.stale_dynamic = Some(match self.stale_dynamic {
                Some((lo, hi)) => (lo.min(k), hi.max(k + 1)),
                None => (k, k + 1),
            });
        } else if (id as usize) < self.instances_written {
            self.stale.push(id);
        }
    }

    /// Moves instance `id` to `transform`. The transform it was drawn with last frame stays its
    /// previous one until this frame is drawn, however often it moves before then.
    pub fn set_transform(&mut self, id: u32, transform: Affine3A) {
        let Some(Some(inst)) = self.slot_mut(id) else {
            return;
        };
        if inst.transform == transform {
            return;
        }
        let first = inst.previous == inst.transform;
        inst.transform = transform;
        if first {
            self.moved.push(id);
        }
        if id < self.dynamic_base() {
            self.static_epoch += 1;
        }
        self.stale_id(id);
    }

    /// Moves instance `id` into cell slot `cell` (the outdoors' or a room's).
    pub fn set_cell(&mut self, id: u32, cell: u32) {
        let Some(Some(inst)) = self.slot_mut(id) else {
            return;
        };
        if inst.cell == cell {
            return;
        }
        inst.cell = cell;
        self.stale_id(id);
    }

    /// Takes instance `id` out of the scene; its slot is written dead and reused.
    pub fn remove_instance(&mut self, id: u32) {
        let base = self.dynamic_base();
        let Some(slot) = self.slot_mut(id) else {
            return;
        };
        let Some(gone) = slot.take() else {
            return;
        };
        if id >= base {
            self.free_dynamic.push(id - base);
        } else {
            self.free_static.push(id);
            self.static_epoch += 1;
        }
        self.alive -= 1;
        self.count_out(id, gone.mesh);
        self.stale_id(id);
    }

    /// Every live instance with its slot.
    pub fn instances(&self) -> impl Iterator<Item = (u32, &Instance)> {
        let base = self.dynamic_base();
        let id = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
        self.placed
            .iter()
            .enumerate()
            .filter_map(move |(i, s)| Some((id(i), s.as_ref()?)))
            .chain(
                self.dynamic
                    .iter()
                    .enumerate()
                    .filter_map(move |(k, s)| Some((base + id(k), s.as_ref()?))),
            )
    }

    /// The instances in the dynamic region with their slots, in slot order.
    pub fn dynamic_instances(&self) -> impl Iterator<Item = (u32, &Instance)> {
        let base = self.dynamic_base();
        self.dynamic
            .iter()
            .enumerate()
            .filter_map(move |(k, s)| Some((base + u32::try_from(k).ok()?, s.as_ref()?)))
    }

    /// Changes whenever a static instance is placed, moved or taken out.
    #[must_use]
    pub fn static_epoch(&self) -> u64 {
        self.static_epoch
    }

    /// Writes a cell volume's words (see [`crate::cells::Volume::write`]); its first word's
    /// index.
    ///
    /// # Errors
    /// The cut pool is full.
    pub fn add_cut_volume(&mut self, gpu: &Gpu, words: &[[f32; 4]]) -> Result<u32, EngineError> {
        let first = self.cuts.alloc(words.len() as u64)?;
        let bytes: Vec<u8> = words
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        self.cuts.write(&gpu.queue, first, &bytes);
        Ok(u32::try_from(first).unwrap_or(0))
    }

    /// Writes one terrain cell's list of cut volumes; the first entry's index.
    ///
    /// # Errors
    /// The list pool is full.
    pub fn add_cut_list(&mut self, gpu: &Gpu, list: &[u32]) -> Result<u32, EngineError> {
        let first = self.cut_lists.alloc(list.len() as u64)?;
        let bytes: Vec<u8> = list.iter().flat_map(|v| v.to_le_bytes()).collect();
        self.cut_lists.write(&gpu.queue, first, &bytes);
        Ok(u32::try_from(first).unwrap_or(0))
    }

    /// Writes this frame's cell rectangles, slot by slot.
    pub fn write_cell_rects(&self, queue: &wgpu::Queue, rects: &[crate::cells::Rect]) {
        let n = rects
            .len()
            .min(usize::try_from(self.sizes.cells).unwrap_or(0));
        let bytes: Vec<u8> = rects[..n]
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        if !bytes.is_empty() {
            queue.write_buffer(&self.cell_rects, 0, &bytes);
        }
    }

    /// Writes terrain cell records; the first one's index.
    ///
    /// # Errors
    /// The terrain cell pool is full.
    pub fn add_terrain_cells(&mut self, gpu: &Gpu, bytes: &[u8]) -> Result<u32, EngineError> {
        let n = bytes.len() as u64 / crate::terrain::CELL_RECORD_SIZE;
        let first = self.terrain_cells.alloc(n)?;
        self.terrain_cells.write(&gpu.queue, first, bytes);
        self.terrain_used += n;
        Ok(u32::try_from(first).unwrap_or(0))
    }

    /// Frees `count` terrain cell records from `first`.
    pub fn free_terrain_cells(&mut self, first: u32, count: u64) {
        self.terrain_cells.free(u64::from(first), count);
        self.terrain_used = self.terrain_used.saturating_sub(count);
    }

    #[must_use]
    pub fn mesh(&self, id: MeshId) -> Option<&Mesh> {
        self.meshes.get(id.0 as usize)
    }

    /// Live instances.
    #[must_use]
    pub fn instance_count(&self) -> u32 {
        u32::try_from(self.alive).unwrap_or(u32::MAX)
    }

    /// The slots the cull pass walks.
    #[must_use]
    pub fn cull_range(&self) -> CullRange {
        CullRange {
            static_count: u32::try_from(self.placed.len()).unwrap_or(u32::MAX),
            dynamic_base: self.dynamic_base(),
            dynamic_count: u32::try_from(self.dynamic.len()).unwrap_or(u32::MAX),
        }
    }

    /// The buckets, grouped by pipeline in [`PipelineKind::ALL`]'s order.
    #[must_use]
    pub fn buckets(&self) -> &[Bucket] {
        &self.buckets
    }

    /// This frame's translucent and additive draws, farthest object first: each object in the
    /// frustum and its cell's rectangle, at the degrade level its distance asks for
    /// (`lod_scale` as the cull pass has it), its draws in mesh order. Each is `(instance, draw,
    /// distance)`, the draw's material word carrying its level's billboard mode above
    /// [`BILLBOARD_SHIFT`].
    #[must_use]
    pub fn sorted_translucent(
        &self,
        eye: Vec3,
        planes: &[glam::Vec4],
        rects: &[crate::cells::Rect],
        view_proj: &glam::Mat4,
        lod_scale: f32,
    ) -> Vec<(u32, Draw, f32)> {
        let mut objects: Vec<(f32, u32, MeshId)> = Vec::new();
        for &i in &self.translucent {
            let Some(inst) = self.instance(i) else {
                continue;
            };
            let Some(base) = self.meshes.get(inst.mesh.0 as usize) else {
                continue;
            };
            let m = inst.transform.matrix3;
            let scale = m
                .x_axis
                .length()
                .max(m.y_axis.length())
                .max(m.z_axis.length());
            let center = inst.transform.transform_point3(base.center);
            let radius = base.radius * scale;
            if planes
                .iter()
                .any(|p| p.truncate().dot(center) + p.w < -radius)
            {
                continue;
            }
            let rect = rects
                .get((inst.cell & CELL_BITS) as usize)
                .copied()
                .unwrap_or(crate::cells::FULL);
            if crate::cells::is_empty(&rect) || outside_rect(&rect, view_proj, center, radius) {
                continue;
            }
            let d = center.distance(eye);
            let mut mesh = inst.mesh;
            let measured = d / m.z_axis.length().max(1e-3) * lod_scale;
            let mut drawn = true;
            for _ in 0..LOD_STEPS {
                let Some(level) = self.meshes.get(mesh.0 as usize) else {
                    break;
                };
                if level.lod_next == LOD_NONE || measured < level.lod_switch {
                    break;
                }
                if level.lod_next == LOD_NOTHING {
                    drawn = false;
                    break;
                }
                mesh = MeshId(level.lod_next);
            }
            if drawn {
                objects.push((d, i, mesh));
            }
        }
        objects.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut out = Vec::new();
        for (far, i, mesh) in objects {
            if let Some(m) = self.meshes.get(mesh.0 as usize) {
                let turn = m.billboard << BILLBOARD_SHIFT;
                out.extend(m.draws.iter().filter(|d| !d.pipeline.deferred()).map(|d| {
                    let draw = Draw {
                        material: d.material | turn,
                        ..*d
                    };
                    (i, draw, far)
                }));
            }
            if out.len() as u64 >= SORTED_ENTRIES {
                out.truncate(usize::try_from(SORTED_ENTRIES).unwrap_or(0));
                break;
            }
        }
        out
    }

    /// Where the sorted translucent entries start in the visible list.
    #[must_use]
    pub fn sorted_base(&self) -> u32 {
        u32::try_from(self.sizes.visible - SORTED_ENTRIES).unwrap_or(0)
    }

    /// The ground's draws, each with its own instance (the depth prepass draws them directly).
    #[must_use]
    pub fn ground(&self) -> &[GroundDraw] {
        &self.ground
    }

    /// After a frame is drawn: what moved in it now has that frame's transform as its previous
    /// one, so it draws still unless it moves again.
    pub fn settle(&mut self) {
        for id in std::mem::take(&mut self.moved) {
            if let Some(Some(inst)) = self.slot_mut(id) {
                inst.previous = inst.transform;
                self.stale_id(id);
            }
        }
    }

    fn write_instances(&mut self, queue: &wgpu::Queue) {
        // Rewritten static slots, in runs.
        let mut stale = std::mem::take(&mut self.stale);
        stale.sort_unstable();
        stale.dedup();
        let mut k = 0;
        while k < stale.len() {
            let first = stale[k];
            let mut last = first;
            while k + 1 < stale.len() && stale[k + 1] <= last + 8 {
                k += 1;
                last = stale[k];
            }
            k += 1;
            let mut bytes = Vec::new();
            for i in first..=last {
                write_slot(
                    self.placed.get(i as usize).and_then(Option::as_ref),
                    &mut bytes,
                );
            }
            self.instances.write(queue, u64::from(first), &bytes);
        }
        // New static slots.
        if self.instances_written < self.placed.len() {
            let mut bytes = Vec::with_capacity((self.placed.len() - self.instances_written) * 112);
            for s in &self.placed[self.instances_written..] {
                write_slot(s.as_ref(), &mut bytes);
            }
            self.instances
                .write(queue, self.instances_written as u64, &bytes);
            self.instances_written = self.placed.len();
        }
        // The dynamic region's changed span, in one write.
        if let Some((lo, hi)) = self.stale_dynamic.take() {
            let mut bytes = Vec::with_capacity((hi - lo) as usize * 112);
            for k in lo..hi {
                write_slot(
                    self.dynamic.get(k as usize).and_then(Option::as_ref),
                    &mut bytes,
                );
            }
            self.instances
                .write(queue, u64::from(self.dynamic_base() + lo), &bytes);
        }
    }

    /// Uploads new instances and, when the instance set or the meshes changed, the bucket layout.
    ///
    /// # Errors
    /// More buckets or visible entries than the pools hold.
    pub fn upload(&mut self, gpu: &Gpu) -> Result<(), EngineError> {
        self.write_instances(&gpu.queue);
        if self.recount {
            self.count_all();
        }
        if self.ground_dirty {
            self.ground_dirty = false;
            self.ground = self
                .ground_instances
                .iter()
                .filter_map(|&i| Some((i, self.meshes.get(self.instance(i)?.mesh.0 as usize)?)))
                .flat_map(|(i, m)| {
                    m.draws
                        .iter()
                        .filter(|d| d.pipeline == PipelineKind::Terrain)
                        .map(move |d| GroundDraw {
                            first_index: d.first_index,
                            index_count: d.index_count,
                            instance: i,
                        })
                })
                .collect();
        }
        if !self.dirty {
            return Ok(());
        }
        self.dirty = false;
        // Every mesh of an instance's chain may draw it; each mesh's share has room to grow, so
        // a few more instances lay nothing out again. Past the list's room, the exact counts.
        let roomy = |c: u32| if c == 0 { 0 } else { (c + c / 2).max(8) };
        let total: u64 = self
            .chain_count
            .iter()
            .zip(&self.meshes)
            .map(|(&c, m)| u64::from(roomy(c)) * m.draws.len() as u64)
            .sum();
        let room = total + SORTED_ENTRIES <= self.sizes.visible;
        for (k, c) in self.capacity.iter_mut().enumerate() {
            let n = self.chain_count[k];
            *c = if room { roomy(n) } else { n };
        }
        let mut buckets: Vec<Bucket> = Vec::new();
        for (m, mesh) in self.meshes.iter().enumerate() {
            if self.capacity[m] == 0 || !mesh.live {
                continue;
            }
            for d in &mesh.draws {
                buckets.push(Bucket {
                    mesh: MeshId(u32::try_from(m).unwrap_or(0)),
                    draw: *d,
                    list_base: 0,
                    capacity: self.capacity[m],
                });
            }
        }
        buckets.sort_by_key(|b| (b.draw.pipeline, b.mesh));
        if buckets.len() as u64 > self.sizes.buckets {
            return Err(EngineError::PoolFull("bucket", buckets.len() as u64));
        }
        let mut base = 0u64;
        for b in &mut buckets {
            b.list_base = u32::try_from(base).unwrap_or(u32::MAX);
            base += u64::from(b.capacity);
        }
        if base + SORTED_ENTRIES > self.sizes.visible {
            // The exact counts, as a last resort.
            let mut base = 0u64;
            for b in &mut buckets {
                b.capacity = self.chain_count[b.mesh.0 as usize];
                b.list_base = u32::try_from(base).unwrap_or(u32::MAX);
                base += u64::from(b.capacity);
            }
            for (k, c) in self.capacity.iter_mut().enumerate() {
                *c = self.chain_count[k];
            }
            if base + SORTED_ENTRIES > self.sizes.visible {
                return Err(EngineError::PoolFull("visible list", base + SORTED_ENTRIES));
            }
        }
        self.layouts += 1;
        // Each mesh's buckets, contiguous in `mesh_buckets`.
        let mut by_mesh: Vec<Vec<u32>> = vec![Vec::new(); self.meshes.len()];
        for (k, b) in buckets.iter().enumerate() {
            by_mesh[b.mesh.0 as usize].push(u32::try_from(k).unwrap_or(0));
        }
        let mut mesh_bytes =
            Vec::with_capacity(self.meshes.len() * usize::try_from(MESH_INFO_SIZE).unwrap_or(48));
        let mut list = Vec::new();
        for (m, mesh) in self.meshes.iter().enumerate() {
            let first = u32::try_from(list.len()).unwrap_or(0);
            list.extend_from_slice(&by_mesh[m]);
            for v in [mesh.center.x, mesh.center.y, mesh.center.z, mesh.radius] {
                mesh_bytes.extend_from_slice(&v.to_le_bytes());
            }
            // The bucket count, with the billboard mode in its top byte.
            let count = u32::try_from(by_mesh[m].len()).unwrap_or(0) | (mesh.billboard << 24);
            for v in [first, count, mesh.lod_next, mesh.lod_switch.to_bits()] {
                mesh_bytes.extend_from_slice(&v.to_le_bytes());
            }
            for v in [mesh.edge, 0.0, 0.0, 0.0] {
                mesh_bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        let q = &gpu.queue;
        q.write_buffer(&self.mesh_table, 0, &mesh_bytes);
        let list_bytes: Vec<u8> = list.iter().flat_map(|v| v.to_le_bytes()).collect();
        if !list_bytes.is_empty() {
            q.write_buffer(&self.mesh_buckets, 0, &list_bytes);
        }
        let mut table = Vec::with_capacity(buckets.len() * 8);
        let mut args = Vec::with_capacity(buckets.len() * 20);
        for b in &buckets {
            for v in [b.list_base, b.draw.material] {
                table.extend_from_slice(&v.to_le_bytes());
            }
            for v in [b.draw.index_count, 0, b.draw.first_index, 0, b.list_base] {
                args.extend_from_slice(&v.to_le_bytes());
            }
        }
        if !table.is_empty() {
            q.write_buffer(&self.bucket_table, 0, &table);
            q.write_buffer(&self.args_template, 0, &args);
            // Each cascade (and the shelter map) draws from its own list, past the frame's.
            let mut shadow = Vec::with_capacity(args.len() * 4);
            for c in 1..=SHADOW_LISTS {
                let offset = u32::try_from(self.sizes.visible * c).unwrap_or(u32::MAX);
                for b in &buckets {
                    for v in [
                        b.draw.index_count,
                        0,
                        b.draw.first_index,
                        0,
                        b.list_base + offset,
                    ] {
                        shadow.extend_from_slice(&v.to_le_bytes());
                    }
                }
            }
            q.write_buffer(&self.shadow_args_template, 0, &shadow);
        }
        self.buckets = buckets;
        Ok(())
    }

    /// Counts every instance again: after a degrade link changed under placed instances.
    fn count_all(&mut self) {
        self.recount = false;
        self.chain_count.iter_mut().for_each(|c| *c = 0);
        self.translucent.clear();
        self.ground_instances.clear();
        let all: Vec<(u32, MeshId)> = self.instances().map(|(i, inst)| (i, inst.mesh)).collect();
        for (i, mesh) in all {
            self.count_in(i, mesh);
        }
        self.ground_dirty = true;
        self.dirty = true;
    }

    /// Moves everything placed by `delta` (render space's origin moved the other way): every
    /// instance, now and last frame, written again at the next upload.
    pub fn translate(&mut self, delta: Vec3) {
        self.static_epoch += 1;
        let d = glam::Vec3A::from(delta);
        for inst in self
            .placed
            .iter_mut()
            .chain(self.dynamic.iter_mut())
            .flatten()
        {
            inst.transform.translation += d;
            inst.previous.translation += d;
        }
        self.instances_written = 0;
        self.stale.clear();
        if !self.dynamic.is_empty() {
            self.stale_dynamic = Some((0, u32::try_from(self.dynamic.len()).unwrap_or(0)));
        }
    }

    /// Times the bucket layout was made, for the tests and the report.
    #[must_use]
    pub fn layouts(&self) -> u64 {
        self.layouts
    }

    /// What the scene holds.
    #[must_use]
    pub fn counts(&self) -> SceneCounts {
        let triangles = self
            .instances()
            .filter_map(|(_, i)| self.meshes.get(i.mesh.0 as usize))
            .map(|m| {
                m.draws
                    .iter()
                    .map(|d| u64::from(d.index_count / 3))
                    .sum::<u64>()
            })
            .sum();
        SceneCounts {
            meshes: self.meshes.iter().filter(|m| m.live).count(),
            instances: self.alive,
            buckets: self.buckets.len(),
            triangles,
            vertices_used: self.vertices.used(),
            indices_used: self.indices.used(),
            visible_entries: self.buckets.iter().map(|b| u64::from(b.capacity)).sum(),
        }
    }

    /// The pool sizes in bytes, by name.
    #[must_use]
    pub fn pool_bytes(&self) -> Vec<(&'static str, u64)> {
        vec![
            (self.vertices.name, self.vertices.bytes()),
            (self.indices.name, self.indices.bytes()),
            (self.instances.name, self.instances.bytes()),
            ("visible list", self.sizes.visible * 8),
            (self.terrain_cells.name, self.terrain_cells.bytes()),
            (self.cuts.name, self.cuts.bytes()),
            ("draw arguments", self.sizes.buckets * ARGS_SIZE),
        ]
    }
}
