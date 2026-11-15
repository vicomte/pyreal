//! The surfaces' depth: relief only where a material has a height, the ground's
//! depth never moved by it, the road's relief following depth mapping, steep ground stood up past
//! the slope threshold.

use std::sync::Arc;

use glam::{Affine3A, Vec3};
use pyreal_art_pack::{ArtPack, Ground, Image, PackGround};
use pyreal_engine::asset::material::{flags, MaterialRecord, TextureKey, NO_TEXTURE};
use pyreal_engine::asset::mesh::{MeshData, Sides, SubMesh, Vertex};
use pyreal_engine::asset::texture::{PixelLayout, TexturePixels};
use pyreal_engine::renderer::{Exposure, Surfaces, RELIEF_HIGH, TRIPLANAR_FROM, TRIPLANAR_WHOLE};
use pyreal_engine::scene::{Instance, PipelineKind};
use pyreal_engine::world::{ground_height, load_area, Area, AssetLoader};
use pyreal_engine::{Camera, Engine, SkyLight};
use pyreal_terrain::consts::ROAD_TERRAIN_TYPE;
use pyreal_terrain::land::merge::get_terrain_tex;

use super::{engine, words};

/// A picture `side` texels square, each texel's RGBA from `f(x, y)`.
fn picture(side: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> TexturePixels {
    let mut texels = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        for x in 0..side {
            texels.extend(f(x, y));
        }
    }
    TexturePixels {
        width: side,
        height: side,
        layout: PixelLayout::Rgba8 { srgb: false },
        levels: vec![texels],
    }
}

/// White and black stripes down the picture, eight texels each.
fn striped(x: u32) -> bool {
    (x / 8).is_multiple_of(2)
}

/// A wall four metres square in the plane y = 0, facing south (the eye), its picture twice across
/// it, drawn with material `material`.
fn wall(engine: &mut Engine, material: u32) {
    let corners = [(-2.0, 0.0), (2.0, 0.0), (2.0, 4.0), (-2.0, 4.0)];
    let mut mesh = MeshData::default();
    for (x, z) in corners {
        mesh.vertices.push(Vertex {
            position: [x, 0.0, z],
            normal: [0.0, -1.0, 0.0],
            uv: [(x + 2.0) / 2.0, (4.0 - z) / 2.0],
            extra: [0, 0],
        });
    }
    mesh.indices = vec![0, 1, 2, 0, 2, 3];
    mesh.submeshes.push(SubMesh {
        surface_index: 0,
        surface: None,
        sides: Sides::One,
        first_index: 0,
        index_count: 6,
    });
    mesh.fit_bounds();
    let id = engine
        .scene
        .add_mesh(&engine.gpu, &mesh, |_| {
            Some((material, PipelineKind::Opaque))
        })
        .expect("the wall");
    engine
        .scene
        .add_instance(Instance::at(id, Affine3A::IDENTITY, 1, 0))
        .expect("its instance");
}

/// The wall seen from the south-west, two metres off and a metre and a half up, its picture striped and, when
/// `height` says, its white stripes standing above its black ones: the frame with relief on and
/// with it off, and which pixels are the wall's.
fn wall_frames(height: bool) -> (Vec<u8>, Vec<u8>, Vec<bool>) {
    let mut e = engine();
    e.renderer.exposure = Exposure::Manual(0.3);
    e.renderer.bounce = false;
    let stripes = picture(64, |x, _| {
        let v = if striped(x) { 230 } else { 20 };
        [v, v, v, 255]
    });
    let base = e.materials.add_texture(&e.gpu, &stripes).expect("a slot");
    let mut textures = [
        base, NO_TEXTURE, NO_TEXTURE, NO_TEXTURE, NO_TEXTURE, NO_TEXTURE,
    ];
    let mut bits = flags::HAS_BASE;
    if height {
        textures[2] = base;
        bits |= flags::HAS_HEIGHT;
    }
    let material = e
        .materials
        .add_material(MaterialRecord {
            base_color: [1.0; 4],
            textures,
            flags: bits,
            roughness: 0.8,
            alpha_cutoff: 0.5,
            ..Default::default()
        })
        .expect("a record");
    wall(&mut e, material);
    let camera = Camera {
        position: Vec3::new(-1.2, -1.5, 1.5),
        yaw: -0.7,
        pitch: 0.05,
        far: 50.0,
        ..Camera::default()
    };
    let light = SkyLight::default();
    let mut frame = |surfaces: Surfaces| {
        e.renderer.surfaces = surfaces;
        e.render_to_rgba(&camera, &light, (512, 384))
            .expect("a frame")
    };
    // The first frame has nothing of a last frame's to go on.
    frame(Surfaces::NONE);
    let on = frame(Surfaces::ALL);
    let off = frame(Surfaces::NONE);
    // The wall is all there is: what has depth is the wall.
    let wall: Vec<bool> = words(&e, e.renderer.depth_texture())
        .iter()
        .map(|&d| d != 0)
        .collect();
    (on, off, wall)
}

