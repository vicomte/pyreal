//! GPU-tier tests for `pyreal-client`, including DAT and CPU controls beside the device fixtures.
//! `--features retail-dats,wgpu` enables the device and the tier switch.
//!
//! One test binary per crate per tier: run one area or module with
//! `--test gpu <area>::<module>::`.

mod common;

mod camera;
mod chat;
mod combat;
mod inventory;
mod login;
mod magic;
mod movement;
mod net;
mod objects;
mod panels;
mod presentation;
mod selection;
mod ui;
mod world;
