//! The log window's input line is the game's chat entry: it opens on the game's draft, tells the
//! game every edit, sends to the destination the game holds, steps through the destinations the
//! game lets the player use, and takes the game's replies and recalled lines.

use pyreal_client_contract::chat::entry::{EntryAction, EntryUpdate};
use pyreal_client_contract::chat::interface::window::MAIN;
use pyreal_client_contract::chat::mainchat::ChatFocusView;
use pyreal_client_contract::UiRequest;
use pyreal_horizon::ui::game::GameState;
use pyreal_horizon::ui::input::vk;

use crate::Harness;

fn in_world() -> GameState {
    let mut focus = ChatFocusView {
        focus: 1,
        ..ChatFocusView::default()
    };
    for f in [1, 3, 8] {
        focus.enabled[f] = true;
        focus.selectable[f] = true;
    }
    GameState {
        in_world: true,
        name: "Tester".into(),
        chat_focus: focus,
        chat_draft: "half a line".into(),
        ..GameState::default()
    }
}

/// A harness whose chat input is the original game's.
fn classic() -> Harness {
    Harness::new(pyreal_horizon::options::HorizonOptions {
        chat_style: pyreal_horizon::ui::chat::ChatStyle::Classic,
        ..Default::default()
    })
}

fn type_text(h: &mut Harness, text: &str) {
    h.input.chars = text.chars().collect();
}

/// The text of each line sent in `out`.
fn sent(out: &pyreal_horizon::ui::Outcome) -> Vec<String> {
    out.requests
        .iter()
        .filter_map(|r| match r {
            UiRequest::ChatLine { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// The last draft `out` told the game.
fn draft(out: &pyreal_horizon::ui::Outcome) -> Option<String> {
    out.requests.iter().rev().find_map(|r| match r {
        UiRequest::ChatEntry {
            text,
            action: EntryAction::Draft,
            ..
        } => Some(text.clone()),
        _ => None,
    })
}

/// In the world with an empty draft, the line opened.
fn opened(state: &mut GameState) -> Harness {
    state.chat_draft.clear();
    let mut h = Harness::new(Default::default());
    h.frame(state);
    open_line(&mut h);
    h.frame(state);
    h
}

fn key(h: &mut Harness, key: usize) {
    h.input.keys.push(key);
}

/// Enter pressed as the key map binds it: the key, and the game's enter-chat action.
fn open_line(h: &mut Harness) {
    h.input.keys.push(vk::ENTER);
    h.input
        .actions
        .push(pyreal_client_contract::actions::chat_entry::BEGIN_CHAT_MODE.0);
}

#[test]
fn enter_opens_the_line_on_the_game_s_draft_and_each_edit_is_told_to_the_game() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    open_line(&mut h);
    let opened = h.frame(&state);
    assert!(h.ui.hud.log.typing());
    assert!(
        opened.requests.is_empty(),
        "opening on the draft changes nothing: {:?}",
        opened.requests
    );
    h.input.chars = vec!['!'];
    let typed = h.frame(&state);
    assert_eq!(
        typed.requests,
        [UiRequest::ChatEntry {
            window: MAIN,
            text: "half a line!".into(),
            action: EntryAction::Draft,
        }]
    );
}

#[test]
fn enter_sends_the_line_to_the_main_window_for_the_game_to_route_by_its_destination() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    open_line(&mut h);
    h.frame(&state);
    key(&mut h, vk::ENTER);
    let sent = h.frame(&state);
    assert_eq!(
        sent.requests,
        [UiRequest::ChatLine {
            text: "half a line".into(),
            window: MAIN,
        }]
    );
    assert!(!h.ui.hud.log.typing());
}

#[test]
fn tab_asks_for_the_next_destination_and_up_asks_the_game_for_the_line_before() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    open_line(&mut h);
    h.frame(&state);
    key(&mut h, vk::TAB);
    key(&mut h, vk::UP);
    let out = h.frame(&state);
    // After All in the game's menu order come Vassals, Fellowship (3), ...
    assert!(out.requests.contains(&UiRequest::SetTalkFocus { focus: 3 }));
    assert!(out.requests.contains(&UiRequest::ChatEntry {
        window: MAIN,
        text: "half a line".into(),
        action: EntryAction::Previous,
    }));
}

#[test]
fn a_reply_the_game_begins_opens_the_line_with_its_text_in_classic() {
    let mut state = in_world();
    let mut h = classic();
    h.frame(&state);
    state.chat_entry = vec![EntryUpdate {
        window: MAIN,
        text: "@tell Borin, ".into(),
        cursor: 13,
        focus: true,
    }];
    h.frame(&state);
    state.chat_entry.clear();
    assert!(h.ui.hud.log.typing());
    key(&mut h, vk::ENTER);
    let sent = h.frame(&state);
    assert_eq!(
        sent.requests,
        [UiRequest::ChatLine {
            text: "@tell Borin, ".into(),
            window: MAIN,
        }]
    );
}

#[test]
fn escape_closes_the_line_and_leaves_the_draft_with_the_game() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    open_line(&mut h);
    h.frame(&state);
    key(&mut h, vk::ESCAPE);
    let out = h.frame(&state);
    assert!(!h.ui.hud.log.typing());
    assert!(
        !out.requests
            .iter()
            .any(|r| matches!(r, UiRequest::ChatLine { .. })),
        "nothing is sent"
    );
}

