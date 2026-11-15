use super::*;

/// The four contracts this scenario drives, ids of shipped contract table `0x0E00001D`.
///
/// `"The Shadows of Bitter Winter"` -- a repeat flag, no end NPC, quest area `0xC8DC0033`.
const CONTRACTS_SHADOWS: u32 = 1;
/// `"Test Quest Stamping"` -- the only one of the four with **both** NPCs and a timer flag.
const CONTRACTS_STAMPING: u32 = 2;
/// `"Reign of Terror"`.
const CONTRACTS_TERROR: u32 = 5;
/// `"Glenden Wood Invasion (Low)"`.
const CONTRACTS_GLENDEN: u32 = 6;

/// One wire contract tracker. Its receipt timestamp is **not** on the wire -- the client
/// stamps it on receipt.
fn contracts_tracker(
    id: u32,
    stage: u32,
    when_done: f64,
    when_repeats: f64,
) -> pyreal_protocol::social::ContractTracker {
    pyreal_protocol::social::ContractTracker {
        version: 0,
        contract_id: id,
        contract_stage: stage,
        time_when_done: when_done,
        time_when_repeats: when_repeats,
    }
}

/// `0x0314`, carrying the four contracts.
fn contracts_table() -> pyreal_protocol::social::SocialSendClientContractTrackerTable {
    pyreal_protocol::social::SocialSendClientContractTrackerTable(
        pyreal_protocol::archive::PackedHash {
            table_size: 32,
            entries: vec![
                (
                    CONTRACTS_SHADOWS,
                    contracts_tracker(CONTRACTS_SHADOWS, 1, 0.0, 0.0),
                ),
                (
                    CONTRACTS_STAMPING,
                    contracts_tracker(CONTRACTS_STAMPING, 2, 90.0, 0.0),
                ),
                (
                    CONTRACTS_TERROR,
                    contracts_tracker(CONTRACTS_TERROR, 3, 0.0, 0.0),
                ),
                (
                    CONTRACTS_GLENDEN,
                    contracts_tracker(CONTRACTS_GLENDEN, 4, 0.0, 0.0),
                ),
            ],
        },
    )
}

/// A client in the world with the four contracts delivered through the **real** `0x0314` arm.
fn a_client_holding_four_contracts() -> (HeadlessClient, Peer) {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let peer = Peer::attach_creating(&mut c, HOUSE_PLAYER);
    c.world_mut().player = Some(HOUSE_PLAYER);
    c.when(Inbound::message(&contracts_table()));
    c.tick(2);
    (c, peer)
}

/// The contract ids the world holds, in id order, with each one's stage.
fn contracts_held(c: &HeadlessClient) -> Vec<(u32, u32)> {
    c.view()
        .world()
        .contracts
        .trackers
        .values()
        .map(|t| (t.contract_id, t.contract_stage))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// contracts.list.one-contract-at-a-time-is-added-changed-or-taken-away
// ---------------------------------------------------------------------------------------------

/// `0x0315` updates one tracker in place, deletes one, or adds one, and each is counted once.
pub(super) fn one_contract_at_a_time_is_added_changed_or_taken_away() {
    let (mut c, _peer) = a_client_holding_four_contracts();
    let all_four = contracts_held(&c)
        == vec![
            (CONTRACTS_SHADOWS, 1),
            (CONTRACTS_STAMPING, 2),
            (CONTRACTS_TERROR, 3),
            (CONTRACTS_GLENDEN, 4),
        ];

    // An update in place: SHADOWS moves from stage 1 to stage 2.
    deliver(
        &mut c,
        &pyreal_protocol::social::SocialSendClientContractTracker {
            tracker: contracts_tracker(CONTRACTS_SHADOWS, 2, 0.0, 0.0),
            delete_contract: 0,
            set_as_display_contract: 0,
        },
    );
    c.tick(1);
    let updated = c.view().expect_app().hud().stats.contract_trackers_updated == 1
        && contracts_held(&c).len() == 4
        && contracts_held(&c).contains(&(CONTRACTS_SHADOWS, 2));

    // A delete removes the matching contract entry.
    deliver(
        &mut c,
        &pyreal_protocol::social::SocialSendClientContractTracker {
            tracker: contracts_tracker(CONTRACTS_SHADOWS, 2, 0.0, 0.0),
            delete_contract: 1,
            set_as_display_contract: 0,
        },
    );
    c.tick(1);
    let removed = c.view().expect_app().hud().stats.contract_trackers_removed == 1
        && contracts_held(&c)
            == vec![
                (CONTRACTS_STAMPING, 2),
                (CONTRACTS_TERROR, 3),
                (CONTRACTS_GLENDEN, 4),
            ];

    // And an add, for a contract the character did not hold.
    deliver(
        &mut c,
        &pyreal_protocol::social::SocialSendClientContractTracker {
            tracker: contracts_tracker(CONTRACTS_SHADOWS, 1, 0.0, 0.0),
            delete_contract: 0,
            set_as_display_contract: 1,
        },
    );
    c.tick(1);
    let added = {
        let s = &c.view().expect_app().hud().stats;
        // `setAsDisplayContract` is counted, because this build pins nothing to the HUD.
        s.contract_trackers_added == 1 && s.contract_display_requests == 1
    } && contracts_held(&c).len() == 4
        && contracts_held(&c).contains(&(CONTRACTS_SHADOWS, 1));

    c.assert_behaviour(
        "contracts.list.one-contract-at-a-time-is-added-changed-or-taken-away",
        move |_| all_four && updated && removed && added,
    );
    c.shutdown();
}
