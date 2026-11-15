//! Where each spell stands on the spell bar, which this interface keeps itself: the game keeps a
//! tab as a list of spells with no gaps, and the bar lets a spell stand in any slot. Kept per
//! world and character, in a small text file in the client's settings folder: each line is
//! `tab.slot=spell`, the tab and the slot counted from 1.
//!
//! The game's list says which spells a tab holds; this says where each stands. A spell the list
//! gains takes the first free slot, one it loses leaves its slot, and the list is kept in the
//! slots' order, so another client shows the same spells closed up.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pyreal_client_contract::UiRequest;

/// Each tab's spells by slot, and their file.
#[derive(Debug, Default)]
pub struct SpellSlots {
    tabs: Vec<BTreeMap<usize, u32>>,
    /// The world and character they were read for, and their file.
    loaded_for: Option<String>,
    path: Option<PathBuf>,
}

impl SpellSlots {
    fn tab(&self, tab: usize) -> Option<&BTreeMap<usize, u32>> {
        self.tabs.get(tab)
    }

    fn tab_mut(&mut self, tab: usize) -> &mut BTreeMap<usize, u32> {
        if self.tabs.len() <= tab {
            self.tabs.resize_with(tab + 1, BTreeMap::new);
        }
        &mut self.tabs[tab]
    }

    /// The spell in slot `slot` (from 0) of tab `tab`.
    #[must_use]
    pub fn get(&self, tab: usize, slot: usize) -> Option<u32> {
        self.tab(tab)?.get(&slot).copied()
    }

    /// The slot spell `spell` stands in on tab `tab`.
    #[must_use]
    pub fn slot_of(&self, tab: usize, spell: u32) -> Option<usize> {
        self.tab(tab)?
            .iter()
            .find(|(_, s)| **s == spell)
            .map(|(slot, _)| *slot)
    }

    /// One past the last slot tab `tab` fills; 0 for an empty tab.
    #[must_use]
    pub fn extent(&self, tab: usize) -> usize {
        self.tab(tab)
            .and_then(|t| t.keys().next_back())
            .map_or(0, |last| last + 1)
    }

    /// The slots tab `tab` fills, in order.
    #[must_use]
    pub fn filled(&self, tab: usize) -> Vec<usize> {
        self.tab(tab)
            .map_or_else(Vec::new, |t| t.keys().copied().collect())
    }

    /// Tab `tab`'s spells in their slots' order: the list the game should hold.
    #[must_use]
    pub fn order(&self, tab: usize) -> Vec<u32> {
        self.tab(tab)
            .map_or_else(Vec::new, |t| t.values().copied().collect())
    }

    /// Bring the slots to the game's lists: a spell a list holds that no slot does takes the
    /// first free slot, in the list's order; a slot whose spell the list lacks is emptied.
    /// Kept when anything changed.
    pub fn reconcile(&mut self, lists: &[Vec<u32>]) {
        let mut changed = false;
        for (tab, list) in lists.iter().enumerate() {
            let slots = self.tab_mut(tab);
            let before = slots.len();
            slots.retain(|_, spell| list.contains(spell));
            changed |= slots.len() != before;
            for spell in list {
                if !slots.values().any(|s| s == spell) {
                    let free = (0..).find(|n| !slots.contains_key(n)).unwrap_or(0);
                    slots.insert(free, *spell);
                    changed = true;
                }
            }
        }
        if changed {
            self.save();
        }
    }

    /// Put `spell` in slot `slot` of tab `tab`, and keep it. A spell already there changes
    /// places with it, or, when `spell` was not on the tab, takes the first free slot.
    pub fn place(&mut self, tab: usize, spell: u32, slot: usize) {
        let from = self.slot_of(tab, spell);
        let slots = self.tab_mut(tab);
        if let Some(from) = from {
            slots.remove(&from);
        }
        if let Some(there) = slots.insert(slot, spell).filter(|s| *s != spell) {
            let to = from.unwrap_or_else(|| (0..).find(|n| !slots.contains_key(n)).unwrap_or(0));
            slots.insert(to, there);
        }
        self.save();
    }

    /// Take `spell` off tab `tab`, and keep it.
    pub fn remove(&mut self, tab: usize, spell: u32) {
        if let Some(from) = self.slot_of(tab, spell) {
            self.tab_mut(tab).remove(&from);
            self.save();
        }
    }

