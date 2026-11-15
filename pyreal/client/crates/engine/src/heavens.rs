//! The heavens from the region's sky objects: where the moons stand and what they look like, and
//! the cloud deck, at a time of day.
//!
//! Retail's sky is a handful of objects per day group: a dome, a night layer, the moons and the
//! sun on flat cards, and a cloud deck whose picture is laid flat from above and drifts with the
//! game's clock. Each object's graphics is looked at once ([`Heavens::load`]): a flat
//! four-cornered picture is a card (additive alone, the sun; otherwise a moon, its face over any
//! added glow), a picture laid evenly over the ground's axes is a deck. At a moment
//! ([`Heavens::at`]) the day group's sky positions (`pyreal_world_render::sky::get_sky`) turn the
//! cards and say how see-through each object is.
//! The physical sky replaces the dome and the night layer; the sun's disc is drawn where its light
//! comes from. A pack's moon (`[[moon]]`, by the retail moon's graphics id) is drawn as a lit ball
//! with its own pictures in place of the dat's card.

use std::collections::HashMap;

use glam::{Vec2, Vec3};
use pyreal_assets::{Decode, GfxObj, Region, Surface};
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::{DataId, Frame};

use crate::asset::material::{flags, NO_TEXTURE};
use crate::asset::texture::TextureSource;
use crate::world::AssetLoader;
use crate::Engine;

/// A flat four-cornered picture in its object's frame: its centre, the steps across it for one
/// unit of its picture's u and v, the picture's (u, v) at the centre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Card {
    pub center: Vec3,
    pub u: Vec3,
    pub v: Vec3,
    pub uv: Vec2,
}

/// A picture laid flat from above, `u = a + b x` and `v = c + d y` over metres, and how high
/// it lies at its highest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Deck {
    pub laying: [f32; 4],
    pub height: f32,
}

/// What one sky object's graphics is to the heavens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// A card: the sun's when additive (unless a pack's moon), else a moon's; `ball` the
    /// centre and radius of the disc its picture paints, in its object's frame.
    Card {
        card: Card,
        additive: bool,
        texture: u32,
        ball: Option<(Vec3, f32)>,
    },
    /// A cloud deck.
    Deck { deck: Deck, texture: u32 },
    /// The dome, the night layer, a weather curtain: the physical sky stands for them.
    Other,
}

/// A pack's moon: its surface, night lights' and clouds' texture slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackMoon {
    pub surface: u32,
    pub lights: u32,
    pub clouds: u32,
}

/// A moon at a moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoonNow {
    /// Its sky object's graphics id, which names the moon.
    pub gfx: u32,
    /// Toward the card's centre.
    pub dir: Vec3,
    /// The steps across the picture for one unit of u and of v, at unit distance along `dir`.
    pub u: Vec3,
    pub v: Vec3,
    /// The picture's (u, v) at the card's corner where q = 0 (the centre less half a step).
    pub uv0: Vec2,
    /// The dat picture's slot, or the pack's surface.
    pub texture: u32,
    /// The pack's night lights, or [`NO_TEXTURE`].
    pub lights: u32,
    /// The pack's clouds over its ground, or [`NO_TEXTURE`].
    pub clouds: u32,
    /// Drawn as a pack ball.
    pub ball: bool,
    /// How much of it shows, 0 to 1.
    pub opacity: f32,
}

/// The cloud deck at a moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeckNow {
    pub deck: Deck,
    pub texture: u32,
    /// The picture's drift with the clock, picture units, and its speed, picture units a second.
    pub scroll: Vec2,
    pub velocity: Vec2,
    pub opacity: f32,
    /// How bright, from the dats' luminosity and brightness ramps (1 plain).
    pub brightness: f32,
}

/// The heavens at a moment.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeavensNow {
    pub moons: Vec<MoonNow>,
    pub deck: Option<DeckNow>,
    /// The time of day, 0 to 1, which turns the stars.
    pub time: f32,
}

/// The region's sky objects, looked at once.
#[derive(Debug, Default)]
pub struct Heavens {
    // ORDER-OK: looked up by graphics id, never iterated.
    shapes: HashMap<u32, Shape>,
    // ORDER-OK: looked up by graphics id, never iterated.
    pack_moons: HashMap<u32, PackMoon>,
    /// Each day group's objects: (graphics ids it can show, texture velocity).
    groups: Vec<pyreal_assets::region::SkyDayPreset>,
}

fn decode<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

