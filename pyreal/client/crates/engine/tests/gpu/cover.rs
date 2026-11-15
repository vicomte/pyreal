//! Ground cover: placed on the GPU, the same on every visit.

use std::sync::Arc;

use glam::Vec3;
use pyreal_art_pack::{ArtPack, PackGroundCover, PackLod, PackMaterial, PackModel, PackPrimitive};
use pyreal_engine::world::{ground_height, load_area, Area, AssetLoader};
use pyreal_engine::{Camera, SkyLight};
use pyreal_primitives::DataId;

/// A meadow west of Holtburg, all grass.
const MEADOW: u16 = 0xA9B3;

/// A pack with one kind of cover on the grasses: a blade, a third of a metre high.
pub(super) fn grass_pack() -> ArtPack {
    let id = DataId(0xC000_0000);
    let v = |x: f32, z: f32| {
        [
            x,
            0.0,
            z,
            0.0,
            -1.0,
            0.0,
            1.0,
            0.0,
            0.0,
            1.0,
            0.5,
            1.0 - z * 3.0,
            1.0,
            1.0,
            1.0,
            1.0,
        ]
    };
    let vertices = [v(-0.05, 0.0), v(0.05, 0.0), v(0.0, 0.33)].concat();
    let lod = |distance: f32| PackLod {
        distance,
        primitives: vec![PackPrimitive {
            vertices: Arc::new(vertices.clone()),
            indices: Arc::new(vec![0, 1, 2]),
            material: PackMaterial {
                base_color: [0.2, 0.6, 0.1, 1.0],
                base_color_image: None,
                normal_image: None,
                normal_scale: 1.0,
                metal_rough_image: None,
                metallic: 0.0,
                roughness: 0.8,
                occlusion_image: None,
                occlusion_strength: 0.0,
                alpha_cutoff: None,
                double_sided: true,
            },
        }],
        views: None,
        shadows_only: false,
    };
    ArtPack {
        models: vec![PackModel {
            id,
            foliage: true,
            wind: 1.0,
            lods: vec![lod(10.0), lod(20.0), lod(30.0)],
            bounds: ([0.0, 0.0, 0.17], 0.2),
            fingerprint: None,
            draws_on: Default::default(),
            credit: Default::default(),
        }],
        ground_cover: vec![PackGroundCover {
            id,
            grounds: (1 << 1) | (1 << 3) | (1 << 9),
            density: 2.0,
            scale: (0.8, 1.2),
            steepest: 40.0,
        }],
        ..Default::default()
    }
}

/// The plants placed in the world square `[x0, x0 + 8) by [y0, y0 + 8)` (world metres), seen
/// straight down from 25 metres over it (`offset` metres east) with the area loaded round block
/// `center`: each one's world foot (to the millimetre) and scale, once.
fn plants_in(center: u16, x0: f32, y0: f32, offset: f32) -> Vec<[i64; 4]> {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = super::engine();
    let mut loader =
        AssetLoader::with_pack(pyreal_engine::pack::EnginePack::new(Arc::new(grass_pack())));
    load_area(
        &mut engine,
        &mut loader,
        &store,
        &region,
        Area { center, radius: 1 },
    );
    let world = engine.renderer.world_origin;
    let (bx, by) = (
        f32::from(MEADOW >> 8) * 192.0,
        f32::from(MEADOW & 0xFF) * 192.0,
    );
    let local = (x0 + 4.0 - bx, y0 + 4.0 - by);
    let z = ground_height(&store, &region, MEADOW, local.0, local.1).expect("the ground");
    let camera = Camera {
        position: Vec3::new(x0 + 4.0 + offset - world.x, y0 + 4.0 - world.y, z + 25.0),
        pitch: -1.5,
        far: 200.0,
        ..Camera::default()
    };
    engine
        .render_to_rgba(&camera, &SkyLight::default(), (256, 256))
        .expect("a frame");
    let cover = engine.renderer.cover.as_ref().expect("the pack's cover");
    let mut out: Vec<[i64; 4]> = cover
        .read_plants(&engine.gpu)
        .into_iter()
        .map(|(at, _, _, _)| (Vec3::new(at[0] + world.x, at[1] + world.y, at[2]), at[3]))
        .filter(|(p, _)| (x0..x0 + 8.0).contains(&p.x) && (y0..y0 + 8.0).contains(&p.y))
        .map(|(p, s)| {
            #[allow(clippy::cast_possible_truncation)]
            let mm = |v: f32| (v * 1000.0).round() as i64;
            [mm(p.x), mm(p.y), mm(p.z), mm(s)]
        })
        .collect();
    out.sort_unstable();
    // A plant in the band before its level's end is drawn at both levels.
    out.dedup();
    out
}

