//! Face and hair detail drawn by the object shader over the player's own pictures: skin pores,
//! grain and a faint mottle, hair strands that follow the way each picture paints its hair, and
//! smooth edges with stray hairs where hair meets skin (brows, beards, the hairline).
//!
//! Nothing is enlarged. Each face or hair picture gets one small map at its own size, made once
//! and shared by every palette that draws it ([`face_map`]): the strand direction (its doubled
//! angle) in red and green, where it is hair, softened, in blue, and where it is skin in alpha. A
//! second ([`edge_map`]) holds the direction of its edges, along which the shader smooths away the
//! stair-steps of an enlarged picture. One shared tile holds strands, pores, grain and mottle
//! ([`tile`]). Which pictures are faces and hair comes from the character-creation table.
//!
//! The heads themselves are rounded ([`shape_head`], [`far_head`]): near ones cut finer, their
//! brows, lips, beards and eyes standing out in depth (the vertex stage's `face_push`).

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use pyreal_assets::{CharGen, ClothingTable, Decode, GfxObj, Setup, Surface};
use pyreal_codecs::palette::ExpandedPalette;
use pyreal_codecs::pixel_format::PixelFormatId;
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::num::math::{atan2f, cosf, sinf};
use pyreal_primitives::DataId;

use crate::asset::texture::{PixelLayout, TexturePixels, TextureSource};

/// A body's palette: skin, hair and eye entries.
const SKIN: (u16, u16) = (0, 192);
const HAIR: (u16, u16) = (192, 256);
const EYE: (u16, u16) = (256, 320);

/// The shared tile's side.
const TILE: usize = 256;
/// How near a player's head is drawn shaped in full (cut fine, its beard, brows, lips and eyes
/// moving it), metres; beyond, lightly rounded ([`far_head`]).
pub const NEAR_HEAD: f32 = 6.0;
/// The fewest polygons a player's head model has: the empty part has four.
const HEAD_LEAST_POLYGONS: usize = 32;
/// The largest side a face or edge map is made at.
const MAP_SIDE: usize = 256;
/// How far a beard or moustache stands off the skin, in the edge map's height (0 to 1).
const BEARD_RISE: f32 = 0.25;
/// How far a brow stands off the skin.
const BROW_RISE: f32 = 0.22;

/// The Detailed Faces option as last set: 0 off, 1 on, 2 not set (on).
static SWITCH: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(2);

/// Whether face detail is on: as `PYREAL_FACE_DETAIL` says (`0` off) when it is set, else as the
/// Detailed Faces option was last set ([`set_enabled`]), on until then.
#[must_use]
pub fn enabled() -> bool {
    match std::env::var_os("PYREAL_FACE_DETAIL") {
        Some(v) => v != "0",
        None => SWITCH.load(std::sync::atomic::Ordering::Relaxed) != 0,
    }
}

/// Sets the Detailed Faces option; whether face detail is on or off differently than before.
pub fn set_enabled(on: bool) -> bool {
    let was = enabled();
    SWITCH.store(u8::from(on), std::sync::atomic::Ordering::Relaxed);
    was != enabled()
}

/// Whether graphics object `id` is a player's head model, bare or capped, whether or not face
/// detail is on.
#[must_use]
pub fn is_player_head(store: &RetailDatStore, id: DataId) -> bool {
    head_models(store).contains(&id.0) || sets(store).caps.contains(&id.0) || worn_head(store, id)
}

/// Whether graphics object `id` is a head worn over a player's (a robe's hood, a helm with the
/// face showing): a whole head drawing the eye, nose and mouth pictures. Looked at once a model.
fn worn_head(store: &RetailDatStore, id: DataId) -> bool {
    static SEEN: OnceLock<std::sync::RwLock<HashMap<u32, bool>>> = OnceLock::new();
    let seen = SEEN.get_or_init(Default::default);
    if let Some(known) = seen.read().ok().and_then(|m| m.get(&id.0).copied()) {
        return known;
    }
    let strips = &sets(store).strips;
    let is = read::<GfxObj>(store, DbType::GfxObj, id).is_some_and(|g| {
        g.polygons.len() >= HEAD_LEAST_POLYGONS
            && g.surfaces.iter().any(|s| {
                read::<Surface>(store, DbType::Surface, *s)
                    .and_then(|s| s.orig_texture_id)
                    .is_some_and(|t| strips.contains(&t.0))
            })
    });
    if let Ok(mut m) = seen.write() {
        m.insert(id.0, is);
    }
    is
}

/// Whether graphics object `id` is a player's bare head model (no cap or hood), whether or not
/// face detail is on.
#[must_use]
pub fn is_bare_head(store: &RetailDatStore, id: DataId) -> bool {
    head_models(store).contains(&id.0)
}

/// Whether picture `id` is a player's face or hair picture, whether or not face detail is on.
#[must_use]
pub fn is_face_picture(store: &RetailDatStore, id: DataId) -> bool {
    face_pictures(store).contains_key(&id.0)
}

fn read<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

/// What a face or hair picture is, for which way its hair grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A mouth strip: its hair is a moustache or beard, growing down and out.
    Mouth,
    /// Anything else: scalp hair and brows follow the picture's own flow.
    Head,
}

/// The pictures players' heads and hair are drawn with.
fn face_pictures(store: &RetailDatStore) -> &'static HashMap<u32, Kind> {
    &sets(store).pictures
}

/// The head models players' heads are drawn with: the body's head part and those hair styles swap in.
fn head_models(store: &RetailDatStore) -> &'static HashSet<u32> {
    &sets(store).heads
}

/// What the character-creation table says of heads: their pictures, their models, and the pictures
/// a nose is drawn with, the nose strips' and those they replace.
struct Sets {
    pictures: HashMap<u32, Kind>,
    heads: HashSet<u32>,
    noses: HashSet<u32>,
    /// The eye, nose and mouth strips' pictures and those they replace.
    strips: HashSet<u32>,
    /// The head models creation's caps swap in: their face pictures rounded, the cap left be.
    caps: HashSet<u32>,
}

/// Whether graphics object `id` is a player's head model, bare or capped, and face detail is on.
#[must_use]
pub fn is_head(store: &RetailDatStore, id: DataId) -> bool {
    enabled() && is_player_head(store, id)
}

