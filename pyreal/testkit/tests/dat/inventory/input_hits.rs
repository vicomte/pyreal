use super::*;

// ---------------------------------------------------------------------------------------------
// inventory.drag-hint.no-cursor-the-client-ships-is-chosen-by-a-drag
// ---------------------------------------------------------------------------------------------

/// The mouse pointer does not change while an icon is being carried, and it **cannot**: the
/// thing that picks a pointer picture is not given the drag at all.
///
/// Every combination of that chooser's five inputs is driven -- busy and not, all four aiming
/// modes, all five fighting stances, hovering and not, over something usable and not -- and the
/// set of pictures that come back is the denominator this claim needs. None of the shipped
/// pictures is named for a drag, a drop or a refusal either, so there is no picture the client
/// could have picked even if something asked for one.
pub(super) fn no_cursor_the_client_ships_is_chosen_by_a_drag() {
    use pyreal_assets::Decode;
    use pyreal_client_model::combat::CombatMode;
    use pyreal_primitives::AssetSource;
    use {
        pyreal_client_shell::cursor::cursor_enum, pyreal_client_shell::cursor::CursorInputs,
        pyreal_client_shell::cursor::TargetMode, pyreal_client_shell::cursor::UICURSOR_GROUP,
    };

    let c = HeadlessClient::new(ClientSpec::retail());

    // The shipped table of pointer pictures, read out of the retail data rather than written
    // here: a list written here would agree with itself whatever the client ships.
    let names: Vec<(u32, String)> = {
        let store = c.view().dat_store().clone();
        let source: &dyn AssetSource = store.as_ref();
        let master = pyreal_assets::DidMapper::decode_payload(
            pyreal_client_runtime::assets::MASTER_DID_MAPPER,
            &source
                .read(pyreal_client_runtime::assets::MASTER_DID_MAPPER)
                .expect("the master table reads"),
        )
        .expect("the master table decodes");
        let which = master
            .enum_to_id
            .iter()
            .find(|(k, _)| *k == UICURSOR_GROUP)
            .map(|(_, v)| pyreal_primitives::DataId(*v))
            .expect("the master table names the pointer group");
        let m = pyreal_assets::DidMapper::decode_payload(
            which,
            &source.read(which).expect("the pointer table reads"),
        )
        .expect("the pointer table decodes");
        let mut v = m.enum_to_name.clone();
        v.sort_by_key(|(k, _)| *k);
        v
    };
    assert_eq!(
        names.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        (1..=cursor_enum::LAST).collect::<Vec<_>>(),
        "the premise: the shipped table is numbered straight through with no gaps"
    );

    let mut reachable: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut runs = 0;
    for busy in [0_u32, 1] {
        for aiming in [
            TargetMode::None,
            TargetMode::Use,
            TargetMode::Examine,
            TargetMode::UseTarget,
        ] {
            for stance in [
                CombatMode::Undef,
                CombatMode::NonCombat,
                CombatMode::Melee,
                CombatMode::Missile,
                CombatMode::Magic,
            ] {
                for hovering in [false, true] {
                    for usable in [false, true] {
                        let picked =
                            pyreal_client_shell::cursor::update_cursor_state(CursorInputs {
                                busy,
                                target_mode: aiming,
                                combat_mode: stance,
                                hovering,
                                target_compatible: usable,
                            });
                        reachable.insert(picked.enum_value);
                        runs += 1;
                    }
                }
            }
        }
    }
    assert_eq!(
        runs, 160,
        "the denominator: every combination of the chooser's five inputs"
    );

    let never_chosen: Vec<&str> = names
        .iter()
        .filter(|(k, _)| !reachable.contains(k))
        .map(|(_, n)| n.as_str())
        .collect();
    // The table's counts, kept as premises so that this scenario cannot quietly become an assertion
    // about a table that has changed under it.
    assert_eq!(
        names.len(),
        41,
        "the premise: the shipped table holds forty-one pictures"
    );
    assert_eq!(reachable.len(), 15, "fifteen of them are selectable at all");
    assert_eq!(
        never_chosen.len(),
        26,
        "and twenty-six are selected by nothing"
    );
    assert_eq!(
        never_chosen
            .iter()
            .filter(|n| n.starts_with("Move_"))
            .count(),
        23,
        "twenty-three of those twenty-six are the movement family -- a different feature"
    );
    assert_eq!(
        never_chosen
            .iter()
            .filter(|n| n.starts_with("Combat_Advanced_"))
            .count(),
        3,
        "and the other three are the advanced-fighting pictures"
    );
    let some_are_never_chosen = !never_chosen.is_empty();
    let none_of_them_is_for_a_drag = !never_chosen.iter().any(|n| {
        let n = n.to_ascii_lowercase();
        n.contains("drag") || n.contains("drop")
    });
    let nothing_shipped_is_named_for_a_drag = !names.iter().any(|(_, n)| {
        let n = n.to_ascii_lowercase();
        n.contains("drag") || n.contains("drop")
    });
    let fewer_are_reachable_than_are_shipped = reachable.len() < names.len();

    let mut c = c;
    c.assert_behaviour(
        "inventory.drag-hint.no-cursor-the-client-ships-is-chosen-by-a-drag",
        move |_| {
            some_are_never_chosen
                && none_of_them_is_for_a_drag
                && nothing_shipped_is_named_for_a_drag
                && fewer_are_reachable_than_are_shipped
        },
    );
    c.shutdown();
}
