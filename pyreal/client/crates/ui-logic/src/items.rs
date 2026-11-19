//! A slot's picture as an icon recipe: the enum lookups an object's or a spell's icon is
//! composited from.

use pyreal_client_contract::view::SlotDecoration;
use pyreal_primitives::DataId;

use crate::icon::IconRecipe;

/// The icon data's render icons and the client magic system's composite spell icon —
/// **the enum lookups a slot's picture is composited out of.**
///
/// # What retail composites
///
/// A slot's picture is not the raw icon id. It is the object's main icon surface, a 32×32 local surface
/// the icon renderer builds once per object out of **six** blits into two surfaces:
///
/// ```text
/// drag surface = new 32x32
///   blit the icon, normal
///   blit the custom overlay, 4-alpha
///   replace white with UIEffectIcons[effects row or Default]
/// main surface = new 32x32
///   blit UIIconBackgrounds[lowest set bit of item type + 1], normal
///   blit the custom underlay, 3-alpha
///   blit the drag surface, 3-alpha
/// ```
///
/// The **fourth** blit is the background: an opaque normal blit under everything else, keyed
/// by the item's own `ITEM_TYPE`. `0x10000004` is `UIIconBackgrounds`
/// (mapper `0x25000008`, 34 rows)
/// and its fourteen distinct surfaces `0x060011CB`…`0x060011F4` plus `0x06005E23` are exactly
/// the red / blue / teal / green / brown tiles in the recorded retail backpack frame.
///
///
/// **Why the order cannot be flattened.** The drag surface is finished *before* it is
/// alpha-blended onto the main surface. Writing the six as one ordered list would put
/// a normal blit of the icon straight onto the tile and erase it; the tile survives only because
/// the icon reaches it through the drag surface's own alpha.
///
/// This module owns the **enum lookups** — the four `DidMapper` groups the two composites
/// index and the index arithmetic for each. The blending is
/// [`crate::icon::SurfaceOp::blit_3alpha`] / [`blit_4alpha`](crate::icon::SurfaceOp::blit_4alpha)
/// and the compositing is [`crate::icon::composite`].
pub mod icon_background {
    /// The `DidMapper 0x25000000` group `UIIconBackgrounds`.
    pub const ITEM_TYPE_GROUP: u32 = 0x1000_0004;

    /// The **local player's** backpack picture, and it is not a row of
    /// [`ITEM_TYPE_GROUP`] at all.
    ///
    /// The icon renderer asks for the enum pair `(0x10000004, 7)`, and the two arguments are
    /// **(value, group)**, not (group, value). Both enum-lookup helpers hand their pair straight
    /// to the DID-from-enum cache in the same order, so the surface lookup `(value, group, type)`
    /// and the DID lookup `(value, group)` are the *same* convention. The type-tile lookup in the
    /// same renderer, `(lowest set bit of type + 1, 0x10000004, 0x0C)`, is the control: there the
    /// row is first and `0x10000004` is second, exactly as here.
    ///
    /// So the substitution is group **7**, row **`0x10000004`** — and the two orders resolve to
    /// two different pictures in the shipped dat: `(7, 0x10000004)` is `0x0600127E`, a backpack on
    /// a transparent field, while `(0x10000004, 7)` is `0x060011F4`, another opaque
    /// `UIIconBackgrounds` tile. Reading the pair the wrong way round therefore blitted a second
    /// *background* over the container tile through a 3-alpha blit, which is why the cell's
    /// backdrop still looked right and no backpack was ever drawn on it.
    pub const PLAYER_ICON_GROUP: u32 = 7;

    /// The group's own catch-all row.
    ///
    /// The icon renderer takes the item type's lowest set bit + 1 and substitutes `0x21` when that is
    /// 0, and the lowest-set-bit helper returns **-1** for a zero mask (it walks bits 0…31 and
    /// falls off the end), so `0x21` is reached by exactly one input: an item type of 0. Every other
    /// row is `bit + 1`, which is why the mapper has 34 entries for 32 bits.
    pub const DEFAULT_INDEX: u32 = 0x21;

