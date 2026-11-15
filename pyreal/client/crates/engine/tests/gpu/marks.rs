//! The interface's triangles that stand in the world are drawn against the world's depth: hidden
//! wherever the world stands nearer, under what the interface drew over them, and drawn whole
//! when nearer than everything.

use glam::Vec3;
use pyreal_engine::overlay::{Blend, Overlay, Sampling, Vertex};
use pyreal_engine::world::{ground_height, load_area, Area, AssetLoader};
use pyreal_engine::{Camera, SkyLight};

const WHITE: u64 = 1;

/// A quad over the whole screen at `distance` metres, in `colour`.
fn whole(distance: f32, colour: u32) -> Vec<Vertex> {
    let v = |x: f32, y: f32| Vertex {
        position: [x, y, distance],
        colour,
        uv: [0.5, 0.5],
    };
    vec![
        v(-1.0, 1.0),
        v(1.0, 1.0),
        v(1.0, -1.0),
        v(-1.0, 1.0),
        v(1.0, -1.0),
        v(-1.0, -1.0),
    ]
}

/// The share of the UI picture's pixels with any alpha, and of them those of colour `red`.
fn coverage(overlay: &Overlay, engine: &pyreal_engine::Engine) -> (f64, f64) {
    let rgba = pyreal_engine::read_texture(&engine.gpu, overlay.texture().expect("the picture"), 4);
    let px: Vec<&[u8; 4]> = rgba.as_chunks::<4>().0.iter().collect();
    let drawn = px.iter().filter(|p| p[3] > 0).count();
    let red = px
        .iter()
        .filter(|p| p[3] > 0 && p[0] > 200 && p[1] < 60)
        .count();
    #[allow(clippy::cast_precision_loss)]
    (
        drawn as f64 / px.len() as f64,
        red as f64 / drawn.max(1) as f64,
    )
}

#[test]
fn marks_in_the_world_hide_behind_it() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = super::engine();
    let mut loader = AssetLoader::default();
    load_area(
        &mut engine,
        &mut loader,
        &store,
        &region,
        Area {
            center: 0xA9B4,
            radius: 1,
        },
    );
    let z = ground_height(&store, &region, 0xA9B4, 96.0, 60.0).expect("the ground") + 2.0;
    // Looking a little down over the town: ground below the horizon, sky above it.
    let camera = Camera {
        position: Vec3::new(96.0, 60.0, z),
        pitch: -0.05,
        far: 900.0,
        ..Camera::default()
    };
    let light = SkyLight::at(&region, 0, 0.5);
    let size = (256, 144);
    let mut overlay = Overlay::new(&engine.gpu);
    overlay.upload(&engine.gpu, WHITE, 1, 1, &[vec![255, 255, 255, 255]]);
    let target = engine.gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("output"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut frame = |overlay: &mut Overlay, items: &dyn Fn(&mut Overlay)| {
        overlay.begin();
        items(overlay);
        engine.renderer.ui = Some(overlay.finish(&engine.gpu, size));
        engine
            .render_with(&camera, &light, &view, size, Some(&*overlay))
            .expect("a frame");
        engine.gpu.wait();
        coverage(overlay, &engine)
    };
    let s = Sampling::default();
    // A mark far behind everything shows only where the sky is.
    let (far, _) = frame(&mut overlay, &|o| {
        o.push_world(WHITE, Blend::Image, s, &whole(5000.0, 0xFFFF_0000), None);
    });
    // One nearer than everything shows everywhere; so does one asked to stay over the world.
    let (near, _) = frame(&mut overlay, &|o| {
        o.push_world(WHITE, Blend::Image, s, &whole(0.2, 0xFFFF_0000), None);
    });
    let (over, _) = frame(&mut overlay, &|o| {
        o.push(WHITE, Blend::Image, s, &whole(0.0, 0xFFFF_0000), None);
    });
    eprintln!("far {far:.3} near {near:.3} over {over:.3}");
    assert!(near > 0.999 && over > 0.999, "near {near} over {over}");
    assert!(
        far > 0.05 && far < 0.8,
        "the far mark shows only over the sky: {far}"
    );
    // Under the interface: a mark nearer than everything, with the interface's own green over
    // the whole screen drawn after it, stays under the green.
    let (_, red) = frame(&mut overlay, &|o| {
        o.push(WHITE, Blend::Image, s, &whole(0.0, 0xFF00_FF00), None);
        o.push_world(WHITE, Blend::Image, s, &whole(0.2, 0xFFFF_0000), None);
    });
    assert!(red < 0.01, "the world's mark is under the interface: {red}");
    // Within one batch the later triangles stay on top, as a name's coloured letters over its
    // dark outline.
    let (_, red) = frame(&mut overlay, &|o| {
        let mut both = whole(0.2, 0xFF00_0000);
        both.extend(whole(0.2, 0xFFFF_0000));
        o.push_world(WHITE, Blend::Image, s, &both, None);
    });
    assert!(red > 0.99, "the later triangles are on top: {red}");
}
