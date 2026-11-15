//! CPU-tier tests for `pyreal-clipboard`: no retail dats, no GPU device.
//!
//! `format` covers the clipboard format and payload a write asks for; `roundtrip` writes the real
//! system clipboard and is ignored by default (`cargo test -p pyreal-clipboard --test cpu
//! roundtrip:: -- --ignored --test-threads=1`).

mod format;
mod roundtrip;
