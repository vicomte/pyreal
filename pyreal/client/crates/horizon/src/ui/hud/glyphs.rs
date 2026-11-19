//! Small one-colour glyphs the HUD draws itself, as polygons in a unit box filled a pixel row at a
//! time: the stance button's dove for peace, axe across a shield for melee, bow and arrow for
//! missile, and wand and star for magic; the AI fellow modes' sword, shield, target and cross
//! (and the dove for passive); and a Mac's modifier signs, drawn as strokes.

use crate::draw::{Argb, Rect};
use crate::ui::paint::Painter;

/// How a glyph's polygon is filled: light, dark (the cutouts and detail), or fine detail light or
/// dark that is left out when the glyph is drawn too small to show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Light,
    Dark,
    Fine(bool),
}

/// One glyph: polygons drawn in order, each with its tone.
pub type Glyph = &'static [(&'static [(f32, f32)], Tone)];

/// The smallest a glyph is drawn with its fine detail, in pixels.
const FINE_SIDE: f32 = 28.0;

/// Peace: a dove in flight, facing right, its near wing raised.
pub const DOVE: Glyph = &[
    // The far wing, behind.
    (
        &[
            (0.50, 0.50),
            (0.56, 0.22),
            (0.66, 0.06),
            (0.68, 0.30),
            (0.62, 0.50),
        ],
        Tone::Light,
    ),
    // Body, head and tail.
    (
        &[
            (0.02, 0.60),
            (0.20, 0.56),
            (0.40, 0.52),
            (0.64, 0.50),
            (0.72, 0.38),
            (0.81, 0.32),
            (0.89, 0.35),
            (0.99, 0.43),
            (0.89, 0.47),
            (0.85, 0.56),
            (0.80, 0.68),
            (0.64, 0.78),
            (0.42, 0.79),
            (0.24, 0.73),
            (0.04, 0.82),
            (0.12, 0.68),
        ],
        Tone::Light,
    ),
    // The near wing, raised.
    (
        &[
            (0.30, 0.58),
            (0.20, 0.34),
            (0.06, 0.04),
            (0.30, 0.14),
            (0.46, 0.28),
            (0.60, 0.54),
        ],
        Tone::Light,
    ),
    // The eye.
    (
        &[(0.82, 0.39), (0.86, 0.39), (0.86, 0.43), (0.82, 0.43)],
        Tone::Dark,
    ),
];

/// A fighting stance: an axe crossing a shield, the shield cut back round it.
pub const AXE_AND_SHIELD: Glyph = &[
    // The shield, with a dark band round a raised middle when there is room for it.
    (
        &[
            (0.02, 0.18),
            (0.58, 0.18),
            (0.58, 0.50),
            (0.52, 0.70),
            (0.30, 0.97),
            (0.08, 0.70),
            (0.02, 0.50),
        ],
        Tone::Light,
    ),
    (
        &[
            (0.08, 0.24),
            (0.52, 0.24),
            (0.52, 0.50),
            (0.48, 0.66),
            (0.30, 0.88),
            (0.12, 0.66),
            (0.08, 0.50),
        ],
        Tone::Fine(false),
    ),
    (
        &[
            (0.13, 0.30),
            (0.47, 0.30),
            (0.47, 0.50),
            (0.44, 0.62),
            (0.30, 0.79),
            (0.16, 0.62),
            (0.13, 0.50),
        ],
        Tone::Fine(true),
    ),
    // The axe's cutout: the shield cut back round it where it crosses.
    (
        &[(0.06, 0.78), (0.60, 0.24), (0.72, 0.36), (0.18, 0.90)],
        Tone::Dark,
    ),
    (
        &[
            (0.61, 0.29),
            (0.75, 0.36),
            (0.93, 0.38),
            (0.90, 0.52),
            (0.83, 0.63),
            (0.73, 0.69),
            (0.59, 0.73),
            (0.56, 0.55),
            (0.49, 0.41),
        ],
        Tone::Dark,
    ),
    // The haft, steep across the shield, its head clear of it.
    (
        &[(0.07, 0.84), (0.66, 0.25), (0.71, 0.30), (0.12, 0.89)],
        Tone::Light,
    ),
    // The head's blade, flaring to two horns round its curved edge.
    (
        &[
            (0.63, 0.33),
            (0.74, 0.38),
            (0.89, 0.41),
            (0.87, 0.52),
            (0.81, 0.60),
            (0.72, 0.66),
            (0.61, 0.69),
            (0.59, 0.54),
            (0.53, 0.43),
        ],
        Tone::Light,
    ),
];

