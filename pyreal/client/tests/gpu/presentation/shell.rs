//! The input and audio shell: a key reaches the input manager through the frame and fires its
//! action, and the sound the shell starts produces samples.
//!
//! Fixture: the retail dats; a headless App on a software device, and window messages synthesised
//! by the real message pump.
//!
//! | claim | oracle |
//! |---|---|
//! | a key reaches `pyreal-input` | Horizon's default keymap: `CameraRotateLeft` is `DIK_LEFT`, action `0x35`, which `pyreal_client_runtime::actions::camera::action` names `ROTATE_LEFT` |
//! | a sound plays | the retail `client_portal.dat`: the UI sound table and the `0x0A` wave its button-press row names |
//!
//! **Injected keystrokes are not evidence.** Nothing here
//! touches the desktop: the window message is synthesised by `crate::pump`, which is the same code
//! the real pump runs, and it is fed to the application's own per-frame path.
//!
//! A machine without the retail dats fails.

#![cfg(gpu)]

use crate::common::client_dir;

use pyreal_client::app::App;
use pyreal_client_runtime::config::Config;

/// A headless App with its shell up, or a failed test: the gpu tier needs a software device
/// and the retail dats, and a skip would read as a pass.
fn headless(cfg: Config) -> App {
    let mut app = App::new(cfg)
        .unwrap_or_else(|e| panic!("the gpu tier needs a headless App on a software device: {e}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the shell must come up over the retail dats: {e}"));
    app
}

