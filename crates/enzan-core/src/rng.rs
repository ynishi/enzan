//! Seeded pseudo-random numbers whose sequence is part of the contract.
//!
//! The generator is SplitMix64. It was chosen because its whole state is one
//! `u64` and it uses only integer arithmetic, so native and WebAssembly cannot
//! differ. xoshiro256** needs four words of state, which is more than the
//! current need. PCG32 outputs 32 bits, so a `u64` or an `f64` takes two draws.
//! The `rand` family does not promise that a distribution gives the same values
//! across major versions, which would make the sequence depend on an outside
//! update.
//!
//! Every step below decides the output, and all multiplications and additions
//! wrap:
//!
//! ```text
//! GAMMA = 0x9E3779B97F4A7C15
//! SALT  = 0xD1B54A32D192ED03
//! mix64(z): z = (z ^ z>>30) * 0xBF58476D1CE4E5B9
//!           z = (z ^ z>>27) * 0x94D049BB133111EB
//!           z ^ z>>31
//! new(seed):        state = seed
//! next_u64():       state += GAMMA; mix64(state)
//! stream(seed, n):  state = mix64(seed ^ mix64(n * GAMMA + SALT))
//! next_f64():       (next_u64() >> 11) as f64 * 2^-53
//! below(n):         Lemire 2019: x = next_u64(); m = x as u128 * n; l = m as u64;
//!                   if l < n { t = (-n) % n; while l < t { redraw x } }
//!                   result m >> 64
//! draw(table):      r = below(total);
//!                   first i with cumulative[i] > r
//! ```
//!
//! Changing any of them changes the output sequence. That is a breaking change:
//! raise the version and record it in the CHANGELOG.

use core::num::{NonZeroU32, NonZeroU64};

const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
const SALT: u64 = 0xD1B5_4A32_D192_ED03;

fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The value a [`Rng`] starts from.
///
/// A distinct type so a seed is not confused with any other `u64` argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Seed(u64);

impl Seed {
    /// The raw value, for storing or logging a seed so a run can be replayed.
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }
}

impl From<u64> for Seed {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// A seeded random number generator.
///
/// The same seed gives the same sequence natively and in WebAssembly. It has
/// neither `Copy` nor `Default`: a copy would silently replay a sequence, and a
/// default would create a generator without a seed. Nothing here reads the OS
/// or a clock; the caller supplies the seed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Starts the sequence of `seed`.
    #[must_use]
    pub fn new(seed: Seed) -> Self {
        Self { state: seed.0 }
    }

    /// Starts the `n`th independent sequence of `seed`.
    ///
    /// It is a different sequence from [`Rng::new`] with the same seed. The
    /// derivation is non-linear in `n`: a linear one (`seed + n * GAMMA`) would
    /// give the same sequence shifted along, so it is not used. For one seed,
    /// different `n` never give the same state (the mapping is a bijection).
    /// Different seeds can collide, but only through a XOR difference of 64-bit
    /// pseudo-random values, which is negligible in practice.
    #[must_use]
    pub fn stream(seed: Seed, n: u64) -> Self {
        let h = mix64(n.wrapping_mul(GAMMA).wrapping_add(SALT));
        Self {
            state: mix64(seed.0 ^ h),
        }
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GAMMA);
        mix64(self.state)
    }

    /// A value in `[0, 1)`, exact in `f64` because only 53 bits are used.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// A value in `[0, n)` without modulo bias (Lemire 2019).
    ///
    /// How many `next_u64` calls it consumes depends on `n` and on the
    /// sequence, so it is the same for the same seed.
    pub fn below(&mut self, n: NonZeroU64) -> u64 {
        let n = n.get();
        let mut m = u128::from(self.next_u64()) * u128::from(n);
        let mut l = m as u64;
        if l < n {
            let t = n.wrapping_neg() % n;
            while l < t {
                m = u128::from(self.next_u64()) * u128::from(n);
                l = m as u64;
            }
        }
        (m >> 64) as u64
    }
}