/// How many of the wall's pixels are its white stripes: brighter than its mean.
fn white(rgba: &[u8], wall: &[bool]) -> usize {
    let px = rgba.as_chunks::<4>().0;
    let on_wall = || {
        px.iter()
            .zip(wall)
            .filter(|(_, &w)| w)
            .map(|(p, _)| u32::from(p[1]))
    };
    let n = u32::try_from(on_wall().count()).unwrap_or(1).max(1);
    let mean = on_wall().sum::<u32>() / n;
    on_wall().filter(|&g| g > mean).count()
}

/// A surface with no height map draws exactly alike with relief on and off.
#[test]
fn a_surface_without_a_height_map_draws_alike_with_relief_on_and_off() {
    let (on, off, wall) = wall_frames(false);
    assert!(
        wall.iter().filter(|&&w| w).count() > 5000,
        "the wall is in view"
    );
    assert!(on == off, "relief changed a surface with no height");
}

/// A surface with a height map seen at a slant shows its relief: the raised white stripes hide
/// part of the sunken black ones beside them, so more of the wall is white.
#[test]
fn a_height_mapped_surface_shows_parallax() {
    let (on, off, wall) = wall_frames(true);
    let (lit_on, lit_off) = (white(&on, &wall), white(&off, &wall));
    assert!(lit_off > 2000, "the wall is in view");
    assert!(
        lit_on as f32 > lit_off as f32 * 1.01,
        "{lit_on} white pixels with relief, {lit_off} without"
    );
}

/// On Holtburg's road the depth drawn is the same with every part of the surfaces' depth on and
/// off: relief bends the light and moves where pictures are read, never the surface, so a body
/// stands where it is drawn standing.
#[test]
fn the_surfaces_depth_never_moves_the_ground() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut e = engine();
    let mut loader = AssetLoader::default();
    load_area(
        &mut e,
        &mut loader,
        &store,
        &region,
        Area {
            center: 0xA9B4,
            radius: 1,
        },
    );
    let z = ground_height(&store, &region, 0xA9B4, 26.0, 78.0).expect("the road") + 1.7;
    let camera = Camera {
        position: Vec3::new(26.0, 78.0, z),
        pitch: -0.6,
        far: 300.0,
        ..Camera::default()
    };
    let light = SkyLight::at(&region, 0, 0.5);
    let mut depth = |surfaces: Surfaces| {
        e.renderer.surfaces = surfaces;
        e.render_to_rgba(&camera, &light, (320, 180))
            .expect("a frame");
        words(&e, e.renderer.depth_texture())
    };
    let on = depth(Surfaces::ALL);
    let off = depth(Surfaces::NONE);
    assert!(on.iter().filter(|&&d| d != 0).count() > on.len() / 2);
    assert!(on == off, "the depth moved");
}

/// A pack whose road is magenta stones in stripes eight texels wide, each standing above the dark
/// joints between them (its height in the picture's alpha), and, where `lean`, its normal leaning
/// east and west by turns.
fn road_pack(lean: bool) -> ArtPack {
    let side = 64;
    let image = |srgb: bool, f: &dyn Fn(u32) -> [u8; 4]| Image {
        width: side,
        height: side,
        srgb,
        levels: vec![Arc::new(
            (0..side * side)
                .flat_map(|i| f(i % side))
                .collect::<Vec<u8>>(),
        )],
        format: None,
    };
    ArtPack {
        grounds: vec![PackGround {
            grounds: vec![Ground::Road],
            albedo: image(true, &|x| {
                if striped(x) {
                    [220, 40, 220, 255]
                } else {
                    [70, 10, 70, 0]
                }
            }),
            normal: image(false, &|x| {
                let east = match (lean, (x / 4) % 2) {
                    (false, _) => 128,
                    (true, 0) => 240,
                    (true, _) => 16,
                };
                [east, 128, 0, 255]
            }),
            surface: image(false, &|_| [200, 255, 0, 255]),
            size: 2.0,
            normal_scale: 1.0,
            steep: false,
            fingerprint: None,
            detail: None,
            detail_size: 1.0,
            detail_strength: 1.0,
            pebbles: true,
            credit: Default::default(),
        }],
        ..Default::default()
    }
}

