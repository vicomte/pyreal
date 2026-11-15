//! Ground cover: the art pack's cover kinds (what grows on which terrain types,
//! how thick, how large, how steep) placed on the GPU every frame round the eye and drawn by a
//! handful of GPU-written draws.
//!
//! - **Placement** (`cover.wgsl`): one invocation per point of each kind's grid, laid on the
//!   world's own metres, every choice a hash of the kind and the point, so a place grows the same
//!   plants on every visit; a workgroup a tile of [`TILE`] points a side, each kind its own
//!   tiles, a tile out of reach or view passed over whole. A plant stands where the terrain types
//!   about it are its kind's (as a share across the cell, its edge wandering), off roads,
//!   buildings, the objects standing on the land ([`clearing_radius`]) and water, thinning on
//!   slopes toward the steepest its kind allows, in bare patches by the world's noise (the sparse
//!   kinds in drifts), taller in patches; frustum culled. Toward a road it thins and shrinks over
//!   a few metres, by how far it stands from the road as the ground's road alpha maps draw it
//!   ([`road_fields`]). It grows in by size, never whole at once.
//! - **Distance**: the pack's three levels of shape by distance over scale (their ends times the
//!   Object Detail scale), then a card (baked here from the near level, sixteen views round it)
//!   out to [`CARD_REACH`] times the last shape's end, thinning to nothing over its last quarter.
//!   Past that the ground's own colour. Over the [`BAND`] before a level's end a plant is drawn at
//!   both levels through complementary screen doors.
//! - **Draws**: one indirect draw per kind, level and material, its instance count written on the
//!   GPU: about thirty for the pack's six kinds.
//! - **Grass** (`blades.wgsl`, `cover_draw.wgsl` `vs_blade`): under the plants, clumps of curved
//!   blades placed on the GPU in tiles round the eye, near out to [`blade_reach`] (with Object
//!   Detail) and fewer, wider single blades beyond it (with Ground Cover), the near shrinking away
//!   as the far grow in. It keeps to the same bare patches, roads, buildings, water and slopes, takes
//!   the ground's colour as the plants do, dark at the root and lighter toward the tip, and bends in
//!   the same wind. It casts no shadow; the dense kinds stand a little thinner over it.
//! - **Flowers and stones** (`scatter.wgsl`, `scatter_draw.wgsl`): among the grass, out to
//!   [`scatter_reach`], placed in the same tiles. Wildflowers grow on grassland in drifts a few
//!   metres across, each stretch of meadow a few colours of its terrain's palette, and sway in the
//!   same wind. Stones lie on bare patches, dirt, sand and rock and along the road's edges, the
//!   ground's colour greyer. Both keep the grass's rules and cast no shadow.
//! - **Colour**: drawn toward the ground's under it (each terrain type's picture's mean), keeping
//!   its own light and dark and its vertex colour, so it sits on the ground as its grass.
//! - **Wind**: the weather's breeze ([`crate::wind`]) leans each plant with it, deeper as gusts
//!   blow across the field, and each takes them at its own stiffness, flutters at its own rates
//!   and leans its own way, bent stiff at the foot
//!   with its length kept (`cover_types.wgsl`); the bodies near the eye push it aside and down
//!   within a metre.

use pyreal_assets::region::TexMerge;
use pyreal_primitives::num::math;
use pyreal_primitives::DataId;
use pyreal_terrain::consts::{CELL_SIZE, ROAD_WIDTH};
use pyreal_terrain::land::merge::{find_road_alpha, get_road_code, MergeKey};
use pyreal_terrain::road::on_road_cell_with;

use crate::gpu::Gpu;

/// A kind's levels: three of shape, then its card.
pub const LEVELS: usize = 4;
/// How far the cards reach, times where the last level of shape ends.
pub const CARD_REACH: f32 = 2.0;
/// The bytes of a kind's record and of a plant's.
pub const KIND_SIZE: usize = 112;
pub const PLANT_SIZE: u64 = 32;
/// The most bodies that push the grass aside.
pub const MOST_PUSHERS: usize = 8;
/// The most plants the list holds in all.
pub const MOST_PLANTS: u64 = 1_500_000;
/// Grid points a side of a placement tile (`cover.wgsl` `COVER_TILE`).
const TILE: u32 = 8;
/// The band before a level's end it is crossfaded into the next over, a share of the end.
pub const BAND: f32 = 0.25;
/// The room for the grass's near clumps and for its far ones (`blades.wgsl`).
pub const NEAR_ROOM: u32 = 200_000;
pub const FAR_ROOM: u32 = 150_000;
/// The bytes of a clump.
const CLUMP_SIZE: u64 = 32;
/// The bytes of the placement's parameters (`cover.wgsl` `CoverParams`).
const PARAMS_SIZE: usize = 208;
/// Blades a clump and segments a blade, near and far (`cover_draw.wgsl` `vs_blade`).
pub const NEAR_BLADES: (u32, u32) = (5, 4);
pub const FAR_BLADES: (u32, u32) = (3, 1);
/// Metres a side of the grass's tiles (`blades.wgsl` `BLADE_TILE`).
const BLADE_TILE: f32 = 8.0;
/// Candidate clumps a square metre, near and far, at the whole reach.
const NEAR_CLUMPS: f32 = 16.0;
const FAR_CLUMPS: f32 = 1.2;
/// The room for the flowers and for the stones among the grass (`scatter.wgsl`).
pub const FLOWER_ROOM: u32 = 60_000;
pub const STONE_ROOM: u32 = 80_000;
/// The corners of a flower and of a stone (`scatter_draw.wgsl`).
pub const FLOWER_CORNERS: u32 = 36;
pub const STONE_CORNERS: u32 = 54;
/// Candidate flowers and stones a square metre, at the whole reach.
const FLOWER_CANDIDATES: f32 = 3.5;
const STONE_CANDIDATES: f32 = 5.0;

