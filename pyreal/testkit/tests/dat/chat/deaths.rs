use super::*;
// ---------------------------------------------------------------------------------------------
// The three death lines the HUD composes
// ---------------------------------------------------------------------------------------------
//
// The model half of two of these -- that somebody else's death is announced and your own is not --
// is `chat.death.a-third-partys-death-is-announced-and-your-own-is-not` in the cpu tier. What is
// here is the whole client's: the line composed for the log, through the client's own scroll.

/// The player this section is, and the two other people in it.
const DEAD_LOCAL: ObjectId = ObjectId(0x5000_0001);
const DEAD_VICTIM: ObjectId = ObjectId(0x5000_0011);
const DEAD_KILLER: ObjectId = ObjectId(0x5000_0012);

/// A client in the world, with a player of its own, ready to be told about a death.
fn a_client_that_can_die() -> HeadlessClient {
    let mut c = a_client_listening();
    c.world_mut().player = Some(DEAD_LOCAL);
    c
}

/// The five counts the two arms keep, so a line refused at a guard is told apart from a message
/// that never read at all.
fn death_counts(c: &HeadlessClient) -> (u64, u64, u64, u64, u64) {
    let s = &c.view().expect_app().hud().stats;
    (
        s.player_deaths,
        s.player_deaths_announced,
        s.victim_notifications,
        s.victim_notifications_announced,
        s.scroll_lines,
    )
}

/// Hand one message to the client, and answer the lines composed from it.
///
/// Both of the arms below end by putting a line on the client's own scroll, which is drained at the
/// **head** of the next batch -- so the empty second call is the client's own ordering rather than
/// a settling loop.
fn hear_a_death(
    c: &mut HeadlessClient,
    e: SessionEvent,
) -> Vec<pyreal_client_contract::chat::interface::ChatMessage> {
    let mut composed = c.app_mut().apply_hud_events(&[e]);
    composed.extend(c.app_mut().apply_hud_events(&[]));
    c.tick(1);
    composed
}

/// Whether any of `composed` carries `needle`.
fn carries(
    composed: &[pyreal_client_contract::chat::interface::ChatMessage],
    needle: &str,
) -> bool {
    composed.iter().any(|m| m.body.contains(needle))
}

/// One announcement that somebody died.
fn death_event(message: &str, killed: ObjectId, killer: ObjectId) -> SessionEvent {
    use pyreal_protocol::Message as _;
    let m = pyreal_protocol::combat::CombatHandlePlayerDeathEvent {
        message: message.to_owned(),
        killed,
        killer,
    };
    let mut w = pyreal_protocol::archive::Writer::new();
    w.u32(pyreal_protocol::Opcode::COMBAT_HANDLE_PLAYER_DEATH_EVENT.0);
    m.write(&mut w).expect("the message encodes");
    SessionEvent::UiEvent {
        opcode: pyreal_protocol::Opcode::COMBAT_HANDLE_PLAYER_DEATH_EVENT,
        blob: w.into_inner(),
    }
}

/// One notification that you died, or that you killed something.
fn notification_event(opcode: pyreal_protocol::Opcode, message: &str) -> SessionEvent {
    use pyreal_protocol::Message as _;
    let m = pyreal_protocol::combat::VictimNotificationOther {
        message: message.to_owned(),
    };
    let mut w = pyreal_protocol::archive::Writer::new();
    w.u32(opcode.0);
    m.write(&mut w).expect("the message encodes");
    SessionEvent::UiEvent {
        opcode,
        blob: w.into_inner(),
    }
}

/// Every recorded notification that the player killed something, found by walking the recordings.
fn recorded_kill_notifications() -> Vec<(pyreal_protocol::Opcode, Vec<u8>, String)> {
    use pyreal_client_net::client_session::testing::Direction;
    let mut out = Vec::new();
    for corpus in Corpus::load_all() {
        for b in &corpus.blobs {
            if b.dir != Direction::ServerToClient || b.payload.len() < 16 {
                continue;
            }
            let (op, body) = if b.opcode == 0xF7B0 {
                (
                    u32::from_le_bytes(b.payload[12..16].try_into().expect("four bytes")),
                    &b.payload[16..],
                )
            } else {
                (b.opcode, &b.payload[4..])
            };
            if op != pyreal_protocol::Opcode::COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_OTHER.0 {
                continue;
            }
            let Ok(m) = pyreal_protocol::read_body::<
                pyreal_protocol::combat::VictimNotificationOther,
            >(body) else {
                continue;
            };
            let blob = if b.opcode == 0xF7B0 {
                b.payload[12..].to_vec()
            } else {
                b.payload.clone()
            };
            out.push((pyreal_protocol::Opcode(op), blob, m.message));
        }
    }
    assert!(
        !out.is_empty(),
        "the corpus carries recorded kill notifications"
    );
    out
}