/// Holtburg's road laid with [`road_pack`], seen from on it at each depth mapping level, Off to
/// High, and which pixels are the road's (magenta).
fn road_frames(lean: bool) -> (Vec<Vec<u8>>, Vec<bool>) {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut e = engine();
    e.renderer.exposure = Exposure::Manual(0.3);
    e.renderer.bounce = false;
    let mut loader = AssetLoader::with_pack(pyreal_engine::pack::EnginePack::new(Arc::new(
        road_pack(lean),
    )));
    load_area(
        &mut e,
        &mut loader,
        &store,
        &region,
        Area {
            center: 0xA9B4,
            radius: 1,
        },
    );
    let z = ground_height(&store, &region, 0xA9B4, 26.0, 78.0).expect("the road") + 1.7;
    let camera = Camera {
        position: Vec3::new(26.0, 78.0, z),
        pitch: -0.6,
        far: 300.0,
        ..Camera::default()
    };
    let light = SkyLight::at(&region, 0, 0.5);
    let mut frame = |relief: u8| {
        e.renderer.surfaces = Surfaces {
            relief,
            ..Surfaces::ALL
        };
        e.render_to_rgba(&camera, &light, (320, 180))
            .expect("a frame")
    };
    frame(0);
    let frames: Vec<Vec<u8>> = (0..=RELIEF_HIGH).map(&mut frame).collect();
    let road = frames[0]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let [r, g, b] = [p[0], p[1], p[2]].map(u32::from);
            r > 12 && 2 * r > 3 * g && 2 * b > 3 * g
        })
        .collect();
    (frames, road)
}

/// How many road pixels there are, and how many of them differ between `a` and `b`.
fn road_changed(a: &[u8], b: &[u8], road: &[bool]) -> (usize, usize) {
    let (a, b) = (a.as_chunks::<4>().0, b.as_chunks::<4>().0);
    let n = road.iter().filter(|&&r| r).count();
    let moved = (0..road.len())
        .filter(|&k| road[k] && a[k][..3] != b[k][..3])
        .count();
    (n, moved)
}

/// The road's stones bend the light by depth mapping's level: none with it off, more at each
/// level up.
#[test]
fn the_roads_bends_follow_depth_mapping() {
    let (frames, road) = road_frames(true);
    for level in 1..frames.len() {
        let (n, moved) = road_changed(&frames[level - 1], &frames[level], &road);
        assert!(n > 2000, "{n} road pixels in view");
        assert!(
            moved * 2 > n,
            "{moved} of {n} road pixels changed from level {} to {level}",
            level - 1
        );
    }
}

/// The road's height shows as relief from Medium up and not before: its raised stones hide part
/// of the sunken joints beside them, so more of it is stone.
#[test]
fn the_roads_height_shows_from_medium() {
    let (frames, road) = road_frames(false);
    let n = road.iter().filter(|&&r| r).count();
    assert!(n > 2000, "{n} road pixels in view");
    let stone = |rgba: &[u8]| {
        let red = rgba.as_chunks::<4>().0.iter().map(|p| p[0]);
        let mean = red
            .clone()
            .zip(&road)
            .filter(|(_, &r)| r)
            .map(|(v, _)| u32::from(v))
            .sum::<u32>()
            / u32::try_from(n).unwrap_or(1).max(1);
        red.zip(&road)
            .filter(|(v, &r)| r && u32::from(*v) > mean)
            .count()
    };
    let [off, low, _, high] = [0, 1, 2, 3].map(|l| stone(&frames[l]) as f32);
    assert!(
        (low - off).abs() < off * 0.005,
        "{low} stone pixels at Low, {off} with it off"
    );
    assert!(
        high > off * 1.02,
        "{high} stone pixels at High, {off} with it off"
    );
}