    /// Read the slots kept for character `name` on the world at `address` in `dir`, once per
    /// character.
    pub fn load_for(&mut self, dir: Option<&Path>, address: &str, name: &str) {
        let key = format!("{address}|{name}");
        if name.is_empty() || self.loaded_for.as_deref() == Some(key.as_str()) {
            return;
        }
        self.path = dir.map(|d| d.join(file_name(address, name)));
        self.tabs = self
            .path
            .as_ref()
            .and_then(|p| pyreal_client_runtime::platform::files::read_to_string(p).ok())
            .map(|t| parse(&t))
            .unwrap_or_default();
        self.loaded_for = Some(key);
    }

    fn save(&self) {
        if let Some(path) = &self.path {
            if let Err(e) = std::fs::write(path, to_text(&self.tabs)) {
                tracing::warn!("the spell bar's slots were not kept: {e}");
            }
        }
    }
}

/// The requests that bring the game's list `list` for tab `tab` to `order` with the fewest
/// moves: the longest run of spells already in `order`'s order stays, the rest are taken off, then
/// put back where `order` has them, the earliest first.
#[must_use]
pub fn order_requests(list: &[u32], order: &[u32], tab: usize) -> Vec<UiRequest> {
    if list == order {
        return Vec::new();
    }
    // Each listed spell's place in `order`, and the longest run of them that rises.
    let places: Vec<Option<usize>> = list
        .iter()
        .map(|s| order.iter().position(|o| o == s))
        .collect();
    let mut best = vec![1usize; list.len()];
    let mut back = vec![None; list.len()];
    for i in 0..list.len() {
        for j in 0..i {
            if let (Some(a), Some(b)) = (places[j], places[i]) {
                if a < b && best[j] + 1 > best[i] {
                    best[i] = best[j] + 1;
                    back[i] = Some(j);
                }
            }
        }
    }
    let mut stay = std::collections::HashSet::new();
    let mut at = (0..list.len())
        .filter(|i| places[*i].is_some())
        .max_by_key(|i| best[*i]);
    while let Some(i) = at {
        stay.insert(list[i]);
        at = back[i];
    }
    let mut out: Vec<UiRequest> = list
        .iter()
        .filter(|s| !stay.contains(*s))
        .map(|s| UiRequest::RemoveSpellFavorite { spell_id: *s, tab })
        .collect();
    for (index, spell) in order.iter().enumerate() {
        if !stay.contains(spell) {
            out.push(UiRequest::AddSpellFavorite {
                spell_id: *spell,
                index: i32::try_from(index).unwrap_or(i32::MAX),
                tab,
            });
        }
    }
    out
}

/// The file, in the settings folder, a character's spell bar slots are kept in.
#[must_use]
pub fn file_name(address: &str, name: &str) -> String {
    let safe = |t: &str| -> String {
        t.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect()
    };
    format!("horizon-spellbar-{}-{}.txt", safe(address), safe(name))
}

/// The slots a file holds; a line it cannot read is left out.
#[must_use]
pub fn parse(text: &str) -> Vec<BTreeMap<usize, u32>> {
    let mut tabs: Vec<BTreeMap<usize, u32>> = Vec::new();
    for line in text.lines() {
        let Some((at, spell)) = line.split_once('=') else {
            continue;
        };
        let Some((tab, slot)) = at.trim().split_once('.') else {
            continue;
        };
        let (Some(tab), Some(slot), Ok(spell)) = (
            tab.parse::<usize>().ok().and_then(|t| t.checked_sub(1)),
            slot.parse::<usize>().ok().and_then(|s| s.checked_sub(1)),
            spell.trim().parse::<u32>(),
        ) else {
            continue;
        };
        // The game has eight tabs; one far past them, or a slot past any bar, is a hand edit.
        if tab >= MAX_TABS || slot >= MAX_SLOTS {
            continue;
        }
        if tabs.len() <= tab {
            tabs.resize_with(tab + 1, BTreeMap::new);
        }
        tabs[tab].insert(slot, spell);
    }
    tabs
}

/// The most tabs a file is read for.
const MAX_TABS: usize = 64;