/// `points` as one flat four-cornered card, when they are triangles over four corners lying in
/// one plane (both faces of a two-sided card among them).
#[must_use]
pub fn card_of(points: &[(Vec3, Vec2)]) -> Option<Card> {
    if points.is_empty() || !points.len().is_multiple_of(3) || points.len() > 12 {
        return None;
    }
    let mut corners: Vec<(Vec3, Vec2)> = Vec::with_capacity(4);
    for p in points {
        if !corners.iter().any(|q| q.0.distance_squared(p.0) < 1e-6) {
            corners.push(*p);
        }
    }
    if corners.len() != 4 {
        return None;
    }
    let center = corners.iter().map(|c| c.0).sum::<Vec3>() * 0.25;
    let uv = corners.iter().map(|c| c.1).sum::<Vec2>() * 0.25;
    let (mut suu, mut svv, mut suv) = (0.0f32, 0.0f32, 0.0f32);
    let (mut pu, mut pv) = (Vec3::ZERO, Vec3::ZERO);
    for c in &corners {
        let d = c.1 - uv;
        let off = c.0 - center;
        suu += d.x * d.x;
        svv += d.y * d.y;
        suv += d.x * d.y;
        pu += off * d.x;
        pv += off * d.y;
    }
    let det = suu * svv - suv * suv;
    if det.abs() < 1e-9 {
        return None;
    }
    let u = pu * (svv / det) - pv * (suv / det);
    let v = pv * (suu / det) - pu * (suv / det);
    let flat = corners.iter().all(|c| {
        let at = center + u * (c.1.x - uv.x) + v * (c.1.y - uv.y);
        at.distance_squared(c.0) < 1e-2
    });
    flat.then_some(Card { center, u, v, uv })
}

/// The disc a picture paints, `width` by `height` with `alpha` a byte a texel (rows from the
/// top): its centre and half its extent each way, in picture units.
#[must_use]
pub fn disc_of(width: u32, height: u32, alpha: &[u8]) -> Option<(Vec2, Vec2)> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || alpha.len() < w * h {
        return None;
    }
    let (mut lo, mut hi) = ((w, h), (0, 0));
    for (i, &a) in alpha[..w * h].iter().enumerate() {
        if a >= 128 {
            let (x, y) = (i % w, i / w);
            lo = (lo.0.min(x), lo.1.min(y));
            hi = (hi.0.max(x + 1), hi.1.max(y + 1));
        }
    }
    if hi.0 <= lo.0 || hi.1 <= lo.1 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let (a, b, size) = (
        Vec2::new(lo.0 as f32, lo.1 as f32),
        Vec2::new(hi.0 as f32, hi.1 as f32),
        Vec2::new(w as f32, h as f32),
    );
    Some(((a + b) * 0.5 / size, (b - a) * 0.5 / size))
}

/// The flat laying `[a, b, c, d]` of `points`' pictures, `u = a + b x`, `v = c + d y`, when every
/// point lies on it to a thousandth of a unit and both steps are real.
#[must_use]
pub fn laying_of(points: &[(Vec3, Vec2)]) -> Option<[f32; 4]> {
    let fit = |pts: &[(f32, f32)]| -> Option<(f32, f32)> {
        #[allow(clippy::cast_precision_loss)]
        let n = pts.len() as f32;
        let mx = pts.iter().map(|p| p.0).sum::<f32>() / n;
        let my = pts.iter().map(|p| p.1).sum::<f32>() / n;
        let sxx: f32 = pts.iter().map(|p| (p.0 - mx) * (p.0 - mx)).sum();
        let sxy: f32 = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
        if sxx < 1.0 {
            return None;
        }
        let b = sxy / sxx;
        let a = my - b * mx;
        let worst = pts
            .iter()
            .map(|p| (a + b * p.0 - p.1).abs())
            .fold(0.0, f32::max);
        (worst < 1e-3 && b.abs() > 1e-7).then_some((a, b))
    };
    if points.len() < 3 {
        return None;
    }
    let xs: Vec<(f32, f32)> = points.iter().map(|p| (p.0.x, p.1.x)).collect();
    let ys: Vec<(f32, f32)> = points.iter().map(|p| (p.0.y, p.1.y)).collect();
    let (a, b) = fit(&xs)?;
    let (c, d) = fit(&ys)?;
    Some([a, b, c, d])
}

