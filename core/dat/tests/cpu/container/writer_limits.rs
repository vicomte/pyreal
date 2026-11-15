//! The writer at the edges a world's server can push it to.
//! Fixture: containers built in memory.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pyreal_dat::{DatStorageMut, DatWriter};
use pyreal_primitives::DataId;

/// Storage that keeps only the bytes written as other than zero, 64 at a time, so a container
/// can be gigabytes long and hold almost nothing.
#[derive(Debug, Clone, Default)]
struct Sparse(Arc<Mutex<BTreeMap<u64, [u8; 64]>>>);

impl DatStorageMut for Sparse {
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let chunks = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let (mut at, mut i) = (offset, 0);
        while i < buf.len() {
            let (key, k) = (at / 64, (at % 64) as usize);
            let n = (64 - k).min(buf.len() - i);
            match chunks.get(&key) {
                Some(c) => buf[i..i + n].copy_from_slice(&c[k..k + n]),
                None => buf[i..i + n].fill(0),
            }
            i += n;
            at += n as u64;
        }
        Ok(())
    }

    fn write_all_at(&mut self, offset: u64, buf: &[u8]) -> io::Result<()> {
        let mut chunks = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let (mut at, mut i) = (offset, 0);
        while i < buf.len() {
            let (key, k) = (at / 64, (at % 64) as usize);
            let n = (64 - k).min(buf.len() - i);
            let part = &buf[i..i + n];
            if let Some(c) = chunks.get_mut(&key) {
                c[k..k + n].copy_from_slice(part);
            } else if part.iter().any(|&b| b != 0) {
                chunks.entry(key).or_insert([0; 64])[k..k + n].copy_from_slice(part);
            }
            i += n;
            at += n as u64;
        }
        Ok(())
    }
}

/// A container never grows past 2 GiB: a block there would have bit 31 of its offset set, which
/// a link reads as "free", so a chain through it could not be read back. Records a server sends
/// past that are refused and every record already written still reads.
#[test]
fn a_container_never_grows_into_offsets_a_link_reads_as_free() {
    const BLOCK: u32 = 0x10_0000;
    let storage = Sparse::default();
    let mut w = DatWriter::create_in(
        PathBuf::from("big.dat"),
        Box::new(storage),
        BLOCK,
        1,
        0,
        0x400 + BLOCK * 2_000,
    )
    .expect("create");
    let mut saved = Vec::new();
    let mut refused = false;
    for n in 0..2_100u32 {
        let mut payload = vec![0u8; (BLOCK - 4) as usize];
        payload[..4].copy_from_slice(&(n + 1).to_le_bytes());
        let id = DataId(0x0600_0000 + n);
        match w.save(id, &payload, 1, 1, 1) {
            Ok(_) => saved.push((id, n + 1)),
            Err(_) => {
                refused = true;
                break;
            }
        }
    }
    assert!(
        refused,
        "2,100 blocks of 1 MiB were taken without a refusal"
    );
    for (id, stamp) in saved {
        let back = w
            .read(id)
            .unwrap_or_else(|e| panic!("{id} does not read back: {e}"));
        assert_eq!(
            back[..4],
            stamp.to_le_bytes(),
            "{id} reads another record's bytes"
        );
    }
}

/// Nor is one made that large to begin with.
#[test]
fn a_container_is_not_made_past_2_gib() {
    let made = DatWriter::create_in(
        PathBuf::from("big.dat"),
        Box::new(Sparse::default()),
        0x10_0000,
        1,
        0,
        0x400 + 0x10_0000 * 2_049,
    );
    assert!(made.is_err());
}

/// xorshift64: the same writes every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u32) -> u32 {
        u32::try_from(self.next() % u64::from(n.max(1))).unwrap_or(0)
    }

    fn pick(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(n.max(1)).unwrap_or(1)).unwrap_or(0)
    }
}

