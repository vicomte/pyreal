//! Cobwebs as fine silk. Of each dat cobweb card ([`COBWEBS`], by picture) only where it lies is
//! kept: its plane, the hull its picture covers and the edges of that hull that run along the
//! card's (where the web holds). A web is spun to fit ([`weave`]), seeded by its placement: an orb
//! web (spokes, a capture spiral round a bare hub, frame and bridge threads) for the corner webs,
//! and for the drapes sagging festoons with cross-threads, loose ends and torn small webs; every
//! thread sags a little. `webs.wgsl` widens the threads to a pixel and stirs them: a slow drift,
//! and a billow where a body moves near ([`Stir`]), settling over a second or two. A faint gauze
//! lies at an orb's hub and in a drape's festoons. The card stays in the mesh, drawn while the webs
//! are off.

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::sync::Arc;

use glam::{Vec2, Vec3};
use pyreal_primitives::num::math::{cosf, expf, sinf};
use pyreal_primitives::num::{floor_to_i32, to_i32};
use pyreal_primitives::DataId;

use crate::asset::mesh::{oct_encode_snorm16, MeshData, Sides, SubMesh, Vertex};

/// The cobweb pictures: the drape of sagging ropes, the two corner webs, the drape of small
/// webs, and the 2005 files' own corner webs and small webs (`docs/ERAS.md`).
pub const COBWEBS: [u32; 5] = [ROPES, ORB, SMALL, ORB_2005, SMALL_2005];
const ROPES: u32 = 0x0500_0D14;
const ORB: u32 = 0x0500_2CDF;
const SMALL: u32 = 0x0500_2CE0;
const ORB_2005: u32 = 0x0500_1F4D;
const SMALL_2005: u32 = 0x0500_1F4E;

/// Whether picture `texture` is a cobweb card's.
#[must_use]
pub fn is_cobweb(texture: DataId) -> bool {
    COBWEBS.contains(&texture.0)
}

/// How far from where it holds the silk hangs wholly free, metres.
const HANG: f32 = 0.6;
/// The spacing of a thread's points, metres.
const STEP: f32 = 0.03;
/// A main thread's width and a fine one's, metres.
const THREAD: f32 = 0.0014;
const FINE: f32 = 0.0007;
/// The width a vertex carries is in these units, metres.
const WIDTH_UNIT: f32 = 1.0e-5;
/// The alpha samples across a card each way that find the hull its picture covers.
const SAMPLES: usize = 32;

/// A cobweb picture's alpha: a byte a texel, rows from the top.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub alpha: Vec<u8>,
}

impl Picture {
    /// The alpha at picture coordinate `q`, the picture repeating.
    #[allow(clippy::cast_precision_loss)]
    fn at(&self, q: Vec2) -> u8 {
        let wrap = |v: f32, n: usize| {
            let n = i32::try_from(n).unwrap_or(1).max(1);
            usize::try_from(floor_to_i32(v * n as f32).rem_euclid(n)).unwrap_or(0)
        };
        let (x, y) = (wrap(q.x, self.width), wrap(q.y, self.height));
        self.alpha.get(y * self.width + x).copied().unwrap_or(0)
    }
}

/// One of a card's triangles, in its local frame and picture coordinates.
struct Tri {
    p: [Vec3; 3],
    uv: [Vec2; 3],
    n: Vec3,
}

impl Tri {
    /// The picture coordinate at local point `p` (on the triangle's plane), when it holds it.
    fn uv_at(&self, p: Vec3) -> Option<Vec2> {
        let (e1, e2) = (self.p[1] - self.p[0], self.p[2] - self.p[0]);
        let r = p - self.p[0];
        let (d00, d01, d11) = (e1.dot(e1), e1.dot(e2), e2.dot(e2));
        let (d20, d21) = (r.dot(e1), r.dot(e2));
        let det = d00 * d11 - d01 * d01;
        if det.abs() < 1e-12 {
            return None;
        }
        let b1 = (d11 * d20 - d01 * d21) / det;
        let b2 = (d00 * d21 - d01 * d20) / det;
        let b0 = 1.0 - b1 - b2;
        const EDGE: f32 = -1e-3;
        (b0 >= EDGE && b1 >= EDGE && b2 >= EDGE)
            .then(|| self.uv[0] * b0 + self.uv[1] * b1 + self.uv[2] * b2)
    }
}

fn hash(mut h: u32) -> u32 {
    h = h.wrapping_mul(0x9E37_79B9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 13)
}

/// A hash as a share, 0 to 1.
#[allow(clippy::cast_precision_loss)]
fn share(h: u32) -> f32 {
    (h >> 8) as f32 / 16_777_216.0
}

