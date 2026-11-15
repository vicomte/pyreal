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