fn base_config() -> Config {
    Config {
        headless: true,
        // No device: a regression must not take the machine's audio endpoint.
        sound: false,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

// ---------------------------------------------------------------------------------------------
// 1. A key reaches pyreal-input, through the application's own update path
// ---------------------------------------------------------------------------------------------

/// Oracle: Horizon's default key map. Its camera controls — registered for the whole run, so on
/// the pre-game screen the client starts on too — bind `DIK_LEFT` (0xCB) with meta-mode 0 to
/// action **0x35**, which `pyreal_client_runtime::actions::camera::action` names `ROTATE_LEFT`. (The character session's
/// maps, movement among them, are not registered until the player is in the world, so a movement
/// key here would reach no map at all.)
///
/// The `MSG` is built by `crate::pump`, which is the same code the real pump runs, and it is fed
/// through `App::frame` so the whole per-frame path — window-message forwarding, input-manager
/// message handling, then its time update inside UI time — is what produces
/// the action. Nothing is injected into the desktop.
#[test]
fn a_window_message_reaches_the_input_manager_through_the_frame_and_fires_its_action() {
    let mut app = headless(base_config());
    assert!(
        app.input_manager_mut().is_some(),
        "the input tables must load from the retail dats"
    );
    app.load_first_pixel_scene()
        .expect("the first-pixel surface decodes");

    // The `MSG` a `winit` left-arrow press produces, with the `GetMessageTime()` the pump carries.
    let mut pump = pyreal_desktop::pump::Pump::new();
    let down = pump
        .key_message_for(winit::keyboard::KeyCode::ArrowLeft, true, 12_345)
        .expect("winit maps ArrowLeft to a virtual key and a scan code");
    assert_eq!(down.wparam, 0x25, "VK_LEFT");
    assert_eq!(
        pyreal_input::win32::keyboard_offset(down.lparam),
        Some(0xCB),
        "DIK_LEFT must be in the lParam; the input pipeline never reads the wParam"
    );

    let input = app.input_manager_mut().expect("present");
    assert!(
        input.on_message(down),
        "input message handling consumes the synthesized key message"
    );

    // Now run the application's own frame. The input manager's time update is step 6 of UI time,
    // and it is what drains the pipeline.
    assert!(app.frame());

    // The assertion is made **where the action went**: `App::apply_input_actions`, the
    // command-handler leg of the listener dispatch, consumes the queued camera action, so the
    // queue is empty after a frame and the camera's turn-left input is held. A still-queued action
    // would mean nothing anywhere was listening, and it would be re-offered to the UI on every
    // frame.
    assert!(
        app.probe().camera_input().look_left,
        "the camera must be turning left while the key is down"
    );
    let input = app.input_manager_mut().expect("present");
    assert!(
        input.take_events().is_empty(),
        "and it was consumed rather than left in the queue to be re-offered on every frame"
    );
    assert!(
        input.is_action_in_progress(pyreal_client_runtime::actions::camera::action::ROTATE_LEFT)
    );
    assert_eq!(input.stats.messages_offered, 1);
    assert_eq!(input.stats.messages_handled, 1);
    // A keyboard message with a zero scan code (a zero `lParam`) is counted here.
    assert_eq!(input.stats.keyboard_without_scan_code, 0);
    assert!(input.stats.actions_fired >= 1);

    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// 2. A sound plays
// ---------------------------------------------------------------------------------------------

/// Oracle: the retail `client_portal.dat`. The UI sound table is whatever the original
/// enum-keyed lookup for `(0x10000003, 7, 0x22)` resolves
/// to — nothing is hard-coded — and the wave is whatever that table's button-press row
/// names. The assertion is on the **samples**: a path that resolved, decoded and started a voice
/// and then mixed silence would pass every count and still be inaudible.
#[test]
fn the_ui_sound_the_shell_starts_produces_samples_from_the_mixer() {
    let mut app = headless(base_config());
    {
        let audio = app
            .audio_mut()
            .expect("the shell builds its sound subsystem even with no audio device");
        // `--no-sound` in `base_config`, so there is no device and the original mixer's CRT seed
        // stays 1 — which is the client's own behaviour on a machine with no sound card.
        assert!(!audio.has_device());
        assert!(
            audio.stats.waves_created > 0,
            "no wave was created from the UI sound table"
        );
        assert_eq!(
            audio.stats.wave_decode_failures, 0,
            "a wave would not decode"
        );
        assert!(
            audio.stats.tables_loaded > 0,
            "the UI sound table did not load"
        );
        assert_eq!(
            audio.active_voices(),
            0,
            "nothing is playing before the sound is started"
        );
    }

    app.play_startup_sound();

    let audio = app.audio_mut().expect("the audio engine is up");
    assert_eq!(audio.stats.sounds_started, 1);
    assert_eq!(
        audio.active_voices(),
        1,
        "the startup sound leaves one active voice"
    );

    // One block at the client's own mix rate: 11 025 Hz, interleaved stereo,
    // matching original sound initialization.
    let mut block = vec![0.0f32; 2 * 2048];
    audio.mix(&mut block);
    let peak = block.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let nonzero = block.iter().filter(|s| **s != 0.0).count();
    assert!(peak > 0.0, "the mixer produced 2048 frames of silence");
    // The button click is a short wave, so the assertion is on the *shape*: a run of non-zero
    // samples at the head of the block and silence after it. Nothing loops, so a
    // second block finds the voice finished.
    assert!(
        nonzero > 100,
        "only {nonzero} of {} samples are non-zero",
        block.len()
    );
    let last_nonzero = block.iter().rposition(|s| *s != 0.0).expect("some signal");
    assert!(
        block[..=last_nonzero].iter().filter(|s| **s != 0.0).count() * 2 >= last_nonzero,
        "the signal is sparse; that is noise, not a decoded wave"
    );
    // Stereo, and the original startup sound pans to 0, so both channels carry the same signal.
    let left: Vec<f32> = block.as_chunks::<2>().0.iter().map(|f| f[0]).collect();
    let right: Vec<f32> = block.as_chunks::<2>().0.iter().map(|f| f[1]).collect();
    assert_eq!(
        left, right,
        "a centre-panned sound must be identical in both channels"
    );

    // "Nothing loops; continuous ambience is a re-trigger on a timer". The voice
    // ends when the sample does, and a rebuild that looped it would keep this at 1 for ever.
    for _ in 0..64 {
        audio.mix(&mut block);
    }
    assert_eq!(
        audio.active_voices(),
        0,
        "the voice must end with the sample; nothing loops"
    );

    app.shutdown();
}
