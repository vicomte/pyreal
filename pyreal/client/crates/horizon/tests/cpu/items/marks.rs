//! The inventory's search and the Keep and Junk marks: what the search matches, the marks kept
//! in the character's file, the question before a Keep item goes, and the vendor's Add Junk.

use pyreal_client_contract::view::{DropTarget, SellSafety, ShopView, SlotDecoration};
use pyreal_client_contract::UiRequest;
use pyreal_horizon::draw::Rect;
use pyreal_horizon::ui::game::{GameState, Item};
use pyreal_horizon::ui::input::vk;
use pyreal_horizon::ui::kit::Drop;
use pyreal_horizon::ui::panels::marks::{self, Mark, Marks};
use pyreal_horizon::ui::panels::pack_search::matches;
use pyreal_horizon::ui::panels::WindowId;
use pyreal_primitives::ObjectId;
use pyreal_rules::weenie::item_type;

use crate::Harness;

const ME: ObjectId = ObjectId(0x5000_0001);
const SWORD: ObjectId = ObjectId(0x8000_0201);
const HELM: ObjectId = ObjectId(0x8000_0202);
const BAG: ObjectId = ObjectId(0x8000_0203);
const SACK: ObjectId = ObjectId(0x8000_0204);
const GEM: ObjectId = ObjectId(0x8000_0205);

/// An item of class `id`'s low half, worth 100 and appraised: one Add Junk lists.
fn item(id: ObjectId, name: &str, obj_type: u32) -> Item {
    Item {
        id,
        name: name.into(),
        stack: Some(1),
        wcid: id.0 & 0xFFFF,
        decoration: Some(SlotDecoration {
            obj_type,
            ..SlotDecoration::default()
        }),
        safety: SellSafety {
            appraised: true,
            value: Some(100),
            ..SellSafety::default()
        },
        ..Item::default()
    }
}

/// The item `id` the character carries.
fn held(state: &GameState, id: ObjectId) -> Item {
    marks::carried(state)
        .find(|i| i.id == id)
        .cloned()
        .expect("carried")
}

fn carrying() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        address: "127.0.0.1:9000".into(),
        player_id: Some(ME),
        pack: vec![
            item(SWORD, "Ivory Long Sword", item_type::MELEE_WEAPON),
            item(HELM, "Bronze Helm", item_type::ARMOR),
            item(BAG, "Salvaged Iron", item_type::TINKERING_MATERIAL),
        ],
        side_packs: vec![(
            item(SACK, "Sack", item_type::CONTAINER),
            vec![item(GEM, "Black Opal", item_type::GEM)],
        )],
        ..GameState::default()
    }
}

fn centre(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

fn zone(h: &Harness, find: impl Fn(&Drop) -> bool) -> Rect {
    h.ui.drops
        .iter()
        .rev()
        .find(|(_, t)| t.as_ref().is_some_and(&find))
        .map(|(r, _)| *r)
        .expect("the zone is on screen")
}

fn slot(index: u32) -> impl Fn(&Drop) -> bool {
    move |t| {
        matches!(t, Drop::Target(DropTarget::ItemListSlot { index: i, container, .. })
            if *i == index && *container == ME)
    }
}

fn open(state: &GameState, windows: &[WindowId]) -> Harness {
    let mut h = Harness::new(Default::default());
    for w in windows {
        h.ui.windows.open(*w, -1.0);
    }
    h.frame(state);
    h.frame(state);
    h
}

fn drag(h: &mut Harness, state: &GameState, from: (f32, f32), to: (f32, f32)) -> Vec<UiRequest> {
    h.move_to(from.0, from.1);
    h.press();
    h.frame(state);
    h.move_to(to.0, to.1);
    h.frame(state);
    h.release();
    h.frame(state).requests
}

/// A folder of the test's own, empty, under the system's temporary folder.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pyreal-marks-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn the_pack_search_matches_names_kinds_and_marks() {
    let sword = item(SWORD, "Ivory Long Sword", item_type::MELEE_WEAPON);
    let helm = item(HELM, "Bronze Helm", item_type::ARMOR);
    let iron = item(BAG, "Salvaged Iron", item_type::TINKERING_MATERIAL);
    assert!(matches(&sword, None, ""), "an empty search matches all");
    assert!(matches(&sword, None, "long"));
    assert!(matches(&sword, None, "LONG sw"), "every word, any case");
    assert!(!matches(&sword, None, "long helm"));
    assert!(
        !matches(&sword, None, "ong"),
        "words match from their start"
    );
    assert!(matches(&sword, None, "weapons"));
    assert!(matches(&helm, None, "armour"));
    assert!(matches(&helm, None, "arm"), "the start of a kind");
    assert!(!matches(&helm, None, "ar"), "too short to name a kind");
    assert!(!matches(&helm, None, "weapon"));
    assert!(matches(&iron, None, "salvage"));
    assert!(matches(&iron, Some(Mark::Junk), "junk"));
    assert!(!matches(&iron, Some(Mark::Keep), "junk"));
    assert!(matches(&sword, Some(Mark::Keep), "keep weapon"));
}

