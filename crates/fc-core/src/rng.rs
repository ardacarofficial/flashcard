//! Randomness as data. The core never touches an OS RNG; callers inject an [`Rng`].

/// A source of random 64-bit values. The app layer provides an OS-seeded implementation;
/// tests use [`SplitMix64`] with a fixed seed.
pub trait Rng: Send {
    /// Next uniformly distributed `u64`.
    fn next_u64(&mut self) -> u64;

    /// Fills `dest` with random bytes.
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for chunk in dest.chunks_mut(8) {
            let bytes = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
    }

    /// Uniform value in `0..upper` (`upper == 0` returns 0). Slight modulo bias is acceptable
    /// for shuffling/ids; do not use for cryptography.
    fn below(&mut self, upper: u64) -> u64 {
        if upper == 0 {
            0
        } else {
            self.next_u64() % upper
        }
    }
}

/// Small, fast, deterministic PRNG (SplitMix64). Good for tests, simulations and as a
/// seeded stream; not cryptographically secure.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Creates a generator from `seed`.
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }
}

impl Rng for SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix_is_deterministic() {
        let mut a = SplitMix64::new(7);
        let mut b = SplitMix64::new(7);
        for _ in 0..10 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_ne!(SplitMix64::new(1).next_u64(), SplitMix64::new(2).next_u64());
    }

    #[test]
    fn fill_bytes_handles_partial_chunks() {
        let mut r = SplitMix64::new(1);
        let mut buf = [0u8; 13];
        r.fill_bytes(&mut buf);
        assert!(buf.iter().any(|b| *b != 0));
        assert_eq!(r.below(0), 0);
        assert!(r.below(10) < 10);
    }
}
