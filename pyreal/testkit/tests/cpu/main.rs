#![recursion_limit = "1024"]

//! CPU-tier scenarios for `pyreal-testkit`: no retail dats, no graphics device, no window.
//!
//! One test binary per crate per tier. The recorded captures under `fixtures/packet-captures` are
//! committed to the repository and are not a retail data file, so some scenarios here replay one;
//! nothing here opens `$PYREAL_TEST_DAT_DIR`.
//!
//! This binary may run its tests in parallel. The `dat` binary may not.
//!
//! One module per registry subject: a scenario lives in the file of the subject its behaviour row
//! is in, so a reader looking for what the client claims about the chat window finds it in one
//! place.

mod chat;
mod combat;
mod frame;
mod inventory;
mod login;
mod magic;
mod net;
mod network;
mod objects;
mod panels;
mod plugins;
mod shell;
mod social;
mod ui;
mod world;
