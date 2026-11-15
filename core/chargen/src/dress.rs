//! The character-generation update tail that dresses a creation's body: the object description
//! a heritage, a sex, a face and four garments make, in the client's order. The creation preview
//! (`pyreal-scene`) and the new engine's viewer (`pyreal-engine-view`) dress bodies with it.

use std::collections::BTreeMap;

use crate::CharGenState;
use pyreal_animation::parts::{AnimPartChange, ObjDesc, PaletteRange, TextureMapChange};
use pyreal_assets::motion::ClothingTable;
use pyreal_assets::tables::{CharGen, GearItem, ObjDesc as CgObjDesc};
use pyreal_primitives::DataId;

/// A `Subpalette`'s `offset` / `numcolors` are palette **entries** everywhere the client keeps
/// them in memory -- `ClothingTable`'s ranges, the char-gen table's `ObjDesc`s, and the three
/// literal ranges -- but [`PaletteRange`] carries the **wire**
/// form, which is those values divided by eight. Every value that
/// crosses into an [`ObjDesc`] here is divided by this; `ExpandedPalette::apply_subpalette` multiplies
/// it back.
const PALETTE_UNIT: u32 = 8;

/// The whole 2,048-entry palette expressed in wire units, which is 256.
const WHOLE_PALETTE_UNITS: u32 = 2048 / PALETTE_UNIT;

/// `ObjDesc`'s three list caps: a list count of `0xff` discards the new entry and fails.
const OBJDESC_LIST_MAX: usize = 0xFF;

/// Add an animation-part change after removing the first earlier change for the same part.
///
/// The duplicate test compares the two entries' identity fields; at that
/// offset an `AnimPartChange` holds its `part_index`, so **a later change for the same part
/// replaces the earlier one and moves to the end of the list**. Only the first match is removed,
/// which is the client's loop.
pub fn add_anim_part_change(od: &mut ObjDesc, c: AnimPartChange) {
    if let Some(i) = od
        .part_changes
        .iter()
        .position(|x| x.part_index == c.part_index)
    {
        od.part_changes.remove(i);
    }
    if od.part_changes.len() < OBJDESC_LIST_MAX {
        od.part_changes.push(c);
    }
}

/// Add a texture-map change, whose duplicate identity is the same `part_index` **and** the same
/// `old_tex_id`.
/// Two garments substituting different original textures on one part therefore both survive.
pub fn add_texture_map_change(od: &mut ObjDesc, c: TextureMapChange) {
    if let Some(i) = od
        .texture_changes
        .iter()
        .position(|x| x.part_index == c.part_index && x.old_texture == c.old_texture)
    {
        od.texture_changes.remove(i);
    }
    if od.texture_changes.len() < OBJDESC_LIST_MAX {
        od.texture_changes.push(c);
    }
}

/// the one of the three with real precedence rules.
///
/// If any existing subpalette supercedes the new one, the new one is dropped (and that counts as
/// success). Otherwise **every** entry the new one replaces is removed, not just the first; a
/// list already at `0xff` entries refuses the new one; else it is appended. Here
/// `supercedes(a, b)` = *a covers the whole palette and b does not*, and
/// `replaces(new, old)` = *same (offset, numcolors)* **or** *new covers the whole palette*. So a
/// full-palette range already present wins over anything narrower, and a full-palette range being
/// added sweeps every narrower one away. Ranges are in wire units here, hence
/// [`WHOLE_PALETTE_UNITS`].
pub fn add_subpalette(od: &mut ObjDesc, s: PaletteRange) {
    let whole = |r: &PaletteRange| r.offset == 0 && r.length == WHOLE_PALETTE_UNITS;
    if od.subpalettes.iter().any(|e| whole(e) && !whole(&s)) {
        return;
    }
    od.subpalettes
        .retain(|e| !((s.offset == e.offset && s.length == e.length) || whole(&s)));
    if od.subpalettes.len() < OBJDESC_LIST_MAX {
        od.subpalettes.push(s);
    }
}

