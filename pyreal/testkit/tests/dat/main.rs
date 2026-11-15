#![recursion_limit = "1024"]

//! DAT-tier scenarios for `pyreal-testkit`: each one opens the retail dats under `$PYREAL_TEST_DAT_DIR`
//! and builds a whole headless client over them.
//!
//! **This binary must run serially** (`-- --test-threads=1`): two headless clients in one process
//! share process-wide state.
//!
//! One module per registry subject, as in the `cpu` binary. `goldens` diffs two runs' frame logs
//! against the committed golden files.

mod chat;
mod combat;
mod frame;
mod goldens;
mod inventory;
mod login;
mod magic;
mod net;
mod objects;
mod panels;
mod plugins;
mod shell;
mod social;
mod ui;
mod world;
