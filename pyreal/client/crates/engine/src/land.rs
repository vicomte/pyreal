//! The land as the GPU reads it beside the ground's triangles: one record per landscape vertex
//! of the loaded area (its height, terrain type, road, the cut of its cell, whether a building
//! stands there) and where the water lies.
//!
//! Retail draws water as ordinary ground: terrain types 16 to 20 are water, and a cell with a
//! water corner blends the water's picture in. Here the water is its own surface, laid at
//! retail's height: a water vertex's own height, and on the land beside it the level of the
//! water about it, so the water stays level up to a rising bank. Where it ends is a distance per
//! vertex (how far inside the water, negative on land): halfway between a water vertex and the
//! land beside it, rounded about lone water vertices, as retail's blend of the two pictures puts
//! it. Connected water vertices are one body, described by a [`crate::water::WaterLook`] picked
//! from its terrain types and its shore's.

use glam::{Vec2, Vec3};
use pyreal_terrain::consts::CELL_SIZE;
use pyreal_terrain::land::mesh::sw_to_ne_cut;
use pyreal_terrain::land::water::{road_bits, terrain_type, SurfChar, TERRAIN_SURF_CHAR};

/// Vertices along a block's side, its last shared with the next block.
const BLOCK_VERTICES: usize = 9;
/// How far from the water the distance is worked out, vertices; past it, the same.
const REACH: i32 = 10;

/// The record word's fields (`w` of a texel).
pub mod bits {
    /// The terrain type, 0 to 31.
    pub const TYPE: u32 = 0x1F;
    /// The road's two bits, shifted.
    pub const ROAD_SHIFT: u32 = 5;
    /// The cell whose south-west corner this is cuts south-west to north-east.
    pub const SW_TO_NE: u32 = 1 << 7;
    /// A building stands on the cell whose south-west corner this is.
    pub const BUILDING: u32 = 1 << 8;
    /// The water look's index, four bits.
    pub const LOOK_SHIFT: u32 = 9;
    /// The body's size class: its reach in eight-metre steps, seven bits.
    pub const SIZE_SHIFT: u32 = 13;
    /// The vertex is water.
    pub const WATER: u32 = 1 << 20;
    /// How far the body's swash runs up its shore, in [`super::SWASH_STEP`]s, six bits.
    pub const SWASH_SHIFT: u32 = 21;
    /// How steeply the bank rises from the water here ([`super::bank_rise`]), five bits.
    pub const BANK_SHIFT: u32 = 27;
}

/// The step the swash's run-up is kept in, metres.
pub const SWASH_STEP: f32 = 0.05;
/// The steepest bank kept, metres up a metre across; finer steps for gentle banks.
pub const BANK_MOST: f32 = 1.6;
/// How far over the level the ground stands where the still water stops showing against a bank.
pub const BANK_CLEAR: f32 = 0.05;

/// The bank's rise (metres up a metre across) a word's five bits keep.
#[must_use]
pub fn bank_rise(code: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let c = (code & 31) as f32 / 31.0;
    c * c * BANK_MOST
}

/// How far out from the drawn edge (halfway to the land) the still water stops showing against a
/// bank rising `rise` from a water vertex: retail's triangles lift the ground over the level from
/// the vertex on.
#[must_use]
pub fn bank_shift(rise: f32) -> f32 {
    (CELL_SIZE * 0.5 - BANK_CLEAR / rise.max(1e-4)).max(0.0)
}

/// One block's worth of what the field is built from.
#[derive(Debug, Clone)]
pub struct BlockLand {
    /// Its grid position.
    pub block: (i32, i32),
    /// Each vertex's height and terrain word, `vx * 9 + vy`.
    pub heights: [f32; 81],
    pub terrain: [u16; 81],
    /// The cells (`i * 8 + j`) a building stands on.
    pub buildings: Vec<usize>,
    /// Its buildings' footprints, block-local.
    pub footprints: Vec<Footprint>,
    /// Its statics' and scenery's ground clearings, block-local: each one's foot (x, y, z) and
    /// radius ([`crate::cover::clearing_radius`]).
    pub clearings: Vec<glam::Vec4>,
    /// Its drawn offset at the drawn ground's vertices ([`crate::drawn::vertex_offsets`]), with
    /// Landscape Smoothing.
    pub drawn: Option<std::sync::Arc<Vec<f32>>>,
}

