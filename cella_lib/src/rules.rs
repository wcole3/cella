//! Rule definitions for 1D and 2D cellular automata.
//!
//! Important: Cells default to the Inactive type if no subrule matches.
//! If you want Inactive cells to become another type (e.g., X/Alive),
//! you must add an explicit subrule whose current_type is `Inactive`.
//! For example, for 1D Rule 30 you typically need two subrules:
//! - current=X, criteria=X, wolfram_code=30 => output=X (propagate active cells)
//! - current=Inactive, criteria=X, wolfram_code=30 => output=X (allow births from Inactive)
//! Without the second subrule, a single X seed cannot spread because
//! Inactive cells would never transition to X.

use std::collections::HashSet;
use memoize::memoize;
use crate::types::CellType;
use serde::{Deserialize, Deserializer, Serialize};

/// Lightweight cell-type counter for hotpath use.
/// Avoids HashMap allocation; typically < 20 unique types.
#[derive(Clone, Debug)]
pub struct TypeCounter {
    entries: Vec<(CellType, u64)>,
}

impl TypeCounter {
    pub fn new() -> Self {
        Self { entries: Vec::with_capacity(16) }
    }

    pub fn add(&mut self, t: CellType) {
        for e in &mut self.entries {
            if e.0 == t {
                e.1 += 1;
                return;
            }
        }
        self.entries.push((t, 1));
    }

    pub fn merge(&mut self, other: &Self) {
        // `continue 'outer` (not `return`): every entry of `other` must be folded in.
        'outer: for (t, c) in &other.entries {
            for e in &mut self.entries {
                if e.0 == *t {
                    e.1 += *c;
                    continue 'outer;
                }
            }
            self.entries.push((*t, *c));
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&CellType, &u64)> {
        self.entries.iter().map(|(t, c)| (t, c))
    }
}

impl Default for TypeCounter {
    fn default() -> Self { Self::new() }
}

/// Rebuild `counts_current` / `peak_counts` from one step's [`TypeCounter`].
///
/// `new_counts` deliberately omits `dominant_type` (the stepping loop skips it),
/// so that type's population is back-filled by subtracting every counted type
/// from `total_cells`. Also re-picks the dominant type for the next step: if some
/// counted type now outnumbers the back-filled remainder, it takes over.
///
/// Shared by `Grid1D` and `Grid2D` so the two can't drift apart.
pub(crate) fn apply_counts(
    counts_current: &mut std::collections::HashMap<lasso2::Spur, u64>,
    peak_counts: &mut std::collections::HashMap<lasso2::Spur, u64>,
    dominant_type: &mut CellType,
    total_cells: u64,
    new_counts: &TypeCounter,
) {
    counts_current.clear();
    let mut remainder = total_cells;
    let mut challenger: (CellType, u64) = (*dominant_type, 0);
    for (k, v) in new_counts.iter() {
        counts_current.insert(k.0, *v);
        remainder = remainder.saturating_sub(*v);
        peak_counts.entry(k.0).and_modify(|peak| { if *v > *peak { *peak = *v; } }).or_insert(*v);
        if *v > challenger.1 { challenger = (*k, *v); }
    }
    counts_current.insert(dominant_type.0, remainder);
    peak_counts.entry(dominant_type.0)
        .and_modify(|peak| { if remainder > *peak { *peak = remainder; } })
        .or_insert(remainder);
    if remainder < challenger.1 {
        *dominant_type = challenger.0;
    }
}

mod serde_u128 {
    use serde::de::{self, Visitor};
    use serde::{Deserializer, Serializer};
    use std::fmt;

