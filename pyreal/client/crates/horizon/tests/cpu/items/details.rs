//! Item details on hover: resting the pointer on an item in a pack asks the game to appraise it
//! once, the tooltip shows what came back or says plainly that the appraisal failed, and with
//! Shift held the item is shown beside what is worn in its place, its differences marked.

use pyreal_client_contract::view::DropTarget;
use pyreal_client_contract::UiRequest;
use pyreal_horizon::draw::Rect;
use pyreal_horizon::ui::appraisals::{Appraised, Stat, StatKind};
use pyreal_horizon::ui::game::{ExaminePane, Examined, GameState, Item};
use pyreal_horizon::ui::kit::Drop;
use pyreal_horizon::ui::panels::item_tip::{Comparison, DetailState};
use pyreal_horizon::ui::panels::WindowId;
use pyreal_primitives::ObjectId;

use crate::Harness;

const ME: ObjectId = ObjectId(0x5000_0001);
const NEW_HELM: ObjectId = ObjectId(0x8000_0020);
const OLD_HELM: ObjectId = ObjectId(0x8000_0021);
/// The head's place, as a helm is worn.
const HEAD: u32 = 0x1;

fn helm(id: ObjectId, name: &str, worn: u32) -> Item {
    Item {
        id,
        name: name.into(),
        stack: Some(1),
        worn,
        equip_locations: HEAD,
        ..Item::default()
    }
}

fn carrying() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        player_id: Some(ME),
        pack: vec![helm(NEW_HELM, "Steel Helm", 0)],
        equipped: vec![helm(OLD_HELM, "Leather Cap", HEAD)],
        ..GameState::default()
    }
}

fn appraised(id: ObjectId, title: &str, success: bool, armor: f64) -> Appraised {
    Appraised {
        id,
        delivery: 1,
        success,
        detail: Examined {
            id,
            pane: ExaminePane::Item,
            title: title.into(),
            runs: vec![
                ("Value: 1,200".into(), false, 0),
                ("Burden: 250".into(), true, 0),
                ("Workmanship: Well-crafted (6)".into(), false, 0),
                ("Spells: Impenetrability II".into(), false, 0),
            ],
            ..Examined::default()
        },
        stats: vec![
            Stat::plain(StatKind::ArmorLevel, armor),
            Stat::plain(StatKind::Burden, 250.0),
        ],
    }
}

/// The inventory open, the pointer resting on the first slot for half a second, and every
/// request made while it did.
fn rest_on_the_helm(h: &mut Harness, state: &GameState) -> Vec<UiRequest> {
    h.ui.windows.open(WindowId::Inventory, -1.0);
    h.frame(state);
    let at = h
        .ui
        .drops
        .iter()
        .rev()
        .find(|(_, t)| {
            matches!(t, Some(Drop::Target(DropTarget::ItemListSlot { index: 0, container, .. })) if *container == ME)
        })
        .map(|(r, _)| *r)
        .expect("the first slot is on screen");
    h.move_to(at.x + at.w / 2.0, at.y + at.w / 2.0);
    let mut requests = Vec::new();
    for _ in 0..30 {
        requests.extend(h.frame(state).requests);
    }
    requests
}

fn appraisals_of(requests: &[UiRequest], id: ObjectId) -> usize {
    requests
        .iter()
        .filter(|r| **r == UiRequest::Appraise(id))
        .count()
}