/// The missile stance: a drawn bow, its arrow pointing up and to the right.
pub const BOW_AND_ARROW: Glyph = &[
    // The bow, its belly toward the mark.
    (
        &[
            (0.10, 0.28),
            (0.20, 0.27),
            (0.30, 0.28),
            (0.41, 0.31),
            (0.50, 0.36),
            (0.58, 0.42),
            (0.64, 0.50),
            (0.69, 0.59),
            (0.72, 0.70),
            (0.73, 0.80),
            (0.72, 0.90),
            (0.64, 0.88),
            (0.65, 0.80),
            (0.64, 0.71),
            (0.62, 0.63),
            (0.57, 0.55),
            (0.52, 0.48),
            (0.45, 0.43),
            (0.37, 0.38),
            (0.29, 0.36),
            (0.20, 0.35),
            (0.12, 0.36),
        ],
        Tone::Light,
    ),
    // The string, drawn back to the nock.
    (
        &[(0.09, 0.33), (0.30, 0.69), (0.34, 0.67), (0.12, 0.31)],
        Tone::Light,
    ),
    (
        &[(0.31, 0.70), (0.67, 0.91), (0.69, 0.88), (0.33, 0.66)],
        Tone::Light,
    ),
    // The arrow: its shaft, its head and its fletching.
    (
        &[(0.31, 0.73), (0.76, 0.29), (0.71, 0.24), (0.27, 0.69)],
        Tone::Light,
    ),
    (&[(0.63, 0.24), (0.83, 0.17), (0.76, 0.37)], Tone::Light),
    (
        &[(0.29, 0.71), (0.37, 0.63), (0.26, 0.60), (0.19, 0.67)],
        Tone::Light,
    ),
    (
        &[(0.29, 0.71), (0.37, 0.63), (0.40, 0.74), (0.33, 0.81)],
        Tone::Light,
    ),
];

/// The magic stance: a wand, a star at its tip.
pub const WAND_AND_STAR: Glyph = &[
    // The wand.
    (
        &[(0.11, 0.97), (0.55, 0.53), (0.49, 0.47), (0.05, 0.91)],
        Tone::Light,
    ),
    // The star at its tip, cut clear of it.
    (
        &[
            (0.66, 0.00),
            (0.73, 0.27),
            (1.00, 0.34),
            (0.73, 0.41),
            (0.66, 0.70),
            (0.59, 0.41),
            (0.30, 0.34),
            (0.59, 0.27),
        ],
        Tone::Light,
    ),
    (
        &[
            (0.71, 0.29),
            (0.73, 0.34),
            (0.71, 0.39),
            (0.66, 0.41),
            (0.61, 0.39),
            (0.59, 0.34),
            (0.61, 0.29),
            (0.66, 0.27),
        ],
        Tone::Dark,
    ),
    // A small spark beside it.
    (
        &[
            (0.20, 0.07),
            (0.23, 0.19),
            (0.35, 0.22),
            (0.23, 0.25),
            (0.20, 0.37),
            (0.17, 0.25),
            (0.05, 0.22),
            (0.17, 0.19),
        ],
        Tone::Light,
    ),
    (
        &[
            (0.88, 0.72),
            (0.90, 0.80),
            (0.98, 0.82),
            (0.90, 0.84),
            (0.88, 0.92),
            (0.86, 0.84),
            (0.78, 0.82),
            (0.86, 0.80),
        ],
        Tone::Light,
    ),
];

