//! A `GfxObj`'s drawing polygons as an indexed mesh: one sub-mesh per surface and side.
//!
//! A polygon is an n-gon drawn as the fan `(0, i, i + 1)`. Its positive side draws with
//! `pos_surface` and `pos_uv_indices`; a two-sided polygon (`sides_type == 1`) draws the same
//! surface from both sides, so it goes to a sub-mesh drawn without culling; a polygon with a
//! negative surface (`sides_type == 2`) also draws its back, as a reversed fan with
//! `neg_surface`, `neg_uv_indices` and the normal turned round.

use std::collections::HashMap;

use pyreal_assets::geometry::CellStruct;
use pyreal_assets::{GfxObj, Polygon, SwVertex};
use pyreal_primitives::DataId;

/// One vertex as the pools hold it (32 bytes): position, octahedral normal, texture coordinate
/// and two words a pipeline reads its own way (the terrain's cell record, for one).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub extra: [u32; 2],
}

/// The bytes of one vertex in the pool's layout.
pub const VERTEX_SIZE: u64 = 32;

impl Vertex {
    /// Appends this vertex in the pool's layout.
    pub fn write(&self, out: &mut Vec<u8>) {
        for v in self.position {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&oct_encode_snorm16(self.normal).to_le_bytes());
        for v in self.uv {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in self.extra {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
}

/// A unit vector folded onto the octahedron, in `[-1, 1]²`.
#[must_use]
pub fn oct_encode(n: [f32; 3]) -> [f32; 2] {
    let len = n[0].abs() + n[1].abs() + n[2].abs();
    if len <= 0.0 {
        return [0.0, 0.0];
    }
    let (x, y, z) = (n[0] / len, n[1] / len, n[2] / len);
    if z >= 0.0 {
        [x, y]
    } else {
        let sx = if x >= 0.0 { 1.0 } else { -1.0 };
        let sy = if y >= 0.0 { 1.0 } else { -1.0 };
        [(1.0 - y.abs()) * sx, (1.0 - x.abs()) * sy]
    }
}

/// [`oct_encode`]'s inverse, normalised.
#[must_use]
pub fn oct_decode(e: [f32; 2]) -> [f32; 3] {
    let (mut x, mut y) = (e[0], e[1]);
    let z = 1.0 - x.abs() - y.abs();
    if z < 0.0 {
        let (ox, oy) = (x, y);
        x = (1.0 - oy.abs()) * if ox >= 0.0 { 1.0 } else { -1.0 };
        y = (1.0 - ox.abs()) * if oy >= 0.0 { 1.0 } else { -1.0 };
    }
    let len = (x * x + y * y + z * z).sqrt();
    [x / len, y / len, z / len]
}

/// [`oct_encode`] as two signed 16-bit normalised values, x in the low half.
#[must_use]
pub fn oct_encode_snorm16(n: [f32; 3]) -> u32 {
    let [x, y] = oct_encode(n);
    let q = |v: f32| -> u32 {
        let s = pyreal_primitives::num::to_i32((v.clamp(-1.0, 1.0) * 32767.0).round());
        u32::from(i16::try_from(s).unwrap_or(0).cast_unsigned())
    };
    q(x) | (q(y) << 16)
}

/// Which way a sub-mesh is culled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Sides {
    /// Back faces culled.
    One,
    /// Drawn from both sides (`sides_type == 1`).
    Both,
}

/// One surface's triangles within a mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct SubMesh {
    /// Index of the surface in the object's own surface list.
    pub surface_index: u16,
    /// That surface's id, `None` when the list is short.
    pub surface: Option<DataId>,
    pub sides: Sides,
    pub first_index: u32,
    pub index_count: u32,
}

/// A mesh ready for the pools: vertices, indices local to them, its sub-meshes and a bounding
/// sphere.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub submeshes: Vec<SubMesh>,
    pub center: [f32; 3],
    pub radius: f32,
}