/// A seeded run of shares.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = hash(self.0.wrapping_add(0x6D2B_79F5));
        share(self.0)
    }

    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.next()
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn count(&mut self, a: usize, b: usize) -> usize {
        a + ((self.next() * (b + 1 - a) as f32) as usize).min(b - a)
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The convex hull of `pts`, anticlockwise.
fn hull(pts: &[Vec2]) -> Vec<Vec2> {
    let mut p = pts.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup_by(|a, b| a.distance_squared(*b) < 1e-10);
    if p.len() < 3 {
        return p;
    }
    let mut out: Vec<Vec2> = Vec::with_capacity(p.len() * 2);
    for pass in 0..2 {
        let start = out.len();
        for &q in &p {
            while out.len() >= start + 2 {
                let (a, b) = (out[out.len() - 2], out[out.len() - 1]);
                if (b - a).perp_dot(q - a) > 1e-9 {
                    break;
                }
                out.pop();
            }
            out.push(q);
        }
        out.pop();
        if pass == 0 {
            p.reverse();
        }
    }
    out
}

/// `poly` without the corners it barely turns at.
fn straighten(mut poly: Vec<Vec2>) -> Vec<Vec2> {
    let mut i = 0;
    while poly.len() > 3 && i < poly.len() {
        let n = poly.len();
        let (a, b, c) = (poly[(i + n - 1) % n], poly[i], poly[(i + 1) % n]);
        if (b - a)
            .normalize_or_zero()
            .perp_dot((c - b).normalize_or_zero())
            .abs()
            < 0.15
        {
            poly.remove(i);
        } else {
            i += 1;
        }
    }
    poly
}

/// How far `q` lies from segment `a`..`b`.
fn to_segment(q: Vec2, a: Vec2, b: Vec2) -> f32 {
    let d = b - a;
    let t = ((q - a).dot(d) / d.length_squared().max(1e-12)).clamp(0.0, 1.0);
    q.distance(a + d * t)
}

/// Where the ray from `c` (inside convex `poly`) along `d` leaves it: how far, the edge and how
/// far along it.
fn leave(poly: &[Vec2], c: Vec2, d: Vec2) -> Option<(f32, usize, f32)> {
    let n = poly.len();
    let mut best: Option<(f32, usize, f32)> = None;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let e = b - a;
        let den = d.perp_dot(e);
        if den.abs() < 1e-9 {
            continue;
        }
        let t = (a - c).perp_dot(e) / den;
        let s = (a - c).perp_dot(d) / den;
        if t > 1e-5 && (-1e-4..=1.0 + 1e-4).contains(&s) && best.is_none_or(|x| t < x.0) {
            best = Some((t, i, s.clamp(0.0, 1.0)));
        }
    }
    best
}

/// A card's plane, the hull its picture covers there and which of that hull's edges hold.
struct Plane {
    origin: Vec3,
    e1: Vec3,
    e2: Vec3,
    normal: Vec3,
    hull: Vec<Vec2>,
    holds: Vec<bool>,
    /// Down the picture, which hangs its webs.
    down: Vec2,
}

impl Plane {
    fn lift(&self, q: Vec2) -> Vec3 {
        self.origin + self.e1 * q.x + self.e2 * q.y
    }

    /// How far `q` hangs from where the web holds, metres.
    fn hang(&self, q: Vec2) -> f32 {
        let n = self.hull.len();
        let held = (0..n)
            .filter(|&i| self.holds[i])
            .map(|i| to_segment(q, self.hull[i], self.hull[(i + 1) % n]))
            .fold(f32::INFINITY, f32::min);
        if held.is_finite() {
            held
        } else {
            self.hull.iter().map(|h| h.distance(q)).fold(HANG, f32::min)
        }
    }

    fn centre(&self) -> Vec2 {
        #[allow(clippy::cast_precision_loss)]
        let n = self.hull.len().max(1) as f32;
        self.hull.iter().copied().sum::<Vec2>() / n
    }
}