/// Aggressive: a sword, point up.
pub const SWORD: Glyph = &[
    (
        &[
            (0.42, 0.10),
            (0.50, 0.00),
            (0.58, 0.10),
            (0.58, 0.62),
            (0.42, 0.62),
        ],
        Tone::Light,
    ),
    (
        &[(0.20, 0.62), (0.80, 0.62), (0.80, 0.73), (0.20, 0.73)],
        Tone::Light,
    ),
    (
        &[(0.44, 0.73), (0.56, 0.73), (0.56, 0.88), (0.44, 0.88)],
        Tone::Light,
    ),
    (
        &[(0.38, 0.88), (0.62, 0.88), (0.62, 0.99), (0.38, 0.99)],
        Tone::Light,
    ),
];

/// Defensive: a shield.
pub const SHIELD: Glyph = &[
    (
        &[
            (0.14, 0.12),
            (0.50, 0.02),
            (0.86, 0.12),
            (0.86, 0.48),
            (0.74, 0.74),
            (0.50, 0.98),
            (0.26, 0.74),
            (0.14, 0.48),
        ],
        Tone::Light,
    ),
    (
        &[(0.46, 0.16), (0.54, 0.16), (0.54, 0.84), (0.46, 0.84)],
        Tone::Fine(false),
    ),
];

/// Assist: a target, rings round a mark.
pub const TARGET: Glyph = &[
    (
        &[
            (0.925, 0.676),
            (0.676, 0.925),
            (0.324, 0.925),
            (0.075, 0.676),
            (0.075, 0.324),
            (0.324, 0.075),
            (0.676, 0.075),
            (0.925, 0.324),
        ],
        Tone::Light,
    ),
    (
        &[
            (0.777, 0.615),
            (0.615, 0.777),
            (0.385, 0.777),
            (0.223, 0.615),
            (0.223, 0.385),
            (0.385, 0.223),
            (0.615, 0.223),
            (0.777, 0.385),
        ],
        Tone::Dark,
    ),
    (
        &[
            (0.64, 0.56),
            (0.56, 0.64),
            (0.44, 0.64),
            (0.36, 0.56),
            (0.36, 0.44),
            (0.44, 0.36),
            (0.56, 0.36),
            (0.64, 0.44),
        ],
        Tone::Light,
    ),
];

/// Heal: a cross.
pub const CROSS: Glyph = &[(
    &[
        (0.36, 0.06),
        (0.64, 0.06),
        (0.64, 0.36),
        (0.94, 0.36),
        (0.94, 0.64),
        (0.64, 0.64),
        (0.64, 0.94),
        (0.36, 0.94),
        (0.36, 0.64),
        (0.06, 0.64),
        (0.06, 0.36),
        (0.36, 0.36),
    ],
    Tone::Light,
)];

/// Lines drawn a stroke wide, each a run of points in a unit box.
pub type Strokes = &'static [&'static [(f32, f32)]];

/// The Option key's sign: a bar, a fall to the lower right, and a bar apart at the upper right.
pub const OPTION_KEY: Strokes = &[
    &[(0.02, 0.24), (0.34, 0.24), (0.64, 0.80), (0.98, 0.80)],
    &[(0.58, 0.24), (0.98, 0.24)],
];

/// The Control key's sign: a caret.
pub const CONTROL_KEY: Strokes = &[&[(0.10, 0.68), (0.50, 0.26), (0.90, 0.68)]];

/// The Shift key's sign: a hollow arrow pointing up.
pub const SHIFT_KEY: Strokes = &[&[
    (0.50, 0.06),
    (0.94, 0.50),
    (0.70, 0.50),
    (0.70, 0.94),
    (0.30, 0.94),
    (0.30, 0.50),
    (0.06, 0.50),
    (0.50, 0.06),
]];

