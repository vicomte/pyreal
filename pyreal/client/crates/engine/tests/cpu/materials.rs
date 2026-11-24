//! The material record and its per-map fallbacks: with nothing from the pack a
//! surface draws from the dat, and each pack map only adds.

use pyreal_assets::Surface;
use pyreal_engine::asset::material::{
    describe_surface, flags, srgb_to_linear, untextured_colour, KindTable, MaterialRecord,
    PackMaps, SurfaceKind, TextureKey, MATERIAL_RECORD_SIZE, NO_TEXTURE,
};
use pyreal_engine::asset::mesh::Sides;
use pyreal_engine::scene::PipelineKind;
use pyreal_engine::world::pipeline_for;
use pyreal_primitives::DataId;

const IMAGE: u32 = 0x2;
const CLIPMAP: u32 = 0x4;
const TRANSLUCENT: u32 = 0x10;
const LUMINOUS: u32 = 0x40;
const ADDITIVE: u32 = 0x1_0000;

fn surface(surface_type: u32) -> Surface {
    let textured = surface_type & 6 != 0;
    Surface {
        surface_type,
        orig_texture_id: textured.then_some(DataId(0x0500_0010)),
        orig_palette_id: textured.then_some(DataId(0x0400_0001)),
        color_value: (!textured).then_some(0xFF80_4020),
        translucency: 0.25,
        luminosity: 0.0,
        diffuse: 1.0,
    }
}

fn dat(id: u32, clip_map: bool) -> TextureKey {
    TextureKey::Dat {
        id: DataId(id),
        clip_map,
        swap: None,
    }
}

/// With no pack map, the base colour is the dat's picture and nothing else claims to exist:
/// normal from the mesh, no height, roughness and metal from the kind, no occlusion, no emission.
#[test]
fn a_surface_with_nothing_from_the_pack_draws_from_the_dat() {
    let d = describe_surface(
        &surface(IMAGE),
        false,
        None,
        &KindTable::default(),
        PackMaps::default(),
    );
    assert_eq!(d.base, Some(dat(0x0500_0010, false)));
    assert_eq!(d.flags, flags::HAS_BASE);
    assert_eq!((d.roughness, d.metal), SurfaceKind::Plain.roughness_metal());
    assert_eq!(d.emissive, 0.0);
    assert!(d.normal.is_none() && d.height.is_none() && d.occlusion.is_none());
}

/// A solid surface has no picture: its colour is the record's, in linear light.
#[test]
fn a_solid_surface_takes_its_colour_from_the_record() {
    let d = describe_surface(
        &surface(0x1),
        false,
        None,
        &KindTable::default(),
        PackMaps::default(),
    );
    assert!(d.base.is_none());
    assert_eq!(d.flags & flags::HAS_BASE, 0);
    assert_eq!(
        d.base_color,
        [
            srgb_to_linear(0x80),
            srgb_to_linear(0x40),
            srgb_to_linear(0x20),
            1.0
        ]
    );
}

/// The clip map is a cut-out, keyed through the picture's palette; translucency and additive
/// blending are drawn forward; the dat's luminosity is the emission.
#[test]
fn the_dat_flags_become_material_flags() {
    let kinds = KindTable::default();
    let cut = describe_surface(
        &surface(IMAGE | CLIPMAP),
        true,
        None,
        &kinds,
        PackMaps::default(),
    );
    assert_eq!(cut.base, Some(dat(0x0500_0010, true)));
    assert_eq!(
        cut.flags & (flags::CUTOUT | flags::TWO_SIDED),
        flags::CUTOUT | flags::TWO_SIDED
    );
    assert_eq!(pipeline_for(cut.flags, Sides::Both), PipelineKind::TwoSided);

    let glass = describe_surface(
        &surface(IMAGE | TRANSLUCENT),
        false,
        None,
        &kinds,
        PackMaps::default(),
    );
    assert_ne!(glass.flags & flags::TRANSLUCENT, 0);
    assert_eq!(glass.flags & flags::CUTOUT, 0);
    assert!((glass.base_color[3] - 0.75).abs() < 1e-6);
    assert_eq!(
        pipeline_for(glass.flags, Sides::One),
        PipelineKind::Translucent
    );

    let glow = describe_surface(
        &surface(IMAGE | ADDITIVE),
        false,
        None,
        &kinds,
        PackMaps::default(),
    );
    assert_eq!(pipeline_for(glow.flags, Sides::One), PipelineKind::Additive);

    let mut lamp = surface(IMAGE | LUMINOUS);
    lamp.luminosity = 0.6;
    let lamp = describe_surface(&lamp, false, None, &kinds, PackMaps::default());
    assert_ne!(lamp.flags & flags::LUMINOUS, 0);
    assert!((lamp.emissive - 0.6).abs() < 1e-6);
}

