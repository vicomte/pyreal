use pyreal_client_net::client_session::SessionEvent;
use pyreal_primitives::ObjectId;
use pyreal_testkit::{ClientSpec, HeadlessClient, Inbound};

/// The body the shard enters the world as, before a scenario names its own character.
const ENTERED_AS: ObjectId = ObjectId(0x5290_0000);

/// A whole client in the world, over a loaded scene.
pub fn a_client_in_the_world() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let _shard = super::support::in_the_world(&mut c, ENTERED_AS);
    c
}

/// The character the shard's own updates are about. The weenie has to exist, because that is
/// the object the character's numbers live on.
pub fn this_character_is(c: &mut HeadlessClient, id: ObjectId) {
    let w = c.world_mut();
    w.player = None;
    w.tables
        .weenies
        .insert(id, pyreal_client_model::weenie::Weenie::new(id));
    assert!(w.set_player(id), "the identity is adopted once");
    c.hud_mut().player_desc_received = true;
}

/// One frame, which carries the mode change to the input manager.
pub fn settle(c: &mut HeadlessClient) {
    c.tick(1);
}

/// Which set of combat keys the client is carrying.
pub fn combat_keys(c: &mut HeadlessClient) -> u32 {
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .combat_input_mode()
}

/// One quality update, built the way the shard writes it: the private form when no subject is
/// named, and the public one when one is.
pub fn the_shard_says(
    subject: Option<ObjectId>,
    sequence: u8,
    property: u32,
    value: u32,
) -> Inbound {
    Inbound::event(update(subject, sequence, property, value))
}

/// The same message with its last byte missing, which is what an undecodable one is.
pub fn a_truncated_update(sequence: u8, property: u32, value: u32) -> Inbound {
    let mut e = update(None, sequence, property, value);
    if let SessionEvent::UiEvent { blob, .. } = &mut e {
        blob.pop();
    }
    Inbound::event(e)
}

fn update(subject: Option<ObjectId>, sequence: u8, property: u32, value: u32) -> SessionEvent {
    let opcode = if subject.is_some() {
        pyreal_protocol::Opcode::QUALITIES_UPDATE_INT
    } else {
        pyreal_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_INT
    };
    let mut blob = opcode.0.to_le_bytes().to_vec();
    blob.push(sequence);
    if let Some(id) = subject {
        blob.extend_from_slice(&id.0.to_le_bytes());
    }
    blob.extend_from_slice(&property.to_le_bytes());
    blob.extend_from_slice(&value.to_le_bytes());
    SessionEvent::UiEvent { opcode, blob }
}
