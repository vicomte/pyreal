//! The ground: exact retail land cells, with the terrain textures blended in the shader.
//!
//! Each block is the retail mesh at full detail ([`pyreal_terrain::land::mesh`]), with its split
//! direction and its cells' UV rotation. Each cell carries a record of the layers it lays, as the
//! client's texture merge would composite them ([`ground_plan`]): the ground of one terrain type,
//! then each other type and the road blended in through an alpha map at a rotation. The dats
//! before Throne of Destiny palette-shift their ground instead: one picture per cell, recoloured
//! from its corners' terrain palettes, which is a palette variant here.

use pyreal_assets::{CellLandblock, Region};
use pyreal_primitives::DataId;
use pyreal_terrain::consts::{CELL_SIZE, LAND_UVS};
use pyreal_terrain::land::drawn::BlockField;
use pyreal_terrain::land::merge::{cell_rotation_keys, cell_x, cell_y, ground_plan};
use pyreal_terrain::land::mesh::{
    generate_landblock_with_table, sw_to_ne_cut, Direction, LandblockMesh,
};
use pyreal_terrain::land::palshift;
use pyreal_terrain::land::water::WaterType;

use crate::asset::material::NO_TEXTURE;
use crate::asset::mesh::{MeshData, Sides, SubMesh, Vertex};
use crate::asset::texture::PaletteSwap;

/// Layers one cell can lay: the ground, three terrain overlays and two road overlays.
pub const MAX_LAYERS: usize = 6;
/// The bytes of one cell record.
pub const CELL_RECORD_SIZE: u64 = 16 + 16 * MAX_LAYERS as u64;

/// [`Layer::ground`] where the layer's terrain type is not known.
pub const NO_GROUND: u32 = 0xFF;

/// One layer of a cell's ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layer {
    /// The ground picture's texture slot.
    pub texture: u32,
    /// The alpha map's slot, [`NO_TEXTURE`] for the bottom layer.
    pub alpha: u32,
    /// The alpha map's rotation, 0 to 3 quarter turns.
    pub rotation: u32,
    /// The ground's [`crate::asset::material::SurfaceKind`] index.
    pub kind: u32,
    /// The terrain type it lays, [`NO_GROUND`] where none is known.
    pub ground: u32,
    /// Repeats of the picture across one cell.
    pub tiling: f32,
}

/// A cell's ground, bottom layer first.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CellRecord {
    pub layers: Vec<Layer>,
    pub water: bool,
    /// The cell volumes over this cell that cut the ground away: a range of the cut lists.
    pub cut_first: u32,
    pub cut_count: u32,
}