#[test]
fn the_line_s_caret_moves_with_the_arrows_and_typing_goes_in_at_it() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    open_line(&mut h);
    h.frame(&state);
    key(&mut h, vk::HOME);
    h.frame(&state);
    h.input.chars = "a ".chars().collect();
    let typed = h.frame(&state);
    assert_eq!(
        typed.requests,
        [UiRequest::ChatEntry {
            window: MAIN,
            text: "a half a line".into(),
            action: EntryAction::Draft,
        }]
    );
}

#[test]
fn a_mac_arrow_key_reaches_the_line_as_a_key_and_types_nothing() {
    use pyreal_input::host::HostEvent;
    use pyreal_input::keys::Key;
    let mut front = pyreal_horizon::runtime::HorizonFrontEnd::new(
        std::sync::Arc::new(pyreal_horizon::art::Art::empty()),
        Default::default(),
        None,
    );
    // macOS sends Left and Right with private-use text, U+F702 and U+F703.
    for (key, text) in [
        (Key::new(0x25, 0xE04B), "\u{F702}"),
        (Key::new(0x27, 0xE04D), "\u{F703}"),
    ] {
        front.host_event(
            &HostEvent::KeyboardInput {
                key,
                pressed: true,
                text: Some(text.into()),
            },
            0,
        );
    }
    assert_eq!(front.frame_input.keys, [vk::LEFT, vk::RIGHT]);
    assert!(front.frame_input.chars.is_empty());
}

#[test]
fn control_a_then_c_copies_the_whole_line_and_control_v_pastes_over_it() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    open_line(&mut h);
    h.frame(&state);
    h.input.ctrl = true;
    key(&mut h, 0x41);
    key(&mut h, 0x43);
    h.frame(&state);
    assert_eq!(h.input.copied.take().as_deref(), Some("half a line"));
    h.input.paste = Some("a whole line".into());
    key(&mut h, 0x56);
    let pasted = h.frame(&state);
    assert_eq!(
        pasted.requests,
        [UiRequest::ChatEntry {
            window: MAIN,
            text: "a whole line".into(),
            action: EntryAction::Draft,
        }]
    );
}