/// The shape of graphics object `id`, its picture uploaded.
fn shape_of(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    id: DataId,
) -> Shape {
    let Some(obj) = decode::<GfxObj>(store, DbType::GfxObj, id) else {
        return Shape::Other;
    };
    let mesh = crate::asset::mesh::gfxobj_mesh(&obj);
    let mut deck = None;
    let mut card = None;
    for sub in &mesh.submeshes {
        let Some(surface) = sub.surface else { continue };
        let first = sub.first_index as usize;
        let points: Vec<(Vec3, Vec2)> = mesh.indices[first..first + sub.index_count as usize]
            .iter()
            .filter_map(|&i| mesh.vertices.get(i as usize))
            .map(|v| (Vec3::from(v.position), Vec2::from(v.uv)))
            .collect();
        let Some((material, mflags)) = loader.material(engine, store, surface, false) else {
            continue;
        };
        let texture = engine
            .materials
            .material(material)
            .map_or(NO_TEXTURE, |r| r.textures[0]);
        if texture == NO_TEXTURE {
            continue;
        }
        // The sky's pictures are drawn by no instance: kept from eviction.
        engine.materials.pin_material(material);
        engine.materials.pin_texture(texture);
        if deck.is_none() {
            if let Some(laying) = laying_of(&points) {
                let height = points.iter().map(|p| p.0.z).fold(f32::MIN, f32::max);
                deck = Some(Shape::Deck {
                    deck: Deck { laying, height },
                    texture,
                });
            }
        }
        // A moon's face over its added glow is the face: the glow alone is the sun's.
        let glow = matches!(card, Some(Shape::Card { additive: true, .. }));
        if card.is_none() || (glow && mflags & flags::ADDITIVE == 0) {
            card = card_of(&points)
                .map(|c| Shape::Card {
                    card: c,
                    additive: mflags & flags::ADDITIVE != 0,
                    texture,
                    ball: ball_of(store, surface, &c),
                })
                .or(card);
        }
    }
    card.or(deck).unwrap_or(Shape::Other)
}

/// The ball the disc on `card`'s picture (that of `surface`) stands for: its centre and radius.
fn ball_of(store: &RetailDatStore, surface: DataId, card: &Card) -> Option<(Vec3, f32)> {
    let s = decode::<Surface>(store, DbType::Surface, surface)?;
    let (w, h, alpha) = TextureSource::new(store)
        .alpha(s.orig_texture_id?, true)
        .ok()?;
    let (at, half) = disc_of(w, h, &alpha)?;
    let center = card.center + card.u * (at.x - card.uv.x) + card.v * (at.y - card.uv.y);
    // The card stretches the disc unevenly; the ball takes its mean width.
    let radius = 0.5 * (half.x * card.u.length() + half.y * card.v.length());
    Some((center, radius))
}

impl Heavens {
    /// Looks at every graphics object `region`'s day groups can show, and the pack's moons.
    pub fn load(
        engine: &mut Engine,
        loader: &mut AssetLoader,
        store: &RetailDatStore,
        region: &Region,
    ) -> Self {
        let mut h = Self::default();
        let Some(sky) = region.sky_info.as_ref() else {
            return h;
        };
        h.groups.clone_from(&sky.day_groups);
        let mut ids: Vec<DataId> = Vec::new();
        for g in &sky.day_groups {
            ids.extend(g.sky_objects.iter().map(|o| o.default_gfx_object));
            for t in &g.sky_time {
                ids.extend(t.sky_obj_replace.iter().map(|r| r.gfx_obj_id));
            }
        }
        ids.sort_unstable_by_key(|d| d.0);
        ids.dedup();
        for id in ids.into_iter().filter(|d| d.0 != 0) {
            let shape = shape_of(engine, loader, store, id);
            h.shapes.insert(id.0, shape);
        }
        if let Some(pack) = loader.pack() {
            let moons: Vec<_> = pack.pack.moons.clone();
            for m in moons {
                let mut slot = |image: &pyreal_art_pack::images::Image| {
                    let s = crate::pack::image_pixels(image, true)
                        .and_then(|px| engine.materials.add_texture(&engine.gpu, &px).ok());
                    if let Some(s) = s {
                        engine.materials.pin_texture(s);
                    }
                    s
                };
                if let Some(surface) = slot(&m.surface) {
                    let lights = m.lights.as_ref().and_then(&mut slot).unwrap_or(NO_TEXTURE);
                    let clouds = m.clouds.as_ref().and_then(&mut slot).unwrap_or(NO_TEXTURE);
                    h.pack_moons.insert(
                        m.object.0,
                        PackMoon {
                            surface,
                            lights,
                            clouds,
                        },
                    );
                }
            }
        }
        h
    }