fn sets(store: &RetailDatStore) -> &'static Sets {
    static SETS: OnceLock<Sets> = OnceLock::new();
    let started = std::time::Instant::now();
    let first = SETS.get().is_none();
    let made = SETS.get_or_init(|| {
        let mut set = HashSet::new();
        let mut mouths = HashSet::new();
        let mut noses = HashSet::new();
        let mut strips = HashSet::new();
        let heads = std::cell::RefCell::new(HashSet::new());
        let mut caps = HashSet::new();
        let Some(cg) = read::<CharGen>(store, DbType::CharGen, DataId(0x0E00_0002)) else {
            return Sets {
                pictures: HashMap::new(),
                heads: HashSet::new(),
                noses: HashSet::new(),
                strips: HashSet::new(),
                caps: HashSet::new(),
            };
        };
        let surfaces = |set: &mut HashSet<u32>, gfx: DataId, head: bool| {
            if head {
                heads.borrow_mut().insert(gfx.0);
            }
            if let Some(g) = read::<GfxObj>(store, DbType::GfxObj, gfx) {
                for s in &g.surfaces {
                    if let Some(t) =
                        read::<Surface>(store, DbType::Surface, *s).and_then(|s| s.orig_texture_id)
                    {
                        set.insert(t.0);
                    }
                }
            }
        };
        for g in cg.heritage_groups.values() {
            for sx in g.sexes.values() {
                let mut parts = HashSet::new();
                let mut ods = vec![&sx.base_objdesc];
                ods.extend(
                    sx.eye_strips
                        .iter()
                        .flat_map(|e| [&e.objdesc, &e.objdesc_bald]),
                );
                ods.extend(sx.nose_strips.iter().map(|n| &n.1));
                ods.extend(sx.mouth_strips.iter().map(|m| &m.1));
                for m in &sx.mouth_strips {
                    mouths.extend(m.1.texture_changes.iter().map(|c| c.2 .0));
                }
                for n in &sx.nose_strips {
                    noses.extend(n.1.texture_changes.iter().flat_map(|c| [c.1 .0, c.2 .0]));
                }
                let strip_ods = sx
                    .eye_strips
                    .iter()
                    .flat_map(|e| [&e.objdesc, &e.objdesc_bald])
                    .chain(sx.nose_strips.iter().map(|n| &n.1))
                    .chain(sx.mouth_strips.iter().map(|m| &m.1));
                // The head: the part the eye, nose and mouth pictures are laid on.
                let mut head_parts = HashSet::new();
                for od in strip_ods {
                    strips.extend(od.texture_changes.iter().flat_map(|c| [c.1 .0, c.2 .0]));
                    head_parts.extend(od.texture_changes.iter().map(|c| c.0));
                }
                ods.extend(sx.hair_styles.iter().map(|h| &h.objdesc));
                for od in ods {
                    for (p, _, new) in &od.texture_changes {
                        set.insert(new.0);
                        parts.insert(*p);
                    }
                }
                // The head models a hair style swaps in, but not a whole body's parts.
                for h in &sx.hair_styles {
                    if h.objdesc.anim_part_changes.len() <= 2 {
                        for (p, gfx) in &h.objdesc.anim_part_changes {
                            surfaces(&mut set, *gfx, head_parts.contains(p));
                        }
                    }
                }
                // The caps: the head models they put on the head part, and the hair pictures they
                // lay on it.
                for gear in &sx.headgear {
                    let Some(table) =
                        read::<ClothingTable>(store, DbType::Clothing, gear.clothing_table)
                    else {
                        continue;
                    };
                    for e in table.clothing_bases.get(&sx.setup).into_iter().flatten() {
                        if !u8::try_from(e.part_num).is_ok_and(|p| head_parts.contains(&p)) {
                            continue;
                        }
                        if e.object_id.0 != 0 {
                            caps.insert(e.object_id.0);
                        }
                    }
                }
                if let Some(setup) = read::<Setup>(store, DbType::Setup, sx.setup) {
                    for p in parts {
                        if let Some(gfx) = setup.parts.get(usize::from(p)) {
                            surfaces(&mut set, *gfx, head_parts.contains(&p));
                        }
                    }
                }
            }
        }
        let pictures: HashMap<u32, Kind> = set
            .into_iter()
            .map(|t| {
                (
                    t,
                    if mouths.contains(&t) {
                        Kind::Mouth
                    } else {
                        Kind::Head
                    },
                )
            })
            .collect();
        // A head or cap model: a whole head, drawing the eye, nose and mouth pictures (not the
        // four-sided empty part a style or a helm puts there).
        let wears = |c: &u32| {
            read::<GfxObj>(store, DbType::GfxObj, DataId(*c)).is_some_and(|g| {
                g.polygons.len() >= HEAD_LEAST_POLYGONS
                    && g.surfaces.iter().any(|s| {
                        read::<Surface>(store, DbType::Surface, *s)
                            .and_then(|s| s.orig_texture_id)
                            .is_some_and(|t| strips.contains(&t.0))
                    })
            })
        };
        let mut heads = heads.into_inner();
        heads.retain(|h| wears(h));
        let caps = caps
            .into_iter()
            .filter(|c| !heads.contains(c) && wears(c))
            .collect();
        Sets {
            pictures,
            heads,
            noses,
            strips,
            caps,
        }
    });
    if first && std::env::var_os("PYREAL_FACE_DETAIL_LOG").is_some() {
        println!(
            "face sets: {} pictures, {} heads, {} caps in {:.1} ms",
            made.pictures.len(),
            made.heads.len(),
            made.caps.len(),
            started.elapsed().as_secs_f64() * 1e3
        );
    }
    made
}

/// A player's head as drawn near: rounded and cut finer, its eye and mouth pictures moving it with
/// their height (`face_push`), a cap's or hood's cloth left as made.
#[must_use]
pub fn shape_head(
    store: &RetailDatStore,
    id: DataId,
    mesh: &crate::asset::mesh::MeshData,
) -> crate::asset::mesh::MeshData {
    let started = std::time::Instant::now();
    let made = make_head(store, id, mesh, true);
    if std::env::var_os("PYREAL_FACE_DETAIL_LOG").is_some() {
        println!(
            "head {:#010X}: {} triangles to {} in {:.1} ms",
            id.0,
            mesh.indices.len() / 3,
            made.indices.len() / 3,
            started.elapsed().as_secs_f64() * 1e3
        );
    }
    made
}

/// A player's head as drawn beyond [`NEAR_HEAD`]: rounded once, nothing moved.
#[must_use]
pub fn far_head(
    store: &RetailDatStore,
    id: DataId,
    mesh: &crate::asset::mesh::MeshData,
) -> crate::asset::mesh::MeshData {
    make_head(store, id, mesh, false)
}

fn make_head(
    store: &RetailDatStore,
    id: DataId,
    mesh: &crate::asset::mesh::MeshData,
    near: bool,
) -> crate::asset::mesh::MeshData {
    let (levels, moves) = if near { (2, moving()) } else { (1, false) };
    let sets = sets(store);
    let picture_of = |sm: &crate::asset::mesh::SubMesh| {
        sm.surface
            .and_then(|f| read::<Surface>(store, DbType::Surface, f))
            .and_then(|f| f.orig_texture_id)
            .map(|t| t.0)
    };
    let strips: Vec<bool> = mesh
        .submeshes
        .iter()
        .map(|sm| picture_of(sm).is_some_and(|t| sets.strips.contains(&t)))
        .collect();
    // What moves with its height: the eye and mouth pictures (brows, lips, a beard, the eyes). A
    // nose is the head's own shape; its painted light would only swell its tip.
    let moving: Vec<bool> = mesh
        .submeshes
        .iter()
        .zip(&strips)
        .map(|(sm, strip)| {
            moves && *strip && !picture_of(sm).is_some_and(|t| sets.noses.contains(&t))
        })
        .collect();
    // A cap's or hood's head: its face rounded, the cloth as it is made.
    if !sets.heads.contains(&id.0) {
        let cloth: Vec<bool> = mesh
            .submeshes
            .iter()
            .map(|sm| !picture_of(sm).is_some_and(|t| sets.pictures.contains_key(&t)))
            .collect();
        return round_mesh(mesh, levels, &strips, &cloth, &moving);
    }
    round_mesh(mesh, levels, &strips, &[], &moving)
}

