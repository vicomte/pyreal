//! The water's time base: waves and drifts run in real seconds, at deep water's speed, along
//! headings fixed in the world, whatever the hour or how far render space lies from the world's
//! corner.

use glam::Vec2;
use pyreal_engine::water::{
    drift, frame_constants, wave_constants, CALM_PACE, CHOP, HEADINGS, PER_HEADING, RIPPLES,
    WAVES_BYTES, WAVE_COUNT,
};

const TAU: f64 = std::f64::consts::TAU;

/// A wave's phase at render-space `p`, radians.
fn phase(w: [f32; 4], p: Vec2) -> f64 {
    (f64::from(w[0]) * f64::from(p.x) + f64::from(w[1]) * f64::from(p.y) + f64::from(w[3]))
        .rem_euclid(TAU)
}

#[test]
fn each_wave_moves_at_deep_waters_speed_in_real_seconds() {
    // Far from the world's corner, as the eastern beaches lie, many hours into a session.
    let global = Vec2::new(42_624.0, 24_768.0);
    let clock = 5.0 * 3600.0;
    let now = wave_constants(clock, global);
    let later = wave_constants(clock + 0.25, global);
    let p = Vec2::new(30.0, 70.0);
    for k in 0..WAVE_COUNT {
        assert_eq!(
            now[k][..3],
            later[k][..3],
            "wave {k} keeps its heading and steepness"
        );
        let kk = f64::from(Vec2::new(now[k][0], now[k][1]).length());
        // A pond's chop runs slower.
        let pace = if k >= CHOP + PER_HEADING * HEADINGS {
            CALM_PACE
        } else {
            1.0
        };
        let omega = (9.81 * kk).sqrt() * pace;
        // A quarter of a second on, the phase has gone back by a quarter of omega.
        let moved = (phase(now[k], p) - phase(later[k], p)).rem_euclid(TAU);
        assert!(
            (moved - omega * 0.25).abs() < 2e-3,
            "wave {k}: {moved} rad in 0.25 s, deep water's {}",
            omega * 0.25
        );
        // Its crests move at c = omega / k: ten metres a second at most, the long swell's.
        assert!(omega / kk < 10.5, "wave {k} runs at {} m/s", omega / kk);
    }
}

#[test]
fn ripples_drift_slowly_along_a_fixed_heading() {
    for &(cell, speed) in &RIPPLES {
        let at = |clock: f64| {
            let d = drift(clock, speed, cell);
            let whole = |v: f32| f64::from(v.to_bits().cast_signed());
            #[allow(clippy::cast_possible_truncation)] // metres within a few cells
            Vec2::new(
                (whole(d[2]) * f64::from(cell) + f64::from(d[0])) as f32,
                (whole(d[3]) * f64::from(cell) + f64::from(d[1])) as f32,
            )
        };
        let a = at(10_000.0);
        let b = at(10_001.0);
        let moved = (b - a).length();
        assert!(
            (f64::from(moved) - speed).abs() < 1e-3,
            "{moved} m in a second, not {speed}"
        );
        assert!(speed < 0.2, "ripples drift at a leisurely pace");
        // The same heading an hour later.
        let c = at(13_600.0);
        let d = at(13_601.0);
        assert!(((d - c).normalize() - (b - a).normalize()).length() < 1e-3);
    }
    assert_eq!(frame_constants(100.0, Vec2::ZERO).len() as u64, WAVES_BYTES);
}

/// The swash runs up the shore as each surf line reaches it: the frame's share of the surf's
/// period (which the lighting's wet ground reads) is the water pass's, and rolls round once a
/// period in real seconds.
#[test]
fn the_swash_keeps_the_surfs_time() {
    use pyreal_engine::water::{surf_phase, SURF_PERIOD};
    let clock = 5.0 * 3600.0 + 1.3;
    let c = frame_constants(clock, Vec2::ZERO);
    let at = WAVE_COUNT * 16;
    let surf = f32::from_le_bytes(c[at..at + 4].try_into().expect("four bytes"));
    assert_eq!(surf, surf_phase(clock));
    let quarter = surf_phase(clock + SURF_PERIOD * 0.25);
    assert!(
        ((quarter - surf).rem_euclid(1.0) - 0.25).abs() < 1e-4,
        "{surf} {quarter}"
    );
}