/// The meadow west of Holtburg, its grasses laid with [`road_pack`]'s level stones and, where
/// `cover`, under the grass pack's cover: the frame at Low and at High, and which pixels are the
/// stones' (magenta).
fn meadow_frames(cover: bool) -> ([Vec<u8>; 2], Vec<bool>) {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut pack = super::cover::grass_pack();
    let mut ground = road_pack(false).grounds.remove(0);
    ground.grounds = [1, 3, 9].map(Ground::Type).to_vec();
    pack.grounds = vec![ground];
    let mut e = engine();
    e.renderer.exposure = Exposure::Manual(0.3);
    e.renderer.bounce = false;
    e.renderer.cover_on = cover;
    let mut loader = AssetLoader::with_pack(pyreal_engine::pack::EnginePack::new(Arc::new(pack)));
    load_area(
        &mut e,
        &mut loader,
        &store,
        &region,
        Area {
            center: 0xA9B3,
            radius: 1,
        },
    );
    let z = ground_height(&store, &region, 0xA9B3, 96.0, 96.0).expect("the meadow") + 1.7;
    let camera = Camera {
        position: Vec3::new(96.0, 96.0, z),
        pitch: -0.7,
        far: 300.0,
        ..Camera::default()
    };
    let light = SkyLight::at(&region, 0, 0.5);
    let mut frame = |relief: u8| {
        e.renderer.surfaces = Surfaces {
            relief,
            ..Surfaces::ALL
        };
        e.render_to_rgba(&camera, &light, (320, 180))
            .expect("a frame")
    };
    frame(1);
    let frames = [frame(1), frame(RELIEF_HIGH)];
    let stones = frames[0]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let [r, g, b] = [p[0], p[1], p[2]].map(u32::from);
            r > 12 && 2 * r > 3 * g && 2 * b > 3 * g
        })
        .collect();
    (frames, stones)
}

/// How many of the `ground` pixels are raised stone: redder than their mean.
fn raised(rgba: &[u8], ground: &[bool]) -> usize {
    let red = || {
        rgba.as_chunks::<4>()
            .0
            .iter()
            .zip(ground)
            .filter(|(_, &g)| g)
            .map(|(p, _)| u32::from(p[0]))
    };
    let n = u32::try_from(red().count()).unwrap_or(1).max(1);
    let mean = red().sum::<u32>() / n;
    red().filter(|&v| v > mean).count()
}

/// Ground the drawn cover grows on takes no depth from its height, the plants hiding it: its
/// stones show no more at High than at Low. With the cover off, the same ground does.
#[test]
fn covered_ground_takes_no_depth() {
    let ([low, high], ground) = meadow_frames(true);
    let n = ground.iter().filter(|&&g| g).count();
    assert!(n > 2000, "{n} ground pixels in view under the cover");
    let (low, high) = (raised(&low, &ground) as f32, raised(&high, &ground) as f32);
    assert!(
        (high - low).abs() < low * 0.005,
        "{high} stone pixels at High under the cover, {low} at Low"
    );
    let ([low, high], ground) = meadow_frames(false);
    let (low, high) = (raised(&low, &ground) as f32, raised(&high, &ground) as f32);
    assert!(
        high > low * 1.02,
        "{high} stone pixels at High with no cover, {low} at Low"
    );
}

/// Holtburg's road with no pack: its relief is worked out from its own picture, its normal and
/// height the picture's companions, and depth mapping changes how it draws.
#[test]
fn the_retail_road_takes_its_relief_from_its_own_picture() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut e = engine();
    e.renderer.exposure = Exposure::Manual(0.3);
    e.renderer.bounce = false;
    let mut loader = AssetLoader::default();
    load_area(
        &mut e,
        &mut loader,
        &store,
        &region,
        Area {
            center: 0xA9B4,
            radius: 1,
        },
    );
    let tm = region
        .land_surf
        .tex_merge
        .as_ref()
        .expect("the land's pictures");
    let id = get_terrain_tex(tm, ROAD_TERRAIN_TYPE)
        .expect("the road's picture")
        .tex_gid;
    let key = TextureKey::Dat {
        id,
        clip_map: false,
        swap: None,
    };
    let slot = loader.texture(&mut e, &store, &key).expect("its slot");
    let c = e.materials.companions(slot).expect("its companions");
    assert!(c[0] != NO_TEXTURE && c[1] != NO_TEXTURE, "{c:?}");
    let z = ground_height(&store, &region, 0xA9B4, 26.0, 78.0).expect("the road") + 1.7;
    let camera = Camera {
        position: Vec3::new(26.0, 78.0, z),
        pitch: -0.6,
        far: 300.0,
        ..Camera::default()
    };
    let light = SkyLight::at(&region, 0, 0.5);
    let mut frame = |relief: u8| {
        e.renderer.surfaces = Surfaces {
            relief,
            ..Surfaces::ALL
        };
        e.render_to_rgba(&camera, &light, (320, 180))
            .expect("a frame")
    };
    frame(0);
    let off = frame(0);
    let high = frame(RELIEF_HIGH);
    let (off, high) = (off.as_chunks::<4>().0, high.as_chunks::<4>().0);
    let moved = off
        .iter()
        .zip(high)
        .filter(|(a, b)| a[..3] != b[..3])
        .count();
    assert!(
        moved * 10 > off.len(),
        "{moved} of {} pixels changed with depth mapping",
        off.len()
    );
}