/// How far a rounded head's new points move toward the curved surface its corners' normals
/// describe (Phong tessellation's shape factor).
const ROUNDING: f32 = 0.6;

/// How far in from a face picture's border its texel-worked detail is whole, metres.
const BORDER_FADE: f32 = 0.006;

/// The cosine of the sharpest turn between two triangles that is still rounded over (90 degrees).
const CREASE: f32 = 0.0;

/// `mesh` rounded: each triangle cut into `4^levels`, its new points lifted onto the curved
/// surface the corners' smoothed normals describe, with the picture coordinates kept as they were,
/// so every face picture lies where it did. Corners at one place share one normal, so neighbouring
/// pictures' triangles meet without cracks. Each new point's first extra word carries, above its
/// low byte, how far short it lies of a few millimetres from its picture's border (0 there), and
/// whether it moves with its picture's height (the `moves` sub-meshes', cut once finer). The
/// `kept` sub-meshes (a cap's cloth) are left as made and out of the shaping: where the rest meets
/// them is an open edge, held still, so nothing opens between them.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn round_mesh(
    mesh: &crate::asset::mesh::MeshData,
    levels: u32,
    smooth_subs: &[bool],
    kept: &[bool],
    moves: &[bool],
) -> crate::asset::mesh::MeshData {
    use crate::asset::mesh::{MeshData, SubMesh, Vertex};
    let key = |p: [f32; 3]| p.map(|v| (v * 20_000.0).round() as i64);
    let unit = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-12);
        v.map(|x| x / l)
    };
    // Each triangle's normal, its length its area's weight; and the triangles round each place.
    let mut faces: HashMap<[i64; 3], Vec<[f32; 3]>> = HashMap::new();
    let face_normal = |t: &[u32; 3]| {
        let [a, b, c] = t.map(|i| mesh.vertices[i as usize].position);
        let (e1, e2) = (
            [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
            [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
        );
        [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ]
    };
    let is_kept = |k: usize| kept.get(k).copied().unwrap_or(false);
    let range = |sm: &crate::asset::mesh::SubMesh| {
        sm.first_index as usize..(sm.first_index + sm.index_count) as usize
    };
    let shaped: Vec<[u32; 3]> = mesh
        .submeshes
        .iter()
        .enumerate()
        .filter(|(k, _)| !is_kept(*k))
        .flat_map(|(_, sm)| mesh.indices[range(sm)].as_chunks::<3>().0.to_vec())
        .collect();
    for t in &shaped {
        let n = face_normal(t);
        for i in t {
            faces
                .entry(key(mesh.vertices[*i as usize].position))
                .or_default()
                .push(n);
        }
    }
    // Places on a face picture (eyes, nose, mouth) are rounded fully, sharp or not: a nose is a
    // curve, not a wedge. Places on the head's open edge (the neck's foot) are not lifted at all.
    // Where a face picture meets the rest of the head (the jaw over the neck) it keeps the rest's rule,
    // so both sides of the line round and shade alike.
    let mut face_places: HashSet<[i64; 3]> = HashSet::new();
    let mut rest_places: HashSet<[i64; 3]> = HashSet::new();
    for (k, sm) in mesh
        .submeshes
        .iter()
        .enumerate()
        .filter(|(k, _)| !is_kept(*k))
    {
        let to = if smooth_subs.get(k).copied().unwrap_or(false) {
            &mut face_places
        } else {
            &mut rest_places
        };
        for i in &mesh.indices[sm.first_index as usize..(sm.first_index + sm.index_count) as usize]
        {
            to.insert(key(mesh.vertices[*i as usize].position));
        }
    }
    face_places.retain(|k| !rest_places.contains(k));
    let mut open_edges: HashMap<([i64; 3], [i64; 3]), u32> = HashMap::new();
    for t in &shaped {
        let q = t.map(|i| key(mesh.vertices[i as usize].position));
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            let e = if q[a] < q[b] {
                (q[a], q[b])
            } else {
                (q[b], q[a])
            };
            *open_edges.entry(e).or_default() += 1;
        }
    }
    let open: HashSet<[i64; 3]> = open_edges
        .iter()
        .filter(|(_, c)| **c == 1)
        .flat_map(|((a, b), _)| [*a, *b])
        .collect();
    // Each place's normal and how smooth it is: the shape is lifted along it, the same for every
    // triangle there (no cracks), and less at a sharp point (a spike of hair, the jaw's line).
    let place = |p: [f32; 3]| {
        let fs = faces.get(&key(p)).map(Vec::as_slice).unwrap_or_default();
        let mut s = [0.0f32; 3];
        for f in fs {
            for k in 0..3 {
                s[k] += f[k];
            }
        }
        let n = unit(s);
        let least = fs
            .iter()
            .map(|f| {
                let u = unit(*f);
                u[0] * n[0] + u[1] * n[1] + u[2] * n[2]
            })
            .fold(1.0f32, f32::min);
        let k = key(p);
        if open.contains(&k) {
            return (n, 0.0);
        }
        if face_places.contains(&k) {
            return (n, 1.0);
        }
        let t = ((least - 0.55) / 0.3).clamp(0.0, 1.0);
        (n, t * t * (3.0 - 2.0 * t))
    };
    // A corner's shading normal: the triangles round its place that turn from its own by less
    // than a crease, so a sharp edge stays sharp in the light.
    let corner = |own: [f32; 3], p: [f32; 3]| {
        let o = unit(own);
        let mut s = [0.0f32; 3];
        for f in faces.get(&key(p)).into_iter().flatten() {
            let u = unit(*f);
            if u[0] * o[0] + u[1] * o[1] + u[2] * o[2] > CREASE {
                for k in 0..3 {
                    s[k] += f[k];
                }
            }
        }
        unit(s)
    };
    type End = ([i64; 3], [i64; 2]);
    let end = |v: &Vertex| (key(v.position), v.uv.map(|u| (u * 4096.0).round() as i64));
    let edge_key = |a: &Vertex, b: &Vertex| {
        let (ka, kb) = (end(a), end(b));
        if ka < kb {
            (ka, kb)
        } else {
            (kb, ka)
        }
    };
    // The eye, nose and mouth pictures move with their height (`face_push`), the whole head cut
    // once finer for a beard's or an eye's shape (all alike, so every shared edge meets).
    let levels = levels + u32::from(moves.contains(&true));
    let n = 1u32 << levels;
    let nf = n as f32;
    let mut out = MeshData::default();
    for (sk, sm) in mesh.submeshes.iter().enumerate() {
        let smooth_sub = smooth_subs.get(sk).copied().unwrap_or(false);
        let move_bit = u32::from(moves.get(sk).copied().unwrap_or(false)) << 16;
        let first = u32::try_from(out.indices.len()).unwrap_or(0);
        let tris =
            &mesh.indices[sm.first_index as usize..(sm.first_index + sm.index_count) as usize];
        if is_kept(sk) {
            for i in tris {
                out.indices
                    .push(u32::try_from(out.vertices.len()).unwrap_or(0));
                out.vertices.push(mesh.vertices[*i as usize]);
            }
            let count = u32::try_from(out.indices.len()).unwrap_or(0) - first;
            out.submeshes.push(SubMesh {
                first_index: first,
                index_count: count,
                ..*sm
            });
            continue;
        }
        // The picture's own border: edges only one of its triangles has, keyed by place and picture
        // coordinates so a seam where the picture is laid twice is a border too. Its height pushes
        // a point out only as far as its distance from there allows, so neighbours meet unmoved.
        let mut edges: HashMap<(End, End), u32> = HashMap::new();
        for t in tris.as_chunks::<3>().0 {
            let q = t.map(|i| mesh.vertices[i as usize]);
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                *edges.entry(edge_key(&q[a], &q[b])).or_default() += 1;
            }
        }
        // Its border's segments, to measure each new point's distance from in metres.
        let segments: Vec<([f32; 3], [f32; 3])> = tris
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|t| {
                let q = t.map(|i| mesh.vertices[i as usize]);
                [(0, 1), (1, 2), (2, 0)]
                    .into_iter()
                    .filter(|(a, b)| edges.get(&edge_key(&q[*a], &q[*b])).copied() == Some(1))
                    .map(|(a, b)| (q[a].position, q[b].position))
                    .collect::<Vec<_>>()
            })
            .collect();
        let from_border = |x: [f32; 3]| {
            segments
                .iter()
                .map(|(a, b)| {
                    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                    let ax = [x[0] - a[0], x[1] - a[1], x[2] - a[2]];
                    let l = (ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2]).max(1e-12);
                    let t = ((ax[0] * ab[0] + ax[1] * ab[1] + ax[2] * ab[2]) / l).clamp(0.0, 1.0);
                    let d = [ax[0] - ab[0] * t, ax[1] - ab[1] * t, ax[2] - ab[2] * t];
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                })
                .fold(f32::MAX, f32::min)
        };
        for t in tris.as_chunks::<3>().0 {
            let v = t.map(|i| mesh.vertices[i as usize]);
            let p = v.map(|x| x.position);
            let own = face_normal(t);
            let nn = v.map(|x| {
                if smooth_sub && face_places.contains(&key(x.position)) {
                    place(x.position).0
                } else {
                    corner(own, x.position)
                }
            });
            let lift = v.map(|x| place(x.position));
            let base = u32::try_from(out.vertices.len()).unwrap_or(0);
            let mut grid = vec![0u32; ((n + 1) * (n + 1)) as usize];
            let mut next = base;
            for i in 0..=n {
                for j in 0..=n - i {
                    let (wb, wc) = (i as f32 / nf, j as f32 / nf);
                    let w = [1.0 - wb - wc, wb, wc];
                    let lin: [f32; 3] =
                        std::array::from_fn(|k| w[0] * p[0][k] + w[1] * p[1][k] + w[2] * p[2][k]);
                    // Phong: the point projected onto each corner's tangent plane, blended.
                    let mut phong = [0.0f32; 3];
                    for c in 0..3 {
                        let (ln, smooth) = lift[c];
                        let d = (lin[0] - p[c][0]) * ln[0]
                            + (lin[1] - p[c][1]) * ln[1]
                            + (lin[2] - p[c][2]) * ln[2];
                        for k in 0..3 {
                            phong[k] += w[c] * (lin[k] - d * ln[k] * smooth);
                        }
                    }
                    let pos = std::array::from_fn(|k| lin[k] + ROUNDING * (phong[k] - lin[k]));
                    let normal = unit(std::array::from_fn(|k| {
                        w[0] * nn[0][k] + w[1] * nn[1][k] + w[2] * nn[2][k]
                    }));
                    let uv = std::array::from_fn(|k| {
                        w[0] * v[0].uv[k] + w[1] * v[1].uv[k] + w[2] * v[2].uv[k]
                    });
                    // Whole a few millimetres in from the picture's border; kept as how far short
                    // of whole, so a mesh never shaped here reads whole.
                    let push = (from_border(lin) / BORDER_FADE).min(1.0);
                    let weight = (push.clamp(0.0, 1.0) * 255.0).round() as u32;
                    // The shade baked inside what the head wears (its low byte), blended from the
                    // corners: the head is shaded as made and shaped after.
                    let shade = (0..3)
                        .map(|c| w[c] * (v[c].extra[1] & 0xFF) as f32)
                        .sum::<f32>()
                        .round() as u32;
                    out.vertices.push(Vertex {
                        position: pos,
                        normal,
                        uv,
                        extra: [
                            (v[0].extra[0] & 0xFF) | ((255 - weight) << 8) | move_bit,
                            (v[0].extra[1] & !0xFF) | shade.min(255),
                        ],
                    });
                    grid[(i * (n + 1) + j) as usize] = next;
                    next += 1;
                }
            }
            let g = |i: u32, j: u32| grid[(i * (n + 1) + j) as usize];
            for i in 0..n {
                for j in 0..n - i {
                    out.indices.extend([g(i, j), g(i + 1, j), g(i, j + 1)]);
                    if i + j + 1 < n {
                        out.indices
                            .extend([g(i + 1, j), g(i + 1, j + 1), g(i, j + 1)]);
                    }
                }
            }
        }
        let count = u32::try_from(out.indices.len()).unwrap_or(0) - first;
        out.submeshes.push(SubMesh {
            first_index: first,
            index_count: count,
            ..*sm
        });
    }
    smooth_shade(&mut out);
    out.fit_bounds();
    out
}