    /// The item type's lowest set bit + 1, with the client's `== 0` fallback.
    #[must_use]
    pub const fn enum_index(item_type: u32) -> u32 {
        if item_type == 0 {
            DEFAULT_INDEX
        } else {
            item_type.trailing_zeros() + 1
        }
    }

    // ---- the other four groups the two composites read -------------------------------------------

    /// `UIEffectIcons` (mapper `0x25000009`, 14 rows): the drag icon's contour replacement.
    pub const EFFECT_GROUP: u32 = 0x1000_0005;
    /// `UISpellBackgrounds` (mapper `0x2500000A`, 12 rows) — the spell-icon composite's opaque
    /// base, indexed by the power level of the spell's power component.
    pub const SPELL_BACKGROUND_GROUP: u32 = 0x1000_0006;
    /// `UISpellOverlays` (mapper `0x2500000B`) — both the reversed/non-reversed wash and the
    /// fellowship / self-targeted badge come out of this one group at four different rows.
    pub const SPELL_OVERLAY_GROUP: u32 = 0x1000_0007;

    /// The effects mask's lowest set bit + 1, with no substitution into the index.
    /// Zero asks row 0 (UNDEF); a missing/zero result falls back to row 0x21.
    #[must_use]
    pub const fn effect_index(effects: u32) -> u32 {
        if effects == 0 {
            0
        } else {
            effects.trailing_zeros() + 1
        }
    }

    /// The effect row an object's effects select, with the default row behind it, through an enum
    /// lookup `(group, value)`.
    #[must_use]
    pub fn effect_surface_by(
        lookup: &dyn Fn(u32, u32) -> Option<pyreal_primitives::DataId>,
        effects: u32,
    ) -> Option<pyreal_primitives::DataId> {
        lookup(EFFECT_GROUP, effect_index(effects)).or_else(|| lookup(EFFECT_GROUP, DEFAULT_INDEX))
    }
}

/// The object icon's recipe: the five surfaces, resolved through an enum lookup `(group, value)`.
#[must_use]
pub fn object_recipe_by(
    lookup: &dyn Fn(u32, u32) -> Option<DataId>,
    d: &SlotDecoration,
) -> IconRecipe {
    // "Is the local player", not "is a player": the icon renderer substitutes the backpack
    // picture and TYPE_CONTAINER for the local player only. The lookup is `(0x10000004, 7)` =
    // **(value, group)** — see `icon_background::PLAYER_ICON_GROUP` for what settles the order.
    let (item_type, icon) = if d.is_player {
        (
            0x200,
            lookup(
                icon_background::PLAYER_ICON_GROUP,
                icon_background::ITEM_TYPE_GROUP,
            ),
        )
    } else {
        (d.obj_type, (d.icon_id != 0).then_some(DataId(d.icon_id)))
    };
    IconRecipe::Object {
        background: lookup(
            icon_background::ITEM_TYPE_GROUP,
            icon_background::enum_index(item_type),
        ),
        effects: icon_background::effect_surface_by(lookup, d.effects),
        icon,
        overlay: d.icon_overlay_id.filter(|x| x.0 != 0),
        underlay: d.icon_underlay_id.filter(|x| x.0 != 0),
    }
}

/// The spell icon's recipe, resolved through an enum lookup `(group, value)`: `power` is the first
/// component's power and `bitfield` the spell's bitfield.
#[must_use]
pub fn spell_recipe_by(
    lookup: &dyn Fn(u32, u32) -> Option<DataId>,
    power: u32,
    icon: Option<DataId>,
    bitfield: u32,
) -> IconRecipe {
    let layers = pyreal_presentation::spell::SpellIconLayers::new(power, bitfield);
    IconRecipe::Spell {
        background: lookup(icon_background::SPELL_BACKGROUND_GROUP, layers.power),
        icon: icon.filter(|x| x.0 != 0),
        tint: lookup(icon_background::SPELL_OVERLAY_GROUP, layers.tint_key()),
        overlay: layers
            .badge_key()
            .and_then(|i| lookup(icon_background::SPELL_OVERLAY_GROUP, i)),
    }
}
