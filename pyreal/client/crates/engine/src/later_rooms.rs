//! End of Retail's room pictures beside an older world. A room End of Retail's files hold with the
//! same piece draws, slot by slot, End of Retail's picture where it is the older one painted again
//! at a higher resolution, tinted toward the older picture under Classic Ground Coloring
//! (`docs/ERAS.md`).
//!
//! The pairs are [`PAIRS`], from the older-era survey (`notes/review/older-era-swaps.md`): dungeon
//! and room pictures whose End of Retail counterpart in the same cell slot is the same layout. Each
//! is checked again as it is loaded ([`measure`]), which also gives the tint for the pairs drawn
//! tinted.

use std::collections::HashMap;
use std::sync::Arc;

use pyreal_assets::{Decode, EnvCell, Environment, Polygon, Surface, VertexArray};
use pyreal_codecs::surface::surface_type as st;
use pyreal_dat::{ContainerEra, DbType, RetailDatStore};
use pyreal_primitives::DataId;
use pyreal_world_data::env_cells::DecodedCell;

use crate::asset::texture::TextureSource;

/// (older picture, End of Retail picture, tinted): the room pictures End of Retail painted again,
/// sorted. A tinted pair is drawn in the older colours under Classic Ground Coloring; the others are
/// End of Retail's picture as it is (the developer's review of the close pairs, 2026-11-19).
pub const PAIRS: &[(u32, u32, bool)] = &[
    (0x05000ABB, 0x0500271E, false),
    (0x05000ABC, 0x0500271C, false),
    (0x05000B7B, 0x0500264B, false),
    (0x05000B82, 0x0500265A, false),
    (0x05000B84, 0x050026C5, false),
    (0x05000B95, 0x050027A1, false),
    (0x05000BB4, 0x0500261B, false),
    (0x05000BCE, 0x05002678, false),
    (0x05000BFB, 0x05002A14, false),
    (0x05000C3E, 0x050026E5, false),
    (0x05000C47, 0x0500280A, false),
    (0x05000CD1, 0x0500264D, false),
    (0x05000CD6, 0x05002652, false),
    (0x05000CD9, 0x05002675, false),
    (0x05000CDB, 0x05002626, true),
    (0x05000CE2, 0x0500279E, true),
    (0x05000CEC, 0x05002667, false),
    (0x05000CED, 0x05002847, false),
    (0x05000D01, 0x050027B1, true),
    (0x05000D02, 0x050027A7, true),
    (0x05000D05, 0x050027B0, true),
    (0x05000D07, 0x050027A8, false),
    (0x05000D08, 0x050027AF, false),
    (0x05000D0B, 0x050027A5, true),
    (0x05000D1E, 0x05002650, false),
    (0x05000D21, 0x05002631, false),
    (0x05000D2A, 0x05002638, false),
    (0x05000D2B, 0x05002620, false),
    (0x05000D37, 0x0500274D, false),
    (0x05000D41, 0x050027BF, false),
    (0x05000D43, 0x050027C7, false),
    (0x05000D44, 0x0500262E, false),
    (0x05000D45, 0x0500266E, false),
    (0x05000D46, 0x0500266D, false),
    (0x05000D48, 0x0500266F, true),
    (0x05000D4B, 0x05002676, false),
    (0x05000D4E, 0x050027E7, true),
    (0x05000D6D, 0x050026B5, false),
    (0x05000D77, 0x050026F3, true),
    (0x05000D7A, 0x050026F4, false),
    (0x05000DA1, 0x0500262F, false),
    (0x05000DA2, 0x05002698, false),
    (0x05000DB2, 0x050026BB, false),
    (0x05000DB3, 0x050026BC, false),
    (0x05000DB8, 0x050026BD, false),
    (0x05000DFE, 0x050026B8, false),
    (0x05000E05, 0x05002808, false),
    (0x05000E08, 0x0500277A, false),
    (0x05000E0B, 0x050026BA, false),
    (0x05000E7B, 0x05002858, false),
    (0x05000E88, 0x05002A0A, true),
    (0x05000E8B, 0x05002A09, false),
    (0x05000E8C, 0x05002A0C, false),
    (0x05000EA3, 0x05000EA3, false),
    (0x05000ED3, 0x0500277B, false),
    (0x05000ED4, 0x0500277F, false),
    (0x05000ED8, 0x0500277E, true),
    (0x05000EDB, 0x05002776, false),
    (0x05000F46, 0x0500296B, false),
    (0x05000FAB, 0x050027C8, false),
    (0x05000FAC, 0x050027C9, true),
    (0x05000FAE, 0x0500279B, false),
    (0x05000FAF, 0x05002799, false),
    (0x05000FB7, 0x05002797, false),
    (0x05000FB8, 0x050027E5, true),
    (0x05000FBC, 0x05002668, false),
    (0x05000FC0, 0x05002669, true),
    (0x05000FCD, 0x050027DD, true),
    (0x05000FD2, 0x05002A01, false),
    (0x05001005, 0x05002874, false),
    (0x05001006, 0x05002875, false),
    (0x05001008, 0x05002967, false),
    (0x0500100C, 0x05002968, false),
    (0x05001011, 0x0500295F, true),
    (0x05001013, 0x0500295D, true),
    (0x0500102E, 0x0500287B, false),
    (0x050010CF, 0x0500296A, false),
    (0x050010DC, 0x050027C2, true),
    (0x050011EF, 0x0500286B, true),
    (0x050011F4, 0x05002624, false),
    (0x05001218, 0x05002621, false),
    (0x05001219, 0x05002622, false),
    (0x05001295, 0x050027C5, false),
    (0x05001297, 0x050027C4, false),
    (0x0500129A, 0x050028BF, false),
    (0x050012AC, 0x050026A4, false),
    (0x050012AD, 0x050026A1, false),
    (0x050012D9, 0x050027A9, false),
    (0x0500130C, 0x050028A3, true),
    (0x05001369, 0x05002692, true),
    (0x0500136A, 0x050027D8, true),
    (0x05001376, 0x05002841, false),
    (0x05001378, 0x05002849, false),
    (0x05001380, 0x050027AB, false),
    (0x05001396, 0x050027BD, false),
    (0x05001397, 0x05002658, false),
    (0x0500139E, 0x05002780, false),
    (0x0500139F, 0x050026C9, false),
    (0x0500142C, 0x050026B3, false),
    (0x05001740, 0x0500265C, false),
    (0x05001742, 0x0500265E, false),
    (0x05001744, 0x0500265D, true),
    (0x05001745, 0x05002660, false),
    (0x05001746, 0x05002657, false),
    (0x05001747, 0x05002656, false),
    (0x05001748, 0x0500265B, false),
    (0x0500174A, 0x05002654, false),
    (0x0500175C, 0x050027B6, false),
    (0x05001776, 0x050026FD, false),
    (0x050017BF, 0x05002928, true),
    (0x0500181B, 0x0500269E, false),
    (0x05001981, 0x05002764, false),
    (0x05001983, 0x05002740, true),
    (0x05001984, 0x05002839, true),
    (0x05001985, 0x05002745, false),
    (0x05001987, 0x05002743, false),
    (0x05001988, 0x0500283A, false),
    (0x05001989, 0x05002742, true),
    (0x0500198A, 0x0500273D, false),
    (0x0500198C, 0x05002741, false),
    (0x0500198D, 0x0500273C, false),
    (0x0500198E, 0x05002746, false),
    (0x05001991, 0x0500283D, true),
    (0x05001992, 0x05002747, false),
    (0x05001998, 0x0500283E, false),
    (0x050019A5, 0x0500274C, false),
    (0x050019D8, 0x0500266A, false),
    (0x050019D9, 0x0500266B, false),
    (0x050019DB, 0x0500283B, false),
    (0x050019DC, 0x050026A8, false),
    (0x050019DD, 0x050026A7, false),
    (0x050019DF, 0x050026AA, false),
    (0x050019E1, 0x050026AC, false),
    (0x05001A5B, 0x050027CA, false),
    (0x05001A5E, 0x050027D9, false),
    (0x05001A98, 0x05002A81, true),
    (0x05001AA5, 0x05002A7F, true),
    (0x05001AAE, 0x05002A82, false),
    (0x05001B17, 0x05002933, false),
    (0x05001B18, 0x05002A2A, false),
    (0x05001C2F, 0x05002A2C, false),
    (0x05001CBD, 0x0500273A, true),
    (0x05001CBE, 0x0500273B, true),
    (0x05001CBF, 0x05002737, true),
    (0x05001CC5, 0x0500281B, false),
    (0x05001CC6, 0x0500281C, false),
    (0x05001CC7, 0x05002824, false),
    (0x05001CC8, 0x0500282D, false),
    (0x05001CD2, 0x05002826, false),
    (0x05001CD3, 0x05002829, false),
    (0x05001CD5, 0x05002642, true),
    (0x05001CD7, 0x05002648, false),
    (0x05001CDA, 0x05002647, false),
    (0x05001CE2, 0x05002838, true),
    (0x05001CE3, 0x05002837, true),
    (0x05001CF7, 0x050028A5, true),
    (0x05001D2D, 0x05002752, false),
    (0x05001D2F, 0x05002751, false),
    (0x05001D30, 0x0500274F, false),
    (0x05001D31, 0x05002750, false),
    (0x05001D41, 0x050028CE, true),
    (0x05002131, 0x050029D8, false),
    (0x05002132, 0x050029D9, false),
    (0x05002331, 0x05002709, false),
    (0x05002378, 0x05002712, true),
    (0x05002379, 0x05002711, false),
    (0x0500237C, 0x05002717, false),
    (0x05002382, 0x050026E7, false),
    (0x05002383, 0x050026E8, false),
    (0x05002384, 0x050026EB, true),
    (0x050023F9, 0x0500278D, false),
    (0x050023FA, 0x0500278C, false),
    (0x050023FB, 0x0500278E, true),
    (0x050023FC, 0x050027D1, false),
];

