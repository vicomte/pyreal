//! The spells ACE's tie-breaks name are the ones the shipped spell table has at those ids.
//! Fixture: the shipped spell table from the retail DATs.

use pyreal_assets::tables::SpellTable;
use pyreal_assets::Decode;
use pyreal_primitives::{AssetSource, DataId};
use pyreal_rules::enchant::{ace_set_spells, FIRST_ACE_SET_SPELL, LEVEL8_AURA_SELF};

fn spell_table() -> SpellTable {
    let dir = pyreal_dat::testing::dat_dir();
    assert!(
        pyreal_dat::testing::have_dats(),
        "the spell table lives in client_portal.dat: none at {} -- set $PYREAL_TEST_DAT_DIR",
        dir.display()
    );
    let store = pyreal_dat::RetailDatStore::open_dir(&dir).expect("the retail dat store opens");
    let sid = DataId(0x0E00_000E);
    SpellTable::decode_payload(sid, &store.read(sid).expect("the spell table"))
        .expect("the spell table decodes")
}

#[test]
fn ace_s_aura_and_set_spell_ids_name_those_spells() {
    let table = spell_table();
    let name = |id: u32| table.spells.get(&id).map_or("", |s| s.name.as_str());
    let auras: Vec<&str> = LEVEL8_AURA_SELF
        .iter()
        .map(|id| name(u32::from(*id)))
        .collect();
    assert_eq!(
        auras,
        [
            "Aura of Incantation of Blood Drinker Self",
            "Aura of Incantation of Defender Self",
            "Aura of Incantation of Heart Seeker Self",
            "Aura of Incantation of Spirit Drinker Self",
            "Aura of Incantation of Swift Killer Self",
            "Aura of Incantation of Hermetic Link Self",
        ]
    );
    assert_eq!(name(5997), "Aura of Incantation of Blood Drinker Other");
    assert_eq!(name(FIRST_ACE_SET_SPELL), "Novice Duelist's Coordination");
    let sets = ace_set_spells(&table);
    assert!(sets.contains(&FIRST_ACE_SET_SPELL), "{sets:?}");
}