/// Merging an object descriptor applies subpalettes, then texture changes, then part changes, each
/// through its own add step. **It does not copy the palette id**, which is why the client's
/// update assigns the sex record's base palette by hand before the first merge.
pub fn merge_objdesc(od: &mut ObjDesc, src: &CgObjDesc) {
    for s in &src.subpalettes {
        add_subpalette(
            od,
            PaletteRange {
                palette_set: s.subpalette,
                offset: s.offset / PALETTE_UNIT,
                length: s.num_colors / PALETTE_UNIT,
            },
        );
    }
    for (part, old, new) in &src.texture_changes {
        add_texture_map_change(
            od,
            TextureMapChange {
                part_index: u32::from(*part),
                old_texture: *old,
                new_texture: *new,
            },
        );
    }
    for (part, id) in &src.anim_part_changes {
        add_anim_part_change(
            od,
            AnimPartChange {
                part_index: u32::from(*part),
                part_id: *id,
            },
        );
    }
}

/// Select a palette for a `shade` from an already-loaded palette list.
///
/// An empty list, or a shade outside `[0, 1]`, gives `INVALID_DID`; otherwise the index is
/// `(num_pals - 0.000001) * shade`, truncated.
///
/// The `- 0.000001` is what keeps `shade == 1.0` off the end. A shade of **-1** -- the reset value
/// of the skin and hair shades until something rolls
/// them -- is rejected, and the client still adds the resulting `INVALID_DID` subpalette.
#[must_use]
pub fn pal_set_palette_id(palettes: &[DataId], shade: f64) -> DataId {
    if palettes.is_empty() || !(0.0..=1.0).contains(&shade) {
        return DataId(0);
    }
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    // LINT-OK: the client's own truncation of `(num_pals - 0.000001) * shade`. The product is in
    // `[0, num_pals)` by construction, so the truncation is neither negative nor out of range.
    let i = ((palettes.len() as f64 - 0.000_001) * shade) as usize;
    palettes.get(i).copied().unwrap_or(DataId(0))
}

/// Build an object descriptor from `setup`, `template_key`, a shade package, and an output value.
///
/// Apply one `AnimPartChange` per object effect before the `TextureMapChange`s nested beneath it.
/// When `template_key != 0`, apply the palette template's sub-palette effects afterward, resolving
/// each through [`pal_set_palette_id`].
///
/// A shade package copies its single `double` into all four slots and
/// reading a value clamps the index to 3, so **every** effect of one garment takes the same
/// shade; the four-value form exists only for the server's dye packages. Hence the scalar.
///
/// Returns false exactly where the client returns 0: no clothing base for this setup, no such
/// palette template, or a `PalSet` that will not load. Whatever was applied before the failure
/// stays applied, as it does in the client, whose preview update ignores the return.
///
/// **The client's fallback chain is not reproduced.** When the clothing table's per-setup base
/// map has no entry for the wearer's setup, the client retries with one of ~30 hard-coded per-race/sex defaults
/// (the human-male default, the Umbraen-female default, …) chosen by a chain of
/// setup-id comparisons against globals this build does not carry. Every char-gen garment is
/// listed for the body that offers it, so the chain is unreachable from the wizard; the miss is
/// **counted** rather than swallowed -- [`ChargenDressStats::clothing_base_missing`].
pub fn build_obj_desc(
    table: &ClothingTable,
    setup: DataId,
    template_key: u32,
    shade: f64,
    palettes: &mut dyn FnMut(DataId) -> Vec<DataId>,
    out: &mut ObjDesc,
) -> bool {
    let Some(base) = table.clothing_bases.get(&setup) else {
        return false;
    };
    for eff in base {
        add_anim_part_change(
            out,
            AnimPartChange {
                part_index: eff.part_num,
                part_id: eff.object_id,
            },
        );
        for t in &eff.texture_effects {
            add_texture_map_change(
                out,
                TextureMapChange {
                    part_index: eff.part_num,
                    old_texture: t.old_texture,
                    new_texture: t.new_texture,
                },
            );
        }
    }
    if template_key == 0 {
        // "No dye": the object and texture changes only.
        return true;
    }
    let Some(tpl) = table.palette_templates.get(&template_key) else {
        return false;
    };
    let mut ok = true;
    for eff in &tpl.subpalette_effects {
        let set = palettes(eff.palette_set);
        if set.is_empty() {
            ok = false;
            continue;
        }
        let sub = pal_set_palette_id(&set, shade);
        for r in &eff.ranges {
            add_subpalette(
                out,
                PaletteRange {
                    palette_set: sub,
                    offset: r.offset / PALETTE_UNIT,
                    length: r.length / PALETTE_UNIT,
                },
            );
        }
    }
    ok
}

