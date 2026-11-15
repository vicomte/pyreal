//! The land's shape the drawn ground takes: retail's facets ([`Facets`]) rounded over a width
//! that narrows where a crease is sharp ([`Rounded`]), so every crease is rounded and none is
//! drawn far from retail's surface.

use super::{GroundSource, Knob, Knobs, WORLD_VERTICES};
use crate::consts::CELL_SIZE;
use crate::land::mesh::sw_to_ne_cut;
use pyreal_primitives::num::{floor_to_i32, math};

/// Meters between the rounded surface's samples.
pub const ROUND_STEP: f32 = 2.0;

/// How many widths a rounding kernel reaches each way.
const KERNEL_REACH: f32 = 2.5;

/// The narrowest a crease is rounded over, meters.
const NARROWEST: f32 = 2.0;

/// Retail's facets over a rectangle of global vertices.
#[derive(Debug, Clone)]
pub struct Facets {
    /// The rectangle's south-west vertex, global.
    origin: (i32, i32),
    /// Its vertices east and north.
    side: (usize, usize),
    heights: Vec<f32>,
}

impl Facets {
    /// The vertices `lo` to `hi` (global, both included) of `src`.
    #[must_use]
    pub fn new(src: &dyn GroundSource, lo: (i32, i32), hi: (i32, i32)) -> Self {
        let side = |a: i32, b: i32| usize::try_from(b - a + 1).unwrap_or(0);
        let mut heights = Vec::with_capacity(side(lo.0, hi.0) * side(lo.1, hi.1));
        for gy in lo.1..=hi.1 {
            for gx in lo.0..=hi.0 {
                let (gx, gy) = (
                    gx.clamp(0, WORLD_VERTICES - 1),
                    gy.clamp(0, WORLD_VERTICES - 1),
                );
                heights.push(src.vertex(gx, gy).map_or(0.0, |v| v.height));
            }
        }
        Self {
            origin: lo,
            side: (side(lo.0, hi.0), side(lo.1, hi.1)),
            heights,
        }
    }

    /// Vertex `(gx, gy)`'s height, global, held to the world and the rectangle.
    #[must_use]
    pub fn height(&self, gx: i32, gy: i32) -> f32 {
        let clamp = |g: i32, o: i32, n: usize| {
            usize::try_from((g.clamp(0, WORLD_VERTICES - 1) - o).max(0))
                .unwrap_or(0)
                .min(n.saturating_sub(1))
        };
        let (i, j) = (
            clamp(gx, self.origin.0, self.side.0),
            clamp(gy, self.origin.1, self.side.1),
        );
        self.heights
            .get(j * self.side.0 + i)
            .copied()
            .unwrap_or(0.0)
    }

    /// The facets' height at global `(x, y)` meters, each cell cut as retail cuts it.
    #[must_use]
    pub fn facet(&self, x: f32, y: f32) -> f32 {
        let (cx, cy) = (
            floor_to_i32(x / CELL_SIZE).clamp(0, WORLD_VERTICES - 2),
            floor_to_i32(y / CELL_SIZE).clamp(0, WORLD_VERTICES - 2),
        );
        // LINT-OK: global cell coordinates, far inside f32's exact range.
        #[allow(clippy::cast_precision_loss)]
        let (fx, fy) = (
            (x / CELL_SIZE - cx as f32).clamp(0.0, 1.0),
            (y / CELL_SIZE - cy as f32).clamp(0.0, 1.0),
        );
        let (h00, h10, h01, h11) = (
            self.height(cx, cy),
            self.height(cx + 1, cy),
            self.height(cx, cy + 1),
            self.height(cx + 1, cy + 1),
        );
        if sw_to_ne_cut(cx, cy) {
            if fx >= fy {
                h00 + fx * (h10 - h00) + fy * (h11 - h10)
            } else {
                h00 + fy * (h01 - h00) + fx * (h11 - h01)
            }
        } else if fx + fy <= 1.0 {
            h00 + fx * (h10 - h00) + fy * (h01 - h00)
        } else {
            h11 + (1.0 - fx) * (h01 - h11) + (1.0 - fy) * (h10 - h11)
        }
    }
}

/// Retail's [`Facets`] rounded, [`ROUND_STEP`] meters a sample over a rectangle: each
/// sample the facets averaged over a round [`Knob::RoundWidth`] wide, or narrower where that would
/// take the ground further than [`Knob::RoundCap`] off them (a sharp ridge, a valley's floor, a
/// cliff's edge), so every crease is rounded and none drawn far off. The width changes slowly from
/// place to place, so the rounding adds no bump of its own.
#[derive(Debug, Clone)]
pub struct Rounded {
    /// The rectangle's south-west corner, global meters.
    origin: (f32, f32),
    /// Its samples east and north.
    side: (usize, usize),
    heights: Vec<f32>,
}

/// How far, meters, the rounding reads past where it is wanted: the kernel, the width's own
/// estimate and its kernel again.
#[must_use]
pub fn rounding_reach(knobs: &Knobs) -> f32 {
    let width = knobs.get(Knob::RoundWidth).max(0.0);
    2.0 * KERNEL_REACH * width + 2.0 * width + 2.0 * ROUND_STEP
}

/// A Gaussian's taps `width` meters wide, [`ROUND_STEP`] apart, the middle first.
fn kernel(width: f32) -> Vec<f32> {
    // LINT-OK: a few dozen taps.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let half = (KERNEL_REACH * width / ROUND_STEP).ceil().max(0.0) as usize;
    (0..=half)
        .map(|k| {
            // LINT-OK: a tap index.
            #[allow(clippy::cast_precision_loss)]
            let d = k as f32 * ROUND_STEP;
            math::expf(-d * d / (2.0 * width * width).max(1e-6))
        })
        .collect()
}

