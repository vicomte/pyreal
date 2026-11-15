//! The shipped key maps, the input manager and the actions the keys reach.

use super::*;
/// The host's own resolved key for a physical one.
///
/// # Panics
/// Panics on a key the host has no scan code for, which is not a key this client could ever see.
pub(super) fn key(code: KeyCode) -> pyreal_input::keys::Key {
    key_from_key_code(code).expect("the host names this key")
}

// ---------------------------------------------------------------------------------------------
// keymap.function-keys.*
//
// The twelve expectations are the shipped map's own, read off the generated binding fixture; one
// is deliberately bound to nothing at all, which makes the reading a measurement.
// ---------------------------------------------------------------------------------------------

/// The shipped default for each function key. `None` is a key the shipped map leaves free.
const FUNCTION_KEYS: [(KeyCode, Option<u32>); 12] = [
    (KeyCode::F1, Some(0x7B)),
    (KeyCode::F2, Some(0x3E)),
    (KeyCode::F3, Some(0x1000_000E)),
    (KeyCode::F4, Some(0x1000_000F)),
    (KeyCode::F5, Some(0x1000_0011)),
    (KeyCode::F6, Some(0x1000_0012)),
    (KeyCode::F7, None),
    (KeyCode::F8, Some(0x1000_0014)),
    (KeyCode::F9, Some(0x1000_0015)),
    (KeyCode::F10, Some(0x1000_0016)),
    (KeyCode::F11, Some(0x1000_001A)),
    (KeyCode::F12, Some(0x1000_0019)),
];

/// Every function key produces the action the shipped map binds it to, and the free one produces
/// none.
pub(super) fn every_function_key_does_what_the_shipped_map_says() {
    let mut kb = BareKeyboard::new();
    let got: Vec<(KeyCode, Vec<u32>)> = FUNCTION_KEYS
        .iter()
        .map(|(k, _)| (*k, kb.tap(key(*k))))
        .collect();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "keymap.function-keys.each-one-does-what-the-shipped-map-binds-it-to",
        move |_| {
            got.iter()
                .zip(FUNCTION_KEYS)
                .all(|((_, fired), (_, want))| *fired == want.into_iter().collect::<Vec<u32>>())
        },
    );
}

// ---------------------------------------------------------------------------------------------
// keymap.rebind.*
//
// The key the rebind uses is the one key in both shipped maps that fires nothing, which is what
// makes "it walks now" unambiguous; the key it is taken from is the shipped one for walking
// forward.
// ---------------------------------------------------------------------------------------------

/// The movement map and the walk-forward action, as the shipped binding fixture names them.
pub(super) const MOVEMENT: pyreal_input::InputMapId = pyreal_input::InputMapId(4);
pub(super) const MOVE_FORWARD: pyreal_input::ActionId = pyreal_input::ActionId(0x29);
/// The scan codes of the key that walks and the key that is free.
pub(super) const SCAN_W: u16 = 0x11;
pub(super) const SCAN_FREE: u16 = 0x41;

/// The control a binding is, and the same control as the key-hit handler receives it: on release.
pub(super) fn control(offset: u16, activation: u32) -> pyreal_input::ControlChord {
    use pyreal_input::spec::{ControlCode, SubControlIndex};
    pyreal_input::ControlChord::new(
        ControlCode::new(0, SubControlIndex::None, offset),
        0,
        activation,
    )
}

/// Hold `code` for three frames and answer whether the body ever walked the way `read` looks.
pub(super) fn held_moves(
    c: &mut HeadlessClient,
    hands: &mut Hands,
    code: KeyCode,
    read: fn(&pyreal_client::app::App) -> bool,
) -> bool {
    // **Held and not tapped.** A down and an up inside one frame cancel before the frame's sweep
    // looks, so a tap reports "did not walk" for a reason that has nothing to do with the binding.
    hands.press(c, key(code));
    let mut ever = false;
    for _ in 0..3 {
        c.tick(1);
        ever |= read(c.view().expect_app());
    }
    hands.release(c, key(code));
    c.tick(1);
    ever
}

pub(super) fn forward(app: &pyreal_client::app::App) -> bool {
    app.probe().char_input().forward
}

fn backward(app: &pyreal_client::app::App) -> bool {
    app.probe().char_input().back
}

