use super::*;

// =============================================================================================
// A message that reached its handler, measured at both ends. Every message is the recording's own.
// =============================================================================================

/// The locked chest the recording unlocks and locks again, and the two moments.
const LOCKED_CHEST: ObjectId = ObjectId(0x77F0_3053);
const UNLOCKED_AT: usize = 4114;
const RELOCKED_AT: usize = 4274;

/// Whether the client believes `id` can be opened.
fn believes_it_opens(c: &HeadlessClient, id: ObjectId) -> bool {
    c.view()
        .world()
        .weenie(id)
        .unwrap_or_else(|| panic!("{id:?} is created by the recording"))
        .pwd
        .bitfield
        & pyreal_rules::weenie::bitfield::OPENABLE
        != 0
}

// ---------------------------------------------------------------------------------------------
// container.ground.the-shards-unlock-reaches-the-chest-and-the-relock-follows-it
// ---------------------------------------------------------------------------------------------

/// The shard saying a chest is unlocked reaches the chest, and saying it is locked again reaches
/// it too.
///
/// The control is the same run: before the unlock arrives the client believes the chest is
/// locked, so "it opens" afterwards cannot be the state the chest was created with. The relock
/// is the other half and it is what makes this a live receiver rather than a one-way switch --
/// a client that dropped these leaves one stale flag, and the chest is dead for the rest of the
/// session however many times the player uses the right key on it.
pub(super) fn the_shards_unlock_reaches_the_chest_and_the_relock_follows_it() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.when(Inbound::from_corpus("long-solo-play", 0..UNLOCKED_AT))
        .tick(1);
    let locked_to_begin_with = !believes_it_opens(&c, LOCKED_CHEST);

    c.when(Inbound::from_corpus(
        "long-solo-play",
        UNLOCKED_AT..UNLOCKED_AT + 1,
    ))
    .tick(1);
    let the_unlock_landed = believes_it_opens(&c, LOCKED_CHEST);

    c.when(Inbound::from_corpus(
        "long-solo-play",
        UNLOCKED_AT + 1..RELOCKED_AT + 1,
    ))
    .tick(1);
    let the_relock_landed = !believes_it_opens(&c, LOCKED_CHEST);

    c.assert_behaviour(
        "container.ground.the-shards-unlock-reaches-the-chest-and-the-relock-follows-it",
        move |_| locked_to_begin_with && the_unlock_landed && the_relock_landed,
    );
    c.shutdown();
}