/// A building's footprint: where its frame stands, the way its own x axis faces, and its walls'
/// box in its own frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Footprint {
    pub at: Vec2,
    pub axis: Vec2,
    pub low: Vec2,
    pub high: Vec2,
}

impl Footprint {
    /// How far `p` lies outside the walls, metres; nought inside.
    #[must_use]
    pub fn gap(&self, p: Vec2) -> f32 {
        let d = p - self.at;
        let l = Vec2::new(d.dot(self.axis), d.dot(self.axis.perp()));
        (self.low - l).max(l - self.high).max(Vec2::ZERO).length()
    }
}

/// How far past a building's walls the cover keeps thinner (`cover.wgsl`'s clearing, with its
/// wander).
pub const CLEARING_REACH: f32 = 3.5;

/// One body of water: its vertices' look and how far it reaches.
#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    /// Its water vertices.
    pub vertices: usize,
    /// The look it was given ([`crate::water::WaterTable`]'s index).
    pub look: usize,
    /// How far it reaches across, metres.
    pub reach: f32,
}

/// The land of the loaded area, one record per vertex, east then north.
#[derive(Debug, Clone, Default)]
pub struct LandField {
    /// The first vertex's world vertex coordinates (block times eight).
    pub first: (i32, i32),
    /// Vertices across and along.
    pub side: (usize, usize),
    /// Where the first vertex lies in render space.
    pub origin: Vec2,
    pub height: Vec<f32>,
    pub word: Vec<u32>,
    /// How far inside the water each vertex is, metres, negative on land.
    pub inside: Vec<f32>,
    /// The water's level at each vertex (on land, the water beside it).
    pub level: Vec<f32>,
    pub bodies: Vec<Body>,
    /// The loaded buildings' footprints, in render space.
    pub footprints: Vec<Footprint>,
    /// The loaded statics' and scenery's ground clearings, in render space.
    pub clearings: Vec<glam::Vec4>,
    /// The drawn ground's offset at its vertices ([`crate::drawn::SUB`] a cell), east then north;
    /// empty without Landscape Smoothing.
    pub drawn: Vec<f32>,
    /// The shore field ([`crate::shore`]), when built.
    pub shore: Option<std::sync::Arc<crate::shore::ShoreField>>,
}

impl LandField {
    /// An empty field over `blocks` blocks a side from block `first`, the first vertex at
    /// `origin` in render space.
    #[must_use]
    pub fn new(first_block: (i32, i32), blocks: (usize, usize), origin: Vec2) -> Self {
        let side = (blocks.0 * 8 + 1, blocks.1 * 8 + 1);
        let n = side.0 * side.1;
        Self {
            first: (first_block.0 * 8, first_block.1 * 8),
            side,
            origin,
            height: vec![0.0; n],
            word: vec![0; n],
            inside: vec![-(REACH as f32) * CELL_SIZE; n],
            level: vec![0.0; n],
            bodies: Vec::new(),
            footprints: Vec::new(),
            clearings: Vec::new(),
            drawn: Vec::new(),
            shore: None,
        }
    }

    /// The drawn ground's vertices across and along.
    #[must_use]
    pub fn drawn_side(&self) -> (usize, usize) {
        let s = crate::drawn::SUB;
        (
            self.side.0.saturating_sub(1) * s + 1,
            self.side.1.saturating_sub(1) * s + 1,
        )
    }

