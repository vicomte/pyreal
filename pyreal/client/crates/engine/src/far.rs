//! The far terrain: CDLOD rings past the exact near cells, out to several kilometres.
//!
//! The landscape's heights around the area (24 m apart, as the dats hold them) go into a height
//! texture with point-sampled mips, and each vertex's terrain type into a colour texture (each
//! type's picture averaged to one colour, which is what it looks like from that far). Each frame a
//! quadtree over the field picks nodes by distance: a node of level `k` is one 16 by 16 grid
//! patch with vertices `24 · 2^k` metres apart, drawn where the eye is between its level's range
//! and the next finer one's. Vertices morph toward the next coarser grid as they near the end of
//! their range (CDLOD), so neighbouring levels meet without cracks. Inside the near square the far
//! terrain discards itself: the exact cells are drawn there, and at the square's edge the level-0
//! patches lie on the same vertices as the exact cells.
//!
//! Its water is the near water's: the field's vertices are worked out as a [`LandField`] (how far
//! inside the water, its level, each body's look), which the water pass reads where the far rings
//! draw water, so it shades on as the near water does past the square's edge.

use glam::{Vec2, Vec3, Vec4, Vec4Swizzles};
use pyreal_assets::{CellLandblock, Region};
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::DataId;
use pyreal_terrain::land::water::{SurfChar, TERRAIN_SURF_CHAR};
use pyreal_world_data::landblock::landblock_did;

use crate::gpu::Gpu;
use crate::land::{BlockLand, LandField};
use crate::renderer::{ALBEDO, DEPTH, IDS, MATERIAL, MOTION, NORMAL};

/// Quads along one side of a node's patch.
pub const PATCH: u32 = 16;
/// Metres between the dats' height samples.
pub const TEXEL: f32 = 24.0;
/// Nodes one frame can draw.
pub const MAX_NODES: usize = 4096;
/// The bytes of one node record.
pub const NODE_SIZE: u64 = 32;

/// The heights and colours around an area, on the dats' 24 m vertex grid.
#[derive(Debug, Clone, PartialEq)]
pub struct FarField {
    /// Render-space position of texel `(0, 0)`.
    pub origin: Vec2,
    /// Texels per side: a power of two at least as large as the field.
    pub size: u32,
    /// Texels per side the field covers; the rest repeat its edge.
    pub extent: u32,
    /// `size * size` heights, row `y` then column `x`.
    pub heights: Vec<f32>,
    /// `size * size` sRGB colours; alpha nought on water.
    pub colours: Vec<[u8; 4]>,
    /// The water's records, as the land field's texels ([`LandField::texels`]), `water_side`
    /// a side from texel `(0, 0)`; empty with no water.
    pub water: Vec<u8>,
    pub water_side: u32,
    /// The near square the exact cells cover, in render space.
    pub near_min: Vec2,
    pub near_max: Vec2,
    /// Landscape Smoothing: the heights drawn as a smooth surface through the vertices, meeting
    /// the drawn ground at the square's edge.
    pub smooth: bool,
}

