//! The world as the lamps' rays see it: a bottom-level structure per mesh, straight from the
//! shared vertex and index pools, and a top level of the instances near the eye, rebuilt as the
//! eye moves or bodies move (the desktop tier's ray queries).
//!
//! Only the solid surfaces stop a ray: translucent, additive, cut-out and foliage draws are left
//! out, so leaves never throw a lamp's shadow as a solid blob.

use std::collections::HashMap;

use glam::Vec3;

use crate::asset::material::flags;
use crate::gpu::Gpu;
use crate::materials::MaterialTable;
use crate::scene::{MeshId, Scene};

/// How far from the eye instances are kept in the top level, metres.
pub const RAY_RADIUS: f32 = 90.0;
/// The most instances in the top level.
pub const MAX_RAY_INSTANCES: usize = 8192;
/// How far the eye moves before the top level is rebuilt with the instances round it, metres.
pub const REBUILD_DISTANCE: f32 = 8.0;

/// One mesh's bottom level, with the size descriptors it was built from.
#[derive(Debug)]
struct MeshBlas {
    blas: wgpu::Blas,
    sizes: Vec<wgpu::BlasTriangleGeometrySizeDescriptor>,
    draws: Vec<(u32, u32)>,
    /// Where its draws' (first index, material) start in the draw table.
    geom_base: u32,
    built: bool,
}

/// The most draws the bounce light's rays can tell apart.
pub const MAX_RAY_DRAWS: u64 = 256 << 10;

/// Builds a small bottom level and a top level of it, as the world's are built (the same vertex
/// and index formats, stride, geometry and structure flags), and waits until the device has.
///
/// The first such build on a device whose driver cache is cold compiles the driver's own builders,
/// which took some sixteen seconds on the first world frame (an Intel Arc on Vulkan and on DX12
/// alike). Done on a thread of its own while the device is otherwise idle (the front door, the
/// login), it leaves the world's first frame to find them compiled; run late, the world's first
/// build waits only for what is left. A device without ray queries does nothing.
pub fn warm_up(gpu: &Gpu) {
    if !gpu.rays() {
        return;
    }
    // A square of the pool's own vertex layout: positions first, the rest of each vertex unused.
    const SIDE: u32 = 33;
    let stride = crate::asset::mesh::VERTEX_SIZE;
    let words = usize::try_from(stride / 4).unwrap_or(1);
    let mut vertices = vec![0f32; (SIDE * SIDE) as usize * words];
    for y in 0..SIDE {
        for x in 0..SIDE {
            let at = (y * SIDE + x) as usize * words;
            #[allow(clippy::cast_precision_loss)]
            {
                vertices[at] = x as f32;
                vertices[at + 1] = y as f32;
                vertices[at + 2] = ((x * 7 + y * 3) % 5) as f32 * 0.1;
            }
        }
    }
    let mut indices = Vec::new();
    for y in 0..SIDE - 1 {
        for x in 0..SIDE - 1 {
            let i = y * SIDE + x;
            indices.extend_from_slice(&[i, i + 1, i + SIDE, i + 1, i + SIDE + 1, i + SIDE]);
        }
    }
    let vertex_bytes: Vec<u8> = vertices.iter().flat_map(|v| v.to_le_bytes()).collect();
    let index_bytes: Vec<u8> = indices.iter().flat_map(|i| i.to_le_bytes()).collect();
    let buffer = |label, data: Vec<u8>| {
        let b = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: data.len() as u64,
            usage: wgpu::BufferUsages::BLAS_INPUT | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue.write_buffer(&b, 0, &data);
        b
    };
    let vertex_buffer = buffer("warm-up vertices", vertex_bytes);
    let index_buffer = buffer("warm-up indices", index_bytes);
    // Two geometries, as a mesh of two solid draws has.
    let half = u32::try_from(indices.len() / 2).unwrap_or(0) / 3 * 3;
    let size = |count| wgpu::BlasTriangleGeometrySizeDescriptor {
        vertex_format: wgpu::VertexFormat::Float32x3,
        vertex_count: SIDE * SIDE,
        index_format: Some(wgpu::IndexFormat::Uint32),
        index_count: Some(count),
        flags: wgpu::AccelerationStructureGeometryFlags::empty(),
    };
    let sizes = [size(half), size(half)];
    let blas = gpu.device.create_blas(
        &wgpu::CreateBlasDescriptor {
            label: Some("warm-up"),
            flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
            update_mode: wgpu::AccelerationStructureUpdateMode::Build,
        },
        wgpu::BlasGeometrySizeDescriptors::Triangles {
            descriptors: sizes.to_vec(),
        },
    );
    let mut tlas = gpu.device.create_tlas(&wgpu::CreateTlasDescriptor {
        label: Some("warm-up"),
        max_instances: 2,
        flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
        update_mode: wgpu::AccelerationStructureUpdateMode::Build,
    });
    for (k, x) in [0.0f32, 40.0].into_iter().enumerate() {
        tlas[k] = Some(wgpu::TlasInstance::new(
            &blas,
            [1.0, 0.0, 0.0, x, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            0,
            0xFF,
        ));
    }
    let geometry: Vec<wgpu::BlasTriangleGeometry<'_>> = sizes
        .iter()
        .zip([0, half])
        .map(|(size, first)| wgpu::BlasTriangleGeometry {
            size,
            vertex_buffer: &vertex_buffer,
            first_vertex: 0,
            vertex_stride: stride,
            index_buffer: Some(&index_buffer),
            first_index: Some(first),
            transform_buffer: None,
            transform_buffer_offset: None,
        })
        .collect();
    let mut enc = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("warm-up"),
        });
    enc.build_acceleration_structures(
        std::iter::once(&wgpu::BlasBuildEntry {
            blas: &blas,
            geometry: wgpu::BlasGeometries::TriangleGeometries(geometry),
        }),
        std::iter::once(&tlas),
    );
    gpu.queue.submit([enc.finish()]);
    gpu.wait();
}

