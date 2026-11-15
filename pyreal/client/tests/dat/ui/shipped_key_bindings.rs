//! Key bindings reach their actions: the examine key with nothing selected arms the identify
//! cursor and with a selection appraises, Escape puts the cursor away, `V` is not an emote key,
//! and the early recordings carry no emote. Fixture: a headless `App` on the game screen with the
//! retail dats and no terrain or body; keys are resolved from the key map in use and fed as window
//! messages through `Pump`, and the early recordings are read through the shared corpus. Nothing
//! opens a socket: assertions read the client's own state or the request that *would* be sent.
//!
//! # The examine key and the identify pick
//!
//! `SelectionExamine 0x1000002B` reaches `Interaction::on_actions`. The UI examine operation loads
//! the selected id unconditionally and examines that value: with a non-zero id it sends and
//! returns; with zero it examines target mode, keeps that mode active, registers input map
//! `0x1000000B`, and updates the cursor. The zero case must reach the operation, so pressing the
//! examine key with nothing selected arms the magnifying glass.

use pyreal_input::{ActionId, InputMapId};
use pyreal_primitives::ObjectId;
use winit::keyboard::KeyCode;
use {
    pyreal_client::app::App, pyreal_client_runtime::config::Config,
    pyreal_client_runtime::interaction::TargetMode, pyreal_desktop::pump::Pump,
};

/// `UICommands`, where the shipped `ActionMap` declares `SelectionExamine` and `EscapeKey`.
const UI_COMMANDS: u32 = 0x1000_0009;
/// `Emotes`: the map player-input setup registers last.
const EMOTES: u32 = 0x1000_0006;
/// The wave motion command value.
const MOTION_WAVE: u32 = 0x1300_0087;

// ---------------------------------------------------------------------------------------------
// Bring-up
// ---------------------------------------------------------------------------------------------

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        assert!(app.frame());
    }
}

/// The gameplay screen on the real dats. No terrain and no body: the cursor is chosen every frame
/// whether or not there is a world to point at, and the emote half below measures the command the
/// interpreter hands out rather than the animation it becomes.
fn screen_only() -> App {
    let mut app = crate::common::sim_app::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: 800,
        height: 600,
        dat_dir: pyreal_dat::testing::dat_dir(),
        ..Config::default()
    })
    .expect("required retail DATs and a simulated presentation");
    app.start_shell().expect("the UI shell");
    crate::common::sim_app::game_screen(&mut app);
    frames(&mut app, 4);
    app
}

/// **The physical key is resolved from the shipped keymap, never written down.**
///
/// Ask the production `InputManager` which control the shipped master input map binds to this
/// action in this map, take the unmodified one, and find the `winit` key whose scan code is that
/// control's. A test that hard-codes `DIK_J = 0x24` measures its own table; this one fails loudly
/// if the dats ever say something else.
fn key_for(app: &mut App, action: u32, map: u32) -> KeyCode {
    const CANDIDATES: [KeyCode; 16] = [
        KeyCode::KeyE,
        KeyCode::KeyJ,
        KeyCode::KeyB,
        KeyCode::KeyO,
        KeyCode::KeyU,
        KeyCode::KeyI,
        KeyCode::KeyK,
        KeyCode::KeyY,
        KeyCode::KeyH,
        KeyCode::KeyG,
        KeyCode::KeyW,
        KeyCode::KeyS,
        KeyCode::KeyV,
        KeyCode::Escape,
        KeyCode::KeyR,
        KeyCode::KeyF,
    ];
    let binding = app
        .input_manager_mut()
        .expect("the production input manager")
        .keys_for_action(ActionId(action), InputMapId(map))
        .into_iter()
        .find(|b| b.meta_mode == 0)
        .unwrap_or_else(|| {
            panic!(
                "the shipped keymap binds no unmodified control to {action:#010X} in {map:#010X}"
            )
        });
    CANDIDATES
        .into_iter()
        .find(|c| {
            pyreal_desktop::pump::scan_code_from_key_code(*c)
                .is_some_and(|s| u32::from(s & 0x7F) == u32::from(binding.control.offset() & 0x7F))
        })
        .unwrap_or_else(|| {
            panic!(
                "no candidate key carries scan {:#06X}",
                binding.control.offset()
            )
        })
}

