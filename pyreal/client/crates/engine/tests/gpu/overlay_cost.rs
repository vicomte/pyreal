//! Not a claim: an instrument that times the interface's pass with a soft picture stretched over
//! the whole screen (the low health warning's red edges) against the same pass without it.
//!
//! - `PYREAL_OVERLAY_SIZE`: the screen (default `2560x1440`).
//! - `PYREAL_OVERLAY_FRAMES`: frames timed for each case (default 600).

use pyreal_engine::overlay::{Blend, Overlay, Sampling, Vertex};
use pyreal_engine::Gpu;

const WHITE: u64 = 1;
const MASK: u64 = 2;

/// A quad from `(x0, y0)` to `(x1, y1)` in clip space, the picture's whole face over it.
fn quad((x0, y0, x1, y1): (f32, f32, f32, f32), colour: u32) -> [Vertex; 6] {
    let v = |x: f32, y: f32, u: f32, w: f32| Vertex {
        position: [x, y, 0.0],
        colour,
        uv: [u, w],
    };
    [
        v(x0, y1, 0.0, 0.0),
        v(x1, y1, 1.0, 0.0),
        v(x1, y0, 1.0, 1.0),
        v(x0, y1, 0.0, 0.0),
        v(x1, y0, 1.0, 1.0),
        v(x0, y0, 0.0, 1.0),
    ]
}

/// A HUD's worth of small flat quads: bars, slots and a chat window's ground.
fn hud(o: &mut Overlay) {
    let mut v = Vec::new();
    for i in 0..300u16 {
        let x = -0.95 + f32::from(i % 30) * 0.06;
        let y = -0.9 + f32::from(i / 30) * 0.04;
        v.extend(quad((x, y, x + 0.05, y + 0.03), 0xC020_2020));
    }
    v.extend(quad((-0.98, -0.95, -0.4, -0.4), 0xA010_1010));
    o.push(WHITE, Blend::Image, Sampling::default(), &v, None);
}

#[test]
#[ignore = "an instrument: times the interface's pass on the device"]
fn time_a_picture_stretched_over_the_whole_screen_in_the_interfaces_pass() {
    let size = std::env::var("PYREAL_OVERLAY_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((2560u32, 1440u32));
    let frames: usize = std::env::var("PYREAL_OVERLAY_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(600);
    let gpu = Gpu::headless().expect("a GPU with the engine's features");
    let mut overlay = Overlay::new(&gpu);
    overlay.upload(&gpu, WHITE, 1, 1, &[vec![255, 255, 255, 255]]);
    // A 256-texel mask, as the warning's: white, its alpha rising toward the edges.
    let side = 256usize;
    let mask: Vec<u8> = (0..side * side)
        .flat_map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = (
                (i % side) as f32 / 128.0 - 1.0,
                (i / side) as f32 / 128.0 - 1.0,
            );
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let a = (((x * x + y * y).sqrt() - 0.55) / 0.8).clamp(0.0, 1.0) * 255.0;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            [255, 255, 255, a as u8]
        })
        .collect();
    overlay.upload(&gpu, MASK, 256, 256, &[mask]);
    // None, the warning's one quad (a little past the screen), and eight to see one above the
    // noise.
    let cases = [0usize, 1, 8];
    let mut times: Vec<Vec<f64>> = vec![Vec::new(); cases.len()];
    for frame in 0..frames + 60 {
        for (c, &n) in cases.iter().enumerate() {
            let start = std::time::Instant::now();
            overlay.begin();
            hud(&mut overlay);
            for _ in 0..n {
                overlay.push(
                    MASK,
                    Blend::Image,
                    Sampling::default(),
                    &quad((-1.2, -1.2, 1.2, 1.2), 0xA08C_0A08),
                    None,
                );
            }
            let _ = overlay.finish(&gpu, size);
            gpu.wait();
            if frame >= 60 {
                times[c].push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
    }
    let median = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    let m: Vec<f64> = times.iter_mut().map(median).collect();
    println!(
        "interface pass at {}x{}, median ms over {frames} frames (submit to done):",
        size.0, size.1
    );
    for (n, ms) in cases.iter().zip(&m) {
        println!("  {n} stretched quads: {ms:.4}");
    }
    let each = (m[2] - m[0]) / 8.0;
    println!(
        "  added by one: {:.4} ms; by each of eight: {each:.4} ms",
        m[1] - m[0]
    );
}
