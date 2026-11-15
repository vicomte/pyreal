//! The role-neutral UDP transport a client and a server both speak: packet framing, the checksums,
//! the ISAAC key stream, fragments and reassembly, retransmission and the handshake records.
//!
//! **Depends on** `pyreal-primitives`. **Used by** the client's connection (`pyreal-client-net`),
//! the client runtime, the client and its test kit, and the capture tools
//! (`dereth-pcap`, `dereth-corpus`).
//!
//! **Must never** open a socket or look past a blob's first byte: the socket is the caller's, and
//! message bodies belong to `pyreal-protocol`.
//!
//! Framing and the optional headers are [`wire`], the checksums [`crc`], the key stream [`isaac`],
//! fragments and blobs [`blob`], reassembly [`indicator`], the per-connection receive state and the
//! retransmit cache [`session`], the send path [`flow`], the handshake, referral and server-switch
//! records [`conn`], and the status ping a launcher asks a world's live status with
//! [`status_ping`] (not retail's).
//! The four rules most likely to silently break a rebuild:
//!
//! - A packet is encrypted **exactly when** it carries fragments or a non-disposable optional
//!   header, and the peer verifies the equivalence bit for bit ([`wire::packet`]).
//! - Optional headers appear in **ascending flag-mask order**, and the per-section checksum depends
//!   on it ([`wire::optional`]).
//! - Retransmit keys are **positional**: the receiver parks a drawn key for every skipped sequence
//!   number and the sender reuses its stored key verbatim ([`session`]); ACE's scanning search is
//!   an approximation and is not ported.
//! - There is **no outbound flow control**: the `Flow` header is telemetry only ([`flow`]).
//!
//! **Specified in** `docs/networking/01-packet-format.md` (the datagram, its headers, checksum and
//! cipher), `docs/networking/02-reliability-and-flow.md` (sequencing, acknowledgement and
//! retransmission), `docs/networking/03-connection-state-machine.md` (the handshake and connection
//! states), `docs/networking/04-netblobs-and-queues.md` (message blobs and reassembly) and
//! `docs/networking/06-status-ping.md` (the status ping).

pub mod blob;
pub mod conn;
pub mod crc;
pub mod flow;
pub mod indicator;
pub mod isaac;
pub mod session;
pub mod status_ping;
pub mod wire;

pub use crc::{hash32, header_hash_bytes, payload_hash, wire_checksum};
pub use isaac::{CryptoSystem, Isaac};
pub use wire::{
    Fragment, FragmentHeader, OutPacket, PacketFlags, ParsedPacket, ProtoHeader, WireError,
};