#[test]
fn the_search_dims_what_does_not_match_and_marks_the_packs_with_a_match() {
    let state = carrying();
    let mut h = open(&state, &[WindowId::Inventory]);
    let sword = zone(&h, slot(0));
    let helm = zone(&h, slot(1));
    let sack = zone(&h, |t| *t == Drop::Target(DropTarget::Container(SACK)));
    let dims = |h: &Harness, r: Rect| {
        h.list
            .quads
            .iter()
            .any(|q| q.tex.is_none() && q.colour == 0xC010_0C08 && q.dst.x == r.x && q.dst.y == r.y)
    };
    assert!(
        !dims(&h, sword) && !dims(&h, helm),
        "nothing dimmed unsearched"
    );
    h.ui.windows.pack_search = "helm".into();
    h.frame(&state);
    assert!(dims(&h, sword), "the sword does not match");
    assert!(!dims(&h, helm), "the helm does");
    assert!(dims(&h, sack), "the sack holds no helm");
    // A gem is in the sack: the sack is marked, not dimmed.
    h.ui.windows.pack_search = "gem".into();
    h.frame(&state);
    assert!(!dims(&h, sack), "the sack holds a gem");
    assert!(h.list.quads.iter().any(|q| q.tex.is_none()
        && q.colour == 0xFFF0_C860
        && sack.contains(q.dst.x + 1.0, q.dst.y + 1.0)));
}

