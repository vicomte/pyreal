//! Spell filtering and favorite placement.
use crate::UiRequest;

/// The default spell-filter setting when section `0x0020` is absent — every
/// school and every level. The protocol decoder uses the same
/// number; it is repeated here because this crate does not depend on `pyreal-protocol`.
pub const DEFAULT_SPELL_FILTERS: u32 = 0x3FFF;

/// The **first** formula slot, decrypted — subtracts the key
/// from every non-zero slot in place, so slot 0 keeps its position and a zero slot stays zero.
///
/// `pyreal_assets::tables::SpellBase::comps` drops the zero slots, which loses that position; the
/// power component has to be read off `raw_comps[0]` and the key.
#[must_use]
pub const fn power_component(raw_comp0: u32, comp_key: u32) -> u32 {
    if raw_comp0 == 0 {
        0
    } else {
        raw_comp0.wrapping_sub(comp_key)
    }
}

/// The school and level masks use different, disjoint bits.
#[must_use]
pub const fn school_mask(school: u32) -> u32 {
    match school {
        1 => 8,
        2 => 4,
        3 => 2,
        4 => 1,
        5 => 0x2000,
        _ => 0,
    }
}
/// The ninth level has a persisted bit even when no control exposes it.
#[must_use]
pub const fn level_mask(level: u32) -> u32 {
    if level >= 1 && level <= 9 {
        0x10 << (level - 1)
    } else {
        0
    }
}
#[must_use]
pub const fn accepts(filters: u32, school: u32, level: u32) -> bool {
    filters & school_mask(school) != 0 && filters & level_mask(level) != 0
}
/// A move removes only a displayed row. Duplicate refusal inspects the stored bank instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FavoritePlan {
    pub remove: bool,
    pub index: i32,
}
impl FavoritePlan {
    #[must_use]
    pub fn new(
        raw: &[u32],
        rows: &[u32],
        spell: u32,
        mut index: i32,
        move_allowed: bool,
    ) -> Option<Self> {
        if spell == 0 || (!move_allowed && raw.contains(&spell)) {
            return None;
        }
        let old = rows.iter().position(|id| *id == spell);
        if old.is_some_and(|i| i32::try_from(i).unwrap_or(i32::MAX) < index) {
            index -= 1;
        }
        if index == -1 {
            index = i32::try_from(rows.len() + 1 - usize::from(old.is_some())).unwrap_or(i32::MAX);
        }
        Some(Self {
            remove: old.is_some(),
            index,
        })
    }
    #[must_use]
    pub fn requests(self, spell_id: u32, tab: usize) -> Vec<UiRequest> {
        let mut requests = Vec::new();
        if self.remove {
            requests.push(UiRequest::RemoveSpellFavorite { spell_id, tab });
        }
        requests.push(UiRequest::AddSpellFavorite {
            spell_id,
            index: self.index,
            tab,
        });
        requests
    }
}