/// The corners of a clump of `blades` blades of `segments` segments.
#[must_use]
pub const fn clump_corners((blades, segments): (u32, u32)) -> u32 {
    blades * (6 * (segments - 1) + 3)
}

/// How far the grass's near clumps and its far ones reach at a reach's share and a detail scale,
/// metres: the near with Object Detail, the far with Ground Cover.
#[must_use]
pub fn blade_reach(reach: f32, detail: f32) -> (f32, f32) {
    let far = (70.0 * reach).clamp(30.0, 260.0);
    let near = (18.0 * detail.clamp(0.5, 3.0)).min(far * 0.6);
    (near, far)
}

/// How far the flowers and the stones reach at a reach's share and a detail scale, metres: both
/// with Object Detail, short of the grass's near clumps' reach.
#[must_use]
pub fn scatter_reach(reach: f32, detail: f32) -> (f32, f32) {
    let detail = detail.clamp(0.5, 3.0);
    let (_, far) = blade_reach(reach, detail);
    (
        (10.0 + 12.0 * detail).min(far * 0.6),
        (8.0 + 10.0 * detail).min(far * 0.5),
    )
}

/// The least and the most an object's ground clearing spans, metres: a smaller one is an item
/// the cover may hide, a larger one more than a disc can stand for.
pub const CLEARING_LEAST: f32 = 0.3;
pub const CLEARING_MOST: f32 = 4.0;
/// How high above its foot a cylinder or a physics corner stands and still meets the ground,
/// metres.
pub const CLEARING_BASE: f32 = 0.5;
/// How far an object's foot may stand from the ground under it and still clear it, metres.
pub const CLEARING_GROUND: f32 = 0.6;
/// The most clearings a land cell's list holds (`cover.wgsl` `object_keep`).
const CLEARINGS_A_CELL: usize = 16;

/// The radius of ground a solid object clears of cover, from its physics: its cylinders' reach
/// from its foot (`(low point, radius)`), else the reach of its other shape at its foot
/// (`bounding`); `None` without one, or with one too small or too large.
#[must_use]
pub fn clearing_radius(
    cylinders: impl IntoIterator<Item = (glam::Vec3, f32)>,
    bounding: Option<f32>,
    scale: f32,
) -> Option<f32> {
    let mut reach: Option<f32> = None;
    for (low, r) in cylinders {
        if low.z < CLEARING_BASE {
            reach = Some(reach.unwrap_or(0.0).max(low.truncate().length() + r));
        }
    }
    let r = reach.or(bounding)? * scale;
    (CLEARING_LEAST..=CLEARING_MOST).contains(&r).then_some(r)
}

/// The clearings as the cover reads them, over a land field of `side` vertices whose first lies
/// at `origin`: its sides, where the lists and the discs start, each vertex's cell's first and
/// count of the clearings reaching it, the lists, then each clearing's x, y and radius (render
/// space), all words.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn clearing_words(origin: glam::Vec2, side: (usize, usize), discs: &[glam::Vec3]) -> Vec<u32> {
    let (w, h) = side;
    let mut lists: Vec<Vec<u32>> = vec![Vec::new(); w * h];
    for (k, d) in discs.iter().enumerate() {
        let reach = d.z + 1.0;
        let cell = |v: glam::Vec2| ((v - origin) / CELL_SIZE).floor();
        let (a, b) = (cell(d.truncate() - reach), cell(d.truncate() + reach));
        for y in (a.y as i64).max(0)..=(b.y as i64).min(h as i64 - 1) {
            for x in (a.x as i64).max(0)..=(b.x as i64).min(w as i64 - 1) {
                #[allow(clippy::cast_sign_loss)]
                let l = &mut lists[y as usize * w + x as usize];
                if l.len() < CLEARINGS_A_CELL {
                    l.push(u32::try_from(k).unwrap_or(0));
                }
            }
        }
    }
    let listed: usize = lists.iter().map(Vec::len).sum();
    let word = |v: usize| u32::try_from(v).unwrap_or(0);
    let mut out = Vec::with_capacity(4 + 2 * w * h + listed + 3 * discs.len());
    out.extend([
        word(w),
        word(h),
        word(4 + 2 * w * h),
        word(4 + 2 * w * h + listed),
    ]);
    let mut first = 0;
    for l in &lists {
        out.extend([word(first), word(l.len())]);
        first += l.len();
    }
    out.extend(lists.iter().flatten());
    out.extend(
        discs
            .iter()
            .flat_map(|d| [d.x.to_bits(), d.y.to_bits(), d.z.to_bits()]),
    );
    out
}

/// What the cover keeps clear of: the land's grid, its statics' and scenery's clearings, the
/// objects' this frame, and their buffer.
#[derive(Debug)]
struct Clearings {
    origin: glam::Vec2,
    side: (usize, usize),
    fixed: Vec<glam::Vec3>,
    objects: Vec<glam::Vec3>,
    buffer: wgpu::Buffer,
}

impl Clearings {
    fn new(gpu: &Gpu) -> Self {
        Self {
            origin: glam::Vec2::ZERO,
            side: (0, 0),
            fixed: Vec::new(),
            objects: Vec::new(),
            buffer: clearing_buffer(gpu, 16),
        }
    }

