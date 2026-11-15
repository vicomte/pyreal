//! The movement keys through the interface's key map: the three command lists
//! (`pyreal_client_runtime::actions::movement::CommandLists`) keep held commands, so releasing a
//! second key resumes the first; `R` (0x30 Autorun) toggles the run lock and any movement key
//! cancels it. Assertions read command lists and input flags, not displacement. Fixture: a
//! headless `App` on the game screen on the retail dats, fed Windows-style messages through the
//! pump; absent dats fail.

use crate::common::sim_app::app_in_gameplay;

use pyreal_client::app::App;

use {pyreal_desktop::pump::Pump, pyreal_input::win32::Win32Message};

use winit::keyboard::KeyCode;

/// Feed Windows-style keyboard and pointer messages through the pump and input manager.
struct Hand {
    pump: Pump,
    time_ms: u32,
}

impl Hand {
    fn new() -> Self {
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time_ms: 300_000,
        }
    }

    fn send(&mut self, app: &mut App, m: Win32Message) {
        self.pump.dispatch(m);
        if let Some(input) = app.input_manager_mut() {
            input.on_message(m);
        }
    }

    fn key(&mut self, app: &mut App, code: KeyCode, down: bool) {
        self.time_ms += 10;
        let m = self
            .pump
            .key_message_for(code, down, self.time_ms)
            .expect("winit maps this key to a virtual key and a scan code");
        self.send(app, m);
    }

    /// Press and release with a frame after each edge. Assertions after this helper observe
    /// both edges; down()/up() below allow inspection while a key remains held.
    fn tap(&mut self, app: &mut App, code: KeyCode) {
        self.key(app, code, true);
        app.frame();
        self.key(app, code, false);
        app.frame();
    }

    /// Press `code` and leave it down, one frame.
    fn down(&mut self, app: &mut App, code: KeyCode) {
        self.key(app, code, true);
        app.frame();
    }

    /// Release `code`, one frame.
    fn up(&mut self, app: &mut App, code: KeyCode) {
        self.key(app, code, false);
        app.frame();
    }
}

// -------------------------------------------------------------------------------------------
// 1. The three command lists, driven through the key map
// -------------------------------------------------------------------------------------------

/// Hold A, press D, release D: the left-turn command must resume.
/// Command removal reissues the remaining head as a start, so releasing the second
/// key becomes a renewed press of the first. A single current-command variable cannot retain
/// that history. The final release must also stop turning, rejecting a never-release mutation.
///
/// The shipped default key map binds DIK_A to 0x2F Turn Left and DIK_D to 0x2E Turn Right in map 4.
/// This test checks list order and turn flags through those bindings, not body displacement.
#[test]
fn releasing_the_second_turn_key_resumes_the_first() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    let turn = |app: &App| {
        (
            app.probe().char_input().turn_left,
            app.probe().char_input().turn_right,
        )
    };
    let list = |app: &App| -> Vec<u32> {
        app.probe()
            .movement_commands()
            .lists
            .turn
            .iter()
            .map(|e| e.command)
            .collect()
    };

    // The denominator: something reached the interpreter at all.
    let before = app.probe().movement_commands().commands_handled;

    hand.down(&mut app, KeyCode::KeyA);
    assert_eq!(turn(&app), (true, false), "DIK_A -> 0x2F Turn Left");
    assert_eq!(
        list(&app),
        vec![0x6500_000E],
        "TurnLeft is the head of the turn list"
    );

    hand.down(&mut app, KeyCode::KeyD);
    assert_eq!(turn(&app), (false, true), "the second key wins");
    assert_eq!(
        list(&app),
        vec![0x6500_000D, 0x6500_000E],
        "and the first is still on the list underneath it -- which is the whole mechanism"
    );

    hand.up(&mut app, KeyCode::KeyD);
    assert_eq!(
        turn(&app),
        (true, false),
        "releasing the second key resumes the first: this is what one bool per direction cannot do"
    );
    assert_eq!(list(&app), vec![0x6500_000E]);

    hand.up(&mut app, KeyCode::KeyA);
    assert_eq!(
        turn(&app),
        (false, false),
        "and releasing the first really does stop the turn"
    );
    assert!(list(&app).is_empty());

    assert!(
        app.probe().movement_commands().commands_handled >= before + 4,
        "four presses and releases must all have reached the interpreter: {} -> {}",
        before,
        app.probe().movement_commands().commands_handled
    );
    app.shutdown();
}

