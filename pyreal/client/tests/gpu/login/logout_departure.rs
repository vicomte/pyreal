//! The recorded logout: each of the five recorded logout sessions is six seconds of one emote and
//! no dematerialise, and the logout emote is wire action index `0x11E`, which the final motion
//! table names `LogOut`.
//! Fixture: the five recorded logout sessions and the retail dats.

#![cfg(gpu)]

use pyreal_animation::MotionCommand;
use pyreal_client_net::client_session::testing::{Corpus, Direction};
use pyreal_primitives::ObjectId;
use pyreal_protocol::movement::MovementSetObjectMovement;
use pyreal_protocol::objects::ItemSetState;
use pyreal_protocol::Opcode;

/// The five recorded sessions that reach the world and then leave it. (`login-account-booted` is a bad
/// password and `ddd-interrogation-only` never enters.)
const LOGOUT_SESSIONS: [&str; 5] = [
    "first-login-walk-jump",
    "early-inventory-and-casting",
    "short-second-connection",
    "long-solo-play",
    "short-play-with-training",
];

/// Command index 286 — what every recorded logout `0xF74C` carries, what ACE calls
/// `MotionCommand.LogOut = 0x1000011E`, and the final retail client's `LogOut`.
const LOG_OUT_INDEX: u16 = 0x011E;
/// The 2013 client's own `LogOut`, three entries below it. See the oracle test.
const RETAIL_2013_LOG_OUT_INDEX: u16 = 0x011B;

/// The physics state each recorded logout `0xF74B` carries: `REPORT_COLLISIONS | GRAVITY |
/// EDGE_SLIDE`, and **`HIDDEN_PS` clear**.
const LOGOUT_STATE: u32 = 0x0040_0408;
/// Setting hidden ORs the physics state with `0x4000`.
const HIDDEN_PS: u32 = 0x0000_4000;

// ---------------------------------------------------------------------------------------------
// The oracles: the recording, asserted before anything is built on it
// ---------------------------------------------------------------------------------------------

/// The client's `0xF653` in a recorded session, and everything the server said about the player
/// between it and the server's answer.
struct Window {
    /// Seconds between the client's `0xF653` and the server's.
    length: f64,
    /// The command indices every `0xF74C` about the player carried, in order.
    motions: Vec<u16>,
    /// The state words every `0xF74B` about the player carried, in order.
    states: Vec<u32>,
    /// How many `0xF755` the server sent about **anything** inside the window.
    play_scripts: usize,
    /// Whether a `0xF658` arrives at or within one second after the server's answer.
    character_set_follows: bool,
}

fn logout_window(name: &str) -> Option<Window> {
    let c = Corpus::shared(name);
    let request = c.blobs.iter().find(|r| {
        r.dir == Direction::ClientToServer && r.opcode == Opcode::LOGIN_EXECUTE_LOG_OFF.0
    })?;
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: microseconds of a two-minute recording; f64 is exact well past this.
    let t0 = request.t_rel_micros as f64 / 1e6;
    let player = ObjectId(u32::from_le_bytes(request.payload[4..8].try_into().ok()?));

    let answer = c.blobs.iter().find(|r| {
        r.dir == Direction::ServerToClient
            && r.opcode == Opcode::LOGIN_EXECUTE_LOG_OFF.0
            && r.t_rel_micros > request.t_rel_micros
    })?;
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: as above.
    let t1 = answer.t_rel_micros as f64 / 1e6;

    let mut w = Window {
        length: t1 - t0,
        motions: Vec::new(),
        states: Vec::new(),
        play_scripts: 0,
        character_set_follows: false,
    };
    for row in c.blobs.iter().filter(|r| {
        r.dir == Direction::ServerToClient
            && r.t_rel_micros >= request.t_rel_micros
            && r.t_rel_micros <= answer.t_rel_micros
    }) {
        match Opcode(row.opcode) {
            Opcode::MOVEMENT_SET_OBJECT_MOVEMENT => {
                let Ok(m) = pyreal_protocol::read_body_padded::<MovementSetObjectMovement>(
                    &row.payload[4..],
                ) else {
                    continue;
                };
                if m.id != player {
                    continue;
                }
                let Ok(b) = m.decoded_movement() else {
                    continue;
                };
                if let Some(i) = b.body.interpreted {
                    w.motions.extend(i.actions.iter().map(|a| a.command_index));
                }
            }
            Opcode::ITEM_SET_STATE => {
                if let Ok(m) = pyreal_protocol::read_body_padded::<ItemSetState>(&row.payload[4..])
                {
                    if m.id == player {
                        w.states.push(m.state);
                    }
                }
            }
            Opcode::EFFECTS_PLAY_SCRIPT_TYPE => w.play_scripts += 1,
            _ => {}
        }
    }
    w.character_set_follows = c.blobs.iter().any(|r| {
        r.dir == Direction::ServerToClient
            && r.opcode == Opcode::LOGIN_LOGIN_CHARACTER_SET.0
            && r.t_rel_micros >= answer.t_rel_micros
            && r.t_rel_micros < answer.t_rel_micros + 1_000_000
    });
    Some(w)
}

