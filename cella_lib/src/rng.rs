//! Reproducible randomness for the whole crate.
//!
//! Two tools live here, both built on the SplitMix64 mixer so no external
//! crate is needed:
//!
//! - [`cell_rand`]: a *stateless* draw. Give it a seed, a step number, a cell
//!   index and a stream number and it returns the same number in `[0, 1)`
//!   every time, on any thread, in any order. The steppers use it for subrule
//!   `randomness`, the wildfire model uses it for ignition and spotting, and
//!   the GUI uses it for random fill. Because nothing is shared between calls,
//!   a run's result cannot depend on how the grid was split into chunks.
//! - [`Rng`]: a small *sequential* generator for code that draws many numbers
//!   in one place (sampling a prior, resampling an ensemble, choosing parents
//!   in a genetic algorithm). Same seed, same sequence.
//!
//! Streams keep independent uses from reading the same numbers: the wildfire
//! model owns streams `0..=6`, subrule `i` of a rule draws on
//! [`STREAM_RULE`]` + i`, and random fill uses [`STREAM_FILL`].

/// First stream reserved for rule subrules: subrule `i` draws on `STREAM_RULE + i`.
/// Streams below this belong to external models (the wildfire model uses 0..=4).
pub const STREAM_RULE: u64 = 16;

/// Stream used by "random fill" so a fill never reuses a rule's or a model's draws.
pub const STREAM_FILL: u64 = 64;

/// SplitMix64 finalizer (Steele et al.): a full-avalanche integer mixer.
///
/// Every output bit depends on every input bit, which is what lets
/// [`cell_rand`] combine a seed, a step and an index into one well-spread value.
#[inline]
pub fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Stateless counter-based uniform draw in `[0, 1)`.
///
/// The value depends only on the four inputs, never on call order, thread
/// count, or chunk layout — the property that makes stochastic runs
/// snapshot-testable. Distinct `stream` values give independent draws for the
/// same cell and step.
#[inline]
pub fn cell_rand(seed: u64, step: u64, idx: u64, stream: u64) -> f32 {
    let z = mix(seed
        ^ mix(step.wrapping_mul(0x9E37_79B9_7F4A_7C15))
        ^ mix(idx.wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
        ^ stream.wrapping_mul(0x1656_67B1_9E37_79F9));
    // Top 24 bits -> f32 in [0, 1) with a full mantissa.
    ((z >> 40) as f32) * (1.0 / (1u64 << 24) as f32)
}

/// SplitMix64 sequential generator — small, fast, reproducible.
///
/// Use it where one piece of code needs a *sequence* of random numbers (an
/// ensemble resampling its members, a genetic algorithm picking parents). For
/// per-cell decisions inside a step prefer [`cell_rand`], which needs no state.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    /// Start a sequence. The same `state` always yields the same sequence.
    pub fn new(state: u64) -> Self {
        Rng(state)
    }

    /// Next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }

    /// Uniform in `[0, 1)` with 53 bits of resolution.
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Standard normal (mean 0, variance 1) via Box–Muller.
    pub fn normal(&mut self) -> f64 {
        let (u, v) = (self.uniform().max(1e-12), self.uniform());
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
    }

    /// Log-uniform in `[lo, hi]`: every decade is equally likely. `lo` must be > 0.
    pub fn log_uniform(&mut self, lo: f64, hi: f64) -> f64 {
        (lo.ln() + self.uniform() * (hi.ln() - lo.ln())).exp()
    }

    /// Uniform integer in `0..n` (`0` when `n == 0`).
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            ((self.uniform() * n as f64) as usize).min(n - 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_rand_is_stateless_and_in_range() {
        let a = cell_rand(7, 3, 11, STREAM_RULE);
        let b = cell_rand(7, 3, 11, STREAM_RULE);
        assert_eq!(a, b);
        for step in 0..50u64 {
            for idx in 0..50u64 {
                let v = cell_rand(1, step, idx, 0);
                assert!((0.0..1.0).contains(&v));
            }
        }
    }

    #[test]
    fn cell_rand_streams_and_seeds_are_independent() {
        let base = cell_rand(1, 1, 1, STREAM_RULE);
        assert_ne!(base, cell_rand(1, 1, 1, STREAM_RULE + 1));
        assert_ne!(base, cell_rand(1, 1, 1, STREAM_FILL));
        assert_ne!(base, cell_rand(2, 1, 1, STREAM_RULE));
        assert_ne!(base, cell_rand(1, 2, 1, STREAM_RULE));
        assert_ne!(base, cell_rand(1, 1, 2, STREAM_RULE));
    }

    #[test]
    fn cell_rand_mean_is_about_one_half() {
        let n = 20_000u64;
        let sum: f64 = (0..n).map(|i| f64::from(cell_rand(3, 0, i, 0))).sum();
        let mean = sum / n as f64;
        assert!((mean - 0.5).abs() < 0.01, "mean {mean}");
    }

    #[test]
    fn rng_same_state_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        let mut c = Rng::new(43);
        assert_ne!(a.next_u64(), c.next_u64());
    }

    #[test]
    fn rng_uniform_normal_and_log_uniform_have_the_right_shape() {
        let mut rng = Rng::new(1);
        let n = 20_000;
        let mut sum = 0.0;
        let mut sum_sq = 0.0;
        for _ in 0..n {
            let u = rng.uniform();
            assert!((0.0..1.0).contains(&u));
            let z = rng.normal();
            sum += z;
            sum_sq += z * z;
        }
        let mean = sum / n as f64;
        let var = sum_sq / n as f64 - mean * mean;
        assert!(mean.abs() < 0.05, "normal mean {mean}");
        assert!((var - 1.0).abs() < 0.1, "normal variance {var}");
        for _ in 0..1000 {
            let v = rng.log_uniform(0.01, 100.0);
            assert!((0.01..=100.0).contains(&v));
        }
    }

    #[test]
    fn rng_below_stays_in_range_and_handles_zero() {
        let mut rng = Rng::new(9);
        assert_eq!(rng.below(0), 0);
        let mut seen = [false; 5];
        for _ in 0..500 {
            let k = rng.below(5);
            assert!(k < 5);
            seen[k] = true;
        }
        assert!(seen.iter().all(|s| *s), "every bucket reached");
    }
}
