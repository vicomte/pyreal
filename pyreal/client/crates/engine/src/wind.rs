//! The wind: one breeze over the world from the time of day, as render-hifi's
//! weather blows it, gusting with the game clock. It bends grass, shrubs and the pack's trees,
//! and drives the water's ripples and waves.

use glam::Vec3;
use pyreal_primitives::num::math::{cosf, sinf};

/// The breeze at time of day `day` (0 to 1) and game clock `clock` seconds: x and y its push east
/// and north (1 a fresh breeze), z its gust now, 0 to 1. Its heading turns once a day; its
/// strength rises and falls three times; the gust is three slow waves of the clock.
#[must_use]
pub fn wind(day: f32, clock: f64) -> Vec3 {
    let tau = std::f32::consts::TAU;
    let strength = 0.55 + 0.25 * sinf(day * tau * 3.0);
    #[allow(clippy::cast_possible_truncation)]
    let t = (clock % 100_000.0) as f32;
    let gust = 0.5 * sinf(0.23 * t) + 0.3 * sinf(0.57 * t + 1.3) + 0.2 * sinf(1.31 * t + 0.4);
    Vec3::new(
        cosf(day * tau) * strength,
        sinf(day * tau) * strength,
        gust * 0.5 + 0.5,
    )
}
