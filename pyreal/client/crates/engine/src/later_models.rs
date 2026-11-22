//! End of Retail's remodelled props beside an older world. A prop in [`MODELS`] is drawn from End
//! of Retail's files, its parts, surfaces and pictures all theirs, where it stands and is turned
//! as the older world places it (`docs/ERAS.md`).
//!
//! The list is the developer's review of the props End of Retail remodelled under the same id
//! (`notes/review/era-props/`, rounds 1 to 3, 2026-11-20). A prop marked tinted takes the older model's
//! colours under Classic Ground Coloring: End of Retail's pictures are carried, channel by
//! channel, to the older pictures' mean and spread ([`colour_fit`]).
//!
//! The Bael'Zharon statues (setups 0x02000359, 0x020007C3, 0x02000A53 and 0x02000A54) are
//! listed by the developer's call of 2026-11-20 that every era draws End of Retail's: the same
//! model with its redrawn face. A server places them, so they are swapped part by part
//! (`AssetLoader::later_part`).

use std::collections::HashMap;
use std::sync::Arc;

use glam::Vec3;

use pyreal_assets::{Decode, GfxObj, Surface};
use pyreal_codecs::surface::surface_type as st;
use pyreal_dat::{ContainerEra, DbType, RetailDatStore};
use pyreal_primitives::DataId;

use crate::asset::object::object_parts;
use crate::asset::texture::TextureSource;
use crate::flames::FlameSource;
use crate::later_rooms::Fit;

/// (prop, tinted): the setups and graphics objects an older world draws as End of Retail has
/// them, sorted. A graphics object here is one the world also places by itself.
pub const MODELS: &[(u32, bool)] = &[
    (0x0100_0007, false),
    (0x0100_0075, false),
    (0x0100_007B, false),
    (0x0100_02CE, false),
    (0x0100_0345, false),
    (0x0100_058F, false),
    (0x0100_05F5, false),
    (0x0100_0677, false),
    (0x0100_0692, true),
    (0x0100_0695, true),
    (0x0100_06E0, false),
    (0x0100_0A2F, false),
    (0x0100_0ADA, false),
    (0x0100_0BB2, false),
    (0x0100_1B29, false),
    (0x0100_1B2A, false),
    (0x0100_1B2B, false),
    (0x0100_1BB2, false),
    (0x0100_2D1C, false),
    (0x0100_2EF1, false),
    (0x0100_2EF2, false),
    (0x0100_2EF6, false),
    (0x0100_31AF, false),
    (0x0100_34FA, false),
    (0x0200_0040, false),
    (0x0200_0041, false),
    (0x0200_007C, false),
    (0x0200_00F2, false),
    (0x0200_0109, true),
    (0x0200_010A, true),
    (0x0200_0144, false),
    (0x0200_0165, false),
    (0x0200_0180, false),
    (0x0200_01B4, false),
    (0x0200_0200, false),
    (0x0200_0201, false),
    (0x0200_0222, false),
    (0x0200_0224, false),
    (0x0200_0276, false),
    (0x0200_02FA, false),
    (0x0200_0359, false),
    (0x0200_035F, true),
    (0x0200_039C, false),
    (0x0200_039D, false),
    (0x0200_03B1, false),
    (0x0200_03B2, false),
    (0x0200_03CC, false),
    (0x0200_03CD, false),
    (0x0200_03D3, false),
    (0x0200_03DE, true),
    (0x0200_0469, false),
    (0x0200_0486, false),
    (0x0200_04DD, false),
    (0x0200_0608, false),
    (0x0200_06C1, true),
    (0x0200_06FD, false),
    (0x0200_06FE, false),
    (0x0200_07C3, false),
    (0x0200_07E4, false),
    (0x0200_086E, false),
    (0x0200_091B, false),
    (0x0200_09B4, false),
    (0x0200_09B5, false),
    (0x0200_0A02, false),
    (0x0200_0A23, false),
    (0x0200_0A53, false),
    (0x0200_0A54, false),
    (0x0200_0BA5, false),
    (0x0200_0EE8, false),
    (0x0200_0F9F, false),
    (0x0200_0FA0, false),
    (0x0200_0FF7, false),
    (0x0200_105B, false),
    (0x0200_1065, false),
    (0x0200_1149, false),
    (0x0200_1250, false),
];

/// Whether `id` is one of [`MODELS`], and then whether it is drawn tinted.
#[must_use]
pub fn listed(id: DataId) -> Option<bool> {
    MODELS
        .binary_search_by_key(&id.0, |m| m.0)
        .ok()
        .map(|i| MODELS[i].1)
}

/// How far a tint may scale a channel's spread.
const GAIN_RANGE: (f32, f32) = (0.25, 4.0);

/// A picture set's colour: each channel's mean and spread over its shown texels, in parts of one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colour {
    pub mean: [f32; 3],
    pub spread: [f32; 3],
}

