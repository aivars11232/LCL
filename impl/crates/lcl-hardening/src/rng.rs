//! A seeded generator, so every failure is reproducible from one number.
//!
//! SplitMix64, which is three lines and has no state to get wrong. Nothing here
//! needs statistical quality: it needs to be the same sequence on every machine
//! and every run, which `rand` would not guarantee across versions and which
//! the workspace's std-only policy would not admit anyway.

/// A deterministic sequence of `u64`s from one seed.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng {
            state: seed.wrapping_add(0x9E37_79B9_7F4A_7C15),
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..bound`. `bound` must be positive.
    pub fn below(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }

    /// One element, or `None` from an empty slice.
    pub fn pick<'a, T>(&mut self, from: &'a [T]) -> Option<&'a T> {
        match from.is_empty() {
            true => None,
            false => Some(&from[self.below(from.len())]),
        }
    }

    /// True with probability `1/n`.
    pub fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }
}