/// The webs on `tris`' planes (cards facing alike and in line are one): each patch the picture
/// covers there, by its hull.
fn planes(tris: &[Tri], picture: &Picture) -> Vec<Plane> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (k, t) in tris.iter().enumerate() {
        let found = groups.iter_mut().find(|g| {
            let f = &tris[g[0]];
            t.n.dot(f.n).abs() > 0.98 && (t.p[0] - f.p[0]).dot(f.n).abs() < 0.02
        });
        match found {
            Some(g) => g.push(k),
            None => groups.push(vec![k]),
        }
    }
    let mut out = Vec::new();
    for g in groups {
        let first = &tris[g[0]];
        let normal = first.n;
        let e1 = (first.p[1] - first.p[0]).normalize_or(Vec3::X);
        let e2 = normal.cross(e1).normalize_or(Vec3::Y);
        let origin = first.p[0];
        let flat = |p: Vec3| Vec2::new((p - origin).dot(e1), (p - origin).dot(e2));
        let corners: Vec<Vec2> = g.iter().flat_map(|&k| tris[k].p.map(flat)).collect();
        let card = hull(&corners);
        if card.len() < 3 {
            continue;
        }
        let lo = card.iter().fold(Vec2::splat(f32::MAX), |a, b| a.min(*b));
        let hi = card.iter().fold(Vec2::splat(f32::MIN), |a, b| a.max(*b));
        #[allow(clippy::cast_precision_loss)]
        let cell = (hi - lo) / SAMPLES as f32;
        let mut covered = vec![false; SAMPLES * SAMPLES];
        #[allow(clippy::cast_precision_loss)]
        let place = |i: usize| {
            lo + cell * Vec2::new((i % SAMPLES) as f32 + 0.5, (i / SAMPLES) as f32 + 0.5)
        };
        for (i, c) in covered.iter_mut().enumerate() {
            let q = place(i);
            let p = origin + e1 * q.x + e2 * q.y;
            *c = g
                .iter()
                .find_map(|&k| tris[k].uv_at(p))
                .is_some_and(|uv| picture.at(uv) >= 96);
        }
        let near = cell.length() * 1.2;
        let rim = |q: Vec2| {
            (0..card.len())
                .map(|i| to_segment(q, card[i], card[(i + 1) % card.len()]))
                .fold(f32::INFINITY, f32::min)
                < near
        };
        // Each patch of the picture apart from the rest (its strands joined across a few
        // samples) is a web of its own.
        let joined: Vec<bool> = (0..SAMPLES * SAMPLES)
            .map(|i| {
                let (x, y) = (i % SAMPLES, i / SAMPLES);
                (y.saturating_sub(2)..(y + 3).min(SAMPLES)).any(|cy| {
                    (x.saturating_sub(2)..(x + 3).min(SAMPLES)).any(|cx| covered[cy * SAMPLES + cx])
                })
            })
            .collect();
        let mut seen = vec![false; SAMPLES * SAMPLES];
        for start in 0..SAMPLES * SAMPLES {
            if !covered[start] || seen[start] {
                continue;
            }
            let mut patch = Vec::new();
            let mut stack = vec![start];
            seen[start] = true;
            while let Some(i) = stack.pop() {
                if covered[i] {
                    patch.push(place(i));
                }
                let (x, y) = (i % SAMPLES, i / SAMPLES);
                for ny in y.saturating_sub(1)..(y + 2).min(SAMPLES) {
                    for nx in x.saturating_sub(1)..(x + 2).min(SAMPLES) {
                        let j = ny * SAMPLES + nx;
                        if joined[j] && !seen[j] {
                            seen[j] = true;
                            stack.push(j);
                        }
                    }
                }
            }
            if patch.len() < 6 {
                continue;
            }
            let web = straighten(hull(&patch));
            if web.len() < 3 {
                continue;
            }
            let n = web.len();
            let holds = (0..n)
                .map(|i| {
                    let (a, b) = (web[i], web[(i + 1) % n]);
                    rim(a) && rim(b) && rim((a + b) * 0.5)
                })
                .collect();
            // Down the picture: the way its second coordinate grows.
            #[allow(clippy::cast_precision_loss)]
            let mid = web.iter().copied().sum::<Vec2>() / web.len() as f32;
            let v_at = |q: Vec2| {
                let p = origin + e1 * q.x + e2 * q.y;
                tris.iter().find_map(|t| t.uv_at(p)).map(|uv| uv.y)
            };
            let down = match (
                v_at(mid),
                v_at(mid + Vec2::X * 0.01),
                v_at(mid + Vec2::Y * 0.01),
            ) {
                (Some(v), Some(x), Some(y)) => Vec2::new(x - v, y - v).normalize_or(Vec2::NEG_Y),
                _ => Vec2::NEG_Y,
            };
            out.push(Plane {
                origin,
                e1,
                e2,
                normal,
                hull: web,
                holds,
                down,
            });
        }
    }
    out
}

/// A thread spun on a plane: its local points, how freely each hangs (0 to 1), its width, and
/// whether it is a rope of two twisted threads.
struct Thread {
    points: Vec<Vec3>,
    free: Vec<f32>,
    width: f32,
    rope: bool,
}

/// A web's threads and its gauze (triangles of points with how much of it shows and how freely
/// they hang).
#[derive(Default)]
struct Web {
    threads: Vec<Thread>,
    gauze: Vec<[(Vec3, f32, f32); 3]>,
}

impl Web {
    /// A thread along `curve` (plane points, `t` from 0 to 1) sagging `sag` metres down the
    /// picture at its middle.
    fn spin(&mut self, plane: &Plane, curve: impl Fn(f32) -> Vec2, sag: f32, width: f32) {
        self.spin_rope(plane, curve, sag, width, false);
    }