    /// The shape found for graphics `id`.
    #[must_use]
    pub fn shape(&self, id: DataId) -> Option<Shape> {
        self.shapes.get(&id.0).copied()
    }

    /// How many objects are cards, decks and neither, and how many pack moons there are.
    #[must_use]
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let mut c = (0, 0, 0, self.pack_moons.len());
        for s in self.shapes.values() {
            match s {
                Shape::Card { .. } => c.0 += 1,
                Shape::Deck { .. } => c.1 += 1,
                Shape::Other => c.2 += 1,
            }
        }
        c
    }

    /// The moons and the deck of day group `group` at `time` (0 to 1), the game clock `clock`
    /// (seconds) drifting the deck.
    #[must_use]
    pub fn at(&self, group: usize, time: f32, clock: f64) -> HeavensNow {
        let mut now = HeavensNow {
            time,
            ..HeavensNow::default()
        };
        let Some(g) = self.groups.get(group).or_else(|| self.groups.first()) else {
            return now;
        };
        for p in pyreal_world_render::sky::get_sky(g, time.rem_euclid(1.0)) {
            if p.gfx_id.0 == 0 || p.properties & 4 != 0 {
                continue;
            }
            let shown = if p.transparent >= 0.0 {
                (1.0 - p.transparent * 0.01).clamp(0.0, 1.0)
            } else {
                1.0
            };
            if shown <= 0.0 {
                continue;
            }
            match self.shape(p.gfx_id) {
                // An additive card is the sun's, unless the pack draws it as a moon.
                Some(Shape::Card {
                    card,
                    additive,
                    texture,
                    ball,
                }) if !additive || self.pack_moons.contains_key(&p.gfx_id.0) => {
                    let mut f = Frame::default();
                    pyreal_world_render::sky::calc_frame(&mut f, p.heading, p.rotation);
                    let m = pyreal_primitives::frame::l2g(f.rotation);
                    let turn = |v: Vec3| {
                        let r = pyreal_primitives::frame::localtoglobalvec(
                            m,
                            pyreal_primitives::Vec3::new(v.x, v.y, v.z),
                        );
                        Vec3::new(r.x, r.y, r.z)
                    };
                    let center = turn(card.center);
                    let dist = center.length();
                    if dist <= 1e-3 {
                        continue;
                    }
                    let pack = self.pack_moons.get(&p.gfx_id.0);
                    let mut dir = center / dist;
                    let (mut u, mut v) = (turn(card.u) / dist, turn(card.v) / dist);
                    // A pack's ball: round, as wide as the disc the dat's picture paints.
                    if let Some((at, r)) = ball.filter(|_| pack.is_some()) {
                        let c = turn(at);
                        let d = c.length().max(1e-3);
                        dir = c / d;
                        let across = (u - dir * u.dot(dir)).normalize_or_zero();
                        let down =
                            (v - dir * v.dot(dir) - across * v.dot(across)).normalize_or_zero();
                        u = across * (2.0 * r / d);
                        v = down * (2.0 * r / d);
                    }
                    now.moons.push(MoonNow {
                        gfx: p.gfx_id.0,
                        dir,
                        u,
                        v,
                        uv0: card.uv,
                        texture: pack.map_or(texture, |m| m.surface),
                        lights: pack.map_or(NO_TEXTURE, |m| m.lights),
                        clouds: pack.map_or(NO_TEXTURE, |m| m.clouds),
                        ball: pack.is_some(),
                        opacity: shown,
                    });
                }
                Some(Shape::Deck { deck, texture }) if now.deck.is_none() => {
                    let period = 1.0e6;
                    #[allow(clippy::cast_possible_truncation)]
                    let t = (clock % period) as f32;
                    let scroll = Vec2::new(
                        (p.tex_velocity.0 * t).rem_euclid(1.0),
                        (p.tex_velocity.1 * t).rem_euclid(1.0),
                    );
                    let bright = if p.luminosity > 0.0 {
                        (p.luminosity * 0.01).clamp(0.2, 1.5)
                    } else {
                        1.0
                    };
                    now.deck = Some(DeckNow {
                        deck,
                        texture,
                        scroll,
                        velocity: Vec2::new(p.tex_velocity.0, p.tex_velocity.1),
                        opacity: shown,
                        brightness: bright,
                    });
                }
                _ => {}
            }
        }
        now.moons.truncate(2);
        now
    }
}
