//! Counter-based random numbers (DR-003).
//!
//! Philox4x32-10 (Salmon, Moraes, Dror & Shaw, SC11 2011). A draw is a pure function of
//! `(seed, stream, tick, subject, index)`, so results cannot depend on thread count,
//! scheduling, or on which other rules draw random numbers.
#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

const M0: u32 = 0xD251_1F53;
const M1: u32 = 0xCD9E_8D57;
const W0: u32 = 0x9E37_79B9;
const W1: u32 = 0xBB67_AE85;

/// Stream reserved for initial placement; no rule may use it.
pub const STREAM_INIT: u16 = 0;

/// The Philox4x32-10 bijection: 128-bit counter, 64-bit key, 128-bit output.
#[inline]
pub fn philox4x32_10(mut ctr: [u32; 4], mut key: [u32; 2]) -> [u32; 4] {
    for round in 0..10 {
        if round > 0 {
            key[0] = key[0].wrapping_add(W0);
            key[1] = key[1].wrapping_add(W1);
        }
        let p0 = u64::from(M0) * u64::from(ctr[0]);
        let p1 = u64::from(M1) * u64::from(ctr[2]);
        ctr = [
            (p1 >> 32) as u32 ^ ctr[1] ^ key[0],
            p1 as u32,
            (p0 >> 32) as u32 ^ ctr[3] ^ key[1],
            p0 as u32,
        ];
    }
    ctr
}

/// The random source handed to one rule: a seed plus that rule's stream id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleRng {
    pub seed: u64,
    pub stream: u16,
}

impl RuleRng {
    /// Four 32-bit words for `(tick, subject, index)`.
    ///
    /// `subject` is an entity id or a site index (a rule uses one kind per stream);
    /// `index` distinguishes several independent draws for the same subject and tick.
    #[inline]
    pub fn draw(&self, tick: u32, subject: u64, index: u16) -> [u32; 4] {
        philox4x32_10(
            [tick, subject as u32, (subject >> 32) as u32, (u32::from(self.stream) << 16) | u32::from(index)],
            [self.seed as u32, (self.seed >> 32) as u32],
        )
    }
}

/// Maps a uniform 32-bit word to `0..n` without floats: `floor(x * n / 2^32)`.
///
/// # Panics
/// Panics if `n == 0`.
#[inline]
pub fn below(x: u32, n: u32) -> u32 {
    assert!(n > 0, "below: n must be positive");
    ((u64::from(x) * u64::from(n)) >> 32) as u32
}