#[test]
fn a_drag_over_the_log_selects_its_lines_and_control_c_copies_them() {
    use pyreal_horizon::ui::game::ChatLine;
    let mut state = in_world();
    state.new_chat = vec![
        ChatLine {
            chat_type: 0,
            text: "You say, \"hello\"".into(),
            window: MAIN,
        },
        ChatLine {
            chat_type: 0,
            text: "Cora Vell says, \"well met\"".into(),
            window: MAIN,
        },
    ];
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    state.new_chat.clear();
    let dock = h.ui.hud.log.dock;
    // Pressed above the lines (they sit at the bottom of the log) and dragged below the window.
    h.input.mouse = (dock.x + 20.0, dock.y + 10.0);
    h.input.down[0] = true;
    h.input.pressed[0] = true;
    h.frame(&state);
    h.input.mouse = (dock.x + 20.0, dock.bottom() + 40.0);
    h.frame(&state);
    h.input.down[0] = false;
    h.input.released[0] = true;
    h.frame(&state);
    h.input.ctrl = true;
    key(&mut h, 0x43);
    h.frame(&state);
    assert_eq!(
        h.input.copied.take().as_deref(),
        Some("You say, \"hello\"\nCora Vell says, \"well met\"")
    );
    // While the lines hold a selection, Control-C is the interface's: the game's C key (the
    // character window) never sees it. Without Control, C goes on to the game.
    {
        use pyreal_input::host::HostEvent;
        use pyreal_input::keys::Key;
        let mut front = pyreal_horizon::runtime::HorizonFrontEnd::new(
            std::sync::Arc::new(pyreal_horizon::art::Art::empty()),
            Default::default(),
            None,
        );
        std::mem::swap(&mut front.ui, &mut h.ui);
        let key = |vk, scan, pressed| HostEvent::KeyboardInput {
            key: Key::new(vk, scan),
            pressed,
            text: None,
        };
        assert!(front.host_event(&key(0x43, 0x2E, true), 0).forward);
        front.host_event(&key(0x11, 0x1D, true), 0);
        assert!(!front.host_event(&key(0x43, 0x2E, true), 0).forward);
        std::mem::swap(&mut front.ui, &mut h.ui);
    }
    // A click that is not dragged lets the selection go.
    h.input.ctrl = false;
    h.input.mouse = (dock.x + 20.0, dock.y + 10.0);
    h.input.down[0] = true;
    h.input.pressed[0] = true;
    h.frame(&state);
    h.input.down[0] = false;
    h.frame(&state);
    h.input.ctrl = true;
    key(&mut h, 0x43);
    h.frame(&state);
    assert!(h.input.copied.is_none());
}

/// The interface with the pieces and fonts the client carries, so its text has its widths.
fn with_fonts() -> Harness {
    styled_with_fonts(pyreal_horizon::ui::chat::ChatStyle::Modern)
}

/// [`with_fonts`], its chat input of `style`.
fn styled_with_fonts(style: pyreal_horizon::ui::chat::ChatStyle) -> Harness {
    use std::sync::Arc;
    let art = Arc::new(pyreal_horizon::art::Art::new(Arc::new(
        pyreal_horizon::pieces::Pieces::built_in(),
    )));
    let options = pyreal_horizon::options::HorizonOptions {
        chat_style: style,
        ..Default::default()
    };
    Harness {
        ui: pyreal_horizon::ui::HorizonUi::new(art, options),
        list: Default::default(),
        input: Default::default(),
        screen: (1920.0, 1080.0),
    }
}

#[test]
fn a_web_address_in_a_line_is_a_link_and_a_click_on_it_asks_to_open_it() {
    use pyreal_horizon::ui::game::ChatLine;
    const URL: &str = "https://example.com/a?b=c";
    let mut state = in_world();
    state.new_chat = vec![ChatLine {
        chat_type: 0,
        text: format!(
            r#"<Tell:IIDString:1342177281:Ana>Ana<\Tell> says, "see {URL}." and https://example.org"#
        ),
        window: MAIN,
    }];
    let mut h = with_fonts();
    h.frame(&state);
    state.new_chat.clear();
    let link = h.ui.hud.log.control(URL).expect("the link is drawn");
    assert!(
        h.ui.hud.log.control("https://example.org").is_some(),
        "and the second"
    );
    // A press on the link, let go: the game is asked to open it, and nothing else happens.
    h.input.mouse = (link.x + link.w - 1.0, link.y + link.h / 2.0);
    h.input.down[0] = true;
    h.input.pressed[0] = true;
    let clicked = h.frame(&state);
    h.input.down[0] = false;
    h.input.released[0] = true;
    let released = h.frame(&state);
    assert_eq!(
        clicked.requests,
        [UiRequest::ChatLinkClicked(URL.into())],
        "the whole address, without the full stop"
    );
    assert!(released.requests.is_empty(), "{:?}", released.requests);
    assert!(
        !h.ui.hud.log.has_selection(),
        "a click on a link selects nothing"
    );
    // Beside it, a press is the log's own: it starts a selection and asks nothing.
    h.input.released[0] = false;
    h.input.mouse = (link.x + link.w + 40.0, link.y + link.h / 2.0);
    h.input.down[0] = true;
    h.input.pressed[0] = true;
    let beside = h.frame(&state);
    assert!(
        !beside
            .requests
            .iter()
            .any(|r| matches!(r, UiRequest::ChatLinkClicked(_))),
        "{:?}",
        beside.requests
    );
}