/// Each pack map is optional and only adds: a surface can have any mix of them, and the flags say
/// which exist.
#[test]
fn each_pack_map_only_adds() {
    let pack = PackMaps {
        normal: Some(TextureKey::Pack("brick_n".into())),
        roughness_metal: Some(TextureKey::Pack("brick_rm".into())),
        ..PackMaps::default()
    };
    let d = describe_surface(&surface(IMAGE), false, None, &KindTable::default(), pack);
    assert_eq!(
        d.base,
        Some(dat(0x0500_0010, false)),
        "the dat's picture is still the base"
    );
    assert_eq!(
        d.flags,
        flags::HAS_BASE | flags::HAS_NORMAL | flags::HAS_ROUGH_METAL
    );
    let upscaled = PackMaps {
        base: Some(TextureKey::Pack("brick".into())),
        ..PackMaps::default()
    };
    let d = describe_surface(
        &surface(IMAGE),
        false,
        None,
        &KindTable::default(),
        upscaled,
    );
    assert_eq!(d.base, Some(TextureKey::Pack("brick".into())));
}

/// A `[[solid]]` material on an untextured surface: its grain multiplies the surface's own colour,
/// laid along the world's axes, its repeat carried in the record's cut-out word.
#[test]
fn a_solid_keeps_the_surface_colour_under_its_projected_grain() {
    let solid = surface(0x1);
    assert_eq!(untextured_colour(&solid), Some(0xFF80_4020));
    assert_eq!(untextured_colour(&surface(IMAGE)), None);
    let pack = PackMaps {
        base: Some(TextureKey::Pack("grain".into())),
        normal: Some(TextureKey::Pack("grain_n".into())),
        projected: Some(0.5),
        ..PackMaps::default()
    };
    let d = describe_surface(&solid, false, None, &KindTable::default(), pack.clone());
    assert_eq!(d.base_color[0], srgb_to_linear(0x80));
    assert_eq!(
        d.flags & (flags::HAS_BASE | flags::HAS_NORMAL | flags::PROJECTED),
        flags::HAS_BASE | flags::HAS_NORMAL | flags::PROJECTED
    );
    let r = MaterialRecord::new(&d, |_| Some(3));
    assert!((r.alpha_cutoff - 0.5).abs() < 1e-6);
    // On a picture it is no solid: drawn as any pack picture is.
    let d = describe_surface(&surface(IMAGE), false, None, &KindTable::default(), pack);
    assert_eq!(d.flags & flags::PROJECTED, 0);
    assert!((MaterialRecord::new(&d, |_| Some(3)).alpha_cutoff - 0.5).abs() < 1e-6);
}

/// A map whose texture does not load loses its flag, so the shader takes the fallback.
#[test]
fn a_map_that_does_not_load_falls_back() {
    let pack = PackMaps {
        normal: Some(TextureKey::Pack("missing".into())),
        ..PackMaps::default()
    };
    let d = describe_surface(&surface(IMAGE), false, None, &KindTable::default(), pack);
    let r = MaterialRecord::new(&d, |k| matches!(k, TextureKey::Dat { .. }).then_some(7));
    assert_eq!(r.textures[0], 7);
    assert_eq!(r.textures[1], NO_TEXTURE);
    assert_eq!(r.flags, flags::HAS_BASE);
    let mut bytes = Vec::new();
    r.write(&mut bytes);
    assert_eq!(bytes.len() as u64, MATERIAL_RECORD_SIZE);
}