/// How many times a shaped head's shade inside what it wears is averaged with its neighbours':
/// enough to blend it across the dats' coarse triangles it was worked out on.
const SHADE_ROUNDS: usize = 24;

/// The shade baked inside what a head wears (the low byte of each vertex's second extra word),
/// worked out at the dats' coarse corners, made smooth over the finer cut: each place's averaged
/// with its neighbours', over the sub-meshes that carry any (the skin), never the hair or cloth.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn smooth_shade(mesh: &mut crate::asset::mesh::MeshData) {
    let key = |p: [f32; 3]| p.map(|v| (v * 20_000.0).round() as i64);
    let mut skin = vec![false; mesh.vertices.len()];
    for sm in &mesh.submeshes {
        let range = sm.first_index as usize..(sm.first_index + sm.index_count) as usize;
        let tris = &mesh.indices[range];
        if tris
            .iter()
            .any(|&i| mesh.vertices[i as usize].extra[1] & 0xFF != 0)
        {
            for &i in tris {
                skin[i as usize] = true;
            }
        }
    }
    if !skin.contains(&true) {
        return;
    }
    // The places on the skin, their neighbours and their shade.
    let mut place: HashMap<[i64; 3], usize> = HashMap::new();
    let mut of = vec![usize::MAX; mesh.vertices.len()];
    let mut shade: Vec<f32> = Vec::new();
    let mut count: Vec<f32> = Vec::new();
    for (i, v) in mesh.vertices.iter().enumerate() {
        if !skin[i] {
            continue;
        }
        let k = key(v.position);
        let p = *place.entry(k).or_insert_with(|| {
            shade.push(0.0);
            count.push(0.0);
            shade.len() - 1
        });
        of[i] = p;
        shade[p] += (v.extra[1] & 0xFF) as f32;
        count[p] += 1.0;
    }
    for (s, c) in shade.iter_mut().zip(&count) {
        *s /= c.max(1.0);
    }
    let mut nbrs: Vec<Vec<usize>> = vec![Vec::new(); shade.len()];
    for t in mesh.indices.as_chunks::<3>().0 {
        let q = t.map(|i| of[i as usize]);
        if q.contains(&usize::MAX) {
            continue;
        }
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            if q[a] != q[b] && !nbrs[q[a]].contains(&q[b]) {
                nbrs[q[a]].push(q[b]);
                nbrs[q[b]].push(q[a]);
            }
        }
    }
    let mut next = shade.clone();
    for _ in 0..SHADE_ROUNDS {
        for (p, n) in nbrs.iter().enumerate() {
            if n.is_empty() {
                continue;
            }
            let sum: f32 = n.iter().map(|&q| shade[q]).sum();
            next[p] = 0.5 * shade[p] + 0.5 * sum / n.len() as f32;
        }
        std::mem::swap(&mut shade, &mut next);
    }
    for (i, v) in mesh.vertices.iter_mut().enumerate() {
        if of[i] != usize::MAX {
            v.extra[1] = (v.extra[1] & !0xFF) | (shade[of[i]].round().clamp(0.0, 255.0) as u32);
        }
    }
}