#[test]
fn a_line_with_a_link_copies_as_its_plain_text() {
    use pyreal_horizon::ui::game::ChatLine;
    let mut state = in_world();
    state.new_chat = vec![ChatLine {
        chat_type: 0,
        text: "Cora says, \"https://example.com/x\"".into(),
        window: MAIN,
    }];
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    state.new_chat.clear();
    let dock = h.ui.hud.log.dock;
    h.input.mouse = (dock.x + 20.0, dock.y + 10.0);
    h.input.down[0] = true;
    h.input.pressed[0] = true;
    h.frame(&state);
    h.input.mouse = (dock.x + 20.0, dock.bottom() + 40.0);
    h.frame(&state);
    h.input.down[0] = false;
    h.input.released[0] = true;
    h.frame(&state);
    h.input.ctrl = true;
    key(&mut h, 0x43);
    h.frame(&state);
    assert_eq!(
        h.input.copied.take().as_deref(),
        Some("Cora says, \"https://example.com/x\"")
    );
}

/// A blank line in a message leaves the rows after it at their own characters: a link below it
/// is drawn where it is and a click on it asks to open it.
#[test]
fn a_link_after_a_blank_line_in_a_message_is_still_a_link() {
    use pyreal_horizon::ui::game::ChatLine;
    const URL: &str = "https://example.com/z";
    let mut state = in_world();
    state.new_chat = vec![ChatLine {
        chat_type: 0,
        text: format!("Cora says, \"first\n\nsee {URL} there\""),
        window: MAIN,
    }];
    let mut h = with_fonts();
    h.frame(&state);
    state.new_chat.clear();
    let link = h.ui.hud.log.control(URL).expect("the link is drawn");
    h.input.mouse = (link.x + link.w / 2.0, link.y + link.h / 2.0);
    h.input.down[0] = true;
    h.input.pressed[0] = true;
    let clicked = h.frame(&state);
    assert_eq!(clicked.requests, [UiRequest::ChatLinkClicked(URL.into())]);
}

#[test]
fn tab_in_a_tells_name_asks_the_game_for_a_name_from_the_selection_and_players_nearby() {
    use pyreal_horizon::ui::game::{Blip, BlipKind, Relation, Target};
    use pyreal_primitives::ObjectId;
    let blip = |id, dx: f32, kind, name: &str| Blip {
        id: ObjectId(id),
        dx,
        dy: 0.0,
        kind,
        colour: 0,
        shape: 0,
        name: name.into(),
    };
    let mut state = in_world();
    state.chat_draft = "/t".into();
    state.player_id = Some(ObjectId(0x5000_0001));
    state.target = Some(Target {
        id: ObjectId(0x5000_0002),
        name: "Gale".into(),
        relation: Relation::Player,
        ..Target::default()
    });
    state.targets = vec![
        blip(0x5000_0003, 30.0, BlipKind::Player, "Far"),
        blip(0x8000_0004, 1.0, BlipKind::Creature, "Drudge"),
        blip(0x5000_0005, 5.0, BlipKind::Fellow, "Near"),
        blip(0x5000_0001, 0.0, BlipKind::Player, "Tester"),
    ];
    state.friends = vec![("Online".into(), true), ("Away".into(), false)];
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    open_line(&mut h);
    h.frame(&state);
    h.input.shift = true;
    key(&mut h, vk::TAB);
    let out = h.frame(&state);
    let Some(UiRequest::ChatEntry {
        text,
        action: EntryAction::Complete { names, back },
        ..
    }) = out.requests.first()
    else {
        panic!("{:?}", out.requests);
    };
    assert_eq!(text, "/t");
    assert!(back, "Shift+Tab goes back");
    assert_eq!(names.me.as_deref(), Some("Tester"));
    assert_eq!(names.selected.as_deref(), Some("Gale"));
    assert_eq!(names.nearby, ["Near", "Far"]);
    assert_eq!(names.friends, ["Online"]);
    assert!(
        !out.requests
            .iter()
            .any(|r| matches!(r, UiRequest::SetTalkFocus { .. })),
        "a tell's Tab does not change the channel"
    );
}