/// Why a [`WeightedTable`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WeightedTableError {
    /// There is nothing to draw from.
    #[error("a weighted table needs at least one entry")]
    Empty,
    /// The weights sum to more than `u64::MAX`.
    #[error("the weights sum to more than u64::MAX")]
    TotalOverflow,
}

/// Cumulative sums of `weights` (entry `i` holds the sum of `0..=i`) and their
/// total.
pub(crate) fn accumulate(
    weights: impl IntoIterator<Item = NonZeroU64>,
) -> Result<(Vec<u64>, NonZeroU64), WeightedTableError> {
    let mut sum = 0u64;
    let mut cumulative = Vec::new();
    for w in weights {
        sum = sum
            .checked_add(w.get())
            .ok_or(WeightedTableError::TotalOverflow)?;
        cumulative.push(sum);
    }
    let total = NonZeroU64::new(sum).ok_or(WeightedTableError::Empty)?;
    Ok((cumulative, total))
}

/// The index of the entry that `r` falls in: the first with `cumulative > r`.
///
/// `r` must be below the total, which keeps the index inside `cumulative`.
fn pick(cumulative: &[u64], r: u64) -> usize {
    cumulative.partition_point(|&c| c <= r)
}

/// Entries drawn in proportion to integer weights.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeightedTable<T> {
    entries: Vec<(T, NonZeroU32)>,
    cumulative: Vec<u64>,
    total: NonZeroU64,
}

impl<T> WeightedTable<T> {
    /// Builds a table from `(value, weight)` pairs.
    ///
    /// A weight is an integer above 0, which the type guarantees: integers
    /// leave no rounding to settle and no weight that is silently dropped. The
    /// table cannot be empty, and the weights must sum to at most `u64::MAX`.
    ///
    /// # Errors
    ///
    /// [`WeightedTableError::Empty`] for no entries,
    /// [`WeightedTableError::TotalOverflow`] when the sum does not fit a `u64`.
    pub fn new(entries: Vec<(T, NonZeroU32)>) -> Result<Self, WeightedTableError> {
        let (cumulative, total) = accumulate(entries.iter().map(|(_, w)| NonZeroU64::from(*w)))?;
        Ok(Self {
            entries,
            cumulative,
            total,
        })
    }

    /// Draws one entry.
    ///
    /// Weights are accumulated in the order the entries were given, so that
    /// order is part of the result for a given sequence.
    pub fn draw(&self, rng: &mut Rng) -> &T {
        let r = rng.below(self.total);
        &self.entries[pick(&self.cumulative, r)].0
    }

    /// The sum of all weights.
    #[must_use]
    pub fn total(&self) -> NonZeroU64 {
        self.total
    }

    /// The entries in the order they were given.
    #[must_use]
    pub fn entries(&self) -> &[(T, NonZeroU32)] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nz(n: u64) -> NonZeroU64 {
        NonZeroU64::new(n).unwrap()
    }