/// Roughness and metal come from the surface's kind, keyed by its texture, and the pack can name
/// another kind.
#[test]
fn the_kind_table_sets_roughness_and_metal() {
    let mut kinds = KindTable::default();
    kinds.insert(DataId(0x0500_0010), SurfaceKind::Metal);
    let d = describe_surface(&surface(IMAGE), false, None, &kinds, PackMaps::default());
    assert_eq!(
        (d.kind, d.roughness, d.metal),
        (SurfaceKind::Metal, 0.35, 1.0)
    );
    let water = PackMaps {
        kind: Some(SurfaceKind::Water),
        ..PackMaps::default()
    };
    let d = describe_surface(&surface(IMAGE), false, None, &kinds, water);
    assert_ne!(d.flags & flags::WATER, 0);
    assert_eq!(
        SurfaceKind::of_terrain_name("WaterStandingFresh"),
        SurfaceKind::Water
    );
    assert_eq!(
        SurfaceKind::of_terrain_name("BarrenRock"),
        SurfaceKind::Stone
    );
    assert_eq!(SurfaceKind::of_terrain_name("Snow"), SurfaceKind::Snow);
    assert_eq!(
        SurfaceKind::of_terrain_name("PackedDirt"),
        SurfaceKind::Earth
    );
    assert_eq!(
        SurfaceKind::of_terrain_name("Volcano1"),
        SurfaceKind::Molten
    );
}

/// A molten picture stays molten under a pack's own picture, which then glows where it is hot, and
/// a pack's grit can name it another kind.
#[test]
fn a_packs_own_picture_for_a_molten_one_stays_molten() {
    let mut kinds = KindTable::default();
    kinds.insert(DataId(0x0500_0010), SurfaceKind::Molten);
    let kind = |pack| describe_surface(&surface(IMAGE), false, None, &kinds, pack).kind;
    let base = Some(TextureKey::Pack("material:0:albedo".into()));
    assert_eq!(kind(PackMaps::default()), SurfaceKind::Molten);
    assert_eq!(
        kind(PackMaps {
            base: base.clone(),
            ..PackMaps::default()
        }),
        SurfaceKind::Molten
    );
    assert_eq!(
        kind(PackMaps {
            base,
            kind: Some(SurfaceKind::Stone),
            ..PackMaps::default()
        }),
        SurfaceKind::Stone
    );
}

/// A pack picture of `side` texels in `format`, its one level zeroed.
fn picture(side: u32, format: pyreal_art_pack::images::BlockFormat) -> pyreal_art_pack::Image {
    let blocks = (side.div_ceil(4) * side.div_ceil(4)) as usize;
    pyreal_art_pack::Image {
        width: side,
        height: side,
        srgb: false,
        levels: vec![std::sync::Arc::new(vec![0; blocks * format.block_bytes()])],
        format: Some(format),
    }
}

/// An art pack with two materials for retail pictures: one with a normal map and nothing else
/// (its surface the flat default it is given), one with every map.
fn pack() -> std::sync::Arc<pyreal_art_pack::ArtPack> {
    use pyreal_art_pack::images::BlockFormat::{Bc5, Bc7};
    let material = |texture: u32, albedo: bool, surface: u32| pyreal_art_pack::TextureMaterial {
        textures: vec![DataId(texture)],
        albedo: albedo.then(|| picture(64, Bc7)),
        normal: picture(64, Bc5),
        surface: picture(surface, Bc7),
        grit: None,
        metal: None,
        clear: None,
        grit_repeat: [1.0, 1.0],
        damp_floors: false,
        slime: false,
        ice: false,
        sheen: 0.0,
        matter: Default::default(),
        fingerprints: Vec::new(),
        palettes: Vec::new(),
        laid_palettes: Vec::new(),
        draws_on: Default::default(),
        solid: None,
        credit: Default::default(),
    };
    std::sync::Arc::new(pyreal_art_pack::ArtPack {
        materials: vec![
            material(0x0500_0010, false, 4),
            material(0x0500_0020, true, 32),
        ],
        ..Default::default()
    })
}

