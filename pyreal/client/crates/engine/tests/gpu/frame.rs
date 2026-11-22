//! The frame's tail on the GPU: the temporal anti-aliasing settling on a still scene, the dynamic
//! resolution under a tight budget, picking by object, and the UI laid over at the output's size.

use std::sync::Arc;

use glam::{Affine3A, Vec3};
use pyreal_engine::body::Body;
use pyreal_engine::temporal::{DynamicResolution, Ui, Upscaler};
use pyreal_engine::world::AssetLoader;
use pyreal_engine::{Camera, Engine, SkyLight};
use pyreal_primitives::{DataId, Frame, Quat, Vec3 as DVec3};
use pyreal_world_data::anim_assets::DatAnimAssets;

use super::engine;

/// A drudge, which moves by motion table 0x09000008.
const DRUDGE: DataId = DataId(0x0200_07DD);
/// A Holtburg door: a static object.
const DOOR: DataId = DataId(0x0200_19FF);

/// A door on the left and a drudge on the right, three metres off, seen from a metre up; the
/// drudge's body.
fn door_and_drudge(engine: &mut Engine) -> Body {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let mut loader = AssetLoader::default();
    loader.place_object(
        engine,
        &store,
        DOOR,
        Affine3A::from_translation(Vec3::new(-1.2, 4.0, 0.0)),
        false,
    );
    let mut body = Body::new(
        Arc::new(DatAnimAssets::new(Arc::clone(&store))),
        DRUDGE,
        None,
        Frame::new(DVec3::new(1.0, 4.0, 0.0), Quat::IDENTITY),
    )
    .expect("a drudge");
    assert!(body.place(engine, &mut loader, &store) > 5);
    body
}

fn camera() -> Camera {
    Camera {
        position: Vec3::new(0.0, 0.0, 1.0),
        far: 100.0,
        ..Camera::default()
    }
}

/// Where world point `p` falls in an output of `size`, pixels.
fn pixel_of(camera: &Camera, size: (u32, u32), p: Vec3) -> (u32, u32) {
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (size.0 as f32, size.1 as f32);
    let c = camera.view_projection(w / h) * p.extend(1.0);
    let ndc = c.truncate() / c.w;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (
        ((ndc.x * 0.5 + 0.5) * w) as u32,
        ((0.5 - ndc.y * 0.5) * h) as u32,
    )
}

/// The mean absolute difference of two pictures, in output levels.
#[allow(clippy::cast_precision_loss)]
fn mean_difference(a: &[u8], b: &[u8]) -> f64 {
    let sum: u64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .sum();
    sum as f64 / a.len() as f64
}

/// The object under a pixel is the object drawn there: the drudge's body at its middle, the door
/// (another object) at its, nothing in the sky; at half the render scale too, where the output's
/// pixel is mapped to the drawn one.
#[test]
fn picking_finds_the_object_drawn_under_a_pixel() {
    let mut engine = engine();
    let body = door_and_drudge(&mut engine);
    let camera = camera();
    let light = SkyLight::default();
    let size = (320, 180);
    for scale in [1.0, 0.5] {
        engine.renderer.render_scale = scale;
        engine
            .render_to_rgba(&camera, &light, size)
            .expect("a frame");
        let drudge = pixel_of(&camera, size, Vec3::new(1.0, 4.0, 0.9));
        let door = pixel_of(&camera, size, Vec3::new(-1.2, 4.0, 1.0));
        assert_eq!(
            engine.renderer.pick(&engine.gpu, drudge),
            Some(body.object),
            "the drudge at {drudge:?}, scale {scale}"
        );
        let other = engine.renderer.pick(&engine.gpu, door);
        assert!(
            other.is_some() && other != Some(body.object),
            "the door at {door:?}: {other:?}"
        );
        assert_eq!(engine.renderer.pick(&engine.gpu, (2, 2)), None, "the sky");
    }
}