/// A rebound key walks the body and the key it was taken from stops walking it.
pub(super) fn a_rebound_key_walks_the_body_and_the_old_one_stops() {
    use pyreal_input::binding::{Capture, DO_NOTHING};

    let (mut c, _shard) = super::relog::a_client_in_the_world(ClientSpec::gameplay_in_world(6));
    let mut hands = Hands::new();

    // 1. Before: the shipped key walks and the free one does not -- and produced no action at
    //    all, so it is genuinely free rather than bound to something that does nothing.
    let shipped_walks = held_moves(&mut c, &mut hands, KeyCode::KeyW, forward);
    let fired_before = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .stats
        .actions_fired;
    let free_walks = held_moves(&mut c, &mut hands, KeyCode::F7, forward);
    let fired_after = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .stats
        .actions_fired;

    // 2. The rebind, through the client's own capture policy rather than into the map behind it.
    let (captured_click, no_conflict) = {
        let shell = c.app_mut().input_manager_mut().expect("the input shell");
        let slot = shell
            .keys_for_action(MOVE_FORWARD, MOVEMENT)
            .iter()
            .position(|k| *k == control(SCAN_W, pyreal_input::spec::activation::CLICK))
            .expect("the shipped key is one of the action's keys");
        let Capture::Ready {
            control: new,
            conflicts,
        } = shell.capture_key_hit(
            MOVEMENT,
            MOVE_FORWARD,
            control(SCAN_FREE, pyreal_input::spec::activation::UP),
            false,
        )
        else {
            panic!("the free key can be captured for an action");
        };
        let forced_to_click = new.activation == pyreal_input::spec::activation::CLICK;
        assert!(shell.set_binding(MOVEMENT, MOVE_FORWARD, Some(slot), new));
        (forced_to_click, conflicts.is_empty())
    };

    // 3. The new key walks.
    let rebound_walks = held_moves(&mut c, &mut hands, KeyCode::F7, forward);

    // 4. The old one does not -- with two denominators, because "the shipped key stopped walking"
    //    and "input stopped arriving" look identical from one measurement.
    let fired_before_old = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .stats
        .actions_fired;
    let old_walks = held_moves(&mut c, &mut hands, KeyCode::KeyW, forward);
    let fired_after_old = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .stats
        .actions_fired;
    // The interface's own back key.
    let unrelated_still_works = held_moves(&mut c, &mut hands, KeyCode::KeyS, backward);
    let freed = {
        let shell = c.app_mut().input_manager_mut().expect("the input shell");
        shell.keys_for_action(DO_NOTHING, MOVEMENT)
            == vec![control(SCAN_W, pyreal_input::spec::activation::CLICK)]
    };

    c.assert_behaviour(
        "keymap.rebind.a-rebound-key-moves-the-body-and-the-old-one-stops",
        move |_| {
            shipped_walks
            && !free_walks
            && fired_after == fired_before
            && captured_click
            && no_conflict
            && rebound_walks
            && !old_walks
            // The freed key produces nothing at all, which is why the live denominator has to be
            // a different key rather than an action count.
            && fired_after_old == fired_before_old
            && unrelated_still_works
            && freed
        },
    );
    c.shutdown();
}

/// The rebind is written into a file beside the player's preferences when the client exits, and a
/// client with nowhere to keep them writes nothing.
///
/// **This one owns its `App` outright**: the claim is about what the *shutdown sequence* did, and
/// `HeadlessClient::shutdown` takes the client by value and drops the log it answers with. See
/// `pyreal_testkit::adapters_shell::AppSpec`; that is a gap in the harness, not in the client.
pub(super) fn a_rebind_is_written_beside_the_preferences() {
    use pyreal_input::binding::Capture;
    use {pyreal_client_runtime::shutdown::Outcome, pyreal_client_runtime::shutdown::Step};

    let prefs = scratch_preferences("shell-saved");
    // A clean directory: the client writes the interface's key map beside it on the way out too.
    let dir = prefs.parent().expect("a directory").to_path_buf();
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the scratch directory");
    let mut app = build_app(&AppSpec {
        preferences_file: Some(prefs.clone()),
        ..AppSpec::in_gameplay(4)
    });

    let shell = app.input_manager_mut().expect("the input shell");
    let path = shell
        .keymap_path()
        .expect("a preferences file gives a keymap path")
        .to_path_buf();
    let beside = path.parent() == prefs.parent()
        && path.extension().and_then(std::ffi::OsStr::to_str) == Some("keymap");
    let _ = std::fs::remove_file(&path);

    let slot = shell
        .keys_for_action(MOVE_FORWARD, MOVEMENT)
        .iter()
        .position(|k| *k == control(SCAN_W, pyreal_input::spec::activation::CLICK))
        .expect("the shipped key");
    let Capture::Ready { control: new, .. } = shell.capture_key_hit(
        MOVEMENT,
        MOVE_FORWARD,
        control(SCAN_FREE, pyreal_input::spec::activation::UP),
        false,
    ) else {
        panic!("the free key can be captured");
    };
    assert!(shell.set_binding(MOVEMENT, MOVE_FORWARD, Some(slot), new));

    let log = app.shutdown();
    let saved = log
        .0
        .iter()
        .find(|(s, _)| *s == Step::SaveKeyMap)
        .map(|(_, o)| *o);
    // The interface's own key map holds the rebind; the game's is written beside it.
    let interface_map = dir.join(pyreal_client_shell::input::horizon_scheme::HORIZON_KEYMAP_FILE);
    let both_written = path.is_file() && interface_map.is_file();
    let text = std::fs::read_to_string(&interface_map).expect("the key map file exists");
    // The whole merged map, not a diff: without the freeing of the old key the shipped binding
    // would come back on the next run.
    let carries_both = text.contains("DIK_F7") && text.contains("DoNothing");
    let whole_map = text.len() > 4_000;
    let _ = std::fs::remove_dir_all(&dir);

    // The other direction: nowhere to keep preferences, nothing written.
    let bare = build_app(&AppSpec::in_gameplay(2));
    let no_path = {
        let mut bare = bare;
        let none = bare
            .input_manager_mut()
            .expect("the input shell")
            .keymap_path()
            .is_none();
        let log = bare.shutdown();
        none && log
            .0
            .iter()
            .find(|(s, _)| *s == Step::SaveKeyMap)
            .map(|(_, o)| *o)
            == Some(Outcome::Nothing)
    };

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "keymap.rebind.is-written-beside-the-preferences-on-a-clean-exit",
        move |_| {
            beside
                && saved == Some(Outcome::Ran)
                && both_written
                && carries_both
                && whole_map
                && no_path
        },
    );
}

// ---------------------------------------------------------------------------------------------
// keymap.system-keys.* and window.system-keys.*
//
// The desktop's system keys are bound, not left as an empty and unexplained map. The two rows are
// its two halves: the *action* half, where all four are consumed and do nothing, and the *window*
// half, where the three that arrive as system keys are not treated alike.
// ---------------------------------------------------------------------------------------------

/// The desktop's four, in the client's own map.
const SYSTEM_KEYS: pyreal_input::InputMapId = pyreal_input::InputMapId(0x10);

