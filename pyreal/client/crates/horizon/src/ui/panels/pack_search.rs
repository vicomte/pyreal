//! The inventory's search: which carried items match what is typed, by name, by kind and by
//! mark. Items that do not match are dimmed, and the side packs holding a match are marked.

use pyreal_rules::weenie::item_type as t;

use super::marks::{Mark, Marks};
use crate::ui::game::Item;

/// The kinds a search word can name, each with the item types it covers.
const KINDS: &[(&str, u32)] = &[
    ("weapon", t::WEAPON | t::CASTER),
    ("melee", t::MELEE_WEAPON),
    ("missile", t::MISSILE_WEAPON),
    ("caster", t::CASTER),
    ("armor", t::ARMOR),
    ("armour", t::ARMOR),
    ("clothing", t::CLOTHING),
    ("jewelry", t::JEWELRY),
    ("jewellery", t::JEWELRY),
    ("food", t::FOOD),
    ("money", t::MONEY | t::PROMISSORY_NOTE),
    ("gem", t::GEM),
    ("component", t::SPELL_COMPONENTS),
    ("key", t::KEY),
    ("book", t::WRITABLE),
    ("manastone", t::MANASTONE),
    ("salvage", t::TINKERING_MATERIAL),
    ("tool", t::TINKERING_TOOL),
    ("container", t::CONTAINER),
    ("pack", t::CONTAINER),
];

/// The shortest start of a kind's name that names it ("arm" for armor).
const KIND_PREFIX: usize = 3;

/// Whether `word` (lowercase) names a kind `obj_type` is: it is that kind's name, a plural of it,
/// or the start of it at least three letters long.
fn names_kind(word: &str, obj_type: u32) -> bool {
    let word = word
        .strip_suffix('s')
        .filter(|w| w.len() >= KIND_PREFIX)
        .unwrap_or(word);
    word.len() >= KIND_PREFIX
        && KINDS
            .iter()
            .any(|(kind, mask)| obj_type & mask != 0 && kind.starts_with(word))
}

/// Whether `item` matches the search `needle`: every word of it must begin a word of the item's
/// name, name a kind the item is (armor, weapon, salvage), or name its mark (keep, junk),
/// ignoring case. An empty needle matches every item.
#[must_use]
pub fn matches(item: &Item, mark: Option<Mark>, needle: &str) -> bool {
    let obj_type = item.decoration.as_ref().map_or(0, |d| d.obj_type);
    let words: Vec<String> = item
        .name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    needle.split_whitespace().all(|word| {
        let lower = word.to_lowercase();
        words.iter().any(|w| w.starts_with(&lower))
            || names_kind(&lower, obj_type)
            || mark.is_some_and(|m| m.word() == lower)
    })
}

/// Whether any of `items` matches `needle`.
#[must_use]
pub fn any_match(items: &[Item], marks: &Marks, needle: &str) -> bool {
    items.iter().any(|i| matches(i, marks.get(i), needle))
}
