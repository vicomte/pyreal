//! The input method messages the pump hands to the desktop.

use super::*;
use pyreal_client_contract::window_proc::{msg as win_msg, Effect};

// ---------------------------------------------------------------------------------------------
// text-entry.ime.every-composition-message-is-handed-to-the-desktop
// ---------------------------------------------------------------------------------------------

/// The client writes no input method of its own: it offers every composition message on and lets
/// the desktop drive it.
pub(super) fn every_composition_message_is_handed_to_the_desktop() {
    let mut pump = pyreal_desktop::pump::Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;

    let eight = [
        win_msg::WM_INPUTLANGCHANGE,
        win_msg::WM_IME_STARTCOMPOSITION,
        win_msg::WM_IME_ENDCOMPOSITION,
        win_msg::WM_IME_COMPOSITION,
        win_msg::WM_IME_SETCONTEXT,
        win_msg::WM_IME_NOTIFY,
        win_msg::WM_IME_CONTROL,
        win_msg::WM_IME_COMPOSITIONFULL,
    ];
    let mut all_offered = true;
    for m in eight {
        let r = pump.dispatch(pyreal_input::win32::Win32Message::new(m, 0, 0, 1_000));
        all_offered &= r.effects == vec![Effect::ForwardToBrowser] && !r.handled && r.result == 0;
    }

    // The control: an ordinary typed character takes the other road -- offered on, and then given
    // to the client's own input. Without it the eight above would prove nothing about the arm.
    let r = pump.dispatch(pyreal_input::win32::Win32Message::new(
        win_msg::WM_CHAR,
        u32::from('a') as usize,
        0,
        1_100,
    ));
    let a_character_is_different =
        r.effects == vec![Effect::ForwardToBrowser, Effect::ForwardToInputManager];

    let mut c = HeadlessClient::model();
    c.assert_behaviour("text-entry.ime.every-composition-message-is-handed-to-the-desktop-and-an-ordinary-character-is-not", move |_| {
        all_offered && a_character_is_different
    });
}
