use super::*;
/// The body of the first server-to-client blob of `ROOMS_SESSION` carrying `opcode`, starting at
/// the opcode dword.
///
/// The blobs are found by walking the recording rather than by index, so a re-locked corpus moves
/// nothing here.
pub(super) fn recorded(opcode: u32) -> Vec<u8> {
    let corpus = Corpus::load(ROOMS_SESSION)
        .expect("the corpus decodes")
        .expect("the corpus holds the recording");
    for b in &corpus.blobs {
        if b.dir != Direction::ServerToClient {
            continue;
        }
        if b.opcode == opcode {
            return b.payload.clone();
        }
        // A game event carries its own header before the message it wraps.
        if b.opcode == 0xF7B0 && b.payload.len() >= 16 {
            let sub = u32::from_le_bytes(b.payload[12..16].try_into().expect("four bytes"));
            if sub == opcode {
                return b.payload[12..].to_vec();
            }
        }
    }
    panic!("{ROOMS_SESSION} carries no server-to-client {opcode:#x}");
}

/// A whole client, with or without its interface, and nothing settled yet.
pub(super) fn a_bare_client(shell: bool) -> HeadlessClient {
    HeadlessClient::new(ClientSpec {
        shell,
        settle_frames: 0,
        ..ClientSpec::retail()
    })
}

/// Hand the client a description of its own player with the two listening options as given.
pub(super) fn describe(c: &mut HeadlessClient, general: bool, allegiance: bool) {
    let mut module = pyreal_protocol::login::PlayerModule::default();
    if general {
        module.options2 |= 0x100;
    } else {
        module.options2 &= !0x100;
    }
    if allegiance {
        module.options |= 0x4000_0000;
    } else {
        module.options &= !0x4000_0000;
    }
    c.when(Inbound::event(SessionEvent::PlayerDescription(Box::new(
        pyreal_protocol::login::LoginPlayerDescription {
            player_module: module,
            ..Default::default()
        },
    ))));
}

/// The main chat window, which is where the interface hands a typed line on from.
pub(super) const MAIN_WINDOW: u32 = pyreal_client_contract::chat::interface::window::MAIN;

/// Type a line into the main chat window, as the interface hands it on, and run the frame that
/// sends it.
pub(super) fn say(c: &mut HeadlessClient, text: &str) {
    c.when(Player::ui(pyreal_client_contract::UiRequest::ChatLine {
        text: text.to_owned(),
        window: MAIN_WINDOW,
    }))
    .tick(1);
}

/// Take the lines the client's own scroll is holding, composed as the HUD composes them.
///
/// A line the client says to itself is added to the scroll in one frame and drained at the head of
/// the next, so taking it here, between the two, is what the player would read.
pub(super) fn scroll_lines(c: &mut HeadlessClient) -> Vec<String> {
    c.app_mut()
        .apply_hud_events(&[])
        .into_iter()
        .map(|m| m.body)
        .collect()
}

/// Whether the client has started its interface.
pub(super) fn has_ui(c: &HeadlessClient) -> bool {
    use pyreal_client_runtime::shell::Shell as _;
    c.view().expect_app().shell().has_ui()
}

/// Every chat line the client composed from what this scenario delivered, joined, with the
/// clickable-name markup taken off as the log takes it off.
pub(super) fn composed_text(c: &HeadlessClient) -> String {
    without_tell_markup(&c.view().chat_text().join("\n"))
}

/// `body` without the clickable-name markup around a name.
fn without_tell_markup(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        if let Some(close) = after.find('>') {
            let tag = &after[1..close];
            if tag.starts_with("Tell:") || tag == "\\Tell" {
                rest = &after[close + 1..];
                continue;
            }
        }
        out.push('<');
        rest = &after[1..];
    }
    out.push_str(rest);
    out
}

pub(super) fn word(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}

/// A client in the world with its player described, ready to be spoken to.
pub(super) fn a_client_listening() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    c.when(Inbound::event(SessionEvent::PlayerDescription(
        Box::default(),
    )));
    c.tick(1);
    c
}

/// One message, through the production sender over a mock transport. **Nothing is sent.**
pub(super) fn on_the_wire<M: pyreal_protocol::Message>(m: &M) -> (NetQueue, Vec<u8>) {
    let mut s = Session::new(MockTransport::new());
    s.send_action(m).expect("the message encodes");
    assert_eq!(s.transport.sent.len(), 1, "one action, one datagram");
    let b = &s.transport.sent[0];
    (b.queue, b.payload.clone())
}
