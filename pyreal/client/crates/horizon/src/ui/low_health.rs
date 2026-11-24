//! Low Health Warning: below a share of the player's health the screen's edges redden and the
//! health bar glows, both beating like a heart that quickens as health falls.
//!
//! The reddened edges are drawn over the world and under the HUD and every window, and stay while
//! the interface is hidden: they are the warning left when the bars are. The health bar's glow is
//! the HUD's, and hides with it. Recovered health or death fades both out.

use crate::art::Image;
use crate::draw::{with_alpha, Argb, Rect};
use crate::ui::game::GameState;
use crate::ui::paint::Painter;

/// How low health must fall for the warning, as the Low Health Warning setting chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LowHealthAt {
    Off,
    Fifth,
    #[default]
    Quarter,
    Third,
}

impl LowHealthAt {
    pub const ALL: [Self; 4] = [Self::Off, Self::Fifth, Self::Quarter, Self::Third];

    /// The setting's choice as the page names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Fifth => "Under a Fifth",
            Self::Quarter => "Under a Quarter",
            Self::Third => "Under a Third",
        }
    }

    /// The settings file's word.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Fifth => "fifth",
            Self::Quarter => "quarter",
            Self::Third => "third",
        }
    }

    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.word() == word)
    }

    /// The share of health under which the warning shows, or `None` when it is off.
    #[must_use]
    pub const fn share(self) -> Option<f32> {
        match self {
            Self::Off => None,
            Self::Fifth => Some(0.2),
            Self::Quarter => Some(0.25),
            Self::Third => Some(1.0 / 3.0),
        }
    }
}

/// The cached picture of the reddened edges: white, its alpha rising toward the edges.
pub const VIGNETTE_KEY: &str = "low-health.vignette";

/// The picture's side, in texels; it is stretched over the screen.
const VIGNETTE_SIDE: u32 = 256;

/// The share of health over the threshold that health must reach to leave the warning, so a
/// regeneration tick at the line does not flicker it.
const LEAVE_MARGIN: f32 = 0.02;

/// Seconds for the warning to show, and to fade out.
const FADE_IN: f32 = 0.25;
const FADE_OUT: f32 = 0.6;

/// The heartbeat at the threshold and at no health, beats a minute: two pulses a beat, under
/// three a second at the fastest.
const BEAT_SLOW: f64 = 55.0;
const BEAT_FAST: f64 = 80.0;

/// The edges' red, and the health bar glow's.
const EDGE_RED: Argb = 0xFF8C_0A08;
const GLOW_RED: Argb = 0xFFFF_3A2E;

/// The warning across frames.
#[derive(Debug, Default)]
pub struct LowHealth {
    /// Health is under the threshold.
    danger: bool,
    /// How far under, 0 at the threshold to 1 at no health.
    depth: f32,
    /// How strongly the warning shows, 0 to 1, easing in and out.
    shown: f32,
    /// The heartbeat's place in its beat, 0 to 1.
    beat: f64,
}

impl LowHealth {
    /// Whether health is under the threshold this frame.
    #[must_use]
    pub const fn in_danger(&self) -> bool {
        self.danger
    }

    /// How strongly the warning shows, 0 to 1.
    #[must_use]
    pub const fn shown(&self) -> f32 {
        self.shown
    }

    /// Follow this frame's health `dt` seconds on, with the warning at `at`.
    pub fn step(&mut self, state: &GameState, at: LowHealthAt, dt: f64) {
        let h = state.health;
        let alive = state.in_world && h.max > 0 && h.current > 0;
        let ratio = h.ratio();
        self.danger = match at.share() {
            Some(share) if alive => ratio < share || (self.danger && ratio < share + LEAVE_MARGIN),
            _ => false,
        };
        if let (true, Some(share)) = (self.danger, at.share()) {
            self.depth = (1.0 - ratio / share).clamp(0.0, 1.0);
        }
        let dt32 = crate::ui::narrow(dt);
        self.shown = if self.danger {
            (self.shown + dt32 / FADE_IN).min(1.0)
        } else {
            (self.shown - dt32 / FADE_OUT).max(0.0)
        };
        if self.shown <= 0.0 {
            self.beat = 0.0;
            return;
        }
        let bpm = BEAT_SLOW + (BEAT_FAST - BEAT_SLOW) * f64::from(self.depth);
        self.beat = (self.beat + dt * bpm / 60.0).fract();
    }