#[test]
fn the_typed_line_takes_the_colour_of_the_channel_it_goes_to() {
    use pyreal_horizon::ui::chat::{channel_label, focus_colour};
    let say = ("Say", focus_colour(1));
    assert_eq!(say.1, 0xFFFF_FFFF);
    for (text, label, colour) in [
        ("/t Alba, hi", "Tell", 0xFFFF_80FF),
        ("/r hi", "Tell", 0xFFFF_80FF),
        ("/f ready", "Fellowship", 0xFFAA_AAFF),
        ("/a hello", "Allegiance", 0xFF40_FF40),
        ("/p hi", "Allegiance", 0xFF40_C040),
        ("/cg lfg", "General", 0xFFFF_C0C0),
        ("/e waves", "Emote", 0xFFFF_8040),
        ("@loc", "Command", 0xFFCC_CCCC),
        ("hello", "Say", 0xFFFF_FFFF),
    ] {
        assert_eq!(
            channel_label(text, say),
            (label.to_owned(), colour),
            "{text}"
        );
    }
    assert_eq!(focus_colour(3), 0xFFAA_AAFF, "fellowship");
    assert_eq!(focus_colour(7), 0xFF40_FF40, "allegiance");
    assert_eq!(focus_colour(8), 0xFFFF_C0C0, "general");
}

#[test]
fn a_tell_s_comma_turns_its_name_into_the_chip_and_the_line_keeps_the_message() {
    let mut state = in_world();
    let mut h = opened(&mut state);
    type_text(&mut h, "/t Alba,");
    let out = h.frame(&state);
    assert_eq!(draft(&out).as_deref(), Some("@tell Alba, "));
    type_text(&mut h, "hi there");
    let out = h.frame(&state);
    assert_eq!(draft(&out).as_deref(), Some("@tell Alba, hi there"));
    key(&mut h, vk::ENTER);
    assert_eq!(sent(&h.frame(&state)), ["@tell Alba, hi there"]);
}

#[test]
fn a_tell_stays_after_sending_and_escape_until_another_channel_is_chosen() {
    let mut state = in_world();
    let mut h = opened(&mut state);
    type_text(&mut h, "/t Alba, one");
    h.frame(&state);
    key(&mut h, vk::ENTER);
    assert_eq!(sent(&h.frame(&state)), ["@tell Alba, one"]);
    open_line(&mut h);
    h.frame(&state);
    key(&mut h, vk::ESCAPE);
    h.frame(&state);
    assert!(!h.ui.hud.log.typing());
    open_line(&mut h);
    h.frame(&state);
    type_text(&mut h, "two");
    h.frame(&state);
    key(&mut h, vk::ENTER);
    assert_eq!(sent(&h.frame(&state)), ["@tell Alba, two"]);
    open_line(&mut h);
    h.frame(&state);
    type_text(&mut h, "/f ");
    let out = h.frame(&state);
    assert!(out.requests.contains(&UiRequest::SetTalkFocus { focus: 3 }));
    type_text(&mut h, "three");
    h.frame(&state);
    key(&mut h, vk::ENTER);
    assert_eq!(
        sent(&h.frame(&state)),
        ["three"],
        "the channel's, not a tell"
    );
}

#[test]
fn backspace_on_an_empty_tell_turns_the_chip_back_into_its_text() {
    let mut state = in_world();
    let mut h = opened(&mut state);
    type_text(&mut h, "/t Sir Fenn,");
    h.frame(&state);
    key(&mut h, vk::BACK);
    let out = h.frame(&state);
    assert_eq!(draft(&out).as_deref(), Some("/t Sir Fenn"));
    key(&mut h, vk::BACK);
    let out = h.frame(&state);
    assert_eq!(
        draft(&out).as_deref(),
        Some("/t Sir Fen"),
        "then it is text"
    );
}