/// The least luminance correlation, at the older picture's size, of a pair drawn swapped.
const LEAST_LIKENESS: f32 = 0.6;
/// The most error, in sRGB levels, the tint may leave.
const MOST_TINT_ERROR: f32 = 35.0;
/// The least overlap of the two pictures' cut-outs.
const LEAST_CUT_OVERLAP: f32 = 0.8;
/// A fit's gain and offset are kept in this many parts of one.
const FIT_SCALE: f32 = 1024.0;

/// A per-channel gain and offset that carries End of Retail's colours to the older picture's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    /// Red, green and blue.
    pub gain: [f32; 3],
    /// Red, green and blue, in parts of full scale.
    pub offset: [f32; 3],
}

/// A [`Fit`] as a texture key holds it.
pub type FitKey = [[i16; 2]; 3];

impl Fit {
    #[must_use]
    pub fn key(&self) -> FitKey {
        // LINT-OK: clamped to the i16 range first.
        #[allow(clippy::cast_possible_truncation)]
        let q = |v: f32| (v * FIT_SCALE).round().clamp(-32768.0, 32767.0) as i16;
        [0, 1, 2].map(|c| [q(self.gain[c]), q(self.offset[c])])
    }

    #[must_use]
    pub fn from_key(k: FitKey) -> Self {
        Self {
            gain: k.map(|c| f32::from(c[0]) / FIT_SCALE),
            offset: k.map(|c| f32::from(c[1]) / FIT_SCALE),
        }
    }