    /// The clearings' words on the GPU; whether the buffer had to grow.
    fn upload(&mut self, gpu: &Gpu) -> bool {
        let discs: Vec<glam::Vec3> = self.fixed.iter().chain(&self.objects).copied().collect();
        let bytes = words(&clearing_words(self.origin, self.side, &discs));
        let grew = bytes.len() as u64 > self.buffer.size();
        if grew {
            self.buffer = clearing_buffer(gpu, (bytes.len() as u64).next_power_of_two());
        }
        gpu.queue.write_buffer(&self.buffer, 0, &bytes);
        grew
    }
}

fn clearing_buffer(gpu: &Gpu, size: u64) -> wgpu::Buffer {
    gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cover clearings"),
        size: size.max(16),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// The terrain types whose ground colour the cover is tinted toward.
pub const GROUND_TYPES: usize = 32;
/// Samples a side of a cell's road field (`cover.wgsl` `ROAD_SIDE`).
pub const ROAD_SIDE: usize = 25;
/// Texels a side of the road's mask the fields are measured on.
const ROAD_MASK: usize = 96;

/// How far each point of a land cell lies from the edge of the road drawn on it, metres (negative
/// on the road), for each of the sixteen sets of road corners (SW 1, SE 2, NE 4, NW 8):
/// [`ROAD_SIDE`] squared samples each, east then north. The road is where its alpha maps draw it
/// (`alpha` reads one: size and alpha, rows from the top), or retail's on-road test without them.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn road_fields(
    tm: Option<&TexMerge>,
    alpha: &mut dyn FnMut(DataId) -> Option<(u32, u32, Vec<u8>)>,
) -> Vec<f32> {
    let n = ROAD_MASK;
    let mut out = Vec::with_capacity(16 * ROAD_SIDE * ROAD_SIDE);
    for rc in 0u16..16 {
        let bit = |b: u16| (rc >> b) & 1;
        // Corner order SW, SE, NE, NW, as the merge key holds them.
        let key = MergeKey::new([0; 4], [bit(0), bit(1), bit(2), bit(3)], 0);
        let choice = get_road_code(key);
        let mut masks = Vec::new();
        for code in choice.codes.into_iter().filter(|&c| c != 0) {
            let map = tm.and_then(|tm| find_road_alpha(tm, code, key));
            masks.push(map.and_then(|m| Some((alpha(m.tex_gid)?, m.rotation.index()))));
        }
        // Texel `(i, j)` (east, north) of the cell is road.
        let mut road = vec![choice.all_road; n * n];
        let at = |i: usize| (i as f32 + 0.5) / n as f32;
        for (k, mask) in masks.iter().enumerate() {
            for j in 0..n {
                for i in 0..n {
                    let on = match mask {
                        Some(((w, h, a), rot)) => {
                            let uv = rotate_uv(at(i), 1.0 - at(j), *rot);
                            let x = ((uv.0 * *w as f32) as usize).min(*w as usize - 1);
                            let y = ((uv.1 * *h as f32) as usize).min(*h as usize - 1);
                            // Alpha nought is the road's, as the ground blends it.
                            a.get(y * *w as usize + x).is_some_and(|&v| v < 128)
                        }
                        None if k == 0 => on_road_cell_with(
                            bit(0) != 0,
                            bit(3) != 0,
                            bit(1) != 0,
                            bit(2) != 0,
                            at(i) * CELL_SIZE,
                            at(j) * CELL_SIZE,
                            CELL_SIZE,
                            ROAD_WIDTH,
                        ),
                        None => false,
                    };
                    road[j * n + i] |= on;
                }
            }
        }
        // Only the texels either side of the road's edge need be measured to.
        let rim = |want: bool| -> Vec<(f32, f32)> {
            (0..n * n)
                .filter(|&t| {
                    let (i, j) = (t % n, t / n);
                    let other = |u: usize| road[u] != want;
                    road[t] == want
                        && ((i > 0 && other(t - 1))
                            || (i + 1 < n && other(t + 1))
                            || (j > 0 && other(t - n))
                            || (j + 1 < n && other(t + n)))
                })
                .map(|t| (at(t % n) * CELL_SIZE, at(t / n) * CELL_SIZE))
                .collect()
        };
        let (edge, verge) = (rim(true), rim(false));
        let texel = CELL_SIZE / n as f32;
        let s = CELL_SIZE / (ROAD_SIDE - 1) as f32;
        for j in 0..ROAD_SIDE {
            for i in 0..ROAD_SIDE {
                let (x, y) = (i as f32 * s, j as f32 * s);
                let ti = ((x / texel) as usize).min(n - 1);
                let tj = ((y / texel) as usize).min(n - 1);
                let inside = road[tj * n + ti];
                let near = if inside { &verge } else { &edge }
                    .iter()
                    .map(|&(ex, ey)| math::hypotf(ex - x, ey - y))
                    .fold(1.0e3f32, f32::min);
                // Negative on the road, as deep as it lies.
                let d = (near - texel * 0.5).max(0.0);
                out.push(if inside { -d } else { d });
            }
        }
    }
    out
}

/// An alpha map's walk at `r` quarter turns (`gbuffer.wgsl` `rotate_uv`).
fn rotate_uv(u: f32, v: f32, r: usize) -> (f32, f32) {
    match r {
        1 => (1.0 - v, u),
        2 => (1.0 - u, 1.0 - v),
        3 => (v, 1.0 - u),
        _ => (u, v),
    }
}

