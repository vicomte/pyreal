//! Animated bodies on the GPU: motion vectors from the transforms each instance was drawn with
//! last frame, a faded body seen through, a dressed body's palette variant, a frost elemental's
//! body of snow, and an art pack's posed body.

use std::sync::Arc;

use glam::{Affine3A, Vec3};
use pyreal_animation::data::AnimAssets;
use pyreal_animation::parts::{ObjDesc, PaletteRange};
use pyreal_engine::body::Body;
use pyreal_engine::world::{AssetLoader, Dress};
use pyreal_engine::{Camera, Engine, SkyLight};
use pyreal_primitives::{DataId, Frame, Quat, Vec3 as DVec3};
use pyreal_world_data::anim_assets::DatAnimAssets;

use super::engine;

/// A drudge, which moves by motion table 0x09000008.
const DRUDGE: DataId = DataId(0x0200_07DD);
/// A Holtburg door: a static object.
const DOOR: DataId = DataId(0x0200_19FF);
/// An Avalanche: a frost elemental, its body its default script's snow.
const AVALANCHE: DataId = DataId(0x0200_0CAC);

fn assets(store: &Arc<pyreal_dat::RetailDatStore>) -> Arc<dyn AnimAssets> {
    Arc::new(DatAnimAssets::new(Arc::clone(store)))
}

fn standing(at: glam::Vec3) -> Frame {
    Frame::new(DVec3::new(at.x, at.y, at.z), Quat::IDENTITY)
}

/// An IEEE half as a float.
fn half(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from((h >> 10) & 0x1F);
    let frac = f32::from(h & 0x3FF);
    match exp {
        0 => sign * frac * 2f32.powi(-24),
        31 => sign * f32::INFINITY,
        e => sign * (1.0 + frac / 1024.0) * 2f32.powi(e - 15),
    }
}

/// The largest motion (in texture coordinates) in the left and right halves of the frame, over
/// the pixels something was drawn into.
fn motion_halves(engine: &Engine, (w, h): (u32, u32)) -> (f32, f32) {
    let motion = pyreal_engine::read_texture(
        &engine.gpu,
        engine.renderer.motion_texture().expect("the motion target"),
        4,
    );
    let depth: Vec<f32> = pyreal_engine::read_texture(
        &engine.gpu,
        engine.renderer.depth_texture().expect("the depth target"),
        4,
    )
    .as_chunks::<4>()
    .0
    .iter()
    .map(|b| f32::from_le_bytes(*b))
    .collect();
    let (mut left, mut right) = (0.0f32, 0.0f32);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = y * w as usize + x;
            if depth[i] <= 0.0 {
                continue;
            }
            let m = &motion[i * 4..i * 4 + 4];
            let (mx, my) = (
                half(u16::from_le_bytes([m[0], m[1]])),
                half(u16::from_le_bytes([m[2], m[3]])),
            );
            let len = (mx * mx + my * my).sqrt();
            if x < w as usize / 2 {
                left = left.max(len);
            } else {
                right = right.max(len);
            }
        }
    }
    (left, right)
}