#[test]
fn a_reply_command_s_space_names_the_last_teller_and_tab_steps_through_recent_partners() {
    let mut state = in_world();
    state.reply_targets.last_teller = Some("Bex".into());
    state.reply_targets.recent = vec!["Bex".into(), "Alba".into(), "Cato".into()];
    let mut h = opened(&mut state);
    type_text(&mut h, "/r ");
    let out = h.frame(&state);
    assert_eq!(draft(&out).as_deref(), Some("@tell Bex, "));
    key(&mut h, vk::TAB);
    let out = h.frame(&state);
    assert_eq!(draft(&out).as_deref(), Some("@tell Alba, "));
    assert!(
        !out.requests
            .iter()
            .any(|r| matches!(r, UiRequest::SetTalkFocus { .. })),
        "Tab on the chip does not change the channel"
    );
    h.input.shift = true;
    key(&mut h, vk::TAB);
    let out = h.frame(&state);
    h.input.shift = false;
    assert_eq!(
        draft(&out).as_deref(),
        Some("@tell Bex, "),
        "Shift+Tab goes back"
    );
}

#[test]
fn a_reply_the_game_begins_opens_the_chip_and_an_empty_message_sends_nothing() {
    let mut state = in_world();
    let mut h = opened(&mut state);
    state.chat_entry = vec![EntryUpdate {
        window: MAIN,
        text: "@tell Borin, ".into(),
        cursor: 13,
        focus: true,
    }];
    h.frame(&state);
    state.chat_entry.clear();
    key(&mut h, vk::ENTER);
    assert!(sent(&h.frame(&state)).is_empty());
    open_line(&mut h);
    h.frame(&state);
    type_text(&mut h, "hello");
    h.frame(&state);
    key(&mut h, vk::ENTER);
    assert_eq!(sent(&h.frame(&state)), ["@tell Borin, hello"]);
}

#[test]
fn a_command_typed_in_a_tell_is_sent_as_a_command() {
    let mut state = in_world();
    let mut h = opened(&mut state);
    type_text(&mut h, "/t Alba,/loc");
    h.frame(&state);
    key(&mut h, vk::ENTER);
    assert_eq!(sent(&h.frame(&state)), ["/loc"]);
}

#[test]
fn a_tell_sent_with_only_its_name_turns_the_line_to_that_person() {
    let mut state = in_world();
    let mut h = opened(&mut state);
    type_text(&mut h, "/t Alba");
    h.frame(&state);
    key(&mut h, vk::ENTER);
    let out = h.frame(&state);
    assert!(sent(&out).is_empty());
    assert_eq!(draft(&out).as_deref(), Some("@tell Alba, "));
    assert!(h.ui.hud.log.typing(), "the line stays open");
}

#[test]
fn classic_keeps_a_tell_as_its_typed_text() {
    let mut state = in_world();
    state.chat_draft.clear();
    let mut h = classic();
    h.frame(&state);
    open_line(&mut h);
    h.frame(&state);
    type_text(&mut h, "/t Alba,");
    let out = h.frame(&state);
    assert_eq!(draft(&out).as_deref(), Some("/t Alba,"));
    type_text(&mut h, " hi");
    h.frame(&state);
    key(&mut h, vk::ENTER);
    assert_eq!(sent(&h.frame(&state)), ["/t Alba, hi"]);
    open_line(&mut h);
    h.frame(&state);
    type_text(&mut h, "again");
    h.frame(&state);
    key(&mut h, vk::ENTER);
    assert_eq!(sent(&h.frame(&state)), ["again"], "a tell does not stay");
}

/// In the world with an empty draft, with a line of `chat_type` for each of `lines` taken.
fn with_lines(lines: &[(u8, &str)]) -> (Harness, GameState) {
    use pyreal_horizon::ui::game::ChatLine;
    let mut state = in_world();
    state.chat_draft.clear();
    state.chat_windows = pyreal_horizon::ui::chat::WINDOWS
        .iter()
        .map(|id| pyreal_horizon::ui::game::ChatWindow {
            id: *id,
            filter: pyreal_horizon::ui::chat::default_filter(*id),
            title: None,
        })
        .collect();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    state.new_chat = lines
        .iter()
        .map(|(chat_type, text)| ChatLine {
            chat_type: *chat_type,
            text: (*text).into(),
            window: 0,
        })
        .collect();
    h.frame(&state);
    state.new_chat.clear();
    h.frame(&state);
    (h, state)
}