/// The pack's materials through the engine's hook: a surface whose picture a pack material
/// stands for takes each map the material has, and sets its flag; the maps it lacks (the flat
/// defaults) fall back, so its base is still the dat's picture. A surface no material stands for
/// draws from the dat alone, and a pack map that will not load drops its flag.
#[test]
fn a_pack_material_sets_its_flags_and_a_missing_map_falls_back() {
    use pyreal_engine::asset::material::MaterialOverrides;
    use pyreal_engine::pack::{EnginePack, PackOverrides};
    let pack = pack();
    let hook = PackOverrides(std::sync::Arc::clone(&pack));
    let kinds = KindTable::default();

    let normal_only = hook.maps(DataId(0x0800_0001), Some(DataId(0x0500_0010)));
    let d = describe_surface(&surface(IMAGE), false, None, &kinds, normal_only);
    assert_eq!(d.base, Some(dat(0x0500_0010, false)), "the dat's picture");
    assert_eq!(d.flags, flags::HAS_BASE | flags::HAS_NORMAL);
    assert_eq!((d.roughness, d.metal), SurfaceKind::Plain.roughness_metal());

    let mut whole = surface(IMAGE);
    whole.orig_texture_id = Some(DataId(0x0500_0020));
    let all = hook.maps(DataId(0x0800_0002), whole.orig_texture_id);
    let d = describe_surface(&whole, false, None, &kinds, all);
    assert_eq!(d.base, Some(TextureKey::Pack("material:1:albedo".into())));
    assert_eq!(
        d.flags,
        flags::HAS_BASE
            | flags::HAS_NORMAL
            | flags::HAS_HEIGHT
            | flags::HAS_ROUGH_METAL
            | flags::HAS_OCCLUSION
            | flags::PACKED_SURFACE
    );
    // Every key names a picture the pack can hand over, in the layout its blocks are in.
    let engine_pack = EnginePack::new(std::sync::Arc::clone(&pack));
    let px = engine_pack
        .pixels("material:1:normal")
        .expect("the normal's pixels");
    assert_eq!(px.layout, pyreal_engine::asset::texture::PixelLayout::Bc5);
    assert!(engine_pack.pixels("material:0:albedo").is_none());

    // The surface map will not load: roughness, height and occlusion fall back together.
    let r = MaterialRecord::new(&d, |k| match k {
        TextureKey::Pack(name) if name.ends_with("surface") => None,
        _ => Some(3),
    });
    assert_eq!(
        r.flags,
        flags::HAS_BASE | flags::HAS_NORMAL | flags::PACKED_SURFACE
    );
    assert_eq!(&r.textures[2..5], &[NO_TEXTURE; 3]);

    let none = hook.maps(DataId(0x0800_0003), Some(DataId(0x0500_0030)));
    assert_eq!(none, PackMaps::default());
}

/// A pack material's kind comes from its grit (its metal, which its surface map does not hold) or
/// its sheen; a material with neither keeps the table's kind.
#[test]
fn a_pack_materials_grit_names_its_kind() {
    use pyreal_engine::asset::material::MaterialOverrides;
    use pyreal_engine::pack::{material_kind, PackOverrides};
    let mut pack = (*pack()).clone();
    pack.grit_names = vec!["grit/metal.png".into(), "grit/rock.png".into()];
    pack.materials[0].grit = Some(0);
    pack.materials[1].grit = Some(1);
    let mut silk = pack.materials[1].clone();
    silk.textures = vec![DataId(0x0500_0030)];
    silk.sheen = 0.5;
    let mut bare = pack.materials[1].clone();
    bare.textures = vec![DataId(0x0500_0040)];
    bare.grit = None;
    pack.materials.extend([silk, bare]);
    assert_eq!(material_kind(&pack, 0), Some(SurfaceKind::Metal));
    assert_eq!(material_kind(&pack, 1), Some(SurfaceKind::Stone));
    assert_eq!(material_kind(&pack, 2), Some(SurfaceKind::Cloth));
    assert_eq!(material_kind(&pack, 3), None);

    let hook = PackOverrides(std::sync::Arc::new(pack));
    let kinds = KindTable::default();
    let maps = hook.maps(DataId(0x0800_0001), Some(DataId(0x0500_0010)));
    let d = describe_surface(&surface(IMAGE), false, None, &kinds, maps);
    assert_eq!((d.kind, d.metal), (SurfaceKind::Metal, 1.0));
    let maps = hook.maps(DataId(0x0800_0004), Some(DataId(0x0500_0040)));
    let d = describe_surface(&surface(IMAGE), false, None, &kinds, maps);
    assert_eq!((d.kind, d.metal), (SurfaceKind::Plain, 0.0));
}

