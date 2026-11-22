//! The terrain merge key and the ground layers it names.
//!
//! The cell rotation, the terrain selection, the terrain and road-code lookups, the terrain and
//! road alpha searches, and the ground plan the engine lays from them.
//!
//! Palette and surface records are described in `docs/formats/13-palette-and-surfaces.md`.

use pyreal_assets::region::{Region, TerrainDesc, TexMerge};
use pyreal_assets::world::CellLandblock;
use pyreal_primitives::DataId;

use crate::consts::{ROAD_TERRAIN_TYPE, SIDE_VERTEX_COUNT};
use crate::land::mesh::Rotation;

/// `TexMerge`'s 32-bit surface key.
///
/// Bits 0-19 are the four terrain types, five bits each, in the order SW, SE, NE, NW from the top
/// of that field; bits 20-27 are the four 2-bit road values in the same corner order; bits 28+ are
/// the LOD/detail code (`palLod`), from which surface restoration re-derives the texture size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MergeKey(pub u32);

impl MergeKey {
    /// `key(a, b, c, d)` uses corner order SW, SE, NE, NW.
    #[must_use]
    pub const fn new(types: [u16; 4], roads: [u16; 4], pal_lod: u32) -> Self {
        let r = (roads[0] as u32) * 64
            + (roads[1] as u32) * 16
            + (roads[2] as u32) * 4
            + roads[3] as u32;
        Self(
            (r << 20)
                | ((types[0] as u32) << 15)
                | ((types[1] as u32) << 10)
                | ((types[2] as u32) << 5)
                | (types[3] as u32)
                | (pal_lod << 28),
        )
    }

    /// The four corner terrain types, as re-extracts them:
    /// `t[0] = (key>>15)&0x1F` (SW), `t[1] = (key>>10)&0x1F` (SE), `t[2] = (key>>5)&0x1F` (NE),
    /// `t[3] = key&0x1F` (NW).
    #[must_use]
    pub const fn corner_types(self) -> [u32; 4] {
        [
            (self.0 >> 15) & 0x1F,
            (self.0 >> 10) & 0x1F,
            (self.0 >> 5) & 0x1F,
            self.0 & 0x1F,
        ]
    }

    /// `key >> 28` — the palette-LOD code the surface restore reads the size back out of.
    #[must_use]
    pub const fn pal_lod(self) -> u32 {
        self.0 >> 28
    }
}

/// The cell rotation followed by the terrain selection.
///
/// Returns `(key, rotation, uniform)`.
///
/// The client computes **four** keys, one per rotation of the corner pattern, so that a single
/// cached surface can serve four orientations of the same terrain arrangement. Only the
/// palette-shifting path uses more than the first: the texture-merging path takes `keys[0]`
/// and `ROT_0` unconditionally (no cell rotation). The region from Throne of Destiny on texture
/// merges; the region before it palette shifts, and [`crate::land::palshift::select`] chooses its
/// texture and rotation. [`cell_rotation_keys`] exposes the full set for the rotation test.
#[must_use]
pub fn cell_rotation(
    lb: &CellLandblock,
    region: &Region,
    side_cell_count: usize,
    i: usize,
    j: usize,
) -> (MergeKey, Rotation, bool) {
    let (keys, uniform) = cell_rotation_keys(lb, region, side_cell_count, i, j);
    match &region.land_surf.pal_shift {
        // Palette shifting chooses a texture and rotation per cell, at its global cell position.
        Some(ps) => {
            let choice = crate::land::palshift::select(ps, &keys, cell_x(lb, i), cell_y(lb, j));
            (choice.key, choice.rotation, uniform)
        }
        None => (keys[0], Rotation::Rot0, uniform),
    }
}

/// The global cell column of cell row `i` of a landblock: the block's x times eight, plus `i`.
#[must_use]
pub fn cell_x(lb: &CellLandblock, i: usize) -> i32 {
    i32::try_from((lb.id.raw() >> 24) * 8).unwrap_or(0) + i32::try_from(i).unwrap_or(0)
}

/// The global cell row of cell column `j` of a landblock: the block's y times eight, plus `j`.
#[must_use]
pub fn cell_y(lb: &CellLandblock, j: usize) -> i32 {
    i32::try_from(((lb.id.raw() >> 16) & 0xFF) * 8).unwrap_or(0) + i32::try_from(j).unwrap_or(0)
}

