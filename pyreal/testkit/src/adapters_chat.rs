//! What the chat scenarios of both tiers share: the private messages the recordings carry.

// ---------------------------------------------------------------------------------------------
// The private messages the recordings carry
// ---------------------------------------------------------------------------------------------

/// Every private message the recorded shards sent, as `(recording, blob)` -- the bytes the shard
/// put on the wire, with the leading opcode dword, which is `SessionEvent::UiEvent`'s own contract.
///
/// **Why this reads the raw recordings and not the decoded corpus.** `pyreal_client_net::client_session::testing::Corpus`
/// is the index every count in this crate is read from, and it carries **no** `0x02BD` at all: the
/// derivation that builds it leaves private messages out, which is the right thing for an index
/// that is read by name. The claims about *who* a private message is addressed to have no other
/// oracle -- a synthesised message would be this crate agreeing with itself -- so they are taken
/// off the datagrams, one layer below, through the same reader every replaying scenario uses.
///
/// Nothing of what the messages *say* leaves this function: the scenarios that call it assert over
/// names, ids and byte equality.
///
/// The whole set is replayed once per test binary and held, because eighteen recordings through a
/// real session is seconds rather than milliseconds.
///
/// # Panics
/// Panics when a recording named by the corpus index is missing or does not parse, which is a
/// broken checkout rather than a state the client could be in.
#[must_use]
pub fn recorded_private_messages() -> &'static [(String, Vec<u8>)] {
    static ALL: std::sync::OnceLock<Vec<(String, Vec<u8>)>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut out = Vec::new();
        for name in pyreal_client_net::client_session::testing::session_names() {
            let records = pyreal_client_net::client_session::testing::capture::load_session(name)
                .unwrap_or_else(|e| panic!("the recording {name} does not parse: {e}"));
            let sequence =
                pyreal_client_net::recording::connection_sequence_number(&records).unwrap_or(0);
            let mut net = pyreal_client_runtime::net::ClientNetwork::new(
                &pyreal_client_net::client_session::testing::capture::peer(0).to_string(),
                7304,
                "pyreal-testkit",
                "unused",
                sequence,
            )
            .expect("the replay endpoint");
            let mut objects = pyreal_client_runtime::objects::ObjectStream::new();
            let mut entered = false;
            for r in &records {
                let now = pyreal_primitives::LocalTime(r.t);
                if !r.c2s {
                    net.feed(
                        &r.raw,
                        pyreal_client_net::client_session::testing::capture::peer(r.pair),
                        now,
                    );
                }
                net.tick(now);
                let _ = net.take_outgoing();
                for e in objects.pump(&mut net, now) {
                    if let pyreal_client_net::client_session::SessionEvent::CharacterSet(set) = &e {
                        if !entered {
                            if let Some(c) = set.characters.first() {
                                let account = set.account.clone();
                                net.enter_world(c.gid, &account);
                                entered = true;
                            }
                        }
                    }
                    if let pyreal_client_net::client_session::SessionEvent::UiEvent {
                        opcode,
                        blob,
                    } = e
                    {
                        if opcode == pyreal_protocol::Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH {
                            out.push(((*name).to_owned(), blob));
                        }
                    }
                }
            }
        }
        out
    })
}