/// The structures.
#[derive(Debug, Default)]
pub struct RayScene {
    // ORDER-OK: looked up by mesh, never iterated.
    meshes: HashMap<u32, Option<MeshBlas>>,
    tlas: Option<wgpu::Tlas>,
    built_at: Option<Vec3>,
    built_instances: usize,
    /// Instances in the top level last built.
    pub instances: usize,
    /// Per instance of the top level, where its mesh's draws start in [`Self::draws`]; and per
    /// draw, its first index and material, for a ray's hit to find its surface.
    pub instance_table: Option<wgpu::Buffer>,
    pub draws: Option<wgpu::Buffer>,
    /// The draw table's entries handed out, freed with their mesh.
    draws_used: Option<crate::pool::RangeAllocator>,
    /// Bumped each time the top level is rebuilt, so bind groups holding it are made again.
    pub generation: u64,
    /// The static instances that can be in reach while the eye stays near where they were
    /// gathered, in slot order; that place and the scene's static epoch then.
    statics: Vec<u32>,
    statics_at: Option<(Vec3, u64)>,
    /// Meshes whose vertices are written again each frame (posed bodies): built again each update.
    // ORDER-OK: looked up by mesh.
    pub deforming: std::collections::HashSet<u32>,
}

/// How far past [`RAY_RADIUS`] the static instances are gathered: the eye's leeway, and a metre
/// over it for rounding.
const STATIC_REACH: f32 = RAY_RADIUS + REBUILD_DISTANCE + 1.0;

impl RayScene {
    /// The top level, once built.
    #[must_use]
    pub fn tlas(&self) -> Option<&wgpu::Tlas> {
        self.tlas.as_ref()
    }

    /// Forgets `mesh`'s bottom level and its draws (the mesh was freed; its slot may be reused).
    pub fn forget(&mut self, mesh: MeshId) {
        self.deforming.remove(&mesh.0);
        if let Some(Some(m)) = self.meshes.remove(&mesh.0) {
            if let Some(a) = self.draws_used.as_mut() {
                a.free(u64::from(m.geom_base), m.draws.len() as u64);
            }
        }
    }