    pub fn serialize<S: Serializer>(val: &u128, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&val.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u128, D::Error> {
        struct U128Visitor;
        impl<'de> Visitor<'de> for U128Visitor {
            type Value = u128;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a u128 as a string or integer")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<u128, E> {
                v.parse().map_err(de::Error::custom)
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<u128, E> {
                Ok(v as u128)
            }
            fn visit_u128<E: de::Error>(self, v: u128) -> Result<u128, E> {
                Ok(v)
            }
        }
        d.deserialize_any(U128Visitor)
    }
}

/// Neighborhood types for 2D rules.
///
/// - `Moore`: all cells in the (2n+1)x(2n+1) square.
/// - `VonNeumann`: cells with Manhattan distance <= n.
/// - `Langton`: diagonal cells where |dx|==|dy|<=n.
/// - `StraightLine`: cells in straight cardinal lines (up/down/left/right) up to range n.
/// - `Knight`: cells reachable from the origin in at most `range` chess-knight hops
///   (each hop is an L-shaped move: ±1/±2 or ±2/±1). `range=1` gives exactly the
///   8 classic knight squares; `range=N` unions all cells reachable in 1..=N hops.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Neighborhood2D {
    Moore,
    VonNeumann,
    Langton,
    StraightLine,
    Knight,
}

#[memoize(SharedCache)]
/// Check if a given (dx, dy) offset is in the neighborhood for a given range.
/// This is purely used for tests, do NOT use in hotpath
pub fn neighborhood_contains(dx: i32, dy: i32, range: i32, neighborhood: Neighborhood2D) -> bool {
    neighborhood_offsets(neighborhood, range).contains(&(dx, dy))
}


#[memoize(SharedCache)]
/// For Neighborhoods, we want to save the offsets so that we can
/// loop over them during stepping. Returns a sorted Vec for deterministic
/// iteration order and better cache locality.
pub fn neighborhood_offsets(neighborhood: Neighborhood2D, n: i32) -> Vec<(i32, i32)> {
    let mut set = HashSet::new();
    match neighborhood {
        Neighborhood2D::Moore => {
            for dx in -n..=n {
                for dy in -n..=n {
                    if dx == 0 && dy == 0 { continue; }
                    set.insert((dx, dy));
                }
            }
        },
        Neighborhood2D::VonNeumann => {
            for dx in -n..=n {
                for dy in -n..=n {
                    if dx == 0 && dy == 0 { continue; }
                    if dx.abs() + dy.abs() <= n { set.insert((dx, dy)); }
                }
            }
        },
        Neighborhood2D::Langton => {
            for dx in -n..=n {
                for dy in -n..=n {
                    if dx == 0 && dy == 0 { continue; }
                    if dx.abs() == dy.abs() && dx.abs() <= n { set.insert((dx, dy)); }
                }
            }
        },
        Neighborhood2D::StraightLine => {
            for i in -n..=n {
                if i != 0 {
                    set.insert((i, 0));
                    set.insert((0, i));
                }
            }
        },
        Neighborhood2D::Knight => {
            for dx in -2*n..=2*n {
                for dy in -2*n..=2*n {
                    if knight_reachable(dx, dy, n as u8) { set.insert((dx, dy)); }
                }
            }
        }
    }
    let mut offsets: Vec<_> = set.into_iter().collect();
    offsets.sort();
    offsets
}

/// Returns `true` if `(dx, dy)` is reachable from `(0, 0)` in at most `max_moves` knight hops.
/// Each hop is an L-shaped chess-knight move: (±1, ±2) or (±2, ±1).
fn knight_reachable(dx: i32, dy: i32, max_moves: u8) -> bool {
    use std::collections::VecDeque;
    if dx == 0 && dy == 0 { return false; }
    let mut visited = HashSet::new();
    let mut queue: VecDeque<(i32, i32, u8)> = VecDeque::new();
    queue.push_back((0, 0, 0));
    visited.insert((0i32, 0i32));
    const MOVES: [(i32, i32); 8] = [
        (1, 2), (1, -2), (-1, 2), (-1, -2),
        (2, 1), (2, -1), (-2, 1), (-2, -1),
    ];
    while let Some((x, y, depth)) = queue.pop_front() {
        if depth >= max_moves { continue; }
        for (mx, my) in MOVES {
            let nx = x + mx;
            let ny = y + my;
            if nx == dx && ny == dy { return true; }
            if !visited.contains(&(nx, ny)) {
                visited.insert((nx, ny));
                queue.push_back((nx, ny, depth + 1));
            }
        }
    }
    false
}

