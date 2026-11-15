use pyreal_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};

/// The recorded session that carries the option going on and off again.
const A_RECORDED_SESSION: &str = "melee-attack-run";
/// The bit the option lives in, written as a literal rather than read back through the same
/// table the client uses, so a wrong mask on either side cannot hide behind the symbol.
pub const THE_OPTION_BIT: u32 = 0x0000_1000;
/// The envelope a client's own game action leaves in, and where its sub-opcode sits.
const A_CLIENT_ACTION: u32 = 0xF7B1;
const ACTION_SUB_TYPE: usize = 8;
/// The envelope the shard's ordered events arrive in, and where their sub-opcode sits.
const AN_ORDERED_EVENT: u32 = 0xF7B0;
const EVENT_SUB_TYPE: usize = 12;
/// The message that saves the whole option word, and where that word sits in its body.
const SAVES_THE_OPTIONS: u32 = 0x01A1;
const OPTIONS_WORD: usize = 16;
/// The character's own description.
const A_DESCRIPTION: u32 = 0x0013;
/// The swing, and where the power it went out at sits in its body.
const A_MELEE_SWING: u32 = 0x0008;
const SWING_POWER: usize = 20;

fn dword(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

pub fn the_recorded_session() -> Corpus {
    Corpus::load(A_RECORDED_SESSION)
        .expect("the recording parses")
        .expect("the decoded corpus carries it")
}

pub fn is_a_description(b: &CorpusBlob) -> bool {
    dword(&b.payload, 0) == Some(AN_ORDERED_EVENT)
        && dword(&b.payload, EVENT_SUB_TYPE) == Some(A_DESCRIPTION)
}

fn is_an_option_save(b: &CorpusBlob) -> bool {
    b.dir == Direction::ClientToServer
        && dword(&b.payload, 0) == Some(A_CLIENT_ACTION)
        && dword(&b.payload, ACTION_SUB_TYPE) == Some(SAVES_THE_OPTIONS)
}

/// The option word of every save the recorded client sent, in recorded order.
pub fn the_recorded_option_saves(c: &Corpus) -> Vec<u32> {
    c.blobs
        .iter()
        .filter(|b| is_an_option_save(b))
        .filter_map(|b| dword(&b.payload, OPTIONS_WORD))
        .collect()
}

/// When the recorded player turned the option on.
pub fn when_the_option_went_on(c: &Corpus) -> Option<f64> {
    c.blobs
        .iter()
        .filter(|b| is_an_option_save(b))
        .find(|b| dword(&b.payload, OPTIONS_WORD).is_some_and(|w| w & THE_OPTION_BIT != 0))
        .map(seconds)
}

/// `(when, the power it went out at)` for every swing the recorded client sent.
pub fn the_recorded_swings(c: &Corpus) -> Vec<(f64, f32)> {
    c.blobs
        .iter()
        .filter(|b| {
            b.dir == Direction::ClientToServer
                && dword(&b.payload, 0) == Some(A_CLIENT_ACTION)
                && dword(&b.payload, ACTION_SUB_TYPE) == Some(A_MELEE_SWING)
        })
        .filter_map(|b| {
            let p = b.payload.get(SWING_POWER..SWING_POWER + 4)?;
            Some((seconds(b), f32::from_le_bytes([p[0], p[1], p[2], p[3]])))
        })
        .collect()
}

fn seconds(b: &CorpusBlob) -> f64 {
    std::time::Duration::from_micros(b.t_rel_micros).as_secs_f64()
}
