use super::*;
// -------------------------------------------------------------------------------------------
// spellbook.removal and spellbook.the-bar-keeps
// -------------------------------------------------------------------------------------------

/// A character who knows five of the shipped table's spells plus [`KNOWN_BUT_UNDRAWABLE`], with
/// all six on the first tab of the bar -- delivered as the description the login sends.
fn a_character_with_six_favourites() -> (HeadlessClient, Vec<u32>) {
    use pyreal_protocol::archive::PackedHash;
    use pyreal_protocol::types::qualities::SpellBookPage;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let known: Vec<u32> = c
        .view()
        .hud()
        .spell_table
        .as_ref()
        .expect("the shipped spell table")
        .spells
        .keys()
        .copied()
        .take(5)
        .collect();
    assert_eq!(known.len(), 5, "the shipped table has spells");
    assert!(
        !c.view()
            .hud()
            .spell_table
            .as_ref()
            .expect("the shipped spell table")
            .spells
            .contains_key(&KNOWN_BUT_UNDRAWABLE),
        "and no row for the undrawable one"
    );

    let bar: Vec<u32> = known
        .iter()
        .copied()
        .chain([KNOWN_BUT_UNDRAWABLE])
        .collect();
    let page = SpellBookPage {
        casting_likelihood: 1.0,
        legacy: None,
    };
    let mut d = pyreal_protocol::login::LoginPlayerDescription::default();
    d.qualities.spell_book = Some(PackedHash {
        table_size: 8,
        entries: bar.iter().map(|id| (*id, page)).collect(),
    });
    d.player_module.spell_bars = {
        let mut bars = vec![Vec::new(); 8];
        bars[0] = bar.clone();
        bars
    };

    // The description is the player weenie's, so a described player needs one.
    c.world_mut()
        .seed_player_desc(A_PLAYER, pyreal_client_model::Qualities::default());
    c.when(Inbound::event(
        pyreal_client_net::client_session::SessionEvent::PlayerDescription(Box::new(d)),
    ));
    c.world_mut().player_system.spell_tabs[0] = bar;
    c.tick(1);
    (c, known)
}

/// The shard's removal reaches the book, the bar and the copy a relog reads.
pub fn a_removed_spell_leaves_the_book_the_bar_and_the_copy() {
    let (mut c, known) = a_character_with_six_favourites();
    let doomed = known[2];

    let known_before = c.view().hud().spells.iter().any(|s| s.id == doomed)
        && c.view().hud().stats.spells_removed == 0;
    let outbound_before = c.view().outbound().len();

    // The shard takes it away.
    c.when(Inbound::message(
        &pyreal_protocol::qualities::MagicRemoveSpell {
            layered_spell_id: doomed,
        },
    ));
    let book_lost_it = c.view().hud().stats.spells_removed == 1
        && !c.view().hud().spells.iter().any(|s| s.id == doomed)
        // and took out nothing else
        && c.view().hud().spells.len() == known.len() - 1;

    // The frame after it: the bar tells the shard it has lost the row.
    c.tick(1);
    let bar_followed = c.view().interaction().stats.spell_favorites_changed == 1;

    let pruned: Vec<(u32, i32)> = c.view().outbound()[outbound_before..]
        .iter()
        .filter_map(|r| match r {
            Request::RemoveSpellFavorite(m) => Some((m.spell_id, m.spell_bank)),
            _ => None,
        })
        .collect();

    let packed_lost_it = {
        let w = c.view().world();
        !w.player_system.spell_tabs[0].contains(&doomed)
            && !w
                .player_system
                .client_packed_module()
                .expect("a module to re-pack")
                .spell_bars[0]
                .contains(&doomed)
    };

    c.assert_behaviour(
        "spellbook.removal.reaches-the-book-the-bar-and-the-copy-a-relog-reads",
        move |_| {
            known_before
            && book_lost_it
            && bar_followed
            // one prune, one tab, one spell
            && pruned == vec![(doomed, 0)]
            && packed_lost_it
        },
    );
    c.shutdown();
}

/// A spell the shipped table cannot draw keeps its place on the bar.
pub fn the_bar_keeps_a_spell_the_table_cannot_draw() {
    let (mut c, _known) = a_character_with_six_favourites();
    let outbound_before = c.view().outbound().len();
    // A second frame, so that a prune that merely took one more pass would still show up.
    c.tick(1);

    let still_a_favourite =
        c.view().world().player_system.spell_tabs[0].contains(&KNOWN_BUT_UNDRAWABLE);
    let not_pruned = c.view().interaction().stats.spell_favorites_changed == 0;
    let nothing_sent = !c.view().outbound()[outbound_before..].iter().any(
        |r| matches!(r, Request::RemoveSpellFavorite(m) if m.spell_id == KNOWN_BUT_UNDRAWABLE),
    );

    c.assert_behaviour(
        "spellbook.the-bar-keeps-a-spell-the-shipped-table-cannot-draw",
        move |v| {
            still_a_favourite
            && not_pruned
            && nothing_sent
            // and the character really does still know it
            && v.hud().view(v.objects()).is_spell_known(KNOWN_BUT_UNDRAWABLE)
        },
    );
    c.shutdown();
}