/// Every notification a shard really sent about something the player killed is composed for the
/// log word for word, as a plain line.
pub fn every_recorded_kill_notification_is_drawn_verbatim() {
    let recorded = recorded_kill_notifications();
    let mut c = a_client_that_can_die();
    let mut every = true;

    for (opcode, blob, message) in &recorded {
        let before = death_counts(&c);
        let composed = hear_a_death(
            &mut c,
            SessionEvent::UiEvent {
                opcode: *opcode,
                blob: blob.clone(),
            },
        );
        let after = death_counts(&c);
        // Read, announced, and it travelled the client's own scroll -- the road every line takes,
        // rather than a second one built for this message.
        let counted = (after.2 - before.2, after.3 - before.3, after.4 - before.4) == (1, 1, 1);
        let line = composed
            .iter()
            .find(|m| m.body.contains(message.trim()))
            .unwrap_or_else(|| panic!("no line carried {message:?}"));
        let composed_right =
            line.ty == 0 && line.window == 0 && line.body == *message && !line.body.ends_with('\n');
        assert!(counted && composed_right, "{message:?}");
        every &= counted && composed_right;
    }

    c.assert_behaviour(
        "chat.death.every-recorded-kill-notification-is-drawn-word-for-word-on-the-log",
        move |_| every,
    );
    c.shutdown();
}

/// Being told that you died goes through the same hand as being told you killed something -- and a
/// notification with nothing in it is read and then deliberately says nothing.
pub fn your_own_death_goes_through_the_same_hand() {
    let mut c = a_client_that_can_die();

    let mut every = true;
    for message in [
        "You died!",
        "Gaerlan cleaves you in twain!",
        "You are torn to ribbons by Gaerlan's assault!",
    ] {
        let before = death_counts(&c);
        let composed = hear_a_death(
            &mut c,
            notification_event(
                pyreal_protocol::Opcode::COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_SELF,
                message,
            ),
        );
        let after = death_counts(&c);
        let counted = (after.2 - before.2, after.3 - before.3, after.4 - before.4) == (1, 1, 1);
        let line = composed
            .iter()
            .find(|m| m.body.contains(message))
            .unwrap_or_else(|| panic!("no line carried {message:?}"));
        let composed_right = (line.ty, line.window) == (0, 0) && line.body == message;
        assert!(counted && composed_right, "{message:?}");
        every &= counted && composed_right;
    }

    // A notification with nothing in it: read either way, and deliberately not announced.
    let mut empty_holds = true;
    let mut nothing_drawn = true;
    for opcode in [
        pyreal_protocol::Opcode::COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_SELF,
        pyreal_protocol::Opcode::COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_OTHER,
    ] {
        let before = death_counts(&c);
        nothing_drawn &= hear_a_death(&mut c, notification_event(opcode, "")).is_empty();
        let after = death_counts(&c);
        let holds = (after.2 - before.2, after.3 - before.3, after.4 - before.4) == (1, 0, 0);
        assert!(holds, "{opcode:?}");
        empty_holds &= holds;
    }

    c.assert_behaviour(
        "chat.death.your-own-death-goes-through-the-same-hand-and-an-empty-one-says-nothing",
        move |_| every && empty_holds && nothing_drawn,
    );
    c.shutdown();
}