/// The most slots a tab is read for.
const MAX_SLOTS: usize = 4096;

/// The file's text for `tabs`.
#[must_use]
pub fn to_text(tabs: &[BTreeMap<usize, u32>]) -> String {
    let mut text = String::new();
    for (tab, slots) in tabs.iter().enumerate() {
        for (slot, spell) in slots {
            text.push_str(&format!("{}.{}={spell}\n", tab + 1, slot + 1));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hand_edited_tab_past_any_the_game_has_is_left_out() {
        let tabs = parse("99999999999.1=5\n1.1=7\n");
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].get(&0), Some(&7));
    }

    #[test]
    fn a_hand_edited_slot_past_any_bar_is_left_out() {
        let tabs = parse("1.18446744073709551615=5\n1.2=7\n");
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].len(), 1);
        let slots = SpellSlots {
            tabs,
            ..SpellSlots::default()
        };
        assert_eq!(slots.extent(0), 2);
    }

    #[test]
    fn new_spells_take_the_first_free_slots_and_a_spell_gone_leaves_its_slot() {
        let mut slots = SpellSlots::default();
        slots.reconcile(&[vec![7, 8, 9]]);
        assert_eq!(slots.filled(0), [0, 1, 2]);
        slots.place(0, 7, 30);
        slots.reconcile(&[vec![8, 9, 10]]);
        assert_eq!((slots.get(0, 0), slots.get(0, 30)), (Some(10), None));
        assert_eq!(slots.order(0), [10, 8, 9]);
    }

    #[test]
    fn a_spell_put_on_another_changes_places_with_it() {
        let mut slots = SpellSlots::default();
        slots.reconcile(&[vec![7, 8]]);
        slots.place(0, 7, 1);
        assert_eq!((slots.get(0, 0), slots.get(0, 1)), (Some(8), Some(7)));
        // A spell new to the tab dropped on one sends it to the first free slot.
        slots.place(0, 9, 0);
        assert_eq!(slots.order(0), [9, 7, 8]);
        assert_eq!(slots.slot_of(0, 8), Some(2));
    }

    #[test]
    fn the_game_s_list_is_brought_to_the_slots_order_with_the_fewest_moves() {
        let remove = |spell_id| UiRequest::RemoveSpellFavorite { spell_id, tab: 0 };
        let add = |spell_id, index| UiRequest::AddSpellFavorite {
            spell_id,
            index,
            tab: 0,
        };
        assert_eq!(
            order_requests(&[7, 8, 9], &[8, 9, 7], 0),
            [remove(7), add(7, 2)]
        );
        assert_eq!(
            order_requests(&[1, 2, 3, 4], &[1, 4, 3, 2], 0),
            [remove(2), remove(3), add(3, 2), add(2, 3)]
        );
        // A spell new to the tab is only put in.
        assert_eq!(order_requests(&[7, 8], &[7, 5, 8], 0), [add(5, 1)]);
        assert!(order_requests(&[7, 8], &[7, 8], 0).is_empty());
        // Applied in order, each list comes out as wanted.
        for (list, order) in [
            (vec![9, 7, 8], vec![7, 8, 9]),
            (vec![1, 2, 3, 4], vec![4, 3, 2, 1]),
            (vec![3, 1, 2], vec![2, 3, 1]),
        ] {
            let mut got = list.clone();
            for r in order_requests(&list, &order, 0) {
                match r {
                    UiRequest::RemoveSpellFavorite { spell_id, .. } => {
                        got.retain(|s| *s != spell_id);
                    }
                    UiRequest::AddSpellFavorite {
                        spell_id, index, ..
                    } => got.insert(usize::try_from(index).unwrap().min(got.len()), spell_id),
                    _ => {}
                }
            }
            assert_eq!(got, order);
        }
    }

    #[test]
    fn slots_read_back_what_they_wrote() {
        let mut tabs = vec![BTreeMap::new(), BTreeMap::new()];
        tabs[0].insert(0, 7);
        tabs[1].insert(29, 1234);
        let text = to_text(&tabs);
        assert_eq!(text, "1.1=7\n2.30=1234\n");
        assert_eq!(parse(&format!("{text}nonsense\n0.1=5\n")), tabs);
    }
}