    /// The index of world vertex `(gx, gy)`, when the field holds it.
    #[must_use]
    pub fn index(&self, gx: i32, gy: i32) -> Option<usize> {
        let (x, y) = (gx - self.first.0, gy - self.first.1);
        let (x, y) = (usize::try_from(x).ok()?, usize::try_from(y).ok()?);
        (x < self.side.0 && y < self.side.1).then_some(y * self.side.0 + x)
    }

    /// Lays one block's vertices in.
    pub fn add_block(&mut self, b: &BlockLand) {
        #[allow(clippy::cast_precision_loss)]
        let corner = self.origin
            + Vec2::new(
                (b.block.0 * 8 - self.first.0) as f32,
                (b.block.1 * 8 - self.first.1) as f32,
            ) * CELL_SIZE;
        self.footprints
            .extend(b.footprints.iter().map(|f| Footprint {
                at: f.at + corner,
                ..*f
            }));
        self.clearings.extend(
            b.clearings
                .iter()
                .map(|c| *c + glam::Vec4::new(corner.x, corner.y, 0.0, 0.0)),
        );
        if let Some(d) = &b.drawn {
            let (w, h) = self.drawn_side();
            if self.drawn.is_empty() {
                self.drawn = vec![0.0; w * h];
            }
            let s = crate::drawn::SUB;
            let n = crate::drawn::BLOCK_SIDE;
            let (ox, oy) = (b.block.0 * 8 - self.first.0, b.block.1 * 8 - self.first.1);
            if let (Ok(ox), Ok(oy)) = (usize::try_from(ox), usize::try_from(oy)) {
                for v in 0..n {
                    for u in 0..n {
                        let (x, y) = (ox * s + u, oy * s + v);
                        if x < w && y < h {
                            self.drawn[y * w + x] = d.get(v * n + u).copied().unwrap_or(0.0);
                        }
                    }
                }
            }
        }
        for vx in 0..BLOCK_VERTICES {
            for vy in 0..BLOCK_VERTICES {
                let (gx, gy) = (
                    b.block.0 * 8 + i32::try_from(vx).unwrap_or(0),
                    b.block.1 * 8 + i32::try_from(vy).unwrap_or(0),
                );
                let Some(i) = self.index(gx, gy) else {
                    continue;
                };
                let k = vx * BLOCK_VERTICES + vy;
                let word = b.terrain[k];
                let mut w = u32::try_from(terrain_type(word)).unwrap_or(0)
                    | (u32::from(road_bits(word)) << bits::ROAD_SHIFT);
                if sw_to_ne_cut(gx, gy) {
                    w |= bits::SW_TO_NE;
                }
                if vx < 8 && vy < 8 && b.buildings.contains(&(vx * 8 + vy)) {
                    w |= bits::BUILDING;
                }
                if TERRAIN_SURF_CHAR[terrain_type(word)] == SurfChar::Water {
                    w |= bits::WATER;
                }
                // A block's last row is the next block's first: a building there is kept.
                self.word[i] = w | (self.word[i] & bits::BUILDING);
                self.height[i] = b.heights[k];
            }
        }
    }

