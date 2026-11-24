//! The world stream: creates, identity, deletes, positions, appearance, sound and scripts.
//!
//! Most of it has the later layout and passes on its own queue. The differences: a create's three
//! descriptions ([`crate::objects`]), appearance events in the 1999 appearance form, and position
//! packs, where a missing quaternion w means 0 rather than the later 1.

use pyreal_primitives::{IncomingMessage, NetBlobId, NetQueue, ObjectId, RecipientId};
use pyreal_protocol::movement::{
    position_flags, MovementPositionAndMovementEvent, MovementPositionEvent, PositionPack,
};
use pyreal_protocol::objects::{ItemDeleteObject, ItemObjDescEvent, LoginCreatePlayer};
use pyreal_protocol::types::PhysicsEventStamp;
use pyreal_protocol::{read_body_padded, Message, Opcode, Reader};

use crate::objects::{read_error, read_objdesc, Objects, OBJECT_CREATE};
use crate::stamps::Stamps;
use crate::strings::ReadError;
use crate::translate::{Context, Translate};

/// `0xF746`, the player's own id.
pub const IDENTITY: u32 = 0xF746;
/// `0xF747`.
pub const DELETE: u32 = 0xF747;
/// `0xF748`.
pub const POSITION: u32 = 0xF748;
/// `0xF619`.
pub const POSITION_AND_MOVEMENT: u32 = 0xF619;
/// `0xF625`.
pub const APPEARANCE: u32 = 0xF625;
/// `0xF7DB`: no 1999 client reads it. AC1999 writes it for crafting and admin stat changes with a
/// 1999 appearance and later physics and public descriptions.
pub const UPDATE: u32 = 0xF7DB;
/// Messages with the later layout: parent, pickup, state, movement, vector, sound, teleport and the
/// two scripts.
pub const PASSED: [u32; 9] = [
    0xF749, 0xF74A, 0xF74B, 0xF74C, 0xF74E, 0xF750, 0xF751, 0xF754, 0xF755,
];

/// Whether [`World::translate`] takes this opcode.
#[must_use]
pub fn handles(opcode: u32) -> bool {
    matches!(
        opcode,
        OBJECT_CREATE | IDENTITY | DELETE | POSITION | POSITION_AND_MOVEMENT | APPEARANCE | UPDATE
    ) || PASSED.contains(&opcode)
}

/// The world stream's state for one connection.
#[derive(Debug, Clone, Default)]
pub struct World {
    pub objects: Objects,
}

impl World {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The enter-world request went out: every object comes again.
    pub fn enter_world(&mut self) {
        self.objects.enter_world();
    }

    /// Translate one world-stream message, opcode included. `None` is a message the 1999 client
    /// would not act on.
    ///
    /// # Errors
    /// [`ReadError`] for a message that does not read or does not encode.
    pub fn translate(
        &mut self,
        stamps: &mut Stamps,
        message: &[u8],
    ) -> Result<Option<IncomingMessage>, ReadError> {
        let opcode = word(message, 0).ok_or(ReadError::Short)?;
        let body = &message[4..];
        let out = match opcode {
            OBJECT_CREATE => {
                let m = self.objects.create(stamps, message)?;
                deliver(stamps, &m)?
            }
            IDENTITY => {
                let m: LoginCreatePlayer = read_body_padded(body).map_err(read_error)?;
                let id = m.player_id.0;
                if id == 0 {
                    return Err(ReadError::Bad("player id 0"));
                }
                match stamps.player() {
                    0 => stamps.set_player(id),
                    known if known == id => {}
                    known => {
                        tracing::warn!("1999: a second identity {id:#010X} after {known:#010X}");
                        return Ok(None);
                    }
                }
                deliver(stamps, &m)?
            }
            DELETE => {
                let m: ItemDeleteObject = read_body_padded(body).map_err(read_error)?;
                self.objects.delete(stamps, m.id.0, m.instance_sequence);
                deliver(stamps, &m)?
            }
            POSITION => {
                let mut m: MovementPositionEvent = read_body_padded(body).map_err(read_error)?;
                restate_w(&mut m.position);
                deliver(stamps, &m)?
            }
            POSITION_AND_MOVEMENT => {
                let mut m: MovementPositionAndMovementEvent =
                    read_body_padded(body).map_err(read_error)?;
                restate_w(&mut m.position);
                deliver(stamps, &m)?
            }
            APPEARANCE => deliver(stamps, &read_appearance(message)?)?,
            UPDATE => {
                tracing::debug!("1999: 0xF7DB dropped, as the 1999 client does");
                return Ok(None);
            }
            op if PASSED.contains(&op) => pass(op, body),
            _ => return Err(ReadError::Bad("not a world-stream message")),
        };
        Ok(Some(out))
    }
}