/// The full four-key set from `cell_rotation`, in `Rotation` order, plus the `uniform` flag.
///
/// ```text
/// keys[ROT_0]   = key(SW, SE, NE, NW)
/// keys[ROT_90]  = key(SE, NE, NW, SW)
/// keys[ROT_180] = key(NE, NW, SW, SE)
/// keys[ROT_270] = key(NW, SW, SE, NE)
/// uniform = (all four road values equal) && (all four types equal) && road_SW == 0
/// ```
#[must_use]
pub fn cell_rotation_keys(
    lb: &CellLandblock,
    region: &Region,
    side_cell_count: usize,
    i: usize,
    j: usize,
) -> ([MergeKey; 4], bool) {
    let step = crate::consts::BLOCK_SIDE / side_cell_count;
    // Palette shifting (type 1) is the land surface of the software region of a dat set from before
    // Throne of Destiny.
    let pal_shifted = is_pal_shifted(region);
    let pal_lod = if pal_shifted || step == 1 { 1 } else { 4 };

    // The 9x9 terrain grid, sampled at the LOD step. The client writes the index as
    // `(a*9 + b) * step`, which is the same thing as `(a*step)*9 + b*step`.
    let word = |a: usize, b: usize| lb.terrain[(a * SIDE_VERTEX_COUNT + b) * step];
    let corners = [
        word(i, j),
        word(i + 1, j),
        word(i + 1, j + 1),
        word(i, j + 1),
    ];
    let roads = corners.map(|w| w & 3);
    let types = corners.map(|w| (w >> 2) & 0x1F);

    let rot = |k: usize| {
        MergeKey::new(
            [
                types[k % 4],
                types[(k + 1) % 4],
                types[(k + 2) % 4],
                types[(k + 3) % 4],
            ],
            [
                roads[k % 4],
                roads[(k + 1) % 4],
                roads[(k + 2) % 4],
                roads[(k + 3) % 4],
            ],
            pal_lod,
        )
    };
    let keys = [rot(0), rot(1), rot(2), rot(3)];
    let uniform = roads.iter().all(|&r| r == roads[0])
        && types.iter().all(|&t| t == types[0])
        && roads[0] == 0;
    (keys, uniform)
}

/// True when the region's land surface is palette shifting (`LandSurf.type != 0`): the region
/// before Throne of Destiny. From Throne of Destiny on the region texture merges.
#[must_use]
pub fn is_pal_shifted(region: &Region) -> bool {
    region.land_surf.surf_type != 0
}

// -------------------------------------------------------------------------------------------
// get_terrain / get_road_code
// -------------------------------------------------------------------------------------------

/// What decides: the base terrain type plus up to three overlay
/// terrain types with their corner masks.
///
/// Corner bit numbering is **bit0 = SW, bit1 = SE, bit2 = NE, bit3 = NW**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TerrainChoice {
    /// `tex[0..4]` as terrain types; `tex[0]` is the base. `None` past the last overlay.
    pub tex: [Option<u32>; 4],
    /// `code[0..3]`; zero terminates. Each is a corner mask, or a two-corner "side" mask.
    pub code: [u32; 3],
}

/// The terrain lookup.
///
/// Note the asymmetry the transcription makes explicit: the adjacent pair (NW, SW) = mask 9 is
/// never produced, because the merge only fires for `i` immediately after the recorded corner.
/// That configuration falls through to two separate corner overlays (masks 1 and 8), and so do two
/// opposite corners.
#[must_use]
pub fn get_terrain(key: MergeKey) -> TerrainChoice {
    let t = key.corner_types();
    let mut out = TerrainChoice::default();

    // find the smallest i such that t[i] == t[j] for some j > i
    let base = (0..4).find(|&i| t.iter().skip(i + 1).any(|&o| o == t[i]));

    let Some(bi) = base else {
        // All four corners distinct: the SW corner is the base and the other three are overlaid
        // with their own corner masks.
        out.tex = [Some(t[0]), Some(t[1]), Some(t[2]), Some(t[3])];
        out.code = [2, 4, 8];
        return out;
    };

    let base = t[bi];
    out.tex[0] = Some(base);
    let mut second: Option<u32> = None;
    for (i, &ti) in t.iter().enumerate() {
        if ti == base {
            continue;
        }
        if out.code[0] == 0 {
            out.code[0] = 1 << i;
            out.tex[1] = Some(ti);
            second = Some(ti);
            continue;
        }
        if second == Some(ti)
            && ((i == 1 && out.code[0] == 1)
                || (i == 2 && out.code[0] == 2)
                || (i == 3 && out.code[0] == 4))
        {
            out.code[0] += 1 << i; // an adjacent-pair "side" mask: 3, 6 or 12
            return out;
        }
        out.tex[2] = Some(ti);
        out.code[1] = 1 << i;
        return out;
    }
    out
}