impl FarField {
    /// Builds the field `radius` blocks out from block `center`, whose south-west corner is
    /// `center_origin` in render space; `near` is the square the exact cells cover. `colour`
    /// names each terrain type's colour (and the road's, as type 32).
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        store: &RetailDatStore,
        region: &Region,
        center: u16,
        center_origin: Vec3,
        radius: i32,
        near: (Vec2, Vec2),
        colour: &mut dyn FnMut(u32) -> [u8; 4],
    ) -> Self {
        let table = pyreal_terrain::land::mesh::height_table(region);
        let (cx, cy) = (i32::from(center >> 8), i32::from(center & 0xFF));
        let blocks = 2 * radius + 1;
        let extent = u32::try_from(blocks * 8 + 1).unwrap_or(1);
        let size = extent.next_power_of_two();
        #[allow(clippy::cast_precision_loss)]
        let origin = center_origin.truncate() - Vec2::splat(radius as f32 * 192.0);
        let n = size as usize;
        let mut heights = vec![0.0f32; n * n];
        let mut colours = vec![[0u8, 0, 0, 255]; n * n];
        // ORDER-OK: a cache by colour key, looked up only.
        let mut palette: std::collections::HashMap<u32, [u8; 4]> = std::collections::HashMap::new();
        let blocks_side = usize::try_from(blocks).unwrap_or(0);
        let mut land = LandField::new(
            (cx - radius, cy - radius),
            (blocks_side, blocks_side),
            origin,
        );
        let mut wet = false;
        for b in 0..blocks * blocks {
            let (bx, by) = (cx - radius + b / blocks, cy - radius + b % blocks);
            if !(0..=0xFE).contains(&bx) || !(0..=0xFE).contains(&by) {
                continue;
            }
            let id = u16::try_from((bx << 8) | by).unwrap_or(0);
            let Some(lb) = decode::<CellLandblock>(store, DbType::LandBlock, landblock_did(id))
            else {
                continue;
            };
            land.add_block(&BlockLand {
                block: (bx, by),
                heights: std::array::from_fn(|i| table[usize::from(lb.height[i])]),
                terrain: lb.terrain,
                buildings: Vec::new(),
                footprints: Vec::new(),
                clearings: Vec::new(),
                drawn: None,
            });
            let (ox, oy) = ((b / blocks) * 8, (b % blocks) * 8);
            for vx in 0..9usize {
                for vy in 0..9usize {
                    let (tx, ty) = (
                        usize::try_from(ox).unwrap_or(0) + vx,
                        usize::try_from(oy).unwrap_or(0) + vy,
                    );
                    if tx >= n || ty >= n {
                        continue;
                    }
                    let at = ty * n + tx;
                    heights[at] = table[usize::from(lb.height[vx * 9 + vy])];
                    let key = if lb.road(vx, vy) != 0 {
                        32
                    } else {
                        u32::from(lb.terrain_type(vx, vy))
                    };
                    let water =
                        TERRAIN_SURF_CHAR[usize::from(lb.terrain_type(vx, vy))] == SurfChar::Water;
                    wet |= water;
                    colours[at] = *palette.entry(key).or_insert_with(|| colour(key));
                    colours[at][3] = if water { 0 } else { 255 };
                }
            }
        }
        // Past the field's edge the edge repeats.
        let e = extent as usize;
        for y in 0..n {
            for x in 0..n {
                if x < e && y < e {
                    continue;
                }
                let from = y.min(e - 1) * n + x.min(e - 1);
                heights[y * n + x] = heights[from];
                colours[y * n + x] = colours[from];
            }
        }
        let (water, water_side) = if wet {
            land.finish(&crate::water::WaterTable::default());
            (land.texels(), u32::try_from(land.side.0).unwrap_or(0))
        } else {
            (Vec::new(), 0)
        };
        Self {
            origin,
            size,
            extent,
            heights,
            colours,
            water,
            water_side,
            near_min: near.0,
            near_max: near.1,
            smooth: false,
        }
    }

    /// Level `level`'s heights: every `2^level`-th texel, the vertices a patch of that level sits on.
    #[must_use]
    pub fn height_level(&self, level: u32) -> (u32, Vec<f32>) {
        let s = (self.size >> level).max(1);
        let step = 1usize << level;
        let n = self.size as usize;
        let mut out = Vec::with_capacity((s * s) as usize);
        for y in 0..s as usize {
            for x in 0..s as usize {
                out.push(self.heights[(y * step).min(n - 1) * n + (x * step).min(n - 1)]);
            }
        }
        (s, out)
    }

    /// The lowest and highest height of every node of each level, `[level][y * nodes + x]`.
    #[must_use]
    pub fn bounds(&self, levels: u32) -> Vec<Vec<(f32, f32)>> {
        let n = self.size as usize;
        let mut out: Vec<Vec<(f32, f32)>> = Vec::new();
        for level in 0..levels {
            let span = (PATCH as usize) << level;
            let count = n.div_ceil(span).max(1);
            let mut v = vec![(f32::MAX, f32::MIN); count * count];
            if level == 0 {
                for (j, row) in v.chunks_mut(count).enumerate() {
                    for (i, b) in row.iter_mut().enumerate() {
                        for y in (j * span)..=((j + 1) * span).min(n - 1) {
                            for x in (i * span)..=((i + 1) * span).min(n - 1) {
                                let h = self.heights[y * n + x];
                                *b = (b.0.min(h), b.1.max(h));
                            }
                        }
                    }
                }
            } else {
                let below = &out[level as usize - 1];
                let bc = n.div_ceil(span / 2).max(1);
                for j in 0..count {
                    for i in 0..count {
                        let mut b = (f32::MAX, f32::MIN);
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            let (x, y) = (i * 2 + dx, j * 2 + dy);
                            if x < bc && y < bc {
                                let c = below[y * bc + x];
                                b = (b.0.min(c.0), b.1.max(c.1));
                            }
                        }
                        v[j * count + i] = b;
                    }
                }
            }
            out.push(v);
        }
        out
    }
}