/// **What the shard actually sends when a character leaves — measured, not assumed.**
///
/// Five recordings, one shape: six seconds, one `0xF74C` carrying one action, one `0xF74B` with
/// `HIDDEN_PS` clear, **no `0xF755` at all**, and the `0xF658` in the same instant as the answer.
#[test]
fn the_recorded_logout_window_is_six_seconds_of_one_emote_and_no_dematerialise() {
    let mut seen = 0usize;
    for name in LOGOUT_SESSIONS {
        let Some(w) = logout_window(name) else {
            continue;
        };
        seen += 1;
        assert!(
            (5.9..6.1).contains(&w.length),
            "{name}: the window is {:.4} s — ACE's `logOffRequestTime.AddSeconds(6)`",
            w.length
        );
        assert_eq!(
            w.motions,
            vec![LOG_OUT_INDEX],
            "{name}: exactly one motion about the player, and it is the logout emote"
        );
        assert!(
            !w.states.is_empty(),
            "{name}: the state re-broadcast is there"
        );
        for s in &w.states {
            assert_eq!(*s, LOGOUT_STATE, "{name}: the recorded logout state word");
            assert_eq!(
                s & HIDDEN_PS,
                0,
                "{name}: HIDDEN_PS is CLEAR — nothing is hidden"
            );
        }
        assert_eq!(
            w.play_scripts, 0,
            "{name}: no 0xF755 in the window — this shard asks for no dematerialise, which is \
             `Player.LogOut_Final`'s own shape and not a decode gap"
        );
        assert!(
            w.character_set_follows,
            "{name}: the 0xF658 arrives with the answer"
        );
    }
    assert_eq!(
        seen,
        LOGOUT_SESSIONS.len(),
        "all five recorded logouts must be readable"
    );
}

/// **The clap's wire index is `0x11E`, and the final retail table names it `LogOut`.**
///
/// The recordings were made by the 2013 client, whose command table had three fewer rows below
/// this one: it called `0x11E` `DoubleSlashHigh` and numbered its own `LogOut` `0x11B`. The server
/// sends `0x11E`, because the server's numbering is the final client's. The client uses the final
/// table, so the name and the wire agree, and the 2013 numbering survives only as the 2013 map,
/// which is what reads a 2013 index.
#[test]
fn the_logout_emote_is_wire_index_0x11e_which_the_final_table_names_log_out() {
    let on_the_wire = MotionCommand::from_index(LOG_OUT_INDEX).expect("index 286 is in the table");
    assert_eq!(
        on_the_wire.0, 0x1000_011E,
        "the id the shard broadcasts — ACE's MotionCommand.LogOut and the recorded logout motion"
    );
    assert_eq!(
        on_the_wire.to_index(),
        Some(LOG_OUT_INDEX),
        "and it round-trips, so a client that plays it plays the shard's own animation"
    );
    assert_eq!(
        on_the_wire,
        MotionCommand::LOG_OUT,
        "and it is the table's LogOut"
    );
    assert_eq!(on_the_wire.name(), Some("LogOut"));

    // The 2013 fact, kept explicit: that client's `LogOut` index maps onto this one.
    assert_eq!(
        pyreal_animation::command::retail_2013_index_to_current(RETAIL_2013_LOG_OUT_INDEX),
        Some(LOG_OUT_INDEX),
        "the 2013 client's LogOut (0x11B) is the final table's 0x11E"
    );
}
