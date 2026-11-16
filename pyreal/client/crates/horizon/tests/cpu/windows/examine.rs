//! The examine window follows the game: an examine opens it, a new selection while it is open is
//! examined in turn, and clearing the selection closes it.

use pyreal_client_contract::UiRequest;
use pyreal_horizon::ui::game::{GameState, Relation, Target};
use pyreal_horizon::ui::panels::WindowId;
use pyreal_primitives::ObjectId;

use crate::Harness;

fn selecting(id: u32) -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        target: Some(Target {
            id: ObjectId(id),
            name: "Drudge".into(),
            relation: Relation::Hostile,
            health: None,
            level: None,
            icon: None,
            look: None,
        }),
        ..GameState::default()
    }
}

#[test]
fn an_examine_opens_the_window_and_a_new_selection_is_examined_in_turn() {
    let mut h = Harness::new(Default::default());
    let mut state = selecting(1);
    state.examine_opened = true;
    h.frame(&state);
    assert!(h.ui.windows.is_open(WindowId::Examine));
    state.examine_opened = false;
    h.frame(&state);
    let next = selecting(2);
    let out = h.frame(&next);
    assert!(
        out.requests.contains(&UiRequest::Examine(ObjectId(2))),
        "{:?}",
        out.requests
    );
}

#[test]
fn clearing_the_selection_closes_the_window() {
    let mut h = Harness::new(Default::default());
    let mut state = selecting(1);
    state.examine_opened = true;
    h.frame(&state);
    let none = GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    };
    h.frame(&none);
    assert!(!h.ui.windows.is_open(WindowId::Examine));
}

#[test]
fn the_examine_key_again_closes_the_window() {
    let mut h = Harness::new(Default::default());
    let mut state = selecting(1);
    state.examine_opened = true;
    h.frame(&state);
    state.examine_opened = false;
    state.examine_closed = true;
    h.frame(&state);
    assert!(!h.ui.windows.is_open(WindowId::Examine));
}

#[test]
fn an_inscription_takes_several_lines_as_typed_and_is_sent_when_the_box_lets_go() {
    use pyreal_horizon::ui::game::{ExaminePane, Examined};
    use pyreal_horizon::ui::input::vk;
    let mut h = Harness::new(Default::default());
    let mut state = selecting(0x8000_0001);
    state.examine_opened = true;
    state.examined = Some(Examined {
        id: ObjectId(0x8000_0001),
        pane: ExaminePane::Item,
        title: "Sword".into(),
        inscription_editable: true,
        ..Examined::default()
    });
    h.frame(&state);
    state.examine_opened = false;
    let w =
        h.ui.windows
            .rects(1.0)
            .into_iter()
            .find(|(id, _)| *id == WindowId::Examine)
            .map(|(_, r)| r)
            .expect("the examine window is open");
    // A press in the box gives it the keyboard.
    h.move_to(w.x + 60.0, w.bottom() - 80.0);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    // Two lines of a drawing, the second starting with spaces.
    h.input.chars = vec!['/', '\\'];
    h.frame(&state);
    h.input.keys.push(vk::ENTER);
    h.frame(&state);
    h.input.chars = vec![' ', '|'];
    let typing = h.frame(&state).requests;
    assert!(
        !typing
            .iter()
            .any(|r| matches!(r, UiRequest::SetInscription { .. })),
        "Enter starts a line and sends nothing"
    );
    // A press elsewhere lets the box go, and the lines are sent as typed.
    h.move_to(w.x + 60.0, w.y + 60.0);
    h.press();
    let sent = h.frame(&state).requests;
    assert!(
        sent.contains(&UiRequest::SetInscription {
            object: ObjectId(0x8000_0001),
            text: "/\\\n |".into()
        }),
        "{sent:?}"
    );
    // The box keeps what was committed, before the server answers: in and out again unchanged
    // sends nothing.
    h.release();
    h.frame(&state);
    h.move_to(w.x + 60.0, w.bottom() - 80.0);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    h.move_to(w.x + 60.0, w.y + 60.0);
    h.press();
    let again = h.frame(&state).requests;
    assert!(
        !again
            .iter()
            .any(|r| matches!(r, UiRequest::SetInscription { .. })),
        "{again:?}"
    );
}