/// The colour of texels of blue, green, red and alpha, those at least half opaque; `None` when
/// none is.
#[must_use]
pub fn colour_of(bgra: &[u8]) -> Option<Colour> {
    let (mut sum, mut sq, mut n) = ([0f64; 3], [0f64; 3], 0f64);
    for p in bgra.as_chunks::<4>().0 {
        if p[3] < 128 {
            continue;
        }
        for (c, v) in [p[2], p[1], p[0]].into_iter().enumerate() {
            let v = f64::from(v) / 255.0;
            sum[c] += v;
            sq[c] += v * v;
        }
        n += 1.0;
    }
    if n == 0.0 {
        return None;
    }
    // LINT-OK: colour levels in parts of one.
    #[allow(clippy::cast_possible_truncation)]
    let f = |c: usize| {
        let m = sum[c] / n;
        (m as f32, (sq[c] / n - m * m).max(0.0).sqrt() as f32)
    };
    Some(Colour {
        mean: [0, 1, 2].map(|c| f(c).0),
        spread: [0, 1, 2].map(|c| f(c).1),
    })
}

/// The tint that carries `later`'s colour to `older`'s: each channel scaled to the older spread
/// and moved to the older mean.
#[must_use]
pub fn colour_fit(older: Colour, later: Colour) -> Fit {
    let mut fit = Fit {
        gain: [1.0; 3],
        offset: [0.0; 3],
    };
    for c in 0..3 {
        let g = if later.spread[c] > 1e-3 {
            (older.spread[c] / later.spread[c]).clamp(GAIN_RANGE.0, GAIN_RANGE.1)
        } else {
            1.0
        };
        fit.gain[c] = g;
        fit.offset[c] = older.mean[c] - g * later.mean[c];
    }
    fit
}

/// Colours pooled by weight: their texels taken together, each colour's counted `weight` times.
#[must_use]
pub fn pooled(parts: &[(Colour, f32)]) -> Option<Colour> {
    let total: f32 = parts.iter().map(|p| p.1).sum();
    if total <= 0.0 {
        return None;
    }
    let mut mean = [0f32; 3];
    let mut square = [0f32; 3];
    for (c, w) in parts {
        for k in 0..3 {
            mean[k] += c.mean[k] * w / total;
            square[k] += (c.spread[k] * c.spread[k] + c.mean[k] * c.mean[k]) * w / total;
        }
    }
    Some(Colour {
        mean,
        spread: [0, 1, 2].map(|k| (square[k] - mean[k] * mean[k]).max(0.0).sqrt()),
    })
}

fn decode<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

/// A picture and its clip-map keying.
type Picture = (DataId, bool);

/// One face of a prop at rest: its middle, its area and its picture.
struct Face {
    at: Vec3,
    area: f32,
    picture: Picture,
}

/// The faces prop `id` draws in `store` with a picture.
fn faces(store: &RetailDatStore, id: DataId) -> Vec<Face> {
    let mut out = Vec::new();
    for part in object_parts(store, id) {
        let Some(g) = decode::<GfxObj>(store, DbType::GfxObj, part.gfxobj) else {
            continue;
        };
        let slots: Vec<Option<Picture>> = g
            .surfaces
            .iter()
            .map(|&s| {
                let rec = decode::<Surface>(store, DbType::Surface, s)?;
                let ty = rec.surface_type;
                (ty & (st::BASE1_IMAGE | st::BASE1_CLIPMAP) != 0)
                    .then_some((rec.orig_texture_id?, ty & st::BASE1_CLIPMAP != 0))
            })
            .collect();
        for p in &g.polygons {
            let Some(Some(picture)) = slots.get(usize::from(p.pos_surface)).copied() else {
                continue;
            };
            let pts: Vec<Vec3> = p
                .vertex_ids
                .iter()
                .filter_map(|&v| g.vertex_array.vertices.get(usize::from(v)))
                .map(|v| {
                    part.transform.transform_point3(Vec3::new(
                        v.position.x,
                        v.position.y,
                        v.position.z,
                    ))
                })
                .collect();
            if pts.len() < 3 {
                continue;
            }
            let area: f32 = (1..pts.len() - 1)
                .map(|k| (pts[k] - pts[0]).cross(pts[k + 1] - pts[0]).length() * 0.5)
                .sum();
            // LINT-OK: a polygon has a few dozen corners at most.
            #[allow(clippy::cast_precision_loss)]
            let at = pts.iter().copied().sum::<Vec3>() / pts.len() as f32;
            out.push(Face { at, area, picture });
        }
    }
    out
}

/// For each picture of `later`, the pictures of `older` over the same places, each by the area
/// of `later`'s faces whose nearest face of `older` it is drawn on.
fn pairs(older: &[Face], later: &[Face]) -> Vec<(Picture, Vec<(Picture, f32)>)> {
    let mut out: Vec<(Picture, Vec<(Picture, f32)>)> = Vec::new();
    for f in later {
        let Some(near) = older.iter().min_by(|a, b| {
            a.at.distance_squared(f.at)
                .total_cmp(&b.at.distance_squared(f.at))
        }) else {
            continue;
        };
        let i = match out.iter().position(|e| e.0 == f.picture) {
            Some(i) => i,
            None => {
                out.push((f.picture, Vec::new()));
                out.len() - 1
            }
        };
        match out[i].1.iter_mut().find(|e| e.0 == near.picture) {
            Some(e) => e.1 += f.area,
            None => out[i].1.push((near.picture, f.area)),
        }
    }
    out
}

