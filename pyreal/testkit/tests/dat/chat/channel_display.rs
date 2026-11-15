use super::*;
// ---------------------------------------------------------------------------------------------
// A line broadcast on a channel, and who can silence it
// ---------------------------------------------------------------------------------------------

/// Hand one broadcast to the client, and answer what the HUD composed from it.
fn hear_on_a_channel(
    c: &mut HeadlessClient,
    channel: u32,
    sender: &str,
    message: &str,
) -> Vec<pyreal_client_contract::chat::interface::ChatMessage> {
    use pyreal_protocol::Message as _;
    let m = pyreal_protocol::comms::CommunicationChannelBroadcastRecv {
        channel,
        sender_name: sender.to_owned(),
        message: message.to_owned(),
    };
    let mut w = pyreal_protocol::archive::Writer::new();
    w.u32(pyreal_protocol::Opcode::COMMUNICATION_CHANNEL_BROADCAST.0);
    m.write(&mut w).expect("the message encodes");
    let composed = c.app_mut().apply_hud_events(&[SessionEvent::UiEvent {
        opcode: pyreal_protocol::Opcode::COMMUNICATION_CHANNEL_BROADCAST,
        blob: w.into_inner(),
    }]);
    c.tick(1);
    composed
}

/// How many channel lines the client has composed and how many it has refused. The pair tells a
/// line dropped at the gate apart from a line the client never read at all.
fn channel_counts(c: &HeadlessClient) -> (u64, u64) {
    let s = &c.view().expect_app().hud().stats;
    (
        s.channel_broadcast_lines_composed,
        s.channel_broadcast_lines_squelched,
    )
}

/// A speaker's name, lifted off a recorded spoken line so that the name is a shard's and not this
/// file's.
fn a_recorded_speaker_name() -> String {
    use pyreal_client_net::client_session::testing::Direction;
    for corpus in Corpus::load_all() {
        for b in &corpus.blobs {
            if b.dir != Direction::ServerToClient
                || b.opcode != pyreal_protocol::Opcode::COMMUNICATION_HEAR_SPEECH.0
            {
                continue;
            }
            let Ok(m) = pyreal_protocol::read_body::<pyreal_protocol::comms::CommunicationHearSpeech>(
                &b.payload[4..],
            ) else {
                continue;
            };
            if !m.sender_name.is_empty() && m.sender_name.is_ascii() {
                return m.sender_name;
            }
        }
    }
    panic!("the corpus carries a recorded speaker's name");
}

/// Silencing the speaker does not silence a channel line, and silencing the kind of line does --
/// but only for a kind the client will answer about at all.
pub fn a_squelched_speaker_is_still_heard_on_a_channel() {
    use pyreal_client_model::chat::text_type;

    let who = a_recorded_speaker_name();
    let mut c = a_client_listening();

    // The speaker, silenced on everything there is and on his account too. A channel line carries
    // no speaker's id, so the strongest form of "this one is silenced" cannot reach the question
    // the client asks -- and that is the claim.
    {
        let mut entry = pyreal_client_model::chat::SquelchEntry {
            name: who.clone(),
            ..Default::default()
        };
        entry.squelch_everything();
        let squelch = &mut c.world_mut().chat.squelch;
        squelch.characters.insert(ObjectId(0x5000_0099), entry);
        squelch.accounts.insert(who.clone(), 0);
    }
    let still_heard = hear_on_a_channel(&mut c, 0x0000_0800, &who, "channel speaker silenced")
        .iter()
        .any(|m| m.body.contains("channel speaker silenced"));

    // The kind of line, silenced: a fellowship line is one the client will answer about, and it is
    // dropped before it is composed.
    c.world_mut()
        .chat
        .squelch
        .global
        .types
        .insert(text_type::FELLOWSHIP);
    let before = channel_counts(&c);
    let composed = hear_on_a_channel(&mut c, 0x0000_0800, &who, "channel kind silenced");
    let after = channel_counts(&c);
    let dropped = (after.0 - before.0, after.1 - before.1) == (0, 1)
        && !composed
            .iter()
            .any(|m| m.body.contains("channel kind silenced"));

    // The same table, a kind the client will not answer about: the question is refused before it
    // reaches the table, so the line is composed.
    c.world_mut()
        .chat
        .squelch
        .global
        .types
        .insert(text_type::SOCIAL);
    let before = channel_counts(&c);
    let composed = hear_on_a_channel(&mut c, 0x0200_0000, &who, "channel kind not asked");
    let after = channel_counts(&c);
    let still_drawn = (after.0 - before.0, after.1 - before.1) == (1, 0)
        && composed
            .iter()
            .any(|m| m.body.contains("channel kind not asked"));

    c.assert_behaviour(
        "chat.channel.silencing-the-speaker-does-not-silence-a-channel-line",
        move |_| still_heard && dropped && still_drawn,
    );
    c.shutdown();
}