#[test]
fn a_tell_opens_no_tab_of_its_own_and_the_log_keeps_the_game_s_five() {
    let (h, _) = with_lines(&[(3, "Alba tells you, \"hi\""), (4, "You tell Bex, \"yo\"")]);
    for name in ["Alba", "Bex", "Tells"] {
        assert!(h.ui.hud.log.control(name).is_none(), "no {name} tab");
    }
    for name in ["Main", "Combat", "Allegiance", "Fellowship", "Global"] {
        assert!(h.ui.hud.log.control(name).is_some(), "{name}");
    }
    assert!(
        h.ui.hud.log.control("Close Tab").is_none() && h.ui.hud.log.control("Close Alba").is_none()
    );
}

#[test]
fn a_name_in_a_tell_to_the_player_is_a_link_without_the_game_s_tag() {
    let (h, _) = with_lines(&[(3, "Alba tells you, \"hi\"")]);
    assert!(h.ui.hud.log.control("Name: Alba").is_some());
}

#[test]
fn an_unread_count_stands_in_a_circle_and_each_tab_can_leave_it_off() {
    let (mut h, state) = with_lines(&[(21, "You hit the drudge for 12 points of damage!")]);
    let badge = h.ui.hud.log.control("Unread Combat").expect("the count");
    assert_eq!(badge.w, badge.h, "one figure is a circle: {badge:?}");
    assert!(
        h.ui.hud.log.control("Unread Main").is_none(),
        "the tab shown has none"
    );
    h.ui.options.chat_unread[1] = false;
    h.frame(&state);
    assert!(h.ui.hud.log.control("Unread Combat").is_none());
}

/// The input line's bar as this frame drew it.
fn input_bar(h: &Harness) -> pyreal_horizon::draw::Rect {
    h.list
        .quads
        .iter()
        .find(|q| q.tex.is_none() && q.colour == 0xE020_1C14)
        .expect("the input bar")
        .dst
}

/// The top and bottom of the letters drawn in the bar between `x0` and `x1`, their dark edges
/// included.
fn letters(h: &Harness, bar: pyreal_horizon::draw::Rect, x0: f32, x1: f32) -> (f32, f32) {
    h.list
        .quads
        .iter()
        .filter(|q| {
            q.tex.is_some()
                && q.dst.x >= x0
                && q.dst.right() <= x1
                && q.dst.y >= bar.y - 4.0
                && q.dst.bottom() <= bar.bottom() + 4.0
        })
        .fold((f32::MAX, f32::MIN), |(t, b), q| {
            (t.min(q.dst.y), b.max(q.dst.bottom()))
        })
}

#[test]
fn the_tell_chip_stands_in_the_middle_of_the_input_bar_with_the_text_on_its_line() {
    let mut state = in_world();
    state.chat_draft.clear();
    let mut h = with_fonts();
    h.frame(&state);
    open_line(&mut h);
    h.frame(&state);
    type_text(&mut h, "/t Alba,");
    h.frame(&state);
    type_text(&mut h, "Tell");
    h.frame(&state);
    let bar = input_bar(&h);
    let chip = h.ui.hud.log.control("Chat Destination").expect("the chip");
    // As much room above it, under the bar's top line, as below it.
    let (above, below) = (chip.y - (bar.y + 1.0), bar.bottom() - chip.bottom());
    assert!(
        (above - below).abs() < 0.5,
        "{above} above the chip, {below} below"
    );
    assert!(above >= 4.0, "the chip has room around it: {above}");
    // Its name and the typed text on one line, their letters in the chip's middle.
    let name = letters(&h, bar, chip.x, chip.right());
    let typed = letters(&h, bar, chip.right(), bar.right());
    assert!((name.0 - typed.0).abs() < 0.5 && (name.1 - typed.1).abs() < 0.5);
    let middle = (typed.0 + typed.1) / 2.0;
    assert!(
        (middle - (chip.y + chip.h / 2.0)).abs() < 1.5,
        "the letters' middle {middle} against the chip's {}",
        chip.y + chip.h / 2.0
    );
}