/// The Command key's sign: a square with a loop at each corner.
pub const COMMAND_KEY: Strokes = &[
    &[
        (0.34, 0.34),
        (0.66, 0.34),
        (0.66, 0.66),
        (0.34, 0.66),
        (0.34, 0.34),
    ],
    &[
        (0.34, 0.34),
        (0.34, 0.08),
        (0.08, 0.08),
        (0.08, 0.34),
        (0.34, 0.34),
    ],
    &[
        (0.66, 0.34),
        (0.66, 0.08),
        (0.92, 0.08),
        (0.92, 0.34),
        (0.66, 0.34),
    ],
    &[
        (0.34, 0.66),
        (0.34, 0.92),
        (0.08, 0.92),
        (0.08, 0.66),
        (0.34, 0.66),
    ],
    &[
        (0.66, 0.66),
        (0.66, 0.92),
        (0.92, 0.92),
        (0.92, 0.66),
        (0.66, 0.66),
    ],
];

/// Draw `strokes` over the square `r` in `colour`, a pixel or a tenth of the square wide, each
/// segment carried past its ends by half its width so corners close.
pub fn draw_strokes(p: &mut Painter<'_>, strokes: Strokes, r: Rect, colour: Argb) {
    let side = r.w.min(r.h);
    let w = (side * 0.1).max(1.0) / 2.0;
    let at = |(x, y): (f32, f32)| (r.x + x * side, r.y + y * side);
    for line in strokes {
        for pair in line.windows(2) {
            let (a, b) = (at(pair[0]), at(pair[1]));
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
            let (ux, uy) = (dx / len * w, dy / len * w);
            let quad = [
                (a.0 - ux - uy, a.1 - uy + ux),
                (b.0 + ux - uy, b.1 + uy + ux),
                (b.0 + ux + uy, b.1 + uy - ux),
                (a.0 - ux + uy, a.1 - uy - ux),
            ];
            fill(p, &quad, colour);
        }
    }
}

/// Draw `glyph` over `r` (a square fits it best): its light parts in `colour`, its dark parts and
/// a one-pixel edge round it in `dark`.
pub fn draw(p: &mut Painter<'_>, glyph: Glyph, r: Rect, colour: Argb, dark: Argb) {
    let side = r.w.min(r.h);
    let x0 = r.x + (r.w - side) / 2.0;
    let y0 = r.y + (r.h - side) / 2.0;
    let at = |pts: &[(f32, f32)], dx: f32, dy: f32| -> Vec<(f32, f32)> {
        pts.iter()
            .map(|(x, y)| (x0 + x * side + dx, y0 + y * side + dy))
            .collect()
    };
    let fine = side >= FINE_SIDE;
    for (pts, _) in glyph.iter().filter(|(_, tone)| *tone == Tone::Light) {
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
            fill(p, &at(pts, dx, dy), dark);
        }
    }
    for (pts, tone) in glyph {
        let light = match tone {
            Tone::Light => true,
            Tone::Dark => false,
            Tone::Fine(light) if fine => *light,
            Tone::Fine(_) => continue,
        };
        fill(p, &at(pts, 0.0, 0.0), if light { colour } else { dark });
    }
}

/// Fill the polygon `pts` (screen pixels), a pixel row at a time, sampled at each row's middle.
fn fill(p: &mut Painter<'_>, pts: &[(f32, f32)], colour: Argb) {
    let top = pts.iter().map(|q| q.1).fold(f32::MAX, f32::min).floor();
    let bottom = pts.iter().map(|q| q.1).fold(f32::MIN, f32::max).ceil();
    let mut y = top;
    while y < bottom {
        let sy = y + 0.5;
        let mut xs: Vec<f32> = Vec::new();
        for (i, a) in pts.iter().enumerate() {
            let b = pts[(i + 1) % pts.len()];
            if (a.1 <= sy) != (b.1 <= sy) {
                xs.push(a.0 + (sy - a.1) / (b.1 - a.1) * (b.0 - a.0));
            }
        }
        xs.sort_by(f32::total_cmp);
        for [l, r] in xs.as_chunks::<2>().0 {
            let (l, r) = (l.round(), r.round());
            if r > l {
                p.fill(Rect::new(l, y, r - l, 1.0), colour);
            }
        }
        y += 1.0;
    }
}
