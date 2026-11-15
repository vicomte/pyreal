//! The census over every era a player can pick: end of retail, February 2005 and the October 1999
//! launch files, each read in its own record layouts. Ignored: run with `--ignored era_sweep`.
//! Fixture: `PYREAL_TEST_DAT_DIR`, `PYREAL_TEST_PRETOD_DAT_DIR` and `PYREAL_TEST_LAUNCH_DAT_DIR`
//! (the 1999 `portal.dat` and `cell.dat`); an era whose variable is unset is skipped by name.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pyreal_assets::exhaustive_decode;
use pyreal_dat::{DbType, RetailDatStore};

fn dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn sweep(name: &str, s: &RetailDatStore) -> Vec<String> {
    let r = exhaustive_decode(s).expect("the census runs");
    let mut by_type: BTreeMap<DbType, (usize, String)> = BTreeMap::new();
    for (id, kind, e) in &r.failures {
        by_type.entry(*kind).or_insert((0, format!("{id}: {e}"))).0 += 1;
    }
    eprintln!(
        "{name}: {} entries, {} decoded, highres {}",
        r.entries,
        r.decoded,
        s.highres().is_some()
    );
    let mut bad = Vec::new();
    for (kind, (n, first)) in by_type {
        bad.push(format!("{name}: {n} {kind:?} failures, first {first}"));
    }
    for (kind, n) in &r.no_decoder {
        bad.push(format!("{name}: no decoder for {kind:?} ({n})"));
    }
    if !r.untyped.is_empty() {
        bad.push(format!(
            "{name}: {} untyped, first {}",
            r.untyped.len(),
            r.untyped[0]
        ));
    }
    for (id, e) in &r.iteration_failures {
        bad.push(format!("{name}: iteration list {id}: {e}"));
    }
    bad
}

#[test]
#[ignore = "reads all three eras' dats; slow"]
fn era_sweep_every_record_of_every_era_decodes_whole() {
    let mut bad = Vec::new();
    let mut ran = 0;
    if let Some(d) = dir("PYREAL_TEST_DAT_DIR") {
        let s = RetailDatStore::open_dir(&d).expect("end of retail opens");
        let _ = s.grant_highres();
        bad.extend(sweep("end of retail", &s));
        ran += 1;
    }
    for (name, var) in [
        ("february 2005", "PYREAL_TEST_PRETOD_DAT_DIR"),
        ("october 1999", "PYREAL_TEST_LAUNCH_DAT_DIR"),
    ] {
        if let Some(d) = dir(var) {
            let s = RetailDatStore::open_classic_dir(&d).unwrap_or_else(|e| panic!("{name}: {e}"));
            bad.extend(sweep(name, &s));
            ran += 1;
        } else {
            eprintln!("{name}: skipped, {var} unset");
        }
    }
    assert!(ran > 0, "no era's dats are set");
    assert!(bad.is_empty(), "{bad:#?}");
}