/// What [`chargen_objdesc`] reached, so a caller -- and a test -- can tell an empty descriptor
/// from a descriptor that was never asked for.
///
/// Every field is a count of a stage that *ran*, except the two that count failures. A zero without
/// a denominator cannot distinguish inactivity from a missing path; these are the denominators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChargenDressStats {
    /// How many `+=` merges ran after the sex's base object description: hair style, eyes strip,
    /// nose strip, mouth strip -- four call sites over the row's three descriptor *families*.
    pub merges: u32,
    /// How many of the **four** calls were reached. A style of -1
    /// ("nothing worn") reaches none, which is the client's own `style != -1` test.
    pub clothing_calls: u32,
    /// How many of those returned true.
    pub clothing_ok: u32,
    /// Calls that found no `ClothingBase` for the wearer's setup -- the arm whose hard-coded
    /// fallback chain is not reproduced -- plus any whose `ClothingTable` was not in the closed
    /// set the host loaded.
    pub clothing_base_missing: u32,
    /// How many of the **three** `Subpalette`s were added: skin (offset 0, `0xC0` colours), hair
    /// (`0xC0`, `0x40`) and the eye colour (`0x100`, `0x40`).
    pub subpalettes: u32,
    /// Subpalettes whose palette resolved to `INVALID_DID` -- an unrolled shade of -1, or a
    /// `PalSet` that would not load. The client adds them anyway and so does this.
    pub subpalette_unresolved: u32,
}

/// A garment row: the style list it comes from, the chosen style, the palette-template ids
/// the colour spots index, the chosen colour and the chosen shade -- one tuple per
/// clothing-description build in the character-generation preview update.
type GearRow<'a> = (&'a [GearItem], i32, &'a [u32], i32, f64);