/// A pick reads how far the drawn surface under the pixel is: the drudge's front a little short of
/// its middle, at full and half render scale, and nothing for the sky.
#[test]
fn picking_reads_the_drawn_surfaces_distance() {
    let mut engine = engine();
    let body = door_and_drudge(&mut engine);
    let camera = camera();
    let size = (320, 180);
    let middle = Vec3::new(1.0, 4.0, 0.9);
    let far = middle.distance(camera.position);
    for scale in [1.0, 0.5] {
        engine.renderer.render_scale = scale;
        engine
            .render_to_rgba(&camera, &SkyLight::default(), size)
            .expect("a frame");
        let p = engine
            .renderer
            .pick_texel(&engine.gpu, pixel_of(&camera, size, middle));
        assert_eq!(p.object, Some(body.object));
        let d = p.distance.expect("a surface");
        assert!(
            d > far - 0.6 && d < far + 0.1,
            "{d} m to the drudge, {far} m to its middle"
        );
        let sky = engine.renderer.pick_texel(&engine.gpu, (2, 2));
        assert_eq!((sky.object, sky.distance), (None, None), "the sky");
    }
}

/// The olthoi door (weenie 5627's setup): opaque jambs and lintel round ten translucent leaves.
const OLTHOI_DOOR: DataId = DataId(0x0200_05F2);
/// ...and its motion table, which closes it.
const OLTHOI_DOOR_MOTIONS: DataId = DataId(0x0900_0087);

/// A click on a door's translucent leaves picks the door, as retail's polygon test does: the
/// leaves write the object under them, nearest last. Objects below `pick_from` leave the pick to
/// what is behind their translucent surfaces (here the sky).
#[test]
fn picking_finds_an_object_by_its_translucent_surfaces() {
    let mut engine = engine();
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let mut loader = AssetLoader::default();
    let mut body = Body::new(
        Arc::new(DatAnimAssets::new(Arc::clone(&store))),
        OLTHOI_DOOR,
        None,
        Frame::new(DVec3::new(0.0, 4.0, 0.0), Quat::IDENTITY),
    )
    .expect("the olthoi door");
    assert!(body.driver.set_motion_table(OLTHOI_DOOR_MOTIONS));
    body.advance(0.5);
    body.driver.update_parts(&body.frame);
    assert_eq!(body.place(&mut engine, &mut loader, &store), 13);
    let camera = Camera {
        position: Vec3::new(0.0, -4.0, 1.6),
        far: 100.0,
        ..Camera::default()
    };
    let light = SkyLight::default();
    let size = (320, 320);
    let leaves = [(0.0, 1.6), (0.4, 2.0), (-0.4, 1.2)]
        .map(|(x, z)| pixel_of(&camera, size, Vec3::new(x, 4.0, z)));
    let jamb = pixel_of(&camera, size, Vec3::new(-1.95, 4.0, 1.5));
    engine
        .render_to_rgba(&camera, &light, size)
        .expect("a frame");
    for at in leaves {
        assert_eq!(
            engine.renderer.pick(&engine.gpu, at),
            Some(body.object),
            "the leaves at {at:?}"
        );
    }
    assert_eq!(engine.renderer.pick(&engine.gpu, jamb), Some(body.object));
    engine.renderer.pick_from = body.object + 1;
    engine
        .render_to_rgba(&camera, &light, size)
        .expect("a frame");
    assert_eq!(engine.renderer.pick(&engine.gpu, leaves[0]), None);
    assert_eq!(engine.renderer.pick(&engine.gpu, jamb), Some(body.object));
}

/// With the temporal anti-aliasing on a still scene, consecutive frames settle: once the history
/// holds the jitter's offsets, two frames differ by far less than two frames drawn before it did,
/// and the settled frame differs from the unjittered one only at edges.
#[test]
fn the_temporal_anti_aliasing_settles_on_a_still_scene() {
    let mut engine = engine();
    door_and_drudge(&mut engine);
    engine.renderer.anti_aliasing = true;
    engine.renderer.upscaler = Upscaler::Temporal;
    let camera = camera();
    let light = SkyLight::default();
    engine.renderer.frame_seconds = Some(1.0 / 60.0);
    let size = (320, 180);
    let frames: Vec<Vec<u8>> = (0..40)
        .map(|_| {
            engine
                .render_to_rgba(&camera, &light, size)
                .expect("a frame")
        })
        .collect();
    assert_eq!(engine.renderer.temporal.used, Some(Upscaler::Temporal));
    let early = mean_difference(&frames[0], &frames[1]);
    let late = mean_difference(&frames[38], &frames[39]);
    assert!(late < 0.1, "settled frames differ by {late} levels");
    assert!(late < early * 0.25, "early {early}, late {late}");
    engine.renderer.anti_aliasing = false;
    let plain = engine
        .render_to_rgba(&camera, &light, size)
        .expect("a frame");
    let edges = mean_difference(&plain, &frames[39]);
    assert!(edges > 0.0 && edges < 3.0, "{edges} levels from unjittered");
}

