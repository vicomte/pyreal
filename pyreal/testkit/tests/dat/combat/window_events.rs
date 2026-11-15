use super::*;

// ---------------------------------------------------------------------------------------------
// combat.mode.* -- the shard's word on the combat mode, and the keys that follow it
//
// **The corpus census is read at run time and never pinned.** No recording and no count of mode
// changes is written down: this walks
// `pyreal_client_net::client_session::testing::session_index()`, asserts that the corpus carries
// such a change at all, and asserts the client's answer to every one it finds.
// ---------------------------------------------------------------------------------------------

pub(super) fn the_shards_own_word_opens_the_window_and_the_client_sends_nothing() {
    use pyreal_client_model::combat::CombatMode;
    use pyreal_primitives::ObjectId;

    const THE_PLAYER: ObjectId = ObjectId(0x5290_0001);
    /// The number the shard keeps this character's combat mode in.
    const COMBAT_MODE: u32 = 0x28;

    let mut c = window::a_client_in_the_world();
    window::this_character_is(&mut c, THE_PLAYER);
    {
        let w = c.world_mut();
        w.player_system.options.set(
            pyreal_client_model::player::options::option::ADVANCED_COMBAT_UI,
            false,
        );
        // Deliberately stale, so the mode change is what clears it.
        w.combat.advanced_combat_mode = true;
        w.player_system.options.set(
            pyreal_client_model::player::options::option::AUTO_TARGET,
            false,
        );
        // A request of the player's own, left pending, so "the shard's word is not an answer to
        // it" is a measurement.
        w.combat.pending_combat_mode = CombatMode::Magic;
    }
    let sent_before = c.view().interaction().pending_requests().len();

    c.when(window::the_shard_says(
        None,
        10,
        COMBAT_MODE,
        CombatMode::Missile.raw(),
    ));
    let taken = (
        c.view().world().combat.combat_mode,
        c.view().world().combat.pending_combat_mode,
        c.view().world().combat.advanced_combat_mode,
        c.view().interaction().pending_requests().len(),
    );
    // The keys follow the mode on the next frame.
    c.world_mut().combat.pending_combat_mode = CombatMode::Undef;
    window::settle(&mut c);
    let keys = window::combat_keys(&mut c);

    // The public form of the same message about this character is authority too, including the
    // way back to peace.
    c.when(window::the_shard_says(
        Some(THE_PLAYER),
        11,
        COMBAT_MODE,
        CombatMode::NonCombat.raw(),
    ));
    let mode_back = c.view().world().combat.combat_mode;
    window::settle(&mut c);
    let back = (mode_back, window::combat_keys(&mut c));

    c.assert_behaviour(
        "combat.mode.the-shards-own-word-opens-the-window-and-the-client-sends-nothing",
        move |_| {
            taken.0 == CombatMode::Missile
                && taken.1 == CombatMode::Magic
                && !taken.2
                && taken.3 == sent_before
                && keys == CombatMode::Missile.raw()
                && back.0 == CombatMode::NonCombat
                && back.1 == CombatMode::NonCombat.raw()
        },
    );
    c.shutdown();
}

pub(super) fn an_update_that_is_not_this_players_or_is_older_is_refused() {
    use pyreal_client_model::combat::CombatMode;
    use pyreal_primitives::ObjectId;

    const THE_PLAYER: ObjectId = ObjectId(0x5290_0002);
    const COMBAT_MODE: u32 = 0x28;
    /// Another of this character's numbers, so "the right property" is a measurement.
    const SOME_OTHER_NUMBER: u32 = 0x29;

    let mut c = window::a_client_in_the_world();
    window::this_character_is(&mut c, THE_PLAYER);
    c.when(window::the_shard_says(
        None,
        10,
        COMBAT_MODE,
        CombatMode::Missile.raw(),
    ));
    let live = c.view().world().combat.combat_mode;

    let stale_before = c.view().hud().stats.quality_updates_stale;
    c.when(window::the_shard_says(
        Some(ObjectId(THE_PLAYER.0 + 1)),
        11,
        COMBAT_MODE,
        1,
    ));
    c.when(window::the_shard_says(None, 11, SOME_OTHER_NUMBER, 1));
    c.when(window::the_shard_says(None, 9, COMBAT_MODE, 1));
    let after_the_three = (
        c.view().world().combat.combat_mode,
        c.view().hud().stats.quality_updates_stale,
    );

    let undecodable_before = c.view().hud().stats.undecodable;
    c.when(window::a_truncated_update(11, COMBAT_MODE, 1));
    let after_the_broken = (
        c.view().world().combat.combat_mode,
        c.view().hud().stats.undecodable,
    );

    // The two refusals are told apart: the stale one moved the stale count and not the
    // undecodable one, and the broken one moved the undecodable count and not the stale one.
    let told_apart = after_the_broken.1 == undecodable_before + 1
        && c.view().hud().stats.quality_updates_stale == after_the_three.1;

    // **What this scenario does not assert**: that after the character's own description is torn
    // down no such message moves the mode at all. That guard is the part of the client that keeps
    // the character's own numbers, and it is only one consumer. A `when` step delivers the message
    // to the **whole** frame's consumers, as a real frame does, and the object stream applies the
    // number with no such guard -- so the mode does move. Neither half is asserted here.
    c.assert_behaviour(
        "combat.mode.an-update-that-is-not-this-players-or-is-older-than-the-last-is-refused",
        move |_| {
            live == CombatMode::Missile
                && after_the_three == (CombatMode::Missile, stale_before + 1)
                && after_the_broken == (CombatMode::Missile, undecodable_before + 1)
                && told_apart
        },
    );
    c.shutdown();
}

