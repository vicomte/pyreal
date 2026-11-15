//! Scripted runs press the same actions as physical input.

use super::{ClientShell, Cx};
use crate::platform::host::Host;

/// The UI's input map, which a window toggle with no key of its own is pressed in.
const UI_INPUT_MAP: pyreal_input::InputMapId = pyreal_input::InputMapId(3);

impl<H: Host> ClientShell<H> {
    /// `--action-at`: each action as its key would give it. This client's own actions go through
    /// their own input map, a window toggle through the map a key of it is bound in, and any other
    /// action to the game, as an unclaimed key does.
    pub(super) fn scripted_world(&mut self, cx: &mut Cx<'_, H>) {
        for action in cx.take_scripted_actions() {
            let window = pyreal_input::names::enum_name_for_action(action).starts_with("Toggle");
            let own = pyreal_input::pyreal::name(action).is_some();
            if let (true, Some(input)) = (own, self.shared.input.as_mut()) {
                // This client's own actions, in their own map, pressed and let go.
                for start in [true, false] {
                    input.inject_action(pyreal_input::InputEvent {
                        action,
                        input_map: pyreal_input::pyreal::INPUT_MAP,
                        toggle: pyreal_input::ToggleType::Hold,
                        extent: 1.0,
                        start,
                        repeat_delta: 0,
                        repeat_total: 0,
                        from_key_down: start,
                    });
                }
            } else if !window {
                cx.inject_action(pyreal_client_runtime::actions::Action::begin(action));
                cx.inject_action(pyreal_client_runtime::actions::Action::end(action));
            } else if let Some(input) = self.shared.input.as_mut() {
                // On the map a key of it is bound in, else the UI's.
                let input_map = input
                    .manager
                    .keymap
                    .sections
                    .iter()
                    .find(|s| !s.keys_for_action(action).is_empty())
                    .map_or(UI_INPUT_MAP, |s| s.input_map_id);
                input.inject_action(pyreal_input::InputEvent {
                    action,
                    input_map,
                    toggle: pyreal_input::ToggleType::OneShot,
                    extent: 1.0,
                    start: true,
                    repeat_delta: 1,
                    repeat_total: 0,
                    from_key_down: false,
                });
            }
        }
    }
}