/// What the road-code lookup decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RoadChoice {
    /// The whole cell is road; the base texture is replaced rather than overlaid.
    pub all_road: bool,
    /// Up to two road masks; zero terminates. Three-corner roads are drawn as two overlapping side
    /// masks.
    pub codes: [u32; 2],
}

/// The road-code lookup.
#[must_use]
pub fn get_road_code(key: MergeKey) -> RoadChoice {
    let k = key.0;
    let rc = u32::from(k & 0x0C00_0000 != 0)          // SW road, bits 26-27
        | (u32::from(k & 0x0300_0000 != 0) << 1)       // SE road, bits 24-25
        | (u32::from(k & 0x00C0_0000 != 0) << 2)       // NE road, bits 22-23
        | (u32::from(k & 0x0030_0000 != 0) << 3); // NW road, bits 20-21
    match rc {
        0 => RoadChoice::default(),
        15 => RoadChoice {
            all_road: true,
            codes: [0, 0],
        },
        7 => RoadChoice {
            all_road: false,
            codes: [3, 6],
        }, // SW, SE, NE
        11 => RoadChoice {
            all_road: false,
            codes: [9, 3],
        }, // SW, SE, NW
        13 => RoadChoice {
            all_road: false,
            codes: [9, 12],
        }, // SW, NE, NW
        14 => RoadChoice {
            all_road: false,
            codes: [6, 12],
        }, // SE, NE, NW
        _ => RoadChoice {
            all_road: false,
            codes: [rc, 0],
        },
    }
}

// -------------------------------------------------------------------------------------------
// The alpha-map lookup
// -------------------------------------------------------------------------------------------

/// The pseudo-random alpha-map index both lookups derive from the surface key, in `f32` as the
/// project's scenery hash does; near a bucket's edge the client's own arithmetic may differ:
///
/// ```text
/// h = (int32)(key * 0x523AA99E - 0x51C9E74A)
/// idx = (int)floor(((uint32)h * 2^-32) * count)
/// if (float)(uint)idx >= (float)count: idx = 0
/// ```
#[must_use]
pub fn alpha_map_index(key: MergeKey, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let h = key.0.wrapping_mul(0x523A_A99E).wrapping_sub(0x51C9_E74A);
    #[allow(clippy::cast_precision_loss)] // the scenery hash's f32 path
    let f = (h as f32) * 2.328_306_4e-10 * (count as f32);
    let idx = pyreal_primitives::num::floor_to_i32(f);
    // The client's guard is `(float)(uint)idx >= (float)count`, i.e. it reinterprets a negative
    // index as a huge unsigned one and therefore catches it too.
    let idx = idx as u32 as usize;
    if idx >= count {
        0
    } else {
        idx
    }
}

/// `c*2` with `-15` on overflow — a 4-bit cyclic rotate of the corner mask
/// (1→2→4→8→1, 3→6→12→9→3).
#[must_use]
pub const fn rotate_code(c: u32) -> u32 {
    let c = c * 2;
    if c > 15 {
        c - 15
    } else {
        c
    }
}

/// One alpha map: which of the region's `CodeTexture` lists it came from, and how far it must be
/// rotated to present the wanted mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlphaMap {
    pub tex_gid: DataId,
    pub rotation: Rotation,
}

/// The terrain alpha search.
///
/// Uses `corner_terrain_maps` when the code is 1, 2, 4 or 8 and `side_terrain_maps` otherwise, then
/// takes **exactly** `maps[idx]` and rotates its `tcode` until it equals the wanted code. If four
/// rotations do not reach it, the lookup fails — the client returns 0 and the overlay is skipped.
#[must_use]
pub fn find_terrain_alpha(tm: &TexMerge, code: u32, key: MergeKey) -> Option<AlphaMap> {
    let maps = if matches!(code, 1 | 2 | 4 | 8) {
        &tm.corner_terrain_maps
    } else {
        &tm.side_terrain_maps
    };
    if maps.is_empty() {
        return None;
    }
    let m = &maps[alpha_map_index(key, maps.len())];
    let mut c = m.code;
    let mut rot = Rotation::Rot0;
    while c != code {
        c = rotate_code(c);
        rot = rot.next()?;
    }
    Some(AlphaMap {
        tex_gid: m.tex_gid,
        rotation: rot,
    })
}

