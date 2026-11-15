//! Ground Coloring: end of retail's ground pictures drawn in colours toward the 1999 ground's
//! (Classic), or as they are (Throne of Destiny).
//!
//! Each tint was measured from the two eras' pictures of a terrain type: the mean hue's turn and
//! the mean saturation's ratio (`docs/ERAS.md`). Only the retail picture is tinted, as it is
//! loaded, before the ground cover takes its colour from it; an art pack's own ground picture is
//! drawn as the pack made it.

use std::sync::atomic::{AtomicU8, Ordering};

/// How end of retail's ground is coloured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GroundColoring {
    /// Toward the 1999 ground's colours.
    #[default]
    Classic,
    /// As the pictures are.
    ThroneOfDestiny,
}

/// The coloring in force, for the whole process: the near ground and the far terrain, which is
/// worked out away from the loader, read it.
static COLORING: AtomicU8 = AtomicU8::new(0);

/// The coloring in force.
#[must_use]
pub fn coloring() -> GroundColoring {
    match COLORING.load(Ordering::Relaxed) {
        0 => GroundColoring::Classic,
        _ => GroundColoring::ThroneOfDestiny,
    }
}

/// Make `coloring` the one in force; whether it changed.
pub fn set_coloring(coloring: GroundColoring) -> bool {
    let v = match coloring {
        GroundColoring::Classic => 0,
        GroundColoring::ThroneOfDestiny => 1,
    };
    COLORING.swap(v, Ordering::Relaxed) != v
}

/// One terrain type's tint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tint {
    /// The hue turned by `turn` degrees, then pulled `pull` of the way toward the hue `toward`,
    /// and the saturation scaled by `saturation`. Value is kept.
    Turn {
        turn: f32,
        toward: f32,
        pull: f32,
        saturation: f32,
    },
    /// Leaf litter over deep green: each texel coloured by how light it and the texels within
    /// about `blur` of it are (`mix` of the way the neighbourhood's), from the hue `green` for the
    /// darkest `low` of the picture to `brown` for the lightest above `high`, at `saturation`, its
    /// value scaled by `value`.
    Litter {
        blur: f32,
        mix: f32,
        low: f32,
        high: f32,
        green: f32,
        brown: f32,
        saturation: f32,
        value: f32,
    },
}

const fn turn(turn: f32, saturation: f32) -> Tint {
    Tint::Turn {
        turn,
        toward: 0.0,
        pull: 0.0,
        saturation,
    }
}

/// The Classic tints, by terrain type (`ROAD_TERRAIN_TYPE` for the road). The road's colours
/// spread from orange to teal, so a turn would make some pink; it is pulled toward one hue. The
/// forest floor is a plain grass in end of retail and dark leaf litter in 1999, which no turn
/// reaches.
pub const CLASSIC: [(u32, Tint); 10] = [
    (0, turn(5.0, 1.8)),    // BarrenRock
    (1, turn(8.0, 1.4)),    // Grassland
    (3, turn(20.0, 1.25)),  // LushGrass
    (4, turn(-5.0, 1.25)),  // MarshSparseSwamp
    (8, turn(7.0, 1.9)),    // PatchyDirt
    (9, turn(22.0, 1.9)),   // PatchyGrassland
    (13, turn(-7.0, 1.45)), // SedimentaryRock
    (14, turn(30.0, 0.95)), // SemiBarrenRock
    (
        21, // forestfloor
        Tint::Litter {
            blur: 3.0,
            mix: 0.5,
            low: 0.55,
            high: 0.88,
            green: 88.0,
            brown: 34.0,
            saturation: 0.85,
            value: 0.95,
        },
    ),
    (
        pyreal_terrain::consts::ROAD_TERRAIN_TYPE,
        Tint::Turn {
            turn: 0.0,
            toward: 40.0,
            pull: 0.8,
            saturation: 1.4,
        },
    ),
];

/// Terrain type `terrain_type`'s tint under `coloring`, if it has one.
#[must_use]
pub fn tint_for(coloring: GroundColoring, terrain_type: u32) -> Option<Tint> {
    if coloring != GroundColoring::Classic {
        return None;
    }
    CLASSIC
        .iter()
        .find(|(t, _)| *t == terrain_type)
        .map(|(_, tint)| *tint)
}

/// `bgra` (sRGB bytes, blue first, `width` by `height`, wrapping as it tiles) tinted in place.
pub fn apply(tint: Tint, width: u32, height: u32, bgra: &mut [u8]) {
    match tint {
        Tint::Turn {
            turn,
            toward,
            pull,
            saturation,
        } => {
            for px in bgra.as_chunks_mut::<4>().0 {
                let [b, g, r, _] = *px;
                let (h, s, v) = hsv(r, g, b);
                let mut h = (h + turn).rem_euclid(360.0);
                let to = ((toward - h + 540.0).rem_euclid(360.0)) - 180.0;
                h = (h + pull * to).rem_euclid(360.0);
                let [r, g, b] = rgb(h, (s * saturation).clamp(0.0, 1.0), v);
                px[..3].copy_from_slice(&[b, g, r]);
            }
        }
        Tint::Litter { .. } => litter(tint, width as usize, height as usize, bgra),
    }
}

