//! Registered console commands. **Optional**, and unbound in the shipped keymaps.
//!
//! Covers command registration, the debug console, and the tab-completion cache the
//! registration invalidates.
//!
//! Unrelated to chat: these are dotted identifiers such as
//! `Ui.SurfaceUsage`, `Camera.AdjustmentSpeed`, `Display.Resolution`. The
//! console is opened by action `0x56` in input map `0xB`, which **has no default binding in either
//! shipped keymap**, so it is unreachable in retail without editing the saved keymap file.
//!
//! Three things are called "command" — console commands, chat commands, and the player's
//! movement commands. Do not conflate them.

/// The action that toggles the debug console. The console treats it as the
/// only action it handles while inactive. Unbound in both shipped keymaps.
pub const TOGGLE_DEBUG_CONSOLE: pyreal_client_contract::actions::ActionId =
    pyreal_client_contract::actions::ActionId(0x56);

/// One registered console name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleCommand {
    /// A dotted identifier.
    pub name: String,
    pub help: String,
}

/// Console command registration and tab completion.
#[derive(Debug, Default)]
pub struct ConsoleRegistry {
    commands: Vec<ConsoleCommand>,
}

impl ConsoleRegistry {
    /// Register a command.
    pub fn register(&mut self, name: &str, help: &str) {
        if self
            .commands
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(name))
        {
            return;
        }
        self.commands.push(ConsoleCommand {
            name: name.to_owned(),
            help: help.to_owned(),
        });
        // The completion cache is invalidated on every registration; keeping the list sorted
        // is the equivalent.
        self.commands.sort_by_key(|c| c.name.to_ascii_lowercase());
    }

    #[must_use]
    pub fn find(&self, name: &str) -> Option<&ConsoleCommand> {
        self.commands
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// Tab completion: every registered name with this prefix, case-insensitively, in name order.
    #[must_use]
    pub fn complete(&self, prefix: &str) -> Vec<&str> {
        let p = prefix.to_ascii_lowercase();
        self.commands
            .iter()
            .filter(|c| c.name.to_ascii_lowercase().starts_with(&p))
            .map(|c| c.name.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered command-interpreter behavior §8 — dotted names and prefix completion.
    /// The console has no default binding, so this is the only exercise it gets.
    #[test]
    fn a_registered_command_is_found_by_prefix() {
        let mut c = ConsoleRegistry::default();
        c.register("Ui.SurfaceUsage", "print UI surface usage");
        c.register("Camera.AdjustmentSpeed", "camera adjustment speed");
        c.register("Camera.Stiffness", "camera stiffness");
        c.register("Display.Resolution", "display resolution");
        c.register("Camera.Stiffness", "duplicate, ignored");

        assert_eq!(
            c.complete("Camera."),
            ["Camera.AdjustmentSpeed", "Camera.Stiffness"]
        );
        assert_eq!(c.complete("camera.st"), ["Camera.Stiffness"]);
        assert!(c.complete("Nope").is_empty());
        assert!(c.find("display.resolution").is_some());
    }
}