/// The road alpha search.
///
/// Differs from [`find_terrain_alpha`]: it **scans all** `road_maps` starting at `idx` modulo the
/// count until one rotates to the wanted code.
#[must_use]
pub fn find_road_alpha(tm: &TexMerge, code: u32, key: MergeKey) -> Option<AlphaMap> {
    let maps = &tm.road_maps;
    if maps.is_empty() {
        return None;
    }
    let start = alpha_map_index(key, maps.len());
    for n in 0..maps.len() {
        let m = &maps[(start + n) % maps.len()];
        let mut c = m.code;
        let mut rot = Some(Rotation::Rot0);
        while let Some(r) = rot {
            if c == code {
                return Some(AlphaMap {
                    tex_gid: m.tex_gid,
                    rotation: r,
                });
            }
            c = rotate_code(c);
            rot = r.next();
        }
    }
    None
}

/// Select `terrain_desc[k]` for the entry whose `terrain_type == t`, falling
/// back to `terrain_desc[0]` when the type is absent.
#[must_use]
pub fn get_terrain_tex(tm: &TexMerge, t: u32) -> Option<&TerrainDesc> {
    tm.terrain_desc
        .iter()
        .find(|d| d.terrain_type == t)
        .or_else(|| tm.terrain_desc.first())
}

// -------------------------------------------------------------------------------------------
// The pixels
// -------------------------------------------------------------------------------------------

/// A decoded BGRA8 image. `dst[0] = B`, `[1] = G`, `[2] = R`, `[3] = A`, which is the byte order
/// used by the client's image textures and by the texture merge operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bgra8 {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

impl Bgra8 {
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![[0; 4]; (width * height) as usize],
        }
    }

    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        self.pixels[(y * self.width + x) as usize]
    }

    /// The buffer laid out for upload, `TextureFormat::Bgra8`.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.pixels.into_iter().flatten().collect()
    }
}

/// One layer of a cell's ground, as [`ground_plan`] names it: the ground of terrain type `ground`
/// (the road's is [`ROAD_TERRAIN_TYPE`]; `None` where the type names no picture) blended in
/// through the alpha map `alpha` walked at `rotation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroundOverlay {
    pub alpha: DataId,
    pub rotation: Rotation,
    pub ground: Option<u32>,
}

/// A cell's ground layers by terrain type: the ground under them, then each overlay in the
/// order it is laid. A renderer that draws its own material for each
/// terrain type lays them through the same alpha maps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundPlan {
    pub base: Option<u32>,
    pub overlays: Vec<GroundOverlay>,
}

