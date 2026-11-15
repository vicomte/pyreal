//! A fast hash for maps keyed by grid coordinates, which the region build looks up per place.
//!
//! Deterministic and not DoS-resistant: the keys are the planner's own. Every walk over such a
//! map sorts its keys first, so the order the hash gives never reaches a result.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// The multiply-rotate hash (rustc's `FxHasher`), word by word.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct GridHasher(u64);

impl GridHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

impl Hasher for GridHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.add(u64::from(b));
        }
    }

    #[inline]
    fn write_i32(&mut self, i: i32) {
        self.add(u64::from(i.cast_unsigned()));
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(u64::from(i));
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

/// A `HashMap` over [`GridHasher`].
pub(crate) type GridMap<K, V> = HashMap<K, V, BuildHasherDefault<GridHasher>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbouring_columns_hash_apart() {
        let h = |k: (i32, i32)| {
            let mut s = GridHasher::default();
            std::hash::Hash::hash(&k, &mut s);
            s.finish()
        };
        let mut seen = std::collections::HashSet::new();
        for x in -20..20 {
            for y in -20..20 {
                assert!(seen.insert(h((x, y))), "({x}, {y})");
            }
        }
    }
}