#[test]
fn marks_are_kept_in_the_characters_file_and_read_back() {
    let dir = scratch("file");
    let state = carrying();
    let mut m = Marks::default();
    m.load_for(Some(&dir), "127.0.0.1:9000", "Tester");
    m.set(&held(&state, SWORD), Some(Mark::Keep));
    m.set(&held(&state, BAG), Some(Mark::Junk));
    m.set(&held(&state, HELM), Some(Mark::Junk));
    m.set(&held(&state, HELM), None);
    let file = dir.join(marks::file_name("127.0.0.1:9000", "Tester"));
    assert_eq!(
        std::fs::read_to_string(&file).expect("the file is written"),
        "keep 80000201 513 Ivory Long Sword\njunk 80000203 515 Salvaged Iron\n"
    );
    // Through the interface: the character's marks come back when it enters the world.
    let mut h = Harness::new(Default::default());
    h.ui.hud.settings_dir = Some(dir.clone());
    h.frame(&carrying());
    let m = &h.ui.windows.marks;
    assert_eq!(m.get(&held(&state, SWORD)), Some(Mark::Keep));
    assert_eq!(m.get(&held(&state, BAG)), Some(Mark::Junk));
    assert_eq!(m.get(&held(&state, HELM)), None);
    // Another character of the same world has none of them.
    let mut other = Marks::default();
    other.load_for(Some(&dir), "127.0.0.1:9000", "Someone Else");
    assert_eq!(other.get(&held(&state, SWORD)), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_keep_item_dropped_on_the_world_asks_first_and_goes_on_yes() {
    let state = carrying();
    let mut h = open(&state, &[WindowId::Inventory]);
    h.ui.windows
        .marks
        .set(&held(&state, SWORD), Some(Mark::Keep));
    let from = centre(zone(&h, slot(0)));
    let requests = drag(&mut h, &state, from, (300.0, 500.0));
    let drop = UiRequest::DragDrop {
        item: SWORD,
        target: DropTarget::World,
    };
    assert!(!requests.contains(&drop), "held back: {requests:?}");
    assert_eq!(h.ui.windows.keep_asked.len(), 1);
    assert!(h.ui.windows.keep_asked[0]
        .text
        .starts_with("Ivory Long Sword is marked Keep."));
    // Enter says yes.
    h.input.keys.push(vk::ENTER);
    let out = h.frame(&state);
    assert!(out.requests.contains(&drop), "{:?}", out.requests);
    assert!(h.ui.windows.keep_asked.is_empty());
}

#[test]
fn a_keep_item_put_on_the_sell_list_stays_off_it_on_no() {
    let mut state = carrying();
    state.shop = Some(ShopView {
        open: true,
        sell_mode: true,
        ..ShopView::default()
    });
    let mut h = open(&state, &[WindowId::Inventory]);
    h.ui.windows
        .marks
        .set(&held(&state, SWORD), Some(Mark::Keep));
    let from = centre(zone(&h, slot(0)));
    let to = centre(zone(&h, |t| *t == Drop::Sell));
    let requests = drag(&mut h, &state, from, to);
    let sell = UiRequest::VendorAddToSell { item: SWORD };
    assert!(!requests.contains(&sell), "held back: {requests:?}");
    assert_eq!(h.ui.windows.keep_asked.len(), 1);
    // Escape says no, and nothing is sold.
    h.input.keys.push(vk::ESCAPE);
    let out = h.frame(&state);
    assert!(!out.requests.contains(&sell), "{:?}", out.requests);
    assert!(h.ui.windows.keep_asked.is_empty());
    // An unmarked item goes straight on.
    let from = centre(zone(&h, slot(1)));
    let requests = drag(&mut h, &state, from, to);
    assert!(requests.contains(&UiRequest::VendorAddToSell { item: HELM }));
}

#[test]
fn a_pack_holding_a_keep_item_asks_too() {
    let state = carrying();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.ui.windows.marks.set(&held(&state, GEM), Some(Mark::Keep));
    let mut requests = vec![UiRequest::PutInWorld(SACK), UiRequest::Select(SACK)];
    h.ui.windows.hold_keep(&state, &mut requests);
    assert_eq!(requests, vec![UiRequest::Select(SACK)]);
    assert!(h.ui.windows.keep_asked[0]
        .text
        .starts_with("This pack holds Black Opal, which is marked Keep."));
}

#[test]
fn ctrl_and_right_click_opens_the_item_menu_to_mark_an_item() {
    let state = carrying();
    let mut h = open(&state, &[WindowId::Inventory]);
    let (x, y) = centre(zone(&h, slot(1)));
    h.move_to(x, y);
    h.input.ctrl = true;
    h.input.down[1] = true;
    h.input.pressed[1] = true;
    let out = h.frame(&state);
    h.input.down[1] = false;
    h.input.released[1] = true;
    h.input.ctrl = false;
    assert!(
        !out.requests.contains(&UiRequest::Examine(HELM)),
        "it opens the menu instead of examining"
    );
    h.frame(&state);
    let menu =
        h.ui.windows
            .item_menu
            .as_ref()
            .expect("the item menu is open")
            .at;
    assert_eq!((menu.x, menu.y), (zone(&h, slot(1)).x, zone(&h, slot(1)).y));
    // The menu's rows are nav stops 26 high; an unmarked item's last two are Mark as Keep
    // and Mark as Junk.
    let mut rows: Vec<Rect> = h
        .input
        .nav
        .last
        .targets
        .iter()
        .map(|t| t.rect)
        .filter(|r| (r.h - 26.0).abs() < 0.5 && r.y > menu.y)
        .collect();
    rows.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap());
    let junk = rows[rows.len() - 1];
    h.move_to(junk.x + 10.0, junk.y + junk.h / 2.0);
    h.frame(&state);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    assert_eq!(
        h.ui.windows.marks.get(&held(&state, HELM)),
        Some(Mark::Junk)
    );
    assert!(h.ui.windows.item_menu.is_none());
}

#[test]
fn add_junk_puts_every_junk_item_not_yet_listed_on_the_sell_list() {
    let mut state = carrying();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.ui.windows.marks.set(&held(&state, BAG), Some(Mark::Junk));
    h.ui.windows.marks.set(&held(&state, GEM), Some(Mark::Junk));
    h.ui.windows
        .marks
        .set(&held(&state, SWORD), Some(Mark::Keep));
    assert_eq!(
        h.ui.windows.add_junk(&state),
        vec![
            UiRequest::VendorAddToSell { item: BAG },
            UiRequest::VendorAddToSell { item: GEM },
        ]
    );
    state.shop = Some(ShopView {
        open: true,
        sell_mode: true,
        sell_list: vec![pyreal_client_contract::view::ShopRow {
            item: BAG,
            ..Default::default()
        }],
        ..ShopView::default()
    });
    assert_eq!(
        h.ui.windows.add_junk(&state),
        vec![UiRequest::VendorAddToSell { item: GEM }]
    );
}

#[test]
fn a_mark_does_not_carry_to_a_new_item_given_a_recycled_id() {
    let dir = scratch("recycled");
    let mut state = carrying();
    let mut m = Marks::default();
    m.load_for(Some(&dir), "127.0.0.1:9000", "Tester");
    m.set(&held(&state, GEM), Some(Mark::Junk));
    // The opal is sold, and hours later the server gives its id to a new drop.
    let found = item(GEM, "Gold Ring", item_type::JEWELRY);
    state.side_packs[0].1 = vec![Item {
        wcid: 0x0777,
        ..found
    }];
    assert_eq!(m.get(&held(&state, GEM)), None, "a different item");
    let mut h = Harness::new(Default::default());
    h.ui.windows.marks = m;
    h.frame(&state);
    assert!(h.ui.windows.add_junk(&state).is_empty());
    // Seen in the pack, the stale mark is dropped from the file.
    assert!(h.ui.windows.marks.all().is_empty());
    let file = dir.join(marks::file_name("127.0.0.1:9000", "Tester"));
    assert_eq!(std::fs::read_to_string(&file).expect("the file"), "");
    // The same class under another name is not the same item either.
    let mut m = Marks::default();
    let opal = item(GEM, "Black Opal", item_type::GEM);
    m.set(&opal, Some(Mark::Junk));
    let renamed = Item {
        name: "Fire Opal".into(),
        ..opal.clone()
    };
    assert_eq!(m.get(&renamed), None);
    assert_eq!(m.get(&opal), Some(Mark::Junk));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_old_mark_file_still_loads_and_its_junk_marks_are_asked_about() {
    let dir = scratch("old");
    std::fs::create_dir_all(&dir).expect("the folder");
    let file = dir.join(marks::file_name("127.0.0.1:9000", "Tester"));
    std::fs::write(&file, "keep 80000201\njunk 80000205\njunk 80000203\n").expect("written");
    let state = carrying();
    let mut m = Marks::default();
    m.load_for(Some(&dir), "127.0.0.1:9000", "Tester");
    assert_eq!(m.all().len(), 3, "every old line reads");
    // An old Keep mark holds on the id alone, so the item still asks before it goes.
    assert_eq!(m.get(&held(&state, SWORD)), Some(Mark::Keep));
    // An old Junk mark does nothing until the player says it still holds.
    assert_eq!(m.get(&held(&state, GEM)), None);
    let mut h = Harness::new(Default::default());
    h.ui.hud.settings_dir = Some(dir.clone());
    h.frame(&state);
    let asked: Vec<&str> =
        h.ui.windows
            .marks
            .junk_asked
            .iter()
            .map(|q| q.name.as_str())
            .collect();
    assert_eq!(asked, ["Salvaged Iron", "Black Opal"], "asked once each");
    assert!(h.ui.windows.add_junk(&state).is_empty());
    // Enter says yes to the iron, Escape no to the opal.
    h.input.keys.push(vk::ENTER);
    h.frame(&state);
    h.input.keys.push(vk::ESCAPE);
    h.frame(&state);
    h.frame(&state);
    assert!(h.ui.windows.marks.junk_asked.is_empty(), "not asked again");
    let m = &h.ui.windows.marks;
    assert_eq!(m.get(&held(&state, BAG)), Some(Mark::Junk));
    assert_eq!(m.get(&held(&state, GEM)), None);
    assert_eq!(
        std::fs::read_to_string(&file).expect("the file"),
        "keep 80000201 513 Ivory Long Sword\njunk 80000203 515 Salvaged Iron\n",
        "the Keep mark took its item's class and name when seen"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn marks_of_items_no_longer_carried_are_dropped_once_the_items_have_come() {
    let state = carrying();
    let mut m = Marks::default();
    m.set(&held(&state, SWORD), Some(Mark::Keep));
    m.set(
        &item(ObjectId(0x8000_0299), "Sold Axe", 0),
        Some(Mark::Junk),
    );
    m.follow(marks::carried(&state), false);
    assert_eq!(m.all().len(), 2, "kept while the items may still be coming");
    m.follow(marks::carried(&state), true);
    assert_eq!(m.all().keys().copied().collect::<Vec<_>>(), [SWORD]);
}

/// One item with one unsafe fact, and the reason Add Junk gives for keeping it back.
fn unsafe_items() -> Vec<(Item, &'static str)> {
    let base = item(ObjectId(0x8000_0300), "Thing", item_type::MISC);
    let with = |n: u32, f: &dyn Fn(&mut Item)| {
        let mut i = Item {
            id: ObjectId(0x8000_0300 + n),
            name: format!("Thing {n}"),
            ..base.clone()
        };
        f(&mut i);
        i
    };
    vec![
        (with(1, &|i| i.safety.attuned = true), "attuned"),
        (with(2, &|i| i.safety.retained = true), "retained"),
        (with(3, &|i| i.safety.inscribed = true), "inscribed"),
        (with(4, &|i| i.safety.imbued = true), "imbued"),
        (with(5, &|i| i.safety.tinkered = true), "tinkered"),
        (with(6, &|i| i.safety.rare = true), "rare"),
        (with(7, &|i| i.safety.worn = true), "worn or wielded"),
        (with(8, &|i| i.worn = 0x200), "worn or wielded"),
        (with(9, &|i| i.safety.value = Some(0)), "no value"),
        (with(10, &|i| i.safety.value = None), "no value"),
        (with(11, &|i| i.safety.unsellable = true), "cannot be sold"),
        (
            with(12, &|i| i.safety.vendor_refusal = Some(1)),
            "this vendor does not buy it",
        ),
        (
            with(13, &|i| i.safety.vendor_refusal = Some(3)),
            "too cheap for this vendor",
        ),
        (
            with(14, &|i| i.safety.vendor_refusal = Some(4)),
            "too valuable for this vendor",
        ),
        (
            with(15, &|i| i.safety.appraised = false),
            "not appraised yet",
        ),
    ]
}

#[test]
fn add_junk_keeps_back_each_unsafe_item_and_says_why() {
    for (i, why) in unsafe_items() {
        assert_eq!(marks::kept_back(&i), Some(why), "{}", i.name);
    }
    let clean = item(ObjectId(0x8000_0300), "Thing", item_type::MISC);
    assert_eq!(marks::kept_back(&clean), None);
    let accepted = Item {
        safety: SellSafety {
            vendor_refusal: Some(0),
            ..clean.safety
        },
        ..clean.clone()
    };
    assert_eq!(marks::kept_back(&accepted), None, "the vendor takes it");
    // Marked Junk in the pack, only the clean one is listed, and the rest are named.
    let mut state = carrying();
    state.pack = std::iter::once(clean.clone())
        .chain(unsafe_items().into_iter().map(|(i, _)| i))
        .collect();
    state.side_packs.clear();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    for i in state.pack.clone() {
        h.ui.windows.marks.set(&i, Some(Mark::Junk));
    }
    let plan = h.ui.windows.junk_plan(&state);
    assert_eq!(
        plan.listed,
        vec![UiRequest::VendorAddToSell { item: clean.id }]
    );
    assert_eq!(plan.kept.len(), 15);
    let tip = marks::junk_tip(&plan.kept);
    assert_eq!(tip[0], "Put every item marked Junk on the sell list.");
    assert_eq!(tip[1], "Thing 1. Kept back: attuned.");
    assert_eq!(tip.last().map(String::as_str), Some("7 more kept back."));
}

#[test]
fn the_sell_tab_asks_for_appraisals_of_junk_items_not_yet_appraised() {
    let mut state = carrying();
    for i in &mut state.pack {
        i.safety.appraised = false;
    }
    state.shop = Some(ShopView {
        open: true,
        sell_mode: true,
        ..ShopView::default()
    });
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.ui.windows.marks.set(&held(&state, BAG), Some(Mark::Junk));
    h.ui.windows
        .marks
        .set(&held(&state, HELM), Some(Mark::Junk));
    h.ui.windows.marks.set(&held(&state, GEM), Some(Mark::Junk));
    let mut asked = Vec::new();
    // A second of frames, with a quarter of a second between asks.
    for _ in 0..60 {
        asked.extend(
            h.frame(&state)
                .requests
                .into_iter()
                .filter_map(|r| match r {
                    UiRequest::Appraise(id) => Some(id),
                    _ => None,
                }),
        );
    }
    asked.sort();
    assert_eq!(
        asked,
        [HELM, BAG],
        "each once; the gem is already appraised"
    );
}