#[test]
fn a_creature_is_examined_under_its_own_name_with_its_kind_beside_its_level() {
    use pyreal_client_contract::snapshot::{GameSnapshot, ObjectSnapshot};
    use pyreal_client_contract::view::AppraisalView;
    use pyreal_horizon::ui::game::ExaminePane;
    let golem = ObjectId(0x8000_09D9);
    let mut view = GameSnapshot::default();
    view.objects.insert(
        golem,
        ObjectSnapshot {
            name: Some("Sparring Golem".into()),
            appraisal: Some(AppraisalView {
                creature: true,
                level: Some(1),
                creature_display_name: Some("Golem".into()),
                ..AppraisalView::default()
            }),
            ..ObjectSnapshot::default()
        },
    );
    let e = pyreal_horizon::state::examined(&view, golem).expect("an appraisal");
    assert_eq!(e.pane, ExaminePane::Creature);
    assert_eq!(e.title, "Sparring Golem");
    assert_eq!(e.kind.as_deref(), Some("Golem"));
    assert_eq!(e.level, "1");
}

/// The game clearing the selection while the player writes on the item (sold, given away) closes
/// the window, and the keyboard goes back to the game: Escape closes windows again.
#[test]
fn the_window_closing_under_an_inscription_lets_the_keyboard_go() {
    use pyreal_horizon::ui::game::{ExaminePane, Examined};
    let mut h = Harness::new(Default::default());
    let mut state = selecting(0x8000_0001);
    state.examine_opened = true;
    state.examined = Some(Examined {
        id: ObjectId(0x8000_0001),
        pane: ExaminePane::Item,
        title: "Sword".into(),
        inscription_editable: true,
        ..Examined::default()
    });
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
    assert!(
        h.ui.windows.has_text_focus(),
        "premise: the box has the keyboard"
    );
    state.target = None;
    h.frame(&state);
    h.frame(&state);
    assert!(!h.ui.windows.is_open(WindowId::Examine));
    assert!(
        !h.ui.windows.has_text_focus(),
        "the closed window still holds the keyboard"
    );
}

/// What the box committed stands in for the game's inscription only until the game's changes:
/// after that the game's is the one the box compares with.
#[test]
fn a_committed_inscription_gives_way_to_what_the_game_says_next() {
    use pyreal_horizon::ui::game::{ExaminePane, Examined};
    let mut h = Harness::new(Default::default());
    let mut state = selecting(0x8000_0001);
    state.examine_opened = true;
    state.examined = Some(Examined {
        id: ObjectId(0x8000_0001),
        pane: ExaminePane::Item,
        title: "Sword".into(),
        inscription_editable: true,
        ..Examined::default()
    });
    h.frame(&state);
    state.examine_opened = false;
    let w =
        h.ui.windows
            .rects(1.0)
            .into_iter()
            .find(|(id, _)| *id == WindowId::Examine)
            .map(|(_, r)| r)
            .expect("the examine window is open");
    let in_and_out = |h: &mut Harness, state: &GameState, chars: Vec<char>| {
        h.move_to(w.x + 60.0, w.bottom() - 80.0);
        h.press();
        h.frame(state);
        h.release();
        h.frame(state);
        h.input.chars = chars;
        h.frame(state);
        h.move_to(w.x + 60.0, w.y + 60.0);
        h.press();
        let sent = h.frame(state).requests;
        h.release();
        h.frame(state);
        sent
    };
    let first = in_and_out(&mut h, &state, vec!['a', 'b']);
    assert!(first
        .iter()
        .any(|r| matches!(r, UiRequest::SetInscription { .. })));
    // The game says otherwise (someone else's words): the same text again is a change.
    if let Some(e) = state.examined.as_mut() {
        e.inscription_value = "Mine".into();
        e.scribe = "Other".into();
    }
    h.frame(&state);
    let again = in_and_out(&mut h, &state, Vec::new());
    assert!(
        again.contains(&UiRequest::SetInscription {
            object: ObjectId(0x8000_0001),
            text: "ab".into()
        }),
        "{again:?}"
    );
}

