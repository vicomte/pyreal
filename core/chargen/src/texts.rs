//! The creation texts a world's own files carry.
//!
//! The character-generation table from before Throne of Destiny names its texts as records of their
//! own: each heritage's description, each profession's description for each sex, the naming help of
//! each sex and the help of each page of the wizard. The later table names none; its interface's
//! string tables hold the texts instead. [`CreationTexts::read`] reads every record the table names,
//! so either creation interface can show the world's own words where the world has them, and the
//! later files' naming help and taboo table, which the name generators answer to.

use std::collections::BTreeMap;

use pyreal_assets::tables::CharGen;
use pyreal_assets::{Decode as _, TabooTable};
use pyreal_primitives::{AssetSource, DataId};

/// The string table the later creation interface's texts are rows of.
const CREATION_STRINGS: DataId = DataId(0x2300_0002);
/// The table of words a server refuses in a name.
const TABOO_TABLE: DataId = DataId(0x0E00_001E);

/// Each heritage's word in the later naming rows' tokens (`ID_CharGen_<word><Sex>Names`).
const NAMING_TOKENS: [(u32, &str); 13] = [
    (1, "Alu"),
    (2, "Gharu"),
    (3, "Sho"),
    (4, "Via"),
    (5, "Shad"),
    (6, "Gear"),
    (7, "AunT"),
    (8, "Lug"),
    (9, "Emp"),
    (10, "Shad"),
    (11, "Und"),
    (12, "Olthoi"),
    (13, "OlthoiAcid"),
];

/// The texts the world's files carry. Empty for files that carry none.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CreationTexts {
    /// The records the table names, by record id.
    records: BTreeMap<DataId, String>,
    /// The later files' naming help, by heritage and sex.
    naming: BTreeMap<(u32, u32), String>,
    /// The later files' taboo table.
    taboo: Option<TabooTable>,
}

impl CreationTexts {
    /// Every text record the table names that the files hold and that decodes.
    #[must_use]
    pub fn read(assets: &dyn AssetSource, chargen: &CharGen) -> Self {
        let mut records = BTreeMap::new();
        for id in Self::named(chargen) {
            if records.contains_key(&id) {
                continue;
            }
            if let Some(text) = assets.read(id).ok().and_then(|b| decode(id, &b)) {
                records.insert(id, text);
            }
        }
        let taboo = assets
            .read(TABOO_TABLE)
            .ok()
            .and_then(|b| TabooTable::decode_payload(TABOO_TABLE, &b).ok());
        Self {
            records,
            naming: Self::read_naming(assets),
            taboo,
        }
    }

    /// The later files' naming rows of string table `0x23000002`, by heritage and sex.
    fn read_naming(assets: &dyn AssetSource) -> BTreeMap<(u32, u32), String> {
        let Some(table) = assets.read(CREATION_STRINGS).ok().and_then(|b| {
            pyreal_assets::ui::StringTable::decode_payload(CREATION_STRINGS, &b).ok()
        }) else {
            return BTreeMap::new();
        };
        let mut out = BTreeMap::new();
        for (heritage, word) in NAMING_TOKENS {
            for (sex, sex_word) in [(1, "Male"), (2, "Female")] {
                let token = format!("ID_CharGen_{word}{sex_word}Names");
                let hash = pyreal_primitives::num::hash::str_hash(token.as_bytes());
                let text = table.strings.iter().find(|(k, _)| *k == hash);
                if let Some(text) = text.and_then(|(_, e)| e.strings.first()) {
                    out.insert(
                        (heritage, sex),
                        pyreal_assets::escape::unescape(text.clone()),
                    );
                }
            }
        }
        out
    }

    /// The text of record `id`, when the world's files carry it.
    #[must_use]
    pub fn get(&self, id: Option<DataId>) -> Option<&str> {
        self.records.get(&id?).map(String::as_str)
    }

    /// The naming help for `heritage` and `sex`: the sex's own record before Throne of Destiny,
    /// the string table's row after.
    #[must_use]
    pub fn naming_help(&self, record: Option<DataId>, heritage: u32, sex: u32) -> Option<&str> {
        self.get(record)
            .or_else(|| self.naming.get(&(heritage, sex)).map(String::as_str))
    }

    /// Whether a word of `name` is one the world's taboo table refuses, as the server checks a
    /// new character's name. False when the files carry no table.
    #[must_use]
    pub fn refuses_name(&self, name: &str) -> bool {
        self.taboo.as_ref().is_some_and(|t| {
            pyreal_rules::taboo::contains_taboo_word(
                t,
                &pyreal_primitives::text::cp1252::Cp1252,
                name,
            )
        })
    }

    /// Whether the world names no text of its own.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// A text set from records already read, for a world built in memory.
    #[must_use]
    pub fn from_records(records: impl IntoIterator<Item = (DataId, String)>) -> Self {
        Self {
            records: records.into_iter().collect(),
            ..Self::default()
        }
    }

    /// Every text record id the table names: the page help, then each heritage's description,
    /// each sex's naming help and each profession's description for each sex.
    fn named(chargen: &CharGen) -> impl Iterator<Item = DataId> + '_ {
        let heritages = chargen.heritage_groups.values();
        chargen
            .help_strings
            .iter()
            .copied()
            .chain(heritages.clone().filter_map(|h| h.description))
            .chain(
                heritages
                    .clone()
                    .flat_map(|h| h.sexes.values())
                    .filter_map(|s| s.naming_help),
            )
            .chain(
                heritages
                    .flat_map(|h| h.template_presentations.values().flatten())
                    .filter_map(|p| p.description),
            )
            .filter(|id| id.0 != 0)
    }
}

/// A text record: its own id, then the text, a 16-bit length (`0xFFFF` and a 32-bit one for a
/// long text) and that many Windows-1252 bytes.
#[must_use]
pub fn decode(id: DataId, bytes: &[u8]) -> Option<String> {
    let word = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    if word(0)? != id.0 {
        return None;
    }
    let short = u16::from_le_bytes(bytes.get(4..6)?.try_into().ok()?);
    let (len, start): (usize, usize) = if short == 0xFFFF {
        (usize::try_from(word(6)?).ok()?, 10)
    } else {
        (usize::from(short), 6)
    };
    let text = bytes.get(start..start.checked_add(len)?)?;
    Some(pyreal_primitives::text::cp1252::decode(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_record_decodes_its_short_and_long_forms_and_refuses_another_id() {
        let mut short = 0x3100_0001u32.to_le_bytes().to_vec();
        short.extend_from_slice(&5u16.to_le_bytes());
        short.extend_from_slice(b"Hello\0\0\0");
        assert_eq!(
            decode(DataId(0x3100_0001), &short).as_deref(),
            Some("Hello")
        );
        assert_eq!(decode(DataId(0x3100_0002), &short), None);
        let mut long = 0x3100_0003u32.to_le_bytes().to_vec();
        long.extend_from_slice(&0xFFFFu16.to_le_bytes());
        long.extend_from_slice(&3u32.to_le_bytes());
        long.extend_from_slice(&[b'A', 0xE9, b'B']);
        assert_eq!(
            decode(DataId(0x3100_0003), &long).as_deref(),
            Some("A\u{e9}B")
        );
        assert_eq!(decode(DataId(0x3100_0003), &long[..9]), None);
    }
}
