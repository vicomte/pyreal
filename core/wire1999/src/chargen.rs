//! Character creation (`0xF656`, answered by `0xF643`) and deletion (`0xF655`) on the 1999 wire.
//!
//! The 1999 creation carries the account as a fixed field, heritage and sex as 0-based indices
//! into the 1999 portal file's lists (Female first), a skill class per skill id, three heraldry
//! and access words the later message lacks, and no slot of its own.

use pyreal_primitives::{NetQueue, ObjectId};
use pyreal_protocol::login::{
    CharGenResult, CharGenVerificationResponse, CharacterDeleteRequest, CharacterIdentity,
    CharacterSendCharGenResult,
};
use pyreal_protocol::{read_body, Opcode};

use crate::login_area::LOGIN_QUEUE;
use crate::strings::{ReadError, Reader, Writer, ACCOUNT_MARKER};
use crate::translate::{Context, Translate};

/// `0xF656`.
pub const CREATE: u32 = 0xF656;
/// `0xF643`.
pub const ANSWER: u32 = 0xF643;
/// `0xF655`, the request one way and the empty acknowledgement the other.
pub const DELETE: u32 = 0xF655;

/// The answer code for a created character; only it carries an identity.
pub const CREATED: u32 = 1;
/// The answer code for a creation the server could not read, which the layer also gives for one
/// it cannot write.
pub const CORRUPT: u32 = 5;
/// The access level the stock 1999 client asks for; the server decides the real one.
pub const ACCESS_PLAYER: u32 = 1;
/// The heraldry symbol of a character whose heraldry page was skipped.
pub const NO_HERALDRY: i32 = -1;
/// Skill classes written: one per skill id up to the 1999 table's highest, 39.
pub const SKILL_COUNT: usize = 40;

/// Why a later creation has no 1999 form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unwritable {
    /// Not Aluvian, Gharu'ndim or Sho.
    Heritage(u32),
    /// Neither male nor female.
    Sex(u32),
}

/// The later heritage (Aluvian 1, Gharu'ndim 2, Sho 3) as the 1999 index.
#[must_use]
pub fn heritage_1999(later: u32) -> Option<u32> {
    (1..=3).contains(&later).then(|| later - 1)
}

/// The later sex (Male 1, Female 2) as the 1999 index (Female 0, Male 1).
#[must_use]
pub fn sex_1999(later: u32) -> Option<u32> {
    match later {
        1 => Some(1),
        2 => Some(0),
        _ => None,
    }
}

/// The closing word: the wrapping sum of heritage and sex (1999 values), the seven face values,
/// the three clothing styles, the template and the six attributes.
#[must_use]
pub fn closing_sum(r: &CharGenResult, heritage: u32, sex: u32) -> u32 {
    [
        r.eyes_strip,
        r.nose_strip,
        r.mouth_strip,
        r.hair_color,
        r.eye_color,
        r.hair_style,
        r.headgear_style,
        r.shirt_style,
        r.trousers_style,
        r.footwear_style,
        r.template_num,
        r.strength,
        r.endurance,
        r.coordination,
        r.quickness,
        r.focus,
        r.self_,
    ]
    .into_iter()
    .map(i32::cast_unsigned)
    .fold(heritage.wrapping_add(sex), u32::wrapping_add)
}

/// `0xF656` from the later creation, with the roster's `account`.
///
/// # Errors
/// [`Unwritable`] for a heritage or sex the 1999 lists do not have.
pub fn create(r: &CharGenResult, account: &str) -> Result<Vec<u8>, Unwritable> {
    let heritage = heritage_1999(r.heritage_group).ok_or(Unwritable::Heritage(r.heritage_group))?;
    let sex = sex_1999(r.gender).ok_or(Unwritable::Sex(r.gender))?;
    let int = |v: i32| v.cast_unsigned();
    let mut w = Writer::new(CREATE);
    w.account(ACCOUNT_MARKER, account)
        .u32(1)
        .u32(heritage)
        .u32(sex);
    for v in [
        r.eyes_strip,
        r.nose_strip,
        r.mouth_strip,
        r.hair_color,
        r.eye_color,
        r.hair_style,
        r.headgear_style,
    ] {
        w.u32(int(v));
    }
    w.u32(r.headgear_color)
        .u32(int(r.shirt_style))
        .u32(r.shirt_color)
        .u32(int(r.trousers_style))
        .u32(r.trousers_color)
        .u32(int(r.footwear_style))
        .u32(r.footwear_color);
    for shade in [
        r.skin_shade,
        r.hair_shade,
        r.headgear_shade,
        r.shirt_shade,
        r.trousers_shade,
        r.footwear_shade,
    ] {
        w.bytes(&shade.to_le_bytes());
    }
    for v in [
        r.template_num,
        r.strength,
        r.endurance,
        r.coordination,
        r.quickness,
        r.focus,
        r.self_,
    ] {
        w.u32(int(v));
    }
    // The 1999 client's target slot, then the access it asks for.
    w.u32(int(r.slot)).u32(ACCESS_PLAYER);
    w.u32(u32::try_from(SKILL_COUNT).unwrap_or(0));
    for id in 0..SKILL_COUNT {
        let class = r.skill_advancement_classes.get(id).copied().unwrap_or(0);
        w.u32(u32::try_from(class).ok().filter(|c| *c <= 3).unwrap_or(0));
    }
    w.string(&r.name)
        .u32(int(NO_HERALDRY))
        .u32(0)
        .u32(r.start_area)
        .u32(closing_sum(r, heritage, sex));
    Ok(w.into_inner())
}

