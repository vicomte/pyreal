//! Character creation's wire: a template id is the template the character fits (exponents 2.5,
//! 3.0, 3.5 and a 0.75 floor), and restore is the three-field `0xF7D9`.
//! Fixture: the retail character-generation tables; nothing is sent.

#![cfg(gpu)]

use crate::common::client_dir;

use pyreal_chargen::{Attr, CharGenState};
use pyreal_primitives::{DataId, ObjectId};

/// Fails the test when the retail dats are not where `$PYREAL_TEST_DAT_DIR` says.
fn have_dats() {
    assert!(
        pyreal_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set PYREAL_TEST_DAT_DIR",
        client_dir().display()
    );
}

/// The retail store; the test fails when it is absent, with the shared shortfall message from
/// `pyreal_dat::testing::open_store_or_fail`.
fn store() -> std::sync::Arc<pyreal_dat::RetailDatStore> {
    crate::common::dats()
}

fn tables() -> Option<(
    pyreal_assets::tables::CharGen,
    pyreal_assets::tables::SkillTable,
)> {
    use pyreal_assets::Decode;
    let s = store();
    let cg_bytes = pyreal_primitives::AssetSource::read(s.as_ref(), DataId(0x0E00_0002)).ok()?;
    let cg = pyreal_assets::tables::CharGen::decode_payload(DataId(0x0E00_0002), &cg_bytes).ok()?;
    let sk_bytes = pyreal_primitives::AssetSource::read(s.as_ref(), DataId(0x0E00_0004)).ok()?;
    let sk =
        pyreal_assets::tables::SkillTable::decode_payload(DataId(0x0E00_0004), &sk_bytes).ok()?;
    Some((cg, sk))
}

// -------------------------------------------------------------------------------------------
// 1. Template fitting, evaluated against the retail table
// -------------------------------------------------------------------------------------------

/// Oracle: the retail template-fit calculation. Its three `pow` exponents are 2.5, 3.0
/// and 3.5, with a 0.75 floor.
///
/// **This is what the wire's template index actually is**, and it is why the client never sends the
/// constructor's -1 in a real session: with a heritage and a gender set, the function assigns 0 or
/// a real index every time it runs. Both profession-page and summary-page updates evaluate it.
///
/// Evaluated, not restated: applying template `n` makes every one of the three terms exactly 1 —
/// `min(char, tmpl) == tmpl`, every normal skill trained, every primary specialised — so the score
/// is 1.0 and `pow` cannot change it, while the untouched default build scores under the floor.
#[test]
fn the_template_the_wire_carries_is_the_one_the_character_fits() {
    have_dats();
    let (cg, skills) = tables().expect("the retail character-generation tables");
    let mut s = CharGenState::default();
    s.reset(&cg, &skills);
    s.set_heritage_group(&cg, &skills, 1);
    s.set_gender(&cg, 1);
    s.reset_skill_levels(&cg, &skills);

    // The default build -- six fifties and the heritage's free skills -- fits nothing.
    assert_eq!(
        Attr::ALL.map(|a| s.get(a)),
        [50; 6],
        "reset applies the default template"
    );
    s.fit_template_to_character(&cg);
    assert_eq!(
        s.template, 0,
        "under the 0.75 floor, so the Custom profession"
    );

    // Every real template reports itself once it has been applied.
    let n = cg.heritage_groups[&1].templates.len();
    assert_eq!(n, 7);
    for want in 1..i32::try_from(n).unwrap() {
        let mut s = CharGenState::default();
        s.reset(&cg, &skills);
        s.set_heritage_group(&cg, &skills, 1);
        s.set_gender(&cg, 1);
        s.set_template(&cg, &skills, want, true);
        let applied = s.template;
        s.fit_template_to_character(&cg);
        assert_eq!(
            s.template, want,
            "template {want} did not fit itself (applied {applied})"
        );
        // And it is a build ACE takes: the attribute total is the template's own.
        let total: i32 = Attr::ALL.iter().map(|a| s.get(*a)).sum();
        assert!(total <= 330, "template {want} spends {total} of 330");
        assert!(
            s.remaining_skill_credits >= 0,
            "template {want} overspends its skill credits"
        );
    }

    // A build one point away from a template still fits it: the ratio is 329/330 raised to 2.5.
    let mut s = CharGenState::default();
    s.reset(&cg, &skills);
    s.set_heritage_group(&cg, &skills, 1);
    s.set_gender(&cg, 1);
    s.set_template(&cg, &skills, 5, true);
    s.set_attribute(Attr::Focus, s.get(Attr::Focus) - 1);
    s.fit_template_to_character(&cg);
    assert_eq!(s.template, 5, "one point off Wayfarer is still Wayfarer");
}

// -------------------------------------------------------------------------------------------
// 2. Restore reaches the wire
// -------------------------------------------------------------------------------------------

/// Oracle: the retail restore request contains the id and **two empty strings**, as does
/// `pyreal_protocol`'s own `0xF7D9` transcription.
#[test]
fn restore_character_is_the_documented_three_field_message() {
    use pyreal_protocol::Message as _;
    let m = pyreal_protocol::admin::AdminSendAdminRestoreCharacter {
        iid: ObjectId(0x5000_0002),
        restored_char_name: String::new(),
        account_to_restore_to: String::new(),
    };
    let body = pyreal_protocol::write_body(&m).expect("encodes");
    assert_eq!(
        pyreal_protocol::admin::AdminSendAdminRestoreCharacter::OPCODE,
        pyreal_protocol::Opcode(0xF7D9)
    );
    // `[guid][""][""]`: four bytes of id then two empty packed strings.
    assert_eq!(&body[..4], &0x5000_0002u32.to_le_bytes());
    let back =
        pyreal_protocol::read_body::<pyreal_protocol::admin::AdminSendAdminRestoreCharacter>(&body)
            .expect("round-trips");
    assert_eq!(back, m);
}
