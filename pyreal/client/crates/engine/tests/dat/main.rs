//! The engine's conversions over the retail dats ($PYREAL_TEST_DAT_DIR; the pre-Throne-of-Destiny
//! test also $PYREAL_TEST_PRETOD_DAT_DIR). A missing input fails.

mod beacons;
mod bodies;
mod drawn;
mod falls;
mod far;
mod fountains;
mod gates;
mod lamps;
mod later_models;
mod later_rooms;
mod particles;

use glam::Vec3;
use pyreal_assets::{CellLandblock, Decode, GfxObj, Region, Surface};
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_engine::asset::material::{describe_surface, KindTable, PackMaps};
use pyreal_engine::asset::mesh::gfxobj_mesh;
use pyreal_engine::asset::object::{degrade_chain, object_affine, object_parts};
use pyreal_engine::asset::texture::{PaletteSwap, PixelLayout, TextureSource};
use pyreal_engine::camera::Camera;
use pyreal_engine::cells::{self, CellGraph};
use pyreal_engine::terrain::{build_block, GroundTextures};
use pyreal_engine::world::{ground_height, room_cell};
use pyreal_primitives::num::math::atan2f;
use pyreal_primitives::DataId;
use pyreal_terrain::land::mesh::{height_table, sw_to_ne_cut};
use pyreal_world_data::env_cells::EnvCellLoader;
use pyreal_world_data::landblock::{landblock_did, load_region, PYREAL_REGION};

fn decode<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

/// Every 32nd graphics object converts to whole triangles, its indices in range, its sub-meshes
/// covering its indices exactly, its vertices inside its bounding sphere.
#[test]
fn retail_objects_convert_to_consistent_meshes() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let ids = store.ids_of(DbType::GfxObj);
    let mut drawn = 0;
    for &id in ids.iter().step_by(32) {
        let obj: GfxObj = decode(&store, DbType::GfxObj, id).expect("the object decodes");
        let mesh = gfxobj_mesh(&obj);
        assert_eq!(mesh.indices.len() % 3, 0, "{id}");
        assert!(
            mesh.indices
                .iter()
                .all(|&i| (i as usize) < mesh.vertices.len()),
            "{id}"
        );
        let covered: u32 = mesh.submeshes.iter().map(|s| s.index_count).sum();
        assert_eq!(covered as usize, mesh.indices.len(), "{id}");
        for v in &mesh.vertices {
            let d: f32 = (0..3)
                .map(|k| (v.position[k] - mesh.center[k]).powi(2))
                .sum::<f32>()
                .sqrt();
            assert!(d <= mesh.radius * 1.0001 + 1e-4, "{id}");
        }
        drawn += usize::from(!mesh.indices.is_empty());
    }
    assert!(drawn > 300, "only {drawn} objects drew");
}

/// Holtburg's buildings and statics: every part's surfaces describe a material whose picture
/// decodes, DXT pictures staying BC blocks with the client's four mips.
#[test]
fn holtburgs_objects_have_pictures_that_decode() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let info: pyreal_assets::LandblockInfo =
        decode(&store, DbType::Lbi, DataId(0xA9B4_FFFE)).expect("Holtburg's statics");
    let src = TextureSource::new(&store);
    let (mut pictures, mut blocks, mut objects, mut empty) = (0, 0, 0, 0);
    for id in info
        .buildings
        .iter()
        .map(|b| b.id)
        .chain(info.objects.iter().map(|o| o.id))
    {
        // A marker or a physics-only setup draws nothing.
        let parts = object_parts(&store, id);
        objects += 1;
        empty += usize::from(parts.is_empty());
        for p in parts {
            let obj: GfxObj = decode(&store, DbType::GfxObj, p.gfxobj).expect("a part decodes");
            for s in gfxobj_mesh(&obj).submeshes {
                let surface: Surface =
                    decode(&store, DbType::Surface, s.surface.expect("a surface"))
                        .expect("decodes");
                let d = describe_surface(
                    &surface,
                    false,
                    None,
                    &KindTable::default(),
                    PackMaps::default(),
                );
                let Some(pyreal_engine::asset::material::TextureKey::Dat { id, clip_map, .. }) =
                    d.base
                else {
                    continue;
                };
                let px = src
                    .decode(id, clip_map, None)
                    .unwrap_or_else(|e| panic!("{id}: {e}"));
                assert!(px.width > 0 && px.height > 0 && !px.levels.is_empty());
                if px.layout != PixelLayout::Bgra8 {
                    blocks += 1;
                    assert!(px.levels.len() <= 4);
                }
                pictures += 1;
            }
        }
    }
    assert!(
        pictures > 50 && blocks > 0,
        "{pictures} pictures, {blocks} BC"
    );
    assert!(
        empty * 10 < objects,
        "{empty} of {objects} objects draw nothing"
    );
}