    /// Tints rows of blue, green, red and alpha in place.
    // LINT-OK: each level is clamped to 0..=255 first.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn apply(&self, bgra: &mut [u8]) {
        for p in bgra.as_chunks_mut::<4>().0 {
            for (i, c) in [2usize, 1, 0].into_iter().enumerate() {
                let v = f32::from(p[c]) / 255.0 * self.gain[i] + self.offset[i];
                p[c] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
}

/// A picture's size and texels, blue, green, red and alpha.
#[derive(Debug, Clone, Copy)]
pub struct Pixels<'a> {
    pub width: u32,
    pub height: u32,
    pub bgra: &'a [u8],
}

/// `later` box-averaged to `w` by `h`, as red, green, blue and alpha in parts of one.
fn shrink(later: Pixels<'_>, w: u32, h: u32) -> Vec<[f32; 4]> {
    let (sx, sy) = (later.width / w, later.height / h);
    let n = (sx * sy) as f32;
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0f32; 4];
            for yy in y * sy..(y + 1) * sy {
                for xx in x * sx..(x + 1) * sx {
                    let i = ((yy * later.width + xx) * 4) as usize;
                    let p = &later.bgra[i..i + 4];
                    for (a, c) in acc.iter_mut().zip([p[2], p[1], p[0], p[3]]) {
                        *a += f32::from(c) / 255.0;
                    }
                }
            }
            out.push(acc.map(|a| a / n));
        }
    }
    out
}

