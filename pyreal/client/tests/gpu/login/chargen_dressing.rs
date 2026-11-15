//! The char-gen preview's dressing arithmetic: the palette-set index truncates as the client's
//! does, `build_obj_desc` emits the part, texture and palette changes, and the three object
//! description list rules are the client's.
//! Fixture: synthetic inputs; no device.

#![cfg(gpu)]

use pyreal_animation::parts::{AnimPartChange, ObjDesc, PaletteRange, TextureMapChange};
use pyreal_primitives::DataId;
use pyreal_scene::preview::pal_set_palette_id;

// =================================================================================================
// 1. The arithmetic, without a device
// =================================================================================================

/// Palette selection truncates `(palette_count - 0.000001) * shade` after rejecting
/// empty sets and out-of-range shades. The subtraction keeps shade 1.0 within the palette list.
#[test]
fn pal_set_get_palette_id_is_the_clients_truncating_index() {
    let set: Vec<DataId> = (1..=4).map(DataId).collect();
    assert_eq!(pal_set_palette_id(&set, 0.0), DataId(1));
    // At this boundary, (4 - 0.000001) * 0.25 is 0.99999975 and truncates to 0, not 1.
    // Plain palette_count * shade would answer DataId(2); the same subtraction also bounds 1.0.
    assert_eq!(pal_set_palette_id(&set, 0.25), DataId(1));
    assert_eq!(pal_set_palette_id(&set, 0.5), DataId(2));
    assert_eq!(pal_set_palette_id(&set, 0.75), DataId(3));
    assert_eq!(
        pal_set_palette_id(&set, 1.0),
        DataId(4),
        "the -0.000001 keeps 1.0 in range"
    );
    // Invalid shades or an empty set return the invalid id 0; composition can retain it.
    assert_eq!(
        pal_set_palette_id(&set, -1.0),
        DataId(0),
        "reset character state leaves shade -1"
    );
    assert_eq!(pal_set_palette_id(&set, 1.5), DataId(0));
    assert_eq!(pal_set_palette_id(&[], 0.5), DataId(0));
}

/// Build a clothing description from a synthetic table, checking part, texture and palette
/// effects plus shade selection without archive inputs.
#[test]
fn build_obj_desc_emits_the_part_texture_and_palette_changes() {
    use pyreal_assets::motion::{
        ClothingTable, ObjectEffect, PaletteEffect, PaletteRange as CloRange, PaletteTemplate,
        TextureEffect,
    };
    let table = ClothingTable {
        id: DataId(0x1000_0001),
        clothing_base_buckets: 8,
        clothing_bases: [(
            DataId(0x0200_0001),
            vec![ObjectEffect {
                part_num: 9,
                object_id: DataId(0x0100_0AAA),
                texture_effects: vec![TextureEffect {
                    old_texture: DataId(0x0500_0001),
                    new_texture: DataId(0x0500_0002),
                }],
            }],
        )]
        .into_iter()
        .collect(),
        palette_template_buckets: 8,
        palette_templates: [(
            7,
            PaletteTemplate {
                icon: DataId(0),
                subpalette_effects: vec![PaletteEffect {
                    // 0x400 entries in, 0x40 long -- 128 and 8 in wire units.
                    ranges: vec![CloRange {
                        offset: 0x400,
                        length: 0x40,
                    }],
                    palette_set: DataId(0x0F00_0001),
                }],
            },
        )]
        .into_iter()
        .collect(),
    };
    let mut sets = |_: DataId| vec![DataId(0x0400_0010), DataId(0x0400_0011)];
    let mut od = ObjDesc::default();
    assert!(pyreal_scene::preview::build_obj_desc(
        &table,
        DataId(0x0200_0001),
        7,
        1.0,
        &mut sets,
        &mut od
    ));
    assert_eq!(
        od.part_changes,
        vec![AnimPartChange {
            part_index: 9,
            part_id: DataId(0x0100_0AAA)
        }]
    );
    assert_eq!(
        od.texture_changes,
        vec![TextureMapChange {
            part_index: 9,
            old_texture: DataId(0x0500_0001),
            new_texture: DataId(0x0500_0002),
        }]
    );
    assert_eq!(
        od.subpalettes,
        vec![PaletteRange {
            palette_set: DataId(0x0400_0011),
            offset: 0x400 / 8,
            length: 0x40 / 8,
        }],
        "the range is divided into the wire's 8-entry units and the shade picks the last palette"
    );

    // `template_key == 0` is "no dye": the object and texture changes only.
    let mut plain = ObjDesc::default();
    assert!(pyreal_scene::preview::build_obj_desc(
        &table,
        DataId(0x0200_0001),
        0,
        1.0,
        &mut sets,
        &mut plain
    ));
    assert!(plain.subpalettes.is_empty(), "no template key means no dye");
    assert_eq!(plain.part_changes.len(), 1);

    // This table lacks the requested setup. The client retries per-race/sex defaults; that
    // fallback is not implemented here, so this build reports failure without applying data.
    let mut miss = ObjDesc::default();
    assert!(!pyreal_scene::preview::build_obj_desc(
        &table,
        DataId(0x0200_9999),
        7,
        1.0,
        &mut sets,
        &mut miss
    ));
    assert_eq!(
        miss,
        ObjDesc::default(),
        "nothing is applied for a body the garment does not fit"
    );
}

