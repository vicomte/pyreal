//! The spellbook, the enchantment registry, the cast and its two failures, the resist, the vitae
//! and the spell formula, over the retail dats. Every scenario builds a whole client
//! (`App::with_platform` over a headless platform) or reads the shipped tables through one, and
//! **this binary runs serially**: two headless clients in one process share process-wide state.
//! An enchantment's start is rebased on receipt, so most of these move the session clock with a
//! `pyreal_testkit::Peer`: it sends the shard's own time-sync header, delivers ordered game events
//! and replays recorded blobs re-addressed to this session, over the socket-free endpoint the
//! harness installs. Nothing is bound and no datagram leaves this process.

use pyreal_client_contract::GameView as _;
use pyreal_client_model::Request;
use pyreal_primitives::num::math;
use pyreal_testkit::{ClientSpec, HeadlessClient, Inbound};

pyreal_testkit::scenarios! {
    scenario_a_removed_spell_leaves_the_book_the_bar_and_the_copy => a_removed_spell_leaves_the_book_the_bar_and_the_copy ["spellbook.removal.reaches-the-book-the-bar-and-the-copy-a-relog-reads"],
    scenario_the_bar_keeps_a_spell_the_table_cannot_draw => the_bar_keeps_a_spell_the_table_cannot_draw ["spellbook.the-bar-keeps-a-spell-the-shipped-table-cannot-draw"],
    scenario_a_timed_buff_shows_its_real_remaining_time_across_a_relog => a_timed_buff_shows_its_real_remaining_time_across_a_relog ["enchantments.duration.a-timed-buff-shows-its-real-remaining-time-across-a-relog"],
    scenario_the_row_survives_its_own_duration_running_out => the_row_survives_its_own_duration_running_out ["enchantments.expiry.the-row-stays-until-the-shard-takes-it-and-not-when-its-time-runs-out"],
    scenario_the_shards_removal_empties_the_pane_and_the_skill => the_shards_removal_empties_the_pane_and_the_skill ["enchantments.removal.takes-the-row-out-of-the-pane-the-skill-back-to-base-and-says-so"],
    scenario_a_dispel_empties_the_same_panels_without_a_line => a_dispel_empties_the_same_panels_without_a_line ["enchantments.dispel.empties-the-same-two-panels-and-says-nothing"],
    scenario_a_second_buff_reaches_an_already_open_pane => a_second_buff_reaches_an_already_open_pane ["enchantments.pane.a-second-buff-reaches-a-pane-that-is-already-open"],
    scenario_a_recorded_resist_is_drawn_verbatim_on_the_magic_channel => a_recorded_resist_is_drawn_verbatim_on_the_magic_channel ["magic.resist.the-shards-own-sentence-is-drawn-verbatim-on-the-magic-channel"],
    scenario_a_squelched_magic_channel_drops_the_same_bytes => a_squelched_magic_channel_drops_the_same_bytes ["magic.resist.a-squelched-magic-channel-drops-the-same-recorded-bytes"],
    scenario_the_resist_sound_reaches_the_clients_queue => the_resist_sound_reaches_the_clients_queue ["magic.resist.the-sound-that-comes-with-it-reaches-the-clients-own-queue"],
    scenario_a_fizzle_ends_the_cast_prints_its_line_and_burns_nothing => a_fizzle_ends_the_cast_prints_its_line_and_burns_nothing ["magic.cast.a-fizzle-ends-the-cast-prints-its-line-and-burns-nothing"],
    scenario_a_recorded_fizzle_prints_the_line_without_ending_the_cast => a_recorded_fizzle_prints_the_line_without_ending_the_cast ["magic.cast.the-shards-recorded-fizzle-prints-the-same-line-and-does-not-end-the-cast"],
    scenario_a_refusal_the_client_makes_itself_never_reaches_the_shard => a_refusal_the_client_makes_itself_never_reaches_the_shard ["magic.cast.a-refusal-the-client-makes-itself-is-said-in-its-own-words-and-never-sent"],
    scenario_a_live_vitae_lights_the_lamp_and_fills_the_panel => a_live_vitae_lights_the_lamp_and_fills_the_panel ["vitae.lamp.lights-for-a-live-penalty-and-the-panel-says-how-much"],
    scenario_the_penalty_takes_the_skill_down_but_leaves_the_row_plain => the_penalty_takes_the_skill_down_but_leaves_the_row_plain ["vitae.penalty.takes-the-skill-down-leaves-the-row-plain-and-spares-the-attributes"],
    scenario_the_account_the_shard_names_is_the_one_the_client_keeps => the_account_the_shard_names_is_the_one_the_client_keeps ["spell-examine.formula.the-account-the-shard-names-is-the-one-the-client-keeps"],
    scenario_the_formula_the_client_would_cast_follows_that_account => the_formula_the_client_would_cast_follows_that_account ["spell-examine.formula.the-formula-the-client-would-cast-follows-that-account"],
    scenario_the_shipped_foci_table_reaches_the_game_model_with_the_description => the_shipped_foci_table_reaches_the_game_model_with_the_description ["spell-examine.formula.the-shipped-foci-table-reaches-the-game-model-with-the-description"],
    scenario_carrying_a_foci_changes_what_the_client_would_spend => carrying_a_foci_changes_what_the_client_would_spend ["spell-examine.formula.carrying-a-foci-changes-what-the-client-would-spend"],
    scenario_a_foci_for_another_school_leaves_the_long_formula_alone => a_foci_for_another_school_leaves_the_long_formula_alone ["spell-examine.formula.a-foci-for-another-school-leaves-the-long-formula-alone"],
    scenario_the_component_a_slot_stands_for_is_a_representative_of_its_kind => the_component_a_slot_stands_for_is_a_representative_of_its_kind ["spell-examine.components.the-answer-is-a-representative-of-the-class-and-not-one-object"],
    scenario_the_notice_moves_for_a_component_and_for_nothing_else => the_notice_moves_for_a_component_and_for_nothing_else ["spell-examine.marks.the-notice-moves-for-a-component-and-for-nothing-else"],
}