/// `0xF643`: a code, and for [`CREATED`] the new character's id, name and deletion countdown.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Answer {
    pub code: u32,
    pub id: u32,
    pub name: String,
    pub deletion: u32,
}

impl Answer {
    /// # Errors
    /// [`ReadError`].
    pub fn read(message: &[u8]) -> Result<Self, ReadError> {
        let mut r = Reader::after_opcode(message);
        let code = r.u32()?;
        let mut a = Self {
            code,
            ..Self::default()
        };
        if code == CREATED {
            a.id = r.u32()?;
            a.name = r.string()?;
            a.deletion = r.u32()?;
        }
        r.finish()?;
        Ok(a)
    }

    #[must_use]
    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::new(ANSWER);
        w.u32(self.code);
        if self.code == CREATED {
            w.u32(self.id).string(&self.name).u32(self.deletion);
        }
        w.into_inner()
    }

    /// The later answer, the code passed through: AC1999's codes are the later table's.
    #[must_use]
    pub fn to_later(&self) -> CharGenVerificationResponse {
        CharGenVerificationResponse {
            response_type: self.code,
            identity: if self.code == CREATED {
                CharacterIdentity {
                    gid: ObjectId(self.id),
                    name: self.name.clone(),
                    seconds_greyed_out: self.deletion,
                }
            } else {
                CharacterIdentity::default()
            },
        }
    }
}

/// `0xF655` to the server: the roster's account, then the row's slot.
#[must_use]
pub fn delete(slot: i32, account: &str) -> Vec<u8> {
    let mut w = Writer::new(DELETE);
    w.account(ACCOUNT_MARKER, account).u32(slot.cast_unsigned());
    w.into_inner()
}

/// Creation and deletion. It keeps no state: the account the roster gave is shared.
#[derive(Debug, Default, Clone, Copy)]
pub struct Characters;