/// Repeat held-command resumption on the substate list: hold W, press S, then release S.
/// DIK_W maps 0x29 Move Forward to WalkForward 0x45000005; DIK_S maps 0x2A Move Backward to
/// WalkBackwards 0x45000006. Both command values carry 0x40000000 and 0x04000000, selecting the
/// substate list. Releasing the last key must clear both movement flags and the list.
#[test]
fn releasing_the_second_movement_key_resumes_the_first() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    hand.down(&mut app, KeyCode::KeyW);
    assert!(app.probe().char_input().forward);
    hand.down(&mut app, KeyCode::KeyS);
    assert!(
        app.probe().char_input().back && !app.probe().char_input().forward,
        "the second key wins"
    );
    hand.up(&mut app, KeyCode::KeyS);
    assert!(
        app.probe().char_input().forward && !app.probe().char_input().back,
        "releasing backward resumes forward"
    );
    hand.up(&mut app, KeyCode::KeyW);
    assert!(
        !app.probe().char_input().forward && !app.probe().char_input().back,
        "and the last release stops"
    );
    assert!(app.probe().movement_commands().lists.substate.is_empty());
    app.shutdown();
}

// -------------------------------------------------------------------------------------------
// 2. The run lock
// -------------------------------------------------------------------------------------------

/// DIK_R toggles autorun; a subsequent forward command cancels it without losing that command.
/// `CharacterInput`'s autorun view reflects the command-list latch rather than duplicating it.
///
/// The toggle reapplies movement on both transitions, which the off-direction assertion checks.
/// With autorun set, the base command is WalkForward even with an empty substate list. The
/// second R must clear both autorun and forward input. Turning it on again and then pressing W
/// must cancel autorun but retain W's WalkForward command; releasing W must finally stop it.
///
/// Input activation cancels the run lock on starts of 0x29, 0x2A or 0x2B by releasing
/// action 0x30. This test samples W (0x29); it does not prove every movement key cancels autorun,
/// and turning while autorunning is a separate steering case. All movement readings are flags.
#[test]
fn the_run_lock_starts_on_dik_r_and_any_movement_key_cancels_it() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    assert!(
        !app.probe().char_input().auto_run,
        "the run lock starts off"
    );
    assert!(!app.probe().char_input().forward);

    // 1. On.
    hand.tap(&mut app, KeyCode::KeyR);
    assert!(app.probe().char_input().auto_run, "DIK_R -> 0x30 Autorun");
    assert!(
        app.probe().char_input().forward,
        "autorun supplies WalkForward in the base slot while it is set"
    );
    assert!(
        app.probe().movement_commands().lists.substate.is_empty(),
        "with nothing on the substate list"
    );

    // 2. Off, and it stops.
    hand.tap(&mut app, KeyCode::KeyR);
    assert!(!app.probe().char_input().auto_run, "the toggle alternates");
    assert!(
        !app.probe().char_input().forward,
        "turning autorun off also clears forward input -- movement is reapplied in both directions"
    );

    // 3. On again, then a movement key.
    hand.tap(&mut app, KeyCode::KeyR);
    assert!(app.probe().char_input().auto_run);
    hand.down(&mut app, KeyCode::KeyW);
    assert!(
        !app.probe().char_input().auto_run,
        "any movement key cancels the run lock"
    );
    assert!(
        app.probe().char_input().forward,
        "and the character is still walking, on W's own WalkForward"
    );
    assert_eq!(
        app.probe()
            .movement_commands()
            .lists
            .substate
            .iter()
            .map(|e| e.command)
            .collect::<Vec<_>>(),
        vec![0x4500_0005]
    );
    hand.up(&mut app, KeyCode::KeyW);
    assert!(!app.probe().char_input().forward);
    app.shutdown();
}