    fn spin_rope(
        &mut self,
        plane: &Plane,
        curve: impl Fn(f32) -> Vec2,
        sag: f32,
        width: f32,
        rope: bool,
    ) {
        let mut length = 0.0;
        let mut last = curve(0.0);
        for k in 1..=16 {
            #[allow(clippy::cast_precision_loss)]
            let q = curve(k as f32 / 16.0);
            length += q.distance(last);
            last = q;
        }
        if length < 0.01 {
            return;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = ((length / STEP).ceil() as usize).max(2);
        let mut points = Vec::with_capacity(n + 1);
        let mut free = Vec::with_capacity(n + 1);
        for k in 0..=n {
            #[allow(clippy::cast_precision_loss)]
            let t = k as f32 / n as f32;
            let q = curve(t) + plane.down * (sag * 4.0 * t * (1.0 - t));
            points.push(plane.lift(q));
            free.push(smoothstep(0.0, HANG, plane.hang(q)));
        }
        self.threads.push(Thread {
            points,
            free,
            width,
            rope,
        });
    }

    /// A straight thread from `a` to `b` sagging by `slack` of its length.
    fn tie(&mut self, plane: &Plane, a: Vec2, b: Vec2, slack: f32, width: f32) {
        self.spin(plane, |t| a.lerp(b, t), a.distance(b) * slack, width);
    }

    fn sheet(&mut self, plane: &Plane, tri: [(Vec2, f32); 3]) {
        self.gauze
            .push(tri.map(|(q, fade)| (plane.lift(q), fade, smoothstep(0.0, HANG, plane.hang(q)))));
    }
}

/// An orb web on `plane`: a hub, spokes out to the frame, a capture spiral with a bare zone round
/// the hub, frame threads along the hull's free edges and a bridge or two to where it holds.
fn orb(plane: &Plane, rng: &mut Rng, web: &mut Web) {
    let poly = &plane.hull;
    let n = poly.len();
    let centre = plane.centre();
    let inner = (0..n)
        .map(|i| to_segment(centre, poly[i], poly[(i + 1) % n]))
        .fold(f32::INFINITY, f32::min);
    let hub = centre + Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)) * (inner * 0.25);
    // Free edges are frame threads, bowed in toward the hub.
    let bow = |i: usize, s: f32| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let on = a.lerp(b, s);
        if plane.holds[i] {
            return on;
        }
        let inward = (hub - on).normalize_or_zero();
        on + inward * (a.distance(b) * 0.05 * 4.0 * s * (1.0 - s))
    };
    let spokes = rng.count(12, 24);
    let turn = rng.range(0.0, TAU);
    let mut ends = Vec::with_capacity(spokes);
    for k in 0..spokes {
        #[allow(clippy::cast_precision_loss)]
        let a = turn + TAU * (k as f32 + rng.range(-0.25, 0.25)) / spokes as f32;
        let d = Vec2::new(cosf(a), sinf(a));
        let Some((_, edge, s)) = leave(poly, hub, d) else {
            continue;
        };
        let end = bow(edge, s);
        ends.push((d, hub.distance(end), end));
    }
    if ends.len() < 6 {
        return;
    }
    for &(_, _, end) in &ends {
        web.tie(plane, hub, end, 0.008, THREAD * rng.range(0.8, 1.2));
    }
    for i in 0..n {
        if !plane.holds[i] {
            web.spin(plane, |t| bow(i, t), 0.0, THREAD * 1.2);
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let mean = ends.iter().map(|e| e.1).sum::<f32>() / ends.len() as f32;
    let least = ends.iter().map(|e| e.1).fold(f32::INFINITY, f32::min);
    let gap = least * rng.range(0.16, 0.26);
    let spacing = (mean * rng.range(0.08, 0.11)).clamp(0.03, 0.09);
    let turns = ((0.9 * mean - gap) / spacing).clamp(2.0, 18.0);
    #[allow(clippy::cast_precision_loss)]
    let steps = (turns * ends.len() as f32).ceil();
    // The capture spiral, chord by chord between the spokes, broken here and there.
    let at = |j: usize, jitter: f32| {
        let (d, r, _) = ends[j % ends.len()];
        #[allow(clippy::cast_precision_loss)]
        let f = j as f32 / steps;
        hub + d * ((gap + (0.9 * r - gap) * f) + jitter)
    };
    let mut prev = at(0, 0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    for j in 1..steps as usize {
        let next = at(j, rng.range(-0.15, 0.15) * spacing);
        if rng.next() > 0.04 {
            web.tie(plane, prev, next, 0.04, FINE * rng.range(0.9, 1.3));
        }
        prev = next;
    }
    // The hub: a few tight, uneven turns, and a faint sheet over it.
    let hub_turns = rng.range(2.0, 3.5);
    let r_hub = gap * 0.55;
    let lap = |t: f32| {
        let a = turn + t * hub_turns * TAU;
        let r = r_hub * (0.15 + 0.85 * t);
        hub + Vec2::new(cosf(a), sinf(a)) * r
    };
    web.spin(plane, lap, 0.0, FINE);
    for k in 0..12 {
        #[allow(clippy::cast_precision_loss)]
        let (a0, a1) = (TAU * k as f32 / 12.0, TAU * (k + 1) as f32 / 12.0);
        let rim = gap * 1.2;
        web.sheet(
            plane,
            [
                (hub, 1.0),
                (hub + Vec2::new(cosf(a0), sinf(a0)) * rim, 0.0),
                (hub + Vec2::new(cosf(a1), sinf(a1)) * rim, 0.0),
            ],
        );
    }
    // A bridge or two from a corner to where the web holds.
    let held: Vec<usize> = (0..n).filter(|&i| plane.holds[i]).collect();
    for _ in 0..rng.count(1, 2) {
        if held.is_empty() {
            break;
        }
        let corner = poly[rng.count(0, n - 1)];
        let i = held[rng.count(0, held.len() - 1)];
        let to = poly[i].lerp(poly[(i + 1) % n], rng.range(0.2, 0.8));
        if corner.distance(to) > 0.05 {
            web.tie(plane, corner, to, 0.05, THREAD);
        }
    }
}

/// A drape on `plane`: festoons hung from its top edge, sagging down the card, tied by fine
/// cross-threads, with loose ends hanging and (`small`) torn small webs in the festoons.
fn drape(plane: &Plane, small: bool, rng: &mut Rng, web: &mut Web) {
    let poly = &plane.hull;
    let n = poly.len();
    let centre = plane.centre();
    // The top: the edge highest up the picture, across it, where the web holds if it can.
    let up = |i: usize| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let across = 1.0 - ((b - a).normalize_or_zero().dot(plane.down)).abs();
        -(a + b).dot(plane.down) + across + if plane.holds[i] { 1.0 } else { 0.0 }
    };
    let Some(top) = (0..n).max_by(|&i, &j| up(i).total_cmp(&up(j))) else {
        return;
    };
    let (a, b) = (poly[top], poly[(top + 1) % n]);
    let side = (b - a).perp().normalize_or_zero();
    let down = if side.dot(centre - a) >= 0.0 {
        side
    } else {
        -side
    };
    let depth = |q: Vec2| leave(poly, q + down * 1e-3, down).map_or(0.0, |x| x.0);
    let count = if small {
        rng.count(3, 5)
    } else {
        rng.count(5, 7)
    };
    let mut festoons: Vec<(Vec2, Vec2, f32)> = Vec::new();
    for k in 0..count {
        // Spread along the top, overlapping.
        #[allow(clippy::cast_precision_loss)]
        let (k, count) = (k as f32, count as f32);
        let span = (rng.range(1.5, 3.0) / count).min(1.0);
        let middle = (k + 0.5 + rng.range(-0.3, 0.3)) / count;
        let s0 = (middle - span * 0.5).clamp(0.0, 1.0 - span);
        let (p, q) = (a.lerp(b, s0), a.lerp(b, s0 + span));
        // As deep as the card lets it, everywhere along.
        let room = (1..10)
            .map(|k| {
                #[allow(clippy::cast_precision_loss)]
                let t = k as f32 / 10.0;
                depth(p.lerp(q, t)) / (4.0 * t * (1.0 - t))
            })
            .fold(f32::INFINITY, f32::min);
        let sag = room * rng.range(0.2, 0.75);
        festoons.push((p, q, sag));
    }
    let along =
        |f: (Vec2, Vec2, f32), t: f32| f.0.lerp(f.1, t) + down * (f.2 * 4.0 * t * (1.0 - t));
    for &f in &festoons {
        let (width, rope) = if small {
            (THREAD * rng.range(0.9, 1.2), false)
        } else {
            (THREAD * rng.range(1.2, 1.6), true)
        };
        web.spin_rope(
            plane,
            |t| along(f, t),
            f.0.distance(f.1) * 0.01,
            width,
            rope,
        );
        // A faint sheet in some festoons, thickest between the thread and its chord.
        if rng.next() < 0.25 {
            let row = |t: f32, r: usize| {
                #[allow(clippy::cast_precision_loss)]
                let up = r as f32 * 0.25;
                let fade = if r == 1 {
                    0.6 * 4.0 * t * (1.0 - t)
                } else {
                    0.0
                };
                (along(f, t).lerp(f.0.lerp(f.1, t), up), fade)
            };
            for k in 0..8 {
                #[allow(clippy::cast_precision_loss)]
                let (t0, t1) = (k as f32 / 8.0, (k + 1) as f32 / 8.0);
                for r in 0..2 {
                    web.sheet(plane, [row(t0, r), row(t1, r), row(t0, r + 1)]);
                    web.sheet(plane, [row(t1, r), row(t1, r + 1), row(t0, r + 1)]);
                }
            }
        }
    }
    let inside =
        |q: Vec2| (0..n).all(|i| (poly[(i + 1) % n] - poly[i]).perp_dot(q - poly[i]) >= -1e-4);
    // Fine cross-threads between neighbouring festoons.
    let ties = if small {
        rng.count(10, 16)
    } else {
        rng.count(20, 32)
    };
    for _ in 0..ties * 2 {
        if festoons.len() < 2 {
            break;
        }
        let i = rng.count(0, festoons.len() - 1);
        let j = (i + rng.count(1, festoons.len() - 1)) % festoons.len();
        let p = along(festoons[i], rng.range(0.1, 0.9));
        let q = (0..=24)
            .map(|k| {
                #[allow(clippy::cast_precision_loss)]
                let t = k as f32 / 24.0;
                along(festoons[j], t)
            })
            .min_by(|x, y| x.distance_squared(p).total_cmp(&y.distance_squared(p)))
            .unwrap_or(p);
        let d = p.distance(q);
        if (0.03..0.35).contains(&d) {
            web.tie(plane, p, q, 0.1, FINE * rng.range(0.8, 1.2));
        }
    }
    // Loose ends hanging from the festoons.
    for _ in 0..rng.count(4, 9) {
        let f = festoons[rng.count(0, festoons.len() - 1)];
        let from = along(f, rng.range(0.15, 0.85));
        let lean = rng.range(-0.35, 0.35);
        let way = down * cosf(lean) + down.perp() * sinf(lean);
        let mut length = rng.range(0.05, 0.3);
        while length > 0.03 && !inside(from + way * length) {
            length *= 0.7;
        }
        let bend = rng.range(-0.15, 0.15) * length;
        web.spin(
            plane,
            |t| from + way * (length * t) + way.perp() * (bend * t * t),
            0.0,
            FINE * 1.3,
        );
    }
    // Torn small webs: a fan of spokes under a festoon and a few broken arcs across them.
    let torn = if small {
        rng.count(2, 3)
    } else {
        rng.count(0, 1)
    };
    for _ in 0..torn {
        let f = festoons[rng.count(0, festoons.len() - 1)];
        let tc = rng.range(0.3, 0.7);
        let mut drop = rng.range(0.08, 0.18);
        let top = along(f, tc);
        while drop > 0.03 && !inside(top + down * drop) {
            drop *= 0.7;
        }
        let c = top + down * drop;
        let spokes = rng.count(4, 7);
        let ends: Vec<Vec2> = (0..spokes)
            .map(|k| {
                #[allow(clippy::cast_precision_loss)]
                let t = tc + 0.3 * (k as f32 / (spokes - 1) as f32 - 0.5);
                along(f, t.clamp(0.02, 0.98))
            })
            .collect();
        for &e in &ends {
            web.tie(plane, c, e, 0.02, FINE * 1.2);
        }
        for frac in [0.35, 0.6, 0.85] {
            for w in ends.windows(2) {
                if rng.next() < 0.75 {
                    web.tie(plane, c.lerp(w[0], frac), c.lerp(w[1], frac), 0.12, FINE);
                }
            }
        }
    }
}

/// The card `card` with a web spun to fit its sub-meshes `subs` (whose picture `picture` is
/// cobweb `kind`, seeded by `seed`), as one more sub-mesh at its end (its index returned);
/// `None` when the picture covers none of the card.
#[must_use]
pub fn weave(
    card: &MeshData,
    subs: &[usize],
    picture: &Picture,
    kind: u32,
    seed: u32,
) -> Option<(MeshData, usize)> {
    let mut tris = Vec::new();
    for &k in subs {
        let s = card.submeshes.get(k)?;
        let range = s.first_index as usize..(s.first_index + s.index_count) as usize;
        for t in card.indices.get(range)?.as_chunks::<3>().0 {
            let v = [t[0], t[1], t[2]].map(|i| card.vertices[i as usize]);
            let p = v.map(|v| Vec3::from(v.position));
            let uv = v.map(|v| Vec2::from(v.uv));
            let n = (p[1] - p[0]).cross(p[2] - p[0]);
            if n.length() < 1e-8 {
                continue;
            }
            tris.push(Tri {
                p,
                uv,
                n: n.normalize(),
            });
        }
    }
    let mut out = card.clone();
    let first_index = u32::try_from(out.indices.len()).ok()?;
    let mut far = card.radius;
    let centre = Vec3::from(card.center);
    for (k, plane) in planes(&tris, picture).iter().enumerate() {
        let mut rng = Rng(hash(seed ^ hash(u32::try_from(k).unwrap_or(0) ^ kind)));
        let mut web = Web::default();
        if matches!(kind, ORB | ORB_2005) {
            orb(plane, &mut rng, &mut web);
        } else {
            drape(
                plane,
                matches!(kind, SMALL | SMALL_2005),
                &mut rng,
                &mut web,
            );
        }
        let normal = oct_encode_snorm16(plane.normal.to_array());
        for thread in &web.threads {
            far = far.max(emit(&mut out, thread, plane.normal, &mut rng, centre)?);
        }
        for tri in &web.gauze {
            let base = u32::try_from(out.vertices.len()).ok()?;
            for &(p, fade, free) in tri {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let fade = (fade.clamp(0.0, 1.0) * 65535.0) as u32;
                out.vertices.push(Vertex {
                    position: p.to_array(),
                    normal: plane.normal.to_array(),
                    uv: [0.0, free],
                    extra: [fade << 16, normal],
                });
            }
            out.indices.extend_from_slice(&[base, base + 1, base + 2]);
        }
    }
    let index_count = u32::try_from(out.indices.len()).ok()? - first_index;
    if index_count == 0 {
        return None;
    }
    out.submeshes.push(SubMesh {
        surface_index: u16::MAX,
        surface: None,
        sides: Sides::Both,
        first_index,
        index_count,
    });
    // Room for the stir.
    out.radius = far + 0.25;
    Some((out, card.submeshes.len()))
}

/// `thread` as ribbons in `out` (a rope as two threads twisted round each other), lying on a
/// card facing `normal`; the farthest it reaches from `centre`.
fn emit(
    out: &mut MeshData,
    thread: &Thread,
    normal: Vec3,
    rng: &mut Rng,
    centre: Vec3,
) -> Option<f32> {
    let pts = &thread.points;
    let key = hash(rng.0 ^ 0xC0B);
    rng.next();
    let mut far = 0.0f32;
    for strand in 0..=u32::from(thread.rope) {
        let (turn, phase) = if strand == 0 {
            (0.0, 0.0)
        } else {
            (TAU / rng.range(0.3, 0.6), rng.range(0.0, TAU))
        };
        let width = if strand == 0 { thread.width } else { FINE };
        let mut s = 0.0;
        let base = u32::try_from(out.vertices.len()).ok()?;
        for (i, q) in pts.iter().enumerate() {
            if i > 0 {
                s += q.distance(pts[i - 1]);
            }
            #[allow(clippy::cast_precision_loss)]
            let t = i as f32 / (pts.len() - 1).max(1) as f32;
            let tangent = tangent_at(pts, i);
            let side = normal.cross(tangent).normalize_or_zero();
            let open = if strand == 0 {
                0.0
            } else {
                sinf(PI * t).max(0.0).sqrt() * 0.002
            };
            let a = s * turn + phase;
            let at = *q + (side * sinf(a) + normal * 0.6 * cosf(a)) * open;
            far = far.max(at.distance(centre));
            let extra = [
                u32::try_from(to_i32((width / WIDTH_UNIT).round()))
                    .unwrap_or(0)
                    .clamp(1, 0xFFFF)
                    | (key & 0xFFFF_0000),
                oct_encode_snorm16(normal.to_array()),
            ];
            for side in [-1.0, 1.0] {
                out.vertices.push(Vertex {
                    position: at.to_array(),
                    normal: tangent.to_array(),
                    uv: [side, thread.free[i]],
                    extra,
                });
            }
        }
        for i in 0..u32::try_from(pts.len() - 1).ok()? {
            let (a, b, c, d) = (
                base + 2 * i,
                base + 2 * i + 1,
                base + 2 * i + 2,
                base + 2 * i + 3,
            );
            out.indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    Some(far)
}

fn tangent_at(pts: &[Vec3], i: usize) -> Vec3 {
    let a = pts[i.saturating_sub(1)];
    let b = pts[(i + 1).min(pts.len() - 1)];
    (b - a).normalize_or(Vec3::X)
}

/// How many webs each cobweb mesh is spun as.
const VARIANTS: u32 = 8;

/// Which of a cobweb mesh's webs stands at `spot` (its place, quarter metres).
#[must_use]
pub fn variant(spot: [u32; 3]) -> u32 {
    hash(spot[0] ^ hash(spot[1] ^ hash(spot[2]))) % VARIANTS
}

/// The loader's cobweb state: each picture's alpha, the silk's material, and the cards'.
#[derive(Debug, Default)]
pub struct Weaver {
    // ORDER-OK: looked up by picture id, never iterated.
    pictures: HashMap<u32, Option<Arc<Picture>>>,
    /// The silk's material, once made.
    pub silk: Option<u32>,
    /// The graphics objects whose cards are spun as webs.
    // ORDER-OK: looked up by id, never iterated.
    pub woven: std::collections::HashSet<u32>,
    /// Each cobweb surface's card material (drawn only with the webs off), by surface and sides.
    // ORDER-OK: looked up by surface, never iterated in an order that matters.
    pub cards: HashMap<(u32, bool), Option<(u32, u32)>>,
}

impl Weaver {
    /// Picture `texture`'s alpha, read the first time.
    pub fn picture(
        &mut self,
        store: &pyreal_dat::RetailDatStore,
        texture: DataId,
    ) -> Option<Arc<Picture>> {
        self.pictures
            .entry(texture.0)
            .or_insert_with(|| {
                let src = crate::asset::texture::TextureSource::new(store);
                let (w, h, alpha) = src.alpha(texture, true).ok()?;
                Some(Arc::new(Picture {
                    width: w as usize,
                    height: h as usize,
                    alpha,
                }))
            })
            .clone()
    }

    /// The silk's material record.
    #[must_use]
    pub fn silk_record() -> crate::asset::material::MaterialRecord {
        use crate::asset::material::{flags, MaterialRecord, NO_TEXTURE};
        MaterialRecord {
            base_color: [0.75, 0.75, 0.76, 1.0],
            textures: [NO_TEXTURE; 6],
            flags: flags::TRANSLUCENT | flags::TWO_SIDED,
            roughness: 0.6,
            ..MaterialRecord::default()
        }
    }

    /// Drops what refers to freed materials.
    pub fn forget(&mut self, materials: &std::collections::HashSet<u32>) {
        if self.silk.is_some_and(|m| materials.contains(&m)) {
            self.silk = None;
        }
        self.cards
            .retain(|_, m| !m.is_some_and(|(m, _)| materials.contains(&m)));
    }
}

/// The most moving bodies that stir the webs at once.
pub const STIR_SLOTS: usize = 8;
/// The stir's uniform bytes: a header, each slot's place, each slot's push.
pub const STIR_SIZE: u64 = 16 + 32 * STIR_SLOTS as u64;
/// How far a body's stir reaches from its middle, metres.
const STIR_REACH: f32 = 3.0;
/// Where a body's middle stands over its feet, metres.
const MIDDLE: f32 = 1.5;
/// The silk's spring: its stiffness (per second squared), its damping (per second) and how hard
/// the air a body moves pushes it (per second).
const STIFF: f32 = 30.0;
const DAMP: f32 = 3.0;
const PUSH: f32 = 0.45;
/// The most the silk is pushed, metres.
const MOST: f32 = 0.18;

/// The most bodies followed, the nearest the eye.
const TRACKED: usize = 32;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Slot {
    at: Vec3,
    /// The body's velocity, smoothed.
    wind: Vec3,
    push: Vec3,
    speed: Vec3,
    /// Whether its body was seen this step.
    seen: bool,
}

impl Slot {
    fn energy(&self) -> f32 {
        self.push.length() + 0.1 * self.speed.length()
    }
}

/// The bodies that stir the webs: each a damped spring driven by its body's movement, which the
/// silk near it follows by how freely it hangs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Springs {
    slots: Vec<Slot>,
    last: Option<f64>,
}

/// The webs' switch and their stir, for the shader.
#[derive(Debug)]
pub struct Stir {
    /// Whether the webs are drawn as silk (Silk Cobwebs); off, the cards draw.
    pub on: bool,
    pub springs: Springs,
    buffer: wgpu::Buffer,
}

impl Stir {
    #[must_use]
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            on: true,
            springs: Springs::default(),
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("webs stir"),
                size: STIR_SIZE,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        }
    }

    #[must_use]
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// Steps the springs (as [`Springs::step`]) and writes them for the shader.
    pub fn update(&mut self, queue: &wgpu::Queue, bodies: &[Vec3], eye: Vec3, clock: f64) {
        self.springs.step(bodies, eye, clock);
        queue.write_buffer(&self.buffer, 0, &self.springs.bytes(clock));
    }
}