fn decode<T: pyreal_assets::Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

/// One node drawn this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Node {
    /// Render-space position of its south-west vertex.
    pub origin: Vec2,
    pub level: u32,
    /// Where its vertices start morphing to the next level, and where they have.
    pub morph: (f32, f32),
}

impl Node {
    pub fn write(&self, out: &mut Vec<u8>) {
        #[allow(clippy::cast_precision_loss)]
        let spacing = TEXEL * (1u32 << self.level) as f32;
        for v in [self.origin.x, self.origin.y, spacing] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.level.to_le_bytes());
        for v in [self.morph.0, self.morph.1, 0.0, 0.0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
}

/// The distance out to which each level is drawn: `first`, doubling per level.
#[must_use]
pub fn ranges(first: f32, levels: u32) -> Vec<f32> {
    #[allow(clippy::cast_precision_loss)]
    (0..levels).map(|k| first * (1u32 << k) as f32).collect()
}

/// The levels the far terrain may draw land `d` metres from an eye within `slack` metres of
/// where it is measured from, with the first level drawn to `first`: those its range picks, and the
/// next coarser one its vertices morph toward.
#[must_use]
pub fn levels_near(first: f32, d: f32, slack: f32) -> std::ops::RangeInclusive<u32> {
    let level = |d: f32| {
        let mut k = 0;
        while k < 16 && d >= first * (1u32 << k) as f32 {
            k += 1;
        }
        k
    };
    level((d - slack).max(0.0))..=level(d + slack) + 1
}

/// The nodes to draw for an eye at `eye` with frustum `planes`: a level-`k` node is drawn whole
/// unless the sphere of the next finer level's range reaches it, when its four children are
/// chosen instead. Nodes outside the frustum or wholly off the field are left out.
#[must_use]
pub fn select(
    field: &FarField,
    bounds: &[Vec<(f32, f32)>],
    ranges: &[f32],
    eye: Vec3,
    planes: &[Vec4],
) -> Vec<Node> {
    let top = u32::try_from(bounds.len().saturating_sub(1)).unwrap_or(0);
    let n = field.size as usize;
    let mut out = Vec::new();
    let mut stack = Vec::new();
    let roots = n.div_ceil((PATCH as usize) << top).max(1);
    for j in 0..roots {
        for i in 0..roots {
            stack.push((top, i, j));
        }
    }
    while let Some((level, i, j)) = stack.pop() {
        let span = (PATCH as usize) << level;
        if i * span >= field.extent as usize || j * span >= field.extent as usize {
            continue;
        }
        let count = n.div_ceil(span).max(1);
        let Some(&(lo, hi)) = bounds[level as usize].get(j * count + i) else {
            continue;
        };
        #[allow(clippy::cast_precision_loss)]
        let min = field.origin + Vec2::new((i * span) as f32, (j * span) as f32) * TEXEL;
        #[allow(clippy::cast_precision_loss)]
        let max = min + Vec2::splat(span as f32 * TEXEL);
        let (bmin, bmax) = (min.extend(lo), max.extend(hi));
        let outside = planes.iter().any(|p| {
            let far = Vec3::select(p.xyz().cmpge(Vec3::ZERO), bmax, bmin);
            p.xyz().dot(far) + p.w < 0.0
        });
        // Wholly inside the near square: the exact cells draw it.
        let covered = min.cmpge(field.near_min).all() && max.cmple(field.near_max).all();
        if outside || covered || out.len() >= MAX_NODES {
            continue;
        }
        let d = (eye.clamp(bmin, bmax) - eye).length();
        if level > 0 && d < ranges[level as usize - 1] {
            for (di, dj) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                stack.push((level - 1, i * 2 + di, j * 2 + dj));
            }
            continue;
        }
        let end = ranges[level as usize];
        out.push(Node {
            origin: min,
            level,
            morph: (end * 0.7, end),
        });
    }
    out
}