#[test]
fn resting_on_an_item_appraises_it_once_and_shows_its_details() {
    let mut h = Harness::new(Default::default());
    let mut state = carrying();
    let asked = rest_on_the_helm(&mut h, &state);
    assert_eq!(appraisals_of(&asked, NEW_HELM), 1, "{asked:?}");
    let shown = h.ui.windows.item_details().to_vec();
    assert_eq!(shown.len(), 1, "no comparison without Shift");
    assert_eq!(shown[0].state, DetailState::Assessing);
    assert_eq!(shown[0].lines[0].0, "Assessing...");

    state.appraised = vec![appraised(NEW_HELM, "Steel Helm", true, 120.0)];
    let mut again = Vec::new();
    for _ in 0..30 {
        again.extend(h.frame(&state).requests);
    }
    assert_eq!(appraisals_of(&again, NEW_HELM), 0, "not asked twice");
    let shown = &h.ui.windows.item_details()[0];
    assert_eq!(shown.state, DetailState::Ready);
    let lines: Vec<&str> = shown.lines.iter().map(|(l, _)| l.as_str()).collect();
    for want in [
        "Value: 1,200",
        "Burden: 250",
        "Workmanship: Well-crafted (6)",
        "Spells: Impenetrability II",
    ] {
        assert!(lines.contains(&want), "{want} in {lines:?}");
    }
}

#[test]
fn a_failed_appraisal_and_a_missing_reply_say_so() {
    let mut h = Harness::new(Default::default());
    let mut state = carrying();
    rest_on_the_helm(&mut h, &state);
    state.appraised = vec![appraised(NEW_HELM, "Steel Helm", false, 0.0)];
    h.frame(&state);
    let shown = &h.ui.windows.item_details()[0];
    assert_eq!(shown.state, DetailState::Failed);
    assert_eq!(
        shown.lines[0].0,
        "You could not assess this item. Some details are unknown."
    );

    // No reply at all: after a while the tooltip says the server did not answer.
    let mut h = Harness::new(Default::default());
    let state = carrying();
    rest_on_the_helm(&mut h, &state);
    for _ in 0..(60 * 6) {
        h.frame(&state);
    }
    let shown = &h.ui.windows.item_details()[0];
    assert_eq!(shown.state, DetailState::NoReply);
    assert_eq!(
        shown.lines[0].0,
        "This item could not be assessed. The game did not answer."
    );
    assert!(
        shown.lines.len() == 1,
        "nothing stale under it: {:?}",
        shown.lines
    );
}

#[test]
fn shift_shows_the_worn_item_beside_it_with_the_differences_marked() {
    let mut h = Harness::new(Default::default());
    let mut state = carrying();
    h.input.shift = true;
    let asked = rest_on_the_helm(&mut h, &state);
    assert_eq!(appraisals_of(&asked, OLD_HELM), 1, "the worn cap too");
    state.appraised = vec![
        appraised(NEW_HELM, "Steel Helm", true, 120.0),
        appraised(OLD_HELM, "Leather Cap", true, 80.0),
    ];
    h.input.shift = true;
    h.frame(&state);
    let shown = h.ui.windows.item_details().to_vec();
    assert_eq!(shown.len(), 2, "the helm, then the cap");
    assert!(!shown[0].worn && shown[1].worn);
    assert_eq!(shown[1].title, "Leather Cap");
    let Comparison::Differences(c) = &shown[0].comparisons[0] else {
        panic!("{:?}", shown[0].comparisons);
    };
    assert_eq!(c.with, "Leather Cap");
    assert!(c.parts.is_empty() && !c.totals, "one piece on one part");
    let diffs = &c.rows;
    let armor = diffs
        .iter()
        .find(|d| d.kind == StatKind::ArmorLevel)
        .expect("armour compared");
    assert_eq!((armor.by(), armor.better), (Some(40.0), Some(true)));
    let burden = diffs.iter().find(|d| d.kind == StatKind::Burden).unwrap();
    assert_eq!(burden.better, None, "the same burden is not marked");

    // The worn cap's appraisal failed: the comparison says so.
    state.appraised[1].success = false;
    h.input.shift = true;
    h.frame(&state);
    assert_eq!(
        h.ui.windows.item_details()[0].comparisons,
        [Comparison::Missing(
            "No comparison. The worn item could not be assessed."
        )]
    );
    assert_eq!(h.ui.windows.item_details()[1].state, DetailState::Failed);

    // Let go of Shift and the comparison goes.
    h.input.shift = false;
    h.frame(&state);
    assert_eq!(h.ui.windows.item_details().len(), 1);
}