/// Validation errors for rules.
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum RuleError {
    #[error("invalid neighborhood size n for 1D rule: {0}")]
    InvalidN1D(u8),
    #[error("too many neighborhood patterns for n={0}; supported up to n<=3")]
    TooManyPatterns(u8),
    #[error("wolfram code {0} exceeds maximum for n={1}")]
    InvalidWolframCode(u128, u8),
    #[error("randomness must be in [0.0, 1.0]")]
    InvalidRandomness,
    #[error("range must be >= 1")]
    InvalidRange2D,
}

/// One subrule for a 1D automaton using Wolfram-style code.
///
/// The `wolfram_code` bitmask enumerates all neighborhood windows of
/// size 2n+1, interpreting a bit=1 as a match when the window equals
/// the pattern of `criteria_type` vs "other".
///
/// Example
/// ```rust
/// use cella_lib::{CellType, Rule1DSubrule};
/// // Match only the central cell being X with neighbors not X (pattern 010 => idx=2)
/// let sub = Rule1DSubrule { current_type: CellType::from("X"), criteria_type: CellType::from("X"), wolfram_code: 1u128<<2, n: 1, randomness: None, output_type: CellType::from("Y") };
/// assert!(sub.validate().is_ok());
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule1DSubrule {
    #[serde(with = "serde_u128")]
    pub wolfram_code: u128,
    /// Optional randomness in (0-1); pass only if random >= value.
    pub randomness: Option<f64>,
    /// Neighborhood radius (>=1): window size is 2n+1.
    pub n: u8,
    pub output_type: CellType,
    pub current_type: CellType,
    pub criteria_type: CellType,
}

impl Rule1DSubrule {
    /// Validate subrule parameters.
    ///
    /// Ensures `n>=1`, `randomness` in (0-1), and `wolfram_code` within range
    /// for the window size (when computable within u128 limits).
    pub fn validate(&self) -> Result<(), RuleError> {
        if self.n < 1 { return Err(RuleError::InvalidN1D(self.n)); }
        if let Some(r) = self.randomness { if !(0.0..=1.0).contains(&r) { return Err(RuleError::InvalidRandomness) } }
        let b: u32 = 2u32 * self.n as u32 + 1; // window bits
        // n=1 -> b=3, patterns=8;   n=2 -> b=5, patterns=32;
        // n=3 -> b=7, patterns=128 (all u128 values valid);
        // n>=4 -> patterns > 128, exceeds u128 capacity.
        if self.n > 3 { return Err(RuleError::TooManyPatterns(self.n)); }
        if b < 7 {
            let patterns: u32 = 1u32 << b;
            let max: u128 = 1u128 << patterns;
            if self.wolfram_code >= max {
                return Err(RuleError::InvalidWolframCode(self.wolfram_code, self.n));
            }
        }
        // n=3: patterns=128, all u128 values are valid (0..2^128-1)
        Ok(())
    }

    /// Compute the wolfram-code bit index for the given neighborhood window.
    /// Returns `true` if the corresponding bit in `wolfram_code` is set.
    ///
    /// Caller already verified `current_type == self.current_type`.
    #[inline]
    pub fn applies(&self, neighborhood: &[CellType]) -> bool {
        let crit = &self.criteria_type;
        let mut idx: u128 = 0;
        for t in neighborhood {
            idx = (idx << 1) | ((*t == *crit) as u128);
        }
        (self.wolfram_code >> idx) & 1u128 == 1u128
    }
}

/// A 1D rule consisting of multiple subrules evaluated in order.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule1D { pub subrules: Vec<Rule1DSubrule> }

impl Rule1D {
    /// Validate all subrules.
    pub fn validate(&self) -> Result<(), RuleError> { for s in &self.subrules { s.validate()?; } Ok(()) }
    /// Return the maximum neighborhood radius among subrules (or 1 if empty).
    pub fn n_max(&self) -> u8 { self.subrules.iter().map(|s| s.n).max().unwrap_or(1) }
    /// Whether any subrule draws from the RNG. Lets the stepper skip RNG setup entirely.
    pub(crate) fn needs_rng(&self) -> bool { self.subrules.iter().any(|s| s.randomness.is_some()) }
}

