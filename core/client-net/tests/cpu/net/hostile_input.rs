//! Contracts for recorded traffic and damaged copies of it.
//! Fixture: shared recorded messages and recorded datagrams.

#![cfg(feature = "replay")]

use std::collections::BTreeSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use pyreal_client_net::client_session::testing::{session_names, Corpus, Direction};
use pyreal_client_net::net::{Net, NetConfig};
use pyreal_client_net::replay::{self, Capture};
use pyreal_primitives::{LocalTime, Transport};
use pyreal_protocol::codecs::{read_blob, Dir};
use pyreal_transport::wire::{PacketFlags, ParsedPacket};

fn captures_dir() -> PathBuf {
    const REL: &str = "fixtures/packet-captures";
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join(REL).join("index.json").is_file() {
            return dir.join(REL);
        }
        if !dir.pop() {
            return PathBuf::from(REL);
        }
    }
}

fn dir_of(d: Direction) -> Dir {
    match d {
        Direction::ServerToClient => Dir::S2c,
        Direction::ClientToServer => Dir::C2s,
    }
}

/// Every recorded message decodes, exactly, through the run-time codecs. The one exception is the
/// 44-byte all-zero blob ACE sent in `requested-death-vitae-salvage`, which has no opcode.
#[test]
fn every_recorded_message_decodes() {
    let mut failed = Vec::new();
    let mut total = 0usize;
    for corpus in Corpus::shared_all() {
        for b in &corpus.blobs {
            total += 1;
            let read = read_blob(dir_of(b.dir), &b.payload).expect("a type dword");
            if read.fields.is_none() && b.payload.iter().any(|x| *x != 0) {
                failed.push(format!(
                    "{} blob {}: {:#06X}",
                    corpus.name, b.idx, read.mtype
                ));
            }
        }
    }
    assert!(total > 40_000, "the corpus is loaded ({total} blobs)");
    assert!(failed.is_empty(), "{} undecoded: {failed:?}", failed.len());
}

/// No truncation of any recorded message makes a codec panic: one sample of each type each way.
#[test]
fn no_truncated_message_panics_a_decoder() {
    let mut seen = BTreeSet::new();
    for corpus in Corpus::shared_all() {
        for b in &corpus.blobs {
            let dir = dir_of(b.dir);
            let Some(read) = read_blob(dir, &b.payload) else {
                continue;
            };
            if !seen.insert((b.dir == Direction::ServerToClient, read.mtype)) {
                continue;
            }
            for n in 0..b.payload.len() {
                let cut = &b.payload[..n];
                assert!(
                    catch_unwind(AssertUnwindSafe(|| read_blob(dir, cut))).is_ok(),
                    "{} blob {} cut to {n} bytes panics",
                    corpus.name,
                    b.idx
                );
            }
        }
    }
}

/// A damaged copy of each server datagram, arriving just before the real one, is refused without
/// moving the sequence window or the key stream: the real stream delivers exactly what it does
/// alone. Damage in the sequence number, the checksum and a fragment's payload.
#[test]
fn a_damaged_datagram_does_not_disturb_the_stream() {
    for name in session_names() {
        let capture = Capture::load(&captures_dir(), name).expect("capture");
        let Some(cr) = capture.packets.iter().position(|d| {
            d.dir == replay::Direction::S2c
                && ParsedPacket::parse(&d.raw)
                    .is_ok_and(|p| p.header.header.contains(PacketFlags::CONNECT_REQUEST))
        }) else {
            continue;
        };
        let rec_id = ParsedPacket::parse(&capture.packets[cr].raw)
            .expect("parses")
            .header
            .rec_id;
        let run = |damage_at: Option<usize>| {
            let mut n = Net::new(NetConfig::default());
            let m = &capture.meta;
            n.add_connection(
                rec_id,
                m.net_id,
                m.iteration,
                m.outgoing_seed,
                m.incoming_seed,
                None,
            );
            let mut accepted = 0usize;
            let mut blobs = Vec::new();
            for dg in capture.packets.iter().skip(cr + 1) {
                if dg.dir != replay::Direction::S2c {
                    continue;
                }
                let now = LocalTime(dg.t_rel);
                if let Some(at) = damage_at {
                    let mut bad = dg.raw.clone();
                    let i = at % bad.len();
                    bad[i] ^= 0x10;
                    let _ = n.feed(&bad, None, now);
                }
                if n.feed(&dg.raw, None, now).is_ok() {
                    accepted += 1;
                }
                while let Some(msg) = n.poll() {
                    blobs.push((msg.opcode, msg.body));
                }
            }
            (accepted, blobs)
        };
        let clean = run(None);
        for at in [1, 9, 44] {
            assert!(
                run(Some(at)) == clean,
                "{name}: damage at byte {at} changed the stream"
            );
        }
    }
}