impl CellRecord {
    pub fn write(&self, out: &mut Vec<u8>) {
        let n = u32::try_from(self.layers.len().min(MAX_LAYERS)).unwrap_or(0);
        for v in [n, u32::from(self.water), self.cut_first, self.cut_count] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for k in 0..MAX_LAYERS {
            let l = self.layers.get(k).copied().unwrap_or(Layer {
                texture: 0,
                alpha: NO_TEXTURE,
                rotation: 0,
                kind: 0,
                ground: NO_GROUND,
                tiling: 1.0,
            });
            for v in [
                l.texture,
                l.alpha,
                l.rotation | (l.kind << 8) | (l.ground.min(NO_GROUND) << 16),
                l.tiling.to_bits(),
            ] {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
}

/// Where the ground's pictures come from: texture slots for the terrain types, the alpha maps
/// and the palette-shifted pictures.
pub trait GroundTextures {
    /// A terrain type's picture: its slot, its repeats across a cell (negative: metres a
    /// repeat covers, laid on the world's axes) and its kind's index.
    fn ground(&mut self, terrain_type: u32) -> Option<(u32, f32, u32)>;
    /// An alpha map's slot.
    fn alpha(&mut self, id: DataId) -> Option<u32>;
    /// A palette-shifted picture's slot, and the kind of terrain type `kind_of` names.
    fn shifted(&mut self, texture: DataId, swap: PaletteSwap, kind_of: u32) -> Option<(u32, u32)>;
    /// The palette a picture names for itself.
    fn own_palette(&mut self, texture: DataId) -> Option<DataId>;
}

/// One block of ground, ready for the pools: block-local vertices and its cells' records, the
/// cell records numbered from `first_cell`.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundBlock {
    pub mesh: MeshData,
    pub cells: Vec<CellRecord>,
    pub land: LandblockMesh,
    /// Each cell's vertices in the mesh.
    pub cell_vertices: Vec<std::ops::Range<usize>>,
}

/// Height at a world vertex `(gx, gy)` (block times eight plus vertex), for smooth normals that
/// cross block edges.
pub type Heights<'a> = &'a dyn Fn(i32, i32) -> Option<f32>;

/// Builds one block's ground at full detail: retail's triangles, or with `drawn` (Landscape
/// Smoothing) each cell [`crate::drawn::SUB`] quads a side at the drawn offset.
#[allow(clippy::too_many_arguments)]
pub fn build_block(
    lb: &CellLandblock,
    region: &Region,
    table: &[f32; 256],
    bx: i32,
    by: i32,
    heights: Heights<'_>,
    first_cell: u32,
    textures: &mut dyn GroundTextures,
    drawn: Option<&BlockField>,
) -> GroundBlock {
    let land =
        generate_landblock_with_table(lb, region, table, bx, by, 1, Direction::InViewerBlock);
    let side = usize::from(land.side_cell_count);
    let svc = usize::from(land.side_vertex_count);
    let own = |a: usize, b: usize| land.vertices[a * svc + b].z;
    let normal_at = |a: usize, b: usize| -> [f32; 3] {
        // LINT-OK: vertex indices within one block's 9x9 grid.
        let (gx, gy) = (
            bx * 8 + i32::try_from(a).unwrap_or(0),
            by * 8 + i32::try_from(b).unwrap_or(0),
        );
        let h = |dx: i32, dy: i32| {
            heights(gx + dx, gy + dy).unwrap_or_else(|| {
                let (x, y) = (
                    usize::try_from((i32::try_from(a).unwrap_or(0) + dx).clamp(0, 8)).unwrap_or(0),
                    usize::try_from((i32::try_from(b).unwrap_or(0) + dy).clamp(0, 8)).unwrap_or(0),
                );
                own(x, y)
            })
        };
        let nx = (h(-1, 0) - h(1, 0)) / (2.0 * CELL_SIZE);
        let ny = (h(0, -1) - h(0, 1)) / (2.0 * CELL_SIZE);
        let len = (nx * nx + ny * ny + 1.0).sqrt();
        [nx / len, ny / len, 1.0 / len]
    };
    let retail = |x: usize, y: usize| {
        let (ci, cj) = ((x / 24).min(7), (y / 24).min(7));
        #[allow(clippy::cast_precision_loss)]
        let f = glam::Vec2::new((x - ci * 24) as f32, (y - cj * 24) as f32) / CELL_SIZE;
        let (gx, gy) = (
            bx * 8 + i32::try_from(ci).unwrap_or(0),
            by * 8 + i32::try_from(cj).unwrap_or(0),
        );
        crate::land::on_triangles(
            sw_to_ne_cut(gx, gy),
            [
                own(ci, cj),
                own(ci + 1, cj),
                own(ci, cj + 1),
                own(ci + 1, cj + 1),
            ],
            f,
        )
    };
    let mut mesh = MeshData::default();
    let mut cells = Vec::with_capacity(side * side);
    let mut cell_vertices = Vec::with_capacity(side * side);
    for i in 0..side {
        for j in 0..side {
            let n = i * side + j;
            let cell = first_cell + u32::try_from(n).unwrap_or(0);
            let (p0, p1) = land.cell_polygons(i, j);
            let first = mesh.vertices.len();
            if let Some(field) = drawn {
                let corner = |di: usize, dj: usize| {
                    let vi = (i + di) * svc + j + dj;
                    let uv = [p0, p1]
                        .iter()
                        .flat_map(|p| (0..3).map(move |k| (p.v[k] as usize, p.uv_indices[k])))
                        .find(|(v, _)| *v == vi)
                        .map_or([0.0, 0.0], |(_, u)| LAND_UVS[usize::from(u)]);
                    (land.vertices[vi].z, normal_at(i + di, j + dj), uv)
                };
                let has = |p: &pyreal_terrain::land::mesh::LandPolygon, vi: usize| {
                    p.v.iter().any(|v| *v as usize == vi)
                };
                let (sw, ne) = (i * svc + j, (i + 1) * svc + j + 1);
                let sw_to_ne = [p0, p1].iter().any(|p| has(p, sw) && has(p, ne));
                fine_cell(
                    &mut mesh,
                    field,
                    (i, j),
                    [corner(0, 0), corner(1, 0), corner(0, 1), corner(1, 1)],
                    sw_to_ne,
                    &retail,
                    cell,
                );
                cell_vertices.push(first..mesh.vertices.len());
                let mut record = cell_record(lb, region, &land, i, j, textures);
                record.water = land.cell_water.get(n) == Some(&WaterType::EntirelyWater);
                cells.push(record);
                continue;
            }
            for p in [p0, p1] {
                for k in 0..3 {
                    let vi = p.v[k] as usize;
                    let pos = land.vertices[vi];
                    let uv = LAND_UVS[usize::from(p.uv_indices[k])];
                    mesh.indices
                        .push(u32::try_from(mesh.vertices.len()).unwrap_or(0));
                    mesh.vertices.push(Vertex {
                        position: [pos.x, pos.y, pos.z],
                        normal: normal_at(vi / svc, vi % svc),
                        uv,
                        extra: [cell, 0],
                    });
                }
            }
            cell_vertices.push(first..mesh.vertices.len());
            let mut record = cell_record(lb, region, &land, i, j, textures);
            record.water = land.cell_water.get(n) == Some(&WaterType::EntirelyWater);
            cells.push(record);
        }
    }
    mesh.submeshes.push(SubMesh {
        surface_index: 0,
        surface: None,
        sides: Sides::One,
        first_index: 0,
        index_count: u32::try_from(mesh.indices.len()).unwrap_or(0),
    });
    mesh.fit_bounds();
    GroundBlock {
        mesh,
        cells,
        land,
        cell_vertices,
    }
}

/// One corner of a cell: retail's height, its smooth normal and its picture's coordinates.
type Corner = (f32, [f32; 3], [f32; 2]);

/// Cell `(i, j)` drawn [`crate::drawn::SUB`] quads a side at `field`'s offset, its quads cut as
/// retail cuts the cell, so a vertex at no offset lies on retail's triangle. Each vertex's normal
/// is the drawn surface's where the offset is free and retail's smooth one where it is pinned.
/// `retail` is retail's height at block-local metre `(x, y)`, 0 to 192 each.
#[allow(clippy::too_many_arguments)]
fn fine_cell(
    mesh: &mut MeshData,
    field: &BlockField,
    (i, j): (usize, usize),
    [c00, c10, c01, c11]: [Corner; 4],
    sw_to_ne: bool,
    retail: &dyn Fn(usize, usize) -> f32,
    cell: u32,
) {
    use crate::drawn::SUB;
    let tri = |a: f32, b: f32, c: f32, d: f32, f: glam::Vec2| {
        crate::land::on_triangles(sw_to_ne, [a, b, c, d], f)
    };
    let drawn = |x: usize, y: usize| retail(x, y) + crate::drawn::at_sample(field, x, y).0;
    let base = u32::try_from(mesh.vertices.len()).unwrap_or(0);
    let step = 24 / SUB;
    for v in 0..=SUB {
        for u in 0..=SUB {
            #[allow(clippy::cast_precision_loss)]
            let f = glam::Vec2::new(u as f32, v as f32) / SUB as f32;
            let (x, y) = (i * 24 + u * step, j * 24 + v * step);
            let (offset, weight) = crate::drawn::at_sample(field, x, y);
            let z = tri(c00.0, c10.0, c01.0, c11.0, f) + offset;
            let smooth = glam::Vec3::from_array(std::array::from_fn(|k| {
                tri(c00.1[k], c10.1[k], c01.1[k], c11.1[k], f)
            }))
            .normalize_or(glam::Vec3::Z);
            let (xa, xb, ya, yb) = (
                x.saturating_sub(1),
                (x + 1).min(192),
                y.saturating_sub(1),
                (y + 1).min(192),
            );
            #[allow(clippy::cast_precision_loss)]
            let slope = glam::Vec2::new(
                (drawn(xb, y) - drawn(xa, y)) / (xb - xa) as f32,
                (drawn(x, yb) - drawn(x, ya)) / (yb - ya) as f32,
            );
            let own_normal = slope.extend(-1.0).normalize() * -1.0;
            let n = smooth.lerp(own_normal, weight).normalize_or(glam::Vec3::Z);
            let uv = [
                tri(c00.2[0], c10.2[0], c01.2[0], c11.2[0], f),
                tri(c00.2[1], c10.2[1], c01.2[1], c11.2[1], f),
            ];
            #[allow(clippy::cast_precision_loss)]
            mesh.vertices.push(Vertex {
                position: [x as f32, y as f32, z],
                normal: n.to_array(),
                uv,
                extra: [cell, 0],
            });
        }
    }
    let k = |u: usize, v: usize| base + u32::try_from(v * (SUB + 1) + u).unwrap_or(0);
    for v in 0..SUB {
        for u in 0..SUB {
            let (a, b, c, d) = (k(u, v), k(u + 1, v), k(u, v + 1), k(u + 1, v + 1));
            if sw_to_ne {
                mesh.indices.extend([a, b, d, a, d, c]);
            } else {
                mesh.indices.extend([a, b, c, d, c, b]);
            }
        }
    }
}

/// The layers of cell `(i, j)`.
fn cell_record(
    lb: &CellLandblock,
    region: &Region,
    land: &LandblockMesh,
    i: usize,
    j: usize,
    textures: &mut dyn GroundTextures,
) -> CellRecord {
    let side = usize::from(land.side_cell_count);
    let mut record = CellRecord::default();
    if let Some(tm) = &region.land_surf.tex_merge {
        let (key, _) = land.cell_keys[i * side + j];
        let plan = ground_plan(tm, key);
        if let Some((ground, (texture, tiling, kind))) =
            plan.base.and_then(|t| textures.ground(t).map(|g| (t, g)))
        {
            record.layers.push(Layer {
                texture,
                alpha: NO_TEXTURE,
                rotation: 0,
                kind,
                ground,
                tiling,
            });
        }
        for o in &plan.overlays {
            let (Some((texture, tiling, kind)), Some(alpha)) = (
                o.ground.and_then(|t| textures.ground(t)),
                textures.alpha(o.alpha),
            ) else {
                continue;
            };
            record.layers.push(Layer {
                texture,
                alpha,
                rotation: u32::try_from(o.rotation.index()).unwrap_or(0),
                kind,
                ground: o.ground.unwrap_or(NO_GROUND),
                tiling,
            });
        }
    } else if let Some(ps) = &region.land_surf.pal_shift {
        let (keys, _) = cell_rotation_keys(lb, region, side, i, j);
        let choice = palshift::select(ps, &keys, cell_x(lb, i), cell_y(lb, j));
        if let Some(t) = ps.textures.get(choice.texture) {
            let ranges = palshift::sub_palettes(ps, &choice);
            if let Some(base) = textures.own_palette(t.tex_gid) {
                let corner = keys[0].corner_types()[0];
                let swap = PaletteSwap {
                    base,
                    ranges,
                    wire: false,
                };
                if let Some((texture, kind)) = textures.shifted(t.tex_gid, swap, corner) {
                    record.layers.push(Layer {
                        texture,
                        alpha: NO_TEXTURE,
                        rotation: 0,
                        kind,
                        ground: corner,
                        tiling: 1.0,
                    });
                }
            }
        }
    }
    record
}

/// The fewest repeats of a dat ground picture across a cell: the older eras' 128-texel 3D-card
/// pictures are drawn as densely as end of retail's, which all repeat twice.
pub const LEAST_GROUND_REPEATS: u32 = 2;

/// A dat picture's repeats across a cell as the layer records them.
#[must_use]
pub fn tiling_f32(t: u32) -> f32 {
    f32::from(u16::try_from(t.max(1)).unwrap_or(1))
}