#[derive(Default)]
struct Requests {
    grounds: Vec<u32>,
    alphas: Vec<DataId>,
    shifted: Vec<(DataId, PaletteSwap)>,
}

impl GroundTextures for Requests {
    fn ground(&mut self, terrain_type: u32) -> Option<(u32, f32, u32)> {
        self.grounds.push(terrain_type);
        Some((1, 1.0, 0))
    }
    fn alpha(&mut self, id: DataId) -> Option<u32> {
        self.alphas.push(id);
        Some(2)
    }
    fn shifted(&mut self, texture: DataId, swap: PaletteSwap, _kind_of: u32) -> Option<(u32, u32)> {
        self.shifted.push((texture, swap));
        Some((3, 0))
    }
    fn own_palette(&mut self, _texture: DataId) -> Option<DataId> {
        Some(DataId(0x0400_0001))
    }
}

/// Holtburg's ground is the retail mesh: 128 triangles split by the client's hash, every cell
/// laying at least its ground, the blended ones through alpha maps.
#[test]
fn holtburgs_ground_is_the_retail_mesh_with_layered_cells() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = load_region(&store).expect("the region");
    let lb: CellLandblock =
        decode(&store, DbType::LandBlock, landblock_did(0xA9B4)).expect("Holtburg");
    let mut req = Requests::default();
    let table = height_table(&region);
    let g = build_block(
        &lb,
        &region,
        &table,
        0xA9,
        0xB4,
        &|_, _| None,
        10,
        &mut req,
        None,
    );
    assert_eq!(g.mesh.triangle_count(), 128);
    assert_eq!(g.cells.len(), 64);
    assert!(g.cells.iter().all(|c| !c.layers.is_empty()));
    assert!(g.cells.iter().any(|c| c.layers.len() > 1), "nothing blends");
    assert!(!req.alphas.is_empty() && req.shifted.is_empty());
    // Cell (i, j)'s first triangle shares the diagonal its global hash names.
    for i in 0..8 {
        for j in 0..8 {
            let n = i * 8 + j;
            let tri = &g.mesh.vertices[n * 6..n * 6 + 3];
            let cut = sw_to_ne_cut(
                0xA9 * 8 + i32::try_from(i).unwrap_or(0),
                0xB4 * 8 + i32::try_from(j).unwrap_or(0),
            );
            let x0 = 24.0 * f32::from(u8::try_from(i).unwrap_or(0));
            let y0 = 24.0 * f32::from(u8::try_from(j).unwrap_or(0));
            let ne = tri
                .iter()
                .any(|v| v.position[0] == x0 + 24.0 && v.position[1] == y0 + 24.0);
            assert_eq!(ne, cut, "cell ({i}, {j})");
            assert!(tri
                .iter()
                .all(|v| v.extra[0] == 10 + u32::try_from(n).unwrap_or(0)));
        }
    }
}

/// Before Throne of Destiny the software renderer's region palette-shifts the ground: each cell is
/// one picture drawn with a palette made from its corners' terrain palettes, a palette variant.
#[test]
fn the_older_ground_is_a_palette_variant_per_cell() {
    let store = pyreal_dat::testing::open_classic_store_or_fail();
    let region: Region =
        decode(&store, DbType::Region, PYREAL_REGION).expect("the palette-shifting region");
    let lb: CellLandblock =
        decode(&store, DbType::LandBlock, landblock_did(0xA9B4)).expect("Holtburg");
    let mut req = Requests::default();
    let table = height_table(&region);
    let g = build_block(
        &lb,
        &region,
        &table,
        0xA9,
        0xB4,
        &|_, _| None,
        0,
        &mut req,
        None,
    );
    assert_eq!(g.cells.len(), 64);
    assert_eq!(req.shifted.len(), 64);
    assert!(req.shifted.iter().all(|(_, s)| !s.ranges.is_empty()));
    // The variant decodes: a picture recoloured by its corners.
    let src = TextureSource::new(&store);
    let (tex, _) = &req.shifted[0];
    let base = src.own_palette(*tex).expect("the picture's own palette");
    let swap = PaletteSwap {
        base,
        ranges: req.shifted[0].1.ranges.clone(),
        wire: false,
    };
    let palette = src.compose(&swap).expect("the palette composes");
    let px = src
        .decode(*tex, false, Some(&palette))
        .expect("the variant decodes");
    let plain = src.decode(*tex, false, None).expect("the picture decodes");
    assert_eq!(px.layout, PixelLayout::Bgra8);
    assert_ne!(px.levels[0], plain.levels[0], "the corners recolour it");
}

