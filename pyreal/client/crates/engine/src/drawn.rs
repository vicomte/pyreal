//! Landscape Smoothing: retail's surface plus the visual offset of
//! [`pyreal_terrain::land::drawn`] (retail's facets rounded, within a metre of them, rock bands on
//! steep ground, kept at retail's height at buildings, statics and water and partly on roads), at
//! a strength of Low, Medium or High. Physics, picking and the server stay on retail's triangles.
//!
//! The planners work out each block's offset ([`block_field`]); the ground is meshed [`SUB`]
//! quads a cell and drawn at it ([`crate::terrain::build_block`]), the scenery is placed moved by
//! it ([`scenery_lift`]), and the land field carries it for the cover, the bodies and the camera
//! ([`crate::land::LandField::lift`]).

use std::collections::HashMap;

use glam::{Vec2, Vec3};
use pyreal_assets::{CellLandblock, Decode, GfxObj, LandblockInfo};
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::{DataId, Frame};
use pyreal_terrain::land::drawn::{
    self, BlockField, GroundSource, Knob, Knobs, Pin, Vertex, BLOCK_METERS, SIDE,
};

use crate::asset::object::{object_affine, object_parts};

/// Quads along a land cell's side the drawn ground is meshed with: 2 m apart.
pub const SUB: usize = 12;
/// Metres between the drawn ground's vertices.
#[allow(clippy::cast_precision_loss)]
pub const STEP: f32 = 24.0 / SUB as f32;
/// A block's drawn vertices along a side, both edges included.
pub const BLOCK_SIDE: usize = 8 * SUB + 1;
/// The height above a model's base its footprint is taken from: what meets the ground.
const FOOT: f32 = 2.5;

/// The 3 by 3 records round a block as the offset's landscape, heights through `table`.
#[derive(Debug)]
pub struct Around<'a> {
    pub records: &'a [((i32, i32), Option<CellLandblock>)],
    pub table: &'a [f32; 256],
}

impl GroundSource for Around<'_> {
    fn vertex(&self, gx: i32, gy: i32) -> Option<Vertex> {
        let look = |b: (i32, i32), x: i32, y: i32| {
            let (_, r) = self.records.iter().find(|(k, _)| *k == b)?;
            let r = r.as_ref()?;
            let i = usize::try_from(x * 9 + y).ok()?;
            Some(Vertex {
                height: self.table[usize::from(r.height[i])],
                word: r.terrain[i],
            })
        };
        let (b, v) = (
            (gx.div_euclid(8), gy.div_euclid(8)),
            (gx.rem_euclid(8), gy.rem_euclid(8)),
        );
        look(b, v.0, v.1)
            .or_else(|| (v.0 == 0).then(|| look((b.0 - 1, b.1), 8, v.1)).flatten())
            .or_else(|| (v.1 == 0).then(|| look((b.0, b.1 - 1), v.0, 8)).flatten())
    }
}

fn decode<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

/// A model's footprint in its own frame: the box, east and north, of its vertices within
/// [`FOOT`] of its base; `None` for one with none there.
#[must_use]
pub fn footprint(store: &RetailDatStore, id: DataId) -> Option<[f32; 4]> {
    let mut boxed: Option<[f32; 4]> = None;
    for p in object_parts(store, id) {
        let Some(g) = decode::<GfxObj>(store, DbType::GfxObj, p.gfxobj) else {
            continue;
        };
        for v in &g.vertex_array.vertices {
            let q =
                p.transform
                    .transform_point3(Vec3::new(v.position.x, v.position.y, v.position.z));
            if q.z > FOOT {
                continue;
            }
            let b = boxed.get_or_insert([q.x, q.y, q.x, q.y]);
            *b = [b[0].min(q.x), b[1].min(q.y), b[2].max(q.x), b[3].max(q.y)];
        }
    }
    boxed
}

/// Footprint `fp` of an object at `frame` in block `block`, with `margin` round it, as global
/// ground kept at retail's height.
fn pin_of(fp: [f32; 4], frame: &Frame, block: (i32, i32), margin: f32) -> Pin {
    let at = object_affine(frame, 1.0);
    let middle = at.transform_point3(Vec3::new((fp[0] + fp[2]) * 0.5, (fp[1] + fp[3]) * 0.5, 0.0));
    let axis = Vec2::new(at.x_axis.x, at.x_axis.y).normalize_or(Vec2::X);
    #[allow(clippy::cast_precision_loss)]
    let corner = Vec2::new(block.0 as f32, block.1 as f32) * BLOCK_METERS;
    Pin {
        x: corner.x + middle.x,
        y: corner.y + middle.y,
        half: ((fp[2] - fp[0]) * 0.5, (fp[3] - fp[1]) * 0.5),
        axis: (axis.x, axis.y),
        margin,
    }
}