/// A half float's value.
fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from((bits >> 10) & 0x1F);
    let frac = f32::from(bits & 0x3FF);
    sign * match exp {
        0 => frac * 2f32.powi(-24),
        31 => f32::INFINITY,
        _ => (1.0 + frac / 1024.0) * 2f32.powi(exp - 15),
    }
}

/// An octahedral normal's height.
fn oct_up(x: f32, y: f32) -> f32 {
    let z = 1.0 - x.abs() - y.abs();
    let (x, y) = if z < 0.0 {
        ((1.0 - y.abs()) * x.signum(), (1.0 - x.abs()) * y.signum())
    } else {
        (x, y)
    };
    z / (x * x + y * y + z * z).sqrt()
}

/// The frame at `block`'s `x, y` (the eye two metres over the ground, looking toward compass
/// `heading` and `pitch` up) with the slope's projection off and on, and each pixel's normal's
/// height.
fn slope_frames(
    block: u16,
    x: f32,
    y: f32,
    heading: f32,
    pitch: f32,
) -> (Vec<u8>, Vec<u8>, Vec<f32>) {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut e = engine();
    e.renderer.exposure = Exposure::Manual(0.3);
    e.renderer.bounce = false;
    let mut loader = AssetLoader::default();
    load_area(
        &mut e,
        &mut loader,
        &store,
        &region,
        Area {
            center: block,
            radius: 1,
        },
    );
    let z = ground_height(&store, &region, block, x, y).expect("the ground") + 2.0;
    let camera = Camera {
        position: Vec3::new(x, y, z),
        yaw: -heading.to_radians(),
        pitch: pitch.to_radians(),
        far: 300.0,
        ..Camera::default()
    };
    let light = SkyLight::at(&region, 0, 0.5);
    let mut frame = |surfaces: Surfaces| {
        e.renderer.surfaces = surfaces;
        e.render_to_rgba(&camera, &light, (320, 180))
            .expect("a frame")
    };
    frame(Surfaces::NONE);
    let off = frame(Surfaces::NONE);
    let on = frame(Surfaces {
        triplanar: true,
        ..Surfaces::NONE
    });
    let up = words(&e, e.renderer.normal_texture())
        .iter()
        .map(|n| oct_up(half((n & 0xFFFF) as u16), half((n >> 16) as u16)))
        .collect();
    (on, off, up)
}

/// How many pixels whose normal's height `keep` takes there are, and how many of them changed.
fn changed(on: &[u8], off: &[u8], up: &[f32], keep: impl Fn(f32) -> bool) -> (usize, usize) {
    let mut n = (0, 0);
    for (k, &u) in up.iter().enumerate() {
        if keep(u) {
            n.0 += 1;
            n.1 += usize::from(on[k * 4..k * 4 + 3] != off[k * 4..k * 4 + 3]);
        }
    }
    n
}

/// Under the cliff at 0xBBBE the ground's pictures stand up along the slope: the steep ground
/// changes. On Holtburg's gentle ground (the stored normal is the smoothed one, so a margin keeps
/// each triangle's own facing on its side of the threshold) nothing does.
#[test]
fn steep_ground_alone_is_stood_up() {
    let (on, off, up) = slope_frames(0xBBBE, 128.0, 60.0, 90.0, 6.0);
    let (steep, moved) = changed(&on, &off, &up, |u| u < TRIPLANAR_WHOLE - 0.15);
    assert!(steep > 1000, "{steep} steep pixels");
    assert!(
        moved * 10 > steep * 8,
        "{moved} of {steep} steep pixels changed"
    );
    let (on, off, up) = slope_frames(0xA9B4, 26.0, 78.0, 0.0, -20.0);
    let (gentle, moved) = changed(&on, &off, &up, |u| u > TRIPLANAR_FROM + 0.17);
    assert!(gentle > 10_000, "{gentle} gentle pixels");
    assert_eq!(moved, 0, "{moved} of {gentle} gentle pixels changed");
}
