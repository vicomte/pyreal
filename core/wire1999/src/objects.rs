//! The 1999 object create (`0xF745`) and its three descriptions, read into Pyreal's later structs.
//!
//! Appearance ids are fixed u16 offsets from their type's base, physics has no translucency, and the
//! public description has fixed u16 ids, 1999 strings, a one-byte ammo type and the vendor class
//! where the later wire has burden. The 1999 server re-sends a create to update an object, keeping
//! its instance stamp, so Pyreal's own equal-instance merge applies and the create passes as
//! `0xF745`.

use std::collections::BTreeMap;

use pyreal_primitives::text::cp1252;
use pyreal_primitives::ObjectId;
use pyreal_protocol::objects::{is_newer, ItemCreateObject, ObjectCreatePayload};
use pyreal_protocol::types::objdesc::{MODEL_BASE, PALETTE_BASE, TEXTURE_BASE};
use pyreal_protocol::types::physicsdesc::flags as physics;
use pyreal_protocol::types::weeniedesc::{header, ICON_BASE};
use pyreal_protocol::types::{
    AnimPartChange, ObjDesc, PhysicsDesc, PublicWeenieDesc, Subpalette, TextureMapChange, Vec3,
};
use pyreal_protocol::{MessageError, Reader};

use crate::stamps::Stamps;
#[cfg(doc)]
use crate::strings;
use crate::strings::ReadError;

/// `0xF745`.
pub const OBJECT_CREATE: u32 = 0xF745;

/// Physics flags the 1999 client reads a field for: the low 18 bits but the timestamp flag. The
/// nine timestamps are always there; translucency is not.
pub const PHYSICS_FLAGS_1999: u32 = 0x0003_FFFF & !physics::TIMESTAMPS;
/// Header bits the 1999 client has a field for: `0x1` to `0x1000000` but monarch `0x40`.
pub const HEADER_1999: u32 = 0x01FF_FFBF;
/// Header bit `0x200000`: on this wire the vendor class that issued a trade note, not burden.
pub const VENDOR_CLASS: u32 = header::BURDEN;
/// Description flags the 1999 server sets; `0xFC000000` (the second header among them) never.
pub const DESCRIPTION_FLAGS_1999: u32 = 0x03FF_FFFF;

/// Map a later reader's error onto this crate's.
pub(crate) fn read_error(e: MessageError) -> ReadError {
    match e {
        MessageError::UnexpectedEof { .. } | MessageError::LengthOverrun { .. } => ReadError::Short,
        MessageError::TrailingBytes { .. } => ReadError::Bad("trailing bytes"),
        _ => ReadError::Bad("field"),
    }
}

/// A 1999 string at `r`, by the rules of [`strings::Reader::string`]: a u16 count including one
/// NUL, the bytes, then zero padding counted from the opcode (the reader's blob offset). Empty is
/// the bare count.
///
/// # Errors
/// [`ReadError`] for a short message, a misplaced or missing NUL, or non-zero padding.
pub fn read_string(r: &mut Reader<'_>) -> Result<String, ReadError> {
    let len = usize::from(r.u16().map_err(read_error)?);
    if len == 0 {
        return Ok(String::new());
    }
    let bytes = r.bytes(len).map_err(read_error)?;
    let (&nul, text) = bytes.split_last().ok_or(ReadError::Short)?;
    if nul != 0 || text.contains(&0) {
        return Err(ReadError::Bad("string NUL"));
    }
    let pad = r.blob_offset().wrapping_neg() & 3;
    if r.bytes(pad).map_err(read_error)?.iter().any(|&b| b != 0) {
        return Err(ReadError::Bad("string padding"));
    }
    Ok(cp1252::decode(text))
}

/// A fixed u16 asset id with its type's base restored.
fn asset(r: &mut Reader<'_>, base: u32) -> Result<u32, ReadError> {
    Ok(base | u32::from(r.u16().map_err(read_error)?))
}