/// Assemble the character-generation preview's appearance.
///
/// This is the whole of what dresses the char-gen model, in the client's own order:
///
/// 1. the palette id is the sex record's base palette, then its base object description is
///    merged in;
/// 2. the chosen hair style's object description is merged in;
/// 3. four clothing-description builds from the setup, the selected colour's palette-template id,
///    and the item's selected shade — **headgear, trousers, shirt, footwear, in that order** —
///    with any item whose style is -1 skipped;
/// 4. the eye strip's normal or bald object description according to
///    the chosen *hair style*'s `bald` byte, then the nose and mouth
///    strips' object descriptions;
/// 5. three `Subpalette`s -- skin selected from the sex record's skin `PalSet` at the skin shade, at
///    offset **0** for **0xC0** colours; hair from the chosen hair *colour*'s `PalSet` at
///    **0xC0**/**0x40**; and the chosen eye colour, which is a `Palette` id needing no `PalSet`
///    lookup at all, at **0x100**/**0x40**.
///
/// The ordering is load-bearing: the clothing goes on **between** the hair and the face, and the
/// three subpalettes come last, after every subpalette a garment's dye contributed.
///
/// The skin subpalette is added **unconditionally** -- it is the one of the three with no
/// `!= -1` guard -- so a wizard that has rolled nothing still gets an entry, with an
/// `INVALID_DID` palette that `PaletteComposition::build` then counts as a failed range. That is the
/// client.
///
/// `palettes` resolves a `PalSet` from `(id, 0x18)` to its palette id list. It is a
/// closure so the dat read stays with the caller and the block itself is testable against a stub.
#[must_use]
pub fn chargen_objdesc(
    cg: &CharGen,
    state: &CharGenState,
    clothing: &BTreeMap<DataId, ClothingTable>,
    setup: DataId,
    palettes: &mut dyn FnMut(DataId) -> Vec<DataId>,
) -> (ObjDesc, ChargenDressStats) {
    let mut od = ObjDesc::default();
    let mut st = ChargenDressStats::default();
    // Retail requires a non-zero heritage group and gender -- the whole update is inside that test.
    let Some(hg) = cg.heritage_groups.get(&state.heritage_group) else {
        return (od, st);
    };
    let Some(sx) = hg.sexes.get(&state.gender) else {
        return (od, st);
    };

    od.palette_id = sx.base_palette;
    merge_objdesc(&mut od, &sx.base_objdesc);

    let pick = |i: i32, n: usize| usize::try_from(i).ok().filter(|i| *i < n);
    if let Some(h) = pick(state.hair_style, sx.hair_styles.len()) {
        merge_objdesc(&mut od, &sx.hair_styles[h].objdesc);
        st.merges += 1;
    }

    // The four garments, in the client's update order. Each garment's palette-template lookup is
    // `(0 <= colour && colour < colour_count) ? ids[colour] : 0`, and a key of 0 is the clothing
    // build's own "no dye".
    let gear: [GearRow<'_>; 4] = [
        (
            &sx.headgear,
            state.headgear_style,
            &state.headgear_palette_template_ids,
            state.headgear_color,
            state.headgear_shade,
        ),
        (
            &sx.pants,
            state.trousers_style,
            &state.trousers_palette_template_ids,
            state.trousers_color,
            state.trousers_shade,
        ),
        (
            &sx.shirts,
            state.shirt_style,
            &state.shirt_palette_template_ids,
            state.shirt_color,
            state.shirt_shade,
        ),
        (
            &sx.footwear,
            state.footwear_style,
            &state.footwear_palette_template_ids,
            state.footwear_color,
            state.footwear_shade,
        ),
    ];
    for (list, style, ids, color, shade) in gear {
        let Some(item) = pick(style, list.len()).map(|i| &list[i]) else {
            continue;
        };
        st.clothing_calls += 1;
        let Some(table) = clothing.get(&item.clothing_table) else {
            st.clothing_base_missing += 1;
            continue;
        };
        if !table.clothing_bases.contains_key(&setup) {
            st.clothing_base_missing += 1;
        }
        let key = pick(color, ids.len()).map_or(0, |i| ids[i]);
        if build_obj_desc(table, setup, key, shade, palettes, &mut od) {
            st.clothing_ok += 1;
        }
    }

    // The bald state is the chosen **hair style**'s `bald` byte, not a state of its
    // own: a bald head needs the eyes strip drawn onto scalp rather than under a fringe.
    if let Some(e) = pick(state.eyes_strip, sx.eye_strips.len()) {
        let bald = pick(state.hair_style, sx.hair_styles.len())
            .is_some_and(|h| sx.hair_styles[h].bald != 0);
        let strip = &sx.eye_strips[e];
        merge_objdesc(
            &mut od,
            if bald {
                &strip.objdesc_bald
            } else {
                &strip.objdesc
            },
        );
        st.merges += 1;
    }
    if let Some(n) = pick(state.nose_strip, sx.nose_strips.len()) {
        merge_objdesc(&mut od, &sx.nose_strips[n].1);
        st.merges += 1;
    }
    if let Some(m) = pick(state.mouth_strip, sx.mouth_strips.len()) {
        merge_objdesc(&mut od, &sx.mouth_strips[m].1);
        st.merges += 1;
    }

    // The three `Subpalette`s. Offsets and lengths are the client's literals in palette entries;
    // `add_subpalette` takes wire units, hence the divisions.
    let sub = |od: &mut ObjDesc, st: &mut ChargenDressStats, pal: DataId, off: u32, n: u32| {
        if pal.0 == 0 {
            st.subpalette_unresolved += 1;
        }
        add_subpalette(
            od,
            PaletteRange {
                palette_set: pal,
                offset: off / PALETTE_UNIT,
                length: n / PALETTE_UNIT,
            },
        );
        st.subpalettes += 1;
    };
    // Skin -- unguarded in the client, so unguarded here.
    let skin = pal_set_palette_id(&palettes(sx.skin_palset), state.skin_shade);
    sub(&mut od, &mut st, skin, 0, 0xC0);
    if let Some(c) = pick(state.hair_color, sx.hair_colors.len()) {
        let set = palettes(DataId(sx.hair_colors[c]));
        let hair = pal_set_palette_id(&set, state.hair_shade);
        sub(&mut od, &mut st, hair, 0xC0, 0x40);
    }
    // The eye-colour list holds `Palette` ids, not `PalSet`s: `Update` uses the entry directly and
    // makes no call for it at all.
    if let Some(c) = pick(state.eye_color, sx.eye_colors.len()) {
        sub(&mut od, &mut st, DataId(sx.eye_colors[c]), 0x100, 0x40);
    }
    (od, st)
}