/// One subrule for a 2D automaton using threshold counts in a neighborhood.
///
/// Example
/// ```rust
/// use cella_lib::{CellType, Rule2DSubrule, Neighborhood2D, CountOp};
/// let a = CellType::from("A");
/// let b = CellType::from("B");
/// let s = Rule2DSubrule::new(a.clone(), b.clone(), 1,
///  CountOp::Gt, 1, Neighborhood2D::Moore,
///  b.clone(), None, None );
/// assert!(s.validate().is_ok());
/// ```
/// Comparison operator for neighbor counts.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum CountOp {
    /// At most (inclusive): `neighbor_count <= target_count`.
    #[serde(rename = "lt")] Lt,
    /// At least (inclusive): `neighbor_count >= target_count`.
    #[serde(rename = "gt")] Gt,
    /// Equal to: `neighbor_count == target_count`.
    #[serde(rename = "eq")] Eq,
}

/// One subrule for a 2D automaton using neighbor-count comparisons.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Rule2DSubrule {
    /// Sorted offsets for deterministic iteration and better cache locality.
    #[serde(skip)]
    pub offsets: Vec<(i32, i32)>,
    /// Precomputed: `op == Gt && limit.is_none()`, i.e. neighbor counting may stop
    /// as soon as `count` is reached. Hoisted out of the per-neighbor loop.
    #[serde(skip)]
    pub(crate) early_exit: bool,
    /// Precomputed: largest `max(|dx|, |dy|)` over `offsets` — the Chebyshev radius
    /// of this subrule's neighborhood. Cells at least this far from every border
    /// can read all their neighbors without bounds checks.
    #[serde(skip)]
    pub(crate) pad: usize,
    /// Optional randomness in (0-1); pass only if random >= value.
    pub randomness: Option<f64>,
    /// Comparison operator: lt/gt/eq. When accompanied by `limit`, creates a
    /// between-range inclusive clause (see `validate`).
    pub op: CountOp,
    /// Optional bound for "between":
    /// - If op=Gt, `limit` is an inclusive upper bound (count..=limit).
    /// - If op=Lt, `limit` is an inclusive lower bound (limit..=count).
    /// - If op=Eq, `limit` must be None.
    pub limit: Option<u32>,
    /// Comparison baseline value.
    pub count: u32,
    /// Range n >= 1 defines (2n+1)^2 window.
    pub range: u8,
    pub neighborhood: Neighborhood2D,
    pub output_type: CellType,
    pub current_type: CellType,
    pub criteria_type: CellType,
}

impl Rule2DSubrule {

    pub fn new(current_type: CellType, criteria_type: CellType, count: u32, op: CountOp,
               range: u8, neighborhood: Neighborhood2D, output_type: CellType,
               randomness: Option<f64>, limit: Option<u32>) -> Self {
        // compute the offsets for the neighborhood
        let offsets = neighborhood_offsets(neighborhood, range as i32);
        let pad = offsets.iter().map(|(dx, dy)| dx.abs().max(dy.abs()) as usize).max().unwrap_or(0);
        let early_exit = matches!(op, CountOp::Gt) && limit.is_none();
        // make the struct
        Self { current_type, criteria_type, count, op, limit, range,
            neighborhood, randomness, output_type, offsets, early_exit, pad }
    }

    /// Linear index offsets into a row-major grid of the given width.
    /// Interior cells can add these to their own index with no bounds logic.
    pub(crate) fn linear_offsets(&self, width: usize) -> Vec<isize> {
        self.offsets.iter().map(|(dx, dy)| *dy as isize * width as isize + *dx as isize).collect()
    }

