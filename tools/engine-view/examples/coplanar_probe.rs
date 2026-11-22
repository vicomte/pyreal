//! Lists drawn polygons of a dungeon block that lie in one plane, face the same way and overlap:
//! the pairs the depth buffer cannot order, from cell to cell, cell to object or object to object.
//!
//! Usage: `coplanar_probe DAT_DIR BLOCK [X,Y,Z RADIUS]` (BLOCK as 0x5A45; the point in the
//! block's metres, only polygons within RADIUS of it).

use std::collections::HashMap;

use glam::{Mat4, Quat, Vec3};
use pyreal_assets::geometry::{Environment, GfxObj};
use pyreal_assets::world::EnvCell;
use pyreal_assets::{Decode, Setup};
use pyreal_dat::{divine_type, DbType, RetailDatStore};
use pyreal_primitives::DataId;

fn read<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

fn mat(f: &pyreal_primitives::Frame) -> Mat4 {
    let r = Quat::from_xyzw(f.rotation.x, f.rotation.y, f.rotation.z, f.rotation.w);
    Mat4::from_rotation_translation(r, Vec3::new(f.origin.x, f.origin.y, f.origin.z))
}

struct Poly {
    source: String,
    surface: u32,
    pts: Vec<Vec3>,
    normal: Vec3,
    d: f32,
}

fn poly(source: String, surface: u32, pts: Vec<Vec3>) -> Option<Poly> {
    let mut n = Vec3::ZERO;
    for (i, a) in pts.iter().enumerate() {
        let b = pts[(i + 1) % pts.len()];
        n += Vec3::new(
            (a.y - b.y) * (a.z + b.z),
            (a.z - b.z) * (a.x + b.x),
            (a.x - b.x) * (a.y + b.y),
        );
    }
    let normal = n.try_normalize()?;
    let d = normal.dot(pts[0]);
    Some(Poly {
        source,
        surface,
        pts,
        normal,
        d,
    })
}

fn add_mesh(
    out: &mut Vec<Poly>,
    source: &str,
    to_block: Mat4,
    verts: &[pyreal_assets::common::SwVertex],
    polys: &[pyreal_assets::common::Polygon],
    surfaces: &dyn Fn(u16) -> u32,
    skip: &[i16],
) {
    let by_id: HashMap<i16, Vec3> = verts
        .iter()
        .map(|v| {
            let p = Vec3::new(v.position.x, v.position.y, v.position.z);
            (v.id, to_block.transform_point3(p))
        })
        .collect();
    for p in polys.iter().filter(|p| !skip.contains(&p.poly_id)) {
        let pts: Option<Vec<Vec3>> = p
            .vertex_ids
            .iter()
            .map(|i| by_id.get(&(*i as i16)).copied())
            .collect();
        if let Some(pts) = pts {
            let src = format!("{source} poly {}", p.poly_id);
            out.extend(poly(src, surfaces(p.pos_surface), pts));
        }
    }
}

/// The overlap of two convex polygons in their shared plane, square metres.
fn overlap(a: &Poly, b: &Poly) -> f32 {
    let n = a.normal;
    let u = n.any_orthonormal_vector();
    let v = n.cross(u);
    let flat =
        |p: &Poly| -> Vec<(f32, f32)> { p.pts.iter().map(|q| (q.dot(u), q.dot(v))).collect() };
    let mut out = flat(a);
    let clip = flat(b);
    let area2 = |p: &[(f32, f32)]| -> f32 {
        (0..p.len())
            .map(|i| {
                let (x0, y0) = p[i];
                let (x1, y1) = p[(i + 1) % p.len()];
                x0 * y1 - x1 * y0
            })
            .sum::<f32>()
    };
    let sign = area2(&clip).signum();
    for i in 0..clip.len() {
        let (ax, ay) = clip[i];
        let (bx, by) = clip[(i + 1) % clip.len()];
        let side = |p: (f32, f32)| sign * ((bx - ax) * (p.1 - ay) - (by - ay) * (p.0 - ax));
        let input = std::mem::take(&mut out);
        for j in 0..input.len() {
            let p = input[j];
            let q = input[(j + 1) % input.len()];
            let (sp, sq) = (side(p), side(q));
            if sp >= 0.0 {
                out.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                let t = sp / (sp - sq);
                out.push((p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t));
            }
        }
        if out.is_empty() {
            return 0.0;
        }
    }
    area2(&out).abs() * 0.5
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&a[1]);
    let store = if pyreal_dat::holds_modern_dats(dir) {
        RetailDatStore::open_dir(dir)
    } else {
        RetailDatStore::open_classic_dir(dir)
    }
    .expect("dats");
    if a[2] == "--all" {
        let mut blocks: Vec<u32> = store
            .ids_of(DbType::Cell)
            .iter()
            .filter(|i| i.0 & 0xFFFF >= 0x100)
            .map(|i| i.0 >> 16)
            .collect();
        blocks.dedup();
        for b in blocks {
            let polys = block_polys(&store, b);
            let found = pairs(&polys);
            let set_back = found.iter().filter(|l| l.contains("set back")).count();
            if !found.is_empty() {
                println!("{b:#06X} pairs {} set back {set_back}", found.len());
            }
        }
        return;
    }
    let block = u32::from_str_radix(a[2].trim_start_matches("0x"), 16).expect("block");
    let near = a.get(3).map(|p| {
        let v: Vec<f32> = p.split(',').map(|x| x.parse().expect("point")).collect();
        (
            Vec3::new(v[0], v[1], v[2]),
            a[4].parse::<f32>().expect("radius"),
        )
    });
    let mut polys = block_polys(&store, block);
    if let Some((c, r)) = near {
        polys.retain(|p| p.pts.iter().any(|q| q.distance(c) < r));
    }
    println!("{} polygons", polys.len());
    for l in pairs(&polys) {
        println!("{l}");
    }
}