/// A door standing still on the left and a drudge on the right, seen by a still camera: no
/// motion anywhere. Moving the drudge moves the right half only, and only for the frame it moved
/// in; turning the camera moves everything.
#[test]
fn motion_vectors_follow_what_moved_and_nothing_else() {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let mut engine = engine();
    let mut loader = AssetLoader::default();
    loader.place_object(
        &mut engine,
        &store,
        DOOR,
        Affine3A::from_translation(Vec3::new(-1.5, 6.0, 0.0)),
        false,
    );
    let mut body = Body::new(
        assets(&store),
        DRUDGE,
        None,
        standing(Vec3::new(1.2, 6.0, 0.0)),
    )
    .expect("a drudge");
    assert!(body.place(&mut engine, &mut loader, &store) > 5);
    let mut camera = Camera {
        position: Vec3::new(0.0, 0.0, 1.0),
        far: 100.0,
        ..Camera::default()
    };
    let light = SkyLight::default();
    let size = (160, 90);
    let frame = |engine: &mut Engine, camera: &Camera| {
        engine
            .render_to_rgba(camera, &light, size)
            .expect("a frame");
        motion_halves(engine, size)
    };
    frame(&mut engine, &camera);
    let (l, r) = frame(&mut engine, &camera);
    assert!(l < 1e-5 && r < 1e-5, "still: {l} {r}");

    body.frame.origin.x += 0.3;
    body.update(&mut engine.scene);
    let (l, r) = frame(&mut engine, &camera);
    assert!(l < 1e-5, "the door moved: {l}");
    assert!(r > 1e-3, "the drudge did not: {r}");

    let (l, r) = frame(&mut engine, &camera);
    assert!(l < 1e-5 && r < 1e-5, "settled: {l} {r}");

    camera.yaw += 0.02;
    let (l, r) = frame(&mut engine, &camera);
    assert!(l > 1e-3 && r > 1e-3, "the camera turned: {l} {r}");
}

/// A drudge faded half away is seen through evenly: over its body each pixel lies between the
/// drudge drawn whole and the sky behind it, where a screen door leaves each pixel one or the
/// other.
#[test]
fn a_faded_body_is_seen_through_evenly() {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let mut engine = engine();
    engine.renderer.exposure = pyreal_engine::renderer::Exposure::Manual(1.0);
    let mut loader = AssetLoader::default();
    let camera = Camera {
        position: Vec3::new(0.0, 0.0, 1.0),
        far: 100.0,
        ..Camera::default()
    };
    let light = SkyLight::default();
    let size = (160, 90);
    let frame = |engine: &mut Engine| {
        engine
            .render_to_rgba(&camera, &light, size)
            .expect("a frame")
    };
    let sky = frame(&mut engine);
    let mut body = Body::new(
        assets(&store),
        DRUDGE,
        None,
        standing(Vec3::new(0.0, 3.0, 0.0)),
    )
    .expect("a drudge");
    body.place(&mut engine, &mut loader, &store);
    let whole = frame(&mut engine);
    body.driver.set_translucency(0.5);
    body.update(&mut engine.scene);
    let faded = frame(&mut engine);
    let luma =
        |p: &[u8]| 0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2]);
    let mut shares: Vec<f32> = sky
        .as_chunks::<4>()
        .0
        .iter()
        .zip(whole.as_chunks::<4>().0)
        .zip(faded.as_chunks::<4>().0)
        .filter_map(|((s, w), f)| {
            let (s, w, f) = (luma(s), luma(w), luma(f));
            ((w - s).abs() > 24.0).then(|| (f - s) / (w - s))
        })
        .collect();
    assert!(
        shares.len() > 300,
        "the drudge covers {} pixels",
        shares.len()
    );
    let either = shares
        .iter()
        .filter(|&&k| !(0.1..=0.9).contains(&k))
        .count();
    assert!(
        either * 10 < shares.len(),
        "{either} of {} pixels are the drudge or the sky alone",
        shares.len()
    );
    shares.sort_by(f32::total_cmp);
    let middle = shares[shares.len() / 2];
    assert!((0.25..=0.75).contains(&middle), "the middle share {middle}");
}

/// A removed body's instances leave the scene and their slots are used again.
#[test]
fn a_removed_body_leaves_the_scene() {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let mut engine = engine();
    let mut loader = AssetLoader::default();
    let mut body = Body::new(
        assets(&store),
        DRUDGE,
        None,
        standing(Vec3::new(0.0, 5.0, 0.0)),
    )
    .expect("a drudge");
    let n = body.place(&mut engine, &mut loader, &store);
    assert_eq!(engine.scene.instance_count() as usize, n);
    let slots: Vec<u32> = body.instances().iter().flatten().copied().collect();
    body.remove(&mut engine.scene);
    assert_eq!(engine.scene.instance_count(), 0);
    engine
        .render_to_rgba(&Camera::default(), &SkyLight::default(), (32, 32))
        .expect("a frame");
    let mut again = body;
    again.place(&mut engine, &mut loader, &store);
    let reused: Vec<u32> = again.instances().iter().flatten().copied().collect();
    assert_eq!(engine.scene.instance_count() as usize, n);
    let mut a = slots.clone();
    let mut b = reused;
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b, "the freed slots are used again");
}

