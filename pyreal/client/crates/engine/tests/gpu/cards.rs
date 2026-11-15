//! Degrade levels drawn as cards turned to the eye (the dats' `degrade_mode` 2 to 5).

use glam::{Affine3A, Vec3};
use pyreal_engine::world::AssetLoader;
use pyreal_engine::{Camera, SkyLight};
use pyreal_primitives::DataId;

use super::engine;

/// A plant whose level past 53 degrade metres is a card turned about its upright axis (mode 5).
const PLANT: DataId = DataId(0x0100_0044);

/// The plant, scaled up, seen at its card's distance from the south and from the east covers about as much
/// of the frame either way: the card turns to face the eye rather than showing its edge.
#[test]
fn a_card_level_turns_to_the_eye() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let chain = pyreal_engine::asset::object::degrade_chain(&store, PLANT);
    assert_eq!(chain.first().map(|l| l.mode), Some(5), "{chain:?}");
    let mut engine = engine();
    // Forty times its size, 30 m away is 0.75 m over its scale; at a 75th of the dats'
    // distances that is 56 degrade metres, the card's, short of its fading out from 60.
    engine.renderer.lod_bias = 1.0 / 75.0;
    // A fixed exposure: the frames differ only where the plant is.
    engine.renderer.exposure = pyreal_engine::renderer::Exposure::Manual(0.2);
    let at = Affine3A::from_scale(Vec3::splat(40.0));
    let mut loader = AssetLoader::default();
    let size = (240, 240);
    // The pixels the plant changes: the frame with it against the frame without it.
    let mut covered = |position: Vec3, yaw: f32| {
        let camera = Camera {
            position,
            yaw,
            fov_y: 58.7f32.to_radians(),
            far: 100.0,
            ..Camera::default()
        };
        let light = SkyLight::default();
        assert_eq!(loader.place_object(&mut engine, &store, PLANT, at, true), 1);
        let with = engine
            .render_to_rgba(&camera, &light, size)
            .expect("a frame");
        let stats = engine.renderer.read_cull_stats(&engine.gpu);
        assert_eq!(stats.degraded, 1, "drawn at its card: {stats:?}");
        let (id, _) = engine.scene.instances().next().expect("the plant");
        engine.scene.remove_instance(id);
        let without = engine
            .render_to_rgba(&camera, &light, size)
            .expect("a frame");
        with.as_chunks::<4>()
            .0
            .iter()
            .zip(without.as_chunks::<4>().0)
            .filter(|(a, b)| a != b)
            .count()
    };
    // From the south looking north, and from the east looking west.
    let south = covered(Vec3::new(0.0, -30.0, -2.5), 0.0);
    let east = covered(Vec3::new(30.0, 0.0, -2.5), std::f32::consts::FRAC_PI_2);
    assert!(south > 50 && east > 50, "{south} and {east} pixels");
    let ratio = south.max(east) as f32 / south.min(east) as f32;
    assert!(ratio < 1.5, "{south} against {east} pixels");
}

/// The Object Detail scale moves where the plant gives way to its card: at a ninetieth of the
/// dats' distances (the scale 1) it is its card alone, at 2.5 times that its near level alone, and
/// at 1.5 times, inside the band before the switch, both, each through the other's screen door.
#[test]
fn object_detail_moves_the_switch_and_crossfades_the_levels_over_a_band() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let mut engine = engine();
    engine.renderer.exposure = pyreal_engine::renderer::Exposure::Manual(0.2);
    let at = Affine3A::from_scale(Vec3::splat(40.0));
    let mut loader = AssetLoader::default();
    assert_eq!(loader.place_object(&mut engine, &store, PLANT, at, true), 1);
    let camera = Camera {
        position: Vec3::new(0.0, -30.0, -2.5),
        fov_y: 58.7f32.to_radians(),
        far: 100.0,
        ..Camera::default()
    };
    let mut at_scale = |scale: f32| {
        engine.renderer.lod_bias = scale / 90.0;
        engine
            .render_to_rgba(&camera, &SkyLight::default(), (160, 160))
            .expect("a frame");
        engine.renderer.read_cull_stats(&engine.gpu)
    };
    let card = at_scale(1.0);
    let near = at_scale(2.5);
    let both = at_scale(1.5);
    assert_eq!((card.visible_instances, card.degraded), (1, 1), "{card:?}");
    assert_eq!((near.visible_instances, near.degraded), (1, 0), "{near:?}");
    assert!(
        near.visible_triangles > card.visible_triangles,
        "{near:?} {card:?}"
    );
    assert_eq!(both.degraded, 0, "{both:?}");
    assert_eq!(
        both.visible_triangles,
        near.visible_triangles + card.visible_triangles,
        "both levels in the band: {both:?}"
    );
}