const SHADER: &str = include_str!("shaders/far.wgsl");

/// The far terrain's WGSL.
#[must_use]
pub fn source() -> String {
    format!(
        "{}\n{}\n{SHADER}",
        include_str!("shaders/common.wgsl"),
        include_str!("shaders/land.wgsl")
    )
}

/// The far shader's parameters: the field's origin, spacing and size, the near square, the
/// ground's material, and the water records' side.
fn params_bytes(field: &FarField, terrain_material: u32) -> Vec<u8> {
    let mut params = Vec::with_capacity(48);
    #[allow(clippy::cast_precision_loss)]
    for v in [
        field.origin.x,
        field.origin.y,
        TEXEL,
        field.size as f32,
        field.near_min.x,
        field.near_min.y,
        field.near_max.x,
        field.near_max.y,
    ] {
        params.extend_from_slice(&v.to_le_bytes());
    }
    for v in [
        terrain_material,
        u32::from(field.smooth),
        field.water_side,
        0,
    ] {
        params.extend_from_slice(&v.to_le_bytes());
    }
    params
}

/// The far terrain on the GPU: its textures, its patch, this frame's nodes and its pipeline.
#[derive(Debug)]
pub struct FarTerrain {
    pub field_origin: Vec2,
    field: FarField,
    bounds: Vec<Vec<(f32, f32)>>,
    ranges: Vec<f32>,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    nodes: wgpu::Buffer,
    node_count: u32,
    params: wgpu::Buffer,
    terrain_material: u32,
    group: wgpu::BindGroup,
    /// Its group's layout and its pipelines, made once and handed on to the next far terrain.
    pipes: std::sync::Arc<FarPipelines>,
    /// The water's records ([`FarField::water`]), which the frame's group takes.
    water: wgpu::TextureView,
    /// Tells this far terrain from any other, so the frame's group is bound to its water once.
    pub id: u64,
    // Kept alive with the bind group.
    _textures: [wgpu::Texture; 2],
}

/// The far terrain's group layout and pipelines: the same for every far terrain drawn with one
/// frame layout.
#[derive(Debug)]
pub struct FarPipelines {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    /// The same vertices into depth alone, for the ground's prepass.
    depth_pipeline: wgpu::RenderPipeline,
}

