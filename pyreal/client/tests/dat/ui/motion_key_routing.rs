//! Motion keys through the key map: the five other motion slots reach the body, and an action the
//! body does not model is put back. Fixture: a headless `App` on the game screen on the
//! retail dats, with keyboard messages sent through `Pump` and `InputShell`; nothing links to a
//! shard, and every fixture path is an `assert!`/`expect`.
//!
//! A tap can press and release within one frame; holding across frames avoids that cancellation.
//! `app.probe().actions_routed` supplies an independent count that the movement path was
//! exercised.

use crate::common::sim_app::app_in_gameplay;

use pyreal_client::app::App;
use winit::keyboard::KeyCode;
use {pyreal_desktop::pump::Pump, pyreal_input::win32::Win32Message};

/// Pump/InputShell keyboard messages, with key holds across frames. No
/// operating-system window procedure is called.
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
            time_ms: 200_000,
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

    /// **Hold** `code` down across `frames` frames, then release it, and answer whether the motion
    /// slot named by `read` was ever set while it was held.
    ///
    /// This is the shape a tap cannot substitute for: the press
    /// and the release are separate messages with frames between them, so a build in which the
    /// down and the up cancel inside one frame still reports the truth.
    fn hold(
        &mut self,
        app: &mut App,
        code: KeyCode,
        frames: u32,
        read: fn(pyreal_client_runtime::character::CharacterInput) -> bool,
    ) -> bool {
        self.key(app, code, true);
        let mut ever = false;
        for _ in 0..frames {
            app.frame();
            ever |= read(app.probe().char_input());
        }
        self.key(app, code, false);
        app.frame();
        ever
    }
}

/// The other five motion slots reach the body through the same seam as W, from the key map's
/// bindings: S->0x2A back, Q->0x2D strafe-left, E->0x2C strafe-right, A->0x2F turn-left and
/// D->0x2E turn-right. X->0x2B stops movement by entering Ready and releasing directional slots.
#[test]
fn the_key_map_drives_the_five_other_motion_slots() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    type Case = (
        KeyCode,
        &'static str,
        fn(pyreal_client_runtime::character::CharacterInput) -> bool,
    );
    let cases: [Case; 5] = [
        (KeyCode::KeyS, "DIK_S -> 0x2A Move Backward", |c| c.back),
        (KeyCode::KeyQ, "DIK_Q -> 0x2D Strafe Left", |c| c.step_left),
        (KeyCode::KeyE, "DIK_E -> 0x2C Strafe Right", |c| {
            c.step_right
        }),
        (KeyCode::KeyA, "DIK_A -> 0x2F Turn Left", |c| c.turn_left),
        (KeyCode::KeyD, "DIK_D -> 0x2E Turn Right", |c| c.turn_right),
    ];
    for (code, what, read) in cases {
        assert!(
            hand.hold(&mut app, code, 2, read),
            "{what} must reach the body"
        );
    }

    // Stop Moving enters Ready and clears directional slots; it is not another direction.
    hand.key(&mut app, KeyCode::KeyS, true);
    app.frame();
    assert!(app.probe().char_input().back, "S is held");
    hand.key(&mut app, KeyCode::KeyX, true);
    app.frame();
    assert!(
        !app.probe().char_input().back,
        "DIK_X is 0x2B Stop Moving -- the Ready state, all slots out"
    );
    hand.key(&mut app, KeyCode::KeyX, false);
    hand.key(&mut app, KeyCode::KeyS, false);

    app.shutdown();
}

/// The action-listener walk continues when a handler declines; movement decoding returns
/// MovementAction::NotHandled outside its movement/emote cases. Backquote's shipped action
/// 0x1000005A in map 0x10000002 is combat toggle, handled beyond the movement/camera seam.
/// After a W routing positive, this test requires that action not increment app.probe().actions_routed.
/// It checks non-consumption by that seam, not the downstream combat-state change itself.
#[test]
fn an_action_the_body_does_not_model_is_put_back() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    // The denominator: the movement seam is alive in this app at all.
    let before = app.probe().actions_routed();
    hand.key(&mut app, KeyCode::KeyW, true);
    app.frame();
    hand.key(&mut app, KeyCode::KeyW, false);
    app.frame();
    assert!(
        app.probe().actions_routed() > before,
        "the movement seam is live in this application"
    );

    // Backquote is bound to the combat toggle, so the key below reaches a handler at all.
    let toggle = app
        .input_manager()
        .expect("the input shell exists in a UI build")
        .keys_for_action(
            pyreal_input::ActionId(0x1000_005A),
            pyreal_input::InputMapId(0x1000_0002),
        );
    assert!(
        toggle
            .iter()
            .any(|k| k.meta_mode == 0 && k.control.offset() & 0x7F == 0x29),
        "Backquote is the combat toggle: {toggle:?}"
    );
    let before = app.probe().actions_routed();
    hand.key(&mut app, KeyCode::Backquote, true);
    app.frame();
    hand.key(&mut app, KeyCode::Backquote, false);
    app.frame();
    assert_eq!(
        app.probe().actions_routed(),
        before,
        "the combat-mode toggle is not a motion command and must reach the combat handler untouched"
    );

    app.shutdown();
}