/// The median of `p` over the texels `m` holds within `r` of each; a broad painted step (a jaw's
/// rim over its shadow) is followed, a lip line or a nostril smaller than the window is not.
fn median(p: &Plane, m: &Plane, r: isize) -> Plane {
    let (w, h) = (p.w as isize, p.h as isize);
    let mut out = Plane::new(p.w, p.h);
    let mut near = Vec::with_capacity(((2 * r + 1) * (2 * r + 1)) as usize);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            // Only the region's own texels are read back.
            if m.v[i] <= 0.5 {
                out.v[i] = p.v[i];
                continue;
            }
            near.clear();
            for yy in (y - r).max(0)..=(y + r).min(h - 1) {
                for xx in (x - r).max(0)..=(x + r).min(w - 1) {
                    let i = (yy * w + xx) as usize;
                    if m.v[i] > 0.5 {
                        near.push(p.v[i]);
                    }
                }
            }
            out.v[i] = if near.is_empty() {
                p.v[i]
            } else {
                let k = near.len() / 2;
                *near.select_nth_unstable_by(k, f32::total_cmp).1
            };
        }
    }
    out
}

/// Whether the eye, nose and mouth pictures move the head with their height: on unless
/// `PYREAL_FACE_MOVE=0`.
fn moving() -> bool {
    std::env::var("PYREAL_FACE_MOVE").map_or(true, |v| v != "0")
}

/// Whether picture `id` gets face detail: it is on and the picture is a face or hair picture.
#[must_use]
pub fn is_face(store: &RetailDatStore, id: DataId) -> bool {
    enabled() && face_pictures(store).contains_key(&id.0)
}

/// A plane of floats.
#[derive(Clone)]
struct Plane {
    w: usize,
    h: usize,
    v: Vec<f32>,
}

impl Plane {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            v: vec![0.0; w * h],
        }
    }

    fn transposed(&self) -> Self {
        let mut t = Self::new(self.h, self.w);
        for y in 0..self.h {
            for x in 0..self.w {
                t.v[x * self.h + y] = self.v[y * self.w + x];
            }
        }
        t
    }
}

/// The box radius whose three passes spread about as far as a Gaussian of `sigma`.
#[allow(clippy::cast_possible_truncation)]
fn radius(sigma: f32) -> isize {
    (((4.0 * sigma * sigma + 1.0).sqrt() - 1.0) / 2.0)
        .round()
        .max(0.0) as isize
}

/// Three running boxes along each row, clamped or repeating at the ends.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
fn blur_rows(p: &Plane, sigma: f32, wrap: bool) -> Plane {
    let r = radius(sigma);
    if r == 0 {
        return p.clone();
    }
    let (w, wi) = (p.w, p.w as isize);
    let mut cur = p.clone();
    let mut next = p.clone();
    for _ in 0..3 {
        for y in 0..p.h {
            let at = |v: &[f32], i: isize| {
                let i = if wrap {
                    i.rem_euclid(wi)
                } else {
                    i.clamp(0, wi - 1)
                };
                v[y * w + i as usize]
            };
            let mut sum: f32 = (-r..=r).map(|i| at(&cur.v, i)).sum();
            for i in 0..wi {
                next.v[y * w + i as usize] = sum / (2 * r + 1) as f32;
                sum += at(&cur.v, i + r + 1) - at(&cur.v, i - r);
            }
        }
        std::mem::swap(&mut cur, &mut next);
    }
    cur
}

/// About a Gaussian blur of spread `sigma`.
fn blur(p: &Plane, sigma: f32, wrap: bool) -> Plane {
    blur_rows(&blur_rows(p, sigma, wrap).transposed(), sigma, wrap).transposed()
}