/// Construct a `WM_KEYDOWN`/`WM_KEYUP` pair through `Pump`'s production key-message path and feed
/// it to the production input manager, which is why this file can say *"a key press reaches it"*.
fn press(app: &mut App, code: KeyCode, time: u32) {
    let mut pump = Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    for (down, t) in [(true, time), (false, time + 1)] {
        let m = pump.key_message_for(code, down, t).expect("a physical key");
        app.input_manager_mut()
            .expect("the production input manager")
            .on_message(m);
    }
}

/// The current cursor `DataId`, compared against a `UICURSOR` enum key resolved through the
/// shipped mapper rather than by a literal (`pyreal_desktop::cursor`: *"never hard-code a cursor's
/// DataID"*).
fn cursor_did(key: u32) -> pyreal_primitives::DataId {
    let store = pyreal_dat::testing::open_store().expect("required retail DAT");
    pyreal_client_runtime::assets::enum_did(
        &store,
        pyreal_client_shell::cursor::UICURSOR_GROUP,
        key,
    )
    .expect("the shipped UICURSOR mapper")
}

// ---------------------------------------------------------------------------------------------
// 1. The examine key: the identify pick
// ---------------------------------------------------------------------------------------------

/// Nothing selected, press the examine key, and ask what the pointer is.
///
/// The observable is the **cursor**, not the action firing: `Interaction::on_actions` consumes
/// `SelectionExamine` either way. What changes is target mode, which the cursor-state update's
/// fourth arm turns into `Examine` / `Examine_OverObject` (enum `10 + hovering`): the magnifying
/// glass.
#[test]
fn e_with_nothing_selected_arms_the_identify_cursor() {
    let mut app = screen_only();
    let e = key_for(
        &mut app,
        pyreal_client_contract::actions::mapped::SELECTION_EXAMINE.0,
        UI_COMMANDS,
    );

    app.probe_mut().objects_mut().world.set_selected_object(
        None,
        false,
        &mut pyreal_client_model::RecordingSink::default(),
    );
    frames(&mut app, 1);
    assert_eq!(
        app.objects().world.selected,
        None,
        "the state under test is an empty selection"
    );
    assert_eq!(
        app.interaction().target_mode(),
        TargetMode::None,
        "and no mode is armed yet"
    );
    let before = app.current_cursor_did();
    assert_eq!(
        before,
        Some(cursor_did(
            pyreal_client_shell::cursor::cursor_enum::DEFAULT
        )),
        "Default"
    );

    let armed = app.interaction().stats.target_modes_armed;
    press(&mut app, e, 400_000);
    frames(&mut app, 2);

    assert_eq!(
        app.interaction().target_mode(),
        TargetMode::Examine,
        "the zero-id examine arm sets target mode to examine"
    );
    assert_eq!(
        app.interaction().stats.target_modes_armed,
        armed + 1,
        "and it is the arming statement that ran, not a field that happened to be set"
    );
    assert_eq!(
        app.current_cursor_did(),
        Some(cursor_did(
            pyreal_client_shell::cursor::cursor_enum::EXAMINE
        )),
        "the examine target-mode cursor arm is enum 10 + hovering -- the magnifying glass"
    );
    assert_ne!(app.current_cursor_did(), before, "the pointer changed");
    assert_eq!(app.cursor_stats().failures, 0, "and the image resolved");
}

/// **The other half of the same `if`, as a negative control.** With something selected, retail
/// *sends* and arms nothing, so a client that armed the cursor unconditionally would pass the
/// test above and break appraisal.
#[test]
fn e_with_a_selection_appraises_and_arms_nothing() {
    let mut app = screen_only();
    let e = key_for(
        &mut app,
        pyreal_client_contract::actions::mapped::SELECTION_EXAMINE.0,
        UI_COMMANDS,
    );
    let target = ObjectId(0x8000_09A4);
    app.probe_mut().objects_mut().world.set_selected_object(
        Some(target),
        false,
        &mut pyreal_client_model::RecordingSink::default(),
    );
    frames(&mut app, 1);

    let armed = app.interaction().stats.target_modes_armed;
    press(&mut app, e, 400_000);
    for _ in 0..2 {
        assert!(app.frame());
    }

    assert_eq!(
        app.interaction().target_mode(),
        TargetMode::None,
        "a nonzero selected id raises the examine notice and returns before the zero-id target-mode arm"
    );
    assert_eq!(
        app.interaction().stats.target_modes_armed,
        armed,
        "nothing was armed"
    );
    assert_eq!(
        app.objects().world.appraisal.examining,
        Some(target),
        "the non-zero arm examines the selected id and records what it asked about"
    );
    // No identify cursor: the pointer is the hourglass, because the examine now waits on the
    // shard's description, and nothing here answers it.
    assert_eq!(
        app.current_cursor_did(),
        Some(cursor_did(pyreal_client_shell::cursor::cursor_enum::WAIT)),
        "and the pointer waits on the description rather than arming anything"
    );
}