/// `values` (`w` by `h`, row by row) along rows (`east`) or columns, each sample through the
/// kernel `pick` gives it; held at the edges.
fn pass(
    values: &[f32],
    (w, h): (usize, usize),
    east: bool,
    pick: &dyn Fn(usize) -> usize,
    kernels: &[Vec<f32>],
) -> Vec<f32> {
    let mut out = vec![0.0; values.len()];
    let (len, lines) = if east { (w, h) } else { (h, w) };
    let index = |line: usize, k: usize| if east { line * w + k } else { k * w + line };
    for line in 0..lines {
        for k in 0..len {
            let at = index(line, k);
            let taps = &kernels[pick(at)];
            let (mut sum, mut total) = (values[at] * taps[0], taps[0]);
            for (d, t) in taps.iter().enumerate().skip(1) {
                let a = values[index(line, k.saturating_sub(d))];
                let b = values[index(line, (k + d).min(len - 1))];
                sum += (a + b) * t;
                total += 2.0 * t;
            }
            out[at] = sum / total;
        }
    }
    out
}

/// `values` (`w` by `h`) each the greatest within `r` samples, a square round it.
fn greatest(values: &[f32], (w, h): (usize, usize), r: usize) -> Vec<f32> {
    let mut rows = vec![0.0f32; values.len()];
    for j in 0..h {
        for i in 0..w {
            let (a, b) = (i.saturating_sub(r), (i + r).min(w - 1));
            rows[j * w + i] = values[j * w + a..=j * w + b]
                .iter()
                .fold(0.0, |m, v| m.max(*v));
        }
    }
    let mut out = vec![0.0f32; values.len()];
    for j in 0..h {
        for i in 0..w {
            let (a, b) = (j.saturating_sub(r), (j + r).min(h - 1));
            out[j * w + i] = (a..=b).fold(0.0, |m, jj| m.max(rows[jj * w + i]));
        }
    }
    out
}

impl Rounded {
    /// `retail` rounded by the `knobs` over `side` samples from global `origin` meters.
    #[must_use]
    pub fn new(retail: &Facets, origin: (f32, f32), side: (usize, usize), knobs: &Knobs) -> Self {
        let (w, h) = side;
        let mut facets = Vec::with_capacity(w * h);
        for j in 0..h {
            for i in 0..w {
                // LINT-OK: sample indices, a few hundred.
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = (i as f32 * ROUND_STEP, j as f32 * ROUND_STEP);
                facets.push(retail.facet(origin.0 + x, origin.1 + y));
            }
        }
        let width = knobs.get(Knob::RoundWidth);
        if width < NARROWEST {
            return Self {
                origin,
                side,
                heights: facets,
            };
        }
        // How far rounding at the full width takes each place, the most round it, evened out.
        let full = [kernel(width)];
        let once = pass(&facets, side, true, &|_| 0, &full);
        let wide = pass(&once, side, false, &|_| 0, &full);
        let off: Vec<f32> = wide
            .iter()
            .zip(&facets)
            .map(|(a, b)| (a - b).abs())
            .collect();
        // LINT-OK: a few samples.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let r = (width / ROUND_STEP).ceil() as usize;
        let most = greatest(&off, side, r);
        let box_ = [vec![1.0; r + 1]];
        let most = pass(
            &pass(&most, side, true, &|_| 0, &box_),
            side,
            false,
            &|_| 0,
            &box_,
        );
        // Each place's width, in quarter meters: narrower where the full width goes too far.
        let cap = knobs.get(Knob::RoundCap).max(0.0);
        // LINT-OK: a width in quarter meters, a few dozen.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let level: Vec<usize> = most
            .iter()
            .map(|m| {
                let wanted = if *m > 1e-4 { width * cap / m } else { width };
                (wanted.clamp(NARROWEST, width) * 4.0).round() as usize
            })
            .collect();
        // LINT-OK: quarter meters up to the widest.
        #[allow(clippy::cast_precision_loss)]
        let kernels: Vec<Vec<f32>> = (0..=level.iter().copied().max().unwrap_or(0))
            .map(|q| kernel(q as f32 * 0.25))
            .collect();
        let pick = |k: usize| level[k];
        let once = pass(&facets, side, true, &pick, &kernels);
        let heights = pass(&once, side, false, &pick, &kernels);
        Self {
            origin,
            side,
            heights,
        }
    }

    /// The rounded surface at global `(x, y)` meters, between the samples.
    #[must_use]
    pub fn at(&self, x: f32, y: f32) -> f32 {
        let (w, h) = self.side;
        // LINT-OK: the rectangle's sides, a few hundred.
        #[allow(clippy::cast_precision_loss)]
        let (fx, fy) = (
            ((x - self.origin.0) / ROUND_STEP).clamp(0.0, (w - 1) as f32),
            ((y - self.origin.1) / ROUND_STEP).clamp(0.0, (h - 1) as f32),
        );
        // LINT-OK: clamped to the rectangle.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (i, j) = ((fx as usize).min(w - 2), (fy as usize).min(h - 2));
        // LINT-OK: sample indices, exact in f32.
        #[allow(clippy::cast_precision_loss)]
        let (tx, ty) = (fx - i as f32, fy - j as f32);
        let v = |a: usize, b: usize| self.heights[b * w + a];
        let south = v(i, j) + (v(i + 1, j) - v(i, j)) * tx;
        let north = v(i, j + 1) + (v(i + 1, j + 1) - v(i, j + 1)) * tx;
        south + (north - south) * ty
    }
}
