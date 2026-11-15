//! The shore field: for each 3 m texel of a land field, the signed distance
//! to the water's edge, the way the shore lies blurred by how far off it is (its length how much
//! the shores about agree), and the water's width there. Built once per land field on the stream's
//! worker and read by the water for which way its swell runs.
//!
//! - **The edge** is where the land field's distance inside the water, read bilinearly, is nought:
//!   the edge the water draws, without its wander.
//! - **Distance**: an exact Euclidean transform (Felzenszwalb and Huttenlocher's two passes of
//!   lower envelopes) both ways, keeping each texel's nearest texel of the other kind.
//! - **Direction**: the unit vector landward (to the nearest land from the water, away from the
//!   nearest water on land, so it runs on across the edge), blurred on three levels of 3, 6 and
//!   12 m texels, four texels each; a texel `d` metres from the edge reads the levels about the one
//!   whose blur is `d / 2`.
//! - **Width**: the distance to the edge and on, away from it, to the opposite shore, marched
//!   through the distance itself; at most [`MOST`].

use glam::Vec2;
use pyreal_primitives::num::math::log2;

use crate::land::LandField;

/// Metres between texels, and texels to a cell.
pub const TEXEL: f32 = 3.0;
pub const PER_CELL: usize = 8;
/// The farthest distance and width kept, metres.
pub const MOST: f32 = 512.0;
/// Each level's blur, in its own texels.
const SIGMA: f32 = 4.0;
/// The most steps the width's march takes.
const MARCH_STEPS: usize = 96;

/// One land field's shore field, `side` texels a side, texel `(0, 0)` on the field's first vertex.
#[derive(Debug, Clone, PartialEq)]
pub struct ShoreField {
    pub side: (usize, usize),
    /// Signed distance to the edge, metres, positive in the water.
    pub distance: Vec<f32>,
    /// The blurred way landward; its length the shores' agreement, 0 to 1.
    pub toward: Vec<Vec2>,
    /// The water's width through the texel, metres (nought on land).
    pub width: Vec<f32>,
}

/// What [`transform`] gives each texel: the squared distance to the nearest site, texels, and it.
struct Nearest {
    dist2: Vec<f64>,
    site: Vec<(u32, u32)>,
}

const FAR2: f64 = 1.0e20;