/// **What clears it.** The UI action handler's `EscapeKey` arm sets target mode to none, and the
/// cursor goes back with it. A mode with no exit is a stuck pointer.
#[test]
fn escape_puts_the_identify_cursor_away() {
    let mut app = screen_only();
    let e = key_for(
        &mut app,
        pyreal_client_contract::actions::mapped::SELECTION_EXAMINE.0,
        UI_COMMANDS,
    );
    let esc = key_for(
        &mut app,
        pyreal_client_contract::actions::mapped::ESCAPE_KEY.0,
        UI_COMMANDS,
    );
    assert_eq!(esc, KeyCode::Escape);

    app.probe_mut().objects_mut().world.set_selected_object(
        None,
        false,
        &mut pyreal_client_model::RecordingSink::default(),
    );
    frames(&mut app, 1);
    press(&mut app, e, 400_000);
    frames(&mut app, 2);
    assert_eq!(
        app.interaction().target_mode(),
        TargetMode::Examine,
        "armed, so there is something to clear"
    );

    press(&mut app, esc, 500_000);
    frames(&mut app, 2);
    assert_eq!(
        app.interaction().target_mode(),
        TargetMode::None,
        "clearing target mode"
    );
    assert_eq!(
        app.current_cursor_did(),
        Some(cursor_did(
            pyreal_client_shell::cursor::cursor_enum::DEFAULT
        )),
        "and the no-target-mode cursor arm puts the default pointer back"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. `V`
// ---------------------------------------------------------------------------------------------

/// **`V` is not an emote key.** The only shipped input-map binding for `V` is
/// `Ctrl+V -> PasteText`, and the emote map binds no `V` at all.
#[test]
fn v_is_not_bound_to_any_emote_in_the_shipped_keymaps() {
    let mut app = screen_only();
    let scan = u32::from(
        pyreal_desktop::pump::scan_code_from_key_code(KeyCode::KeyV).expect("V has a scan code")
            & 0x7F,
    );
    let shell = app
        .input_manager_mut()
        .expect("the production input manager");
    let in_emotes = shell
        .manager
        .keymap
        .section(InputMapId(EMOTES))
        .expect("the shipped Emotes keymap section")
        .bindings()
        .iter()
        .any(|(qc, _)| u32::from(qc.control.offset() & 0x7F) == scan);
    assert!(!in_emotes, "no Emotes binding uses V");
    // ...and it is bound to something, so this is a measurement rather than a keymap that failed
    // to load: `CopyAndPasteControls 8` carries `Ctrl+V -> PasteText 0x24`.
    let paste = shell.keys_for_action(ActionId(0x24), InputMapId(8));
    assert!(
        paste
            .iter()
            .any(|qc| u32::from(qc.control.offset() & 0x7F) == scan && qc.meta_mode != 0),
        "V is bound, with a modifier, to PasteText -- got {paste:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The recordings carry no emote
// ---------------------------------------------------------------------------------------------

/// **Whether the recordings carry any emote.**
///
/// They do not, and the answer decides what the corpus can and cannot be used for here. Across
/// the seven sessions named below, every `0xF61C Movement_MoveToState` body the client sends
/// carries no queued action at all: the five-bit action count in the raw motion state's flags
/// word is zero in every one. Nobody emoted while any of these sessions was being recorded. (The
/// later `requested-death-vitae-salvage` recording does queue one action, which is why the claim
/// is made over this fixed selection and not over every recording.)
///
/// So the corpus is a byte-level oracle for the *movement* half of `RawMotionState` and is
/// **silent** on the emote half. The non-empty columns below are what tell "we do not send
/// emotes" and "nobody waved" apart.
///
/// The instrument is proved able to see a non-empty answer in the same selection: the bodies
/// carry forward commands and run hold keys, so a zero in the action column is the corpus
/// speaking and not the decoder failing.
#[test]
fn every_early_recorded_move_to_state_queues_no_action_so_the_wire_half_is_local() {
    use pyreal_client_net::client_session::testing::{Corpus, Direction};
    use pyreal_protocol::actions::unpack_action;
    use pyreal_protocol::movement::MovementMoveToState;
    use pyreal_protocol::Message;

    const SCENARIOS: [&str; 7] = [
        "first-login-walk-jump",
        "early-inventory-and-casting",
        "short-second-connection",
        "login-account-booted",
        "ddd-interrogation-only",
        "long-solo-play",
        "short-play-with-training",
    ];
    const GAME_ACTION: u32 = 0xF7B1;
    const MOVE_TO_STATE: u32 = 0xF61C;

    let (mut bodies, mut with_actions, mut with_forward, mut with_hold_run) = (0, 0, 0, 0);
    let mut emotes: Vec<u32> = Vec::new();
    for name in SCENARIOS {
        let corpus = Corpus::shared(name);
        for b in corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ClientToServer && b.opcode == GAME_ACTION)
        {
            let mut action = unpack_action(&b.payload).expect("a captured game action unpacks");
            if action.sub_type.0 != MOVE_TO_STATE {
                continue;
            }
            let m = MovementMoveToState::read(&mut action.body).expect("a captured body decodes");
            let s = &m.0.raw_motion_state;
            bodies += 1;
            if !s.actions.is_empty() {
                with_actions += 1;
            }
            if s.forward_command.is_some() {
                with_forward += 1;
            }
            if s.current_holdkey == Some(2) {
                with_hold_run += 1;
            }
            for a in &s.actions {
                if let Some(c) = pyreal_animation::MotionCommand::from_index(a.command_index) {
                    if c.is_emote() {
                        emotes.push(c.0);
                    }
                }
            }
        }
    }

    // Calibration first: the decoder can see a non-empty answer over exactly this population.
    assert!(bodies > 0, "the seven sessions carry 0xF61C bodies");
    assert!(
        with_forward > 0,
        "some recorded 0xF61C carries a forward command"
    );
    assert!(
        with_hold_run > 0,
        "some recorded 0xF61C carries the run hold key"
    );

    // **The positive control for the column that reads zero, which is the only one that matters.**
    // The three calibrations above prove the decoder reads *some* fields; they do not prove it
    // reads the **action queue**, and a reader that ignored the five-bit count would answer zero
    // for a corpus full of emotes. So one is round-tripped through the same `read`: a state whose
    // flags word carries a count of 1 and nothing else, followed by the wave motion's wire index.
    {
        use pyreal_protocol::movement::{MotionAction, RawMotionState};
        use pyreal_protocol::{Reader, Writer};
        let wave = pyreal_animation::MotionCommand(MOTION_WAVE)
            .to_index()
            .expect("Wave has a wire index");
        let built = RawMotionState {
            actions: vec![MotionAction {
                command_index: wave,
                stamp_and_autonomy: 0x8001,
                speed: 1.0,
            }],
            ..RawMotionState::default()
        };
        let mut w = Writer::new();
        built.write(&mut w).expect("one action encodes");
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        let back = RawMotionState::read(&mut r).expect("and decodes");
        assert_eq!(
            back.actions.len(),
            1,
            "the reader must be able to answer non-zero at all"
        );
        assert_eq!(back.actions[0].command_index, wave);
        assert!(
            pyreal_animation::MotionCommand::from_index(back.actions[0].command_index)
                .is_some_and(|c| c.0 == MOTION_WAVE),
            "and the index resolves back to wave motion, which is how the loop above would have \
             recognised one"
        );
    }

    // ...and then the answer.
    assert_eq!(
        with_actions, 0,
        "no recorded 0xF61C queues an action of any kind"
    );
    assert!(
        emotes.is_empty(),
        "and therefore none of the 87 emotes: {emotes:08X?}"
    );
}
