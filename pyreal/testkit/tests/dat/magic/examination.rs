use super::*;
pub fn the_account_the_shard_names_is_the_one_the_client_keeps() {
    let mut c = examine::a_client();
    let before = c.view().world().player_system.account.clone();

    examine::greet(&mut c, AN_ACCOUNT);
    let first = c.view().world().player_system.account.clone();
    examine::greet(&mut c, ANOTHER_ACCOUNT);
    let second = c.view().world().player_system.account.clone();

    c.assert_behaviour(
        "spell-examine.formula.the-account-the-shard-names-is-the-one-the-client-keeps",
        move |_| before.is_empty() && first == AN_ACCOUNT && second == ANOTHER_ACCOUNT,
    );
    c.shutdown();
}

pub fn the_formula_the_client_would_cast_follows_that_account() {
    let mut c = examine::a_client();
    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);
    let stored = pyreal_client_model::magic::decrypt_formula(&spell.raw_comps, spell.comp_key);

    assert_eq!(
        spell.school, LIFE_MAGIC,
        "the shipped row is the Life spell this scenario drives"
    );
    assert_eq!(
        stored.iter().filter(|s| **s != 0).count(),
        8,
        "and its formula fills every slot"
    );

    let nameless = c.view().world().spell_formula(&spell);
    examine::greet(&mut c, AN_ACCOUNT);
    let mine = c.view().world().spell_formula(&spell);
    examine::greet(&mut c, ANOTHER_ACCOUNT);
    let theirs = c.view().world().spell_formula(&spell);

    c.assert_behaviour(
        "spell-examine.formula.the-formula-the-client-would-cast-follows-that-account",
        move |_| mine != nameless && mine != stored && theirs != mine && theirs != stored,
    );
    c.shutdown();
}

pub fn the_shipped_foci_table_reaches_the_game_model_with_the_description() {
    let mut c = examine::a_client();
    let shipped: Vec<(u32, u32)> = {
        let mut rows = c.view().expect_app().hud().school_pack_wcid.clone();
        rows.sort_unstable();
        rows
    };
    assert!(
        shipped.len() >= 5,
        "the shipped mapper names a foci for each of the spell schools"
    );
    assert!(
        shipped.iter().all(|(_, w)| *w != 0),
        "and every one is a real class"
    );

    let before: Vec<u32> = shipped
        .iter()
        .map(|(s, _)| c.view().world().school_of_magic_to_wcid(*s))
        .collect();
    examine::describe(&mut c);
    let after: Vec<u32> = shipped
        .iter()
        .map(|(s, _)| c.view().world().school_of_magic_to_wcid(*s))
        .collect();
    let want: Vec<u32> = shipped.iter().map(|(_, w)| *w).collect();
    // A school the shipped table has no foci for stays at nothing on either side.
    let unmapped: u32 = (1..64)
        .find(|s| !shipped.iter().any(|(k, _)| k == s))
        .expect("a school with none");
    let none = c.view().world().school_of_magic_to_wcid(unmapped);

    c.assert_behaviour(
        "spell-examine.formula.the-shipped-foci-table-reaches-the-game-model-with-the-description",
        move |_| before.iter().all(|w| *w == 0) && after == want && none == 0,
    );
    c.shutdown();
}

pub fn carrying_a_foci_changes_what_the_client_would_spend() {
    let mut c = examine::a_client();
    examine::greet(&mut c, AN_ACCOUNT);
    examine::describe(&mut c);
    let me = examine::stand_in_the_world(&mut c);
    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);

    let before = c.view().world().spell_formula(&spell);
    let foci = examine::foci_of(&c, LIFE_MAGIC);
    examine::carry_the_foci(&mut c, me, foci);
    let after = c.view().world().spell_formula(&spell);

    let before_len = before.iter().filter(|s| **s != 0).count();
    let after_len = after.iter().filter(|s| **s != 0).count();

    c.assert_behaviour(
        "spell-examine.formula.carrying-a-foci-changes-what-the-client-would-spend",
        move |_| before_len == 8 && after_len < before_len && after != before,
    );
    c.shutdown();
}

pub fn a_foci_for_another_school_leaves_the_long_formula_alone() {
    let mut c = examine::a_client();
    examine::greet(&mut c, AN_ACCOUNT);
    examine::describe(&mut c);
    let me = examine::stand_in_the_world(&mut c);
    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);

    let before = c.view().world().spell_formula(&spell);
    let wrong = examine::foci_of(&c, examine::another_school(&c, LIFE_MAGIC));
    assert_ne!(
        wrong,
        examine::foci_of(&c, LIFE_MAGIC),
        "the premise: it is another school's"
    );
    examine::carry_the_foci(&mut c, me, wrong);
    let after = c.view().world().spell_formula(&spell);

    c.assert_behaviour(
        "spell-examine.formula.a-foci-for-another-school-leaves-the-long-formula-alone",
        move |_| after == before && before.iter().filter(|s| **s != 0).count() == 8,
    );
    c.shutdown();
}