#[test]
fn the_setting_compares_without_shift() {
    let mut h = Harness::new(pyreal_horizon::options::HorizonOptions {
        compare_worn: true,
        ..Default::default()
    });
    let state = carrying();
    rest_on_the_helm(&mut h, &state);
    assert_eq!(h.ui.windows.item_details().len(), 2);
}

#[test]
fn with_assess_on_hover_off_nothing_is_asked_and_known_details_still_show() {
    let mut h = Harness::new(pyreal_horizon::options::HorizonOptions {
        assess_on_hover: false,
        ..Default::default()
    });
    let mut state = carrying();
    h.input.shift = true;
    let asked = rest_on_the_helm(&mut h, &state);
    assert_eq!(appraisals_of(&asked, NEW_HELM), 0, "{asked:?}");
    assert_eq!(appraisals_of(&asked, OLD_HELM), 0, "{asked:?}");
    assert!(h.ui.windows.item_details().is_empty(), "the name alone");

    // The helm was examined: its details show, and the cap says it is not assessed.
    state.appraised = vec![appraised(NEW_HELM, "Steel Helm", true, 120.0)];
    h.input.shift = true;
    let mut asked = Vec::new();
    for _ in 0..(60 * 6) {
        asked.extend(h.frame(&state).requests);
    }
    assert_eq!(appraisals_of(&asked, OLD_HELM), 0, "{asked:?}");
    let shown = h.ui.windows.item_details().to_vec();
    assert_eq!(shown[0].state, DetailState::Ready);
    assert_eq!(shown[1].state, DetailState::NotAssessed);
    assert_eq!(
        shown[1].lines[0].0,
        "Not assessed. Examine it to see its details."
    );
    assert_eq!(
        shown[0].comparisons,
        [Comparison::Missing(
            "No comparison. The worn item is not assessed."
        )]
    );
}

#[test]
fn nothing_is_asked_before_the_pointer_has_rested() {
    let mut h = Harness::new(Default::default());
    let state = carrying();
    h.ui.windows.open(WindowId::Inventory, -1.0);
    h.frame(&state);
    let at: Rect =
        h.ui.drops
            .iter()
            .rev()
            .find(|(_, t)| {
                matches!(
                    t,
                    Some(Drop::Target(DropTarget::ItemListSlot { index: 0, .. }))
                )
            })
            .map(|(r, _)| *r)
            .unwrap();
    h.move_to(at.x + at.w / 2.0, at.y + at.w / 2.0);
    let mut asked = Vec::new();
    for _ in 0..10 {
        asked.extend(h.frame(&state).requests);
    }
    assert_eq!(appraisals_of(&asked, NEW_HELM), 0, "{asked:?}");
    assert!(h.ui.windows.item_details().is_empty());
}

/// An appraisal as the game sends it, read the way the runtime reads it: armour `level` with the
/// `active` spells on it (description and category).
fn appraised_from_view(
    id: ObjectId,
    title: &str,
    level: i32,
    burden: i32,
    active: &[(&str, u32)],
    delivery: u64,
) -> Appraised {
    use pyreal_client_contract::view::{AppraisalSpellView, AppraisalView, MagicInfoView};
    let view = AppraisalView {
        success: true,
        delivery,
        armor_level: Some(level),
        armor_mods: Some([1.0; 8]),
        burden: Some(burden),
        magic: MagicInfoView {
            spells: Some(
                active
                    .iter()
                    .enumerate()
                    .map(|(i, (description, category))| AppraisalSpellView {
                        resolved: true,
                        raw_id: 0x8000_0000 | (1000 + u32::try_from(i).unwrap()),
                        enchantment: true,
                        description: (*description).into(),
                        category: *category,
                        ..AppraisalSpellView::default()
                    })
                    .collect(),
            ),
            ..MagicInfoView::default()
        },
        ..AppraisalView::default()
    };
    let detail = Examined {
        id,
        pane: ExaminePane::Item,
        title: title.into(),
        ..Examined::default()
    };
    Appraised::of(id, &view, detail)
}