/// The 1999 appearance: counts, then fixed u16 ids where the later wire packs them.
///
/// # Errors
/// [`ReadError`] for a short message or a version other than `0x11`.
pub fn read_objdesc(r: &mut Reader<'_>) -> Result<ObjDesc, ReadError> {
    let mut b = || r.u8().map_err(read_error);
    let (version, palettes, textures, models) = (b()?, b()?, b()?, b()?);
    if version != ObjDesc::VERSION {
        return Err(ReadError::Bad("appearance version"));
    }
    let mut d = ObjDesc::default();
    if palettes > 0 {
        d.palette_id = asset(r, PALETTE_BASE)?;
        for _ in 0..palettes {
            d.subpalettes.push(Subpalette {
                sub_id: asset(r, PALETTE_BASE)?,
                offset: r.u8().map_err(read_error)?,
                num_colors: r.u8().map_err(read_error)?,
            });
        }
    }
    for _ in 0..textures {
        d.texture_changes.push(TextureMapChange {
            part_index: r.u8().map_err(read_error)?,
            old_tex_id: asset(r, TEXTURE_BASE)?,
            new_tex_id: asset(r, TEXTURE_BASE)?,
        });
    }
    for _ in 0..models {
        d.anim_part_changes.push(AnimPartChange {
            part_index: r.u8().map_err(read_error)?,
            part_id: asset(r, MODEL_BASE)?,
        });
    }
    r.align4().map_err(read_error)?;
    Ok(d)
}

/// The 1999 physics description at `at` in `message`, and the offset after it.
///
/// Its layout is the later one read with only the flags the 1999 client honours, so the flags are
/// cleaned first (the animation frame too when a movement buffer is present) and Pyreal's own
/// reader does the rest.
///
/// # Errors
/// [`ReadError`] for a short message or a non-finite number.
pub fn read_physics(message: &[u8], at: usize) -> Result<(PhysicsDesc, usize), ReadError> {
    let mut copy = message.get(at..).ok_or(ReadError::Short)?.to_vec();
    let raw = copy.get(..4).ok_or(ReadError::Short)?;
    let mut bits = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) & PHYSICS_FLAGS_1999;
    if bits & physics::MOVEMENT != 0 {
        bits &= !physics::ANIMFRAME;
    }
    copy[..4].copy_from_slice(&bits.to_le_bytes());
    let mut r = Reader::with_origin(&copy, at);
    let d = PhysicsDesc::read(&mut r).map_err(read_error)?;
    if !physics_is_finite(&d) {
        return Err(ReadError::Bad("non-finite physics"));
    }
    Ok((d, at + r.position()))
}

fn physics_is_finite(d: &PhysicsDesc) -> bool {
    let v = |v: &Vec3| v.x.is_finite() && v.y.is_finite() && v.z.is_finite();
    let position = d.position.as_ref().is_none_or(|p| {
        let q = p.frame.orientation;
        v(&p.frame.origin) && [q.w, q.x, q.y, q.z].iter().all(|f| f.is_finite())
    });
    let scalars = [
        d.object_scale,
        d.friction,
        d.elasticity,
        d.default_script_intensity,
    ]
    .iter()
    .flatten()
    .all(|f| f.is_finite());
    let vectors = [d.velocity, d.acceleration, d.omega]
        .iter()
        .flatten()
        .all(v);
    position && scalars && vectors
}