/// Every drawn polygon of `block`'s cells and their objects, in the block's metres.
fn block_polys(store: &RetailDatStore, block: u32) -> Vec<Poly> {
    let mut polys = Vec::new();
    for id in store.ids_of(DbType::Cell) {
        if id.0 >> 16 != block || id.0 & 0xFFFF < 0x100 {
            continue;
        }
        let Some(cell) = read::<EnvCell>(store, DbType::Cell, id) else {
            continue;
        };
        let to_block = mat(&cell.frame);
        if let Some(env) = read::<Environment>(store, DbType::Environment, cell.environment) {
            if let Some(cs) = env.cells.get(usize::from(cell.cell_struct)) {
                // Portals with no picture are openings, never drawn.
                let portals: Vec<i16> = cell
                    .portals
                    .iter()
                    .map(|p| p.polygon_id as i16)
                    .filter(|&i| {
                        cs.polygons
                            .get(i as usize)
                            .is_none_or(|p| p.pos_uv_indices.is_none())
                    })
                    .collect();
                let centre = cs
                    .vertex_array
                    .vertices
                    .iter()
                    .map(|v| {
                        to_block.transform_point3(Vec3::new(
                            v.position.x,
                            v.position.y,
                            v.position.z,
                        ))
                    })
                    .sum::<Vec3>()
                    / cs.vertex_array.vertices.len().max(1) as f32;
                let first = polys.len();
                let surf = |i: u16| cell.surfaces.get(usize::from(i)).map_or(0, |s| s.0);
                add_mesh(
                    &mut polys,
                    &format!("cell {:#010X}", id.0),
                    to_block,
                    &cs.vertex_array.vertices,
                    &cs.polygons,
                    &surf,
                    &portals,
                );
                for q in &mut polys[first..] {
                    let id: u16 = q
                        .source
                        .rsplit(' ')
                        .next()
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(u16::MAX);
                    if cs.portals.contains(&id) {
                        let mid = q.pts.iter().sum::<Vec3>() / q.pts.len() as f32;
                        let out = q.normal.dot(mid - centre) > 0.0;
                        let one = cs
                            .polygons
                            .get(usize::from(id))
                            .is_some_and(|p| p.sides_type == 0);
                        q.source += if out && one {
                            " (portal, faces out: set back)"
                        } else {
                            " (portal)"
                        };
                    }
                }
            }
        }
        for o in &cell.static_objects {
            let at = to_block_obj(&o.frame);
            let mut parts: Vec<(DataId, Mat4)> = Vec::new();
            match divine_type(o.id) {
                Some(DbType::Setup) => {
                    if let Some(s) = read::<Setup>(store, DbType::Setup, o.id) {
                        let frames = s
                            .placement_frames
                            .get(&0x65)
                            .or_else(|| s.placement_frames.values().next());
                        for (i, p) in s.parts.iter().enumerate() {
                            let f = frames
                                .and_then(|f| f.frames.get(i))
                                .map_or(Mat4::IDENTITY, mat);
                            parts.push((*p, at * f));
                        }
                    }
                }
                Some(DbType::GfxObj) => parts.push((o.id, at)),
                _ => {}
            }
            for (part, m) in parts {
                if let Some(g) = read::<GfxObj>(store, DbType::GfxObj, part) {
                    let surf = |i: u16| g.surfaces.get(usize::from(i)).map_or(0, |s| s.0);
                    add_mesh(
                        &mut polys,
                        &format!(
                            "cell {:#010X} object {:#010X} part {:#010X}",
                            id.0, o.id.0, part.0
                        ),
                        m,
                        &g.vertex_array.vertices,
                        &g.polygons,
                        &surf,
                        &[],
                    );
                }
            }
        }
    }
    polys
}

/// The overlapping pairs of `polys` lying in one plane and facing the same way.
fn pairs(polys: &[Poly]) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0..polys.len() {
        for j in i + 1..polys.len() {
            let (p, q) = (&polys[i], &polys[j]);
            if p.normal.dot(q.normal) < 0.999 || (p.d - q.d).abs() > 0.02 {
                continue;
            }
            let area = overlap(p, q);
            if area > 0.01 {
                out.push(format!(
                    "{:.2} m2 apart {:.4} m normal {:.2?}: {} surface {:#010X} | {} surface {:#010X}",
                    area,
                    (p.d - q.d).abs(),
                    p.normal,
                    p.source,
                    p.surface,
                    q.source,
                    q.surface
                ));
            }
        }
    }
    out
}

fn to_block_obj(f: &pyreal_primitives::Frame) -> Mat4 {
    mat(f)
}
