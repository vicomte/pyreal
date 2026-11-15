//! Every record of the three dat sets Pyreal reads, the October 1999 launch files, February 2005
//! and the end of retail, decodes in the layouts of its own container with the cursor on the
//! record's end, and every id has a type.
//! Fixture: `PYREAL_TEST_LAUNCH_DAT_DIR`, `PYREAL_TEST_PRETOD_DAT_DIR` and `PYREAL_TEST_DAT_DIR`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pyreal_assets::ui::PropertyTypes;
use pyreal_assets::{exhaustive_decode_file, Decode, MasterProperty, VerifyReport};
use pyreal_dat::testing::{classic_dat_dir, dat_dir, launch_dat_dir, CLASSIC_DAT_DIR_VAR};
use pyreal_dat::DatFile;
use pyreal_primitives::DataId;

/// Each set's files and their records.
const SETS: &[(&str, &[(&str, usize)])] = &[
    (
        "1999-10-09",
        &[("cell.dat", 131_560), ("portal.dat", 17_794)],
    ),
    ("2005-02", &[("cell.dat", 524_954), ("portal.dat", 51_001)]),
    (
        "end of retail",
        &[
            ("client_cell_1.dat", 805_348),
            ("client_highres.dat", 2_295),
            ("client_local_English.dat", 118),
            ("client_portal.dat", 79_694),
        ],
    ),
];

fn property_types(f: &DatFile) -> PropertyTypes {
    let mp = DataId(0x3900_0001);
    match f.read(mp) {
        Ok(b) => MasterProperty::decode_payload(mp, &b)
            .expect("the property map decodes")
            .property_types(),
        Err(_) => PropertyTypes::new(),
    }
}

fn census(path: &Path, name: &str, portal: Option<&Path>) -> VerifyReport {
    let f = DatFile::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    // A language file's interface records are typed by the property map of its set's portal.
    let types = match portal {
        Some(p) => property_types(&DatFile::open(p).expect("the portal opens")),
        None => property_types(&f),
    };
    exhaustive_decode_file(&f, f.era(), name.contains("cell"), &types)
}

#[test]
fn every_record_of_the_three_sets_decodes() {
    let dirs: [PathBuf; 3] = [
        launch_dat_dir().expect("PYREAL_TEST_LAUNCH_DAT_DIR names the October 1999 dats"),
        classic_dat_dir().unwrap_or_else(|| panic!("{CLASSIC_DAT_DIR_VAR} names the 2005 dats")),
        dat_dir(),
    ];
    let files: Vec<(&str, &str, usize, PathBuf, PathBuf)> = SETS
        .iter()
        .zip(&dirs)
        .flat_map(|((set, files), dir)| {
            let portal = dir.join(files.last().map_or("", |f| f.0));
            files
                .iter()
                .map(move |(n, records)| (*set, *n, *records, dir.join(n), portal.clone()))
        })
        .collect();
    let reports: Vec<VerifyReport> = std::thread::scope(|s| {
        let handles: Vec<_> = files
            .iter()
            .map(|(_, n, _, p, portal)| {
                s.spawn(move || census(p, n, n.contains("local").then_some(portal.as_path())))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("a census thread panicked"))
            .collect()
    });
    let mut problems = BTreeMap::new();
    for ((set, name, records, _, _), r) in files.iter().zip(&reports) {
        if r.entries != *records
            || !r.failures.is_empty()
            || !r.no_decoder.is_empty()
            || !r.untyped.is_empty()
            || !r.iteration_failures.is_empty()
            || r.decoded + r.iteration_lists != r.entries
        {
            problems.insert(
                format!("{set}/{name}"),
                format!(
                    "{} of {} records decoded; failures {:?}; no decoder {:?}; untyped {:?}; \
                     iteration lists {:?}",
                    r.decoded,
                    r.entries,
                    r.failures.iter().take(5).collect::<Vec<_>>(),
                    r.no_decoder,
                    r.untyped.iter().take(5).collect::<Vec<_>>(),
                    r.iteration_failures,
                ),
            );
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}
