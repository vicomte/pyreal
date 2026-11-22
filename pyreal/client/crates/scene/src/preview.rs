//! The creature-mode preview spaces' plain half: the bodies a space holds (their parts,
//! animation, scripts and emitters) and the dressing of a creation model. The spaces
//! themselves, drawn by the engine, are [`crate::previews`].
//!
//! # Why this exists as a host module and not as a screen
//!
//! Two screens want the same thing and neither may draw. The portal screen wants the
//! portal object spinning behind a hidden world; the character preview viewport
//! wants the heritage model on a turntable; `SummaryPage` and the inventory paper-doll want it
//! after them. In the client all four are the *same* `Viewport`, and the space is a
//! member of the element rather than of the screen. This module is that member.
//!
//! # The two ids the client never writes down, and where they came from
//!
//! Preview initialization resolves enums `0x10000001` and `0x10000002` through the object cache.
//! That is [`pyreal_client_runtime::assets::enum_did`]'s two-level
//! lookup, and the **group is 7, `UIASSET`** -- the same group the UI sound table
//! (`0x10000003`) comes from, which is what pins it. In this dat build the master `DidMapper`
//! `0x25000000` sends group 7 to `0x25000010`, and that mapper's own enum-to-name table names
//! them: `0x10000001` is **`portalspace_background`**, setup `0x02000306`, and `0x10000002` is
//! **`portalspace_animation`**, `Animation 0x030005AC`. Neither id is written at a call site here:
//! a DDD patch may move either, and the enum is what the client carries.
//!
//! The same mapper holds the char-gen turntable's animations (`CharGenAnimation 0x10000006` and
//! the five idle variations), the paper-doll's (`PaperDollAnimation 0x10000005`) and the four
//! images the char-gen colour wheel is built from.
//!
//! # Frame counting is real here
//!
//! The portal's frame is not derived from elapsed time at a fixed rate; it is the real sequence
//! counter. `PreviewSpace::set_sequence_animation` builds an `AnimData` (clearing first if
//! asked) and `PreviewSpace::curr_frame_number` reads the current frame. The player is
//! `pyreal_animation::seq::Sequence`, which crosses whole animation frames against **elapsed
//! seconds** and accumulates nothing per frame.

use pyreal_animation::parts::PartArray;

/// The asset cache's `UIASSET` group, through which both preview enums resolve.
pub const UIASSET_GROUP: u32 = 7;

/// Enum `0x10000001` -- `portalspace_background`, the setup the teleport tunnel's object is
/// made from.
pub const ENUM_PORTALSPACE_BACKGROUND: u32 = 0x1000_0001;

/// Enum `0x10000002` -- `portalspace_animation`, the sequence it runs at 40 fps.
pub const ENUM_PORTALSPACE_ANIMATION: u32 = 0x1000_0002;

/// `PaperDollAnimation`. The inventory paper-doll wants this one; recorded here because it is the
/// same table and finding it again is the expensive part.
pub const ENUM_PAPERDOLL_ANIMATION: u32 = 0x1000_0005;

/// `CharGenAnimation` -- for every non-Olthoi heritage.
pub const ENUM_CHARGEN_ANIMATION: u32 = 0x1000_0006;

// =================================================================================================
// A preview object's body: its parts, its animation, its scripts and its particle emitters
// =================================================================================================

/// The body a preview object is: the setup's part array and animation sequence, as in the world,
/// with the setup's default script queued on its script manager.
///
/// Creating an object from a setup queues that setup's default script, in a preview space as in
/// the world, and the script is what puts an object's own particles on it: the smoke an Umbraen
/// or Penumbraen body trails, an Empyrean's glow, an Undead's wisps. The script and the emitters
/// it makes run through the same driver every object in the world has, so a preview model and the
/// same body in the world show the same effects.
///
/// The setup's motion table is **not** started: a preview object plays only the animations its
/// space is told to play (the setup's default animation until then), never its table's default
/// state. `None` when the setup will not load.
#[must_use]
pub fn preview_body(
    assets: std::sync::Arc<dyn pyreal_animation::data::AnimAssets>,
    setup: std::sync::Arc<pyreal_animation::data::SetupData>,
) -> Option<pyreal_animation::MotionDriver> {
    let mut body = pyreal_animation::MotionDriver::new(std::sync::Arc::clone(&assets));
    body.part_array = PartArray::create_setup(
        std::sync::Arc::clone(&setup),
        true,
        &mut body.sequence,
        &*assets,
    )?;
    // The setup's own default animation, queued at the client's default 30 fps. A caller that
    // wants another one replaces it.
    if let Some(anim) = setup.default_animation {
        body.sequence.clear_animations();
        body.sequence.append_animation(
            pyreal_animation::data::AnimData {
                anim_id: anim,
                low_frame: 0,
                high_frame: -1,
                framerate: 30.0,
            },
            &*assets,
        );
    }
    body.script_table = setup.default_phs_table;
    if let Some(script) = setup.default_script {
        body.scripts.add_script(script, &*assets, body.cur_time);
    }
    // A preview object stands in its space's own cell.
    body.env.in_cell = true;
    Some(body)
}