/// The lower envelope of parabolas `f(q) + (x − q)²` along one line: each `x`'s least value and
/// the `q` it comes from (Felzenszwalb and Huttenlocher 2012).
fn envelope(f: &[f64], d: &mut [f64], arg: &mut [usize], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    let Some(first) = (0..n).find(|&q| f[q] < FAR2) else {
        d.fill(FAR2);
        arg.fill(usize::MAX);
        return;
    };
    #[allow(clippy::cast_precision_loss)]
    let sq = |q: usize| (q * q) as f64;
    let mut k = 0usize;
    v[0] = first;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in first + 1..n {
        if f[q] >= FAR2 {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let meet = |p: usize| ((f[q] + sq(q)) - (f[p] + sq(p))) / (2.0 * q as f64 - 2.0 * p as f64);
        let mut s = meet(v[k]);
        // z[0] is minus infinity, so this stops at the first parabola.
        while s <= z[k] {
            k -= 1;
            s = meet(v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for x in 0..n {
        #[allow(clippy::cast_precision_loss)]
        while z[k + 1] < x as f64 {
            k += 1;
        }
        let q = v[k];
        #[allow(clippy::cast_precision_loss)]
        let dx = x as f64 - q as f64;
        d[x] = f[q] + dx * dx;
        arg[x] = q;
    }
}

/// The exact squared distance from every texel of a `w` by `h` grid to the nearest `site`, and
/// which texel that is.
fn transform(w: usize, h: usize, site: &dyn Fn(usize) -> bool) -> Nearest {
    let n = w.max(h);
    let (mut f, mut d) = (vec![0.0; n], vec![0.0; n]);
    let (mut arg, mut v, mut z) = (vec![0usize; n], vec![0usize; n], vec![0.0f64; n + 1]);
    // Down each column: the squared distance along it and the row it comes from.
    let mut col = vec![FAR2; w * h];
    let mut row = vec![u32::MAX; w * h];
    for x in 0..w {
        for (y, v) in f.iter_mut().enumerate().take(h) {
            *v = if site(y * w + x) { 0.0 } else { FAR2 };
        }
        envelope(&f[..h], &mut d[..h], &mut arg[..h], &mut v, &mut z);
        for y in 0..h {
            col[y * w + x] = d[y].min(FAR2);
            row[y * w + x] = u32::try_from(arg[y]).unwrap_or(u32::MAX);
        }
    }
    // Along each row over the columns' results.
    let mut out = Nearest {
        dist2: vec![FAR2; w * h],
        site: vec![(u32::MAX, u32::MAX); w * h],
    };
    for y in 0..h {
        f[..w].copy_from_slice(&col[y * w..(y + 1) * w]);
        envelope(&f[..w], &mut d[..w], &mut arg[..w], &mut v, &mut z);
        for x in 0..w {
            if d[x] < FAR2 && arg[x] != usize::MAX {
                let q = arg[x];
                out.dist2[y * w + x] = d[x];
                out.site[y * w + x] = (u32::try_from(q).unwrap_or(0), row[y * w + q]);
            }
        }
    }
    out
}

/// A Gaussian blur of `sigma` texels over a `w` by `h` grid of vectors, its weights kept whole at
/// the grid's edge.
fn blur(src: &[Vec2], w: usize, h: usize, sigma: f32) -> Vec<Vec2> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let r = (sigma * 3.0).ceil() as usize;
    #[allow(clippy::cast_precision_loss)]
    let kernel: Vec<f32> = (0..=r)
        .map(|i| pyreal_primitives::num::math::expf(-((i * i) as f32) / (2.0 * sigma * sigma)))
        .collect();
    let pass = |src: &[Vec2], along_x: bool| -> Vec<Vec2> {
        let mut out = vec![Vec2::ZERO; w * h];
        for y in 0..h {
            for x in 0..w {
                let (mut sum, mut weight) = (Vec2::ZERO, 0.0);
                let (at, len) = if along_x { (x, w) } else { (y, h) };
                for t in at.saturating_sub(r)..(at + r + 1).min(len) {
                    let k = kernel[at.abs_diff(t)];
                    let i = if along_x { y * w + t } else { t * w + x };
                    sum += src[i] * k;
                    weight += k;
                }
                out[y * w + x] = sum / weight;
            }
        }
        out
    };
    pass(&pass(src, true), false)
}

/// A grid halved: each texel the mean of the (up to) four under it.
fn halve(src: &[Vec2], w: usize, h: usize) -> (Vec<Vec2>, usize, usize) {
    let (hw, hh) = (w.div_ceil(2), h.div_ceil(2));
    let mut out = vec![Vec2::ZERO; hw * hh];
    for y in 0..hh {
        for x in 0..hw {
            let (mut s, mut n) = (Vec2::ZERO, 0.0);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (sx, sy) = (x * 2 + dx, y * 2 + dy);
                if sx < w && sy < h {
                    s += src[sy * w + sx];
                    n += 1.0;
                }
            }
            out[y * hw + x] = s / n;
        }
    }
    (out, hw, hh)
}

/// A grid read bilinearly at `p`, in its own texels.
fn bilinear(g: &[Vec2], w: usize, h: usize, p: Vec2) -> Vec2 {
    #[allow(clippy::cast_precision_loss)]
    let p = p.clamp(Vec2::ZERO, Vec2::new((w - 1) as f32, (h - 1) as f32));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x0, y0) = (p.x.floor() as usize, p.y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    #[allow(clippy::cast_precision_loss)]
    let f = p - Vec2::new(x0 as f32, y0 as f32);
    let a = g[y0 * w + x0].lerp(g[y0 * w + x1], f.x);
    let b = g[y1 * w + x0].lerp(g[y1 * w + x1], f.x);
    a.lerp(b, f.y)
}

impl ShoreField {
    /// The shore field of a land field whose water is worked out ([`LandField::finish`]).
    #[must_use]
    pub fn build(land: &LandField) -> Self {
        let (vw, vh) = land.side;
        let (w, h) = (
            vw.saturating_sub(1) * PER_CELL + 1,
            vh.saturating_sub(1) * PER_CELL + 1,
        );
        // The water: the land field's distance inside it, bilinear between its vertices.
        let water: Vec<bool> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let (cx, cy) = ((x / PER_CELL).min(vw - 1), (y / PER_CELL).min(vh - 1));
                let (nx, ny) = ((cx + 1).min(vw - 1), (cy + 1).min(vh - 1));
                #[allow(clippy::cast_precision_loss)]
                let (fx, fy) = (
                    (x - cx * PER_CELL) as f32 / PER_CELL as f32,
                    (y - cy * PER_CELL) as f32 / PER_CELL as f32,
                );
                let s = |x: usize, y: usize| land.inside[y * vw + x];
                let a = s(cx, cy) + (s(nx, cy) - s(cx, cy)) * fx;
                let b = s(cx, ny) + (s(nx, ny) - s(cx, ny)) * fx;
                a + (b - a) * fy > 0.0
            })
            .collect();
        Self::from_mask(&water, w, h)
    }

    /// The shore field over a `w` by `h` mask of 3 m texels, true in the water.
    #[must_use]
    pub fn from_mask(water: &[bool], w: usize, h: usize) -> Self {
        let to_land = transform(w, h, &|i| !water[i]);
        let to_water = transform(w, h, &|i| water[i]);
        let mut distance = vec![0.0f32; w * h];
        let mut raw = vec![Vec2::ZERO; w * h];
        for i in 0..w * h {
            #[allow(clippy::cast_precision_loss)]
            let here = Vec2::new((i % w) as f32, (i / w) as f32);
            let near = if water[i] { &to_land } else { &to_water };
            let d2 = near.dist2[i];
            #[allow(clippy::cast_possible_truncation)]
            let d = if d2 >= FAR2 {
                MOST
            } else {
                (d2.sqrt() as f32 * TEXEL - TEXEL * 0.5).clamp(0.0, MOST)
            };
            distance[i] = if water[i] { d } else { -d };
            let (sx, sy) = near.site[i];
            if sx != u32::MAX {
                #[allow(clippy::cast_precision_loss)]
                let to = Vec2::new(sx as f32, sy as f32) - here;
                let way = to.normalize_or_zero();
                raw[i] = if water[i] { way } else { -way };
            }
        }
        // Three levels of blur, each four of its own texels.
        let l0 = blur(&raw, w, h, SIGMA);
        let (h1, w1, hh1) = halve(&raw, w, h);
        let l1 = blur(&h1, w1, hh1, SIGMA);
        let (h2, w2, hh2) = halve(&h1, w1, hh1);
        let l2 = blur(&h2, w2, hh2, SIGMA);
        let toward: Vec<Vec2> = (0..w * h)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let p = Vec2::new((i % w) as f32, (i / w) as f32);
                // The level whose blur (12, 24, 48 m) is nearest half the distance, in log steps.
                let want = (distance[i].abs() * 0.5).max(SIGMA * TEXEL);
                #[allow(clippy::cast_possible_truncation)]
                let t = (log2(f64::from(want / (SIGMA * TEXEL))) as f32).clamp(0.0, 2.0);
                let read = |level: u32| match level {
                    0 => l0[i],
                    1 => bilinear(&l1, w1, hh1, p * 0.5),
                    _ => bilinear(&l2, w2, hh2, p * 0.25),
                };
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let lo = t.floor() as u32;
                read(lo).lerp(read((lo + 1).min(2)), t - t.floor())
            })
            .collect();
        // The width: on from the texel away from its nearest shore, stepping by the distance to
        // the land, till the land or the field's edge.
        let width: Vec<f32> = (0..w * h)
            .map(|i| {
                if !water[i] {
                    return 0.0;
                }
                let (sx, sy) = to_land.site[i];
                if sx == u32::MAX {
                    return MOST;
                }
                #[allow(clippy::cast_precision_loss)]
                let here = Vec2::new((i % w) as f32, (i / w) as f32);
                #[allow(clippy::cast_precision_loss)]
                let away = (here - Vec2::new(sx as f32, sy as f32)).normalize_or_zero();
                let mut t = 0.0f32;
                for _ in 0..MARCH_STEPS {
                    let q = here + away * t;
                    #[allow(clippy::cast_precision_loss)]
                    if q.x < 0.0 || q.y < 0.0 || q.x > (w - 1) as f32 || q.y > (h - 1) as f32 {
                        return MOST;
                    }
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let j = q.y.round() as usize * w + q.x.round() as usize;
                    if !water[j] {
                        break;
                    }
                    #[allow(clippy::cast_possible_truncation)]
                    let step = (to_land.dist2[j].sqrt() as f32 - 0.5).max(1.0);
                    t += step;
                    if t * TEXEL >= MOST {
                        return MOST;
                    }
                }
                (distance[i] + t * TEXEL).min(MOST)
            })
            .collect();
        Self {
            side: (w, h),
            distance,
            toward,
            width,
        }
    }

    /// The texels as the GPU reads them: four half floats each (the distance, the way landward,
    /// the width).
    #[must_use]
    pub fn texels(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.distance.len() * 8);
        for i in 0..self.distance.len() {
            for v in [
                self.distance[i],
                self.toward[i].x,
                self.toward[i].y,
                self.width[i],
            ] {
                out.extend_from_slice(&half(v).to_le_bytes());
            }
        }
        out
    }
}

/// `v` as a half float's bits, rounded to the nearest.
#[must_use]
pub fn half(v: f32) -> u16 {
    let b = v.to_bits();
    let sign = u16::try_from((b >> 16) & 0x8000).unwrap_or(0);
    let exp = i32::try_from((b >> 23) & 0xFF).unwrap_or(0) - 127 + 15;
    let man = b & 0x7F_FFFF;
    if v.is_nan() {
        return sign | 0x7E00;
    }
    if exp >= 31 {
        return sign | 0x7C00;
    }
    if exp <= 0 {
        if exp < -10 {
            return sign;
        }
        let m = (man | 0x80_0000) >> u32::try_from(1 - exp).unwrap_or(0);
        let r = (m >> 13) + ((m >> 12) & 1);
        return sign | u16::try_from(r).unwrap_or(0);
    }
    let e = u32::try_from(exp).unwrap_or(0);
    // Rounding may carry into the exponent, which is still right.
    let r = ((e << 10) | (man >> 13)) + ((man >> 12) & 1);
    sign | u16::try_from(r.min(0x7C00)).unwrap_or(0x7C00)
}