impl Springs {
    /// The most any spring is pushed now, metres.
    #[must_use]
    pub fn most(&self) -> f32 {
        self.slots
            .iter()
            .map(|s| s.push.length())
            .fold(0.0, f32::max)
    }

    /// Steps the springs to `clock` seconds with the bodies at `bodies` (their feet), following
    /// the nearest `eye`.
    pub fn step(&mut self, bodies: &[Vec3], eye: Vec3, clock: f64) {
        #[allow(clippy::cast_possible_truncation)]
        let dt = self
            .last
            .map_or(0.0, |l| (clock - l).clamp(0.0, 0.25) as f32);
        self.last = Some(clock);
        let mut near: Vec<Vec3> = bodies.iter().map(|b| *b + Vec3::Z * MIDDLE).collect();
        near.sort_by(|a, b| a.distance_squared(eye).total_cmp(&b.distance_squared(eye)));
        near.truncate(TRACKED);
        for s in &mut self.slots {
            s.seen = false;
        }
        for b in near {
            let found = (0..self.slots.len())
                .filter(|&k| !self.slots[k].seen)
                .min_by(|&i, &j| {
                    let d = |k: usize| self.slots[k].at.distance_squared(b);
                    d(i).total_cmp(&d(j))
                })
                .filter(|&k| self.slots[k].at.distance(b) < 0.5 + 10.0 * dt);
            if let Some(k) = found {
                let s = &mut self.slots[k];
                if dt > 0.0 {
                    let v = ((b - s.at) / dt).clamp_length_max(8.0);
                    s.wind = s.wind.lerp(v, 1.0 - expf(-dt / 0.08));
                }
                s.at = b;
                s.seen = true;
            } else {
                self.slots.push(Slot {
                    at: b,
                    seen: true,
                    ..Slot::default()
                });
            }
        }
        for s in &mut self.slots {
            if !s.seen {
                s.wind = Vec3::ZERO;
            }
            let mut left = dt;
            while left > 0.0 {
                let h = left.min(1.0 / 120.0);
                let a = s.wind * PUSH - s.push * STIFF - s.speed * DAMP;
                s.speed += a * h;
                s.push = (s.push + s.speed * h).clamp_length_max(MOST);
                left -= h;
            }
        }
        self.slots.retain(|s| s.seen || s.energy() > 1e-4);
    }