impl MeshData {
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Sets the bounding sphere to the vertices' box centre and the farthest vertex from it.
    pub fn fit_bounds(&mut self) {
        if self.vertices.is_empty() {
            return;
        }
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for v in &self.vertices {
            for k in 0..3 {
                lo[k] = lo[k].min(v.position[k]);
                hi[k] = hi[k].max(v.position[k]);
            }
        }
        let c = [0, 1, 2].map(|k| (lo[k] + hi[k]) * 0.5);
        let r2 = self
            .vertices
            .iter()
            .map(|v| (0..3).map(|k| (v.position[k] - c[k]).powi(2)).sum::<f32>())
            .fold(0.0f32, f32::max);
        self.center = c;
        self.radius = r2.sqrt();
    }
}

/// The UV a polygon corner takes: index zero when the polygon has no index list, nothing when the
/// index is negative as a signed byte or out of range.
fn corner_uv(indices: Option<&[u8]>, corner: usize, uvs: &[(f32, f32)]) -> (Option<u8>, [f32; 2]) {
    let index = match indices {
        None => 0,
        Some(indices) => match indices.get(corner) {
            Some(&index) if index < 128 => index,
            _ => return (None, [0.0, 0.0]),
        },
    };
    let uv = uvs
        .get(usize::from(index))
        .map_or([0.0, 0.0], |&(u, v)| [u, v]);
    (Some(index), uv)
}

/// The fan of one polygon side into its sub-mesh, sharing a vertex wherever the same corner
/// recurs with the same UV.
struct Builder<'a> {
    vertices: &'a [SwVertex],
    surfaces: &'a [DataId],
    groups: Vec<(u16, Sides, Vec<u32>)>,
    group_of: HashMap<(u16, Sides), usize>,
    out: Vec<Vertex>,
    shared: HashMap<(u16, Option<u8>, bool, u32), u32>,
    /// Polygon sides seen, so a corner given its face's normal is not shared.
    sides_seen: u32,
}

/// A corner whose normal leans further than this (cosine) from its polygon's face takes the
/// face's: the dats smooth a room's corners into its walls, which bends a lamp's light away.
pub const FLAT_BELOW: f32 = 0.8;
/// Only on a polygon at least this wide, metres: a smaller shape's smoothing (a crystal, a blade,
/// a limb) is its look, as retail's corner light drew it.
pub const FLAT_SPAN: f32 = 1.0;

