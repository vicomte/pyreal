//! The party list's AI fellow mode icons and its rows' tooltips.

use pyreal_client_contract::view::{AiFellowView, FellowEntry, FellowshipView};
use pyreal_client_contract::UiRequest;
use pyreal_horizon::ui::game::{GameState, Vital};
use pyreal_primitives::ObjectId;

use crate::Harness;

const ME: ObjectId = ObjectId(0x5000_0001);
const AERIN: ObjectId = ObjectId(0x5000_0002);

fn party(leader: ObjectId) -> GameState {
    let v = Vital {
        current: 100,
        max: 100,
    };
    GameState {
        in_world: true,
        name: "Tester".into(),
        player_id: Some(ME),
        fellowship: vec![
            ("Tester".into(), v, v, v, leader == ME),
            ("~Aerin".into(), v, v, v, leader == AERIN),
        ],
        fellowship_view: Some(FellowshipView {
            leader,
            members: [(ME, "Tester"), (AERIN, "~Aerin")]
                .into_iter()
                .map(|(id, name)| FellowEntry {
                    id,
                    name: name.into(),
                    level: if id == AERIN { 126 } else { 0 },
                    ..FellowEntry::default()
                })
                .collect(),
            ..FellowshipView::default()
        }),
        ai_fellows: vec![AiFellowView {
            id: AERIN,
            name: "~Aerin".into(),
            mode: "heal".into(),
            in_fellowship: true,
        }],
        ..GameState::default()
    }
}

/// Every click along the AI fellow's name line, each with what it asked for.
fn clicks_along_the_name(state: &GameState) -> Vec<Vec<UiRequest>> {
    let mut h = Harness::with_built_in_art();
    h.frame(state);
    // The second row's name line, as the list lays it out at this size.
    let y = 86.0 + 54.0 + 11.0;
    (40..420)
        .step_by(3)
        .map(|x| {
            #[allow(clippy::cast_precision_loss)]
            h.move_to(x as f32, y);
            h.frame(state);
            h.press();
            let mut asked = h.frame(state).requests;
            h.release();
            asked.extend(h.frame(state).requests);
            asked
        })
        .collect()
}

fn modes_set(clicks: &[Vec<UiRequest>]) -> Vec<String> {
    let mut set: Vec<String> = clicks
        .iter()
        .flatten()
        .filter_map(|r| match r {
            UiRequest::ChatLineOnBehalf { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    set.dedup();
    set
}

/// The leader sets an AI fellow's mode from its icons beside its name; its own mode's icon asks
/// nothing, and a click on an icon does not select the row.
#[test]
fn the_leader_sets_an_ai_fellow_s_mode_from_the_party_list() {
    let clicks = clicks_along_the_name(&party(ME));
    assert_eq!(
        modes_set(&clicks),
        [
            "@aifellow 0x50000002 aggressive",
            "@aifellow 0x50000002 defensive",
            "@aifellow 0x50000002 assist",
            "@aifellow 0x50000002 passive",
        ]
    );
    for asked in &clicks {
        let set = asked
            .iter()
            .any(|r| matches!(r, UiRequest::ChatLineOnBehalf { .. }));
        let selects = asked.iter().any(|r| matches!(r, UiRequest::Select(_)));
        assert!(
            !(set && selects),
            "an icon click selected the row: {asked:?}"
        );
    }
}

/// A member who does not lead sees the mode and sets nothing.
#[test]
fn a_member_who_does_not_lead_only_sees_the_mode() {
    assert!(modes_set(&clicks_along_the_name(&party(AERIN))).is_empty());
}

/// The tooltip's title and lines over the party list's `row` (0 the player), right of its gauges.
fn row_tip(state: &GameState, row: u16) -> Option<(String, Vec<String>)> {
    let mut h = Harness::with_built_in_art();
    h.move_to(330.0, 86.0 + 54.0 * f32::from(row) + 40.0);
    h.frame(state);
    h.frame(state);
    h.ui.hud
        .shown_tip()
        .map(|(t, l)| (t.to_string(), l.to_vec()))
}

/// A member's tooltip names their level; one whose level is not known has no level line.
#[test]
fn a_party_member_s_tooltip_names_their_level() {
    let mut state = party(ME);
    state.level = 0;
    let (title, lines) = row_tip(&state, 1).expect("a tooltip over Aerin's row");
    assert_eq!(title, "~Aerin");
    assert_eq!(lines.first().map(String::as_str), Some("Level 126"));
    let (_, lines) = row_tip(&state, 0).expect("a tooltip over the player's row");
    assert!(lines.iter().all(|l| !l.starts_with("Level")), "{lines:?}");
    state.level = 42;
    let (_, lines) = row_tip(&state, 0).expect("a tooltip over the player's row");
    assert_eq!(lines.first().map(String::as_str), Some("Level 42"));
}