pub(super) fn the_recorded_combat_mode_changes_reach_the_window_the_buttons_and_the_keys() {
    use pyreal_client_net::client_session::testing::{Corpus, Direction};
    use pyreal_primitives::NetQueue;
    use pyreal_protocol::{Message, Opcode};

    /// The number the shard keeps this character's combat mode in.
    const COMBAT_MODE: u32 = 0x28;

    let mut c = window::a_client_in_the_world();
    {
        let w = c.world_mut();
        w.player_system.options.set(
            pyreal_client_model::player::options::option::ADVANCED_COMBAT_UI,
            false,
        );
        w.player_system.options.set(
            pyreal_client_model::player::options::option::AUTO_TARGET,
            false,
        );
    }

    // Every recording the index names, and every mode change each of them really carries. No
    // count and no recording's name is written down.
    let mut seen = 0usize;
    let mut every_reading_holds = true;
    for (id, _slug) in pyreal_client_net::client_session::testing::session_index() {
        let corpus = Corpus::shared(id);
        c.hud_mut().player_desc_received = true;
        let mut have_player = false;
        for row in &corpus.blobs {
            if row.dir != Direction::ServerToClient {
                continue;
            }
            if row.opcode == Opcode::LOGIN_CREATE_PLAYER.0 {
                let player = pyreal_protocol::objects::LoginCreatePlayer::read(
                    &mut pyreal_protocol::Reader::new(&row.payload[4..]),
                )
                .expect("the recorded identity decodes")
                .player_id;
                c.when(pyreal_testkit::Inbound::event(
                    pyreal_client_net::client_session::SessionEvent::PlayerCreated(player),
                ));
                window::this_character_is(&mut c, player);
                have_player = true;
            }
            if row.queue != NetQueue::UiQueue {
                continue;
            }
            let Ok(mut blob) = pyreal_protocol::events::split_ui_blob(&row.payload) else {
                continue;
            };
            if blob.sub_type != Opcode::QUALITIES_PRIVATE_UPDATE_INT {
                continue;
            }
            let Ok(update) =
                pyreal_protocol::qualities::QualitiesPrivateUpdateInt::read(&mut blob.body)
            else {
                continue;
            };
            if update.0.property_id != COMBAT_MODE {
                continue;
            }
            assert!(
                have_player,
                "a private update needs the recorded identity first"
            );
            let offset = if blob.order.is_some() {
                pyreal_protocol::OrderedEventHeader::PACK_SIZE
            } else {
                0
            };
            let sent_before = c.view().interaction().pending_requests().len();
            c.when(pyreal_testkit::Inbound::event(
                pyreal_client_net::client_session::SessionEvent::UiEvent {
                    opcode: blob.sub_type,
                    blob: row.payload[offset..].to_vec(),
                },
            ));
            #[allow(clippy::cast_sign_loss)]
            let mode = update.0.value as u32;
            window::settle(&mut c);
            every_reading_holds &= c.view().world().combat.combat_mode.raw() == mode
                && window::combat_keys(&mut c) == mode
                && c.view().interaction().pending_requests().len() == sent_before;
            seen += 1;
        }
    }

    c.assert_behaviour(
        "combat.mode.the-recorded-changes-reach-the-window-the-buttons-and-the-keys",
        move |_| seen > 0 && every_reading_holds,
    );
    c.shutdown();
}