    /// The springs as the shader reads them at `clock` seconds: the most stirred of them.
    #[must_use]
    pub fn bytes(&self, clock: f64) -> Vec<u8> {
        let mut live: Vec<&Slot> = self.slots.iter().filter(|s| s.energy() > 1e-4).collect();
        live.sort_by(|a, b| b.energy().total_cmp(&a.energy()));
        live.truncate(STIR_SLOTS);
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let head = [live.len() as f32, (clock % 3600.0) as f32, 0.0, 0.0];
        let mut b = Vec::with_capacity(16 + 32 * STIR_SLOTS);
        let mut put = |v: [f32; 4]| b.extend(v.iter().flat_map(|f| f.to_le_bytes()));
        put(head);
        for k in 0..STIR_SLOTS {
            put(live
                .get(k)
                .map_or([0.0; 4], |s| s.at.extend(STIR_REACH).to_array()));
        }
        for k in 0..STIR_SLOTS {
            put(live
                .get(k)
                .map_or([0.0; 4], |s| s.push.extend(s.speed.length()).to_array()));
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_2005_files_corner_and_small_webs_are_cobwebs_too() {
        for id in [ROPES, ORB, SMALL, ORB_2005, SMALL_2005] {
            assert!(is_cobweb(DataId(id)), "{id:#010X}");
        }
        assert!(!is_cobweb(DataId(0x0500_1F4F)));
    }

    #[test]
    fn a_corner_card_spins_an_orb_web() {
        let quad = |p: [f32; 3], uv: [f32; 2]| Vertex {
            position: p,
            normal: [0.0, 0.0, 1.0],
            uv,
            extra: [0, 0],
        };
        let card = MeshData {
            vertices: vec![
                quad([0.0, 0.0, 0.0], [0.0, 1.0]),
                quad([1.0, 0.0, 0.0], [1.0, 1.0]),
                quad([1.0, 0.0, 1.0], [1.0, 0.0]),
                quad([0.0, 0.0, 1.0], [0.0, 0.0]),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            submeshes: vec![SubMesh {
                surface_index: 0,
                surface: None,
                sides: Sides::Both,
                first_index: 0,
                index_count: 6,
            }],
            ..MeshData::default()
        };
        let picture = Picture {
            width: 8,
            height: 8,
            alpha: vec![255; 64],
        };
        let (woven, silk) = weave(&card, &[0], &picture, ORB, 7).expect("a web");
        assert_eq!(silk, 1);
        assert!(
            woven.submeshes[1].index_count > 6 * 400,
            "{}",
            woven.submeshes[1].index_count
        );
        let again = weave(&card, &[0], &picture, ORB, 7).expect("a web");
        assert_eq!(again.0.vertices.len(), woven.vertices.len());
    }

    #[test]
    fn a_passing_body_stirs_then_settles() {
        let mut s = Springs::default();
        let mut t = 0.0;
        let mut most = 0.0f32;
        for i in 0..60 {
            #[allow(clippy::cast_precision_loss)]
            let at = Vec3::new(-3.0 + 0.1 * i as f32, 0.0, 0.0);
            s.step(&[at], Vec3::ZERO, t);
            most = most.max(s.most());
            t += 1.0 / 30.0;
        }
        assert!(most > 0.02, "{most}");
        for _ in 0..90 {
            s.step(&[Vec3::new(3.0, 0.0, 0.0)], Vec3::ZERO, t);
            t += 1.0 / 30.0;
        }
        assert!(s.most() < 0.004, "{}", s.most());
    }
}