/// [`Tint::Litter`] over `bgra`.
fn litter(tint: Tint, w: usize, h: usize, bgra: &mut [u8]) {
    let Tint::Litter {
        blur,
        mix,
        low,
        high,
        green,
        brown,
        saturation,
        value,
    } = tint
    else {
        return;
    };
    let texels = bgra.as_chunks_mut::<4>().0;
    if texels.len() != w * h || texels.is_empty() {
        return;
    }
    let v: Vec<f32> = texels.iter().map(|p| hsv(p[2], p[1], p[0]).2).collect();
    let near = blurred(&v, w, h, blur);
    let key: Vec<f32> = v
        .iter()
        .zip(&near)
        .map(|(a, b)| a + (b - a) * mix)
        .collect();
    let mut order: Vec<usize> = (0..key.len()).collect();
    order.sort_by(|a, b| key[*a].total_cmp(&key[*b]));
    #[allow(clippy::cast_precision_loss)]
    let n = order.len() as f32;
    for (rank, &i) in order.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let t = ((rank as f32 / n - low) / (high - low)).clamp(0.0, 1.0);
        let t = t * t * (3.0 - 2.0 * t);
        let hue = green + (brown - green) * t;
        let val = (v[i] * value * (0.9 + 0.15 * t)).clamp(0.0, 1.0);
        let [r, g, b] = rgb(hue.rem_euclid(360.0), saturation, val);
        texels[i][..3].copy_from_slice(&[b, g, r]);
    }
}

/// `v` (`w` by `h`) blurred by a Gaussian of `sigma` texels, wrapping at its edges.
fn blurred(v: &[f32], w: usize, h: usize, sigma: f32) -> Vec<f32> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let reach = (sigma * 3.0).ceil() as usize;
    #[allow(clippy::cast_precision_loss)]
    let weights: Vec<f32> = (0..=2 * reach)
        .map(|k| {
            pyreal_primitives::num::math::expf(
                -((k as f32 - reach as f32).powi(2)) / (2.0 * sigma * sigma),
            )
        })
        .collect();
    let total: f32 = weights.iter().sum();
    let pass = |src: &[f32], along_x: bool| -> Vec<f32> {
        let mut out = vec![0.0; src.len()];
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0.0;
                for (k, wt) in weights.iter().enumerate() {
                    let (sx, sy) = if along_x {
                        ((x + w * (reach + 1) + k - reach) % w, y)
                    } else {
                        (x, (y + h * (reach + 1) + k - reach) % h)
                    };
                    sum += src[sy * w + sx] * wt;
                }
                out[y * w + x] = sum / total;
            }
        }
        out
    };
    pass(&pass(v, true), false)
}

/// Hue in degrees, saturation and value of sRGB bytes.
fn hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let [r, g, b] = [r, g, b].map(|c| f32::from(c) / 255.0);
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let s = if max > 0.0 { d / max } else { 0.0 };
    let h = if d <= f32::EPSILON {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s, max)
}

/// sRGB bytes of a hue in degrees, saturation and value.
fn rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let sector = (h / 60.0) as u32;
    let (r, g, b) = match sector {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    [r, g, b].map(|c| ((c + m) * 255.0 + 0.5).clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_tints_the_named_types_and_throne_of_destiny_none() {
        assert!(tint_for(GroundColoring::Classic, 1).is_some());
        assert!(tint_for(
            GroundColoring::Classic,
            pyreal_terrain::consts::ROAD_TERRAIN_TYPE
        )
        .is_some());
        assert!(tint_for(GroundColoring::Classic, 24).is_none(), "Argila");
        assert!(tint_for(GroundColoring::ThroneOfDestiny, 1).is_none());
    }

    #[test]
    fn a_tint_turns_the_hue_scales_the_saturation_and_keeps_grey_and_value() {
        // An olive green, turned 30 degrees toward pure green, saturation kept.
        let mut px = [0, 128, 128, 255];
        apply(turn(60.0, 1.0), 1, 1, &mut px);
        assert_eq!(px, [0, 128, 0, 255]);
        let mut grey = [90, 90, 90, 7];
        apply(turn(20.0, 1.9), 1, 1, &mut grey);
        assert_eq!(grey, [90, 90, 90, 7]);
        // Pulled all the way to hue 40: an orange-yellow, value kept.
        let mut teal = [128, 128, 0, 255];
        let to_40 = Tint::Turn {
            turn: 0.0,
            toward: 40.0,
            pull: 1.0,
            saturation: 1.0,
        };
        apply(to_40, 1, 1, &mut teal);
        assert_eq!(teal, [0, 85, 128, 255]);
    }

    #[test]
    fn litter_makes_the_dark_texels_deep_green_and_the_lightest_brown() {
        let litter = tint_for(GroundColoring::Classic, 21).unwrap();
        let Tint::Litter { green, brown, .. } = litter else {
            panic!("the forest floor is litter");
        };
        // A 4 by 4 grass, its texels lighter and lighter.
        let mut px: Vec<u8> = (0u8..16).flat_map(|i| [40, 100 + i * 8, 90, 255]).collect();
        apply(litter, 4, 4, &mut px);
        let hue = |i: usize| hsv(px[i * 4 + 2], px[i * 4 + 1], px[i * 4]).0;
        assert!(
            (hue(0) - green).abs() < 3.0,
            "the darkest is green: {}",
            hue(0)
        );
        assert!(
            (hue(15) - brown).abs() < 3.0,
            "the lightest is leaf brown: {}",
            hue(15)
        );
    }
}
