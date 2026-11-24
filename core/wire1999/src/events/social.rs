//! Fellowships, allegiance, books, portal storms and the session's small messages.

use pyreal_primitives::ObjectId;
use pyreal_protocol::archive::PackedHash;
use pyreal_protocol::social::{
    AllegianceData, AllegianceHierarchy, AllegianceProfile, AllegianceUpdate, Fellow, Fellowship,
    FellowshipFullUpdate, FellowshipLocks, FellowshipUpdateFellow,
};
use pyreal_protocol::trade::{
    BookPageDataResponse, MiscPortalStorm, MiscPortalStormBrewing, MiscPortalStormImminent,
    MiscPortalStormSubsided, PageData, PageDataList, WritingBookOpen,
};
use pyreal_protocol::{MessageError, Opcode, Reader};

use super::codec::read_string;
use super::magic::read;
use super::{Handled, Later};

/// `0x00AF` fellowship full update.
pub const FELLOWSHIP: u32 = 0x00AF;
/// `0x00B0` one fellow.
pub const FELLOW: u32 = 0x00B0;
/// `0x0020` allegiance update.
pub const ALLEGIANCE: u32 = 0x0020;
/// `0x00B4` book open.
pub const BOOK_OPEN: u32 = 0x00B4;
/// `0x00B8` one page.
pub const BOOK_PAGE: u32 = 0x00B8;
/// `0x009C` storm subsided.
pub const STORM_SUBSIDED: u32 = 0x009C;
/// `0x009D` storm brewing.
pub const STORM_BREWING: u32 = 0x009D;
/// `0x009E` storm imminent.
pub const STORM_IMMINENT: u32 = 0x009E;
/// `0x009F` storm, whose text the later client writes itself.
pub const STORM: u32 = 0x009F;

/// The storm extent the later warnings carry; the client only checks it is above 0.
const STORM_EXTENT: f32 = 1.0;
/// Update type 1, a whole fellow.
const FELLOW_FULL: u32 = 1;
/// The table size hint of the departed-fellows table.
const DEPARTED_TABLE_SIZE: u32 = 16;
/// 1999 number to later number, bodies unchanged: quit, dismiss, disband.
const RENUMBERED: [(u32, Opcode); 3] = [
    (0x00A7, Opcode::FELLOWSHIP_QUIT),
    (0x00B1, Opcode::FELLOWSHIP_DISMISS),
    (0x00B3, Opcode::FELLOWSHIP_DISBAND),
];
/// Same number, same body: fellow update and stats done, book page added and deleted, age and
/// ping answers, log off, delete, banned, admin environs.
const SAME: [u32; 10] = [
    0x01C9, 0x01CA, 0x00B6, 0x00B7, 0x01C3, 0x01EA, 0xF653, 0xF655, 0xF7C1, 0xEA60,
];

pub(super) fn translate(opcode: u32, message: &[u8]) -> Option<Result<Handled, MessageError>> {
    let out = match opcode {
        FELLOWSHIP => read(message, |r| {
            let members = r.packed_hash(|r| Ok((r.u32()?, fellow(r)?)))?;
            Later::of(&FellowshipFullUpdate(Fellowship {
                members,
                name: read_string(r)?,
                leader: ObjectId(r.u32()?),
                // Launch shares experience, unevenly; nothing is locked or open.
                share_xp: 1,
                even_xp_split: 0,
                open_fellow: 0,
                locked: 0,
                fellows_departed: PackedHash {
                    table_size: DEPARTED_TABLE_SIZE,
                    entries: Vec::new(),
                },
                locks: FellowshipLocks::default(),
            }))
        }),
        FELLOW => read(message, |r| {
            Later::of(&FellowshipUpdateFellow {
                fellow_id: ObjectId(r.u32()?),
                fellow: fellow(r)?,
                update_type: FELLOW_FULL,
            })
        }),
        ALLEGIANCE => read(message, allegiance).and_then(|a| Later::of(&a)),
        // A book with no pages has no page list at all.
        BOOK_OPEN => read(message, |r| book(r, true))
            .or_else(|_| read(message, |r| book(r, false)))
            .and_then(|b| Later::of(&b)),
        BOOK_PAGE => read(message, |r| {
            Later::of(&BookPageDataResponse {
                object_id: ObjectId(r.u32()?),
                page: r.u32()?,
                data: page(r)?,
            })
        }),
        // The warnings carry one word of 0 where the later extent goes.
        STORM_BREWING => read(message, |r| r.u32()).and_then(|_| {
            Later::of(&MiscPortalStormBrewing {
                extent: STORM_EXTENT,
            })
        }),
        STORM_IMMINENT => read(message, |r| r.u32()).and_then(|_| {
            Later::of(&MiscPortalStormImminent {
                extent: STORM_EXTENT,
            })
        }),
        STORM => read(message, read_string).and_then(|_| Later::of(&MiscPortalStorm)),
        STORM_SUBSIDED => read(message, |_| Later::of(&MiscPortalStormSubsided)),
        op if SAME.contains(&op) => Ok(Later::same(op, message)),
        op => {
            let (_, later) = RENUMBERED.iter().find(|(old, _)| *old == op)?;
            Ok(Later::same(later.0, message))
        }
    };
    Some(out.map(Handled::from))
}