/// Under a budget no frame can meet the drawn size falls to the controller's floor, and the
/// output keeps its size.
#[test]
fn the_dynamic_resolution_lowers_the_drawn_size_under_a_tight_budget() {
    let mut engine = engine();
    door_and_drudge(&mut engine);
    if !engine.gpu.has(wgpu::Features::TIMESTAMP_QUERY) {
        return;
    }
    let mut d = DynamicResolution::new(0.001);
    d.settle = 1;
    engine.renderer.dynamic = Some(d);
    let size = (320, 180);
    for _ in 0..40 {
        let rgba = engine
            .render_to_rgba(&camera(), &SkyLight::default(), size)
            .expect("a frame");
        assert_eq!(rgba.len(), 320 * 180 * 4);
        engine.renderer.collect_timings(&engine.gpu, true);
    }
    let floor = engine.renderer.dynamic.map(|d| d.min_scale);
    assert_eq!(engine.renderer.dynamic.map(|d| d.scale()), floor);
    assert_eq!(engine.renderer.internal, (160, 90));
    assert_eq!(engine.renderer.output_size, size);
}

/// A UI picture with lone opaque pixels lands on the output texel for texel at half the render
/// scale: each opaque pixel is the UI's colour exactly, its transparent neighbour the world's.
#[test]
fn the_ui_lands_texel_for_texel_whatever_the_render_scale() {
    let mut engine = engine();
    door_and_drudge(&mut engine);
    engine.renderer.render_scale = 0.5;
    let size = (160, 90);
    let mut ui = vec![0u8; 160 * 90 * 4];
    let dots: Vec<(usize, usize, [u8; 4])> = (0..20)
        .map(|k| {
            let x = 7 + k * 7;
            let y = 5 + (k * 13) % 80;
            #[allow(clippy::cast_possible_truncation)]
            let c = [(k * 12) as u8, 255 - (k * 9) as u8, 100, 255];
            (x, y, c)
        })
        .collect();
    for &(x, y, c) in &dots {
        ui[(y * 160 + x) * 4..(y * 160 + x) * 4 + 4].copy_from_slice(&c);
    }
    let t = engine.gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ui"),
        size: wgpu::Extent3d {
            width: 160,
            height: 90,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    engine.gpu.queue.write_texture(
        t.as_image_copy(),
        &ui,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(160 * 4),
            rows_per_image: Some(90),
        },
        t.size(),
    );
    let world = engine
        .render_to_rgba(&camera(), &SkyLight::default(), size)
        .expect("a frame");
    engine.renderer.ui = Some(Ui {
        view: t.create_view(&Default::default()),
        gamma: false,
        backdrops: Vec::new(),
    });
    let out = engine
        .render_to_rgba(&camera(), &SkyLight::default(), size)
        .expect("a frame");
    assert_eq!(engine.renderer.internal, (80, 45));
    for &(x, y, c) in &dots {
        let i = (y * 160 + x) * 4;
        for k in 0..3 {
            assert!(
                out[i + k].abs_diff(c[k]) <= 1,
                "({x}, {y}) channel {k}: {} for {}",
                out[i + k],
                c[k]
            );
        }
        // The pixel beside it is the world's (the anti-aliasing's history moves it a little).
        let j = i + 4;
        let near = (0..3)
            .map(|k| u32::from(out[j + k].abs_diff(world[j + k])))
            .max()
            .unwrap_or(0);
        assert!(near < 40, "({}, {y}) took the UI: {near}", x + 1);
    }
}