/// The four are bound last of all and swallowed without doing anything.
pub(super) fn the_desktops_four_are_taken_last_and_do_nothing() {
    use pyreal_client_runtime::objects::ObjectStream;
    use pyreal_input::ActionId;
    use pyreal_primitives::{LocalTime, ObjectId};
    use {
        pyreal_client_contract::actions::mapped as ia,
        pyreal_client_runtime::interaction::Interaction,
    };

    const THE_FOUR: [u32; 4] = [
        ia::SYSTEM_ALT_TAB.0,
        ia::SYSTEM_ALT_ENTER.0,
        ia::SYSTEM_ALT_F4.0,
        ia::SYSTEM_CTRL_SHIFT_ESC.0,
    ];

    let store = pyreal_dat::testing::open_store()
        .expect("the shipped key maps live in the retail data files");

    // 1. The shipped data binds all four, in that map, and the map is walked last.
    let (all_bound, last_in_the_walk) = {
        let shell =
            pyreal_client_shell::input::InputShell::new(&store, None).expect("the input tables");
        let section = shell
            .manager
            .keymap
            .section(SYSTEM_KEYS)
            .expect("the shipped section");
        let bound = THE_FOUR
            .iter()
            .all(|a| !section.keys_for_action(ActionId(*a)).is_empty());
        let entries = shell.manager.maps.entries();
        let at = entries
            .iter()
            .position(|e| e.map == SYSTEM_KEYS)
            .expect("the map is registered");
        (bound, at == entries.len() - 1)
    };

    // 2. Each of the four is consumed and changes nothing at all.
    let press = |action: u32| -> (
        usize,
        pyreal_client_runtime::interaction::InteractionStats,
        pyreal_client_runtime::interaction::InteractionStats,
        Option<ObjectId>,
    ) {
        let mut objects = ObjectStream::new();
        objects.world.player = Some(ObjectId(0x5000_0001));
        let mut inter = Interaction::new();
        let before = inter.stats;
        let e = pyreal_client_runtime::actions::Action {
            id: ActionId(action),
            phase: pyreal_client_runtime::actions::ActionPhase::Begin,
            extent: 1.0,
            repeats: 0,
        };
        let (unowned, left) = pyreal_client_runtime::interaction::use_time(
            &mut inter,
            &store,
            None,
            &mut objects,
            None,
            vec![e],
            false,
            (800, 600),
            LocalTime(2.0),
        );
        assert!(unowned.is_empty(), "no unowned UI request was expected");
        (left.len(), before, inter.stats, objects.world.selected)
    };

    let mut swallowed = true;
    for action in THE_FOUR {
        let (left, before, after, selected) = press(action);
        // Consumed, counted once, and nothing else about the client moved: the arm has no body.
        let mut only_the_counter = after;
        only_the_counter.system_keys_swallowed = before.system_keys_swallowed;
        swallowed &= left == 0
            && after.system_keys_swallowed == before.system_keys_swallowed + 1
            && only_the_counter == before
            && selected.is_none();
    }

    // 3. The calibration, so "the arm ran" is a measurement: an action the client has no arm for
    //    comes back unconsumed and does not touch that counter.
    let (left, _, after, _) = press(0x0000_006F);
    let unrelated_is_handed_back = left == 1 && after.system_keys_swallowed == 0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "keymap.system-keys.the-four-the-desktop-owns-are-taken-last-and-do-nothing",
        move |_| all_bound && last_in_the_walk && swallowed && unrelated_is_handed_back,
    );
}

/// The switcher never reaches the desktop; the full-screen toggle and the close do.
pub(super) fn the_window_keeps_the_switcher_and_passes_the_other_two() {
    use pyreal_input::win32::{syskey_is_consumed, SYS_KEYS_ENABLED};

    const VK_TAB: usize = 0x09;
    const VK_RETURN: usize = 0x0D;
    const VK_F4: usize = 0x73;
    const VK_ESCAPE: usize = 0x1B;

    let switcher_is_eaten = syskey_is_consumed(VK_TAB);
    let full_screen_passes = !syskey_is_consumed(VK_RETURN);
    let close_passes = !syskey_is_consumed(VK_F4);
    // Shown rather than asserted away: the fourth of the desktop's four arrives with no modifier
    // at all, so this rule is not what decides it -- and under the rule it would have been eaten.
    let the_rule_is_not_the_decider_for_the_fourth = syskey_is_consumed(VK_ESCAPE);
    let premise = !SYS_KEYS_ENABLED;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "window.system-keys.the-desktop-keeps-the-two-it-must-and-the-client-eats-the-third",
        move |_| {
            switcher_is_eaten
                && full_screen_passes
                && close_passes
                && the_rule_is_not_the_decider_for_the_fourth
                && premise
        },
    );
}