    fn table_127() -> WeightedTable<usize> {
        let w = [1u32, 2, 7];
        WeightedTable::new(
            w.iter()
                .enumerate()
                .map(|(i, &w)| (i, NonZeroU32::new(w).unwrap()))
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn next_u64_reference() {
        let mut r = Rng::new(Seed::from(1234567));
        let got: Vec<u64> = (0..5).map(|_| r.next_u64()).collect();
        assert_eq!(
            got,
            [
                6457827717110365317,
                3203168211198807973,
                9817491932198370423,
                4593380528125082431,
                16408922859458223821
            ]
        );
    }

    #[test]
    fn next_f64_reference() {
        let mut r = Rng::new(Seed::from(1234567));
        assert_eq!(r.next_f64(), 0.3500795420214081);
        assert_eq!(r.next_f64(), 0.17364409667091263);
        assert_eq!(r.next_f64(), 0.5322073040624192);
    }

    #[test]
    fn below_reference() {
        let mut r = Rng::new(Seed::from(1234567));
        let got: Vec<u64> = (0..8).map(|_| r.below(nz(10))).collect();
        assert_eq!(got, [3, 1, 5, 2, 8, 4, 5, 2]);
    }

    #[test]
    fn stream_reference() {
        let mut r = Rng::stream(Seed::from(1234567), 0);
        let got: Vec<u64> = (0..3).map(|_| r.next_u64()).collect();
        assert_eq!(
            got,
            [
                9754944101224836892,
                14910563179292209329,
                12614523108169859473
            ]
        );
        let mut r = Rng::stream(Seed::from(1234567), 1);
        let got: Vec<u64> = (0..3).map(|_| r.next_u64()).collect();
        assert_eq!(
            got,
            [
                16287144396046500677,
                12748082110584137889,
                3347036314163302689
            ]
        );
    }

    #[test]
    fn draw_reference() {
        let t = table_127();
        let mut r = Rng::new(Seed::from(1234567));
        let got: Vec<usize> = (0..8).map(|_| *t.draw(&mut r)).collect();
        assert_eq!(got, [2, 1, 2, 1, 2, 2, 2, 1]);
    }

    #[test]
    fn below_rejection_path_reference() {
        let mut r = Rng::new(Seed::from(42));
        let n = nz((1u64 << 63) + 1);
        let got: Vec<u64> = (0..3).map(|_| r.below(n)).collect();
        assert_eq!(
            got,
            [
                1474913046063446145,
                8007990562831494531,
                2014432356388812462
            ]
        );
        assert_eq!(r.next_u64(), 14769051326987775908);
    }

    #[test]
    fn below_bounds() {
        let mut r = Rng::new(Seed::from(7));
        for _ in 0..50 {
            assert_eq!(r.below(nz(1)), 0);
        }
        for _ in 0..1000 {
            assert!(r.below(nz(u64::MAX)) < u64::MAX);
        }
    }

    #[test]
    fn streams_are_not_shifted_copies() {
        let seed = Seed::from(1234567);
        let mut a = Rng::stream(seed, 0);
        let mut b = Rng::stream(seed, 1);
        let a: Vec<u64> = (0..1000).map(|_| a.next_u64()).collect();
        let b: Vec<u64> = (0..1000).map(|_| b.next_u64()).collect();
        assert!(!a.windows(16).any(|w| w == &b[..16]));
        assert!(!b.windows(16).any(|w| w == &a[..16]));
    }

    #[test]
    fn next_f64_in_unit_interval() {
        let mut r = Rng::new(Seed::from(99));
        for _ in 0..5000 {
            let x = r.next_f64();
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn pick_boundaries() {
        let c = [1, 3, 10];
        assert_eq!(pick(&c, 0), 0);
        assert_eq!(pick(&c, 1), 1);
        assert_eq!(pick(&c, 2), 1);
        assert_eq!(pick(&c, 3), 2);
        assert_eq!(pick(&c, 9), 2);
    }

    #[test]
    fn new_rejects_empty() {
        assert_eq!(
            WeightedTable::<u8>::new(Vec::new()).unwrap_err(),
            WeightedTableError::Empty
        );
    }

    #[test]
    fn accumulate_rejects_overflow() {
        assert_eq!(
            accumulate([nz(u64::MAX), nz(1)]).unwrap_err(),
            WeightedTableError::TotalOverflow
        );
        let (c, total) = accumulate([nz(u64::MAX)]).unwrap();
        assert_eq!(c, [u64::MAX]);
        assert_eq!(total.get(), u64::MAX);
    }

    #[test]
    fn distribution_matches_weights() {
        let t = table_127();
        let mut r = Rng::new(Seed::from(2024));
        let mut counts = [0u32; 3];
        let n = 100_000;
        for _ in 0..n {
            counts[*t.draw(&mut r)] += 1;
        }
        for (count, want) in counts.iter().zip([0.1, 0.2, 0.7]) {
            let got = f64::from(*count) / f64::from(n);
            assert!((got - want).abs() < 0.01, "{got} vs {want}");
        }
    }
}