impl Translate for Characters {
    fn down(
        &mut self,
        opcode: u32,
        _queue: u16,
        message: &[u8],
        cx: &mut Context<'_>,
    ) -> Result<bool, ReadError> {
        match opcode {
            ANSWER => {
                let answer = Answer::read(message)?;
                if let Err(e) = cx.push(NetQueue::UiQueue, &answer.to_later()) {
                    tracing::warn!("1999: the creation answer did not encode: {e}");
                }
            }
            // The deletion's acknowledgement is empty on both wires; a fresh roster follows.
            DELETE => cx.push_raw(
                Opcode::CHARACTER_CHARACTER_DELETE,
                NetQueue::UiQueue,
                Vec::new(),
            ),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn up(
        &mut self,
        opcode: Opcode,
        _queue: NetQueue,
        _ordered: bool,
        payload: &[u8],
        cx: &mut Context<'_>,
    ) -> bool {
        let body = payload.get(4..).unwrap_or_default();
        match opcode {
            Opcode::CHARACTER_SEND_CHAR_GEN_RESULT => {
                let written = read_body::<CharacterSendCharGenResult>(body)
                    .map_err(|e| tracing::warn!("1999: a creation did not read: {e}"))
                    .and_then(|m| {
                        create(&m.result, &cx.shared.account)
                            .map_err(|e| tracing::warn!("1999: a creation has no 1999 form: {e:?}"))
                    });
                match written {
                    Ok(m) => cx.send(LOGIN_QUEUE, &m),
                    Err(()) => {
                        // Answered here, so the character screen's wait ends.
                        let refused = CharGenVerificationResponse {
                            response_type: CORRUPT,
                            identity: CharacterIdentity::default(),
                        };
                        if let Err(e) = cx.push(NetQueue::UiQueue, &refused) {
                            tracing::warn!("1999: the creation refusal did not encode: {e}");
                        }
                        true
                    }
                }
            }
            Opcode::CHARACTER_CHARACTER_DELETE => match read_body::<CharacterDeleteRequest>(body) {
                Ok(m) => cx.send(LOGIN_QUEUE, &delete(m.slot_index, &cx.shared.account)),
                Err(e) => {
                    tracing::warn!("1999: a deletion did not read: {e}");
                    false
                }
            },
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyreal_primitives::era::LAUNCH_SKILLS;

    fn sho_woman() -> CharGenResult {
        let mut skills = vec![1; 55];
        skills[0] = 0;
        skills[6] = 3; // melee defence, specialised
        skills[39] = 2; // cooking, trained: past the relay's 35
        skills[50] = 2; // a later skill, not written
        CharGenResult {
            version: 1,
            heritage_group: 3,
            gender: 2,
            eyes_strip: 1,
            nose_strip: 2,
            mouth_strip: 3,
            hair_color: 4,
            eye_color: 5,
            hair_style: 6,
            headgear_style: 7,
            headgear_color: 0x100,
            shirt_style: 8,
            shirt_color: 0x200,
            trousers_style: 9,
            trousers_color: 0x300,
            footwear_style: 10,
            footwear_color: 0x400,
            skin_shade: 0.25,
            hair_shade: 0.5,
            headgear_shade: 0.75,
            shirt_shade: 1.0,
            trousers_shade: 0.125,
            footwear_shade: 0.0,
            template_num: 2,
            strength: 60,
            endurance: 40,
            coordination: 80,
            quickness: 70,
            focus: 50,
            self_: 30,
            slot: 1,
            class_id: 0,
            skill_advancement_classes: skills,
            name: "Mei Lin".to_owned(),
            start_area: 12,
            is_admin: 0,
            is_envoy: 0,
            checksum_value: 0,
        }
    }

    fn u32_at(m: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([m[at], m[at + 1], m[at + 2], m[at + 3]])
    }

    #[test]
    fn heritage_and_sex_take_their_1999_indices() {
        assert_eq!(
            [1, 2, 3, 4, 0].map(heritage_1999),
            [Some(0), Some(1), Some(2), None, None]
        );
        assert_eq!([1, 2, 0, 3].map(sex_1999), [Some(1), Some(0), None, None]);
        let mut r = sho_woman();
        r.heritage_group = 5;
        assert_eq!(create(&r, "demo"), Err(Unwritable::Heritage(5)));
        r.heritage_group = 1;
        r.gender = 0;
        assert_eq!(create(&r, "demo"), Err(Unwritable::Sex(0)));
    }

    #[test]
    fn the_skill_list_covers_every_launch_skill_id() {
        let highest = LAUNCH_SKILLS.iter().copied().max().unwrap();
        assert_eq!(SKILL_COUNT, usize::try_from(highest).unwrap() + 1);
    }

    #[test]
    fn a_creation_has_the_1999_layout() {
        let r = sho_woman();
        let m = create(&r, "demo").unwrap();
        assert_eq!(u32_at(&m, 0), CREATE);
        assert_eq!(u32_at(&m, 4), ACCOUNT_MARKER);
        assert_eq!(&m[8..12], b"demo");
        // Version, heritage, sex after the 28-byte account field.
        assert_eq!([u32_at(&m, 32), u32_at(&m, 36), u32_at(&m, 40)], [1, 2, 0]);
        // Seven face values, then headgear colour, then style and colour pairs.
        assert_eq!(u32_at(&m, 44), 1);
        assert_eq!(u32_at(&m, 68), 7);
        assert_eq!(u32_at(&m, 72), 0x100);
        assert_eq!([u32_at(&m, 76), u32_at(&m, 80)], [8, 0x200]);
        assert_eq!([u32_at(&m, 92), u32_at(&m, 96)], [10, 0x400]);
        // The shades are packed doubles starting off an eight-byte boundary.
        assert_eq!(&m[100..108], &0.25f64.to_le_bytes());
        assert_eq!(&m[140..148], &0.0f64.to_le_bytes());
        assert_eq!(u32_at(&m, 148), 2, "template");
        assert_eq!(u32_at(&m, 152), 60, "strength");
        assert_eq!(u32_at(&m, 172), 30, "self");
        assert_eq!([u32_at(&m, 176), u32_at(&m, 180)], [1, ACCESS_PLAYER]);
        assert_eq!(u32_at(&m, 184), 40);
        let skills: Vec<u32> = (0..40).map(|i| u32_at(&m, 188 + 4 * i)).collect();
        assert_eq!((skills[0], skills[1], skills[6], skills[39]), (0, 1, 3, 2));
        // The name: "Mei Lin" and its NUL is 8 bytes at an aligned offset.
        let at = 188 + 160;
        assert_eq!(u16::from_le_bytes([m[at], m[at + 1]]), 8);
        assert_eq!(&m[at + 2..at + 9], b"Mei Lin");
        let tail = at + 2 + 8 + 2;
        assert_eq!(u32_at(&m, tail), NO_HERALDRY.cast_unsigned());
        assert_eq!(u32_at(&m, tail + 4), 0);
        assert_eq!(u32_at(&m, tail + 8), 12, "start area");
        assert_eq!(u32_at(&m, tail + 12), closing_sum(&r, 2, 0));
        assert_eq!(m.len(), tail + 16);
    }

    #[test]
    fn the_closing_sum_leaves_out_colours_shades_and_the_slot() {
        let r = sho_woman();
        // 2 + 0 + (1..=10) + 2 + 330.
        assert_eq!(closing_sum(&r, 2, 0), 2 + 55 + 2 + 330);
        let mut other = r.clone();
        other.shirt_color = 9;
        other.hair_shade = 0.9;
        other.slot = 4;
        other.start_area = 3;
        assert_eq!(closing_sum(&other, 2, 0), closing_sum(&r, 2, 0));
    }

    #[test]
    fn an_answer_carries_an_identity_only_when_created() {
        let ok = Answer {
            code: CREATED,
            id: 0x5000_0003,
            name: "Mei Lin".to_owned(),
            deletion: 0,
        };
        let back = Answer::read(&ok.write()).unwrap();
        assert_eq!(back, ok);
        let later = back.to_later();
        assert_eq!(later.identity.gid, ObjectId(0x5000_0003));
        let blob = pyreal_protocol::write_blob(&later).unwrap();
        let read: CharGenVerificationResponse = pyreal_protocol::read_body(&blob[4..]).unwrap();
        assert_eq!(read, later);

        // Name in use: the code alone.
        let refused = Answer::read(&[0x43, 0xF6, 0, 0, 3, 0, 0, 0]).unwrap();
        assert_eq!(refused.to_later().response_type, 3);
        assert_eq!(refused.to_later().identity, CharacterIdentity::default());
        assert!(Answer::read(&[0x43, 0xF6, 0, 0, 1, 0, 0, 0]).is_err());
    }

    #[test]
    fn a_deletion_names_the_slot_after_the_account_field() {
        let m = delete(2, "demo");
        assert_eq!(m.len(), 4 + 28 + 4);
        assert_eq!(u32_at(&m, 4), ACCOUNT_MARKER);
        assert_eq!(u32_at(&m, 32), 2);
    }

    fn parts() -> (
        std::collections::VecDeque<pyreal_primitives::IncomingMessage>,
        crate::conn::Connection,
        crate::translate::Shared,
    ) {
        let cred = crate::login::Credential::new("Demo", "secret").unwrap();
        let shared = crate::translate::Shared {
            account: "demo".to_owned(),
            world_name: "AC1999".to_owned(),
            ..Default::default()
        };
        (
            Default::default(),
            crate::conn::Connection::new(cred, (102, 67)),
            shared,
        )
    }

    #[test]
    fn through_the_link_a_creation_with_no_1999_form_is_refused_here() {
        let (mut inbox, mut conn, mut shared) = parts();
        let mut cx = Context {
            inbox: &mut inbox,
            conn: &mut conn,
            shared: &mut shared,
            wrapped: None,
        };
        let mut c = Characters;
        let mut result = sho_woman();
        result.heritage_group = 6;
        let m = pyreal_protocol::write_blob(&CharacterSendCharGenResult {
            account: "demo".to_owned(),
            result,
        })
        .unwrap();
        assert!(c.up(
            Opcode::CHARACTER_SEND_CHAR_GEN_RESULT,
            NetQueue::Logon,
            false,
            &m,
            &mut cx
        ));
        let ok = Answer {
            code: CREATED,
            id: 0x5000_0003,
            name: "Mei Lin".to_owned(),
            deletion: 0,
        };
        assert_eq!(c.down(ANSWER, 4, &ok.write(), &mut cx), Ok(true));
        assert_eq!(c.down(DELETE, 4, &[0x55, 0xF6, 0, 0], &mut cx), Ok(true));
        assert_eq!(c.down(0xF658, 4, &[0x58, 0xF6, 0, 0], &mut cx), Ok(false));
        let got: Vec<_> = inbox.iter().map(|m| m.opcode).collect();
        assert_eq!(got, [0xF643, 0xF643, 0xF655]);
        assert_eq!(&inbox[0].body, &CORRUPT.to_le_bytes());
        assert!(inbox[2].body.is_empty());
    }
}