/// The rooms of a block in render space, centred on that block, linked.
fn rooms(store: &RetailDatStore, block: u16) -> CellGraph {
    let mut loader = EnvCellLoader::new();
    let mut g = CellGraph::default();
    for d in loader.load_block(store, block) {
        g.add(room_cell(&d, Vec3::ZERO));
    }
    assert_eq!(loader.stats.undecodable + loader.stats.missing, 0);
    g.link();
    g
}

/// A camera at `at` turned toward `target`.
fn looking(at: Vec3, target: Vec3, hint: Option<u32>) -> Camera {
    let d = target - at;
    Camera {
        position: at,
        yaw: atan2f(-d.x, d.y),
        pitch: atan2f(d.z, d.truncate().length()),
        cell: hint,
        ..Camera::default()
    }
}

/// The training dungeon at 0x019E: every portal between its rooms leads to a loaded room, every
/// room has a volume, and from the room a report stood in the walk reaches a handful of rooms and
/// never the outdoors.
#[test]
fn the_portal_walk_stays_inside_a_dungeon() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let g = rooms(&store, 0x019E);
    assert!(g.cells().len() > 50);
    for c in g.cells() {
        assert!(c.volume.is_some(), "{:#X} has no volume", c.id);
        assert!(!c.seen_outside, "{:#X}", c.id);
        for p in &c.portals {
            assert!(p.to_id.is_none() || p.to.is_some(), "{:#X}", c.id);
        }
    }
    // dungeon-bricks-1 in the report library.
    let at = Vec3::new(10.78, -44.2, 1.93);
    let start = g.slot_of(0x019E_0114).expect("the report's room");
    assert_eq!(g.cell_at(at), Some(start), "the eye stands in its room");
    let camera = Camera {
        position: at,
        yaw: -6.090,
        pitch: -0.11,
        cell: Some(0x019E_0114),
        ..Camera::default()
    };
    let v = g.visibility(
        &camera.view_projection(16.0 / 9.0),
        at,
        camera.cell,
        500.0,
        1.0,
    );
    assert_eq!(v.eye_cell, Some(start));
    assert!(!v.outside(), "a dungeon does not see the sky");
    let seen = v.cells_seen();
    assert!(seen >= 4 && seen * 4 < g.cells().len(), "{seen} rooms seen");
}

/// Holtburg's first building: from its main room, looking at one of its outdoor doors, the
/// outdoors is seen; looking at the same door from outdoors, the room is.
#[test]
fn the_portal_walk_crosses_a_holtburg_door_both_ways() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let g = rooms(&store, 0xA9B4);
    let slot = g.slot_of(0xA9B4_0100).expect("the main room");
    let room = g.cell(slot).expect("the room");
    let v = room.volume.as_ref().expect("its volume");
    let door = room
        .portals
        .iter()
        .find(|p| p.to_id.is_none())
        .expect("an outdoor door");
    #[allow(clippy::cast_precision_loss)]
    let mid = door.points.iter().copied().sum::<Vec3>() / door.points.len() as f32;
    let inside = Vec3::new(v.center.x, v.center.y, v.min.z + 1.6);
    let c = looking(inside, mid, Some(0xA9B4_0100));
    let seen = g.visibility(&c.view_projection(16.0 / 9.0), inside, c.cell, 500.0, 1.0);
    assert_eq!(seen.eye_cell, Some(slot));
    assert!(seen.outside(), "the outdoors through the door");
    // Outdoors, three metres out from the door, looking back in.
    let n = door.plane.truncate();
    let outside = mid - n * 3.0;
    let c = looking(outside, mid, None);
    let seen = g.visibility(&c.view_projection(16.0 / 9.0), outside, None, 500.0, 1.0);
    assert_eq!(seen.eye_cell, None);
    assert!(
        !cells::is_empty(&seen.rects[slot as usize]),
        "the room through its door"
    );
}

/// The ground passes through some of Holtburg's rooms (a floor dug into a slope, a cellar under
/// one): what the terrain cut-outs are for.
#[test]
fn the_ground_passes_through_some_holtburg_rooms() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let region = load_region(&store).expect("the region");
    let g = rooms(&store, 0xA9B4);
    let through = g
        .cells()
        .iter()
        .filter_map(|c| c.volume.as_ref())
        .filter(|v| {
            ground_height(&store, &region, 0xA9B4, v.center.x, v.center.y)
                .is_some_and(|z| v.contains(Vec3::new(v.center.x, v.center.y, z), -0.05))
        })
        .count();
    assert!(through > 0, "no room has the ground inside it");
}