fn luminance(p: [f32; 4]) -> f32 {
    0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]
}

fn correlation(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().max(1) as f32;
    let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
    let (mut ab, mut aa, mut bb) = (0f32, 0f32, 0f32);
    for (x, y) in a.iter().zip(b) {
        ab += (x - ma) * (y - mb);
        aa += (x - ma) * (x - ma);
        bb += (y - mb) * (y - mb);
    }
    if aa < 1e-6 || bb < 1e-6 {
        return 0.0;
    }
    ab / (aa.sqrt() * bb.sqrt())
}

/// The tint that carries `later` to `older`, when `later` is `older` at a whole multiple of its
/// size in each direction, more texels, and alike: its luminance correlating, its cut-out
/// overlapping, and the tint leaving little error. `None` otherwise.
#[must_use]
pub fn measure(older: Pixels<'_>, later: Pixels<'_>) -> Option<Fit> {
    let (w, h) = (older.width, older.height);
    if w == 0
        || h == 0
        || later.width * later.height <= w * h
        || !later.width.is_multiple_of(w)
        || !later.height.is_multiple_of(h)
        || later.width / w != later.height / h
        || older.bgra.len() < (w * h * 4) as usize
        || later.bgra.len() < (later.width * later.height * 4) as usize
    {
        return None;
    }
    let b = shrink(later, w, h);
    let a: Vec<[f32; 4]> = older.bgra[..(w * h * 4) as usize]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| [p[2], p[1], p[0], p[3]].map(|c| f32::from(c) / 255.0))
        .collect();
    let cut = a.iter().chain(&b).any(|p| p[3] < 0.5);
    if cut {
        let (mut both, mut either) = (0u32, 0u32);
        for (p, q) in a.iter().zip(&b) {
            let (x, y) = (p[3] >= 0.5, q[3] >= 0.5);
            both += u32::from(x && y);
            either += u32::from(x || y);
        }
        if either > 0 && (both as f32) < LEAST_CUT_OVERLAP * either as f32 {
            return None;
        }
    }
    let shown: Vec<usize> = (0..a.len())
        .filter(|&i| !cut || (a[i][3] >= 0.5 && b[i][3] >= 0.5))
        .collect();
    let la: Vec<f32> = shown.iter().map(|&i| luminance(a[i])).collect();
    let lb: Vec<f32> = shown.iter().map(|&i| luminance(b[i])).collect();
    if correlation(&la, &lb) < LEAST_LIKENESS {
        return None;
    }
    let mut fit = Fit {
        gain: [1.0; 3],
        offset: [0.0; 3],
    };
    let (mut err, mut n) = (0f32, 0f32);
    for c in 0..3 {
        let k = shown.len().max(1) as f32;
        let mx = shown.iter().map(|&i| b[i][c]).sum::<f32>() / k;
        let my = shown.iter().map(|&i| a[i][c]).sum::<f32>() / k;
        let (mut sxy, mut sxx) = (0f32, 0f32);
        for &i in &shown {
            sxy += (b[i][c] - mx) * (a[i][c] - my);
            sxx += (b[i][c] - mx) * (b[i][c] - mx);
        }
        let g = if sxx > 1e-6 {
            (sxy / sxx).clamp(0.0, 4.0)
        } else {
            1.0
        };
        fit.gain[c] = g;
        fit.offset[c] = my - g * mx;
        for &i in &shown {
            let d = (g * b[i][c] + fit.offset[c]).clamp(0.0, 1.0) - a[i][c];
            err += d * d;
            n += 1.0;
        }
    }
    ((err / n.max(1.0)).sqrt() * 255.0 <= MOST_TINT_ERROR).then_some(fit)
}