/// Three description-list rules shared by merges and clothing builds: replace matching ranges,
/// let a full palette supersede narrower ranges, and key geometry/texture replacements correctly.
///
/// The shipped Aluvian-male subpalettes have distinct offset/length pairs and none covers the
/// full palette, so synthetic inputs below exercise the replacement and full-palette cases.
#[test]
fn the_three_objdesc_list_rules_are_the_clients() {
    use {
        pyreal_scene::preview::add_anim_part_change, pyreal_scene::preview::add_subpalette,
        pyreal_scene::preview::add_texture_map_change,
    };
    let r = |offset, length| PaletteRange {
        palette_set: DataId(offset + 1),
        offset,
        length,
    };

    // Equal offset/length replaces the old range and appends the newcomer at the end.
    let mut od = ObjDesc::default();
    add_subpalette(&mut od, r(0x18, 8));
    add_subpalette(&mut od, r(0x20, 8));
    add_subpalette(
        &mut od,
        PaletteRange {
            palette_set: DataId(0xBEEF),
            offset: 0x18,
            length: 8,
        },
    );
    assert_eq!(
        od.subpalettes.len(),
        2,
        "the first entry was replaced, not appended to"
    );
    assert_eq!(
        od.subpalettes[1].palette_set,
        DataId(0xBEEF),
        "remove the matching subpalette then append: the replacement is last"
    );

    // A whole-palette range removes narrow ranges; once present, it blocks a later narrow range.
    let whole = PaletteRange {
        palette_set: DataId(0x77),
        offset: 0,
        length: 2048 / 8,
    };
    add_subpalette(&mut od, whole);
    assert_eq!(
        od.subpalettes,
        vec![whole],
        "the full-palette range replaced both"
    );
    add_subpalette(&mut od, r(0x28, 8));
    assert_eq!(
        od.subpalettes,
        vec![whole],
        "a narrower range cannot displace it"
    );

    // Geometry replacement keys only on part index; the latest change moves to the end.
    let mut od = ObjDesc::default();
    add_anim_part_change(
        &mut od,
        AnimPartChange {
            part_index: 9,
            part_id: DataId(1),
        },
    );
    add_anim_part_change(
        &mut od,
        AnimPartChange {
            part_index: 16,
            part_id: DataId(2),
        },
    );
    add_anim_part_change(
        &mut od,
        AnimPartChange {
            part_index: 9,
            part_id: DataId(3),
        },
    );
    assert_eq!(
        od.part_changes,
        vec![
            AnimPartChange {
                part_index: 16,
                part_id: DataId(2)
            },
            AnimPartChange {
                part_index: 9,
                part_id: DataId(3)
            },
        ]
    );

    // Texture replacement keys on both part index and old texture. Different original
    // textures on the same part survive independently.
    let mut od = ObjDesc::default();
    let t = |part, old, new| TextureMapChange {
        part_index: part,
        old_texture: DataId(old),
        new_texture: DataId(new),
    };
    add_texture_map_change(&mut od, t(9, 0x10, 0x20));
    add_texture_map_change(&mut od, t(9, 0x11, 0x21));
    add_texture_map_change(&mut od, t(9, 0x10, 0x99));
    assert_eq!(
        od.texture_changes,
        vec![t(9, 0x11, 0x21), t(9, 0x10, 0x99)],
        "only the entry with the same (part, old_tex_id) was replaced"
    );
}