/// The same square of meadow grows the same plants seen from two places and with the area
/// loaded round two different blocks (render space moved by a block): where each stands, how
/// high and how large, to the millimetre.
#[test]
fn cover_placement_is_the_same_on_every_visit() {
    let (x0, y0) = (
        f32::from(MEADOW >> 8) * 192.0 + 90.0,
        f32::from(MEADOW & 0xFF) * 192.0 + 90.0,
    );
    let first = plants_in(MEADOW, x0, y0, 0.0);
    assert!(
        first.len() > 40,
        "{} plants in 64 square metres",
        first.len()
    );
    let moved = plants_in(MEADOW, x0, y0, 2.0);
    assert_eq!(first, moved, "seen from two metres east");
    let elsewhere = plants_in(MEADOW + 0x0100, x0, y0, 0.0);
    assert_eq!(
        first, elsewhere,
        "with the area loaded round the next block east"
    );
}

/// The cover keeps off a building's walls, not the 24-metre cell its frame stands in: round
/// 0xAAB3's pyramid no plant stands inside its walls, plants grow within three metres of them,
/// and the cell the building stands on (which the old rule left bare) holds plants.
#[test]
fn cover_grows_up_to_a_buildings_walls() {
    const BLOCK: u16 = 0xAAB3;
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = pyreal_world_data::landblock::load_region(&store).expect("the region");
    let mut engine = super::engine();
    let mut loader =
        AssetLoader::with_pack(pyreal_engine::pack::EnginePack::new(Arc::new(grass_pack())));
    load_area(
        &mut engine,
        &mut loader,
        &store,
        &region,
        Area {
            center: BLOCK,
            radius: 1,
        },
    );
    let info: pyreal_assets::LandblockInfo = {
        let id = DataId(u32::from(BLOCK) << 16 | 0xFFFE);
        let bytes = store
            .read_typed(pyreal_dat::DbType::Lbi, id)
            .expect("the block's information");
        pyreal_assets::Decode::decode_payload_in(store.era_of(id), id, &bytes).expect("decodes")
    };
    let b = info
        .buildings
        .iter()
        .find(|b| b.id == DataId(0x0100_14C3))
        .expect("the pyramid");
    let (low, high) = pyreal_engine::world::walls_of(&store, b.id).expect("its walls");
    let x = pyreal_engine::asset::object::object_affine(&b.frame, 1.0).x_axis;
    let world = engine.renderer.world_origin;
    let block = glam::Vec2::new(
        f32::from(BLOCK >> 8) * 192.0,
        f32::from(BLOCK & 0xFF) * 192.0,
    );
    let print = pyreal_engine::land::Footprint {
        at: glam::Vec2::new(b.frame.origin.x, b.frame.origin.y) + block - world,
        axis: glam::Vec2::new(x.x, x.y).normalize(),
        low,
        high,
    };
    let z = ground_height(&store, &region, BLOCK, b.frame.origin.x, b.frame.origin.y)
        .expect("the ground");
    let camera = Camera {
        position: print.at.extend(z + 60.0),
        pitch: -1.5,
        far: 300.0,
        ..Camera::default()
    };
    engine
        .render_to_rgba(&camera, &SkyLight::default(), (256, 256))
        .expect("a frame");
    let plants = engine
        .renderer
        .cover
        .as_ref()
        .expect("the pack's cover")
        .read_plants(&engine.gpu);
    let gaps: Vec<f32> = plants
        .iter()
        .map(|(at, _, _, _)| print.gap(glam::Vec2::new(at[0], at[1])))
        .collect();
    let inside = gaps.iter().filter(|g| **g <= 0.0).count();
    let close = gaps.iter().filter(|g| **g > 0.0 && **g < 3.0).count();
    eprintln!(
        "{} plants, {inside} inside the walls, {close} within 3 m",
        gaps.len()
    );
    assert_eq!(inside, 0, "no plant inside the walls");
    assert!(close > 10, "plants grow up to the walls: {close}");
    let cell = |v: f32| (v / 24.0).floor();
    let home = (cell(b.frame.origin.x), cell(b.frame.origin.y));
    let in_home = plants
        .iter()
        .filter(|(at, _, _, _)| {
            let p = glam::Vec2::new(at[0], at[1]) + world - block;
            (cell(p.x), cell(p.y)) == home
        })
        .count();
    assert!(in_home > 0, "the building's own cell grows plants");
}