/// The 1999 public description at `r`, and the vendor class when bit `0x200000` carried one.
///
/// # Errors
/// [`ReadError`] for a short message or a bad string.
pub fn read_public_desc(r: &mut Reader<'_>) -> Result<(PublicWeenieDesc, Option<u16>), ReadError> {
    let raw = r.u32().map_err(read_error)?;
    let h = raw & HEADER_1999;
    if h != raw {
        tracing::debug!(
            "1999: header bits {:#010X} have no field",
            raw & !HEADER_1999
        );
    }
    let name = read_string(r)?;
    // Fixed u16s, and no align after them: an empty name leaves the u32s below unaligned.
    let wcid = u32::from(r.u16().map_err(read_error)?);
    let icon_id = asset(r, ICON_BASE)?;
    let obj_type = r.u32().map_err(read_error)?;
    let bitfield = r.u32().map_err(read_error)? & DESCRIPTION_FLAGS_1999;
    let mut d = PublicWeenieDesc {
        header: h & !VENDOR_CLASS,
        name,
        wcid,
        icon_id,
        obj_type,
        bitfield,
        ..PublicWeenieDesc::default()
    };
    let has = |b: u32| h & b != 0;
    macro_rules! opt {
        ($bit:expr, $read:expr) => {
            if has($bit) {
                Some($read.map_err(read_error)?)
            } else {
                None
            }
        };
    }
    if has(header::PLURAL_NAME) {
        d.plural_name = Some(read_string(r)?);
    }
    d.items_capacity = opt!(header::ITEMS_CAPACITY, r.u8());
    d.containers_capacity = opt!(header::CONTAINERS_CAPACITY, r.u8());
    d.value = opt!(header::VALUE, r.u32());
    d.useability = opt!(header::USEABILITY, r.u32());
    d.use_radius = opt!(header::USE_RADIUS, r.f32());
    d.target_type = opt!(header::TARGET_TYPE, r.u32());
    d.effects = opt!(header::EFFECTS, r.u32());
    d.ammo_type = opt!(header::AMMO_TYPE, r.u8()).map(u16::from);
    d.combat_use = opt!(header::COMBAT_USE, r.u8());
    d.structure = opt!(header::STRUCTURE, r.u16());
    d.max_structure = opt!(header::MAX_STRUCTURE, r.u16());
    d.stack_size = opt!(header::STACK_SIZE, r.u16());
    d.max_stack_size = opt!(header::MAX_STACK_SIZE, r.u16());
    d.container_id = opt!(header::CONTAINER_ID, r.u32()).map(ObjectId);
    d.wielder_id = opt!(header::WIELDER_ID, r.u32()).map(ObjectId);
    d.valid_locations = opt!(header::VALID_LOCATIONS, r.u32());
    d.location = opt!(header::LOCATION, r.u32());
    d.priority = opt!(header::PRIORITY, r.u32());
    d.blip_color = opt!(header::BLIP_COLOR, r.u8());
    d.radar_enum = opt!(header::RADAR_ENUM, r.u8());
    d.workmanship = opt!(header::WORKMANSHIP, r.f32());
    let vendor_class = opt!(VENDOR_CLASS, r.u16());
    d.spell_id = opt!(header::SPELL_ID, r.u16());
    r.align4().map_err(read_error)?;
    Ok((d, vendor_class))
}

/// [`read_public_desc`] in the later reader's error type, for descriptions inside other messages
/// (vendor lists). The vendor class is left out.
///
/// # Errors
/// [`MessageError`] for a short message or a bad field.
pub fn read_description(r: &mut Reader<'_>) -> Result<PublicWeenieDesc, MessageError> {
    read_public_desc(r).map(|(d, _)| d).map_err(|e| match e {
        ReadError::Short => MessageError::UnexpectedEof {
            at: r.blob_offset(),
            needed: 1,
            available: r.remaining(),
        },
        ReadError::Bad(field) => MessageError::InvalidValue { field, value: 0 },
    })
}

/// One read `0xF745`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Create1999 {
    pub payload: ObjectCreatePayload,
    /// Header bit `0x200000`'s value: the vendor class that issued a trade note.
    pub vendor_class: Option<u16>,
}

impl Create1999 {
    /// Read a whole `0xF745` (or the same body under another opcode), opcode included.
    ///
    /// # Errors
    /// [`ReadError`] for a short message, an id of 0, a bad field or anything left over.
    pub fn read(message: &[u8]) -> Result<Self, ReadError> {
        let mut r = Reader::new(message);
        r.u32().map_err(read_error)?;
        let id = r.u32().map_err(read_error)?;
        if id == 0 {
            return Err(ReadError::Bad("object id 0"));
        }
        let objdesc = read_objdesc(&mut r)?;
        let (physicsdesc, at) = read_physics(message, r.position())?;
        let mut r = Reader::with_origin(&message[at..], at);
        let (wdesc, vendor_class) = read_public_desc(&mut r)?;
        r.expect_exhausted().map_err(read_error)?;
        Ok(Self {
            payload: ObjectCreatePayload {
                id: ObjectId(id),
                objdesc,
                physicsdesc,
                wdesc,
            },
            vendor_class,
        })
    }
}

/// The objects the layer has seen created: instance stamps and the vendor classes kept beside them.
#[derive(Debug, Clone, Default)]
pub struct Objects {
    instances: BTreeMap<u32, u16>,
    vendor_classes: BTreeMap<u32, u16>,
}