/// One room slot drawn with End of Retail's picture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaterSlot {
    /// End of Retail's surface in the slot, whose pack material the picture takes.
    pub surface: DataId,
    /// End of Retail's picture.
    pub picture: DataId,
    pub fit: Fit,
    /// Whether the pair is drawn in the older colours under Classic Ground Coloring.
    pub tinted: bool,
}

/// End of Retail's rooms and pictures beside an older world.
#[derive(Debug)]
pub struct LaterRooms {
    store: RetailDatStore,
    // ORDER-OK: decode memos, only ever looked up.
    environments: HashMap<u32, Option<Arc<Environment>>>,
    layouts: HashMap<(u32, u16, u16), bool>,
    fits: HashMap<(u32, u32, bool), Option<Fit>>,
}

fn decode<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

/// Whether `(older, later)` is one of [`PAIRS`], and then whether it is drawn tinted.
#[must_use]
pub fn paired(older: DataId, later: DataId) -> Option<bool> {
    PAIRS
        .binary_search_by(|p| (p.0, p.1).cmp(&(older.0, later.0)))
        .ok()
        .map(|i| PAIRS[i].2)
}

/// The texture coordinates slot `slot`'s polygons draw with, in order.
fn slot_uvs(vertices: &VertexArray, polygons: &[Polygon], slot: u16) -> Vec<(f32, f32)> {
    let mut uvs = Vec::new();
    for p in polygons.iter().filter(|p| p.pos_surface == slot) {
        for (k, v) in p.vertex_ids.iter().enumerate() {
            let at = p
                .pos_uv_indices
                .as_ref()
                .and_then(|u| u.get(k))
                .copied()
                .unwrap_or(0) as usize;
            let uv = vertices
                .vertices
                .get(usize::from(*v))
                .and_then(|x| x.uvs.get(at))
                .copied()
                .unwrap_or_default();
            uvs.push(uv);
        }
    }
    uvs
}

/// A surface's picture, when it draws one, and whether it cuts out.
fn picture_of(store: &RetailDatStore, surface: DataId) -> Option<(DataId, bool)> {
    let s: Surface = decode(store, DbType::Surface, surface)?;
    let ty = s.surface_type;
    if ty & (st::BASE1_IMAGE | st::BASE1_CLIPMAP) == 0 {
        return None;
    }
    Some((s.orig_texture_id?, ty & st::BASE1_CLIPMAP != 0))
}

impl LaterRooms {
    /// `None` on a later world, or on an older one without End of Retail's portal and cell files
    /// beside it.
    #[must_use]
    pub fn of(world: &RetailDatStore) -> Option<Self> {
        if world.era() != ContainerEra::Classic {
            return None;
        }
        Some(Self {
            store: world.interior_files(ContainerEra::Modern)?,
            environments: HashMap::new(),
            layouts: HashMap::new(),
            fits: HashMap::new(),
        })
    }

