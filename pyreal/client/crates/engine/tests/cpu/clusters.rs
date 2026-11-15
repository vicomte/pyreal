//! The light clusters cover the whole target at any size: every pixel's tile is one the clusters
//! hold, past the 4096x2304 the buffers first hold.

use pyreal_engine::renderer::{cluster_bytes, cluster_tiles_for, CLUSTER_TILE, CLUSTER_TILES};

#[test]
fn every_pixel_has_its_cluster_at_5k_and_6k() {
    for (w, h) in [(1280, 720), (4096, 2304), (5120, 2880), (6016, 3384)] {
        let (x, y) = cluster_tiles_for(w, h);
        // The last pixel's tile, as `lamp_light.wgsl`'s `cluster_of` finds it.
        assert!(
            (w - 1) / CLUSTER_TILE < x && (h - 1) / CLUSTER_TILE < y,
            "{w}x{h}: {x}x{y}"
        );
        let (counts, lamps) = cluster_bytes(x * y);
        let (c0, l0) = cluster_bytes(CLUSTER_TILES.0 * CLUSTER_TILES.1);
        eprintln!(
            "{w}x{h}: {x}x{y} tiles, {:.1} MB of clusters ({:+.1} MB on the first size)",
            (counts + lamps) as f64 / 1e6,
            (counts + lamps) as f64 / 1e6 - (c0 + l0) as f64 / 1e6
        );
    }
}
