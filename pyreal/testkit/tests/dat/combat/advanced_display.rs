use super::*;

// ---------------------------------------------------------------------------------------------
// combat.advanced.*
//
// **No count and no word is pinned.** These walk the recording and assert the shape: the edge
// exists, the saves differ in one bit, the swings before it sit on notches and the ones after do
// not.
// ---------------------------------------------------------------------------------------------

pub(super) fn the_advanced_option_is_off_at_login_and_the_toggle_is_one_bit() {
    let recorded = advanced::the_recorded_session();
    let default_word = pyreal_client_model::player::options::DEFAULT_OPTIONS;
    let on_word = default_word | advanced::THE_OPTION_BIT;

    // The character's own description, as the shard sent it.
    let carries = |b: &[u8], w: u32| {
        b.windows(4)
            .any(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]) == w)
    };
    let described: Vec<&pyreal_client_net::client_session::testing::CorpusBlob> = recorded
        .blobs
        .iter()
        .filter(|b| {
            b.dir == pyreal_client_net::client_session::testing::Direction::ServerToClient
                && advanced::is_a_description(b)
        })
        .collect();
    let arrives_off = described.len() == 1
        && carries(&described[0].payload, default_word)
        && !carries(&described[0].payload, on_word);

    // Every option save the recorded client sent, in order.
    let saves: Vec<u32> = advanced::the_recorded_option_saves(&recorded);
    let flags: Vec<bool> = saves
        .iter()
        .map(|w| w & advanced::THE_OPTION_BIT != 0)
        .collect();
    let goes_on_and_off = flags.first() == Some(&true) && flags.last() == Some(&false);
    let one_bit = saves
        .iter()
        .all(|w| (w ^ default_word) & !advanced::THE_OPTION_BIT == 0);
    let off_is_the_default = saves.last() == Some(&default_word);

    // And the bit the table names is the one the wire moved.
    let (name, word, mask) = pyreal_client_model::player::options::PLAYER_OPTIONS
        [pyreal_client_model::player::options::option::ADVANCED_COMBAT_UI];
    let named = name == "AdvancedCombatUI"
        && word == pyreal_client_model::player::options::OptionWord::One
        && mask == advanced::THE_OPTION_BIT
        && !pyreal_client_model::player::options::default_option_value(
            pyreal_client_model::player::options::option::ADVANCED_COMBAT_UI,
        );

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.advanced.the-option-is-off-at-login-and-the-toggle-is-one-bit-on-the-wire",
        move |_| {
            arrives_off
                && saves.len() >= 2
                && goes_on_and_off
                && one_bit
                && off_is_the_default
                && named
        },
    );
}

pub(super) fn the_recorded_attacks_are_charges_after_the_toggle_and_notches_before() {
    let recorded = advanced::the_recorded_session();
    let on_at = advanced::when_the_option_went_on(&recorded)
        .expect("the recorded session turns the option on");
    let attacks = advanced::the_recorded_swings(&recorded);
    assert!(!attacks.is_empty(), "the recorded session carries swings");

    let (before, after): (Vec<_>, Vec<_>) = attacks.iter().partition(|(t, _)| *t < on_at);
    let both_halves = !before.is_empty() && !after.is_empty();

    // The gauge offers seven positions; a hair of tolerance, because the value crosses the wire
    // as a float and the client's own is the product of an integer step.
    let on_a_notch = |p: f32| (0u8..=6).any(|k| (p - f32::from(k) / 6.0).abs() < 1e-3);
    let off_the_notches: Vec<(f64, f32)> = before
        .iter()
        .filter(|(_, p)| !on_a_notch(*p))
        .copied()
        .collect();
    // The ones that are off the notches are the documented pairs: the release sends twice at one
    // instant, once at the power it was let go at and once capped.
    let every_odd_one_is_a_pair = off_the_notches
        .iter()
        .all(|(t, _)| before.iter().filter(|(u, _)| (u - t).abs() < 1e-6).count() == 2);
    let most_are_notches = off_the_notches.len() < before.len();

    let none_after_is = after.iter().all(|(_, p)| !on_a_notch(*p));
    let each_is_a_partial_hold = after.iter().all(|(_, p)| *p > 0.0 && *p < 1.0);
    // ...which is what a release before the bar fills looks like, because the bar is a second.
    let a_second = (pyreal_client_model::combat::POWER_BAR_SECONDS - 1.0).abs() < f64::EPSILON;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.advanced.the-recorded-attacks-are-charges-after-the-toggle-and-notches-before",
        move |_| {
            both_halves
                && most_are_notches
                && every_odd_one_is_a_pair
                && none_after_is
                && each_is_a_partial_hold
                && a_second
        },
    );
}