/// A trunk two metres across and ten high, as pack vertices (sixteen floats each) and indices.
fn trunk() -> (Vec<f32>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let sides = [
        ([1.0f32, 0.0], [1.0f32, 0.0, 0.0]),
        ([0.0, 1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0], [-1.0, 0.0, 0.0]),
        ([0.0, -1.0], [0.0, -1.0, 0.0]),
    ];
    for (k, (d, n)) in sides.into_iter().enumerate() {
        let a = [d[0] - d[1], d[1] + d[0]];
        let b = [d[0] + d[1], d[1] - d[0]];
        for (p, z) in [(b, 0.0), (a, 0.0), (a, 10.0), (b, 10.0)] {
            vertices.extend([
                p[0], p[1], z, n[0], n[1], n[2], 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0,
            ]);
        }
        let base = u32::try_from(k * 4).unwrap_or(0);
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (vertices, indices)
}

/// A pack whose model for retail object `tree` is a trunk ten metres high, its one level of shape
/// drawn to the end, then its card (a level of pictures baked from it).
pub(super) fn tree_pack(tree: DataId) -> pyreal_art_pack::ArtPack {
    use pyreal_art_pack::cards::{bake_parts, Part};
    use pyreal_art_pack::{ArtPack, PackLod, PackMaterial, PackModel, PackPrimitive};
    use std::sync::Arc;

    let (vertices, indices) = trunk();
    let material = PackMaterial {
        base_color: [0.2, 0.5, 0.2, 1.0],
        base_color_image: None,
        normal_image: None,
        normal_scale: 1.0,
        metal_rough_image: None,
        metallic: 0.0,
        roughness: 0.8,
        occlusion_image: None,
        occlusion_strength: 0.0,
        alpha_cutoff: None,
        double_sided: false,
    };
    let card = bake_parts(
        &[Part {
            vertices: &vertices,
            indices: &indices,
            picture: None,
            base_color: material.base_color,
            cut: None,
            double_sided: false,
            roughness: 0.8,
        }],
        true,
    )
    .expect("the trunk's card");
    let mut card_lod = card.lod;
    for p in &mut card_lod.primitives {
        p.material.base_color_image = Some(0);
        p.material.normal_image = Some(1);
    }
    let shape = PackLod {
        distance: f32::INFINITY,
        primitives: vec![PackPrimitive {
            vertices: Arc::new(vertices),
            indices: Arc::new(indices),
            material,
        }],
        views: None,
        shadows_only: false,
    };
    ArtPack {
        models: vec![PackModel {
            id: tree,
            foliage: true,
            wind: 0.0,
            lods: vec![shape, card_lod],
            bounds: ([0.0, 0.0, 5.0], 5.2),
            fingerprint: None,
            draws_on: Default::default(),
            credit: Default::default(),
        }],
        images: vec![card.colour, card.normal],
        ..Default::default()
    }
}

/// A pack tree past its last level of shape draws as its card (its level of pictures, turned to
/// the eye): seen from 200 metres its pixels are the card's material, from 20 metres its shape's.
#[test]
fn a_pack_tree_past_its_last_shape_draws_as_a_card() {
    use pyreal_engine::asset::material::flags;
    use std::sync::Arc;

    let store = pyreal_dat::testing::open_store_or_fail();
    let tree = DataId(0x0200_FFF0);
    let pack = tree_pack(tree);
    let mut engine = engine();
    engine.renderer.exposure = pyreal_engine::renderer::Exposure::Manual(0.2);
    let pack = pyreal_engine::pack::EnginePack::new(Arc::new(pack));
    let mut loader = AssetLoader::with_pack(pack);
    let placed = loader.place_object(&mut engine, &store, tree, Affine3A::IDENTITY, true);
    assert_eq!(placed, 1);
    let size = (160, 160);
    // Whether the frame's pixels are drawn with an impostor's material, and with another's.
    let mut seen = |distance: f32| -> (bool, bool) {
        let camera = Camera {
            position: Vec3::new(0.0, -distance, 5.0),
            fov_y: 30f32.to_radians(),
            far: 1000.0,
            ..Camera::default()
        };
        engine
            .render_to_rgba(&camera, &SkyLight::default(), size)
            .expect("a frame");
        let ids = engine.renderer.ids_texture().expect("the ids");
        let ids = pyreal_engine::read_texture(&engine.gpu, ids, 4);
        let depth = engine.renderer.depth_texture().expect("the depth");
        let depth = pyreal_engine::read_texture(&engine.gpu, depth, 4);
        let (mut card, mut shape) = (false, false);
        let drawn = ids.as_chunks::<4>().0.iter().zip(depth.as_chunks::<4>().0);
        for (c, _) in drawn.filter(|(_, d)| f32::from_le_bytes(**d) > 0.0) {
            if let Some(m) = engine.materials.material(u32::from_le_bytes(*c)) {
                if m.flags & flags::IMPOSTOR != 0 {
                    card = true;
                } else if m.flags & flags::TERRAIN == 0 {
                    shape = true;
                }
            }
        }
        (card, shape)
    };
    assert_eq!(seen(20.0), (false, true), "near, its shape");
    assert_eq!(seen(200.0), (true, false), "far, its card");
}