/// The pins of the buildings and statics of `infos` (block `block` and its neighbours) that reach
/// it within a fade, each model's footprint looked up in `memo`.
pub fn pins(
    store: &RetailDatStore,
    block: (i32, i32),
    infos: &[((i32, i32), Option<LandblockInfo>)],
    memo: &mut HashMap<u32, Option<[f32; 4]>>,
    knobs: &Knobs,
) -> Vec<Pin> {
    let reach = knobs.get(Knob::PinFade) + 48.0;
    #[allow(clippy::cast_precision_loss)]
    let (lo, hi) = (
        Vec2::new(block.0 as f32, block.1 as f32) * BLOCK_METERS - reach,
        Vec2::new((block.0 + 1) as f32, (block.1 + 1) as f32) * BLOCK_METERS + reach,
    );
    let mut out = Vec::new();
    for (b, info) in infos {
        let Some(info) = info else {
            continue;
        };
        let objects = info
            .buildings
            .iter()
            .map(|o| (o.id, &o.frame, knobs.get(Knob::PinBuildingMargin)))
            .chain(
                info.objects
                    .iter()
                    .map(|o| (o.id, &o.frame, knobs.get(Knob::PinStaticMargin))),
            );
        for (id, frame, margin) in objects {
            let Some(fp) = *memo.entry(id.0).or_insert_with(|| footprint(store, id)) else {
                continue;
            };
            let pin = pin_of(fp, frame, *b, margin);
            let r = pin.reach();
            if pin.x + r >= lo.x && pin.x - r <= hi.x && pin.y + r >= lo.y && pin.y - r <= hi.y {
                out.push(pin);
            }
        }
    }
    out
}

/// Block `block`'s offset from the records round it, its `pins` and the `knobs`.
#[must_use]
pub fn block_field(src: &Around<'_>, block: (i32, i32), pins: &[Pin], knobs: &Knobs) -> BlockField {
    drawn::block_field(src, block, pins, knobs)
}

/// The offset at block-local metre sample `(x, y)` (0 to 192 each).
#[must_use]
pub fn at_sample(field: &BlockField, x: usize, y: usize) -> (f32, f32) {
    let k = y.min(SIDE - 1) * SIDE + x.min(SIDE - 1);
    (
        field.offset.get(k).copied().unwrap_or(0.0),
        field.weight.get(k).copied().unwrap_or(0.0),
    )
}

/// A block's offset at its drawn vertices, [`BLOCK_SIDE`] a side, row by row from the south.
#[must_use]
pub fn vertex_offsets(field: &BlockField) -> Vec<f32> {
    let mut out = Vec::with_capacity(BLOCK_SIDE * BLOCK_SIDE);
    for v in 0..BLOCK_SIDE {
        for u in 0..BLOCK_SIDE {
            out.push(at_sample(field, u * (24 / SUB), v * (24 / SUB)).0);
        }
    }
    out
}

/// How much of the offset a thing `above` metres over retail's ground keeps: all of it on the
/// ground, none from the knobs' height up.
#[must_use]
pub fn keep(knobs: &Knobs, above: f32) -> f32 {
    let (lo, hi) = (knobs.get(Knob::ShiftFade), knobs.get(Knob::ShiftGone));
    let t = ((above - lo) / (hi - lo).max(1e-3)).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// How far a piece of scenery of block `block` at block-local `at` is drawn above where it
/// stands: the offset averaged over its stance, faded as it stands higher over retail's ground,
/// never under the drawn ground there.
#[must_use]
pub fn scenery_lift(
    field: &BlockField,
    src: &Around<'_>,
    block: (i32, i32),
    at: Vec3,
    knobs: &Knobs,
) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let corner = Vec2::new(block.0 as f32, block.1 as f32) * BLOCK_METERS;
    let retail = drawn::retail_height(src, corner.x + at.x, corner.y + at.y);
    let k = keep(knobs, at.z - retail);
    if k <= 0.0 {
        return 0.0;
    }
    let near = drawn::body_offset(|x, y| drawn::sample(&field.offset, x, y), at.x, at.y, knobs);
    let ground = drawn::Drawn {
        offset: near,
        height: retail + drawn::sample(&field.offset, at.x, at.y),
    };
    drawn::shift(at.z, ground, k)
}