/// The examined player as the window shows them, with the Tell button's name.
fn examining_a_player(name: &str) -> GameState {
    use pyreal_horizon::ui::game::{ExaminePane, Examined};
    let mut state = selecting(0x5000_0002);
    state.examine_opened = true;
    state.examined = Some(Examined {
        id: ObjectId(0x5000_0002),
        pane: ExaminePane::Character,
        title: name.into(),
        tell: Some(name.into()),
        ..Examined::default()
    });
    state
}

#[test]
fn the_tell_button_starts_a_tell_to_the_examined_player_without_sending_it() {
    use pyreal_client_contract::chat::entry::EntryAction;
    let mut h = Harness::new(Default::default());
    let state = examining_a_player("+Sir Alba");
    h.frame(&state);
    let w =
        h.ui.windows
            .rects(1.0)
            .into_iter()
            .find(|(id, _)| *id == WindowId::Examine)
            .map(|(_, r)| r)
            .expect("the examine window is open");
    // The button sits at the top right of the window's body; sweep down the right edge for it.
    let mut found = None;
    for dy in (20..90).step_by(4) {
        h.move_to(w.right() - 30.0, w.y + dy as f32);
        h.press();
        let sent = h.frame(&state).requests;
        h.release();
        h.frame(&state);
        if !sent.is_empty() {
            found = Some(sent);
            break;
        }
    }
    let sent = found.expect("the Tell button was clicked");
    assert!(
        sent.contains(&UiRequest::ChatEntry {
            window: pyreal_client_contract::chat::interface::window::MAIN,
            text: String::new(),
            action: EntryAction::StartTell {
                name: "+Sir Alba".into()
            },
        }),
        "{sent:?}"
    );
    assert!(
        !sent.iter().any(|r| matches!(r, UiRequest::ChatLine { .. })),
        "{sent:?}"
    );
}

#[test]
fn only_another_player_is_examined_with_a_tell() {
    use pyreal_client_contract::snapshot::{GameSnapshot, ObjectSnapshot};
    use pyreal_client_contract::view::{AppraisalView, SelectionQueryFacts};
    let (me, other, golem, sword) = (
        ObjectId(0x5000_0001),
        ObjectId(0x5000_0002),
        ObjectId(0x8000_09D9),
        ObjectId(0x8000_0001),
    );
    let person = |name: &str, is_player: bool| ObjectSnapshot {
        name: Some(name.into()),
        appraisal: Some(AppraisalView {
            creature: true,
            template: is_player,
            ..AppraisalView::default()
        }),
        selection_query_facts: Some(SelectionQueryFacts {
            is_player,
            ..SelectionQueryFacts::default()
        }),
        ..ObjectSnapshot::default()
    };
    let mut view = GameSnapshot {
        player: Some(me),
        ..GameSnapshot::default()
    };
    view.objects.insert(me, person("Tester", true));
    view.objects.insert(other, person("+Sir Alba", true));
    view.objects.insert(golem, person("Sparring Golem", false));
    view.objects.insert(
        sword,
        ObjectSnapshot {
            name: Some("Sword".into()),
            appraisal: Some(AppraisalView::default()),
            ..ObjectSnapshot::default()
        },
    );
    let tell = |id| {
        pyreal_horizon::state::examined(&view, id)
            .expect("an appraisal")
            .tell
    };
    assert_eq!(tell(other).as_deref(), Some("+Sir Alba"));
    assert_eq!(tell(golem), None, "a creature");
    assert_eq!(tell(sword), None, "an item");
    assert_eq!(tell(me), None, "oneself");
}