/// Every 16th graphics object with degrade levels: its chain of coarser levels switches at
/// distances that rise past the client's 50 metres, each a different mesh, and stops at the
/// first level that draws nothing.
#[test]
fn degrade_chains_rise_and_end_at_nothing() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let (mut chains, mut nothing) = (0, 0);
    for &id in store.ids_of(DbType::GfxObj).iter().step_by(16) {
        let chain = degrade_chain(&store, id);
        if chain.is_empty() {
            continue;
        }
        chains += 1;
        let mut last = 50.0f32;
        for (k, l) in chain.iter().enumerate() {
            let (mesh, switch) = (&l.mesh, &l.switch);
            assert!(*switch > last, "{id}: {chain:?}");
            last = *switch;
            if mesh.is_none() {
                assert_eq!(k, chain.len() - 1, "{id}: nothing is last");
                nothing += 1;
            }
        }
    }
    assert!(
        chains > 50 && nothing > 0,
        "{chains} chains, {nothing} to nothing"
    );
}

/// A building's doorways (the polygons its drawing BSP's portal nodes name) are openings, left
/// out of its mesh: 0xAAB3's pyramid (0x010014C3) fills its door with a solid opaque blue.
#[test]
fn a_buildings_doorways_are_left_out_of_its_mesh() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let obj: GfxObj =
        decode(&store, DbType::GfxObj, DataId(0x0100_14C3)).expect("the pyramid decodes");
    let portals: Vec<usize> = obj
        .drawing_bsp
        .iter()
        .flat_map(|b| &b.nodes)
        .flat_map(|n| &n.in_portals)
        .filter_map(|&(p, _)| usize::try_from(p).ok())
        .collect();
    assert!(!portals.is_empty(), "the pyramid has doorways");
    let blue = obj.polygons[1].pos_surface;
    assert!(portals.contains(&1), "its blue polygon is a doorway");
    let mesh = gfxobj_mesh(&obj);
    assert!(
        mesh.submeshes.iter().all(|s| s.surface_index != blue),
        "the doorway's blue is not drawn"
    );
}

/// The developer's report at 0x8A02: the stairs 0x02000623, a static of 0x8A02015F, climb north
/// into 0x8A02015E. From the hallway cell 0x8A020143 the walk reaches 0x015E but not 0x015F, and
/// the stairs are still seen, through 0x015E.
#[test]
fn stairs_reaching_into_the_next_room_are_seen_through_it() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let mut loader = EnvCellLoader::new();
    let mut g = CellGraph::default();
    let mut stairs = None;
    for d in loader.load_block(&store, 0x8A02) {
        let slot = g.add(room_cell(&d, Vec3::ZERO));
        let o = d.cell.static_objects.iter().find(|o| o.id.0 == 0x0200_0623);
        if let (0x8A02_015F, Some(o)) = (d.cell.id.0, o) {
            stairs = Some((slot, object_affine(&o.frame, 1.0), o.id));
        }
    }
    let (home, at, id) = stairs.expect("the stairs in 0x8A02015F");
    let mut slots = Vec::new();
    for part in object_parts(&store, id) {
        let obj: GfxObj = decode(&store, DbType::GfxObj, part.gfxobj).expect("a part");
        let mesh = gfxobj_mesh(&obj);
        let t = at * part.transform;
        let scale = t.matrix3.x_axis.length();
        let center = t.transform_point3(Vec3::from(mesh.center));
        slots.push(g.furniture_slot(home, center, mesh.radius * scale));
    }
    g.link();
    let reaching = *slots
        .iter()
        .find(|&&s| s != home)
        .expect("a part reaching out");

    let camera = Camera {
        position: Vec3::new(46.353_943, -39.379_33, -3.3),
        yaw: -1.651_727_2,
        pitch: -0.365_000_5,
        fov_y: 90f32.to_radians(),
        cell: Some(0x8A02_0143),
        ..Camera::default()
    };
    let v = g.visibility(
        &camera.view_projection(1.0),
        camera.position,
        camera.cell,
        500.0,
        1.0,
    );
    let seen = |id: u32| !cells::is_empty(&v.rects[g.slot_of(id).unwrap() as usize]);
    assert!(seen(0x8A02_015E) && !seen(0x8A02_015F), "{v:?}");
    assert!(!cells::is_empty(&v.rects[reaching as usize]), "the stairs");
}