fn compared(h: &Harness) -> pyreal_horizon::ui::appraisals::Compared {
    match &h.ui.windows.item_details()[0].comparisons[..] {
        [Comparison::Differences(c)] => c.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_buffed_worn_helm_is_compared_without_its_spells_and_asked_about_again() {
    let mut h = Harness::new(Default::default());
    let mut state = carrying();
    h.input.shift = true;
    rest_on_the_helm(&mut h, &state);
    // The worn cap carries Impenetrability VI and its own Minor Impenetrability.
    state.appraised = vec![
        appraised_from_view(NEW_HELM, "Steel Helm", 300, 250, &[], 1),
        appraised_from_view(
            OLD_HELM,
            "Leather Cap",
            440,
            250,
            &[
                (
                    "Improves a shield or piece of armor's armor value by 200 points.",
                    160,
                ),
                (
                    "Improves a shield or piece of armor's Armor value by 20 points.",
                    391,
                ),
            ],
            2,
        ),
    ];
    h.input.shift = true;
    h.frame(&state);
    let c = compared(&h);
    let armor = c
        .rows
        .iter()
        .find(|d| d.kind == StatKind::ArmorLevel)
        .unwrap();
    assert_eq!(armor.worn, Some(220.0), "without its spells");
    assert_eq!((armor.by(), armor.better), (Some(80.0), Some(true)));
    assert_eq!(armor.worn_with_spells, Some(440.0));
    assert_eq!(
        pyreal_horizon::ui::panels::item_tip::spell_notes(&c),
        ["The worn numbers leave out spells. With its spells the worn Armor Level is 440."]
    );

    // A minute on, the worn cap is asked about again, and compared as before meanwhile.
    let mut asked = Vec::new();
    for _ in 0..(60 * 61) {
        h.input.shift = true;
        asked.extend(h.frame(&state).requests);
    }
    assert_eq!(appraisals_of(&asked, OLD_HELM), 1, "the cap once");
    assert_eq!(
        appraisals_of(&asked, NEW_HELM),
        0,
        "the helm in the pack never"
    );
    assert_eq!(h.ui.windows.item_details()[1].state, DetailState::Ready);
    assert_eq!(compared(&h).rows, c.rows);
    // The spell ran out: the new answer compares the same.
    state.appraised[1] = appraised_from_view(OLD_HELM, "Leather Cap", 220, 250, &[], 3);
    h.input.shift = true;
    h.frame(&state);
    let c = compared(&h);
    let armor = c
        .rows
        .iter()
        .find(|d| d.kind == StatKind::ArmorLevel)
        .unwrap();
    assert_eq!((armor.worn, armor.worn_with_spells), (Some(220.0), None));
    assert!(pyreal_horizon::ui::panels::item_tip::spell_notes(&c).is_empty());
}

#[test]
fn a_coat_is_compared_with_the_three_pieces_it_would_displace() {
    const COAT: ObjectId = ObjectId(0x8000_0030);
    let piece = |id: u32, name: &str, places: u32| Item {
        id: ObjectId(id),
        name: name.into(),
        stack: Some(1),
        worn: places,
        equip_locations: places,
        ..Item::default()
    };
    let mut state = GameState {
        in_world: true,
        name: "Tester".into(),
        player_id: Some(ME),
        pack: vec![Item {
            id: COAT,
            name: "Chainmail Coat".into(),
            stack: Some(1),
            equip_locations: 0x1E00,
            ..Item::default()
        }],
        equipped: vec![
            piece(0x8000_0031, "Breastplate", 0x200),
            piece(0x8000_0032, "Girth", 0x400),
            piece(0x8000_0033, "Sleeves", 0x1800),
            helm(OLD_HELM, "Leather Cap", HEAD),
        ],
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.input.shift = true;
    let asked = rest_on_the_helm(&mut h, &state);
    for id in [0x8000_0031, 0x8000_0032, 0x8000_0033] {
        assert_eq!(appraisals_of(&asked, ObjectId(id)), 1, "{id:x}");
    }
    assert_eq!(appraisals_of(&asked, OLD_HELM), 0, "not the cap");
    state.appraised = vec![
        appraised_from_view(COAT, "Chainmail Coat", 300, 900, &[], 1),
        appraised_from_view(ObjectId(0x8000_0031), "Breastplate", 350, 500, &[], 2),
        appraised_from_view(ObjectId(0x8000_0032), "Girth", 250, 300, &[], 3),
        appraised_from_view(ObjectId(0x8000_0033), "Sleeves", 200, 400, &[], 4),
    ];
    h.input.shift = true;
    h.frame(&state);
    let shown = h.ui.windows.item_details();
    assert_eq!(shown.len(), 4, "the coat and the three pieces");
    assert_eq!(shown[0].comparisons.len(), 1, "one comparison of them all");
    let c = compared(&h);
    assert_eq!(c.with, "Breastplate, Girth and Sleeves");
    let parts: Vec<(&str, f64, f64)> = c
        .parts
        .iter()
        .map(|p| (p.part.name(), p.item, p.worn))
        .collect();
    assert_eq!(
        parts,
        [
            ("Chest", 300.0, 350.0),
            ("Abdomen", 300.0, 250.0),
            ("Upper Arms", 300.0, 200.0),
            ("Lower Arms", 300.0, 200.0),
        ]
    );
    assert!(c.totals);
    let total = |k| *c.rows.iter().find(|d| d.kind == k).unwrap();
    assert_eq!(total(StatKind::ArmorLevel).by(), Some(200.0));
    assert_eq!(total(StatKind::Slashing).by(), Some(200.0));
    assert_eq!(total(StatKind::Burden).by(), Some(-300.0));
}

#[test]
fn an_inscribed_item_is_appraised_again_a_second_later() {
    use pyreal_horizon::ui::game::{Relation, Target};
    const SWORD: ObjectId = ObjectId(0x8000_0040);
    let mut h = Harness::new(Default::default());
    let mut state = GameState {
        in_world: true,
        name: "Tester".into(),
        player_id: Some(ME),
        target: Some(Target {
            id: SWORD,
            name: "Sword".into(),
            relation: Relation::Object,
            health: None,
            level: None,
            icon: None,
            look: None,
        }),
        examine_opened: true,
        examined: Some(Examined {
            id: SWORD,
            pane: ExaminePane::Item,
            title: "Sword".into(),
            inscription_editable: true,
            ..Examined::default()
        }),
        ..GameState::default()
    };
    h.frame(&state);
    state.examine_opened = false;
    let w =
        h.ui.windows
            .rects(1.0)
            .into_iter()
            .find(|(id, _)| *id == WindowId::Examine)
            .map(|(_, r)| r)
            .expect("the examine window is open");
    h.move_to(w.x + 60.0, w.bottom() - 80.0);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    h.input.chars = vec!['M', 'i', 'n', 'e'];
    h.frame(&state);
    h.move_to(w.x + 60.0, w.y + 60.0);
    h.press();
    let sent = h.frame(&state).requests;
    assert!(
        sent.iter()
            .any(|r| matches!(r, UiRequest::SetInscription { object, .. } if *object == SWORD)),
        "{sent:?}"
    );
    h.release();
    let mut soon = Vec::new();
    for _ in 0..50 {
        soon.extend(h.frame(&state).requests);
    }
    assert_eq!(appraisals_of(&soon, SWORD), 0, "not at once: {soon:?}");
    let mut later = Vec::new();
    for _ in 0..30 {
        later.extend(h.frame(&state).requests);
    }
    assert_eq!(appraisals_of(&later, SWORD), 1, "a second later, once");
}
