//! Synthetic dats and installs, and one world list's answers, for tests in this crate and the ones
//! above it.

use std::path::Path;

use crate::datset::{DatRole, Iterations};

pub use crate::dat::testdat::{build as dat_bytes, eor as eor_dat_bytes};

/// Write a Classic pair, `portal.dat` and `cell.dat`, with the given iterations.
pub fn write_classic_set(dir: &Path, portal: u32, cell: u32) {
    std::fs::create_dir_all(dir).expect("create the test folder");
    std::fs::write(
        dir.join("portal.dat"),
        crate::dat::testdat::pre_tod(false, portal),
    )
    .expect("write a test dat");
    std::fs::write(
        dir.join("cell.dat"),
        crate::dat::testdat::pre_tod(true, cell),
    )
    .expect("write a test dat");
}

/// Write a folder of the four dats with the given iteration counts. Missing counts are missing files.
pub fn write_dat_set(dir: &Path, it: Iterations) {
    std::fs::create_dir_all(dir).expect("create the test folder");
    for (role, set, sub) in [
        (DatRole::Portal, 1, 0),
        (DatRole::Cell, 2, 1),
        (DatRole::Local, 3, 1),
        (DatRole::Highres, 1, 0x6946_6948),
    ] {
        if let Some(n) = it.get(role) {
            std::fs::write(
                dir.join(role.file_name()),
                crate::dat::testdat::eor(set, sub, n),
            )
            .expect("write a test dat");
        }
    }
}

/// A plugin's zip: `uses` is its manifest's tail (its permissions and web domains).
#[must_use]
pub fn plugin_zip(id: &str, version: &str, api: &str, uses: &str) -> Vec<u8> {
    let manifest = format!(
        "id = \"{id}\"\nname = \"Meter\"\nversion = \"{version}\"\napi = \"{api}\"\n{uses}"
    );
    pyreal_plugin_package::Package {
        origin: String::new(),
        files: [
            ("plugin.toml".to_owned(), manifest.into_bytes()),
            ("main.lua".to_owned(), b"print(1)".to_vec()),
        ]
        .into(),
    }
    .to_zip()
}

/// A source's `plugins.json` listing `zips`, each at `<id>.zip` beside the list.
#[must_use]
pub fn plugin_list(zips: &[&[u8]]) -> Vec<u8> {
    use pyreal_plugin_package::catalog::{Catalog, Entry};
    let mut c = Catalog::default();
    for z in zips {
        let probe = Entry::for_zip(z, "x.zip", "", "").expect("a plugin's zip");
        let e = Entry::for_zip(z, &format!("{}.zip", probe.id), "Notes.", "2026-10-26")
            .expect("a plugin's zip");
        c.put(e);
    }
    c.to_json().into_bytes()
}

/// Write a plugin's folder at `dir`, as an author keeps it on disk: its `plugin.toml`, for this
/// client's API version, and its script.
pub fn write_plugin_folder(dir: &Path, id: &str, name: &str, version: &str) {
    std::fs::create_dir_all(dir).expect("create the plugin's folder");
    let manifest = format!(
        "id = \"{id}\"\nname = \"{name}\"\nversion = \"{version}\"\napi = \"{}\"\nentry = \"main.lua\"\ndescription = \"Says hi\"\n",
        pyreal_plugin_api::ApiVersion::CURRENT
    );
    std::fs::write(dir.join("plugin.toml"), manifest).expect("write its manifest");
    std::fs::write(dir.join("main.lua"), "print(\"hi\")\n").expect("write its script");
}