#[test]
fn the_lava_cells_are_molten_in_every_era() {
    let kinds = KindTable::default();
    for id in [0x0500_2A45, 0x0500_1824] {
        assert_eq!(
            kinds.kind(Some(DataId(id))),
            SurfaceKind::Molten,
            "{id:#010X}"
        );
    }
}

/// A clear map makes a clear surface clear only where it says: the record carries it in the metal
/// map's slot for the clear-ice shader, and a surface that is not clear ignores it.
#[test]
fn a_clear_map_rides_in_the_metal_maps_slot() {
    let matter = pyreal_assets::art_pack::Matter {
        clear: Some(0.6),
        refraction: 1.5,
        ..Default::default()
    };
    let bottle = PackMaps {
        clear: Some(TextureKey::Pack("material:0:clear".into())),
        matter,
        ..PackMaps::default()
    };
    let d = describe_surface(
        &surface(IMAGE),
        false,
        None,
        &KindTable::default(),
        bottle.clone(),
    );
    assert_ne!(d.flags & flags::TINTED_ICE, 0);
    assert_ne!(d.flags & flags::HAS_CLEAR, 0);
    let r = MaterialRecord::new(&d, |k| match k {
        TextureKey::Pack(_) => Some(9),
        _ => Some(7),
    });
    assert_eq!((r.textures[5], r.emissive[1]), (9, 0.6));
    assert_eq!(r.flags & flags::HAS_METAL, 0);
    // Not clear, the map is not laid.
    let plain = PackMaps {
        matter: pyreal_assets::art_pack::Matter::default(),
        ..bottle
    };
    let d = describe_surface(&surface(IMAGE), false, None, &KindTable::default(), plain);
    assert_eq!(d.flags & flags::HAS_CLEAR, 0);
    assert_eq!(MaterialRecord::new(&d, |_| Some(7)).textures[5], NO_TEXTURE);
}

/// A metal map makes a surface a metal only where it says: its picture is not brightened whole,
/// and the record carries the brightening and the map for the shader to lay where it is metal.
#[test]
fn a_metal_map_makes_a_metal_only_where_it_says() {
    let matter = pyreal_assets::art_pack::Matter {
        metal: Some(2.0),
        ..Default::default()
    };
    let banded = PackMaps {
        metal: Some(TextureKey::Pack("material:0:metal".into())),
        matter,
        ..PackMaps::default()
    };
    let d = describe_surface(&surface(IMAGE), false, None, &KindTable::default(), banded);
    assert_eq!((d.kind, d.metal), (SurfaceKind::Metal, 1.0));
    assert_eq!(d.base_color, [1.0; 4]);
    assert_ne!(d.flags & flags::HAS_METAL, 0);
    let r = MaterialRecord::new(&d, |k| match k {
        TextureKey::Pack(_) => Some(9),
        TextureKey::Dat { .. } => Some(7),
        TextureKey::ClassicGround { .. }
        | TextureKey::LaterGround { .. }
        | TextureKey::LaterPicture { .. }
        | TextureKey::FaceMap { .. }
        | TextureKey::FaceEdges { .. }
        | TextureKey::FaceTile
        | TextureKey::LaterModel { .. } => None,
    });
    assert_eq!((r.textures[5], r.emissive[3]), (9, 2.0));
    // Wholly metal, it is brightened whole and names no map.
    let iron = PackMaps {
        matter,
        ..PackMaps::default()
    };
    let d = describe_surface(&surface(IMAGE), false, None, &KindTable::default(), iron);
    assert_eq!(d.base_color, [2.0, 2.0, 2.0, 1.0]);
    assert_eq!(d.flags & flags::HAS_METAL, 0);
    let r = MaterialRecord::new(&d, |_| Some(7));
    assert_eq!((r.textures[5], r.emissive[3]), (NO_TEXTURE, 1.0));
}