/// Somebody else's death is composed for the log; a death the player was part of is not, from either
/// side -- and a client that does not know who its player is yet shows it, because the guard is
/// about the player not being involved and not about there being one.
pub fn a_death_you_were_part_of_reaches_the_log_only_when_you_were_not() {
    let mut c = a_client_that_can_die();
    let knows_itself = c.view().world().is_the_player(DEAD_LOCAL);

    let message = "Gaerlan splits Larktest apart!";
    let before = death_counts(&c);
    let composed = hear_a_death(&mut c, death_event(message, DEAD_VICTIM, DEAD_KILLER));
    let after = death_counts(&c);
    let counted = (after.0 - before.0, after.1 - before.1, after.4 - before.4) == (1, 1, 1);
    let line = composed
        .iter()
        .find(|m| m.body.contains(message))
        .expect("a line carried the announcement");
    let composed_right = (line.ty, line.window) == (0, 0) && line.body == message;

    // The three ways the player can be part of it. The killer side is the one an implementation is
    // most likely to miss, because the victim one alone passes everything else here.
    let mut refused = true;
    for (tag, killed, killer) in [
        ("you are the victim", DEAD_LOCAL, DEAD_KILLER),
        ("you are the killer", DEAD_VICTIM, DEAD_LOCAL),
        ("both, a suicide", DEAD_LOCAL, DEAD_LOCAL),
    ] {
        let message = format!("death {tag}");
        let before = death_counts(&c);
        let composed = hear_a_death(&mut c, death_event(&message, killed, killer));
        let after = death_counts(&c);
        let holds = (after.0 - before.0, after.1 - before.1, after.4 - before.4) == (1, 0, 0)
            && !carries(&composed, &message);
        assert!(holds, "{tag}");
        refused &= holds;
    }

    // And the same thing again on a client that does not know who its player is: it matches
    // nobody, so the line is shown.
    c.world_mut().player = None;
    let before = death_counts(&c);
    let composed = hear_a_death(
        &mut c,
        death_event("death with no player yet", DEAD_LOCAL, DEAD_KILLER),
    );
    let after = death_counts(&c);
    let unknown = (after.0 - before.0, after.1 - before.1) == (1, 1)
        && carries(&composed, "death with no player yet");

    c.assert_behaviour(
        "chat.death.a-death-the-player-was-part-of-reaches-the-log-only-when-he-was-not",
        move |_| knows_itself && counted && composed_right && refused && unknown,
    );
    c.shutdown();
}

/// None of the three death lines can be silenced, and a combat line beside them can -- which is
/// what makes that a measurement rather than a squelch that never armed.
pub fn no_death_line_is_silenced_where_a_combat_line_is() {
    use pyreal_protocol::Message as _;

    let mut c = a_client_that_can_die();

    // Everything the table can be asked about, silenced: every kind, and the killer by name and by
    // account. The two death arms ask nothing, so none of it can bite.
    {
        let mut entry = pyreal_client_model::chat::SquelchEntry {
            name: "Gaerlan".to_owned(),
            ..Default::default()
        };
        entry.squelch_everything();
        let squelch = &mut c.world_mut().chat.squelch;
        squelch.characters.insert(DEAD_KILLER, entry.clone());
        squelch.characters.insert(ObjectId(0), entry);
        squelch.accounts.insert("Gaerlan".to_owned(), 0);
        squelch
            .global
            .types
            .insert(pyreal_client_model::chat::text_type::COMBAT);
        squelch.global.types.insert(0);
    }

    // The control: a combat line, which does ask, and is dropped.
    let before = c.view().expect_app().hud().stats.combat_lines_squelched;
    let mut w = pyreal_protocol::archive::Writer::new();
    w.u32(pyreal_protocol::Opcode::COMBAT_HANDLE_ATTACKER_NOTIFICATION_EVENT.0);
    pyreal_protocol::combat::AttackerNotification {
        defender_name: "death control".to_owned(),
        damage_type: 1,
        percent: 0.1,
        damage: 3,
        critical: 0,
        attack_conditions: 0,
        attack_conditions_high: 0,
    }
    .write(&mut w)
    .expect("the control message encodes");
    hear_a_death(
        &mut c,
        SessionEvent::UiEvent {
            opcode: pyreal_protocol::Opcode::COMBAT_HANDLE_ATTACKER_NOTIFICATION_EVENT,
            blob: w.into_inner(),
        },
    );
    let armed = c.view().expect_app().hud().stats.combat_lines_squelched == before + 1;

    // And the three subjects, with that same silence in force.
    let mut every = true;
    let cases: Vec<(String, SessionEvent)> = vec![
        (
            "death silenced third party".to_owned(),
            death_event("death silenced third party", DEAD_VICTIM, DEAD_KILLER),
        ),
        (
            "death silenced your own".to_owned(),
            notification_event(
                pyreal_protocol::Opcode::COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_SELF,
                "death silenced your own",
            ),
        ),
        (
            "death silenced your kill".to_owned(),
            notification_event(
                pyreal_protocol::Opcode::COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_OTHER,
                "death silenced your kill",
            ),
        ),
    ];
    for (needle, event) in cases {
        let before = death_counts(&c);
        let composed = hear_a_death(&mut c, event);
        let after = death_counts(&c);
        let holds = after.4 - before.4 == 1 && carries(&composed, &needle);
        assert!(holds, "{needle}");
        every &= holds;
    }

    c.assert_behaviour(
        "chat.death.no-death-line-can-be-silenced-where-a-combat-line-can",
        move |_| armed && every,
    );
    c.shutdown();
}