/// The maps are walked in the order the client registers them.
pub(super) fn the_chat_map_is_walked_first_and_movement_before_the_camera() {
    /// The chat window's own map, the one that leaves the chat bar, and the camera's.
    const CHAT: u32 = 0x1000_000A;
    const LEAVE_THE_BAR: u32 = 0x1000_000D;
    const CAMERA: u32 = 5;
    const EMOTES: u32 = 0x1000_0006;

    let (mut c, _shard) = super::relog::a_client_in_the_world(ClientSpec::gameplay(4));
    let (band, all): (Vec<u32>, Vec<(u32, i32)>) = {
        let shell = c.app_mut().input_manager_mut().expect("the input shell");
        let entries = shell.manager.maps.entries();
        (
            entries
                .iter()
                .filter(|e| e.priority == pyreal_input::dispatch::priority::GAMEPLAY)
                .map(|e| e.map.0)
                .collect(),
            entries.iter().map(|e| (e.map.0, e.priority)).collect(),
        )
    };

    let at = move |b: &[u32], m: u32| b.iter().position(|x| *x == m);
    let in_all = move |a: &[(u32, i32)], m: u32| a.iter().position(|(x, _)| *x == m);

    c.assert_behaviour("keymap.walk-order.the-chat-map-is-walked-first-and-movement-before-the-camera", move |_| {
        // The chat window's own map is first of its band, movement is still ahead of the camera,
        // and the emote map is ahead of movement -- the client's own registration order, which is
        // the thing a reordering defect changes.
        at(&band, CHAT) == Some(0)
            && at(&band, MOVEMENT.0) < at(&band, CAMERA)
            && at(&band, EMOTES) < at(&band, MOVEMENT.0)
            // At peace no combat-mode map is in the band at all.
            && !band.iter().any(|m| pyreal_input::combat::MODE_COMBAT_MAPS.iter().any(|x| x.0 == *m))
            // And the map that leaves the chat bar sits above the whole band, which is above the
            // barrier a focused text box puts in front of the keyboard.
            && in_all(&all, LEAVE_THE_BAR) < in_all(&all, MOVEMENT.0)
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The action bench
//
// Three scenarios here drive one input action straight through the frame's own tail --
// `pyreal_client_runtime::interaction::use_time`, which is what the action handler's caller uses here --
// and read the model back. They need no screen and no element tree, only the shipped tables, so
// they are stood up directly rather than through a whole client; the claim is booked at the end
// through a model client, the shape the login scenarios use for a scenario whose subject is not the
// game model.
// ---------------------------------------------------------------------------------------------

/// One press of one action, through the frame's own tail.
struct ActionBench {
    store: pyreal_dat::RetailDatStore,
    objects: pyreal_client_runtime::objects::ObjectStream,
    inter: pyreal_client_runtime::interaction::Interaction,
    now: f64,
}

impl ActionBench {
    fn new() -> Self {
        Self {
            store: pyreal_dat::testing::open_store()
                .expect("the frame's tail takes the retail data files: set PYREAL_TEST_DAT_DIR"),
            objects: pyreal_client_runtime::objects::ObjectStream::new(),
            inter: pyreal_client_runtime::interaction::Interaction::new(),
            now: 1.0,
        }
    }

    /// Press `action` and answer how many events came back **unconsumed** -- which is the half a
    /// fix that consumed the key and did nothing would also have to pass.
    fn press(&mut self, _map: pyreal_input::InputMapId, action: u32) -> usize {
        self.now += 1.0;
        let e = pyreal_client_runtime::actions::Action {
            id: pyreal_input::ActionId(action),
            phase: pyreal_client_runtime::actions::ActionPhase::Begin,
            extent: 1.0,
            repeats: 0,
        };
        let (unowned, left) = pyreal_client_runtime::interaction::use_time(
            &mut self.inter,
            &self.store,
            None,
            &mut self.objects,
            None,
            vec![e],
            false,
            (800, 600),
            pyreal_primitives::LocalTime(self.now),
        );
        assert!(unowned.is_empty(), "no unowned UI request was expected");
        left.len()
    }

    /// A press that must be consumed.
    fn hit(&mut self, map: pyreal_input::InputMapId, action: u32) {
        assert_eq!(
            self.press(map, action),
            0,
            "{action:#010X} must be consumed"
        );
    }

    fn select(&mut self, id: Option<pyreal_primitives::ObjectId>) {
        let mut out = pyreal_client_model::RecordingSink::default();
        self.objects.world.set_selected_object(id, false, &mut out);
    }

    fn selected(&self) -> Option<pyreal_primitives::ObjectId> {
        self.objects.world.selected
    }
}

/// Where the shipped map declares the selection actions.
const ITEM_SELECTION: pyreal_input::InputMapId = pyreal_input::InputMapId(0x1000_0007);

/// What a live press of `qc` resolves to over the shipped registration stack -- the client's own
/// map walk, presented the way a key going down is presented to it.
fn resolves_to(
    shell: &pyreal_client_shell::input::InputShell,
    qc: &pyreal_input::ControlChord,
) -> Option<(pyreal_input::InputMapId, pyreal_input::ActionId)> {
    use pyreal_input::spec::{activation, ControlChord, DeviceType};
    let km = &shell.manager.keymap;
    let live = ControlChord::new(
        qc.control,
        qc.meta_mode,
        (qc.activation & !activation::UP) | activation::LIVE,
    );
    let is_keyboard = km.device_type_of(qc.control) == Some(DeviceType::Keyboard);
    let stack = shell.manager.maps.entries().to_vec();
    pyreal_input::fire::walk_input_maps(&stack, &live, is_keyboard, |m| km.section(m))
        .map(|r| (r.input_map, r.action))
}

/// The one control the shipped key map binds to `action` in `map`.
///
/// # Panics
/// Panics when the shipped map binds none or several, which would make every claim below about a
/// binding this scenario had chosen rather than one the client has.
fn the_only_binding(
    shell: &pyreal_client_shell::input::InputShell,
    map: pyreal_input::InputMapId,
    action: u32,
) -> pyreal_input::ControlChord {
    let section = shell
        .manager
        .keymap
        .section(map)
        .unwrap_or_else(|| panic!("the shipped map has a section {:#010X}", map.0));
    let mut keys = section.keys_for_action(pyreal_input::ActionId(action));
    assert_eq!(
        keys.len(),
        1,
        "one shipped default binding for {action:#010X}"
    );
    keys.pop().expect("one")
}

// ---------------------------------------------------------------------------------------------
// keymap.modified-digits.*
//
// A modifier and a digit looks like "a key does the wrong thing" and is not a defect: the shipped
// map binds one key to two actions and the client settles it the way the original does. The
// scenario carries a counterfactual so the reading is of the walk's order.
// ---------------------------------------------------------------------------------------------

/// The modifier and a number uses a quick slot, and not the floating chat window.
pub(super) fn a_modified_number_uses_a_quick_slot_and_not_the_chat_window() {
    use pyreal_input::{ActionId, InputMapId};

    const UI_COMMANDS: InputMapId = InputMapId(0x1000_0009);
    const QUICKSLOT_COMMANDS: InputMapId = InputMapId(0x1000_000C);
    /// The four floating chat windows, and the four quick slots the same four keys reach.
    const CHAT_WINDOWS: [u32; 4] = [0x1000_0114, 0x1000_0115, 0x1000_0116, 0x1000_0117];
    const QUICK_SLOTS: [u32; 4] = [0x1000_004B, 0x1000_004C, 0x1000_004D, 0x1000_0132];
    /// The unmodified number row, the negative control.
    const USE_QUICK_SLOT_1: u32 = 0x1000_0042;

    let store = pyreal_dat::testing::open_store()
        .expect("the shipped key maps live in the retail data files");
    let shell =
        pyreal_client_shell::input::InputShell::new(&store, None).expect("the input tables");

    // 1. The shipped map really does bind one identical control to both actions, which is the
    //    input on which the tie rule applies at all.
    let mut identical = true;
    for (chat, slot) in CHAT_WINDOWS.into_iter().zip(QUICK_SLOTS) {
        let a = the_only_binding(&shell, UI_COMMANDS, chat);
        let b = the_only_binding(&shell, QUICKSLOT_COMMANDS, slot);
        identical &= a.is_exactly_equal(&b)
            && a.meta_mode != 0
            && a.activation == pyreal_input::spec::activation::CLICK;
    }

    // 2. The two maps are registered at one priority, in the client's own order, and the later of
    //    the two is therefore walked first.
    let regs: Vec<(u32, i32)> = pyreal_client_shell::input::BASE_MAP_REGISTRATIONS
        .iter()
        .map(|(_, m, p)| (*m, *p))
        .collect();
    let ui_at = regs
        .iter()
        .position(|(m, _)| *m == UI_COMMANDS.0)
        .expect("registered");
    let qs_at = regs
        .iter()
        .position(|(m, _)| *m == QUICKSLOT_COMMANDS.0)
        .expect("registered");
    let entries = shell.manager.maps.entries();
    let walk_ui = entries
        .iter()
        .position(|e| e.map == UI_COMMANDS)
        .expect("in the stack");
    let walk_qs = entries
        .iter()
        .position(|e| e.map == QUICKSLOT_COMMANDS)
        .expect("in the stack");
    let registered_in_order = ui_at < qs_at && regs[ui_at].1 == regs[qs_at].1 && walk_qs < walk_ui;

    // 3. The observable, with the plain number as its negative control.
    let mut quick_slot_wins = true;
    for (chat, slot) in CHAT_WINDOWS.into_iter().zip(QUICK_SLOTS) {
        let qc = the_only_binding(&shell, UI_COMMANDS, chat);
        quick_slot_wins &= resolves_to(&shell, &qc) == Some((QUICKSLOT_COMMANDS, ActionId(slot)));
    }
    let plain = shell
        .manager
        .keymap
        .section(QUICKSLOT_COMMANDS)
        .expect("the section")
        .keys_for_action(ActionId(USE_QUICK_SLOT_1))
        .into_iter()
        .find(|q| q.meta_mode == 0)
        .expect("an unmodified binding");
    let plain_still_works =
        resolves_to(&shell, &plain) == Some((QUICKSLOT_COMMANDS, ActionId(USE_QUICK_SLOT_1)));

    // 4. The counterfactual: the same walk with the two swapped *does* reach the chat window, so
    //    the reading above is of the order and not of a stuck reader.
    let swapped_reaches_the_window = {
        let km = &shell.manager.keymap;
        let mut stack = shell.manager.maps.entries().to_vec();
        stack.swap(walk_qs, walk_ui);
        let qc = the_only_binding(&shell, UI_COMMANDS, CHAT_WINDOWS[0]);
        let live = pyreal_input::ControlChord::new(
            qc.control,
            qc.meta_mode,
            (qc.activation & !pyreal_input::spec::activation::UP)
                | pyreal_input::spec::activation::LIVE,
        );
        pyreal_input::fire::walk_input_maps(&stack, &live, true, |m| km.section(m))
            .map(|h| (h.input_map, h.action))
            == Some((UI_COMMANDS, ActionId(CHAT_WINDOWS[0])))
    };

    let mut c = HeadlessClient::model();
    c.assert_behaviour("keymap.modified-digits.a-number-with-the-modifier-uses-a-quick-slot-and-not-the-chat-window", move |_| {
        identical
            && registered_in_order
            && quick_slot_wins
            && plain_still_works
            && swapped_reaches_the_window
    });
}

// ---------------------------------------------------------------------------------------------
// movement.walk-mode-key.*
//
// Two related checks are transcriptions and carry no row: one scans the exclusive-or and its callee
// out of the shipped executable, and the other pins the two statements of the shipped default
// against each other. The premise each of them protects -- that this character has the shipped
// default and it is on -- is asserted inside the first scenario below, where it is load-bearing.
// ---------------------------------------------------------------------------------------------

/// Whether the body is running this frame.
fn running(c: &HeadlessClient) -> bool {
    c.view().expect_app().probe().char_input().run
}

/// Switch the walk-mode action on and answer what the body did, then off and answer again.
///
/// The action and not a key: the interface's layout names the key, and under its orbit camera
/// the action switches rather than holds.
fn walk_mode_edges(c: &mut HeadlessClient) -> (bool, bool) {
    use pyreal_client_contract::actions::{movement::TOGGLE_RUN_WALK, Action};
    let switch = |c: &mut HeadlessClient| {
        c.app_mut().inject_action(Action::begin(TOGGLE_RUN_WALK));
        c.app_mut().inject_action(Action::end(TOGGLE_RUN_WALK));
        c.tick(1);
        running(c)
    };
    let on = switch(c);
    (on, switch(c))
}

/// The walk-mode key follows the run-by-default option, both ways.
pub(super) fn the_walk_mode_key_follows_the_run_by_default_option() {
    use pyreal_client_model::player::options::option::TOGGLE_RUN;

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(8));

    // The premise, asserted rather than assumed: this character has the shipped defaults and
    // run-by-default is on.
    let default_is_on = c
        .view()
        .objects()
        .world
        .player_system
        .options
        .get(TOGGLE_RUN);

    let routed_before = c.view().expect_app().probe().actions_routed();
    let (held_on, released_on) = walk_mode_edges(&mut c);
    // The denominator: "the flag did not change" and "the key never arrived" are the same reading
    // without it, and a value that silently stayed at its default is this claim's whole subject.
    let key_arrived = c.view().expect_app().probe().actions_routed() > routed_before;

    // The other arm, on the same client: with the option off the same key does the opposite.
    assert!(
        c.objects_mut()
            .world
            .player_system
            .options
            .set(TOGGLE_RUN, false),
        "the option was on and this turned it off"
    );
    let (held_off, released_off) = walk_mode_edges(&mut c);

    c.assert_behaviour(
        "movement.walk-mode-key.holding-it-follows-the-run-by-default-option-both-ways",
        move |_| {
            default_is_on && key_arrived && !held_on && released_on && held_off && !released_off
        },
    );
    c.shutdown();
}

/// The option is read on every press rather than latched when the client started.
pub(super) fn the_walk_mode_option_is_read_on_every_press() {
    use pyreal_client_model::player::options::option::TOGGLE_RUN;

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(8));

    let (held_before, _) = walk_mode_edges(&mut c);
    c.objects_mut()
        .world
        .player_system
        .options
        .set(TOGGLE_RUN, false);
    let (held_after, _) = walk_mode_edges(&mut c);

    c.assert_behaviour(
        "movement.walk-mode-key.the-option-is-read-on-every-press-and-not-at-start-up",
        move |_| {
            // The same key edge, before and after the option moved, with no relog in between.
            !held_before && held_after
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// selection.fellow.*
//
// The fellowship is built by the client's own receiver for the shard's fellowship message rather
// than by hand, so a receiver that stopped filling the table would be visible here.
// ---------------------------------------------------------------------------------------------

/// The action ids of the two keys, and the four characters the scenarios use.
const ME: pyreal_primitives::ObjectId = pyreal_primitives::ObjectId(0x5000_0001);
const BOB: pyreal_primitives::ObjectId = pyreal_primitives::ObjectId(0x5000_0002);
const CAI: pyreal_primitives::ObjectId = pyreal_primitives::ObjectId(0x5000_0003);
/// Deliberately not in the fellowship.
const STRANGER: pyreal_primitives::ObjectId = pyreal_primitives::ObjectId(0x7000_0009);

/// An action bench with `members` in a fellowship and a stranger outside it.
fn a_fellowship(members: &[pyreal_primitives::ObjectId]) -> ActionBench {
    let mut b = ActionBench::new();
    b.objects.world.player = Some(ME);
    for id in members.iter().copied().chain([STRANGER]) {
        let mut w = pyreal_client_model::Weenie::new(id);
        w.valid = true;
        b.objects.world.tables.weenies.insert(id, w);
    }
    if !members.is_empty() {
        let wire = pyreal_protocol::social::Fellowship {
            members: pyreal_protocol::archive::PackedHash {
                table_size: 8,
                entries: members
                    .iter()
                    .map(|id| {
                        (
                            id.0,
                            pyreal_protocol::social::Fellow {
                                name: format!("Fellow {:X}", id.0 & 0xFF),
                                level: 10,
                                ..pyreal_protocol::social::Fellow::default()
                            },
                        )
                    })
                    .collect(),
            },
            name: "Fellows".to_owned(),
            leader: members[0],
            share_xp: 1,
            even_xp_split: 1,
            open_fellow: 0,
            locked: 0,
            fellows_departed: pyreal_protocol::archive::PackedHash::default(),
            locks: pyreal_protocol::social::FellowshipLocks::default(),
        };
        b.objects.world.recv_fellowship_full_update(&wire);
        assert_eq!(
            b.objects
                .world
                .fellowship
                .as_ref()
                .expect("a fellowship")
                .members
                .len(),
            members.len(),
            "the client's own receiver must have filled the table"
        );
    }
    b
}

/// One key walks the fellowship forward and the other back, both wrapping.
pub(super) fn the_two_keys_walk_the_fellowship_both_ways_and_wrap() {
    use pyreal_client_contract::actions::mapped as ia;

    // The shipped keys really reach the two actions, rather than the scenario naming them.
    let store = pyreal_dat::testing::open_store().expect("the retail data files");
    let shell =
        pyreal_client_shell::input::InputShell::new(&store, None).expect("the input tables");
    let mut both_bound = true;
    for action in [ia::SELECTION_NEXT_FELLOW.0, ia::SELECTION_PREVIOUS_FELLOW.0] {
        let qc = the_only_binding(&shell, ITEM_SELECTION, action);
        both_bound &= qc.meta_mode == 0
            && resolves_to(&shell, &qc) == Some((ITEM_SELECTION, pyreal_input::ActionId(action)));
    }

    // Forward, three times round a three-member fellowship, back where it started -- and the
    // player's own character is one of the three it lands on.
    let mut b = a_fellowship(&[ME, BOB, CAI]);
    b.select(Some(ME));
    let mut forward_walk = Vec::new();
    for _ in 0..3 {
        b.hit(ITEM_SELECTION, ia::SELECTION_NEXT_FELLOW.0);
        forward_walk.push(b.selected());
    }
    let forward_cycles = b.inter.stats.selection_fellow_cycles;

    // Backward, the other way round.
    let mut b = a_fellowship(&[ME, BOB, CAI]);
    b.select(Some(CAI));
    let mut backward_walk = Vec::new();
    for _ in 0..3 {
        b.hit(ITEM_SELECTION, ia::SELECTION_PREVIOUS_FELLOW.0);
        backward_walk.push(b.selected());
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "selection.fellow.one-key-walks-the-fellowship-forward-and-the-other-back-both-wrapping",
        move |_| {
            both_bound
                && forward_walk == vec![Some(BOB), Some(CAI), Some(ME)]
                && backward_walk == vec![Some(BOB), Some(ME), Some(CAI)]
                && forward_cycles == 3
        },
    );
}

/// An outsider starts the walk at one end, and with no fellowship nothing moves.
pub(super) fn an_outsider_starts_at_one_end_and_no_fellowship_moves_nothing() {
    use pyreal_client_contract::actions::mapped as ia;

    let mut ends = Vec::new();
    for (action, want) in [
        (ia::SELECTION_NEXT_FELLOW.0, ME),
        (ia::SELECTION_PREVIOUS_FELLOW.0, CAI),
    ] {
        // A selection outside the fellowship...
        let mut b = a_fellowship(&[ME, BOB, CAI]);
        b.select(Some(STRANGER));
        b.hit(ITEM_SELECTION, action);
        let from_outsider = b.selected();
        // ...and no selection at all, which is the same leg.
        let mut b = a_fellowship(&[ME, BOB, CAI]);
        b.select(None);
        b.hit(ITEM_SELECTION, action);
        ends.push((from_outsider == Some(want), b.selected() == Some(want)));
    }

    // The negative control: with no fellowship at all neither key touches the selection. A wire
    // that selected "the first fellow" out of an absent table would pass everything above.
    let mut untouched = true;
    for action in [ia::SELECTION_NEXT_FELLOW.0, ia::SELECTION_PREVIOUS_FELLOW.0] {
        let mut b = a_fellowship(&[]);
        assert!(
            b.objects.world.fellowship.is_none(),
            "the premise: no fellowship"
        );
        b.select(Some(STRANGER));
        b.hit(ITEM_SELECTION, action);
        untouched &= b.selected() == Some(STRANGER) && b.inter.stats.selection_fellow_cycles == 0;
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour("selection.fellow.a-selection-outside-the-fellowship-starts-at-one-end-and-with-no-fellowship-nothing-moves", move |_| {
        ends.iter().all(|(a, b)| *a && *b) && untouched
    });
}

/// The cycle follows the order the fellowship panel shows.
pub(super) fn the_fellow_cycle_follows_the_panels_order() {
    use pyreal_client_contract::actions::mapped as ia;

    // The members arrive in an order that is not the panel's, so a reader that walked the message
    // rather than the model would answer the other way round.
    let mut b = a_fellowship(&[CAI, BOB, ME]);
    let panel_order: Vec<pyreal_primitives::ObjectId> = b
        .objects
        .world
        .fellowship
        .as_ref()
        .expect("a fellowship")
        .members
        .keys()
        .copied()
        .collect();
    b.select(Some(panel_order[0]));
    let mut walked = vec![panel_order[0]];
    for _ in 0..2 {
        b.hit(ITEM_SELECTION, ia::SELECTION_NEXT_FELLOW.0);
        walked.push(b.selected().expect("a selection"));
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "selection.fellow.the-cycle-follows-the-order-the-panel-shows",
        move |_| walked == panel_order && panel_order.len() == 3,
    );
}

// ---------------------------------------------------------------------------------------------
// selection.previous.*
// ---------------------------------------------------------------------------------------------

const TARGET_A: pyreal_primitives::ObjectId = pyreal_primitives::ObjectId(0x7000_0011);
const TARGET_B: pyreal_primitives::ObjectId = pyreal_primitives::ObjectId(0x7000_0012);
const TARGET_C: pyreal_primitives::ObjectId = pyreal_primitives::ObjectId(0x7000_0013);

/// An action bench with three things in the world to select.
fn three_targets() -> ActionBench {
    let mut b = ActionBench::new();
    b.objects.world.player = Some(ME);
    for id in [ME, TARGET_A, TARGET_B, TARGET_C] {
        let mut w = pyreal_client_model::Weenie::new(id);
        w.valid = true;
        b.objects.world.tables.weenies.insert(id, w);
    }
    b
}

/// The key that goes back selects the one before, and is a toggle rather than a stack.
pub(super) fn the_key_that_goes_back_is_a_toggle_and_not_a_stack() {
    use pyreal_client_contract::actions::mapped as ia;

    // The shipped key really reaches the action.
    let store = pyreal_dat::testing::open_store().expect("the retail data files");
    let shell =
        pyreal_client_shell::input::InputShell::new(&store, None).expect("the input tables");
    let qc = the_only_binding(&shell, ITEM_SELECTION, ia::SELECTION_PREVIOUS_SELECTION.0);
    let bound = qc.meta_mode == 0
        && resolves_to(&shell, &qc)
            == Some((
                ITEM_SELECTION,
                pyreal_input::ActionId(ia::SELECTION_PREVIOUS_SELECTION.0),
            ));

    // Two selections and one press: the first comes back.
    let mut b = three_targets();
    b.select(Some(TARGET_A));
    b.select(Some(TARGET_B));
    let recorded = b.objects.world.prev_selected == Some(TARGET_A);
    b.hit(ITEM_SELECTION, ia::SELECTION_PREVIOUS_SELECTION.0);
    let went_back =
        b.selected() == Some(TARGET_A) && b.inter.stats.selection_previous_restores == 1;

    // Three selections and three presses: the walk is a toggle between the last two, and the one
    // before them is never reachable.
    let mut b = three_targets();
    for id in [TARGET_A, TARGET_B, TARGET_C] {
        b.select(Some(id));
    }
    let mut walk = Vec::new();
    for _ in 0..3 {
        b.hit(ITEM_SELECTION, ia::SELECTION_PREVIOUS_SELECTION.0);
        walk.push(b.selected());
    }
    let restores = b.inter.stats.selection_previous_restores;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "selection.previous.the-key-goes-back-one-and-is-a-toggle-rather-than-a-stack",
        move |_| {
            bound
                && recorded
                && went_back
                && walk == vec![Some(TARGET_B), Some(TARGET_C), Some(TARGET_B)]
                && !walk.contains(&Some(TARGET_A))
                && restores == 3
        },
    );
}

/// With nothing behind it, the key selects nothing.
pub(super) fn with_nothing_behind_it_the_key_selects_nothing() {
    use pyreal_client_contract::actions::mapped as ia;

    let mut b = three_targets();
    let nothing_yet = b.objects.world.prev_selected.is_none();
    b.hit(ITEM_SELECTION, ia::SELECTION_PREVIOUS_SELECTION.0);
    let selected_nothing = b.selected().is_none() && b.inter.stats.selection_previous_restores == 0;

    // The same leg one step later: the very first selection of a session has nothing behind it
    // either, so the key is still inert. Without this a client that restored "the last thing that
    // was really there" would pass everything else.
    b.select(Some(TARGET_A));
    let still_nothing_behind = b.objects.world.prev_selected.is_none();
    b.hit(ITEM_SELECTION, ia::SELECTION_PREVIOUS_SELECTION.0);
    let unchanged =
        b.selected() == Some(TARGET_A) && b.inter.stats.selection_previous_restores == 0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "selection.previous.with-nothing-behind-it-the-key-selects-nothing",
        move |_| nothing_yet && selected_nothing && still_nothing_behind && unchanged,
    );
}

/// A target that was cleared comes back.
pub(super) fn a_cleared_target_comes_back() {
    use pyreal_client_contract::actions::mapped as ia;

    let mut b = three_targets();
    b.select(Some(TARGET_A));
    b.select(Some(TARGET_B));
    b.select(None);
    let cleared = b.selected().is_none() && b.objects.world.prev_selected == Some(TARGET_B);

    b.hit(ITEM_SELECTION, ia::SELECTION_PREVIOUS_SELECTION.0);
    let came_back = b.selected() == Some(TARGET_B) && b.objects.world.prev_selected.is_none();

    // ...and going back again is the empty step, so it stays where it is.
    b.hit(ITEM_SELECTION, ia::SELECTION_PREVIOUS_SELECTION.0);
    let stayed = b.selected() == Some(TARGET_B) && b.inter.stats.selection_previous_restores == 1;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "selection.previous.a-target-that-was-cleared-comes-back",
        move |_| cleared && came_back && stayed,
    );
}

// ---------------------------------------------------------------------------------------------
// pointer.the-left-button-and-the-wheel-each-belong-to-one-shipped-map
// ---------------------------------------------------------------------------------------------

/// Which maps the shipped keys bind a press and a wheel turn in, read out of the shipped data at
/// run time rather than taken from a document.
pub(super) fn the_left_button_and_the_wheel_each_belong_to_one_shipped_map() {
    // The shipped maps alone, without the interface's layout over them.
    let shipped = BareKeyboard::new();
    let binders = |offset: u16| -> Vec<u32> {
        let input = &shipped.input;
        input
            .manager
            .keymap
            .sections
            .iter()
            .filter(|s| {
                s.bindings().iter().any(|(qc, _)| {
                    input.manager.keymap.device_type_of(qc.control)
                        == Some(pyreal_input::spec::DeviceType::Mouse)
                        && qc.control.offset() == offset
                })
            })
            .map(|s| s.input_map_id.0)
            .collect()
    };

    // The left button: two shipped maps bind it, and only one of them is a map this client ever
    // puts in front of itself -- the other belongs to something that does not exist yet, and a
    // map nothing registers can bind whatever it likes and never win. So the interface's own
    // filter cannot be what kept a real press out.
    let left = binders(0x0C);
    let two_binders = left == vec![0x1000_000B, pyreal_testkit::input_steps::UI_INPUT_MAP.0];
    let only_one_registered: Vec<u32> = pyreal_client_shell::input::BASE_MAP_REGISTRATIONS
        .iter()
        .map(|(_, m, _)| *m)
        .filter(|m| left.contains(m))
        .collect();
    let the_interfaces_own =
        only_one_registered == vec![pyreal_testkit::input_steps::UI_INPUT_MAP.0];

    // The wheel: exactly one shipped map binds it, and it is not one the client registers at
    // start-up -- it goes in when something takes the keyboard, which is what the wheel scenarios
    // are about.
    let wheel = binders(0x08);
    let one_binder = wheel == vec![0x0A]
        && !pyreal_client_shell::input::BASE_MAP_REGISTRATIONS
            .iter()
            .any(|(_, m, _)| *m == 0x0A);

    // And what a real press really produces carries that map and that action.
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let mut hands = Hands::new();
    hands.move_to(&mut c, 100, 100);
    let m = hands.button_message(pyreal_input::keys::MouseButton::Left, true);
    hands.send(&mut c, m);
    let (map, action) = {
        let input = c
            .app_mut()
            .input_manager_mut()
            .expect("the input shell is up");
        input.use_time(pyreal_primitives::LocalTime(1.0));
        let events = input.take_events();
        let click = events
            .iter()
            .find(|e| e.start)
            .expect("a press produces one action");
        (click.input_map, click.action.0)
    };
    let the_press_carries_it = map == pyreal_testkit::input_steps::UI_INPUT_MAP && action == 7;

    c.assert_behaviour(
        "pointer.the-left-button-and-the-wheel-each-belong-to-one-shipped-map",
        move |_| two_binders && the_interfaces_own && one_binder && the_press_carries_it,
    );
    c.shutdown();
}