/// A human dressed in a skin palette: the parts the palette reaches draw with a variant of their
/// material whose picture is another texture slot, and the parts it does not reach share the
/// plain material.
#[test]
fn a_palette_swapped_body_draws_with_its_own_materials() {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let tables = pyreal_chargen::CreationTables::read(&*store).expect("the creation tables");
    let sex = &tables.chargen.heritage_groups[&1].sexes[&1];
    let skins = {
        use pyreal_assets::Decode;
        let bytes = pyreal_primitives::AssetSource::read(&*store, sex.skin_palset).expect("skins");
        pyreal_assets::material::PaletteSet::decode_payload(sex.skin_palset, &bytes)
            .expect("the skin palette set")
            .palette_ids
    };
    let skin = pyreal_chargen::dress::pal_set_palette_id(&skins, 0.9);
    let od = ObjDesc {
        palette_id: sex.base_palette,
        subpalettes: vec![PaletteRange {
            palette_set: skin,
            offset: 0,
            length: 0xC0 / 8,
        }],
        ..ObjDesc::default()
    };
    let mut engine = engine();
    let mut loader = AssetLoader::default();
    let body =
        Body::new(assets(&store), sex.setup, Some(&od), standing(Vec3::ZERO)).expect("a man");
    let (mut tinted, mut plain_parts) = (0, 0);
    for part in &body.driver.part_array.parts {
        let dress = Dress::of(part.surface_overrides.as_ref());
        let Some(gfxobj) = pyreal_engine::asset::object::near_mesh(&store, part.gfxobj_id) else {
            continue;
        };
        if dress.is_plain() {
            plain_parts += 1;
            continue;
        }
        let base = loader
            .mesh(&mut engine, &store, gfxobj, false)
            .expect("the part's mesh");
        let dressed = loader
            .mesh_dressed(&mut engine, &store, gfxobj, &dress)
            .expect("the dressed mesh");
        assert_ne!(base, dressed);
        let draws = |m| engine.scene.mesh(m).expect("a mesh").draws.clone();
        for (a, b) in draws(base).iter().zip(&draws(dressed)) {
            assert_eq!(
                a.first_index, b.first_index,
                "the variant draws the same triangles"
            );
            let (ma, mb) = (
                engine.materials.material(a.material).expect("a material"),
                engine.materials.material(b.material).expect("a material"),
            );
            if ma.textures[0] != mb.textures[0] {
                tinted += 1;
            }
        }
    }
    assert!(tinted > 0, "no part drew in the skin's palette");
    assert!(plain_parts < body.driver.part_array.parts.len());
}

/// How many square metres of an Avalanche 5 m off its snow clearly covers, its default script
/// run three seconds.
fn frost_cover() -> f32 {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let mut engine = engine();
    engine.renderer.exposure = pyreal_engine::renderer::Exposure::Manual(0.3);
    engine.renderer.bounce = false;
    engine.particles.smoke = true;
    let mut loader = AssetLoader::default();
    let at = Vec3::new(0.0, 5.0, 0.0);
    let mut body = Body::new(assets(&store), AVALANCHE, None, standing(at)).expect("an avalanche");
    let camera = Camera {
        position: Vec3::new(0.0, 0.0, 1.2),
        far: 100.0,
        ..Camera::default()
    };
    let light = SkyLight::default();
    let size = (320, 180);
    let empty = engine
        .render_to_rgba(&camera, &light, size)
        .expect("a frame");
    let mut drawn = Vec::new();
    for frame in 0..94 {
        body.advance(1.0 / 30.0);
        body.driver.update_parts(&body.frame);
        body.driver.update_scripts();
        body.driver.update_particles(true);
        let _ = body.driver.take_events();
        let _ = body.driver.take_effects();
        for g in engine.particles.missing_sprites(&body.driver) {
            pyreal_engine::particles::prepare_sprite(&mut engine, &mut loader, &store, g);
        }
        engine.particles.gather(
            &body.driver,
            pyreal_engine::cells::OUTSIDE,
            Vec3::ZERO,
            at,
            false,
            false,
        );
        if frame >= 90 {
            drawn = engine
                .render_to_rgba(&camera, &light, size)
                .expect("a frame");
        }
    }
    let covered = drawn
        .as_chunks::<4>()
        .0
        .iter()
        .zip(empty.as_chunks::<4>().0)
        .filter(|(a, b)| (0..3).any(|c| a[c].abs_diff(b[c]) > 40))
        .count();
    let per_metre =
        size.1 as f32 / 2.0 / (at.y * pyreal_primitives::num::math::tanf(camera.fov_y / 2.0));
    covered as f32 / (per_metre * per_metre)
}

