//! CPU-tier tests for `pyreal-input`: no retail dats, no GPU device.
//!
//! Every integration test of this crate reads the shipped input maps, so they are all in the `dat`
//! tier and this binary declares no module. It exists so that `cargo test -p pyreal-input --test
//! cpu` is the same command in every crate.