    /// The meshes it holds a bottom level for, or knows have none.
    pub fn meshes(&self) -> impl Iterator<Item = MeshId> + '_ {
        self.meshes.keys().map(|&m| MeshId(m))
    }

    /// The top level is built again at the next update (render space moved).
    pub fn invalidate(&mut self) {
        self.built_at = None;
    }

    /// The bottom level of `mesh`, made the first time (`None` when nothing of it is solid).
    fn mesh_blas(
        &mut self,
        gpu: &Gpu,
        scene: &Scene,
        materials: &MaterialTable,
        mesh: MeshId,
    ) -> bool {
        if let Some(m) = self.meshes.get(&mesh.0) {
            return m.is_some();
        }
        let made = scene.mesh(mesh).and_then(|m| {
            let solid: Vec<(u32, u32, u32)> = m
                .draws
                .iter()
                .filter(|d| d.pipeline.deferred())
                .filter(|d| {
                    materials.material(d.material).is_some_and(|r| {
                        r.flags
                            & (flags::CUTOUT
                                | flags::FOLIAGE
                                | flags::TRANSLUCENT
                                | flags::ADDITIVE)
                            == 0
                    })
                })
                .map(|d| (d.first_index, d.index_count, d.material))
                .filter(|d| d.1 >= 3)
                .collect();
            if solid.is_empty() {
                return None;
            }
            let vertex_count = u32::try_from(m.first_vertex + m.vertex_count).ok()?;
            let sizes: Vec<wgpu::BlasTriangleGeometrySizeDescriptor> = solid
                .iter()
                .map(|&(_, count, _)| wgpu::BlasTriangleGeometrySizeDescriptor {
                    vertex_format: wgpu::VertexFormat::Float32x3,
                    vertex_count,
                    index_format: Some(wgpu::IndexFormat::Uint32),
                    index_count: Some(count),
                    // Not opaque: a lamp's rays pass through its own object.
                    flags: wgpu::AccelerationStructureGeometryFlags::empty(),
                })
                .collect();
            let blas = gpu.device.create_blas(
                &wgpu::CreateBlasDescriptor {
                    label: Some("mesh"),
                    flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
                    update_mode: wgpu::AccelerationStructureUpdateMode::Build,
                },
                wgpu::BlasGeometrySizeDescriptors::Triangles {
                    descriptors: sizes.clone(),
                },
            );
            // Its draws in the table the bounce light's rays read.
            let geom_base = self
                .draws_used
                .get_or_insert_with(|| crate::pool::RangeAllocator::new(MAX_RAY_DRAWS))
                .alloc(solid.len() as u64, 1)?;
            let geom_base = u32::try_from(geom_base).ok()?;
            let bytes: Vec<u8> = solid
                .iter()
                .flat_map(|&(first, _, material)| [first, material])
                .flat_map(u32::to_le_bytes)
                .collect();
            if let Some(b) = self.draws.as_ref() {
                gpu.queue.write_buffer(b, u64::from(geom_base) * 8, &bytes);
            }
            Some(MeshBlas {
                blas,
                sizes,
                draws: solid.iter().map(|&(f, c, _)| (f, c)).collect(),
                geom_base,
                built: false,
            })
        });
        let ok = made.is_some();
        self.meshes.insert(mesh.0, made);
        ok
    }

    /// Brings the structures up to date for an eye at `eye`, recording their builds into `enc`:
    /// new meshes' bottom levels, and the top level when the eye has moved far enough, the
    /// instances changed, or bodies move. Whether a top level is ready.
    pub fn update(
        &mut self,
        gpu: &Gpu,
        scene: &Scene,
        materials: &MaterialTable,
        eye: Vec3,
        enc: &mut wgpu::CommandEncoder,
    ) -> bool {
        if !gpu.rays() {
            return false;
        }
        if self.draws.is_none() {
            let buffer = |label, size| {
                gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            };
            self.draws = Some(buffer("ray draws", MAX_RAY_DRAWS * 8));
            self.instance_table = Some(buffer("ray instances", MAX_RAY_INSTANCES as u64 * 4));
        }
        let total = scene.instance_count() as usize;
        let moving = scene.cull_range().dynamic_count > 0 || !self.deforming.is_empty();
        let stale = self
            .built_at
            .is_none_or(|p| p.distance(eye) > REBUILD_DISTANCE)
            || total != self.built_instances
            || moving;
        if !stale {
            return self.tlas.is_some();
        }
        let started = std::time::Instant::now();
        // The instances round the eye, nearest first: the static ones from those gathered while
        // the eye stays within the rebuild distance of where they were, then the dynamic ones, in
        // the scene's slot order as a walk of every instance gives them.
        let reach = |i: &crate::scene::Instance| {
            let m = scene.mesh(i.mesh)?;
            let c = i.transform.transform_point3(m.center);
            Some(c.distance(eye) - m.radius * i.transform.matrix3.x_axis.length())
        };
        let epoch = scene.static_epoch();
        if self
            .statics_at
            .is_none_or(|(p, e)| e != epoch || p.distance(eye) > REBUILD_DISTANCE)
        {
            let base = scene.dynamic_base();
            self.statics.clear();
            self.statics.extend(
                scene
                    .instances()
                    .take_while(|(id, _)| *id < base)
                    .filter(|(_, i)| reach(i).is_some_and(|d| d < STATIC_REACH))
                    .map(|(id, _)| id),
            );
            self.statics_at = Some((eye, epoch));
        }
        let near_of = |i: &crate::scene::Instance| {
            reach(i)
                .filter(|&d| d < RAY_RADIUS)
                .map(|d| (d, i.transform, i.mesh, i.object))
        };
        let mut near: Vec<(f32, glam::Affine3A, MeshId, u32)> = self
            .statics
            .iter()
            .filter_map(|&id| scene.instance(id))
            .chain(scene.dynamic_instances().map(|(_, i)| i))
            .filter_map(near_of)
            .collect();
        #[cfg(debug_assertions)]
        {
            let every: Vec<_> = scene.instances().filter_map(|(_, i)| near_of(i)).collect();
            debug_assert!(
                every == near,
                "the gathered statics miss an instance in reach"
            );
        }
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(MAX_RAY_INSTANCES);
        near.retain(|(_, _, m, _)| self.mesh_blas(gpu, scene, materials, *m));
        let mut tlas = gpu.device.create_tlas(&wgpu::CreateTlasDescriptor {
            label: Some("lamp rays"),
            max_instances: u32::try_from(near.len().max(1)).unwrap_or(1),
            flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
            update_mode: wgpu::AccelerationStructureUpdateMode::Build,
        });
        let mut to_build = Vec::new();
        let mut table = Vec::with_capacity(near.len() * 4);
        for (k, (_, at, mesh, object)) in near.iter().enumerate() {
            let Some(Some(m)) = self.meshes.get(&mesh.0) else {
                table.extend_from_slice(&0u32.to_le_bytes());
                continue;
            };
            table.extend_from_slice(&m.geom_base.to_le_bytes());
            if !m.built || self.deforming.contains(&mesh.0) {
                to_build.push(mesh.0);
            }
            let r = at.matrix3;
            let t = at.translation;
            tlas[k] = Some(wgpu::TlasInstance::new(
                &m.blas,
                [
                    r.x_axis.x, r.y_axis.x, r.z_axis.x, t.x, r.x_axis.y, r.y_axis.y, r.z_axis.y,
                    t.y, r.x_axis.z, r.y_axis.z, r.z_axis.z, t.z,
                ],
                object & 0xFF_FFFF,
                0xFF,
            ));
        }
        to_build.sort_unstable();
        to_build.dedup();
        let built = u32::try_from(to_build.len()).unwrap_or(u32::MAX);
        let entries: Vec<wgpu::BlasBuildEntry<'_>> = to_build
            .iter()
            .filter_map(|id| self.meshes.get(id).and_then(Option::as_ref))
            .map(|m| wgpu::BlasBuildEntry {
                blas: &m.blas,
                geometry: wgpu::BlasGeometries::TriangleGeometries(
                    m.sizes
                        .iter()
                        .zip(&m.draws)
                        .map(|(size, &(first, _))| wgpu::BlasTriangleGeometry {
                            size,
                            vertex_buffer: &scene.vertices.buffer,
                            first_vertex: 0,
                            vertex_stride: crate::asset::mesh::VERTEX_SIZE,
                            index_buffer: Some(&scene.indices.buffer),
                            first_index: Some(first),
                            transform_buffer: None,
                            transform_buffer_offset: None,
                        })
                        .collect(),
                ),
            })
            .collect();
        enc.build_acceleration_structures(entries.iter(), std::iter::once(&tlas));
        drop(entries);
        for id in &to_build {
            if let Some(Some(m)) = self.meshes.get_mut(id) {
                m.built = true;
            }
        }
        if let (Some(b), false) = (self.instance_table.as_ref(), table.is_empty()) {
            gpu.queue.write_buffer(b, 0, &table);
        }
        self.instances = near.len();
        self.generation += 1;
        self.tlas = Some(tlas);
        self.built_at = Some(eye);
        self.built_instances = total;
        let ms = crate::frame_work::ms_since(started);
        crate::frame_work::note(|w| {
            w.blas += built;
            w.tlas += 1;
            w.rays_ms += ms;
        });
        true
    }
}
