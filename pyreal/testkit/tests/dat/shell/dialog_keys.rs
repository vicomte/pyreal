//! The shipped dialog key map.

use super::*;
use pyreal_input::fire::{walk_input_maps, InputMapEntry};
use pyreal_input::spec::{activation, ControlChord, DeviceType};
use pyreal_input::{ActionId, InputMapId};

// =============================================================================================
// dialog-keys.* -- the two keys the shipped dialog map binds
// =============================================================================================

/// The two actions the dialog map declares: answer, and back out.
const ACCEPT_INPUT: u32 = 0x25;
const ESCAPE_KEY: u32 = 0x27;

/// The live map stack, in the order the client walks it.
fn map_stack(c: &mut HeadlessClient) -> Vec<InputMapEntry> {
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell is part of the UI shell")
        .manager
        .maps
        .entries()
        .to_vec()
}

/// Which map and action the key **the shipped dialog map binds to `want`** resolves to, against
/// the stack as it stands.
///
/// The control comes out of the shipped keymap rather than out of a number here, so this is
/// exactly the question "does the dialog map still win the key it declares". It drives no frame,
/// so it can be asked before and after a gesture.
fn dialog_key_resolves_to(c: &mut HeadlessClient, want: u32) -> Option<(InputMapId, ActionId)> {
    let entries = map_stack(c);
    let shell = c.app_mut().input_manager_mut().expect("an input shell");
    let km = &shell.manager.keymap;
    let section = km
        .section(pyreal_input::MAP_DIALOG_BOXES)
        .expect("the dialog section");
    let (qc, _) = section
        .bindings()
        .iter()
        .find(|(_, a)| a.0 == want)
        .copied()
        .unwrap_or_else(|| panic!("the dialog map binds action {want:#X}"));
    // Presented the way a live press is: the release bit cleared and the live bit set.
    let live = ControlChord::new(
        qc.control,
        qc.meta_mode,
        (qc.activation & !activation::UP) | activation::LIVE,
    );
    let is_keyboard = km.device_type_of(qc.control) == Some(DeviceType::Keyboard);
    assert!(
        is_keyboard,
        "both of the dialog map's controls are keyboard controls"
    );
    walk_input_maps(&entries, &live, is_keyboard, |m| km.section(m))
        .map(|r| (r.input_map, r.action))
}

// ---------------------------------------------------------------------------------------------
// keymap.dialog-keys.the-shipped-map-binds-only-escape-and-enter-and-each-once
// ---------------------------------------------------------------------------------------------

/// The map the pre-game screens listen on carries two keys, and no more.
///
pub(super) fn the_dialog_map_binds_only_escape_and_enter() {
    use std::collections::BTreeMap;

    let mut c = HeadlessClient::new(ClientSpec::retail());
    // The shell, and with it the shipped keymap, without any screen on top of it.
    c.app_mut().start_shell().expect("the UI shell comes up");
    let shell = c.app_mut().input_manager_mut().expect("an input shell");
    let section = shell
        .manager
        .keymap
        .section(pyreal_input::MAP_DIALOG_BOXES)
        .expect("the dialog section");
    let mut by_action: BTreeMap<u32, usize> = BTreeMap::new();
    for (_, action) in section.bindings() {
        *by_action.entry(action.0).or_default() += 1;
    }
    let two = by_action.keys().copied().collect::<Vec<u32>>() == vec![ACCEPT_INPUT, ESCAPE_KEY];
    let each_once = by_action.values().all(|n| *n == 1);
    c.shutdown();

    // **The negative control**: in the world, where no screen before it has claimed this map,
    // neither of its two keys resolves in it -- they go to the in-game maps that used to swallow
    // them. Without this, "the pre-game screens win these keys" and "this map always wins them"
    // read alike.
    let (mut w, _shard) = super::relog::a_client_in_the_world(ClientSpec::gameplay(4));
    let not_in_the_world = dialog_key_resolves_to(&mut w, ACCEPT_INPUT)
        .is_none_or(|(m, _)| m != InputMapId(9))
        && dialog_key_resolves_to(&mut w, ESCAPE_KEY).is_none_or(|(m, _)| m != InputMapId(9));
    // ...and they still reach something, rather than nowhere: a walk that answered nothing at all
    // would satisfy the reading above just as well.
    let they_still_arrive = dialog_key_resolves_to(&mut w, ACCEPT_INPUT).is_some()
        && dialog_key_resolves_to(&mut w, ESCAPE_KEY).is_some();

    w.assert_behaviour(
        "keymap.dialog-keys.the-shipped-map-binds-only-escape-and-enter-and-each-once",
        move |_| two && each_once && not_in_the_world && they_still_arrive,
    );
    w.shutdown();
}
