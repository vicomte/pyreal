//! End of Retail's room pictures beside an older world ([`pyreal_engine::later_rooms`]), on the
//! February 2005 (`PYREAL_TEST_PRETOD_DAT_DIR`) and October 1999 (`PYREAL_TEST_LAUNCH_DAT_DIR`)
//! files with End of Retail's (`PYREAL_TEST_DAT_DIR`) beside them. A missing input fails.

use std::path::PathBuf;

use pyreal_dat::RetailDatStore;
use pyreal_engine::later_rooms::LaterRooms;
use pyreal_primitives::CellId;
use pyreal_world_data::env_cells::EnvCellLoader;

fn older_world(dir: Option<PathBuf>, var: &str) -> RetailDatStore {
    let dir = dir.unwrap_or_else(|| panic!("{var} is unset"));
    RetailDatStore::open_classic_with_modern(&dir, &pyreal_dat::testing::dat_dir())
        .unwrap_or_else(|e| panic!("{} did not open beside End of Retail: {e}", dir.display()))
}

/// Room `cell` of `world` draws End of Retail's picture in at least one slot, each a surveyed pair.
fn swaps_in(world: &RetailDatStore, cell: u32) -> usize {
    let mut rooms = LaterRooms::of(world).expect("End of Retail's rooms beside the world");
    let d = EnvCellLoader::new()
        .load_cell(world, CellId(cell))
        .unwrap_or_else(|| panic!("room {cell:#010X} loads"));
    let slots = rooms.slots(world, &d);
    assert_eq!(slots.len(), d.cell.surfaces.len());
    let mut swapped = 0;
    for (older, slot) in d.cell.surfaces.iter().zip(&slots) {
        let Some(slot) = slot else { continue };
        swapped += 1;
        assert_ne!(
            slot.surface, *older,
            "End of Retail renumbered its surfaces"
        );
        assert!(slot.fit.gain.iter().all(|g| g.is_finite() && *g > 0.0));
        let pictures = rooms.store();
        assert!(
            pictures.highres_granted(),
            "the later files read their high-res levels"
        );
    }
    swapped
}

#[test]
fn an_older_world_s_later_files_carry_end_of_retail_s_high_res_file() {
    let world = older_world(
        pyreal_dat::testing::classic_dat_dir(),
        "PYREAL_TEST_PRETOD_DAT_DIR",
    );
    assert!(
        !world.highres_granted(),
        "the world's own reads stay its own"
    );
    let later = world.modern_companion_files().expect("the later files");
    assert!(later.highres_granted());
}

#[test]
fn a_2005_dungeon_room_draws_end_of_retail_s_sharper_walls() {
    let world = older_world(
        pyreal_dat::testing::classic_dat_dir(),
        "PYREAL_TEST_PRETOD_DAT_DIR",
    );
    assert!(swaps_in(&world, 0x0200_0109) > 0);
}

#[test]
fn a_1999_dungeon_room_draws_end_of_retail_s_sharper_walls() {
    let world = older_world(
        pyreal_dat::testing::launch_dat_dir(),
        "PYREAL_TEST_LAUNCH_DAT_DIR",
    );
    assert!(swaps_in(&world, 0x0143_01AF) > 0);
}

#[test]
fn an_end_of_retail_world_has_no_later_rooms() {
    let world = pyreal_dat::testing::open_store_or_fail();
    assert!(LaterRooms::of(&world).is_none());
}

#[test]
fn a_once_left_out_2005_wall_draws_end_of_retail_s_picture_untinted() {
    let world = older_world(
        pyreal_dat::testing::classic_dat_dir(),
        "PYREAL_TEST_PRETOD_DAT_DIR",
    );
    let mut rooms = LaterRooms::of(&world).expect("End of Retail's rooms beside the world");
    let d = EnvCellLoader::new()
        .load_cell(&world, CellId(0x0200_0108))
        .expect("room 0x02000108 loads");
    let slots = rooms.slots(&world, &d);
    let wall = slots
        .iter()
        .flatten()
        .find(|s| s.picture.0 == 0x0500_2751)
        .expect("the wall 0x05001D2F takes End of Retail's 0x05002751");
    assert!(!wall.tinted);
    let ceiling = slots
        .iter()
        .flatten()
        .find(|s| s.picture.0 == 0x0500_2752)
        .expect("the ceiling takes End of Retail's 0x05002752");
    assert!(!ceiling.tinted);
}