impl Objects {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Translate a `0xF745`. A create with a newer instance stamp than the last one for its id
    /// replaces the object in Pyreal, which forgets its windows, so the stamps forget it too.
    ///
    /// # Errors
    /// [`ReadError`] as [`Create1999::read`].
    pub fn create(
        &mut self,
        stamps: &mut Stamps,
        message: &[u8],
    ) -> Result<ItemCreateObject, ReadError> {
        let c = Create1999::read(message)?;
        let id = c.payload.id.0;
        let instance = c.payload.physicsdesc.timestamps.instance;
        if let Some(&known) = self.instances.get(&id) {
            if is_newer(known, instance) {
                stamps.forget_object(id);
            }
        }
        self.instances.insert(id, instance);
        match c.vendor_class {
            Some(v) => self.vendor_classes.insert(id, v),
            None => self.vendor_classes.remove(&id),
        };
        Ok(ItemCreateObject(c.payload))
    }

    /// A `0xF747` for `id` at `instance`. One older than the object Pyreal holds is ignored there,
    /// so it is here.
    pub fn delete(&mut self, stamps: &mut Stamps, id: u32, instance: u16) {
        if self
            .instances
            .get(&id)
            .is_some_and(|&known| is_newer(instance, known))
        {
            return;
        }
        self.instances.remove(&id);
        self.vendor_classes.remove(&id);
        stamps.forget_object(id);
    }

    /// Whether a create for `id` has come since the last enter-world and not been deleted.
    #[must_use]
    pub fn knows(&self, id: u32) -> bool {
        self.instances.contains_key(&id)
    }

    /// The vendor class a trade note's create carried.
    #[must_use]
    pub fn vendor_class(&self, id: u32) -> Option<u16> {
        self.vendor_classes.get(&id).copied()
    }

