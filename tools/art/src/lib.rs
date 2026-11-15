//! Surveys the models the world is built from: where each one is used, how often, what it is
//! made of, and a picture of it; and packs an art pack's folder into one file.
//!
//! **Depends on** `pyreal-primitives`, `pyreal-dat` and `pyreal-assets` (the retail files and
//! their records), `pyreal-world-data` (the interior cells and the buildings they belong to),
//! `pyreal-terrain` (the landblock mesh), `pyreal-client-runtime` (a landblock's outdoor contents
//! placed as the client places them, and the models' parts and triangles), `pyreal-scene` (the
//! pictures, decoded as the client decodes them, and an art pack opened and packed as the client
//! opens it, with no device) and `pyreal-codecs` (the surface record's type bits). **Used
//! by** nothing: it is the `pyreal-art` program.
//!
//! **Must never** write anywhere but the output folder or packed file it is given, or depend on a
//! server crate: the server's spawns arrive as a table the server's own importer writes.
//!
//! [`pack`] packs an art pack's folder as the client opens it. [`skeleton`] writes a setup as a
//! body to rig against.
//!
//! [`survey::run`] is the whole survey: [`world`] walks every landblock into a [`tally::Tally`],
//! [`character`] adds the bodies and the clothing, [`spawns`] reads the server's table,
//! [`model`] says what each model is made of, [`thumb`] draws its picture, [`pictures`] says what
//! each retail picture is drawn on and [`output`] writes the tables, the summary and the report.

#![doc(html_no_source)]

pub mod character;
pub mod fingerprint;
pub mod model;
pub mod output;
pub mod pack;
pub mod pictures;
pub mod skeleton;
pub mod spawns;
pub mod survey;
pub mod tally;
pub mod thumb;
pub mod world;

#[cfg(test)]
mod tests;