    /// End of Retail's files: its portal, cell and high-resolution files.
    #[must_use]
    pub fn store(&self) -> &RetailDatStore {
        &self.store
    }

    /// Room `d` of the older world `world`, slot by slot: End of Retail's picture where End of
    /// Retail's room of that id is the same piece, the slot's pair is one of [`PAIRS`], its
    /// texture coordinates are the world's, and [`measure`] finds the pictures alike.
    pub fn slots(&mut self, world: &RetailDatStore, d: &DecodedCell) -> Vec<Option<LaterSlot>> {
        let none = vec![None; d.cell.surfaces.len()];
        let Some(later) = decode::<EnvCell>(&self.store, DbType::Cell, d.cell.id) else {
            return none;
        };
        if later.environment != d.cell.environment || later.cell_struct != d.cell.cell_struct {
            return none;
        }
        d.cell
            .surfaces
            .iter()
            .enumerate()
            .map(|(i, &older)| {
                let slot = u16::try_from(i).ok()?;
                let surface = *later.surfaces.get(i)?;
                let (old_pic, clip) = picture_of(world, older)?;
                let (picture, _) = picture_of(&self.store, surface)?;
                let tinted = paired(old_pic, picture)?;
                if !self.same_layout(d, slot) {
                    return None;
                }
                let fit = self.fit(world, old_pic, picture, clip)?;
                Some(LaterSlot {
                    surface,
                    picture,
                    fit,
                    tinted,
                })
            })
            .collect()
    }

    /// Whether End of Retail's piece maps slot `slot` as the world's does.
    fn same_layout(&mut self, d: &DecodedCell, slot: u16) -> bool {
        let (env, cs) = (d.cell.environment, d.cell.cell_struct);
        if let Some(&same) = self.layouts.get(&(env.0, cs, slot)) {
            return same;
        }
        let store = &self.store;
        let later = self
            .environments
            .entry(env.0)
            .or_insert_with(|| decode::<Environment>(store, DbType::Environment, env).map(Arc::new))
            .clone();
        let same = later
            .as_ref()
            .and_then(|e| e.cells.get(usize::from(cs)))
            .is_some_and(|c| {
                let a = slot_uvs(&d.structure.vertex_array, &d.structure.polygons, slot);
                let b = slot_uvs(&c.vertex_array, &c.polygons, slot);
                !a.is_empty()
                    && a.len() == b.len()
                    && a.iter()
                        .zip(&b)
                        .all(|(p, q)| (p.0 - q.0).abs() < 0.01 && (p.1 - q.1).abs() < 0.01)
            });
        self.layouts.insert((env.0, cs, slot), same);
        same
    }

    /// The tint for the pair, measured once ([`measure`]).
    fn fit(
        &mut self,
        world: &RetailDatStore,
        older: DataId,
        later: DataId,
        clip: bool,
    ) -> Option<Fit> {
        if let Some(f) = self.fits.get(&(older.0, later.0, clip)) {
            return *f;
        }
        let made = (|| {
            let old_src = TextureSource::new(world);
            let old = if old_src.is_palettised(older) {
                let px = old_src.decode(older, clip, None).ok()?;
                (px.width, px.height, px.levels.into_iter().next()?)
            } else {
                old_src.bgra(older).ok()?
            };
            let new_src = TextureSource::new(&self.store);
            // A palette-indexed later picture would need the older surface's palette.
            if new_src.is_palettised(later) {
                return None;
            }
            let new = new_src.bgra(later).ok()?;
            measure(
                Pixels {
                    width: old.0,
                    height: old.1,
                    bgra: &old.2,
                },
                Pixels {
                    width: new.0,
                    height: new.1,
                    bgra: &new.2,
                },
            )
        })();
        self.fits.insert((older.0, later.0, clip), made);
        made
    }
}

#[cfg(test)]
// LINT-OK: test pictures' levels are within 0..=255.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
mod tests {
    use super::*;
    use pyreal_primitives::num::math::{cosf, sinf};