    /// The server is about to make every object again.
    pub fn enter_world(&mut self) {
        self.instances.clear();
        self.vendor_classes.clear();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use pyreal_protocol::types::physicsdesc::PhysicsTimestamps;
    use pyreal_protocol::types::{Frame, PositionWire, Quat};
    use pyreal_protocol::{read_body, write_blob, Writer};

    /// A 1999-form message writer for the tests, aligned from the opcode.
    pub(crate) struct W(pub Writer);

    impl W {
        pub(crate) fn new(opcode: u32) -> Self {
            let mut w = Writer::new();
            w.u32(opcode);
            Self(w)
        }
        pub(crate) fn string(&mut self, s: &str) -> &mut Self {
            let bytes = cp1252::encode_lossy(s);
            if bytes.is_empty() {
                self.0.u16(0);
                return self;
            }
            self.0.u16(u16::try_from(bytes.len() + 1).unwrap());
            self.0.bytes(&bytes);
            self.0.u8(0);
            self.0.align4();
            self
        }
    }

    /// A 1999 appearance: base palette 0x7E, one subpalette 0x8123 (a high-bit id), one texture
    /// change and one model change.
    pub(crate) fn objdesc_1999(w: &mut Writer) {
        w.bytes(&[0x11, 1, 1, 1]);
        w.u16(0x007E);
        w.u16(0x8123);
        w.bytes(&[3, 2]);
        w.u8(4);
        w.u16(0x0100);
        w.u16(0x9001);
        w.u8(5);
        w.u16(0xA0B0);
        w.align4();
    }

    pub(crate) fn objdesc_later() -> ObjDesc {
        ObjDesc {
            palette_id: 0x0400_007E,
            subpalettes: vec![Subpalette {
                sub_id: 0x0400_8123,
                offset: 3,
                num_colors: 2,
            }],
            texture_changes: vec![TextureMapChange {
                part_index: 4,
                old_tex_id: 0x0500_0100,
                new_tex_id: 0x0500_9001,
            }],
            anim_part_changes: vec![AnimPartChange {
                part_index: 5,
                part_id: 0x0100_A0B0,
            }],
        }
    }

    /// A physics block: position, setup and scale, with translucency, the timestamp flag and a
    /// later high bit set but no fields for them, as a 1999 reader sees it.
    fn physics_1999(w: &mut Writer, instance: u16) {
        w.u32(
            physics::POSITION
                | physics::SETUP
                | physics::OBJSCALE
                | physics::TRANSLUCENCY
                | physics::TIMESTAMPS
                | 0x0100_0000,
        );
        w.u32(0x0000_0408);
        w.u32(0xA9B4_0031);
        for f in [12.0f32, 34.0, 56.0, 1.0, 0.0, 0.0, 0.0] {
            w.f32(f);
        }
        w.u32(0x0200_0001);
        w.f32(1.5);
        for t in [1u16, 2, 3, 4, 5, 6, 7, 8, instance] {
            w.u16(t);
        }
        w.align4();
    }

    fn create_1999(name: &str, instance: u16) -> Vec<u8> {
        let mut m = W::new(OBJECT_CREATE);
        m.0.u32(0x7A9B_4001);
        objdesc_1999(&mut m.0);
        physics_1999(&mut m.0, instance);
        // Header: value, target type, effects, ammo, stack, vendor class, spell, and a later
        // house-owner bit the 1999 client has no field for.
        let h = header::VALUE
            | header::TARGET_TYPE
            | header::EFFECTS
            | header::AMMO_TYPE
            | header::STACK_SIZE
            | header::BLIP_COLOR
            | header::RADAR_ENUM
            | VENDOR_CLASS
            | header::SPELL_ID
            | header::HOUSE_OWNER;
        m.0.u32(h);
        m.string(name);
        m.0.u16(0x8001);
        m.0.u16(0x1234);
        m.0.u32(0x0000_0080);
        m.0.u32(0x0400_0010);
        m.0.u32(25);
        m.0.u32(0x0000_0001);
        m.0.u32(0x0000_0002);
        m.0.u8(4);
        m.0.u16(1);
        m.0.u8(8);
        m.0.u8(2);
        m.0.u16(0x00C7);
        m.0.u16(0x0101);
        m.0.align4();
        m.0.into_inner()
    }

    #[test]
    fn a_1999_create_becomes_the_later_create_with_full_ids_and_clean_flags() {
        let c = Create1999::read(&create_1999("Trade Note", 3)).unwrap();
        let p = &c.payload;
        assert_eq!(p.id, ObjectId(0x7A9B_4001));
        assert_eq!(p.objdesc, objdesc_later());

        let ph = &p.physicsdesc;
        assert_eq!(
            ph.bitfield,
            physics::POSITION | physics::SETUP | physics::OBJSCALE
        );
        assert_eq!(ph.translucency, None);
        assert_eq!(ph.setup_id, Some(0x0200_0001));
        assert_eq!(ph.object_scale, Some(1.5));
        assert_eq!(
            ph.position,
            Some(PositionWire {
                objcell_id: 0xA9B4_0031,
                frame: Frame {
                    origin: Vec3 {
                        x: 12.0,
                        y: 34.0,
                        z: 56.0
                    },
                    orientation: Quat {
                        w: 1.0,
                        x: 0.0,
                        y: 0.0,
                        z: 0.0
                    },
                },
            })
        );
        assert_eq!(
            ph.timestamps,
            PhysicsTimestamps {
                position: 1,
                movement: 2,
                state: 3,
                vector: 4,
                teleport: 5,
                server_controlled_move: 6,
                force_position: 7,
                objdesc: 8,
                instance: 3,
            }
        );

        let d = &p.wdesc;
        assert_eq!(d.name, "Trade Note");
        assert_eq!(d.wcid, 0x8001, "a fixed u16, the high bit included");
        assert_eq!(d.icon_id, 0x0600_1234);
        assert_eq!(d.bitfield, 0x0000_0010, "the second-header bit cleared");
        assert_eq!(d.header2, None);
        assert_eq!(
            (d.value, d.target_type, d.effects, d.ammo_type),
            (Some(25), Some(1), Some(2), Some(4))
        );
        assert_eq!(
            (d.stack_size, d.blip_color, d.radar_enum, d.spell_id),
            (Some(1), Some(8), Some(2), Some(0x0101))
        );
        assert_eq!(d.burden, None);
        assert_eq!(c.vendor_class, Some(0x00C7));
        assert_eq!(d.header & (VENDOR_CLASS | header::HOUSE_OWNER), 0);

        // What Pyreal receives reads back the same through the later codec.
        let blob = write_blob(&ItemCreateObject(p.clone())).unwrap();
        let back: ItemCreateObject = read_body(&blob[4..]).unwrap();
        assert_eq!(back.0, *p);
    }

    #[test]
    fn an_empty_name_leaves_the_fields_after_it_unaligned() {
        let c = Create1999::read(&create_1999("", 3)).unwrap();
        assert_eq!(c.payload.wdesc.name, "");
        assert_eq!(c.payload.wdesc.icon_id, 0x0600_1234);
        assert_eq!(c.payload.wdesc.spell_id, Some(0x0101));
    }

    #[test]
    fn a_create_must_be_consumed_exactly_and_name_an_object() {
        let mut m = create_1999("Ann", 3);
        m.extend_from_slice(&[1, 0, 0, 0]);
        assert!(Create1999::read(&m).is_err());
        let mut m = create_1999("Ann", 3);
        m[4..8].copy_from_slice(&[0; 4]);
        assert_eq!(Create1999::read(&m), Err(ReadError::Bad("object id 0")));
        let m = create_1999("Ann", 3);
        assert_eq!(Create1999::read(&m[..m.len() - 4]), Err(ReadError::Short));
    }

    #[test]
    fn a_movement_buffer_hides_the_animation_frame_and_length_zero_has_no_autonomy() {
        let mut w = Writer::new();
        w.u32(0xF745);
        w.u32(physics::MOVEMENT | physics::ANIMFRAME);
        w.u32(0);
        w.u32(0);
        for t in 0..9u16 {
            w.u16(t);
        }
        w.align4();
        let m = w.into_inner();
        let (d, end) = read_physics(&m, 4).unwrap();
        assert_eq!(end, m.len());
        assert_eq!(d.bitfield, physics::MOVEMENT);
        assert_eq!(d.movement, Some((Vec::new(), 0)));
        assert_eq!(d.animframe_id, None);
        assert_eq!(d.timestamps.instance, 8);
    }

    #[test]
    fn a_non_finite_scale_is_refused() {
        let mut w = Writer::new();
        w.u32(0xF745);
        w.u32(physics::OBJSCALE);
        w.u32(0);
        w.f32(f32::NAN);
        for _ in 0..9 {
            w.u16(0);
        }
        w.align4();
        assert!(read_physics(&w.into_inner(), 4).is_err());
    }

    #[test]
    fn a_re_create_with_a_newer_instance_restarts_the_objects_stamps_and_an_equal_one_does_not() {
        let mut s = Stamps::new();
        let mut o = Objects::new();
        let id = 0x7A9B_4001;
        o.create(&mut s, &create_1999("Ann", 3)).unwrap();
        s.next_event(id);
        o.create(&mut s, &create_1999("Ann", 3)).unwrap();
        assert_eq!(s.next_event(id), 2, "an update keeps the window");
        o.create(&mut s, &create_1999("Ann", 4)).unwrap();
        assert_eq!(s.next_event(id), 1, "a new instance starts again");
        assert_eq!(o.vendor_class(id), Some(0x00C7));

        o.delete(&mut s, id, 3);
        assert!(o.knows(id), "an older delete is ignored");
        o.delete(&mut s, id, 4);
        assert!(!o.knows(id));
        assert_eq!(o.vendor_class(id), None);
        assert_eq!(s.next_event(id), 1);
    }

    #[test]
    fn the_description_reader_takes_a_description_mid_message() {
        let m = create_1999("Ann", 3);
        let mut r = Reader::new(&m);
        r.u32().unwrap();
        r.u32().unwrap();
        read_objdesc(&mut r).unwrap();
        let (_, start) = read_physics(&m, r.position()).unwrap();
        let mut r = Reader::with_origin(&m[start..], start);
        let d = read_description(&mut r).unwrap();
        assert_eq!(d.name, "Ann");
        assert_eq!(r.remaining(), 0);
        let mut r = Reader::with_origin(&m[start..m.len() - 4], start);
        assert!(matches!(
            read_description(&mut r),
            Err(MessageError::UnexpectedEof { .. })
        ));
    }

    #[test]
    fn a_string_counts_its_nul_and_refuses_one_inside_or_dirty_padding() {
        let mut m = W::new(0);
        m.string("Ab");
        let bytes = m.0.into_inner();
        assert_eq!(&bytes[4..], &[3, 0, b'A', b'b', 0, 0, 0, 0]);
        let mut r = Reader::new(&bytes);
        r.u32().unwrap();
        assert_eq!(read_string(&mut r).unwrap(), "Ab");
        assert_eq!(r.remaining(), 0);
        for (bad, why) in [
            ([0, 0, 0, 0, 3, 0, b'A', 0, 0, 0, 0, 0], "string NUL"),
            ([0, 0, 0, 0, 3, 0, b'A', b'b', 0, 0, 7, 0], "string padding"),
        ] {
            let mut r = Reader::new(&bad);
            r.u32().unwrap();
            assert_eq!(read_string(&mut r), Err(ReadError::Bad(why)));
        }
    }
}