impl Translate for World {
    fn down(
        &mut self,
        opcode: u32,
        _queue: u16,
        message: &[u8],
        cx: &mut Context<'_>,
    ) -> Result<bool, ReadError> {
        if !handles(opcode) {
            return Ok(false);
        }
        if let Some(m) = self.translate(&mut cx.shared.stamps, message)? {
            cx.inbox.push_back(m);
        }
        Ok(true)
    }

    fn up(
        &mut self,
        _opcode: Opcode,
        _queue: NetQueue,
        _ordered: bool,
        _payload: &[u8],
        _cx: &mut Context<'_>,
    ) -> bool {
        false
    }

    fn entering_world(&mut self) {
        self.enter_world();
    }
}

fn word(message: &[u8], at: usize) -> Option<u32> {
    message
        .get(at..at + 4)
        .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
}

fn deliver<M: Message>(stamps: &mut Stamps, m: &M) -> Result<IncomingMessage, ReadError> {
    stamps.deliver(m).map_err(|e| {
        tracing::warn!("1999: {:#06X} did not encode: {e}", M::OPCODE.0);
        ReadError::Bad("encode")
    })
}

/// A message whose 1999 bytes are the later ones, on the queue Pyreal drains it from.
fn pass(opcode: u32, body: &[u8]) -> IncomingMessage {
    let queue = Opcode(opcode)
        .info()
        .and_then(|i| i.recv_queue)
        .unwrap_or(NetQueue::WorldObjects);
    IncomingMessage {
        opcode,
        queue,
        sender: RecipientId(0),
        blob_id: NetBlobId(0),
        body: body.to_vec(),
    }
}

/// The 1999 client reads a missing w as 0 where the later reader makes it 1: state it.
pub fn restate_w(p: &mut PositionPack) {
    if p.flags & position_flags::ORIENTATION_HAS_NO_W != 0 {
        p.orientation.w = 0.0;
        p.flags &= !position_flags::ORIENTATION_HAS_NO_W;
    }
}