    /// The footprints as the cover reads them: each two vectors (where it stands and its axis; its
    /// box), then for each vertex's cell the first and count of its list, then the lists (each a
    /// footprint whose clearing reaches the cell). A field with none holds one far footprint.
    #[must_use]
    pub fn footprint_buffers(&self) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let (w, h) = self.side;
        let mut prints = Vec::new();
        let far = Footprint {
            at: Vec2::splat(-1.0e7),
            axis: Vec2::X,
            low: Vec2::ZERO,
            high: Vec2::ZERO,
        };
        for f in self.footprints.iter().chain([&far]) {
            for v in [
                f.at.x, f.at.y, f.axis.x, f.axis.y, f.low.x, f.low.y, f.high.x, f.high.y,
            ] {
                prints.extend(v.to_le_bytes());
            }
        }
        let mut lists: Vec<Vec<u32>> = vec![Vec::new(); w * h];
        for (k, f) in self.footprints.iter().enumerate() {
            let corners = [
                f.low,
                Vec2::new(f.high.x, f.low.y),
                f.high,
                Vec2::new(f.low.x, f.high.y),
            ]
            .map(|c| f.at + f.axis * c.x + f.axis.perp() * c.y);
            let lo = corners.iter().fold(Vec2::splat(f32::MAX), |a, c| a.min(*c)) - CLEARING_REACH;
            let hi = corners.iter().fold(Vec2::splat(f32::MIN), |a, c| a.max(*c)) + CLEARING_REACH;
            let cell = |v: Vec2| ((v - self.origin) / CELL_SIZE).floor();
            let (a, b) = (cell(lo), cell(hi));
            #[allow(clippy::cast_possible_truncation)]
            for y in (a.y as i64).max(0)..=(b.y as i64).min(h as i64 - 1) {
                for x in (a.x as i64).max(0)..=(b.x as i64).min(w as i64 - 1) {
                    #[allow(clippy::cast_sign_loss)]
                    lists[y as usize * w + x as usize].push(u32::try_from(k).unwrap_or(0));
                }
            }
        }
        let (mut ranges, mut list) = (Vec::with_capacity(w * h * 8), Vec::new());
        for l in &lists {
            let first = u32::try_from(list.len() / 4).unwrap_or(0);
            for v in [first, u32::try_from(l.len()).unwrap_or(0)] {
                ranges.extend(v.to_le_bytes());
            }
            for v in l {
                list.extend(v.to_le_bytes());
            }
        }
        list.extend(0u32.to_le_bytes());
        (prints, ranges, list)
    }

    /// Whether vertex `i` is water.
    #[must_use]
    pub fn is_water(&self, i: usize) -> bool {
        self.word[i] & bits::WATER != 0
    }

    /// Works out where the water lies: each vertex's distance inside it, the water's level, its
    /// bodies and their looks (picked from `looks` by its water's terrain types and its shore's).
    pub fn finish(&mut self, looks: &crate::water::WaterTable) {
        let (w, h) = (self.side.0, self.side.1);
        let at = |x: i32, y: i32| -> Option<usize> {
            (x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h)
                .then(|| y as usize * w + x as usize)
        };
        let half = CELL_SIZE * 0.5;
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let water = self.is_water(i);
                // The nearest vertex of the other kind.
                let mut best = f32::MAX;
                for dy in -REACH..=REACH {
                    for dx in -REACH..=REACH {
                        let (Ok(xi), Ok(yi)) = (i32::try_from(x), i32::try_from(y)) else {
                            continue;
                        };
                        let Some(j) = at(xi + dx, yi + dy) else {
                            continue;
                        };
                        if self.is_water(j) != water {
                            #[allow(clippy::cast_precision_loss)]
                            let d = ((dx * dx + dy * dy) as f32).sqrt() * CELL_SIZE;
                            best = best.min(d);
                        }
                    }
                }
                let d = if best == f32::MAX {
                    REACH as f32 * CELL_SIZE
                } else {
                    best
                };
                self.inside[i] = if water { d - half } else { half - d };
                // A water vertex's level is its height; a land vertex's, the lowest water about it.
                self.level[i] = self.height[i];
                if !water {
                    let mut low = f32::MAX;
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let (Ok(xi), Ok(yi)) = (i32::try_from(x), i32::try_from(y)) else {
                                continue;
                            };
                            if let Some(j) = at(xi + dx, yi + dy).filter(|&j| self.is_water(j)) {
                                low = low.min(self.height[j]);
                            }
                        }
                    }
                    if low < f32::MAX {
                        self.level[i] = low;
                    }
                }
            }
        }
        self.find_bodies(looks);
        self.find_banks();
    }

    /// Each vertex beside the other kind: how steeply the bank rises across the water's edge
    /// there, the most up a metre across to a neighbour of the other kind (a cliff's, not a
    /// beach's), kept in the word.
    fn find_banks(&mut self) {
        let (w, h) = (self.side.0, self.side.1);
        let mut out = self.word.clone();
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let water = self.is_water(i);
                let mut rise = 0.0f32;
                for dy in -1isize..=1 {
                    for dx in -1isize..=1 {
                        let (Some(nx), Some(ny)) =
                            (x.checked_add_signed(dx), y.checked_add_signed(dy))
                        else {
                            continue;
                        };
                        if nx >= w || ny >= h {
                            continue;
                        }
                        let j = ny * w + nx;
                        if self.is_water(j) == water {
                            continue;
                        }
                        let (land, wet) = if water { (j, i) } else { (i, j) };
                        #[allow(clippy::cast_precision_loss)]
                        let across = ((dx * dx + dy * dy) as f32).sqrt() * CELL_SIZE;
                        rise = rise.max((self.height[land] - self.height[wet]) / across);
                    }
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let steps = (((rise.max(0.0) / BANK_MOST).sqrt() * 31.0).round() as u32).min(31);
                out[i] = (out[i] & !(31 << bits::BANK_SHIFT)) | (steps << bits::BANK_SHIFT);
            }
        }
        self.word = out;
    }

    /// How steeply the bank rises from the water at render-space `p` (metres up a metre across)
    /// and how far out from the drawn edge the still water stops showing ([`bank_shift`]),
    /// bilinear between the vertices.
    #[must_use]
    pub fn bank_at(&self, p: Vec2) -> (f32, f32) {
        let q = (p - self.origin) / CELL_SIZE;
        let c = q.floor();
        let (x, y) = (
            pyreal_primitives::num::floor_to_i32(c.x),
            pyreal_primitives::num::floor_to_i32(c.y),
        );
        let f = q - c;
        let at = |dx: i32, dy: i32| {
            let rise = self
                .index(self.first.0 + x + dx, self.first.1 + y + dy)
                .map_or(0.0, |i| bank_rise(self.word[i] >> bits::BANK_SHIFT));
            Vec2::new(rise, if rise > 0.0 { bank_shift(rise) } else { 0.0 })
        };
        let v = (at(0, 0) * (1.0 - f.x) + at(1, 0) * f.x) * (1.0 - f.y)
            + (at(0, 1) * (1.0 - f.x) + at(1, 1) * f.x) * f.y;
        (v.x, v.y)
    }

    /// Connected water vertices, each body given a look, a size class and its swash.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    fn find_bodies(&mut self, looks: &crate::water::WaterTable) {
        let (w, h) = (self.side.0, self.side.1);
        let mut body = vec![usize::MAX; w * h];
        self.bodies.clear();
        for start in 0..w * h {
            if !self.is_water(start) || body[start] != usize::MAX {
                continue;
            }
            let id = self.bodies.len();
            let mut stack = vec![start];
            body[start] = id;
            let mut members = Vec::new();
            let mut water_types = [0u32; 32];
            let mut shore_types = [0u32; 32];
            let (mut lo, mut hi) = ((usize::MAX, usize::MAX), (0, 0));
            while let Some(i) = stack.pop() {
                members.push(i);
                let (x, y) = (i % w, i / w);
                lo = (lo.0.min(x), lo.1.min(y));
                hi = (hi.0.max(x), hi.1.max(y));
                water_types[(self.word[i] & bits::TYPE) as usize] += 1;
                for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    if self.is_water(j) {
                        if body[j] == usize::MAX {
                            body[j] = id;
                            stack.push(j);
                        }
                    } else {
                        shore_types[(self.word[j] & bits::TYPE) as usize] += 1;
                    }
                }
            }
            #[allow(clippy::cast_precision_loss)]
            let reach = ((hi.0 - lo.0).max(hi.1 - lo.1) + 1) as f32 * CELL_SIZE;
            let l = looks.pick(&water_types, &shore_types).min(15);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let size = ((reach / 8.0) as u32).min(127);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let swash = looks.looks.get(l).map_or(0, |k| {
                ((k.swash / SWASH_STEP).round().max(0.0) as u32).min(63)
            });
            for &i in &members {
                self.word[i] |= (u32::try_from(l).unwrap_or(0) << bits::LOOK_SHIFT)
                    | (size << bits::SIZE_SHIFT)
                    | (swash << bits::SWASH_SHIFT);
            }
            self.bodies.push(Body {
                vertices: members.len(),
                look: l,
                reach,
            });
        }
        // The land beside a body takes its look, so the edge of the water reads it.
        let looked: Vec<u32> = (0..w * h)
            .map(|i| {
                if self.is_water(i) {
                    return self.word[i];
                }
                let (x, y) = (i % w, i / w);
                let mut out = self.word[i];
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                            continue;
                        }
                        let j = ny as usize * w + nx as usize;
                        if self.is_water(j) {
                            let keep = (0xF << bits::LOOK_SHIFT)
                                | (0x7F << bits::SIZE_SHIFT)
                                | (0x3F << bits::SWASH_SHIFT);
                            out = (out & !keep) | (self.word[j] & keep);
                        }
                    }
                }
                out
            })
            .collect();
        self.word = looked;
    }

    /// The texels the GPU reads, four words a vertex: height, distance inside the water and the
    /// water's level as float bits, then the record word.
    #[must_use]
    pub fn texels(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.word.len() * 16);
        for i in 0..self.word.len() {
            for v in [
                self.height[i].to_bits(),
                self.inside[i].to_bits(),
                self.level[i].to_bits(),
                self.word[i],
            ] {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out
    }

    /// Retail's height at render-space `p`, on its triangles, when the field holds it.
    #[must_use]
    pub fn height_at(&self, p: Vec2) -> Option<f32> {
        let q = (p - self.origin) / CELL_SIZE;
        let c = q.floor();
        let (x, y) = (
            pyreal_primitives::num::floor_to_i32(c.x),
            pyreal_primitives::num::floor_to_i32(c.y),
        );
        let i00 = self.index(self.first.0 + x, self.first.1 + y)?;
        let i10 = self.index(self.first.0 + x + 1, self.first.1 + y)?;
        let i01 = self.index(self.first.0 + x, self.first.1 + y + 1)?;
        let i11 = self.index(self.first.0 + x + 1, self.first.1 + y + 1)?;
        let h = |i: usize| self.height[i];
        Some(on_triangles(
            self.word[i00] & bits::SW_TO_NE != 0,
            [h(i00), h(i10), h(i01), h(i11)],
            q - c,
        ))
    }

    /// The drawn ground's offset at render-space `p`, as its triangles lay it; nought without
    /// Landscape Smoothing or outside the field.
    #[must_use]
    pub fn drawn_offset(&self, p: Vec2) -> f32 {
        if self.drawn.is_empty() {
            return 0.0;
        }
        let (w, h) = self.drawn_side();
        let q = (p - self.origin) / crate::drawn::STEP;
        let c = q.floor();
        let (x, y) = (
            pyreal_primitives::num::floor_to_i32(c.x),
            pyreal_primitives::num::floor_to_i32(c.y),
        );
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return 0.0;
        };
        if x + 1 >= w || y + 1 >= h {
            return 0.0;
        }
        let s = crate::drawn::SUB;
        let cut = self
            .index(
                self.first.0 + i32::try_from(x / s).unwrap_or(0),
                self.first.1 + i32::try_from(y / s).unwrap_or(0),
            )
            .is_some_and(|i| self.word[i] & bits::SW_TO_NE != 0);
        let d = |dx: usize, dy: usize| self.drawn[(y + dy) * w + x + dx];
        on_triangles(cut, [d(0, 0), d(1, 0), d(0, 1), d(1, 1)], q - c)
    }

    /// The drawn ground's height at render-space `p`: retail's and the offset.
    #[must_use]
    pub fn drawn_height(&self, p: Vec2) -> Option<f32> {
        Some(self.height_at(p)? + self.drawn_offset(p))
    }

    /// How far a body standing at render-space `at` is drawn above where physics puts it: the
    /// offset averaged over its stance, faded as it stands higher over retail's ground; never
    /// under the drawn ground there, but by the water (whose bed retail sinks bodies into).
    #[must_use]
    pub fn lift(&self, at: Vec3) -> f32 {
        if self.drawn.is_empty() {
            return 0.0;
        }
        let p = at.truncate();
        let Some(retail) = self.height_at(p) else {
            return 0.0;
        };
        let knobs = pyreal_terrain::land::drawn::Knobs::default();
        let keep = crate::drawn::keep(&knobs, at.z - retail);
        if keep <= 0.0 {
            return 0.0;
        }
        let near = pyreal_terrain::land::drawn::body_offset(
            |x, y| self.drawn_offset(Vec2::new(x, y)),
            p.x,
            p.y,
            &knobs,
        );
        let dry = self.water_at(p).is_none_or(|(_, inside)| inside < -3.0);
        let floor = if dry {
            retail + self.drawn_offset(p)
        } else {
            f32::MIN
        };
        pyreal_terrain::land::drawn::shift(
            at.z,
            pyreal_terrain::land::drawn::Drawn {
                offset: near,
                height: floor,
            },
            keep,
        )
    }

    /// The drawn offsets as the cover reads them: one float a vertex, or a lone nought without
    /// Landscape Smoothing.
    #[must_use]
    pub fn drawn_bytes(&self) -> Vec<u8> {
        if self.drawn.is_empty() {
            return vec![0; 4];
        }
        self.drawn.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// The water's level at render-space `p` on retail's triangles, and how far inside the water
    /// it lies (bilinear), when the field holds it.
    #[must_use]
    pub fn water_at(&self, p: Vec2) -> Option<(f32, f32)> {
        let q = (p - self.origin) / CELL_SIZE;
        let (cx, cy) = (q.x.floor(), q.y.floor());
        let (x, y) = (
            pyreal_primitives::num::floor_to_i32(cx),
            pyreal_primitives::num::floor_to_i32(cy),
        );
        let f = q - Vec2::new(cx, cy);
        let i00 = self.index(self.first.0 + x, self.first.1 + y)?;
        let i10 = self.index(self.first.0 + x + 1, self.first.1 + y)?;
        let i01 = self.index(self.first.0 + x, self.first.1 + y + 1)?;
        let i11 = self.index(self.first.0 + x + 1, self.first.1 + y + 1)?;
        let l = |i: usize| self.level[i];
        let level = on_triangles(
            self.word[i00] & bits::SW_TO_NE != 0,
            [l(i00), l(i10), l(i01), l(i11)],
            f,
        );
        let s = |i: usize| self.inside[i];
        let inside = (s(i00) * (1.0 - f.x) + s(i10) * f.x) * (1.0 - f.y)
            + (s(i01) * (1.0 - f.x) + s(i11) * f.x) * f.y;
        Some((level, inside))
    }
}

/// A value across a cell's two triangles, as retail cuts it: the corners' `[sw, se, nw, ne]` at
/// `f` (0..1 east and north).
#[must_use]
pub fn on_triangles(sw_to_ne: bool, [h00, h10, h01, h11]: [f32; 4], f: Vec2) -> f32 {
    if sw_to_ne {
        if f.x >= f.y {
            h00 + f.x * (h10 - h00) + f.y * (h11 - h10)
        } else {
            h00 + f.y * (h01 - h00) + f.x * (h11 - h01)
        }
    } else if f.x + f.y <= 1.0 {
        h00 + f.x * (h10 - h00) + f.y * (h01 - h00)
    } else {
        h11 + (1.0 - f.x) * (h01 - h11) + (1.0 - f.y) * (h10 - h11)
    }
}
