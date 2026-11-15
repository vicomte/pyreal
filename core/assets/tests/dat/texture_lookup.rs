use pyreal_assets::texture_lookup::{LookupError, TextureLookup};
use pyreal_assets::{Decode, SurfaceTexture};
use pyreal_dat::DbType;
use pyreal_primitives::DataId;

#[test]
fn direct_surfaces_resolve_to_their_own_record_and_non_textures_are_rejected() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let lookup = TextureLookup::new(&store, 2);
    let id = DataId(0x0600_7576);
    let (resolved, surface, bytes) = lookup.resolve(id).expect("direct surface");
    assert_eq!(resolved, id);
    assert_eq!(surface.format, 500);
    assert_eq!(bytes, store.read_typed(DbType::RenderSurface, id).unwrap());
    assert!(matches!(
        lookup.resolve(DataId(0)),
        Err(LookupError::NotATexture(DataId(0)))
    ));
}

#[test]
fn two_level_chains_choose_high_detail_only_when_granted_and_requested() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let (id, levels) = store
        .ids_of(DbType::SurfaceTexture)
        .into_iter()
        .find_map(|id| {
            let bytes = store.read_typed(DbType::SurfaceTexture, id).ok()?;
            let texture = SurfaceTexture::decode_payload(id, &bytes).ok()?;
            (texture.source_levels.len() == 2).then_some((id, texture.source_levels))
        })
        .expect("a two-level retail texture");
    let before = TextureLookup::new(&store, 0);
    assert!(!before.keeps_high_detail());
    assert_eq!(before.resolve(id).unwrap().0, levels[1]);
    assert!(store.grant_highres().unwrap());
    let highest = TextureLookup::new(&store, 0);
    let medium = TextureLookup::new(&store, 2);
    assert!(highest.keeps_high_detail());
    assert!(!medium.keeps_high_detail());
    assert_eq!(highest.resolve(id).unwrap().0, levels[0]);
    assert_eq!(medium.resolve(id).unwrap().0, levels[1]);
}

#[test]
fn a_high_res_level_in_another_pixel_format_than_the_original_is_passed_over() {
    let store = pyreal_dat::testing::open_store_or_fail();
    assert!(store.grant_highres().unwrap());
    let highest = TextureLookup::new(&store, 0);
    // Same format at both levels: the high-res level.
    assert_eq!(
        highest.resolve(DataId(0x0500_000C)).unwrap().0,
        DataId(0x0600_3B9D)
    );
    // A palette-indexed original whose high-res level is DXT1 keeps the original, and its palette.
    let (id, surface, _) = highest.resolve(DataId(0x0500_0ECE)).unwrap();
    assert_eq!(id, DataId(0x0600_45B4));
    assert!(surface.default_palette_id.is_some());
    // A DXT5 original whose high-res level is DXT1 keeps the original, and its alpha.
    assert_eq!(
        highest.resolve(DataId(0x0500_27D0)).unwrap().0,
        DataId(0x0600_3F7B)
    );
}

#[test]
fn palette_lookup_preserves_every_stored_colour() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let lookup = TextureLookup::new(&store, 2);
    let id = store.ids_of(DbType::Palette)[0];
    let bytes = store.read_typed(DbType::Palette, id).unwrap();
    let expected = pyreal_assets::Palette::decode_payload(id, &bytes).unwrap();
    assert_eq!(
        lookup.palette(id).unwrap().colors_argb,
        expected.colors_argb
    );
}

#[test]
fn a_surface_has_its_indexed_pictures_palette_and_a_solid_one_has_none() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let lookup = TextureLookup::new(&store, 1);
    let surface = |id: u32| {
        let id = DataId(id);
        let bytes = store.read_typed(DbType::Surface, id).unwrap();
        pyreal_assets::Surface::decode_payload(id, &bytes).unwrap()
    };
    // A costume limb's surface names no palette; its indexed picture carries its own.
    assert_eq!(surface(0x0800_0192).orig_palette_id, Some(DataId(0)));
    assert_eq!(
        lookup.surface_palette(DataId(0x0800_0192)),
        Some(DataId(0x0400_0742))
    );
    // A body surface's picture carries the body's palette.
    assert_eq!(
        lookup.surface_palette(DataId(0x0800_0519)),
        Some(DataId(0x0400_007E))
    );
    // A solid-color surface has none.
    assert_eq!(surface(0x0800_0034).color_value, Some(0xFF00_0000));
    assert_eq!(lookup.surface_palette(DataId(0x0800_0034)), None);
}