/// One kind of cover as the engine draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct CoverKind {
    /// Its pack id, which seeds its placement.
    pub id: u32,
    /// The terrain types it grows on, a bit each.
    pub grounds: u32,
    /// Plants a square metre.
    pub density: f32,
    pub scale: (f32, f32),
    /// The steepest ground it grows on, degrees.
    pub steepest: f32,
    /// Its near level's height and bounding radius, metres.
    pub height: f32,
    pub radius: f32,
    pub wind: f32,
    /// Its near level's mean brightness as drawn (linear), which the ground's tint is scaled to.
    pub albedo: f32,
    /// Where each level ends, metres a metre of scale.
    pub ends: [f32; LEVELS],
    /// Each level's draws: first index (pool-absolute), index count, material.
    pub draws: [Vec<(u32, u32, u32)>; LEVELS],
}

impl CoverKind {
    /// How far from the eye it is drawn at all, metres.
    #[must_use]
    pub fn reach(&self) -> f32 {
        self.ends[LEVELS - 1] * self.scale.1
    }

    /// Metres between the points of its grid.
    #[must_use]
    pub fn step(&self) -> f32 {
        1.0 / self.density.max(1e-4).sqrt()
    }

    /// Where each level ends at a reach's share and a detail scale, metres a metre of scale: the
    /// card's end times the reach, the shapes' times the detail, short of the card's.
    #[must_use]
    pub fn ends_at(&self, reach: f32, detail: f32) -> [f32; LEVELS] {
        let card = self.ends[LEVELS - 1] * reach;
        std::array::from_fn(|l| {
            if l + 1 == LEVELS {
                card
            } else {
                (self.ends[l] * detail).min(card * 0.8)
            }
        })
    }

    /// Room for each level's plants at a reach's share and a detail scale: its ring's area (with
    /// the crossfade's bands) at its density, with a margin.
    #[must_use]
    pub fn capacities(&self, reach: f32, detail: f32) -> [u32; LEVELS] {
        let ends = self.ends_at(reach, detail);
        let mut out = [0; LEVELS];
        let mut inner = 0.0;
        for (l, o) in out.iter_mut().enumerate() {
            let outer = ends[l] * self.scale.1;
            let area = std::f32::consts::PI * (outer * outer - inner * inner).max(0.0);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let n = (area * self.density * 0.8) as u32 + 256;
            *o = n;
            inner = ends[l] * (1.0 - BAND) * self.scale.0;
        }
        out
    }
}

/// The cover on the GPU: its kinds, the placement and its draws.
#[derive(Debug)]
pub struct Cover {
    pub kinds: Vec<CoverKind>,
    /// How tall it grows, 1 the pack's own height (Grass Height).
    pub height: f32,
    /// The reach's share and the detail scale the lists are sized for ([`Self::fit`]).
    reach: f32,
    detail: f32,
    /// Each kind's levels' room.
    caps: Vec<[u32; LEVELS]>,
    params: wgpu::Buffer,
    counts: wgpu::Buffer,
    pub plants: wgpu::Buffer,
    pub args: wgpu::Buffer,
    pushers: wgpu::Buffer,
    /// The bodies that pushed the grass last frame.
    last_pushers: std::sync::Mutex<Vec<glam::Vec3>>,
    place_group: wgpu::BindGroup,
    place_layout: wgpu::BindGroupLayout,
    /// The buffers the placement's group holds besides the footprints'.
    kinds_buf: wgpu::Buffer,
    counters: wgpu::Buffer,
    /// The land's building footprints, their per-cell lists and the lists, and the drawn ground's
    /// offsets ([`Self::set_footprints`]).
    footprints: [wgpu::Buffer; 4],
    /// Each set of road corners' field ([`road_fields`]).
    roads: wgpu::Buffer,
    clearings: Clearings,
    pub draw_group: wgpu::BindGroup,
    /// The draws, in the arguments' order: (kind, level).
    pub draws: Vec<(usize, usize)>,
    /// The placement's tiles, all kinds'.
    tiles: u32,
    /// Each terrain type's ground colour (linear; w 1 where known) and its buffer.
    ground_colours: [[f32; 4]; GROUND_TYPES],
    grounds_buf: wgpu::Buffer,
    /// The grass under it: its clumps, their counts and their two draws (near, far).
    clumps: wgpu::Buffer,
    clump_counts: wgpu::Buffer,
    pub blade_args: wgpu::Buffer,
    /// The grass's near and far reach, metres, its candidates a square metre near and far, and
    /// its tiles across.
    blades: [f32; 4],
    blade_tiles: u32,
    /// The flowers' and stones' reach, metres, their candidates a square metre, and their tiles
    /// across.
    scatter: [f32; 4],
    scatter_tiles: u32,
    /// The terrain types a pack keeps the pebbles off, a bit each, and whether it keeps them off
    /// the road's edges ([`Self::keep_pebbles_off`]).
    bare: [u32; 2],
}

/// The cover pass's pipelines and layouts, made with the renderer.
#[derive(Debug)]
pub struct CoverPipelines {
    pub place: wgpu::ComputePipeline,
    pub blades: wgpu::ComputePipeline,
    pub scatter: wgpu::ComputePipeline,
    pub finish: wgpu::ComputePipeline,
    pub place_layout: wgpu::BindGroupLayout,
    pub draw_layout: wgpu::BindGroupLayout,
}