fn var(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Random saves, replacements and deletions of ids and sizes at their edges, as a server may send
/// them, checked against a model: every record reads back, the tree stays balanced and ascending,
/// and no block is lost or used twice. Ignored: `ROUNDS`, `IDS` and `STEPS` set how much.
#[test]
#[ignore = "thousands of writes; seconds to minutes"]
fn random_writes_keep_the_container_whole() {
    use pyreal_dat::{DatFile, MemoryFile};
    for seed in 1..=var("ROUNDS", 20) {
        let mut r = Rng(0x9E37_79B9 ^ u64::from(seed).wrapping_mul(0x1234_5678_9ABC));
        let storage = MemoryFile::default();
        let block = [0x100u32, 0x400][r.pick(2)];
        let path = PathBuf::from("f.dat");
        let mut w = DatWriter::create_in(
            path.clone(),
            Box::new(storage.clone()),
            block,
            1,
            0,
            0x400 + block * 64,
        )
        .expect("create");
        let mut model: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
        let ids: Vec<u32> = (0..var("IDS", 400))
            .map(|_| match r.below(6) {
                0 => 1 + r.below(64),
                1 => u32::MAX - r.below(64),
                2 => 0xFFFF_0000 + r.below(8),
                3 => 0x7FFF_FFE0 + r.below(64),
                _ => r.below(u32::MAX) | 1,
            })
            .collect();
        let per = block - 4;
        for step in 0..var("STEPS", 3000) {
            let id = ids[r.pick(ids.len())];
            if r.below(5) == 0 {
                let gone = w.delete_data(DataId(id), 0).expect("deletes");
                assert_eq!(
                    gone,
                    model.remove(&id).is_some(),
                    "seed {seed} step {step} delete {id:#x}"
                );
                continue;
            }
            let len = match r.below(6) {
                0 => 0,
                1 => per,
                2 => per + 1,
                3 => per * 3 - 1,
                4 => r.below(per * 40),
                _ => r.below(64),
            };
            let payload: Vec<u8> = (0..len)
                .map(|i| (i ^ seed ^ step).to_le_bytes()[0])
                .collect();
            match w.save(DataId(id), &payload, 1, 1, 1) {
                Ok(_) => {
                    model.insert(id, payload);
                }
                // An empty record is refused: its entry would point at a free block.
                Err(_) if len == 0 => {}
                Err(e) => panic!("seed {seed} step {step} save {id:#x} of {len} bytes: {e}"),
            }
            if step % 500 == 499 {
                let f =
                    DatFile::from_storage(path.clone(), Box::new(storage.clone())).expect("opens");
                let s = f.verify_structure().expect("walks");
                assert_eq!(s.out_of_order, 0, "seed {seed}");
                assert!(
                    s.leaf_depths.len() <= 1,
                    "seed {seed}: leaves at {:?}",
                    s.leaf_depths
                );
                let got: Vec<u32> = f
                    .iter_ids()
                    .map(DataId::raw)
                    .filter(|i| *i != 0xFFFF_0001)
                    .collect();
                let want: Vec<u32> = model
                    .keys()
                    .copied()
                    .filter(|i| *i != 0xFFFF_0001)
                    .collect();
                assert_eq!(got, want, "seed {seed} step {step}");
                for (id, p) in &model {
                    assert_eq!(
                        &f.read(DataId(*id)).expect("reads"),
                        p,
                        "seed {seed} {id:#x}"
                    );
                }
                let a = w.audit().expect("audits");
                assert!(
                    a.double_allocated.is_empty(),
                    "seed {seed}: {:?}",
                    a.double_allocated
                );
                assert!(
                    a.orphaned.is_empty(),
                    "seed {seed}: {} orphaned",
                    a.orphaned.len()
                );
            }
        }
    }
}

/// A writer over one record of two blocks in memory, with a handle on the bytes.
fn two_block_record() -> (DatWriter, pyreal_dat::MemoryFile, DataId) {
    let file = pyreal_dat::MemoryFile::default();
    let mut w = DatWriter::create_in(
        PathBuf::from("small.dat"),
        Box::new(file.clone()),
        0x100,
        1,
        0,
        0x400 + 0x100 * 16,
    )
    .expect("create");
    let id = DataId(0x0600_0001);
    w.save(id, &[7u8; 0x180], 1, 1, 1).expect("save");
    (w, file, id)
}

/// A block whose link names itself would be read again and again until the entry's size is
/// reached: the chain is refused instead.
#[test]
fn a_self_linked_block_is_refused_by_the_writer() {
    let (mut w, mut file, id) = two_block_record();
    let offset = w.entry(id).expect("reads").expect("present").offset;
    file.write_all_at(u64::from(offset), &offset.to_le_bytes())
        .expect("patch");
    assert!(
        matches!(w.read(id), Err(pyreal_dat::DatError::LoopedWalk(o)) if o == offset),
        "{:?}",
        w.read(id).map(|b| b.len())
    );
}

/// An entry claiming more bytes than the file's blocks hold is refused before it is read.
#[test]
fn an_entry_larger_than_the_file_is_refused_by_the_writer() {
    let (mut w, mut file, id) = two_block_record();
    let e = w.entry(id).expect("reads").expect("present");
    let bytes = file.bytes();
    let mut key = id.raw().to_le_bytes().to_vec();
    key.extend(e.offset.to_le_bytes());
    key.extend(e.size.to_le_bytes());
    let at = bytes
        .windows(key.len())
        .position(|w| w == key)
        .expect("the directory entry");
    file.write_all_at((at + 8) as u64, &0x7FFF_0000u32.to_le_bytes())
        .expect("patch");
    assert!(
        matches!(
            w.read(id),
            Err(pyreal_dat::DatError::EntryTooLarge {
                size: 0x7FFF_0000,
                ..
            })
        ),
        "{:?}",
        w.read(id).map(|b| b.len())
    );
}