/// One step of a preview object's body at `now` seconds on its space's clock, `dt` after the last:
/// the animation advances, the parts are placed at `frame`, the hooks the animation passed run,
/// then the scripts, the emitters and the timed hooks, in the order a placed object in the world
/// runs them. The parts are placed before the scripts run because an emitter born on a part takes
/// that part's frame as where its particles start. What the body raises for the world (sounds,
/// movement) has nowhere to go in a preview and is let go.
pub fn step_preview_body(
    body: &mut pyreal_animation::MotionDriver,
    frame: &pyreal_primitives::Frame,
    dt: f64,
    now: f64,
) {
    use pyreal_physics::MotionSource;
    body.cur_time = pyreal_primitives::ServerTime(now);
    // No root motion: a preview object does not walk anywhere.
    let _ = body.advance(dt);
    body.update_parts(frame);
    body.process_hooks();
    body.update_scripts();
    body.update_particles(true);
    body.update_fp_hooks();
    let _ = body.take_events();
    let _ = body.take_effects();
}

/// The bounding box of a preview body's parts, which the identify portrait's camera is placed
/// from.
///
/// For every present part, the client takes the part's graphics-object bounding box, scales both
/// corners by the part's scale, transforms it by the part's position, and unions it into the
/// result.
///
/// Three details that are easy to miss:
///
/// * The graphics-object lookup dereferences the array base, so the box is always degrade
///   **level 0**'s, never the level currently being drawn.
/// * The function does not initialize `out`; the caller seeds both corners at the origin, so the
///   result always contains the object's origin, and a part array that loads nothing yields the
///   zero box rather than an inverted one. That seed is reproduced here.
/// * Each graphics object is read in the layout of the files it comes from: a world on the
///   February 2005 files has every mesh in the older layout, and a mesh that fails to read adds
///   nothing, which would leave the zero box and the camera standing at the body's feet.
///
/// `gfx_bound_box` itself is min/max over the `GfxObj`'s **vertex array**
/// ([`pyreal_world_data::setup::gfx_bound_box`]).
#[must_use]
pub fn parts_bounding_box(
    parts: &PartArray,
    store: &pyreal_dat::RetailDatStore,
) -> pyreal_physics::geom::BBox {
    use pyreal_assets::Decode;
    use pyreal_physics::BBoxExt;
    let mut out = pyreal_physics::geom::BBox::default();
    for part in &parts.parts {
        // Use the graphics-object array base, `gfxobj[0]`, not `part.gfxobj_id`: on the player
        // body that is a different, smaller mesh for 16 of 34 parts, and the identify portrait's
        // camera is placed from this box.
        let Some(id) = part.gfxobj_at(0) else {
            continue;
        };
        let Ok(bytes) = store.read_typed(pyreal_dat::DbType::GfxObj, id) else {
            continue;
        };
        let Ok(g) = pyreal_assets::GfxObj::decode_payload_in(store.era_of(id), id, &bytes) else {
            continue;
        };
        let b = pyreal_world_data::setup::gfx_bound_box(&g);
        let s = part.gfxobj_scale;
        let local = pyreal_physics::geom::BBox::new(
            pyreal_primitives::Vec3::new(b.min.x * s.x, b.min.y * s.y, b.min.z * s.z),
            pyreal_primitives::Vec3::new(b.max.x * s.x, b.max.y * s.y, b.max.z * s.z),
        );
        local
            .convert_to_global(&part.pos)
            .build_bounding_box(&mut out);
    }
    out
}

// =================================================================================================
// The character-generation update tail that dresses the preview model
// =================================================================================================

use std::collections::BTreeMap;

pub use pyreal_chargen::dress::{
    add_anim_part_change, add_subpalette, add_texture_map_change, build_obj_desc, chargen_objdesc,
    pal_set_palette_id, ChargenDressStats,
};
use pyreal_primitives::DataId;

