//! Drops and merges: auto-merge needs room for the amount it moves, a full open chest refuses a
//! drop without it falling to the ground, and the slider's split is only the selected item's.
//! Fixture: synthetic objects in a bare model.

use crate::common::model_replay::*;
use pyreal_client_model::objects::ObjectInventory;
use pyreal_rules::weenie::bitfield;

const ME: ObjectId = ObjectId(0x5000_0001);
const PILE: ObjectId = ObjectId(0x8000_0010);
const LOOSE: ObjectId = ObjectId(0x8000_0011);
const OTHER: ObjectId = ObjectId(0x8000_0012);
const CHEST: ObjectId = ObjectId(0x8000_0020);
const SWORD: ObjectId = ObjectId(0x8000_0030);
const PYREAL: u32 = 273;

fn stack(
    id: ObjectId,
    wcid: u32,
    size: u16,
    max: u16,
    holder: Option<ObjectId>,
) -> pyreal_client_model::Weenie {
    let mut w = pyreal_client_model::Weenie::new(id);
    w.pwd.name = "Pyreal".into();
    w.pwd.wcid = wcid;
    w.pwd.stack_size = Some(size);
    w.pwd.max_stack_size = Some(max);
    w.pwd.container_id = holder;
    if holder.is_some() {
        w.current_state = pyreal_client_model::weenie::PositionState::InContainer;
    }
    w
}

/// A character whose pack holds a pyreal pile with room for ten more, and another stack that is
/// selected with the slider at `slider`.
fn a_pack_with_room_for_ten(slider: u32) -> World {
    let mut w = World::new();
    w.player = Some(ME);
    let mut me = pyreal_client_model::Weenie::new(ME);
    me.pwd.items_capacity = Some(102);
    w.tables.weenies.insert(ME, me);
    w.tables
        .weenies
        .insert(PILE, stack(PILE, PYREAL, 90, 100, Some(ME)));
    w.tables
        .weenies
        .insert(OTHER, stack(OTHER, 999, 300, 500, Some(ME)));
    let mut inv = ObjectInventory::new(ME);
    inv.items = vec![PILE, OTHER];
    w.tables.inventories.insert(ME, inv);
    w.selected = Some(OTHER);
    w.split = SplitState {
        split_size: slider,
        max_split_size: 300,
    };
    w
}

fn merges(req: &RecordingRequests) -> Vec<(ObjectId, i32)> {
    req.0
        .iter()
        .filter_map(|r| match r {
            Request::StackableMerge(m) => Some((m.merge_to, m.amount)),
            _ => None,
        })
        .collect()
}

#[test]
fn an_unselected_stack_merges_when_it_fits_whatever_the_slider_says() {
    let mut w = a_pack_with_room_for_ten(200);
    w.tables
        .weenies
        .insert(LOOSE, stack(LOOSE, PYREAL, 5, 100, None));
    let (mut req, mut out) = (RecordingRequests::default(), RecordingSink::default());
    let split = w.split;
    assert!(w.place_in_backpack(&mut req, &mut out, LOOSE, false, split, ServerTime(1.0)));
    assert_eq!(merges(&req), vec![(PILE, 5)]);
}

#[test]
fn an_unselected_stack_too_big_for_the_room_does_not_merge() {
    let mut w = a_pack_with_room_for_ten(1);
    w.tables
        .weenies
        .insert(LOOSE, stack(LOOSE, PYREAL, 100, 100, None));
    let (mut req, mut out) = (RecordingRequests::default(), RecordingSink::default());
    let split = w.split;
    w.place_in_backpack(&mut req, &mut out, LOOSE, false, split, ServerTime(1.0));
    assert_eq!(merges(&req), vec![]);
}

/// The character beside an open chest that has no free slot, holding a sword.
fn beside_a_full_open_chest() -> World {
    let mut w = World::new();
    w.player = Some(ME);
    w.tables
        .weenies
        .insert(ME, pyreal_client_model::Weenie::new(ME));
    let mut chest = pyreal_client_model::Weenie::new(CHEST);
    chest.pwd.name = "Chest".into();
    chest.pwd.items_capacity = Some(1);
    chest.pwd.bitfield |= bitfield::OPENABLE;
    w.tables.weenies.insert(CHEST, chest);
    let mut inv = ObjectInventory::new(CHEST);
    inv.items = vec![OTHER];
    w.tables.inventories.insert(CHEST, inv);
    w.tables
        .weenies
        .insert(OTHER, stack(OTHER, 999, 1, 1, Some(CHEST)));
    let mut sword = pyreal_client_model::Weenie::new(SWORD);
    sword.pwd.name = "Sword".into();
    sword.pwd.container_id = Some(ME);
    w.tables.weenies.insert(SWORD, sword);
    let mut pack = ObjectInventory::new(ME);
    pack.items = vec![SWORD];
    w.tables.inventories.insert(ME, pack);
    w.ground_object = Some(CHEST);
    w
}

fn lines(out: &RecordingSink) -> Vec<String> {
    out.0
        .iter()
        .filter_map(|n| match n {
            Notice::DisplayString { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_full_open_chest_refuses_the_drop_once_and_nothing_falls_to_the_ground() {
    let mut w = beside_a_full_open_chest();
    let (mut req, mut out) = (RecordingRequests::default(), RecordingSink::default());
    let placed = w.attempt_place_in_3d(
        &mut req,
        &mut out,
        SWORD,
        Some(CHEST),
        true,
        true,
        SplitState::whole_stack(1),
        ServerTime(1.0),
    );
    assert!(!placed);
    assert_eq!(
        lines(&out),
        vec!["The Chest is completely full!".to_owned()]
    );
    assert!(
        !req.0.iter().any(|r| matches!(r, Request::DropItem(_))),
        "no ground drop: {:?}",
        req.0
    );
}

#[test]
fn dropping_an_unselected_stack_drops_all_of_it() {
    let mut w = a_pack_with_room_for_ten(30);
    let (mut req, mut out) = (RecordingRequests::default(), RecordingSink::default());
    let split = w.split;
    // PILE is not the selection, so the slider set for OTHER does not apply to it.
    assert!(w.attempt_place_in_3d(
        &mut req,
        &mut out,
        PILE,
        None,
        true,
        true,
        split,
        ServerTime(1.0)
    ));
    assert!(
        matches!(req.0.as_slice(), [Request::DropItem(d)] if d.item == PILE),
        "{:?}",
        req.0
    );
}

#[test]
fn wielding_an_unselected_stack_takes_all_of_it() {
    let mut w = a_pack_with_room_for_ten(30);
    let (mut req, mut out) = (RecordingRequests::default(), RecordingSink::default());
    let split = w.split;
    w.attempt_wield(
        &mut req,
        &mut out,
        PILE,
        0x0080_0000,
        split,
        ServerTime(1.0),
        false,
    )
    .expect("sent");
    assert!(
        matches!(req.0.as_slice(), [Request::GetAndWieldItem(g)] if g.item == PILE),
        "{:?}",
        req.0
    );
}