/// `0xF625`: the id, the appearance in the 1999 form, then the later instance and appearance stamps.
///
/// # Errors
/// [`ReadError`] for a short message, a bad appearance or anything left over.
pub fn read_appearance(message: &[u8]) -> Result<ItemObjDescEvent, ReadError> {
    let mut r = Reader::new(message);
    r.u32().map_err(read_error)?;
    let id = ObjectId(r.u32().map_err(read_error)?);
    let objdesc = read_objdesc(&mut r)?;
    let timestamps = PhysicsEventStamp::read(&mut r).map_err(read_error)?;
    r.expect_exhausted().map_err(read_error)?;
    Ok(ItemObjDescEvent {
        id,
        objdesc,
        timestamps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::tests::{objdesc_1999, objdesc_later, W};
    use pyreal_protocol::read_body;
    use pyreal_protocol::types::{Quat, Vec3};
    use pyreal_protocol::Writer;

    fn words(ws: &[u32]) -> Vec<u8> {
        ws.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// A `0xF748` as AC1999 writes it for an object facing 180 degrees: w and x and y zero, so
    /// only z is sent; contact set.
    fn position_1999() -> Vec<u8> {
        let mut w = Writer::new();
        w.u32(POSITION);
        w.u32(0x5000_0001);
        w.u32(0x08 | 0x10 | 0x20 | position_flags::IS_GROUNDED);
        w.u32(0xA9B4_0031);
        for f in [10.0f32, 20.0, 30.0, 1.0] {
            w.f32(f);
        }
        for t in [3u16, 9, 1, 2] {
            w.u16(t);
        }
        w.into_inner()
    }

    #[test]
    fn a_missing_w_is_stated_as_zero_and_the_rest_passes() {
        let mut world = World::new();
        let mut s = Stamps::new();
        let out = world.translate(&mut s, &position_1999()).unwrap().unwrap();
        assert_eq!((out.opcode, out.queue), (POSITION, NetQueue::WorldObjects));
        let m: MovementPositionEvent = read_body(&out.body).unwrap();
        assert_eq!(
            m.position.orientation,
            Quat {
                w: 0.0,
                x: 0.0,
                y: 0.0,
                z: 1.0
            }
        );
        assert_eq!(m.position.flags & position_flags::ORIENTATION_HAS_NO_W, 0);
        assert!(m.position.has_contact());
        assert_eq!(
            m.position.origin.origin,
            Vec3 {
                x: 10.0,
                y: 20.0,
                z: 30.0
            }
        );
        assert_eq!(
            (m.position.instance_timestamp, m.position.position_timestamp),
            (3, 9)
        );
    }

    #[test]
    fn position_and_movement_restates_w_and_keeps_the_movement_bytes() {
        let mut msg = position_1999();
        msg[..4].copy_from_slice(&POSITION_AND_MOVEMENT.to_le_bytes());
        let tail = [4u8, 0, 2, 0, 1, 0, 0, 0, 0xAA, 0xBB, 0xCC, 0xDD];
        msg.extend_from_slice(&tail);
        let out = World::new()
            .translate(&mut Stamps::new(), &msg)
            .unwrap()
            .unwrap();
        let m: MovementPositionAndMovementEvent = read_body(&out.body).unwrap();
        assert_eq!(m.position.orientation.w, 0.0);
        assert_eq!(m.movement, tail);
    }

    #[test]
    fn identity_sets_the_player_once_and_a_different_one_is_dropped() {
        let mut world = World::new();
        let mut s = Stamps::new();
        let id = words(&[IDENTITY, 0x5000_0001]);
        let out = world.translate(&mut s, &id).unwrap().unwrap();
        assert_eq!((out.opcode, out.body.as_slice()), (IDENTITY, &id[4..]));
        assert_eq!(s.player(), 0x5000_0001);
        assert!(
            world.translate(&mut s, &id).unwrap().is_some(),
            "the same again"
        );
        let other = words(&[IDENTITY, 0x5000_0002]);
        assert!(world.translate(&mut s, &other).unwrap().is_none());
        assert_eq!(s.player(), 0x5000_0001);
        assert!(world.translate(&mut s, &words(&[IDENTITY, 0])).is_err());
    }

    #[test]
    fn appearance_reads_the_1999_form_and_keeps_both_stamps() {
        let mut m = W::new(APPEARANCE);
        m.0.u32(0x5000_0001);
        objdesc_1999(&mut m.0);
        m.0.u16(7);
        m.0.u16(12);
        m.0.align4();
        let out = World::new()
            .translate(&mut Stamps::new(), &m.0.into_inner())
            .unwrap()
            .unwrap();
        assert_eq!(out.opcode, APPEARANCE);
        let e: ItemObjDescEvent = read_body(&out.body).unwrap();
        assert_eq!(e.id, ObjectId(0x5000_0001));
        assert_eq!(e.objdesc, objdesc_later());
        assert_eq!((e.timestamps.instance, e.timestamps.event), (7, 12));
    }

    #[test]
    fn delete_passes_and_forgets_the_object_and_update_is_dropped() {
        let mut world = World::new();
        let mut s = Stamps::new();
        s.next_event(0x7000_0001);
        let del = [&words(&[DELETE, 0x7000_0001])[..], &[2, 0, 0, 0]].concat();
        let out = world.translate(&mut s, &del).unwrap().unwrap();
        assert_eq!((out.opcode, out.body.as_slice()), (DELETE, &del[4..]));
        assert_eq!(s.next_event(0x7000_0001), 1);
        assert!(world
            .translate(&mut s, &words(&[UPDATE, 1, 2]))
            .unwrap()
            .is_none());
    }

    #[test]
    fn the_later_layout_ones_pass_untouched_on_their_queue() {
        let mut world = World::new();
        let mut s = Stamps::new();
        for op in PASSED {
            let m = words(&[op, 0x5000_0001, 7]);
            let out = world.translate(&mut s, &m).unwrap().unwrap();
            assert_eq!((out.opcode, out.body.as_slice()), (op, &m[4..]));
            assert_eq!(
                Some(out.queue),
                Opcode(op).info().and_then(|i| i.recv_queue),
                "{op:#06X}"
            );
            assert!(handles(op));
        }
        assert!(!handles(0xF7B0));
    }
}