/// Memoized palette-set lookup over the retail dat, the one dat read [`chargen_objdesc`] needs.
///
/// The client resolves property `0x18` for the palette-set id and lets the object cache absorb repeats;
/// this memoises the id list, which is what a repaint asks for over and over.
#[derive(Debug, Default)]
pub struct PaletteSetCache {
    sets: BTreeMap<DataId, Vec<DataId>>,
}

impl PaletteSetCache {
    /// The palette ids of one `PalSet`, or an empty list when it will not load -- which is the
    /// client's null, and which [`build_obj_desc`] reports as a failure.
    pub fn palettes(&mut self, store: &pyreal_dat::RetailDatStore, pal_set: DataId) -> Vec<DataId> {
        if let Some(hit) = self.sets.get(&pal_set) {
            return hit.clone();
        }
        let v = (|| {
            use pyreal_assets::Decode;
            let bytes = pyreal_primitives::AssetSource::read(store, pal_set).ok()?;
            let s = pyreal_assets::material::PaletteSet::decode_payload(pal_set, &bytes).ok()?;
            Some(s.palette_ids)
        })()
        .unwrap_or_default();
        self.sets.insert(pal_set, v.clone());
        v
    }
}

#[cfg(test)]
mod body_tests {
    use super::{preview_body, step_preview_body};
    use pyreal_animation::data::AnimAssets;
    use pyreal_primitives::{DataId, Frame};
    use std::sync::Arc;

    /// Run the body of `setup` for two seconds at 60 steps a second; how many emitters it has.
    fn emitters_after_two_seconds(setup: u32) -> (usize, usize) {
        let store = Arc::new(pyreal_dat::testing::open_store().expect("the retail dats"));
        let assets: Arc<dyn AnimAssets> =
            Arc::new(pyreal_world_data::anim_assets::DatAnimAssets::new(store));
        let data = assets.setup(DataId(setup)).expect("the setup");
        let mut body = preview_body(Arc::clone(&assets), data).expect("the body");
        let frame = Frame::default();
        let mut now = 0.0;
        for _ in 0..120 {
            now += 1.0 / 60.0;
            step_preview_body(&mut body, &frame, 1.0 / 60.0, now);
        }
        let live = body.particles.iter().map(|e| e.live().count()).sum();
        (body.particles.len(), live)
    }

    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
    fn an_umbraen_body_in_a_preview_trails_the_smoke_its_setup_s_default_script_makes() {
        // The Umbraen male body names a default script whose hooks create its smoke.
        let (emitters, live) = emitters_after_two_seconds(0x0200_196F);
        assert!(emitters > 0, "the default script made emitters");
        assert!(live > 0, "and they are giving off particles");
        // The human body names no default script and gives off nothing.
        assert_eq!(emitters_after_two_seconds(0x0200_0001), (0, 0));
    }
}

#[cfg(test)]
mod bounding_box_tests {
    use super::{parts_bounding_box, preview_body, step_preview_body};
    use pyreal_animation::data::AnimAssets;
    use pyreal_primitives::{DataId, Frame};
    use std::sync::Arc;

    /// How tall the box the identify portrait's camera is placed from stands, for a body of
    /// `setup` read from `store`.
    fn box_height(store: pyreal_dat::RetailDatStore, setup: u32) -> f32 {
        let store = Arc::new(store);
        let assets: Arc<dyn AnimAssets> = Arc::new(
            pyreal_world_data::anim_assets::DatAnimAssets::new(Arc::clone(&store)),
        );
        let data = assets.setup(DataId(setup)).expect("the setup");
        let mut body = preview_body(Arc::clone(&assets), data).expect("the body");
        step_preview_body(&mut body, &Frame::default(), 0.0, 0.0);
        let b = parts_bounding_box(&body.part_array, &store);
        b.max.z - b.min.z
    }

    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
    fn the_portraits_box_is_the_bodys_meshes_on_the_february_2005_files_as_on_the_last_ones() {
        const HUMAN: u32 = 0x0200_0001;
        let last = box_height(pyreal_dat::testing::open_store_or_fail(), HUMAN);
        let first = box_height(pyreal_dat::testing::open_classic_store_or_fail(), HUMAN);
        for (files, height) in [("last", last), ("February 2005", first)] {
            assert!(
                (1.5..2.2).contains(&height),
                "a person's box on the {files} files stands {height} m tall"
            );
        }
    }
}