fn book(r: &mut Reader<'_>, with_pages: bool) -> Result<WritingBookOpen, MessageError> {
    let book_id = ObjectId(r.u32()?);
    let max_num_pages = r.u32()?;
    let pages = if with_pages {
        PageDataList {
            max_num_pages: r.i32()?,
            max_num_chars_per_page: r.i32()?,
            pages: r.packed_list(page)?,
        }
    } else {
        PageDataList::default()
    };
    Ok(WritingBookOpen {
        book_id,
        max_num_pages,
        pages,
        inscription: read_string(r)?,
        scribe_id: ObjectId(r.u32()?),
        scribe_name: read_string(r)?,
    })
}

/// A 1999 fellow: experience cache and level as u16, then the name. No vitals or luminance.
fn fellow(r: &mut Reader<'_>) -> Result<Fellow, MessageError> {
    let cp_cache = u32::from(r.u16()?);
    let level = u32::from(r.u16()?);
    Ok(Fellow {
        name: read_string(r)?,
        level,
        cp_cache,
        ..Fellow::default()
    })
}

/// A 1999 page: author id, name, account, whether text follows, then the text. No version marker.
fn page(r: &mut Reader<'_>) -> Result<PageData, MessageError> {
    let author_id = ObjectId(r.u32()?);
    let author_name = read_string(r)?;
    let author_account = read_string(r)?;
    let text_included = r.i32()?;
    let page_text = if text_included == 0 {
        None
    } else {
        Some(read_string(r)?)
    };
    Ok(PageData {
        author_id,
        author_name,
        author_account,
        version: None,
        text_included,
        ignore_author: 0,
        page_text,
    })
}

/// The 1999 allegiance update: totals, a member count, then the members, each after its patron
/// but the first. It is the later hierarchy at version 0 with the legacy member form.
fn allegiance(r: &mut Reader<'_>) -> Result<AllegianceUpdate, MessageError> {
    let total_members = r.u32()?;
    let total_vassals = r.u32()?;
    let count = r.u32()?;
    let mut members = Vec::new();
    for i in 0..count {
        if r.remaining() == 0 {
            return Err(MessageError::LengthOverrun {
                field: "1999 allegiance members",
                len: count as usize,
                available: i as usize,
            });
        }
        let patron = if i == 0 {
            None
        } else {
            Some(ObjectId(r.u32()?))
        };
        members.push((patron, member(r)?));
    }
    Ok(AllegianceUpdate {
        // The client works the rank out from the members.
        rank: 0,
        profile: AllegianceProfile {
            total_members,
            total_vassals,
            hierarchy: AllegianceHierarchy {
                version: 0,
                members,
                ..AllegianceHierarchy::default()
            },
        },
    })
}

/// One member: the later record without the level or age gates, its time online an f64.
fn member(r: &mut Reader<'_>) -> Result<AllegianceData, MessageError> {
    let id = ObjectId(r.u32()?);
    let cp_cached = r.u32()?;
    let cp_tithed = r.u32()?;
    let bitfield =
        r.u32()? & !(AllegianceData::HAS_ALLEGIANCE_AGE | AllegianceData::HAS_PACKED_LEVEL);
    let gender = u32::from(r.u8()?);
    let heritage_group = u32::from(r.u8()?);
    let rank = u32::from(r.u16()?);
    let loyalty = u32::from(r.u16()?);
    let leadership = u32::from(r.u16()?);
    let time_online = pyreal_primitives::num::to_i32_f64(r.f64()?);
    Ok(AllegianceData {
        id,
        name: read_string(r)?,
        gender,
        heritage_group,
        rank,
        level: 0,
        // The later reader sets this on a record with no packed level; match it.
        bitfield: bitfield | AllegianceData::MAY_PASSUP_EXPERIENCE,
        cp_tithed,
        cp_cached,
        loyalty,
        leadership,
        time_online,
        allegiance_age: 0,
    })
}
