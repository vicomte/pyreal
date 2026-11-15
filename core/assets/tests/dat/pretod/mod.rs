//!
//! The pre-Throne-of-Destiny portal under `PYREAL_TEST_PRETOD_DAT_DIR`, read whole. With
//! `PYREAL_TEST_PRETOD_REFERENCE` (a directory of independently exported art, creation tables and
//! face textures from the same portal) and `PYREAL_TEST_PRETOD_REFERENCE_PREVIEW` (a directory holding
//! an independently converted `client_portal.dat` and its `manifest.json`), each result is also
//! compared with those exports byte for byte.
//!
//! The tests are ignored without the `retail-dats` feature. With it, a test whose portal is not
//! named prints `NOT RUN` and passes, and a comparison whose reference is not named prints
//! `NOT RUN` for that comparison and still checks everything the portal alone can show.

use std::path::PathBuf;
use std::sync::OnceLock;

use pyreal_assets::pretod::ClassicPortal;

mod art;
mod creation_tables;

/// The portal, opened once per binary, or `None` (with a `NOT RUN` line) when it is not named.
fn portal(test: &str) -> Option<&'static ClassicPortal> {
    static PORTAL: OnceLock<Option<ClassicPortal>> = OnceLock::new();
    let portal = PORTAL.get_or_init(|| {
        let dir = pyreal_dat::testing::classic_dat_dir()?;
        Some(ClassicPortal::open(&dir.join("portal.dat")).expect("the February 2005 portal opens"))
    });
    if portal.is_none() {
        eprintln!("NOT RUN: {test}: PYREAL_TEST_PRETOD_DAT_DIR names no February 2005 dats");
    }
    portal.as_ref()
}

/// A reference directory from the environment, or `None` (with a `NOT RUN` line).
fn reference(var: &str, what: &str) -> Option<PathBuf> {
    let dir = std::env::var_os(var).map(PathBuf::from);
    if dir.is_none() {
        eprintln!("NOT RUN: {what}: {var} is not set");
    }
    dir
}

fn read_json(path: &std::path::Path) -> serde_json::Value {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}