/// `mask` averaged along its own edges over `reach` texels each way: the tangent from the
/// structure of its slightly softened self, weighted by how clearly it has one.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
fn along_edges(mask: &Plane, reach: f32) -> Plane {
    let (w, h) = (mask.w, mask.h);
    let soft = blur(mask, 0.7, false);
    let at = |p: &Plane, x: isize, y: isize| {
        p.v[y.clamp(0, h as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize]
    };
    let (mut jxx, mut jxy, mut jyy) = (Plane::new(w, h), Plane::new(w, h), Plane::new(w, h));
    for y in 0..h as isize {
        for x in 0..w as isize {
            let gx = at(&soft, x + 1, y) - at(&soft, x - 1, y);
            let gy = at(&soft, x, y + 1) - at(&soft, x, y - 1);
            let i = y as usize * w + x as usize;
            jxx.v[i] = gx * gx;
            jxy.v[i] = gx * gy;
            jyy.v[i] = gy * gy;
        }
    }
    let (jxx, jxy, jyy) = (
        blur(&jxx, 1.5, false),
        blur(&jxy, 1.5, false),
        blur(&jyy, 1.5, false),
    );
    let bilinear = |fx: f32, fy: f32| {
        let (fx, fy) = (fx.clamp(0.0, (w - 1) as f32), fy.clamp(0.0, (h - 1) as f32));
        let (x0, y0) = (fx as usize, fy as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (ax, ay) = (fx - x0 as f32, fy - y0 as f32);
        let p = |x: usize, y: usize| mask.v[y * w + x];
        (p(x0, y0) * (1.0 - ax) + p(x1, y0) * ax) * (1.0 - ay)
            + (p(x0, y1) * (1.0 - ax) + p(x1, y1) * ax) * ay
    };
    let mut out = mask.clone();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let (p, q, r) = (jxx.v[i], jxy.v[i], jyy.v[i]);
            let sum = p + r;
            if sum < 1e-5 {
                continue;
            }
            let coh = (((p - r) * (p - r) + 4.0 * q * q).sqrt() / sum).powi(2);
            let theta = 0.5 * atan2f(2.0 * q, p - r);
            let (tx, ty) = (-sinf(theta), cosf(theta));
            let (mut acc, mut ws) = (0.0, 0.0);
            let mut s = -reach;
            while s <= reach {
                let wt = 1.0 - s.abs() / (reach + 0.5);
                acc += wt * bilinear(x as f32 + tx * s, y as f32 + ty * s);
                ws += wt;
                s += 0.5;
            }
            out.v[i] += coh * (acc / ws - out.v[i]);
        }
    }
    out
}