fn storage(binding: u32, read_only: bool, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn uniform(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

impl CoverPipelines {
    /// The placement's pipelines (`frame_layout` group 0) and the draw's layout (group 3).
    #[must_use]
    pub fn new(gpu: &Gpu, frame_layout: &wgpu::BindGroupLayout) -> Self {
        use wgpu::ShaderStages as S;
        let device = &gpu.device;
        let place_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cover placement"),
            entries: &[
                uniform(0, S::COMPUTE),
                storage(1, true, S::COMPUTE),
                storage(2, false, S::COMPUTE),
                storage(3, false, S::COMPUTE),
                storage(4, true, S::COMPUTE),
                storage(5, false, S::COMPUTE),
                storage(6, true, S::COMPUTE),
                storage(7, true, S::COMPUTE),
                storage(8, true, S::COMPUTE),
                storage(9, true, S::COMPUTE),
                storage(10, true, S::COMPUTE),
                storage(11, false, S::COMPUTE),
                storage(12, false, S::COMPUTE),
                storage(13, false, S::COMPUTE),
                uniform(14, S::COMPUTE),
                storage(15, true, S::COMPUTE),
            ],
        });
        let draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cover draws"),
            entries: &[
                storage(0, true, S::VERTEX),
                storage(1, true, S::VERTEX),
                uniform(2, S::VERTEX),
                uniform(3, S::VERTEX),
                storage(4, true, S::VERTEX),
            ],
        });
        let module = gpu.shader("cover", &crate::renderer::source::cover());
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cover placement"),
            bind_group_layouts: &[Some(frame_layout), Some(&place_layout)],
            immediate_size: 0,
        });
        let compute = |entry: &str| {
            gpu.compute_pipeline(wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            place: compute("place"),
            blades: compute("blades"),
            scatter: compute("scatter"),
            finish: compute("finish"),
            place_layout,
            draw_layout,
        }
    }
}

fn entire(binding: u32, b: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: b.as_entire_binding(),
    }
}

fn footprint_buffers(gpu: &Gpu, field: &crate::land::LandField) -> [wgpu::Buffer; 4] {
    let (prints, ranges, lists) = field.footprint_buffers();
    let ranges = if ranges.is_empty() {
        vec![0u8; 8]
    } else {
        ranges
    };
    [
        ("cover footprints", prints),
        ("cover footprint cells", ranges),
        ("cover footprint lists", lists),
        ("cover drawn ground", field.drawn_bytes()),
    ]
    .map(|(label, bytes)| {
        let b = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (bytes.len() as u64).max(16),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue.write_buffer(&b, 0, &bytes);
        b
    })
}

fn place_group(
    gpu: &Gpu,
    layout: &wgpu::BindGroupLayout,
    buffers: [&wgpu::Buffer; 6],
    footprints: &[wgpu::Buffer; 4],
    roads: &wgpu::Buffer,
    blades: [&wgpu::Buffer; 4],
    clearings: &wgpu::Buffer,
) -> wgpu::BindGroup {
    let entries: Vec<wgpu::BindGroupEntry<'_>> = buffers
        .iter()
        .chain(footprints.iter().collect::<Vec<_>>().iter())
        .chain([&roads])
        .chain(blades.iter())
        .chain([&clearings])
        .enumerate()
        .map(|(k, b)| entire(u32::try_from(k).unwrap_or(0), b))
        .collect();
    gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cover placement"),
        layout,
        entries: &entries,
    })
}

fn ground_bytes(colours: &[[f32; 4]; GROUND_TYPES]) -> Vec<u8> {
    colours
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect()
}

fn words(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|w| w.to_le_bytes()).collect()
}

impl Cover {
    /// `kinds` on the GPU, their lists laid end to end, at the pack's own distances, kept off
    /// the roads by `roads` ([`road_fields`]).
    #[must_use]
    pub fn new(
        gpu: &Gpu,
        pipelines: &CoverPipelines,
        kinds: Vec<CoverKind>,
        roads: &[f32],
    ) -> Self {
        let footprints = footprint_buffers(gpu, &crate::land::LandField::default());
        let grounds = [[0.0; 4]; GROUND_TYPES];
        let bytes: Vec<u8> = roads.iter().flat_map(|v| v.to_le_bytes()).collect();
        let roads = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cover roads"),
            size: (bytes.len() as u64).max(16),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue.write_buffer(&roads, 0, &bytes);
        Self::sized(
            gpu,
            pipelines,
            kinds,
            (1.0, 1.0),
            (footprints, roads, Clearings::new(gpu)),
            grounds,
        )
    }

    /// Sizes the lists for the reach's share `reach` (1 the pack's own distances, less to draw
    /// nearer) and the Object Detail scale `detail`, laying them out again when either changed.
    pub fn fit(&mut self, gpu: &Gpu, pipelines: &CoverPipelines, reach: f32, detail: f32) {
        let (reach, detail) = (reach.max(0.05), detail.clamp(0.25, 4.0));
        if (reach - self.reach).abs() < 1e-3 && (detail - self.detail).abs() < 1e-3 {
            return;
        }
        let footprints = std::mem::replace(
            &mut self.footprints,
            footprint_buffers(gpu, &crate::land::LandField::default()),
        );
        let kinds = std::mem::take(&mut self.kinds);
        let grounds = self.ground_colours;
        let bare = self.bare;
        let roads = self.roads.clone();
        let clearings = std::mem::replace(&mut self.clearings, Clearings::new(gpu));
        let land = (footprints, roads, clearings);
        let height = self.height;
        *self = Self::sized(gpu, pipelines, kinds, (reach, detail), land, grounds);
        self.height = height;
        self.bare = bare;
    }

