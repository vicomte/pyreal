//! Portal space's cost: the enhanced tunnel drawn at 4K, timed over batches of frames with the
//! device kept busy, the best batch within a few milliseconds a frame. A timing, so it runs on
//! request on an idle device (`--ignored`).

use pyreal_engine::portal::{Leg, Palette, PortalFrame, PortalSpace};
use pyreal_engine::Gpu;

#[test]
#[ignore = "a timing: run alone on an idle GPU"]
fn the_tunnel_at_4k_costs_a_few_milliseconds() {
    let gpu = Gpu::headless().expect("a GPU with the engine's features");
    let size = (3840, 2160);
    let format = wgpu::TextureFormat::Bgra8UnormSrgb;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("4k"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut space = PortalSpace::new(&gpu);
    let frame = |i: u32| {
        #[allow(clippy::cast_precision_loss)]
        let t = 2.0 + i as f32 / 60.0;
        PortalFrame {
            leg: Leg::Tunnel,
            phase: t,
            travelled: t,
            seed: 7,
            palette: Palette::PORTAL,
            from_world: false,
        }
    };
    for i in 0..20 {
        space.render(&gpu, &frame(i), None, None, &view, format, size);
    }
    gpu.wait();
    let n = 80;
    let ms = (1..=3)
        .map(|b| {
            let started = std::time::Instant::now();
            for i in 0..n {
                space.render(&gpu, &frame(b * n + i), None, None, &view, format, size);
            }
            gpu.wait();
            started.elapsed().as_secs_f64() * 1e3 / f64::from(n)
        })
        .fold(f64::INFINITY, f64::min);
    eprintln!("portal space at 4K: {ms:.2} ms a frame");
    assert!(ms < 4.0, "portal space at 4K: {ms:.2} ms a frame");
}