/// `rgba` (`w` by `h`, linear data) with its smaller levels, each the mean of four.
fn with_levels(w: usize, h: usize, rgba: Vec<u8>) -> TexturePixels {
    let mut levels = vec![rgba];
    let (mut lw, mut lh) = (w, h);
    while lw > 1 || lh > 1 {
        let (nw, nh) = ((lw / 2).max(1), (lh / 2).max(1));
        let prev = levels.last().map(Vec::as_slice).unwrap_or_default();
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut s = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let (sx, sy) = ((x * 2 + dx).min(lw - 1), (y * 2 + dy).min(lh - 1));
                        s += u32::from(prev[(sy * lw + sx) * 4 + c]);
                    }
                    next[(y * nw + x) * 4 + c] = u8::try_from((s + 2) / 4).unwrap_or(u8::MAX);
                }
            }
        }
        levels.push(next);
        (lw, lh) = (nw, nh);
    }
    TexturePixels {
        width: u32::try_from(w).unwrap_or(1),
        height: u32::try_from(h).unwrap_or(1),
        layout: PixelLayout::Rgba8 { srgb: false },
        levels,
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// The face map of picture `id` ([`self`]), at its own size.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
pub fn face_map(store: &RetailDatStore, id: DataId) -> Option<TexturePixels> {
    let kind = *face_pictures(store).get(&id.0)?;
    let started = std::time::Instant::now();
    let (w, h, idx, lum, _) = indices(store, id)?;
    let mask = |(lo, hi): (u16, u16)| Plane {
        w,
        h,
        v: idx
            .iter()
            .map(|&i| f32::from(u8::from(i >= lo && i < hi)))
            .collect(),
    };
    let (hair, skin) = (mask(HAIR), mask(SKIN));
    let lum = Plane { w, h, v: lum };
    // The strand direction: across the light and dark of the painted strands and the hair's
    // outline, straight down where nothing says.
    let (a, b) = (blur(&lum, 1.2, false), blur(&hair, 2.0, false));
    let src = Plane {
        w,
        h,
        v: a.v.iter().zip(&b.v).map(|(x, y)| x + 0.6 * y).collect(),
    };
    let at = |x: isize, y: isize| {
        src.v[y.clamp(0, h as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize]
    };
    let (mut jxx, mut jxy, mut jyy) = (Plane::new(w, h), Plane::new(w, h), Plane::new(w, h));
    for y in 0..h as isize {
        for x in 0..w as isize {
            let gx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
            let gy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
            let i = y as usize * w + x as usize;
            jxx.v[i] = gx * gx;
            jxy.v[i] = gx * gy;
            jyy.v[i] = gy * gy;
        }
    }
    let (jxx, jxy, jyy) = (
        blur(&jxx, 6.0, false),
        blur(&jxy, 6.0, false),
        blur(&jyy, 6.0, false),
    );
    // The hair's outline smoothed along itself, so stair-steps become lines and thin spikes stay.
    let soft_hair = blur(&along_edges(&hair, 2.5), 0.55, false);
    let soft_skin = blur(&skin, 0.6, false);
    let mut rgba = vec![0u8; w * h * 4];
    for i in 0..w * h {
        let (dx, dy) = if kind == Kind::Mouth {
            // A moustache and beard grow down from the middle of the face and out to its sides.
            (
                0.35 * ((i % w) as f32 - w as f32 / 2.0) / (w as f32 / 2.0),
                1.0,
            )
        } else {
            let (p, q, r) = (jxx.v[i], jxy.v[i], jyy.v[i]);
            let theta = 0.5 * atan2f(2.0 * q, p - r);
            let coh = (((p - r) * (p - r) + 4.0 * q * q).sqrt() / (p + r + 1e-9)).powi(2);
            let wgt = (coh * 4.0).clamp(0.0, 1.0);
            let (x, y) = (-sinf(theta), cosf(theta));
            (
                x * wgt,
                y * wgt + (1.0 - wgt) * if y >= 0.0 { 1.0 } else { -1.0 },
            )
        };
        // The doubled angle: a strand's two ends are one direction, and it averages smoothly.
        let ang = 2.0 * atan2f(dy, dx);
        rgba[i * 4] = byte(0.5 + 0.5 * cosf(ang));
        rgba[i * 4 + 1] = byte(0.5 + 0.5 * sinf(ang));
        rgba[i * 4 + 2] = byte(soft_hair.v[i]);
        rgba[i * 4 + 3] = byte(soft_skin.v[i]);
    }
    if std::env::var_os("PYREAL_FACE_DETAIL_LOG").is_some() {
        println!(
            "face map {:#010X}: {w}x{h} in {:.1} ms",
            id.0,
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
    Some(with_levels(w, h, rgba))
}

/// The edge map of picture `id`, at its own size: along each edge in its light and dark and its
/// skin, hair and eye outlines, the edge's doubled angle in red and green, as long as the edge is
/// clear (none where it is not), which the shader smooths and sharpens the picture along; where
/// the eyes are in blue; and a height for the face's relief in alpha.
#[must_use]
pub fn edge_map(store: &RetailDatStore, id: DataId) -> Option<TexturePixels> {
    let started = std::time::Instant::now();
    let made = make_edge_map(store, id);
    if std::env::var_os("PYREAL_FACE_DETAIL_LOG").is_some() {
        println!(
            "edge map {:#010X} in {:.1} ms",
            id.0,
            started.elapsed().as_secs_f64() * 1e3
        );
    }
    made
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
fn make_edge_map(store: &RetailDatStore, id: DataId) -> Option<TexturePixels> {
    let kind = *face_pictures(store).get(&id.0)?;
    let (w, h, idx, lum, rgb) = indices(store, id)?;
    let region = |(lo, hi): (u16, u16)| {
        idx.iter()
            .map(move |&i| f32::from(u8::from(i >= lo && i < hi)))
    };
    let src = Plane {
        w,
        h,
        v: lum
            .iter()
            .zip(region(HAIR))
            .zip(region(EYE))
            .map(|((l, a), b)| l + 0.5 * a + 0.5 * b)
            .collect(),
    };
    let at = |x: isize, y: isize| {
        src.v[y.clamp(0, h as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize]
    };
    let (mut jxx, mut jxy, mut jyy) = (Plane::new(w, h), Plane::new(w, h), Plane::new(w, h));
    for y in 0..h as isize {
        for x in 0..w as isize {
            let gx = at(x + 1, y) - at(x - 1, y);
            let gy = at(x, y + 1) - at(x, y - 1);
            let i = y as usize * w + x as usize;
            jxx.v[i] = gx * gx;
            jxy.v[i] = gx * gy;
            jyy.v[i] = gy * gy;
        }
    }
    let (jxx, jxy, jyy) = (
        blur(&jxx, 1.5, false),
        blur(&jxy, 1.5, false),
        blur(&jyy, 1.5, false),
    );
    // Where the eyes are, softened; and a height worked out from the painted light and dark (what
    // is lit stands out, as the pictures are lit from above and in front), hair a little proud of
    // the skin and eyes a little sunk, about the middle value. Each region's relief is read from
    // its own texels alone, hair and eyes rise and sink only well inside themselves, and skin lies
    // flat at their edges, so a beard or a brow never lifts the skin beside it.
    let open = opening(w, h, &idx, &rgb);
    let eye = blur(&open, 0.6, false);
    let dome = dome(&open);
    let hard = |r: (u16, u16)| Plane {
        w,
        h,
        v: region(r).collect(),
    };
    let (hair_m, eye_m) = (hard(HAIR), hard(EYE));
    let skin_m = Plane {
        w,
        h,
        v: hair_m
            .v
            .iter()
            .zip(&eye_m.v)
            .map(|(a, b)| 1.0 - a - b)
            .collect(),
    };
    let l = Plane {
        w,
        h,
        v: lum.clone(),
    };
    let local = |m: &Plane| {
        let lm = Plane {
            w,
            h,
            v: l.v.iter().zip(&m.v).map(|(a, b)| a * b).collect(),
        };
        let (f, fw) = (blur(&lm, 1.5, false), blur(m, 1.5, false));
        let base = median(&l, m, 6);
        let bm = Plane {
            w,
            h,
            v: base.v.iter().zip(&m.v).map(|(a, b)| a * b).collect(),
        };
        let (b, bw) = (blur(&bm, 1.5, false), blur(m, 1.5, false));
        Plane {
            w,
            h,
            v: (0..w * h)
                .map(|i| f.v[i] / fw.v[i].max(1e-3) - b.v[i] / bw.v[i].max(1e-3))
                .collect(),
        }
    };
    let rel_skin = local(&skin_m);
    let inside = |m: &Plane| {
        let soft = blur(m, 1.0, false);
        Plane {
            w,
            h,
            v: soft
                .v
                .iter()
                .map(|v| ((v - 0.5) / 0.5).clamp(0.0, 1.0))
                .collect(),
        }
    };
    let hair_in = inside(&hair_m);
    // A beard or a moustache (hair on a mouth picture) swells off the skin, and a brow (hair on an
    // eye or nose strip) a little: nothing at its edge, rising over a few texels inside, so the
    // skin beside it stays flat.
    let brow = kind == Kind::Head && sets(store).strips.contains(&id.0);
    let (reach, rise) = if kind == Kind::Mouth {
        (3.0, BEARD_RISE)
    } else {
        (1.5, BROW_RISE)
    };
    let swell = if kind == Kind::Mouth || brow {
        let deep = blur(&hair_m, reach, false);
        Plane {
            w,
            h,
            v: deep
                .v
                .iter()
                .zip(&hair_m.v)
                .map(|(d, m)| {
                    let t = ((d - 0.5) / 0.45).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t) * m * rise
                })
                .collect(),
        }
    } else {
        Plane::new(w, h)
    };
    let edge_near = blur(
        &Plane {
            w,
            h,
            v: hair_m.v.iter().zip(&eye_m.v).map(|(a, b)| a + b).collect(),
        },
        1.5,
        false,
    );
    let mut rgba = vec![128u8; w * h * 4];
    for i in 0..w * h {
        let skin_flat = 1.0 - (edge_near.v[i] * 2.5).clamp(0.0, 1.0);
        // Hair takes no relief from its painted light and dark (it reads as wax); its strands bend
        // the light instead, in the shader.
        let relief = skin_m.v[i] * rel_skin.v[i] * skin_flat;
        // The eye: round, so the light catches on it, under the lids.
        let height = 0.5 + 1.6 * relief + 0.04 * hair_in.v[i] + swell.v[i] + 0.3 * dome.v[i]
            - 0.1 * eye.v[i];
        rgba[i * 4 + 2] = byte(eye.v[i]);
        rgba[i * 4 + 3] = byte(height);
        let (p, q, r) = (jxx.v[i], jxy.v[i], jyy.v[i]);
        let sum = p + r;
        // Only an edge with some contrast: flat skin's faint shading is left alone.
        let strength = (sum / 0.004).min(1.0);
        let coh = (((p - r) * (p - r) + 4.0 * q * q).sqrt() / (sum + 1e-9)).powi(2) * strength;
        // The gradient's angle; the edge runs a quarter turn from it, which doubled is a half turn.
        let ang = atan2f(2.0 * q, p - r) + std::f32::consts::PI;
        rgba[i * 4] = byte(0.5 + 0.5 * coh * cosf(ang));
        rgba[i * 4 + 1] = byte(0.5 + 0.5 * coh * sinf(ang));
    }
    Some(with_levels(w, h, rgba))
}

/// A dome over each eye opening: 0 at its edge rising to 1 three texels in, as a ball's side.
#[allow(clippy::cast_precision_loss, clippy::needless_range_loop)]
fn dome(open: &Plane) -> Plane {
    let (w, h) = (open.w, open.h);
    let mut dist = vec![u32::MAX; w * h];
    let mut queue = std::collections::VecDeque::new();
    for i in 0..w * h {
        if open.v[i] < 0.5 {
            dist[i] = 0;
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        let (x, y) = (i % w, i / w);
        for (ok, j) in [
            (x > 0, i.wrapping_sub(1)),
            (x + 1 < w, i + 1),
            (y > 0, i.wrapping_sub(w)),
            (y + 1 < h, i + w),
        ] {
            if ok && dist[j] == u32::MAX {
                dist[j] = dist[i] + 1;
                queue.push_back(j);
            }
        }
    }
    let v = dist
        .iter()
        .map(|&d| {
            let t = (d as f32 / 3.0).min(1.0);
            (t * (2.0 - t)).sqrt()
        })
        .collect();
    blur(&Plane { w, h, v }, 0.6, false)
}

/// Each eye's whole opening: its iris and pupil (the eye entries) and the whites round them, which
/// are painted in pale, colourless skin entries, grown out from the iris a few texels at most.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn opening(w: usize, h: usize, idx: &[u16], rgb: &[[f32; 3]]) -> Plane {
    let is_eye = |i: usize| idx[i] >= EYE.0 && idx[i] < EYE.1;
    let white = |i: usize| {
        let c = rgb[i];
        let (hi, lo) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
        hi > 0.5 && (hi - lo) / hi.max(1e-3) < 0.3
    };
    let reach = (w.max(h) / 16).max(4);
    let mut dist = vec![usize::MAX; w * h];
    let mut queue = std::collections::VecDeque::new();
    #[allow(clippy::needless_range_loop)]
    for i in 0..w * h {
        if is_eye(i) {
            dist[i] = 0;
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        let (x, y) = (i % w, i / w);
        if dist[i] >= reach {
            continue;
        }
        let mut step = |j: usize| {
            if dist[j] == usize::MAX && (is_eye(j) || white(j)) {
                dist[j] = dist[i] + 1;
                queue.push_back(j);
            }
        };
        if x > 0 {
            step(i - 1);
        }
        if x + 1 < w {
            step(i + 1);
        }
        if y > 0 {
            step(i - w);
        }
        if y + 1 < h {
            step(i + w);
        }
    }
    Plane {
        w,
        h,
        v: dist
            .iter()
            .map(|d| f32::from(u8::from(*d != usize::MAX)))
            .collect(),
    }
}

/// Picture `id`'s size, palette indices and light and dark in its own palette, if it is
/// palette-indexed.
#[allow(clippy::type_complexity)]
fn indices(
    store: &RetailDatStore,
    id: DataId,
) -> Option<(usize, usize, Vec<u16>, Vec<f32>, Vec<[f32; 3]>)> {
    let src = TextureSource::new(store);
    let (_, rs, bytes) = src.resolve(id).ok()?;
    if PixelFormatId::from_raw(rs.format) != PixelFormatId::Index16 {
        return None;
    }
    let payload = rs.payload(&bytes)?;
    let (w, h) = (rs.width as usize, rs.height as usize);
    let idx: Vec<u16> = payload
        .get(..w * h * 2)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b))
        .collect();
    let pal = rs
        .default_palette_id
        .and_then(|p| src.palette_colours(p))
        .and_then(|c| ExpandedPalette::from_dat(&c))?;
    let rgb: Vec<[f32; 3]> = idx
        .iter()
        .map(|&i| {
            let [b, g, r, _] = pal.expand(i, false).to_le_bytes();
            [r, g, b].map(|c| f32::from(c) / 255.0)
        })
        .collect();
    let (mut w, mut h, mut idx, mut rgb) = (w, h, idx, rgb);
    // A large picture (a 512 hair) is read at half its size, and again, to at most MAP_SIDE: the
    // maps are looked up by picture coordinates, so they need not match it, and cost a quarter.
    while w.max(h) > MAP_SIDE && w % 2 == 0 && h % 2 == 0 {
        let (hw, hh) = (w / 2, h / 2);
        let at = |x: usize, y: usize| y * w + x;
        let mut i2 = Vec::with_capacity(hw * hh);
        let mut c2 = Vec::with_capacity(hw * hh);
        for y in 0..hh {
            for x in 0..hw {
                let q = [
                    at(2 * x, 2 * y),
                    at(2 * x + 1, 2 * y),
                    at(2 * x, 2 * y + 1),
                    at(2 * x + 1, 2 * y + 1),
                ];
                i2.push(idx[q[0]]);
                c2.push(std::array::from_fn(|k| {
                    q.iter().map(|&j| rgb[j][k]).sum::<f32>() / 4.0
                }));
            }
        }
        (w, h, idx, rgb) = (hw, hh, i2, c2);
    }
    let lum = rgb
        .iter()
        .map(|c| 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2])
        .collect();
    Some((w, h, idx, lum, rgb))
}

/// The shared tile: strands running down it in red, pores in green, grain in blue and a faint
/// mottle in alpha, each repeating seamlessly and averaging to the middle value (no change).
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn tile() -> TexturePixels {
    let mut seed = 0x5EED_FACE_u64;
    let mut rnd = move || {
        seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    };
    let n = TILE;
    let mut noise = || Plane {
        w: n,
        h: n,
        v: (0..n * n).map(|_| rnd() * 2.0 - 1.0).collect(),
    };
    let unit = |p: Plane| {
        let m = p.v.iter().sum::<f32>() / p.v.len() as f32;
        let sd =
            (p.v.iter().map(|v| (v - m) * (v - m)).sum::<f32>() / p.v.len() as f32).sqrt() + 1e-6;
        p.v.into_iter().map(|v| (v - m) / sd).collect::<Vec<f32>>()
    };
    // Strands: noise thin across the tile and drawn out down it.
    let thin = blur(&noise(), 0.6, true);
    let strands = unit(blur_rows(&thin.transposed(), 14.0, true).transposed());
    let grain = unit(blur(&noise(), 0.6, true));
    let mottle = unit(blur(&noise(), 18.0, true));
    let spots = noise();
    let mut pores = Plane::new(n, n);
    for (i, s) in spots.v.iter().enumerate() {
        if *s > 0.95 {
            pores.v[i] = 0.4 + 0.6 * (s - 0.95) / 0.05;
        }
    }
    let pores = blur(&pores, 0.9, true);
    let top = pores.v.iter().copied().fold(0.0, f32::max) + 1e-6;
    let mean = pores.v.iter().sum::<f32>() / (n * n) as f32 / top;
    let mut rgba = vec![0u8; n * n * 4];
    for i in 0..n * n {
        rgba[i * 4] = byte(0.5 + strands[i] / 5.0);
        // Pores sit about the middle value too, so a far, averaged tile changes nothing.
        rgba[i * 4 + 1] = byte(0.5 + (pores.v[i] / top - mean) * 0.5);
        rgba[i * 4 + 2] = byte(0.5 + grain[i] / 5.0);
        rgba[i * 4 + 3] = byte(0.5 + mottle[i] / 5.0);
    }
    with_levels(n, n, rgba)
}