/// A frost elemental's snow is a body about its size, not a few wisps nor a fog.
#[test]
fn a_frost_elementals_snow_is_a_body() {
    let area = frost_cover();
    assert!(
        (1.2..5.0).contains(&area),
        "its snow covers {area:.2} square metres"
    );
}

/// A pack drawing the drudge as the sample body (`pyreal_art_pack::skin::sample_body`), its idle
/// the clip that sways its spine.
fn drudge_body_pack() -> pyreal_engine::pack::EnginePack {
    use pyreal_art_pack::skin::{sample_body, write_gltf};
    let (json, bin) = write_gltf(&sample_body(0.6, [0.8, 0.3, 0.2, 1.0]), "body.bin");
    let manifest = format!(
        "[pack]\nname = \"Body\"\nformat = 1\n[[skin]]\nsetup = \"{:#010X}\"\n\
         file = \"b/body.gltf\"\nclips = {{ idle = \"Sway\" }}\n\
         author = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n",
        DRUDGE.0
    );
    let pack = pyreal_art_pack::ArtPack::open(&manifest, &mut |p| match p {
        "b/body.gltf" => Ok(json.as_bytes().to_vec()),
        "b/body.bin" => Ok(bin.clone()),
        other => Err(format!("no {other}")),
    })
    .expect("the pack opens");
    pyreal_engine::pack::EnginePack::new(Arc::new(pack))
}

/// An art pack's body draws in place of the part it names, and its own pose moves it: with the
/// object standing still, a clip that bends the body gives its pixels motion, and a body held
/// still gives none.
#[test]
fn a_pack_body_draws_and_its_pose_has_motion() {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let mut engine = engine();
    let mut loader = AssetLoader::with_pack(drudge_body_pack());
    let mut body = Body::new(
        assets(&store),
        DRUDGE,
        None,
        standing(Vec3::new(1.0, 5.0, 0.0)),
    )
    .expect("a drudge");
    body.place(&mut engine, &mut loader, &store);
    assert!(body.skinned.is_some(), "the pack's body is made");
    assert!(
        body.instances()[0].is_none(),
        "the part it names draws nothing"
    );
    let camera = Camera {
        position: Vec3::new(0.0, 0.0, 1.0),
        far: 100.0,
        ..Camera::default()
    };
    let light = SkyLight::default();
    let size = (160, 90);
    let frame = |engine: &mut Engine, body: &mut Body, seconds: f64| {
        body.advance(seconds);
        body.update(&mut engine.scene);
        body.draw_skin(engine, &loader);
        engine
            .render_to_rgba(&camera, &light, size)
            .expect("a frame");
        motion_halves(engine, size).1
    };
    frame(&mut engine, &mut body, 0.0);
    let still = frame(&mut engine, &mut body, 0.0);
    let bending = frame(&mut engine, &mut body, 0.25);
    assert!(still < 1e-4, "a body held still has no motion: {still}");
    assert!(bending > 1e-3, "a bending body has motion: {bending}");
}