#[test]
fn classic_s_line_is_in_the_middle_of_the_input_bar() {
    let mut state = in_world();
    state.chat_draft.clear();
    let mut h = styled_with_fonts(pyreal_horizon::ui::chat::ChatStyle::Classic);
    h.frame(&state);
    open_line(&mut h);
    h.frame(&state);
    type_text(&mut h, "Tell");
    h.frame(&state);
    let bar = input_bar(&h);
    let label = h.ui.hud.log.control("Chat Destination").expect("the label");
    let name = letters(&h, bar, label.x, label.right());
    let typed = letters(&h, bar, label.right(), bar.right());
    assert!(
        (name.0 - typed.0).abs() < 1.0,
        "the label and the text share a line"
    );
    let (above, below) = (typed.0 - (bar.y + 1.0), bar.bottom() - typed.1);
    assert!(
        above.min(below) >= 5.0,
        "room around the line: {above} above, {below} below"
    );
    let middle = (typed.0 + typed.1) / 2.0;
    let inside = bar.y + 1.0 + (bar.h - 1.0) / 2.0;
    assert!(
        (middle - inside).abs() < 1.5,
        "the letters' middle {middle} against the bar's {inside}"
    );
}

#[test]
fn the_input_bar_shows_while_the_line_is_closed_and_a_click_on_it_opens_the_line() {
    let state = in_world();
    let mut h = with_fonts();
    h.frame(&state);
    h.frame(&state);
    assert!(!h.ui.hud.log.typing());
    let bar = input_bar(&h);
    let line = h.ui.hud.log.control("Chat Line").expect("the closed line");
    let label =
        h.ui.hud
            .log
            .control("Chat Destination")
            .expect("its destination");
    // The game's draft shows in it, after where it goes.
    let draft = letters(&h, bar, label.right(), bar.right());
    assert!(draft.0 < draft.1, "the draft is drawn");
    // The window stands the same, open or closed.
    let corner = h.ui.hud.log.control("Resize 1").unwrap();
    h.input.mouse = (line.right() - 20.0, line.y + line.h / 2.0);
    h.input.pressed[0] = true;
    h.frame(&state);
    h.input.mouse = (5.0, 5.0);
    h.frame(&state);
    assert!(h.ui.hud.log.typing(), "the click opened the line");
    assert_eq!(h.ui.hud.log.control("Resize 1"), Some(corner));
    type_text(&mut h, " more");
    h.frame(&state);
    key(&mut h, vk::ENTER);
    assert_eq!(sent(&h.frame(&state)), ["half a line more"]);
}

#[test]
fn a_click_on_the_closed_line_s_destination_opens_its_list() {
    let state = in_world();
    let mut h = with_fonts();
    h.frame(&state);
    h.frame(&state);
    let label =
        h.ui.hud
            .log
            .control("Chat Destination")
            .expect("its destination");
    h.input.mouse = (label.x + label.w / 2.0, label.y + label.h / 2.0);
    h.input.pressed[0] = true;
    h.frame(&state);
    assert!(h.ui.hud.log.wants_escape(), "the list is open");
    assert!(!h.ui.hud.log.typing(), "the line stays closed");
}

#[test]
fn a_closed_tell_shows_its_person_in_the_chip_and_its_message() {
    let mut state = in_world();
    state.chat_draft.clear();
    let mut h = with_fonts();
    h.frame(&state);
    open_line(&mut h);
    h.frame(&state);
    type_text(&mut h, "/t Alba,");
    h.frame(&state);
    key(&mut h, vk::ESCAPE);
    h.frame(&state);
    state.chat_draft = "@tell Alba, see you".into();
    h.frame(&state);
    assert!(!h.ui.hud.log.typing());
    let chip = h.ui.hud.log.control("Chat Destination").expect("the chip");
    assert!(chip.w > 40.0, "the chip names Alba: {chip:?}");
    let line = h.ui.hud.log.control("Chat Line").unwrap();
    h.input.mouse = (line.right() - 20.0, line.y + line.h / 2.0);
    h.input.pressed[0] = true;
    h.frame(&state);
    type_text(&mut h, " soon");
    h.frame(&state);
    key(&mut h, vk::ENTER);
    assert_eq!(sent(&h.frame(&state)), ["@tell Alba, see you soon"]);
}