    /// A 16 by 16 picture of soft stripes and a blob, in its own colours.
    fn picture(side: u32, colour: impl Fn(f32) -> [u8; 3]) -> Vec<u8> {
        let mut v = Vec::new();
        for y in 0..side {
            for x in 0..side {
                let (u, w) = (x as f32 / side as f32, y as f32 / side as f32);
                let l = 0.5 + 0.3 * sinf(u * 9.0) * cosf(w * 5.0);
                let [r, g, b] = colour(l);
                v.extend_from_slice(&[b, g, r, 255]);
            }
        }
        v
    }

    fn pic(width: u32, height: u32, bgra: &[u8]) -> Pixels<'_> {
        Pixels {
            width,
            height,
            bgra,
        }
    }

    #[test]
    fn a_sharper_copy_in_other_colours_is_tinted_back() {
        let older = picture(16, |l| {
            [(l * 200.0) as u8, (l * 150.0) as u8, (l * 90.0) as u8]
        });
        let later = picture(32, |l| {
            [(l * 160.0) as u8, (l * 170.0) as u8, (l * 120.0) as u8]
        });
        let fit = measure(
            Pixels {
                width: 16,
                height: 16,
                bgra: &older,
            },
            Pixels {
                width: 32,
                height: 32,
                bgra: &later,
            },
        )
        .expect("alike");
        let mut tinted = later.clone();
        fit.apply(&mut tinted);
        // The tinted texel carries the older colours.
        let red = |px: &[u8]| f32::from(px[2]);
        assert!((red(&tinted[..4]) - red(&older[..4])).abs() < 12.0);
        assert_eq!(Fit::from_key(fit.key()).key(), fit.key());
    }

    #[test]
    fn other_content_no_larger_or_an_odd_size_is_refused() {
        let older = picture(16, |l| [(l * 200.0) as u8; 3]);
        let noise: Vec<u8> = (0..32 * 32)
            .flat_map(|i: u32| {
                let v = (i.wrapping_mul(2_654_435_761) >> 24) as u8;
                [v, v, v, 255]
            })
            .collect();
        let wide = picture(32, |l| [(l * 200.0) as u8; 3]);
        for (later, w, h) in [
            (&noise[..], 32, 32),
            (&older[..], 16, 16),
            (&wide[..32 * 16 * 4], 32, 16),
        ] {
            assert!(measure(pic(16, 16, &older), pic(w, h, later)).is_none());
        }
    }

    /// The developer's review of the close pairs (2026-11-19): close pairs draw End of Retail's
    /// picture as it is, the surveyed tint pairs, 0x05001CBF and 0x05000D77 are tinted, and the
    /// three once left out are back untinted.
    #[test]
    fn the_reviewed_pairs_are_tinted_as_the_developer_chose() {
        let p = |o: u32, l: u32| paired(DataId(o), DataId(l));
        assert_eq!(p(0x0500_0D77, 0x0500_26F3), Some(true));
        assert_eq!(p(0x0500_1CBF, 0x0500_2737), Some(true));
        for (o, l) in [
            (0x0500_1D2F, 0x0500_2751),
            (0x0500_1006, 0x0500_2875),
            (0x0500_0EDB, 0x0500_2776),
            (0x0500_1D2D, 0x0500_2752),
        ] {
            assert_eq!(p(o, l), Some(false), "{o:#010X} draws untinted");
        }
        assert_eq!(p(0x0500_1CBD, 0x0500_273A), Some(true));
        let tinted = PAIRS.iter().filter(|p| p.2).count();
        assert_eq!((tinted, PAIRS.len() - tinted), (41, 133));
    }

    #[test]
    fn the_pairs_are_sorted_for_their_search() {
        assert!(PAIRS
            .windows(2)
            .all(|w| (w[0].0, w[0].1) < (w[1].0, w[1].1)));
        assert!(paired(DataId(PAIRS[0].0), DataId(PAIRS[0].1)).is_some());
    }
}