    /// Validate subrule parameters (range>=1 and randomness/limit bounds).
    pub fn validate(&self) -> Result<(), RuleError> {
        if self.range < 1 { return Err(RuleError::InvalidRange2D); }
        if let Some(r) = self.randomness { if !(0.0..=1.0).contains(&r) { return Err(RuleError::InvalidRandomness); } }
        match self.op {
            CountOp::Eq => {
                if self.limit.is_some() { return Err(RuleError::InvalidRange2D); }
            }
            CountOp::Gt => {
                if let Some(hi) = self.limit { if hi < self.count { return Err(RuleError::InvalidRange2D); } }
            }
            CountOp::Lt => {
                if let Some(lo) = self.limit { if lo > self.count { return Err(RuleError::InvalidRange2D); } }
            }
        }
        Ok(())
    }

    /// Check whether `neighbors` satisfies the subrule's count condition.
    /// Caller already verified `current_type` and counted neighbors.
    #[inline]
    pub fn eval_condition(&self, neighbors: u32) -> bool {
        match (self.op, self.limit) {
            (CountOp::Eq, None) => neighbors == self.count,
            (CountOp::Gt, None) => neighbors >= self.count,
            (CountOp::Lt, None) => neighbors <= self.count,
            (CountOp::Gt, Some(hi)) => neighbors >= self.count && neighbors <= hi,
            (CountOp::Lt, Some(lo)) => neighbors <= self.count && neighbors >= lo,
            (CountOp::Eq, Some(_)) => false,
        }
    }
}

impl<'de> Deserialize<'de> for Rule2DSubrule {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // deserialize the helper so that we can rederive the offsets via `new`
        #[derive(Deserialize)]
        struct Helper {
            current_type: CellType,
            criteria_type: CellType,
            count: u32,
            op: CountOp,
            limit: Option<u32>,
            range: u8,
            neighborhood: Neighborhood2D,
            output_type: CellType,
            randomness: Option<f64>,
        }
        let h = Helper::deserialize(d)?;
        Ok(Self::new(h.current_type, h.criteria_type, h.count, h.op, h.range, h.neighborhood,
                     h.output_type, h.randomness, h.limit))

    }
}

/// A 2D rule consisting of multiple subrules evaluated in order.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule2D { pub subrules: Vec<Rule2DSubrule> }

impl Rule2D {
    /// Validate all subrules.
    pub fn validate(&self) -> Result<(), RuleError> { for s in &self.subrules { s.validate()?; } Ok(()) }
    /// Return the maximum range among subrules (or 1 if empty).
    pub fn range_max(&self) -> u8 { self.subrules.iter().map(|s| s.range).max().unwrap_or(1) }
    /// Whether any subrule draws from the RNG. Lets the stepper skip RNG setup entirely.
    pub(crate) fn needs_rng(&self) -> bool { self.subrules.iter().any(|s| s.randomness.is_some()) }
}

/// Per-step precomputation shared by every chunk of a 2D step.
///
/// Built once in `Grid2D::step` (cost is O(subrules × neighbors), i.e. tens of
/// operations against tens of thousands of cells) rather than cached on the grid,
/// so a caller mutating `grid.rule` between steps can never see a stale plan.
pub(crate) struct Rule2DPlan {
    /// Per subrule: neighbor offsets as linear indices for the grid's width.
    pub lin: Vec<Vec<isize>>,
    /// Chebyshev radius of the widest subrule; the interior margin.
    pub pad: usize,
    /// Whether any subrule needs an RNG.
    pub needs_rng: bool,
    /// Upper bound on neighbor visits per cell, summed over subrules. Used only
    /// to size the parallel split — an over-estimate for rules that early-exit or
    /// whose subrules rarely match, which is the safe direction (it never turns a
    /// grid that is too small to parallelize into one that is).
    pub work_per_cell: usize,
}