/// A polygon's face normal (Newell's), unit length, or `None` when it has no area.
fn face_normal(corners: &[[f32; 3]]) -> Option<[f32; 3]> {
    let mut n = [0.0f32; 3];
    for (k, a) in corners.iter().enumerate() {
        let b = corners[(k + 1) % corners.len()];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    (len > 1e-9).then(|| n.map(|c| c / len))
}

/// The widest distance between two of a polygon's corners.
fn span(corners: &[[f32; 3]]) -> f32 {
    let mut most = 0.0f32;
    for (k, a) in corners.iter().enumerate() {
        for b in &corners[k + 1..] {
            most = most.max((0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>());
        }
    }
    most.sqrt()
}

impl Builder<'_> {
    fn side(&mut self, p: &Polygon, surface: u16, uv_indices: Option<&[u8]>, back: bool) {
        let sides = if p.sides_type == 1 {
            Sides::Both
        } else {
            Sides::One
        };
        let n = usize::from(p.num_pts);
        let mut at = Vec::with_capacity(n);
        let mut sum = [0.0f32; 3];
        for k in 0..n {
            let v = p
                .vertex_ids
                .get(k)
                .and_then(|&vi| self.vertices.get(usize::from(vi)));
            let Some(v) = v else {
                return;
            };
            at.push([v.position.x, v.position.y, v.position.z]);
            sum = [
                sum[0] + v.normal.x,
                sum[1] + v.normal.y,
                sum[2] + v.normal.z,
            ];
        }
        // Turned to the side the corners' normals face, whichever way the polygon winds.
        let face = face_normal(&at)
            .filter(|_| span(&at) >= FLAT_SPAN)
            .map(|f| {
                if f[0] * sum[0] + f[1] * sum[1] + f[2] * sum[2] < 0.0 {
                    f.map(|c| -c)
                } else {
                    f
                }
            });
        self.sides_seen += 1;
        let mut corners = Vec::with_capacity(n);
        for k in 0..n {
            let vi = p.vertex_ids[k];
            let v = &self.vertices[usize::from(vi)];
            let (uv_index, uv) = corner_uv(uv_indices, k, &v.uvs);
            let own = [v.normal.x, v.normal.y, v.normal.z];
            let flat = face.filter(|f| f[0] * own[0] + f[1] * own[1] + f[2] * own[2] < FLAT_BELOW);
            let key = (
                vi,
                uv_index,
                back,
                flat.map_or(u32::MAX, |_| self.sides_seen),
            );
            let index = *self.shared.entry(key).or_insert_with(|| {
                let normal = flat.unwrap_or(own);
                let normal = if back { normal.map(|c| -c) } else { normal };
                self.out.push(Vertex {
                    position: [v.position.x, v.position.y, v.position.z],
                    normal,
                    uv,
                    extra: [0, 0],
                });
                u32::try_from(self.out.len() - 1).unwrap_or(0)
            });
            corners.push(index);
        }
        let g = *self.group_of.entry((surface, sides)).or_insert_with(|| {
            self.groups.push((surface, sides, Vec::new()));
            self.groups.len() - 1
        });
        let tris = &mut self.groups[g].2;
        for i in 1..n - 1 {
            if back {
                tris.extend_from_slice(&[corners[0], corners[i + 1], corners[i]]);
            } else {
                tris.extend_from_slice(&[corners[0], corners[i], corners[i + 1]]);
            }
        }
    }

    fn finish(self) -> MeshData {
        let mut mesh = MeshData {
            vertices: self.out,
            ..MeshData::default()
        };
        // One-sided sub-meshes first, then two-sided, each in first-appearance order.
        let mut groups = self.groups;
        groups.sort_by_key(|g| g.1);
        for (surface_index, sides, tris) in groups {
            if tris.is_empty() {
                continue;
            }
            mesh.submeshes.push(SubMesh {
                surface_index,
                surface: self.surfaces.get(usize::from(surface_index)).copied(),
                sides,
                first_index: u32::try_from(mesh.indices.len()).unwrap_or(0),
                index_count: u32::try_from(tris.len()).unwrap_or(0),
            });
            mesh.indices.extend(tris);
        }
        mesh.fit_bounds();
        mesh
    }
}

fn build<'a>(
    vertices: &[SwVertex],
    polygons: impl IntoIterator<Item = &'a Polygon>,
    surfaces: &[DataId],
) -> MeshData {
    let mut b = Builder {
        vertices,
        surfaces,
        groups: Vec::new(),
        group_of: HashMap::new(),
        out: Vec::new(),
        shared: HashMap::new(),
        sides_seen: 0,
    };
    for p in polygons {
        if p.num_pts < 3 {
            continue;
        }
        b.side(p, p.pos_surface, p.pos_uv_indices.as_deref(), false);
        if p.sides_type == 2 {
            b.side(p, p.neg_surface, p.neg_uv_indices.as_deref(), true);
        }
    }
    b.finish()
}

/// A `GfxObj`'s drawing polygons as a mesh. An object with no drawing polygons (physics only)
/// comes back empty. The polygons its drawing BSP's portal nodes name (a building's doorways)
/// are openings and are left out, as a cell's are: retail only stamps depth through them, and
/// some carry an opaque colour (0xAAB3's pyramid, a solid blue).
#[must_use]
pub fn gfxobj_mesh(obj: &GfxObj) -> MeshData {
    let portals: std::collections::HashSet<usize> = obj
        .drawing_bsp
        .iter()
        .flat_map(|bsp| &bsp.nodes)
        .flat_map(|n| &n.in_portals)
        .filter_map(|&(poly, _)| usize::try_from(poly).ok())
        .collect();
    let polygons = obj
        .polygons
        .iter()
        .enumerate()
        .filter_map(|(i, p)| (!portals.contains(&i)).then_some(p));
    build(&obj.vertex_array.vertices, polygons, &obj.surfaces)
}

/// An indoor cell's drawing polygons as a mesh, against the cell's own surface list. Its portal
/// polygons are openings and are left out (their surfaces are wholly transparent).
#[must_use]
pub fn cell_mesh(cs: &CellStruct, surfaces: &[DataId]) -> MeshData {
    let polygons = cs.polygons.iter().enumerate().filter_map(|(i, p)| {
        let portal = u16::try_from(i).is_ok_and(|i| cs.portals.contains(&i));
        (!portal).then_some(p)
    });
    build(&cs.vertex_array.vertices, polygons, surfaces)
}
