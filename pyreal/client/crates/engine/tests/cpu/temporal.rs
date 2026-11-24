//! The frame's tail without a device: the jitter, the upscaler's choice, the render scale, the
//! dynamic resolution's controller, the drawn sizes one MetalFX scaler takes and the watch on its
//! scales.

use pyreal_engine::metalfx::{within, Watch, MOST_SCALE};
use pyreal_engine::temporal::{
    choose, halton, jitter, scaled, Can, DynamicResolution, Upscaler, JITTER_PHASES,
};

/// The jitter stays within half a pixel each way, repeats every eight frames, and its eight
/// offsets are all different (Halton 2, 3).
#[test]
fn the_jitter_is_eight_halton_offsets_within_half_a_pixel() {
    let size = (640, 360);
    let offsets: Vec<glam::Vec2> = (0..JITTER_PHASES).map(|n| jitter(n, size)).collect();
    for (n, j) in (0..JITTER_PHASES).zip(&offsets) {
        let px = glam::Vec2::new(j.x * 320.0, -j.y * 180.0);
        assert!(px.x.abs() <= 0.5 && px.y.abs() <= 0.5, "{n}: {px:?}");
        assert_eq!(*j, jitter(n + JITTER_PHASES, size), "repeats");
    }
    for a in 0..offsets.len() {
        for b in a + 1..offsets.len() {
            assert_ne!(offsets[a], offsets[b]);
        }
    }
    assert!((halton(1, 2) - 0.5).abs() < 1e-6 && (halton(2, 3) - 2.0 / 3.0).abs() < 1e-6);
}

/// MetalFX where it runs, then the temporal, then the spatial; asking for one starts there.
#[test]
fn the_first_upscaler_that_can_run_is_taken() {
    let all = Can {
        metalfx: true,
        temporal: true,
    };
    assert_eq!(choose(Upscaler::MetalFx, all), Upscaler::MetalFx);
    let off_a_mac = Can {
        metalfx: false,
        ..all
    };
    assert_eq!(choose(Upscaler::MetalFx, off_a_mac), Upscaler::Temporal);
    assert_eq!(choose(Upscaler::MetalFx, Can::default()), Upscaler::Spatial);
    assert_eq!(choose(Upscaler::Plain, all), Upscaler::Plain);
    assert_eq!(Upscaler::named(" TAA "), Some(Upscaler::Temporal));
    assert_eq!(Upscaler::named("fsr"), None);
}

/// The drawn size is the output's at the scale, rounded, never under a pixel or over the output.
#[test]
fn the_drawn_size_follows_the_render_scale() {
    assert_eq!(scaled((2560, 1440), 0.5), (1280, 720));
    assert_eq!(scaled((2560, 1440), 0.67), (1715, 965));
    assert_eq!(scaled((1280, 720), 1.0), (1280, 720));
    assert_eq!(scaled((3, 3), 0.1), (1, 1));
    assert_eq!(scaled((100, 100), 2.0), (100, 100));
}

/// Over the budget the scale drops at once, in steps, and waits for the new size's timings;
/// well under it the scale climbs back a step at a time; it never leaves its range.
#[test]
fn the_dynamic_resolution_holds_its_budget() {
    let mut d = DynamicResolution::new(8.0);
    assert_eq!(d.scale(), 1.0);
    // Twice the budget: down by about the square root of two.
    let s = d.observe(16.0);
    assert!((0.6..=0.7).contains(&s), "{s}");
    // It waits while the new size's timings come in.
    for _ in 0..d.settle {
        assert_eq!(d.observe(16.0), s);
    }
    // Far over: down to the floor.
    for _ in 0..40 {
        d.observe(100.0);
    }
    assert_eq!(d.scale(), d.min_scale);
    // Well under: back up, a step at a time, to the top and no further.
    let mut last = d.scale();
    for _ in 0..200 {
        let s = d.observe(1.0);
        assert!(s - last <= d.step + 1e-4, "{last} -> {s}");
        last = s;
    }
    assert_eq!(d.scale(), d.max_scale);
    // Unknown timings change nothing.
    assert_eq!(d.observe(0.0), d.max_scale);
}

/// A scale whose command buffer ends in an error turns MetalFX off for good, even after later
/// scales complete.
#[test]
fn a_failed_scale_turns_metalfx_off_for_good() {
    let watch = Watch::default();
    assert!(watch.ready() && !watch.failed());
    watch.begin();
    watch.end(false);
    assert!(watch.failed() && !watch.ready());
    watch.begin();
    watch.end(true);
    assert!(watch.failed() && !watch.ready());
    assert_eq!(watch.pending(), 0);
}

/// Too many unfinished scales hold MetalFX back until one completes; a stray completion never
/// counts below none.
#[test]
fn unfinished_scales_hold_metalfx_back() {
    let watch = Watch::default();
    for _ in 0..Watch::MOST_PENDING {
        assert!(watch.ready());
        watch.begin();
    }
    assert!(!watch.ready() && !watch.failed());
    watch.end(true);
    assert!(watch.ready());
    for _ in 0..Watch::MOST_PENDING + 3 {
        watch.end(true);
    }
    assert_eq!(watch.pending(), 0);
}

/// One MetalFX scaler takes every render scale from full down to half, and nothing larger than its
/// output or smaller than half.
#[test]
fn one_metalfx_scaler_takes_every_render_scale_down_to_half() {
    for output in [(1728, 1117), (3024, 1964), (2400, 1800)] {
        for step in 0..=10 {
            let scale = 1.0 - 0.05 * step as f32;
            assert!(
                within(scaled(output, scale), output, MOST_SCALE),
                "{output:?} {scale}"
            );
        }
        assert!(
            !within(scaled(output, 0.45), output, MOST_SCALE),
            "{output:?}"
        );
        assert!(
            !within((output.0 + 1, output.1), output, MOST_SCALE),
            "{output:?}"
        );
    }
    assert!(!within((0, 0), (1728, 1117), MOST_SCALE));
}