/// The colour of one picture in `store`, every shown texel counted.
fn colour_in(store: &RetailDatStore, (t, clip): Picture) -> Option<Colour> {
    let src = TextureSource::new(store);
    let px = if src.is_palettised(t) {
        src.decode(t, clip, None)
            .ok()
            .and_then(|p| p.levels.into_iter().next())
    } else {
        src.bgra(t).ok().map(|b| b.2)
    }?;
    colour_of(&px)
}

/// End of Retail's models beside an older world.
#[derive(Debug)]
pub struct LaterModels {
    store: RetailDatStore,
    // ORDER-OK: memos, only looked up.
    fits: HashMap<u32, Arc<HashMap<u32, Fit>>>,
    colours: HashMap<(bool, u32, bool), Option<Colour>>,
    flames: HashMap<u32, Vec<FlameSource>>,
}

impl LaterModels {
    /// `None` on a later world, or on an older one without End of Retail's portal beside it.
    #[must_use]
    pub fn of(world: &RetailDatStore) -> Option<Self> {
        if world.era() != ContainerEra::Classic {
            return None;
        }
        Some(Self {
            store: world.object_files(ContainerEra::Modern)?,
            fits: HashMap::new(),
            colours: HashMap::new(),
            flames: HashMap::new(),
        })
    }

    /// End of Retail's files.
    #[must_use]
    pub fn store(&self) -> &RetailDatStore {
        &self.store
    }

    /// Where prop `id` burns as End of Retail has it, drawn as the engine's flames: its fire is
    /// End of Retail's even where the older model's was a mesh.
    pub fn flames(&mut self, id: DataId) -> &[FlameSource] {
        let store = &self.store;
        self.flames
            .entry(id.0)
            .or_insert_with(|| crate::flames::examine(store, id))
    }

    fn colour(&mut self, world: &RetailDatStore, later: bool, p: Picture) -> Option<Colour> {
        let key = (later, p.0 .0, p.1);
        if let Some(c) = self.colours.get(&key) {
            return *c;
        }
        let c = colour_in(if later { &self.store } else { world }, p);
        self.colours.insert(key, c);
        c
    }

    /// The tints that carry End of Retail's prop `id` to the older world's colours, by End of
    /// Retail's picture: each picture to the older pictures drawn over the same places.
    pub fn fits(&mut self, world: &RetailDatStore, id: DataId) -> Arc<HashMap<u32, Fit>> {
        if let Some(f) = self.fits.get(&id.0) {
            return Arc::clone(f);
        }
        let paired = pairs(&faces(world, id), &faces(&self.store, id));
        let mut made = HashMap::new();
        for (picture, older) in paired {
            let olds: Vec<(Colour, f32)> = older
                .into_iter()
                .filter_map(|(p, w)| Some((self.colour(world, false, p)?, w)))
                .collect();
            let fit = pooled(&olds)
                .zip(self.colour(world, true, picture))
                .map(|(o, l)| colour_fit(o, l));
            if let Some(f) = fit {
                made.insert(picture.0 .0, f);
            }
        }
        let made = Arc::new(made);
        self.fits.insert(id.0, Arc::clone(&made));
        made
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_sorted_and_found() {
        assert!(MODELS.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(listed(DataId(0x0200_010A)), Some(true));
        assert_eq!(listed(DataId(0x0200_0180)), Some(false));
        assert_eq!(listed(DataId(0x0200_0253)), None);
    }

    #[test]
    fn a_fit_carries_one_colour_to_the_other() {
        // Two texels each: later light and spread wide, older dark and narrow; one cut out.
        let later = [200, 100, 50, 255, 100, 50, 0, 255, 0, 0, 0, 0];
        let older = [60, 60, 60, 255, 40, 40, 40, 255];
        let (o, l) = (colour_of(&older).unwrap(), colour_of(&later).unwrap());
        let fit = colour_fit(o, l);
        let mut px = later;
        fit.apply(&mut px);
        let got = colour_of(&px).unwrap();
        for c in 0..3 {
            assert!((got.mean[c] - o.mean[c]).abs() < 0.01, "{got:?} {o:?}");
        }
        assert_eq!(colour_of(&[0, 0, 0, 0]), None);
    }

    #[test]
    fn pooled_colours_count_by_weight() {
        let a = Colour {
            mean: [0.2; 3],
            spread: [0.0; 3],
        };
        let b = Colour {
            mean: [0.6; 3],
            spread: [0.0; 3],
        };
        let p = pooled(&[(a, 1.0), (b, 3.0)]).unwrap();
        assert!((p.mean[0] - 0.5).abs() < 1e-5);
        // A quarter of the texels at 0.2 and the rest at 0.6 spread about 0.173.
        assert!((p.spread[0] - 0.1732).abs() < 1e-3, "{p:?}");
        assert_eq!(pooled(&[]), None);
    }
}