/// The ground `key` lays, layer for layer as the client's texture merge composites it, each layer
/// named by the terrain type of the description whose picture it draws.
#[must_use]
pub fn ground_plan(tm: &TexMerge, key: MergeKey) -> GroundPlan {
    let choice = get_terrain(key);
    let road = get_road_code(key);
    let road_tex = get_terrain_tex(tm, ROAD_TERRAIN_TYPE);
    let ground_of = |t: Option<u32>| {
        t.and_then(|t| get_terrain_tex(tm, t))
            .map(|d| d.terrain_type)
    };
    let base = if road.all_road && road_tex.is_some() {
        road_tex.map(|d| d.terrain_type)
    } else {
        ground_of(choice.tex[0])
    };
    let mut plan = GroundPlan {
        base,
        overlays: Vec::new(),
    };
    if !road.all_road {
        for k in 0..3 {
            if choice.code[k] == 0 {
                break;
            }
            let Some(map) = find_terrain_alpha(tm, choice.code[k], key) else {
                continue;
            };
            plan.overlays.push(GroundOverlay {
                alpha: map.tex_gid,
                rotation: map.rotation,
                ground: ground_of(choice.tex[k + 1]),
            });
        }
        if let Some(rd) = road_tex {
            for k in 0..2 {
                if road.codes[k] == 0 {
                    break;
                }
                let Some(map) = find_road_alpha(tm, road.codes[k], key) else {
                    continue;
                };
                plan.overlays.push(GroundOverlay {
                    alpha: map.tex_gid,
                    rotation: map.rotation,
                    ground: Some(rd.terrain_type),
                });
            }
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyreal_assets::region::CodeTexture;

    fn tex_merge(size: u32) -> TexMerge {
        let desc = |t: u32, gid: u32| TerrainDesc {
            terrain_type: t,
            tex_gid: DataId(gid),
            tex_tiling: 1,
            max_vert_bright: 0,
            min_vert_bright: 0,
            max_vert_saturate: 0,
            min_vert_saturate: 0,
            max_vert_hue: 0,
            min_vert_hue: 0,
            detail_tex_tiling: 0,
            detail_tex_gid: DataId(0),
        };
        TexMerge {
            base_tex_size: size,
            // Four corner maps, all tcode 8, exactly as the retail region ships them.
            corner_terrain_maps: (0..4)
                .map(|i| CodeTexture {
                    code: 8,
                    tex_gid: DataId(0x0500_0100 + i),
                })
                .collect(),
            side_terrain_maps: vec![CodeTexture {
                code: 9,
                tex_gid: DataId(0x0500_0200),
            }],
            road_maps: vec![
                CodeTexture {
                    code: 9,
                    tex_gid: DataId(0x0500_0300),
                },
                CodeTexture {
                    code: 10,
                    tex_gid: DataId(0x0500_0301),
                },
                CodeTexture {
                    code: 8,
                    tex_gid: DataId(0x0500_0302),
                },
            ],
            terrain_desc: vec![
                desc(1, 0x0600_0001),
                desc(2, 0x0600_0002),
                desc(0x20, 0x0600_0020),
            ],
        }
    }

    /// Oracle: the merge-key bit layout, cross-read against `get_terrain`'s re-extraction
    /// (`t[0] = (key>>15)&0x1F` …) and `get_road_code`'s masks (SW is bits 26-27). If the two
    /// halves of the implementation disagreed, this is where it would show.
    #[test]
    fn key_layout_round_trips_through_both_readers() {
        let k = MergeKey::new([5, 9, 17, 30], [3, 0, 2, 1], 4);
        assert_eq!(k.corner_types(), [5, 9, 17, 30]);
        assert_eq!(k.pal_lod(), 4);
        // roads = 3*64 + 0*16 + 2*4 + 1 = 201 at bits 20-27.
        assert_eq!((k.0 >> 20) & 0xFF, 201);
        // get_road_code's masks must see SW and NE set, SE clear, NW set: rc = 1|4|8 = 13, which is
        // one of the three-corner cases and is drawn as the two overlapping side masks {9, 12}.
        let rc = get_road_code(k);
        assert_eq!(rc.codes, [9, 12]);
    }

    /// Oracle: [`get_terrain`], walked case by case. The
    /// asymmetry — mask 9 (NW, SW) is never produced — is the part a "tidier" implementation
    /// silently fixes.
    #[test]
    fn get_terrain_reproduces_every_documented_branch() {
        let k = |t: [u16; 4]| MergeKey::new(t, [0; 4], 1);

        // All four corners the same: base only, no overlays.
        let g = get_terrain(k([7, 7, 7, 7]));
        assert_eq!(g.tex[0], Some(7));
        assert_eq!(g.code, [0, 0, 0]);

        // All four distinct: SW is the base, the other three become corner overlays 2, 4, 8.
        let g = get_terrain(k([1, 2, 3, 4]));
        assert_eq!(g.tex, [Some(1), Some(2), Some(3), Some(4)]);
        assert_eq!(g.code, [2, 4, 8]);

        // SW and SE share; the NE corner is the first non-base, and NW matches it and is adjacent,
        // so the two merge into the side mask 4 + 8 = 12.
        let g = get_terrain(k([1, 1, 2, 2]));
        assert_eq!(g.tex[0], Some(1));
        assert_eq!(g.tex[1], Some(2));
        assert_eq!(g.code, [12, 0, 0]);

        // The forbidden pair: NW and SW would be mask 9. SW is the base here (it repeats at NW),
        // so the non-base corners are SE and NE -- a different configuration entirely. Build the
        // real mask-9 candidate instead: base at SE/NE, overlay at SW and NW.
        let g = get_terrain(k([2, 1, 1, 2]));
        assert_eq!(g.tex[0], Some(2), "SW repeats at NW, so SW is the base");
        assert_eq!(
            g.code,
            [2 + 4, 0, 0],
            "SE and NE are adjacent and merge to side mask 6"
        );

        // Two opposite corners: masks 2 and 8 never merge, so they stay two overlays.
        let g = get_terrain(k([1, 2, 1, 2]));
        assert_eq!(g.tex[0], Some(1));
        assert_eq!(g.code, [2, 8, 0], "opposite corners stay separate");
        assert!(g.code.iter().all(|&c| c != 9), "mask 9 is never produced");

        // A third distinct type stops the merge and takes the second overlay slot.
        let g = get_terrain(k([1, 2, 3, 1]));
        assert_eq!(g.tex[0], Some(1));
        assert_eq!(g.tex[1], Some(2));
        assert_eq!(g.tex[2], Some(3));
        assert_eq!(g.code, [2, 4, 0]);
    }

    /// Oracle: the road-code switch over all sixteen inputs.
    #[test]
    fn road_codes_cover_all_sixteen_corner_combinations() {
        let key_for = |rc: u32| {
            let roads = [
                u16::from(rc & 1 != 0),
                u16::from(rc & 2 != 0),
                u16::from(rc & 4 != 0),
                u16::from(rc & 8 != 0),
            ];
            MergeKey::new([0; 4], roads, 1)
        };
        for rc in 0..16u32 {
            let g = get_road_code(key_for(rc));
            let expect = match rc {
                0 => (false, [0, 0]),
                15 => (true, [0, 0]),
                7 => (false, [3, 6]),
                11 => (false, [9, 3]),
                13 => (false, [9, 12]),
                14 => (false, [6, 12]),
                _ => (false, [rc, 0]),
            };
            assert_eq!((g.all_road, g.codes), expect, "rc={rc}");
        }
    }

    /// Oracle: `c*2`, with `-15` on overflow, is a 4-bit cyclic rotate of
    /// the corner mask.
    #[test]
    fn rotate_code_is_a_four_bit_cyclic_rotate() {
        assert_eq!([1, 2, 4, 8].map(rotate_code), [2, 4, 8, 1]);
        assert_eq!([3, 6, 12, 9].map(rotate_code), [6, 12, 9, 3]);
        // Four rotations return every mask to itself, which is what makes the search terminate.
        for c in 1..16u32 {
            assert_eq!(
                rotate_code(rotate_code(rotate_code(rotate_code(c)))),
                c,
                "c={c}"
            );
        }
    }

    /// The ground plan lays a cell's layers by terrain type: the base, then each terrain overlay
    /// through the map the terrain search finds, then each road overlay through the road search's
    /// map, named by the road's type.
    #[test]
    fn the_ground_plan_names_each_layer_by_its_terrain_type() {
        let tm = tex_merge(16);
        let layer = |m: AlphaMap, ground| GroundOverlay {
            alpha: m.tex_gid,
            rotation: m.rotation,
            ground,
        };

        // SE alone differs: one corner overlay of type 2 over a base of type 1.
        let key = MergeKey::new([1, 2, 1, 1], [0; 4], 1);
        let corner = find_terrain_alpha(&tm, 2, key).expect("a corner map");
        assert_eq!(
            ground_plan(&tm, key),
            GroundPlan {
                base: Some(1),
                overlays: vec![layer(corner, Some(2))],
            }
        );

        // A road at SW lays over the terrain overlay.
        let key = MergeKey::new([1, 2, 1, 1], [1, 0, 0, 0], 1);
        let corner = find_terrain_alpha(&tm, 2, key).expect("a corner map");
        let road = find_road_alpha(&tm, 1, key).expect("a road map");
        assert_eq!(
            ground_plan(&tm, key),
            GroundPlan {
                base: Some(1),
                overlays: vec![layer(corner, Some(2)), layer(road, Some(ROAD_TERRAIN_TYPE))],
            }
        );

        // Type 7 has no description: it is drawn with the first one's picture, and named by
        // its type.
        let plan = ground_plan(&tm, MergeKey::new([7, 7, 7, 7], [0; 4], 1));
        assert_eq!(plan.base, Some(1));
        let all_road = ground_plan(&tm, MergeKey::new([1, 1, 1, 1], [1; 4], 1));
        assert_eq!(all_road.base, Some(ROAD_TERRAIN_TYPE));
        assert!(all_road.overlays.is_empty());
    }
}
