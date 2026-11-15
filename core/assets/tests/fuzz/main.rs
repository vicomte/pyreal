//! Every decoder meets damaged records without panicking or running away: real records of every
//! type in every era, cut short, bit-flipped, or with a dword set to an extreme count, decoded with
//! a five-second watchdog. Ignored: run with `--ignored`; `FUZZ_N` records a type (40) and
//! `FUZZ_M` mutations each (50).
//! Fixture: `PYREAL_TEST_DAT_DIR`, `PYREAL_TEST_PRETOD_DAT_DIR` and `PYREAL_TEST_LAUNCH_DAT_DIR`;
//! an era whose variable is unset is skipped.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use pyreal_assets::decode_any_in;
use pyreal_dat::{ContainerEra, DbType, RetailDatStore};
use pyreal_primitives::DataId;

/// xorshift64: the same mutations every run.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % u64::try_from(n.max(1)).unwrap_or(1)).unwrap_or(0)
    }
}

const EXTREMES: [u32; 6] = [
    0xFFFF_FFFF,
    0x7FFF_FFFF,
    0x8000_0000,
    0x0100_0000,
    0xFFFF,
    0x1_0000,
];

fn mutate(r: &mut Rng, b: &[u8]) -> Vec<u8> {
    let mut v = b.to_vec();
    match r.below(4) {
        0 => v.truncate(r.below(v.len())),
        1 => {
            for _ in 0..=r.below(4) {
                if !v.is_empty() {
                    let i = r.below(v.len());
                    v[i] ^= 1 << r.below(8);
                }
            }
        }
        _ if v.len() >= 4 => {
            let i = r.below(v.len() - 3) & !3;
            v[i..i + 4].copy_from_slice(&EXTREMES[r.below(EXTREMES.len())].to_le_bytes());
        }
        _ => {}
    }
    v
}

fn var(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// The decode under way and when it began, for the watchdog.
static NOW: Mutex<Option<(Instant, String)>> = Mutex::new(None);

#[test]
#[ignore = "decodes thousands of damaged records of every era; minutes"]
fn every_decoder_refuses_damaged_records_without_panicking() {
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_millis(500));
        if let Some((t, what)) = &*NOW.lock().unwrap_or_else(|e| e.into_inner()) {
            if t.elapsed() > Duration::from_secs(5) {
                eprintln!("a decode ran past 5 s: {what}");
                std::process::abort();
            }
        }
    });
    let mut stores = Vec::new();
    if let Some(d) = std::env::var_os("PYREAL_TEST_DAT_DIR") {
        stores.push(RetailDatStore::open_dir(&PathBuf::from(d)).expect("end of retail opens"));
    }
    for v in ["PYREAL_TEST_PRETOD_DAT_DIR", "PYREAL_TEST_LAUNCH_DAT_DIR"] {
        if let Some(d) = std::env::var_os(v) {
            stores.push(RetailDatStore::open_classic_dir(&PathBuf::from(d)).expect("opens"));
        }
    }
    assert!(!stores.is_empty(), "no era's dats are set");
    let (per_type, mutations) = (var("FUZZ_N", 40), var("FUZZ_M", 50));
    let quiet = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut found: BTreeMap<String, String> = BTreeMap::new();
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    for s in &stores {
        let era = s.era();
        let mut files = vec![(s.portal(), false), (s.cell(), true)];
        if era == ContainerEra::Modern {
            files.push((s.local(), false));
        }
        let mut kinds: BTreeMap<DbType, Vec<(DataId, bool)>> = BTreeMap::new();
        for (f, cell) in files {
            for id in f.iter_ids() {
                let kind = if cell {
                    pyreal_dat::classify_cell_id(id)
                } else {
                    pyreal_dat::divine_type_in(era, id)
                };
                if let Some(k) = kind {
                    kinds.entry(k).or_default().push((id, cell));
                }
            }
        }
        for (k, ids) in kinds {
            let step = (ids.len() / per_type).max(1);
            for &(id, cell) in ids.iter().step_by(step).take(per_type) {
                let read = if cell {
                    s.cell().read(id)
                } else {
                    s.read_typed(k, id)
                };
                let Ok(bytes) = read else { continue };
                for _ in 0..mutations {
                    let m = mutate(&mut r, &bytes);
                    let t = Instant::now();
                    *NOW.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some((t, format!("{era:?} {k:?} {id}")));
                    let res = std::panic::catch_unwind(|| decode_any_in(era, k, id, &m).is_ok());
                    let took = t.elapsed();
                    *NOW.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    if let Err(p) = res {
                        let why = p
                            .downcast_ref::<String>()
                            .cloned()
                            .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                            .unwrap_or_default();
                        found
                            .entry(format!("{era:?} {k:?} panics: {why}"))
                            .or_insert_with(|| format!("{id}, {} bytes", m.len()));
                    } else if took > Duration::from_millis(500) {
                        found
                            .entry(format!("{era:?} {k:?} is slow"))
                            .or_insert_with(|| format!("{id}: {took:?}"));
                    }
                }
            }
        }
    }
    std::panic::set_hook(quiet);
    assert!(found.is_empty(), "{found:#?}");
}