    /// The heartbeat now, 0 to 1: a strong pulse and a weaker one after it.
    #[must_use]
    pub fn pulse(&self) -> f32 {
        let at = crate::ui::narrow(self.beat);
        bump(at, 0.1, 0.1).max(0.55 * bump(at, 0.32, 0.1))
    }

    /// Follow this frame's health and draw the reddened edges over the screen.
    pub fn frame(&mut self, p: &mut Painter<'_>, state: &GameState, at: LowHealthAt, dt: f64) {
        self.step(state, at, dt);
        self.draw_edges(p);
    }

    /// The reddened edges: deeper and further in the lower health is, beating.
    pub fn draw_edges(&self, p: &mut Painter<'_>) {
        if self.shown <= 0.0 {
            return;
        }
        let Some(mask) = p.art.cached(VIGNETTE_KEY, || Some(vignette_image())) else {
            return;
        };
        let pulse = self.pulse();
        let (sw, sh) = p.screen;
        // At the threshold the red keeps to the edges; near death it reaches well in.
        let out = (1.0 - self.depth) * 0.06 - 0.02 * pulse;
        let dst = Rect::new(
            -sw * out,
            -sh * out,
            sw * (1.0 + 2.0 * out),
            sh * (1.0 + 2.0 * out),
        );
        let a = self.shown * (0.6 + 0.3 * self.depth) * (0.8 + 0.2 * pulse);
        p.list
            .image_stretched(mask.tex, mask.src, dst, with_alpha(EDGE_RED, a));
    }

    /// The health bar `bar`'s glow, beating with the edges.
    pub fn draw_bar(&self, p: &mut Painter<'_>, bar: Rect) {
        if self.shown <= 0.0 {
            return;
        }
        let k = p.scale;
        let a = self.shown * (0.55 + 0.45 * self.pulse());
        for ring in 0..3u8 {
            let reach = f32::from(ring + 1) * 2.0 * k;
            let fade = 1.0 - f32::from(ring) / 3.0;
            frame_of(p, bar, reach, 2.0 * k, with_alpha(GLOW_RED, a * fade));
        }
    }
}

/// A ring `width` wide around `r`, `reach` out from it.
fn frame_of(p: &mut Painter<'_>, r: Rect, reach: f32, width: f32, colour: Argb) {
    let o = Rect::new(
        r.x - reach,
        r.y - reach,
        r.w + 2.0 * reach,
        r.h + 2.0 * reach,
    );
    p.list.fill(Rect::new(o.x, o.y, o.w, width), colour);
    p.list
        .fill(Rect::new(o.x, o.bottom() - width, o.w, width), colour);
    let side = o.h - 2.0 * width;
    p.list
        .fill(Rect::new(o.x, o.y + width, width, side), colour);
    p.list.fill(
        Rect::new(o.right() - width, o.y + width, width, side),
        colour,
    );
}

/// A smooth pulse at `centre`, `half` wide each side, 1 at its top.
fn bump(at: f32, centre: f32, half: f32) -> f32 {
    let t = (at - centre) / half;
    if t.abs() >= 1.0 {
        0.0
    } else {
        0.5 + 0.5 * pyreal_primitives::num::math::cosf(std::f32::consts::PI * t)
    }
}

/// The reddened edges' mask: clear in the middle, its alpha rising smoothly toward the edges and
/// most in the corners.
#[must_use]
pub fn vignette_image() -> Image {
    let side = VIGNETTE_SIDE as usize;
    let mut bgra = vec![0; side * side * 4];
    #[allow(clippy::cast_precision_loss)] // LINT-OK: within the picture's few pixels.
    let half = side as f32 / 2.0;
    for (i, px) in bgra.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)] // LINT-OK: within the picture's few pixels.
        let (x, y) = (
            ((i % side) as f32 + 0.5) / half - 1.0,
            ((i / side) as f32 + 0.5) / half - 1.0,
        );
        let r = (x * x + y * y).sqrt();
        let t = ((r - 0.55) / 0.8).clamp(0.0, 1.0);
        let a = t * t * (3.0 - 2.0 * t);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a coverage in 0..1 scaled to a byte.
        px.copy_from_slice(&[255, 255, 255, (a * 255.0).round() as u8]);
    }
    Image {
        width: VIGNETTE_SIDE,
        height: VIGNETTE_SIDE,
        bgra,
    }
}