    /// The reach's share and the detail scale the lists are sized for.
    #[must_use]
    pub fn sizing(&self) -> (f32, f32) {
        (self.reach, self.detail)
    }

    fn sized(
        gpu: &Gpu,
        pipelines: &CoverPipelines,
        kinds: Vec<CoverKind>,
        (reach, detail): (f32, f32),
        (footprints, roads, clearings): ([wgpu::Buffer; 4], wgpu::Buffer, Clearings),
        ground_colours: [[f32; 4]; GROUND_TYPES],
    ) -> Self {
        let device = &gpu.device;
        // Each kind's room, held within the list's.
        let mut caps: Vec<[u32; LEVELS]> =
            kinds.iter().map(|k| k.capacities(reach, detail)).collect();
        let total: u64 = caps.iter().flatten().map(|&c| u64::from(c)).sum();
        if total > MOST_PLANTS {
            #[allow(clippy::cast_precision_loss)]
            let f = MOST_PLANTS as f64 / total as f64;
            for c in caps.iter_mut().flatten() {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let n = (f64::from(*c) * f) as u32;
                *c = n;
            }
        }
        let mut records = Vec::with_capacity(kinds.len() * KIND_SIZE);
        let mut base = 0u32;
        let mut draws = Vec::new();
        let mut args = Vec::new();
        let mut counters = Vec::new();
        let mut tiles = 0u32;
        for (k, kind) in kinds.iter().enumerate() {
            let caps = caps[k];
            let mut bases = [0u32; LEVELS];
            for l in 0..LEVELS {
                bases[l] = base;
                base = base.saturating_add(caps[l]);
            }
            let first = u32::try_from(k * LEVELS).unwrap_or(0);
            for w in [kind.grounds, first, kind.id, kind.albedo.to_bits()] {
                records.extend_from_slice(&w.to_le_bytes());
            }
            let steepest = pyreal_primitives::num::math::cosf(kind.steepest.to_radians());
            for v in [
                kind.step(),
                kind.scale.0,
                kind.scale.1,
                steepest,
                kind.height,
                kind.radius,
                kind.wind,
                kind.reach() * reach,
            ]
            .into_iter()
            .chain(kind.ends_at(reach, detail))
            {
                records.extend_from_slice(&v.to_le_bytes());
            }
            records.extend(words(&bases));
            records.extend(words(&caps));
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let across =
                ((kind.reach() * reach * 2.0 / kind.step()).ceil() as u32 + 1).div_ceil(TILE);
            records.extend(words(&[across, tiles, 0, 0]));
            tiles = tiles.saturating_add(across * across);
            for (l, level) in kind.draws.iter().enumerate() {
                for &(first_index, count, _) in level {
                    draws.push((k, l));
                    args.extend(words(&[count, 0, first_index, 0, bases[l]]));
                    counters.extend(words(&[first + u32::try_from(l).unwrap_or(0), caps[l]]));
                }
            }
        }
        let total = u64::from(base).clamp(1, MOST_PLANTS);
        use wgpu::BufferUsages as U;
        let buffer = |label, size: u64, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size.max(16),
                usage,
                mapped_at_creation: false,
            })
        };
        let kinds_buf = buffer(
            "cover kinds",
            records.len() as u64,
            U::STORAGE | U::COPY_DST,
        );
        gpu.queue.write_buffer(&kinds_buf, 0, &records);
        let counts = buffer(
            "cover counts",
            (kinds.len() * LEVELS * 4) as u64,
            U::STORAGE | U::COPY_DST | U::COPY_SRC,
        );
        let plants = buffer("cover plants", total * PLANT_SIZE, U::STORAGE | U::COPY_SRC);
        let args_buf = buffer(
            "cover draws",
            args.len() as u64,
            U::STORAGE | U::INDIRECT | U::COPY_DST,
        );
        gpu.queue.write_buffer(&args_buf, 0, &args);
        let counters_buf = buffer(
            "cover counters",
            counters.len() as u64,
            U::STORAGE | U::COPY_DST,
        );
        gpu.queue.write_buffer(&counters_buf, 0, &counters);
        let params = buffer("cover params", PARAMS_SIZE as u64, U::UNIFORM | U::COPY_DST);
        let pushers = buffer(
            "cover pushers",
            16 + 32 * MOST_PUSHERS as u64,
            U::UNIFORM | U::COPY_DST,
        );
        let grounds_buf = buffer(
            "cover ground colours",
            (GROUND_TYPES * 16) as u64,
            U::UNIFORM | U::COPY_DST,
        );
        gpu.queue
            .write_buffer(&grounds_buf, 0, &ground_bytes(&ground_colours));
        let clumps = buffer(
            "cover grass clumps",
            u64::from(NEAR_ROOM + FAR_ROOM + FLOWER_ROOM + STONE_ROOM) * CLUMP_SIZE,
            U::STORAGE,
        );
        let clump_counts = buffer(
            "cover grass counts",
            16,
            U::STORAGE | U::COPY_DST | U::COPY_SRC,
        );
        let blade_args = buffer(
            "cover grass draws",
            64,
            U::STORAGE | U::INDIRECT | U::COPY_DST,
        );
        gpu.queue.write_buffer(
            &blade_args,
            0,
            &words(&[
                clump_corners(NEAR_BLADES),
                0,
                0,
                0,
                clump_corners(FAR_BLADES),
                0,
                0,
                NEAR_ROOM,
                FLOWER_CORNERS,
                0,
                0,
                NEAR_ROOM + FAR_ROOM,
                STONE_CORNERS,
                0,
                0,
                NEAR_ROOM + FAR_ROOM + FLOWER_ROOM,
            ]),
        );
        let (near, far) = blade_reach(reach, detail);
        // Short draws thinner grass as well as nearer.
        let thick = reach.clamp(0.6, 1.0);
        let blades = [near, far, NEAR_CLUMPS * thick, FAR_CLUMPS * thick];
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let blade_tiles = (far * 2.0 / BLADE_TILE).ceil() as u32 + 1;
        let (flowers, stones) = scatter_reach(reach, detail);
        let scatter = [
            flowers,
            stones,
            FLOWER_CANDIDATES * thick,
            STONE_CANDIDATES * thick,
        ];
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let scatter_tiles = (flowers.max(stones) * 2.0 / BLADE_TILE).ceil() as u32 + 1;
        let place_group = place_group(
            gpu,
            &pipelines.place_layout,
            [
                &params,
                &kinds_buf,
                &counts,
                &plants,
                &counters_buf,
                &args_buf,
            ],
            &footprints,
            &roads,
            [&clumps, &clump_counts, &blade_args, &grounds_buf],
            &clearings.buffer,
        );
        let draw_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cover draws"),
            layout: &pipelines.draw_layout,
            entries: &[
                entire(0, &plants),
                entire(1, &kinds_buf),
                entire(2, &pushers),
                entire(3, &grounds_buf),
                entire(4, &clumps),
            ],
        });
        Self {
            kinds,
            height: 1.0,
            reach,
            detail,
            caps,
            params,
            counts,
            plants,
            args: args_buf,
            pushers,
            last_pushers: std::sync::Mutex::new(Vec::new()),
            place_group,
            place_layout: pipelines.place_layout.clone(),
            kinds_buf,
            counters: counters_buf,
            footprints,
            roads,
            clearings,
            draw_group,
            draws,
            tiles,
            ground_colours,
            grounds_buf,
            clumps,
            clump_counts,
            blade_args,
            blades,
            blade_tiles,
            scatter,
            scatter_tiles,
            bare: [0, 0],
        }
    }

    /// Keeps the pebbles and stones off the terrain types in `types` (a bit each), and off the
    /// road's edges where `road`.
    pub fn keep_pebbles_off(&mut self, types: u32, road: bool) {
        self.bare = [types, u32::from(road)];
    }

    /// Terrain type `terrain_type`'s ground colour (linear), which the cover on it is tinted toward.
    pub fn set_ground(&mut self, gpu: &Gpu, terrain_type: u32, colour: [f32; 3]) {
        let Some(slot) = self.ground_colours.get_mut(terrain_type as usize) else {
            return;
        };
        *slot = [colour[0], colour[1], colour[2], 1.0];
        gpu.queue
            .write_buffer(&self.grounds_buf, 0, &ground_bytes(&self.ground_colours));
    }

    /// The land's building footprints, which the cover keeps clear of, thinning toward the walls,
    /// and its statics' and scenery's clearings, those standing on the ground.
    pub fn set_footprints(&mut self, gpu: &Gpu, field: &crate::land::LandField) {
        self.footprints = footprint_buffers(gpu, field);
        let c = &mut self.clearings;
        c.origin = field.origin;
        c.side = field.side;
        c.fixed = field
            .clearings
            .iter()
            .filter(|f| {
                field
                    .height_at(f.truncate().truncate())
                    .is_some_and(|z| (f.z - z).abs() < CLEARING_GROUND)
            })
            .map(|f| glam::Vec3::new(f.x, f.y, f.w))
            .collect();
        c.upload(gpu);
        self.bind_placement(gpu);
    }

    /// What the objects standing on the land this frame clear about their feet (x, y and radius,
    /// render space), uploaded when it changed.
    pub fn set_objects(&mut self, gpu: &Gpu, objects: &[glam::Vec3]) {
        let held = &self.clearings.objects;
        if held.len() == objects.len()
            && held
                .iter()
                .zip(objects)
                .all(|(a, b)| a.abs_diff_eq(*b, 0.01))
        {
            return;
        }
        self.clearings.objects = objects.to_vec();
        if self.clearings.upload(gpu) {
            self.bind_placement(gpu);
        }
    }

    fn bind_placement(&mut self, gpu: &Gpu) {
        self.place_group = place_group(
            gpu,
            &self.place_layout,
            [
                &self.params,
                &self.kinds_buf,
                &self.counts,
                &self.plants,
                &self.counters,
                &self.args,
            ],
            &self.footprints,
            &self.roads,
            [
                &self.clumps,
                &self.clump_counts,
                &self.blade_args,
                &self.grounds_buf,
            ],
            &self.clearings.buffer,
        );
    }

    /// This frame's parameters: the view's frustum and eye, where render space's origin lies in
    /// the world, and the bodies that push the grass aside.
    pub fn prepare(
        &self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        planes: &[glam::Vec4],
        eye: glam::Vec3,
        world: glam::Vec2,
        bodies: &[glam::Vec3],
    ) {
        let mut p = Vec::with_capacity(PARAMS_SIZE);
        for k in 0..6 {
            let pl = planes
                .get(k)
                .copied()
                .unwrap_or(glam::Vec4::new(0.0, 0.0, 0.0, 1.0));
            p.extend(pl.to_array().iter().flat_map(|v| v.to_le_bytes()));
        }
        #[allow(clippy::cast_precision_loss)]
        for v in [
            eye.x,
            eye.y,
            eye.z,
            self.height,
            world.x,
            world.y,
            self.draws.len() as f32,
            self.kinds.len() as f32,
        ] {
            p.extend_from_slice(&v.to_le_bytes());
        }
        p.extend(self.blades.iter().flat_map(|v| v.to_le_bytes()));
        p.extend(words(&[NEAR_ROOM, NEAR_ROOM, FAR_ROOM, self.blade_tiles]));
        p.extend(self.scatter.iter().flat_map(|v| v.to_le_bytes()));
        p.extend(words(&[
            NEAR_ROOM + FAR_ROOM,
            FLOWER_ROOM,
            STONE_ROOM,
            self.scatter_tiles,
        ]));
        p.extend(words(&[self.bare[0], self.bare[1], 0, 0]));
        gpu.queue.write_buffer(&self.params, 0, &p);
        let n = bodies.len().min(MOST_PUSHERS);
        let mut b = words(&[u32::try_from(n).unwrap_or(0), self.height.to_bits(), 0, 0]);
        let vec = |b: &mut Vec<u8>, at: glam::Vec3| {
            b.extend(
                at.extend(0.0)
                    .to_array()
                    .iter()
                    .flat_map(|v| v.to_le_bytes()),
            );
        };
        for at in bodies[..n]
            .iter()
            .chain(std::iter::repeat_n(&glam::Vec3::ZERO, MOST_PUSHERS - n))
        {
            vec(&mut b, *at);
        }
        // Where each stood last frame: the nearest body then within a stride, so the push's motion
        // reaches the upscaler (itself where none was).
        let mut last = self
            .last_pushers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for at in &bodies[..n] {
            let was = last
                .iter()
                .copied()
                .filter(|w| w.distance_squared(*at) < 1.0)
                .min_by(|a, c| a.distance_squared(*at).total_cmp(&c.distance_squared(*at)))
                .unwrap_or(*at);
            vec(&mut b, was);
        }
        last.clear();
        last.extend_from_slice(&bodies[..n]);
        gpu.queue.write_buffer(&self.pushers, 0, &b);
        enc.clear_buffer(&self.counts, 0, None);
        enc.clear_buffer(&self.clump_counts, 0, None);
    }

    /// The placement and the draws' counts.
    pub fn encode(
        &self,
        pass: &mut wgpu::ComputePass<'_>,
        pipelines: &CoverPipelines,
        frame: &wgpu::BindGroup,
    ) {
        pass.set_bind_group(0, frame, &[]);
        pass.set_bind_group(1, &self.place_group, &[]);
        pass.set_pipeline(&pipelines.place);
        // A workgroup a tile, in rows short of the dispatch's limit.
        let row = self.tiles.clamp(1, 32_768);
        pass.dispatch_workgroups(row, self.tiles.div_ceil(row), 1);
        pass.set_pipeline(&pipelines.blades);
        pass.dispatch_workgroups(self.blade_tiles, self.blade_tiles, 1);
        pass.set_pipeline(&pipelines.scatter);
        pass.dispatch_workgroups(self.scatter_tiles, self.scatter_tiles, 1);
        pass.set_pipeline(&pipelines.finish);
        pass.dispatch_workgroups(
            u32::try_from(self.draws.len())
                .unwrap_or(0)
                .div_ceil(64)
                .max(1),
            1,
            1,
        );
    }

    /// The grass's clumps placed last frame, near and far, read back (blocking).
    pub fn read_clumps(&self, gpu: &Gpu) -> (u32, u32) {
        let b = gpu.read_buffer(&self.clump_counts, 0, 8);
        let w = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        (w(0).min(NEAR_ROOM), w(4).min(FAR_ROOM))
    }

    /// The flowers and stones placed last frame, read back (blocking).
    pub fn read_scatter(&self, gpu: &Gpu) -> (u32, u32) {
        let b = gpu.read_buffer(&self.clump_counts, 8, 8);
        let w = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        (w(0).min(FLOWER_ROOM), w(4).min(STONE_ROOM))
    }

    /// The plants placed last frame at each kind's levels, read back (blocking).
    pub fn read_counts(&self, gpu: &Gpu) -> Vec<[u32; LEVELS]> {
        let b = gpu.read_buffer(&self.counts, 0, (self.kinds.len() * LEVELS * 4) as u64);
        let w: Vec<u32> = b
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect();
        self.kinds
            .iter()
            .enumerate()
            .map(|(k, _)| {
                let caps = self.caps[k];
                std::array::from_fn(|l| w.get(k * LEVELS + l).copied().unwrap_or(0).min(caps[l]))
            })
            .collect()
    }

    /// The plants of the list, read back (blocking): each one's foot and scale, turn, kind and
    /// level, for the first `n` of each kind's level.
    pub fn read_plants(&self, gpu: &Gpu) -> Vec<([f32; 4], f32, u32, u32)> {
        let counts = self.read_counts(gpu);
        let mut out = Vec::new();
        let mut base = 0u64;
        for (caps, count) in self.caps.iter().zip(&counts) {
            for l in 0..LEVELS {
                let n = u64::from(count[l]);
                if n > 0 {
                    let b = gpu.read_buffer(&self.plants, base * PLANT_SIZE, n * PLANT_SIZE);
                    for p in b.as_chunks::<32>().0 {
                        let f = |i: usize| f32::from_le_bytes([p[i], p[i + 1], p[i + 2], p[i + 3]]);
                        let u = |i: usize| u32::from_le_bytes([p[i], p[i + 1], p[i + 2], p[i + 3]]);
                        out.push(([f(0), f(4), f(8), f(12)], f(16), u(20), u(24)));
                    }
                }
                base += u64::from(caps[l]);
            }
        }
        out
    }
}