impl FarPipelines {
    /// The layout and pipelines, with `frame_layout` as group 0.
    #[must_use]
    pub fn new(gpu: &Gpu, frame_layout: &wgpu::BindGroupLayout) -> Self {
        let device = &gpu.device;
        use wgpu::ShaderStages as S;
        let tex_entry = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: S::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("far"),
            entries: &[
                tex_entry(0, wgpu::TextureSampleType::Float { filterable: false }),
                tex_entry(1, wgpu::TextureSampleType::Float { filterable: true }),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: S::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: S::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: S::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let module = gpu.shader("far", &source());
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("far terrain"),
            bind_group_layouts: &[Some(frame_layout), Some(&layout)],
            immediate_size: 0,
        });
        let gbuffer = [
            Some(ALBEDO.into()),
            Some(NORMAL.into()),
            Some(MATERIAL.into()),
            Some(MOTION.into()),
            Some(IDS.into()),
            Some(IDS.into()),
        ];
        let make = |fs: &str, targets: &[Option<wgpu::ColorTargetState>]| {
            gpu.render_pipeline(wgpu::RenderPipelineDescriptor {
                label: Some("far terrain"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_far"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 8,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = make("fs_far", &gbuffer);
        let depth_pipeline = make("fs_far_depth", &[]);
        Self {
            layout,
            pipeline,
            depth_pipeline,
        }
    }
}

/// The next far terrain's [`FarTerrain::id`].
static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Levels of the quadtree: enough that one root node spans the field.
#[must_use]
pub fn levels_for(size: u32) -> u32 {
    let mut l = 1;
    while (PATCH << (l - 1)) < size {
        l += 1;
    }
    l
}

impl FarTerrain {
    /// Puts `field` on the GPU, drawn into the G-buffer with `frame_layout` as group 0; its
    /// finest level is drawn out to `first_range` metres. `pipes` are the last far terrain's
    /// ([`Self::pipelines`]), made anew when `None`.
    #[must_use]
    pub fn new(
        gpu: &Gpu,
        frame_layout: &wgpu::BindGroupLayout,
        field: FarField,
        first_range: f32,
        terrain_material: u32,
        pipes: Option<std::sync::Arc<FarPipelines>>,
    ) -> Self {
        use wgpu::util::DeviceExt;
        let device = &gpu.device;
        let levels = levels_for(field.size);
        let mips = field.size.trailing_zeros() + 1;
        let texture = |label, format, mip_level_count| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: field.size,
                    height: field.size,
                    depth_or_array_layers: 1,
                },
                mip_level_count,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let heights = texture("far heights", wgpu::TextureFormat::R32Float, mips);
        let colours = texture("far colours", wgpu::TextureFormat::Rgba8UnormSrgb, mips);
        let write = |t: &wgpu::Texture, level: u32, size: u32, bytes: &[u8], texel: u32| {
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: t,
                    mip_level: level,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size * texel),
                    rows_per_image: Some(size),
                },
                wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
            );
        };
        let mut rgba: Vec<u8> = field.colours.iter().flatten().copied().collect();
        let mut size = field.size;
        for level in 0..mips {
            let (s, h) = field.height_level(level);
            let bytes: Vec<u8> = h.iter().flat_map(|v| v.to_le_bytes()).collect();
            write(&heights, level, s, &bytes, 4);
            write(&colours, level, size, &rgba, 4);
            // The next colour level: a box filter (in the bytes, near enough this far away).
            let next = (size / 2).max(1);
            let mut down = Vec::with_capacity((next * next * 4) as usize);
            for y in 0..next {
                for x in 0..next {
                    for c in 0..4 {
                        let mut sum = 0u32;
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            let (sx, sy) = ((x * 2 + dx).min(size - 1), (y * 2 + dy).min(size - 1));
                            sum += u32::from(rgba[((sy * size + sx) * 4 + c) as usize]);
                        }
                        down.push(u8::try_from(sum / 4).unwrap_or(u8::MAX));
                    }
                }
            }
            rgba = down;
            size = next;
        }
        let side = field.water_side.max(1);
        let water = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("far water"),
            size: wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        if field.water_side > 0 {
            write(&water, 0, side, &field.water, 16);
        } else {
            write(&water, 0, 1, &[0u8; 16], 16);
        }
        let mut grid = Vec::new();
        for y in 0..=PATCH {
            for x in 0..=PATCH {
                #[allow(clippy::cast_precision_loss)]
                for v in [x as f32, y as f32] {
                    grid.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        let mut idx: Vec<u8> = Vec::new();
        let row = PATCH + 1;
        for y in 0..PATCH {
            for x in 0..PATCH {
                let a = y * row + x;
                for i in [a, a + 1, a + row + 1, a, a + row + 1, a + row] {
                    idx.extend_from_slice(&i.to_le_bytes());
                }
            }
        }
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("far patch"),
            contents: &grid,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("far patch indices"),
            contents: &idx,
            usage: wgpu::BufferUsages::INDEX,
        });
        let nodes = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("far nodes"),
            size: MAX_NODES as u64 * NODE_SIZE,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("far params"),
            contents: &params_bytes(&field, terrain_material),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("far colours"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let pipes =
            pipes.unwrap_or_else(|| std::sync::Arc::new(FarPipelines::new(gpu, frame_layout)));
        let hv = heights.create_view(&Default::default());
        let cv = colours.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("far"),
            layout: &pipes.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&hv),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&cv),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: nodes.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: params.as_entire_binding(),
                },
            ],
        });
        let bounds = field.bounds(levels);
        Self {
            field_origin: field.origin,
            field,
            bounds,
            ranges: ranges(first_range, levels),
            vertices,
            indices,
            index_count: PATCH * PATCH * 6,
            nodes,
            node_count: 0,
            params,
            terrain_material,
            group,
            pipes,
            water: water.create_view(&Default::default()),
            id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            _textures: [heights, colours],
        }
    }

    /// Its group's layout and pipelines, for the next far terrain.
    #[must_use]
    pub fn pipelines(&self) -> std::sync::Arc<FarPipelines> {
        std::sync::Arc::clone(&self.pipes)
    }

    /// Whether any of its field is water.
    #[must_use]
    pub fn has_water(&self) -> bool {
        self.field.water_side > 0
    }

    /// The water's records and the parameters that place them (`FarParams`), for the frame's group.
    #[must_use]
    pub fn water_bindings(&self) -> (&wgpu::TextureView, &wgpu::Buffer) {
        (&self.water, &self.params)
    }

    /// Moves the field by `delta` (render space's origin moved the other way).
    pub fn translate(&mut self, gpu: &Gpu, delta: Vec3) {
        let d = delta.truncate();
        self.field.origin += d;
        self.field.near_min += d;
        self.field.near_max += d;
        self.field_origin = self.field.origin;
        gpu.queue.write_buffer(
            &self.params,
            0,
            &params_bytes(&self.field, self.terrain_material),
        );
    }

    /// Chooses this frame's nodes for an eye at `eye` seeing through `planes`, and writes them.
    pub fn update(&mut self, gpu: &Gpu, eye: Vec3, planes: &[Vec4]) {
        let nodes = select(&self.field, &self.bounds, &self.ranges, eye, planes);
        let mut bytes = Vec::with_capacity(nodes.len() * 32);
        for n in &nodes {
            n.write(&mut bytes);
        }
        if !bytes.is_empty() {
            gpu.queue.write_buffer(&self.nodes, 0, &bytes);
        }
        self.node_count = u32::try_from(nodes.len()).unwrap_or(0);
    }

    /// The heights and colours it draws.
    #[must_use]
    pub fn field(&self) -> &FarField {
        &self.field
    }

    /// Nodes drawn this frame, and their triangles.
    #[must_use]
    pub fn drawn(&self) -> (u32, u32) {
        (self.node_count, self.node_count * PATCH * PATCH * 2)
    }

    /// Draws this frame's nodes into the G-buffer pass, `frame` bound as group 0.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        self.draw_with(pass, frame, &self.pipes.pipeline);
    }

    /// Draws this frame's nodes into depth alone, for the ground's prepass.
    pub fn draw_depth(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        self.draw_with(pass, frame, &self.pipes.depth_pipeline);
    }

    fn draw_with(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        frame: &wgpu::BindGroup,
        pipeline: &wgpu::RenderPipeline,
    ) {
        if self.node_count == 0 {
            return;
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, frame, &[]);
        pass.set_bind_group(1, &self.group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..self.node_count);
    }
}