// -------------------------------------------------------------------------------------------
// spellbook.removal.reaches-the-book-the-bar-and-the-copy-a-relog-reads
// spellbook.the-bar-keeps-a-spell-the-shipped-table-cannot-draw
// -------------------------------------------------------------------------------------------
//
// The spells are the shipped table's own and are never written here, and the prune the bar
// raises is consumed by the client's own request dispatch.

/// An id the shipped spell table has no row for. The character knows it; nothing can draw it.
const KNOWN_BUT_UNDRAWABLE: u32 = 0xFFFF_FFF0;
/// The player the description below belongs to.
const A_PLAYER: pyreal_primitives::ObjectId = pyreal_primitives::ObjectId(0x5000_000A);

mod support;

// -------------------------------------------------------------------------------------------
// magic.resist.*
//
// The recording's own bytes are replayed on the queue it puts them on; the sentence a scenario
// expects is the blob's own decoded text and never a constant, because the recording's first
// resist and its fourth name two different monsters and a scenario that wrote either down would
// be asserting its own transcription. Which recordings carry a resist is the first scenario's
// premise, read at run time.
// -------------------------------------------------------------------------------------------

/// The chat channel the shard marks a resist with.
const MAGIC_CHANNEL: u8 = 7;
/// The sound the shard plays with it.
const RESIST_SOUND: i32 = 0x91;

/// The bench the three casting scenarios share: a client whose character carries the whole of
/// one spell's formula, with a legal target and an illegal one in the world.
mod cast;

/// The fixture the vitae scenarios share.
mod vitae;

// ---------------------------------------------------------------------------------------------
// spell-examine.formula.*
//
// The per-account taper formula, and what carrying a foci changes in it. The spell **ids** below
// are the only data values here; everything else is read off the shipped tables, and the
// formulas are derived through `pyreal_client_model::magic`, whose own arithmetic
// `pyreal-client-model` tests.
// ---------------------------------------------------------------------------------------------

/// `Flame Bolt I` -- War Magic, a bolt: five components.
const FLAME_BOLT: u32 = 27;
/// `Regeneration Self V` -- Life Magic, and an eight-slot formula two of whose slots are tapers.
const REGENERATION_SELF_V: u32 = 169;
/// Life Magic, which is the school the foci these scenarios carry belongs to.
const LIFE_MAGIC: u32 = 2;
/// Synthetic. Two strings invented here; neither is an account.
const AN_ACCOUNT: &str = "derer66done";
const ANOTHER_ACCOUNT: &str = "derer66dtwo";

/// The account, the pack and the foci the formula scenarios stand up.
mod examine;

// ---------------------------------------------------------------------------------------------
// spell-examine.components.* and spell-examine.marks.*
//
// What a formula slot stands for in the player's pack. **The pack is filled through the client's
// own move seam**; nothing here adds a row to the component tracker by hand.
// ---------------------------------------------------------------------------------------------

/// `Acid Stream V` -- War Magic, an eight-slot formula, every slot with an icon.
const ACID_STREAM_V: u32 = 62;

mod spellbook;
use spellbook::{
    a_removed_spell_leaves_the_book_the_bar_and_the_copy,
    the_bar_keeps_a_spell_the_table_cannot_draw,
};

mod enchantments;
use enchantments::{
    a_dispel_empties_the_same_panels_without_a_line, a_second_buff_reaches_an_already_open_pane,
    a_timed_buff_shows_its_real_remaining_time_across_a_relog,
    the_row_survives_its_own_duration_running_out,
    the_shards_removal_empties_the_pane_and_the_skill,
};

mod cast_feedback;
use cast_feedback::{
    a_fizzle_ends_the_cast_prints_its_line_and_burns_nothing,
    a_recorded_fizzle_prints_the_line_without_ending_the_cast,
    a_recorded_resist_is_drawn_verbatim_on_the_magic_channel,
    a_refusal_the_client_makes_itself_never_reaches_the_shard,
    a_squelched_magic_channel_drops_the_same_bytes, the_resist_sound_reaches_the_clients_queue,
};

mod vitae_cases;
use vitae_cases::{
    a_live_vitae_lights_the_lamp_and_fills_the_panel,
    the_penalty_takes_the_skill_down_but_leaves_the_row_plain,
};

mod examination;
use examination::{
    a_foci_for_another_school_leaves_the_long_formula_alone,
    carrying_a_foci_changes_what_the_client_would_spend,
    the_account_the_shard_names_is_the_one_the_client_keeps,
    the_formula_the_client_would_cast_follows_that_account,
    the_shipped_foci_table_reaches_the_game_model_with_the_description,
};

mod components;
use components::{
    the_component_a_slot_stands_for_is_a_representative_of_its_kind,
    the_notice_moves_for_a_component_and_for_nothing_else,
};