impl Rule2DPlan {
    pub fn new(rule: &Rule2D, width: usize) -> Self {
        Self {
            lin: rule.subrules.iter().map(|s| s.linear_offsets(width)).collect(),
            pad: rule.subrules.iter().map(|s| s.pad).max().unwrap_or(0),
            needs_rng: rule.needs_rng(),
            work_per_cell: rule.subrules.iter().map(|s| s.offsets.len()).sum::<usize>().max(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::value::{Error as DeError, StrDeserializer, U64Deserializer, U128Deserializer};

    #[test]
    fn type_counter_merge_combines_existing_and_new_entries() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let c = CellType::from("C");

        let mut left = TypeCounter::new();
        left.add(a);
        left.add(a);
        left.add(b);

        let mut right = TypeCounter::new();
        right.add(a);
        right.add(c);
        right.add(c);

        left.merge(&right);

        let mut seen = std::collections::HashMap::new();
        for (t, n) in left.iter() {
            seen.insert(t.as_str().to_string(), *n);
        }
        assert_eq!(seen.get("A"), Some(&3));
        assert_eq!(seen.get("B"), Some(&1));
        assert_eq!(seen.get("C"), Some(&2));
    }

    #[test]
    fn type_counter_default_and_add_work() {
        let mut c = TypeCounter::default();
        let a = CellType::from("A");
        c.add(a);
        c.add(a);
        let v: Vec<u64> = c.iter().map(|(_, n)| *n).collect();
        assert_eq!(v, vec![2]);
    }

    #[test]
    fn serde_u128_accepts_string_u64_and_u128_inputs() {
        assert_eq!(super::serde_u128::deserialize(StrDeserializer::<DeError>::new("123")).unwrap(), 123u128);
        assert_eq!(super::serde_u128::deserialize(U64Deserializer::<DeError>::new(123)).unwrap(), 123u128);
        assert_eq!(super::serde_u128::deserialize(U128Deserializer::<DeError>::new(123)).unwrap(), 123u128);
    }

    #[test]
    fn serde_u128_rejects_invalid_inputs() {
        let bad = super::serde_u128::deserialize(StrDeserializer::<DeError>::new("not-a-number"));
        assert!(bad.is_err());

        let bad_ty = super::serde_u128::deserialize(serde::de::value::BoolDeserializer::<DeError>::new(true));
        assert!(bad_ty.is_err());
    }

    #[test]
    fn eval_condition_covers_limit_variants() {
        let a = CellType::from("A");
        let b = CellType::from("B");

        let gt = Rule2DSubrule::new(a.clone(), b.clone(), 2, CountOp::Gt, 1, Neighborhood2D::Moore, b.clone(), None, Some(4));
        assert!(gt.eval_condition(2));
        assert!(gt.eval_condition(4));
        assert!(!gt.eval_condition(5));

        let lt = Rule2DSubrule::new(a.clone(), b.clone(), 4, CountOp::Lt, 1, Neighborhood2D::Moore, b.clone(), None, Some(2));
        assert!(lt.eval_condition(2));
        assert!(lt.eval_condition(4));
        assert!(!lt.eval_condition(1));

        let eq_with_limit = Rule2DSubrule::new(a, b, 3, CountOp::Eq, 1, Neighborhood2D::Moore, CellType::from("B"), None, Some(1));
        assert!(!eq_with_limit.eval_condition(3));
    }

    #[test]
    fn rule2d_validate_covers_limit_bounds() {
        let a = CellType::from("A");
        let b = CellType::from("B");

        let valid_gt = Rule2DSubrule::new(a.clone(), b.clone(), 2, CountOp::Gt, 1, Neighborhood2D::Moore, b.clone(), None, Some(5));
        assert!(valid_gt.validate().is_ok());

        let invalid_gt = Rule2DSubrule::new(a.clone(), b.clone(), 5, CountOp::Gt, 1, Neighborhood2D::Moore, b.clone(), None, Some(4));
        assert_eq!(invalid_gt.validate(), Err(RuleError::InvalidRange2D));

        let valid_lt = Rule2DSubrule::new(a.clone(), b.clone(), 5, CountOp::Lt, 1, Neighborhood2D::Moore, b.clone(), None, Some(2));
        assert!(valid_lt.validate().is_ok());

        let invalid_lt = Rule2DSubrule::new(a, b, 5, CountOp::Lt, 1, Neighborhood2D::Moore, CellType::from("B"), None, Some(6));
        assert_eq!(invalid_lt.validate(), Err(RuleError::InvalidRange2D));
    }

    #[test]
    fn rule_applies_and_range_helpers_are_covered() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let sub = Rule1DSubrule {
            wolfram_code: 1u128 << 7,
            randomness: None,
            n: 1,
            output_type: b,
            current_type: a,
            criteria_type: a,
        };
        assert!(sub.applies(&[a, a, a]));
        assert!(!sub.applies(&[a, b, a]));

        let r1 = Rule1D { subrules: vec![sub.clone()] };
        assert!(r1.validate().is_ok());
        assert_eq!(r1.n_max(), 1);

        let r2 = Rule2D {
            subrules: vec![
                Rule2DSubrule::new(a, b, 1, CountOp::Gt, 1, Neighborhood2D::Moore, b, None, None),
                Rule2DSubrule::new(CellType::from("B"), CellType::from("A"), 1, CountOp::Gt, 3, Neighborhood2D::Moore, CellType::from("A"), None, None),
            ],
        };
        assert_eq!(r2.range_max(), 3);
        assert!(r2.validate().is_ok());
    }

    #[test]
    fn rule1d_validate_covers_n3_and_invalid_wolfram_code() {
        let a = CellType::from("A");
        let b = CellType::from("B");

        // n=3 skips the bounded-code check (all u128 values are valid).
        let n3 = Rule1DSubrule {
            current_type: a,
            criteria_type: a,
            wolfram_code: u128::MAX,
            n: 3,
            randomness: None,
            output_type: b,
        };
        assert!(n3.validate().is_ok());

        // n=1 has 2^(2n+1)=8 patterns, so max valid wolfram code is < 2^8.
        let bad_code = Rule1DSubrule {
            wolfram_code: 1u128 << 8,
            n: 1,
            ..n3
        };
        assert!(bad_code.validate().is_err());
    }

    #[test]
    fn eval_condition_covers_lt_without_limit() {
        let a = CellType::from("A");
        let sub = Rule2DSubrule::new(
            a,
            a,
            2,
            CountOp::Lt,
            1,
            Neighborhood2D::Moore,
            a,
            None,
            None,
        );
        assert!(sub.eval_condition(2));
        assert!(sub.eval_condition(1));
        assert!(!sub.eval_condition(3));
    }

    #[test]
    fn validate_and_deserialize_error_paths_are_exercised() {
        let a = CellType::from("A");
        let b = CellType::from("B");

        let bad_1d = Rule1DSubrule {
            current_type: a,
            criteria_type: b,
            wolfram_code: 0,
            n: 0,
            randomness: None,
            output_type: b,
        };
        assert!(Rule1D { subrules: vec![bad_1d] }.validate().is_err());

        let bad_rand = Rule2DSubrule::new(a, b, 1, CountOp::Gt, 1, Neighborhood2D::Moore, b, Some(2.0), None);
        assert!(bad_rand.validate().is_err());

        let bad_eq_limit = Rule2DSubrule::new(a, b, 1, CountOp::Eq, 1, Neighborhood2D::Moore, b, None, Some(0));
        assert!(bad_eq_limit.validate().is_err());

        let bad_gt_limit = Rule2DSubrule::new(a, b, 2, CountOp::Gt, 1, Neighborhood2D::Moore, b, None, Some(1));
        assert!(bad_gt_limit.validate().is_err());

        let bad_lt_limit = Rule2DSubrule::new(a, b, 1, CountOp::Lt, 1, Neighborhood2D::Moore, b, None, Some(2));
        assert!(bad_lt_limit.validate().is_err());

        let bad_rule = Rule2D { subrules: vec![bad_rand] };
        assert!(bad_rule.validate().is_err());

        let bad_json = "{\"subrules\":\"nope\"}";
        let deser: Result<Rule2D, _> = serde_json::from_str(bad_json);
        assert!(deser.is_err());

        let bad_subrule_json = "{\"current_type\":\"A\"}";
        let deser_sub: Result<Rule2DSubrule, _> = serde_json::from_str(bad_subrule_json);
        assert!(deser_sub.is_err());
    }
}

