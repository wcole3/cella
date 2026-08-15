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
    /// Allocation-free until the first `add` — the rayon `reduce` identity and
    /// empty chunks then cost nothing.
    pub fn new() -> Self {
        Self { entries: Vec::new() }
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

    /// Add `n` counts of `t` at once (no-op for `n == 0`, matching the
    /// per-cell path, which never creates zero-count entries).
    pub(crate) fn add_n(&mut self, t: CellType, n: u64) {
        if n == 0 {
            return;
        }
        for e in &mut self.entries {
            if e.0 == t {
                e.1 += n;
                return;
            }
        }
        self.entries.push((t, n));
    }

    /// Remove one count of `t`, dropping the entry when it reaches zero.
    /// Absent types are a no-op (the type was dominant-skipped or never counted).
    pub(crate) fn sub(&mut self, t: CellType) {
        for i in 0..self.entries.len() {
            if self.entries[i].0 == t {
                self.entries[i].1 -= 1;
                if self.entries[i].1 == 0 {
                    self.entries.swap_remove(i);
                }
                return;
            }
        }
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

/// Precomputed helper data for **one subrule** during a single 1D step.
///
/// See [`Rule1DPlan`] for what a "plan" is and why it exists. This struct
/// holds the two things we don't want to recompute for every cell:
///
/// - `code_lo`: a faster-to-use copy of the subrule's `wolfram_code`.
///   The code is stored on the subrule as a `u128` because an `n = 3` window
///   has 128 possible patterns. But for `n <= 2` there are at most 32
///   patterns, so the whole transition table fits in the low 64 bits — and a
///   64-bit shift is a single CPU instruction, while a 128-bit shift with a
///   runtime shift amount compiles to several. The stepper uses `code_lo` for
///   `n <= 2` and falls back to the full `u128` only for `n = 3`.
/// - `valid`: whether `n` is in the supported `1..=3` range. Checking this
///   once per step (instead of once per cell) keeps the per-cell loop free of
///   re-validation. An invalid subrule simply never matches.
#[derive(Clone, Copy)]
pub(crate) struct Sub1DPlan {
    /// Low 64 bits of `wolfram_code`; the complete transition table for `n <= 2`.
    pub code_lo: u64,
    /// `1 <= n <= 3`. Hoisted out of the per-cell loop.
    pub valid: bool,
}

/// Marks a 1D rule that qualifies for the "packed" fast path. Built by
/// `Rule1DPlan::detect_packed`.
///
/// # What shape qualifies?
///
/// A classic two-state Wolfram automaton, written as two subrules:
///
/// ```text
/// { current: active,   criteria: active, code: C, n: 1, output: active }
/// { current: inactive, criteria: active, code: C, n: 1, output: active }
/// ```
///
/// Read those together and something nice falls out: whether the cell is
/// currently active (first subrule) or inactive (second), the *same* thing
/// happens — look up the 3-cell window in table `C`; on a hit the cell
/// becomes `active`, on a miss no subrule matches and the engine's default
/// makes it inactive. So the next state doesn't depend on the cell's own type
/// at all, only on the window bits. That is exactly the definition of a
/// Wolfram rule, and it means the whole row can be treated as plain bits:
/// 1 = active, 0 = inactive.
///
/// `Grid1D::step_packed` exploits that by storing 64 cells in one 64-bit
/// integer and stepping all 64 with a handful of machine instructions — see
/// its docs for the walk-through.
///
/// # Safety valve
///
/// Bits can only represent a two-type world. If any other cell type appears
/// on the grid (e.g. painted in by the user), the stepper detects it with a
/// quick scan each step and falls back to the normal scalar path, so results
/// are always identical to the slow path — the fast path is invisible except
/// for speed.
#[derive(Clone, Copy)]
pub(crate) struct PackedWolfram {
    /// The one counted, non-background cell type (e.g. "X").
    pub active: CellType,
    /// The shared transition table. With `n = 1` the window is 3 cells, so
    /// there are only 2³ = 8 possible windows and the table fits in 8 bits.
    pub code: u8,
}

/// Per-step "plan" for a 1D rule: everything the stepper can work out **once**
/// before touching any cells.
///
/// # What is a plan?
///
/// When `Grid1D::step()` runs, the same rule is applied to every cell — often
/// tens of thousands of them. Some values the stepper needs depend only on the
/// *rule*, not on the *cell*, so computing them per cell would repeat identical
/// work thousands of times. A plan gathers those values in one small struct
/// that is built at the top of `step()` and then shared (read-only) by every
/// cell and every worker thread in that step.
///
/// # Why rebuild it every step instead of caching it on the grid?
///
/// Two reasons:
///
/// 1. `rule` is a public field, so a caller can mutate it between steps. A
///    cached plan could then be stale and silently disagree with the rule.
///    Rebuilding each step makes that bug impossible.
/// 2. Building the plan is O(number of subrules) — a handful of operations —
///    while a step is O(number of cells). The cost is unmeasurable.
///
/// The same pattern exists for 2D as [`Rule2DPlan`], which carries more data
/// (neighbor offsets converted to linear indices). One entry in `subs` lines
/// up with one entry in `rule.subrules`, matched by index.
pub(crate) struct Rule1DPlan {
    /// One [`Sub1DPlan`] per subrule, in the same order as `rule.subrules`.
    pub subs: Vec<Sub1DPlan>,
    /// `Some` when the rule matches the [`PackedWolfram`] shape and the
    /// bit-parallel fast path may be attempted.
    pub packed: Option<PackedWolfram>,
}

impl Rule1DPlan {
    /// Build the plan for `rule`. Called once at the top of `Grid1D::step()`.
    ///
    /// (Derived data cannot live on `Rule1DSubrule` itself: its fields are
    /// public and it has no constructor, so anyone building one with struct
    /// literal syntax would skip the precomputation.)
    pub fn new(rule: &Rule1D, inactive: CellType) -> Self {
        let subs = rule.subrules.iter()
            .map(|s| Sub1DPlan {
                // Truncating to u64 is safe for the n <= 2 fast path: those
                // windows only ever index bits 0..=31.
                code_lo: s.wolfram_code as u64,
                valid: (1..=3).contains(&s.n),
            })
            .collect();
        Self { subs, packed: Self::detect_packed(rule, inactive) }
    }

    /// See [`PackedWolfram`] for the shape this recognizes.
    fn detect_packed(rule: &Rule1D, inactive: CellType) -> Option<PackedWolfram> {
        let [a, b] = rule.subrules.as_slice() else { return None };
        let active = a.current_type;
        let eligible = |s: &Rule1DSubrule| {
            s.n == 1 && s.randomness.is_none() && s.criteria_type == active
                && s.output_type == active && s.wolfram_code < 256
        };
        if active != inactive
            && b.current_type == inactive
            && eligible(a)
            && eligible(b)
            && a.wolfram_code == b.wolfram_code
        {
            Some(PackedWolfram { active, code: a.wolfram_code as u8 })
        } else {
            None
        }
    }
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
    /// Precomputed inclusive `(lo, hi)` neighbor-count range equivalent to
    /// `(op, count, limit)` — `eval_condition` becomes two compares instead of a
    /// six-arm match per matching cell. `Eq + limit` (rejected by `validate`)
    /// maps to the empty range `(1, 0)`.
    #[serde(skip)]
    pub(crate) cond_lo: u32,
    #[serde(skip)]
    pub(crate) cond_hi: u32,
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
        let (cond_lo, cond_hi) = match (op, limit) {
            (CountOp::Eq, None) => (count, count),
            (CountOp::Gt, None) => (count, u32::MAX),
            (CountOp::Lt, None) => (0, count),
            (CountOp::Gt, Some(hi)) => (count, hi),
            (CountOp::Lt, Some(lo)) => (lo, count),
            (CountOp::Eq, Some(_)) => (1, 0), // invalid; never matches
        };
        // make the struct
        Self { current_type, criteria_type, count, op, limit, range,
            neighborhood, randomness, output_type, offsets, early_exit, pad, cond_lo, cond_hi }
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
    ///
    /// The `(op, count, limit)` semantics are folded into the precomputed
    /// inclusive `cond_lo..=cond_hi` range at construction. Only usable on
    /// subrules built via [`Rule2DSubrule::new`] (which deserialization also
    /// routes through) — a struct literal skips the precomputation, like the
    /// other derived fields.
    #[inline]
    pub fn eval_condition(&self, neighbors: u32) -> bool {
        self.cond_lo <= neighbors && neighbors <= self.cond_hi
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

/// Marks a rule that qualifies for the 2D "bit-parallel" fast path, plus the
/// small lookup table that path needs. Built by `Rule2DPlan::detect_packed`.
///
/// # The idea, step by step
///
/// Normally the stepper visits every cell, and for each cell walks its
/// subrules and counts matching neighbors one at a time. That is a lot of
/// work per cell. But a big family of rules — Conway's Game of Life and its
/// relatives — is much simpler than the general machinery allows:
///
/// 1. Only **two cell types** ever appear: one we call `active` (e.g.
///    "Alive") and the engine's inactive/background type.
/// 2. Every subrule counts the **same** type of neighbor (`active`) over the
///    **same** neighborhood, and that neighborhood fits inside the 8 cells
///    that immediately surround a cell (radius 1).
/// 3. No subrule uses randomness.
///
/// When all of that holds, a cell's next state depends on exactly two things:
/// *what it is now* (active or not) and *how many active neighbors it has*
/// (0 through 8). Two possible current states × nine possible counts = only
/// **18 distinct situations**. So instead of re-running the subrule chain for
/// every cell on every step, we run it once for each of the 18 situations
/// while building the plan, and store the answers in `table`. During the
/// step, "evaluate the rule" becomes a table lookup.
///
/// The second trick is *how* the lookup is applied: `Grid2D::step_packed`
/// stores the grid as one **bit** per cell inside 64-bit integers ("words"),
/// so a single machine instruction operates on 64 cells at once. See that
/// method's docs for the walk-through.
///
/// # Safety valve
///
/// The table only describes a two-type world. If the grid ever contains any
/// *other* cell type (say the user paints one in), the fast path would get it
/// wrong — so the stepper scans the grid each step and simply falls back to
/// the normal scalar path when it finds a foreign type. Result: the fast path
/// is invisible except for speed.
#[derive(Clone, Copy)]
pub(crate) struct PackedThreshold2D {
    /// The one counted, non-background cell type (e.g. "Alive").
    pub active: CellType,
    /// The 18 precomputed answers: `table[cur][count]` is `true` when a cell
    /// that is currently `cur` (0 = inactive, 1 = active) with `count` active
    /// neighbors becomes `active` next step. Counts a small neighborhood can
    /// never reach (e.g. 8 for VonNeumann's 4 neighbors) are filled in anyway;
    /// they are simply never looked up.
    pub table: [[bool; 9]; 2],
    /// Which of the eight surrounding positions this rule's neighborhood
    /// actually counts — Moore radius 1 counts all eight, VonNeumann only the
    /// four up/down/left/right ones, and so on. Ordered like
    /// `neighborhood_offsets(Moore, 1)` (sorted by `(dx, dy)`).
    pub slots: [bool; 8],
}

/// Per-step "plan" for a 2D rule: everything `Grid2D::step()` can work out
/// **once** before touching any cells, shared read-only by every chunk and
/// worker thread of that step.
///
/// # What is a plan, and why does it exist?
///
/// A step applies the same rule to every cell — commonly tens of thousands.
/// Several values the stepper needs depend only on the *rule* and the *grid
/// width*, never on the individual cell, so computing them per cell would
/// repeat identical work thousands of times. The plan is where that shared,
/// rule-level work happens exactly once. (The 1D equivalent is
/// [`Rule1DPlan`], which has the longer general explanation.)
///
/// The most important piece is the neighbor-offset conversion. A subrule
/// stores its neighborhood as `(dx, dy)` pairs; to read a neighbor you would
/// normally compute `(y + dy) * width + (x + dx)` — a multiply per neighbor
/// per cell. The plan converts each pair once into a *linear* offset
/// (`dy * width + dx`) that interior cells can simply add to their own flat
/// index: `cells[idx + offset]`. No multiply, no coordinate math in the hot
/// loop.
///
/// # Why rebuild it every step instead of caching it on the grid?
///
/// `grid.rule` is a public field, so a caller can change the rule between
/// steps; a cached plan could then be stale and silently disagree with it.
/// Rebuilding costs O(subrules × neighbors) — tens of operations against tens
/// of thousands of cells — so it is effectively free and makes stale-plan bugs
/// impossible.
pub(crate) struct Rule2DPlan {
    /// Every subrule's linear neighbor offsets, concatenated into one flat
    /// buffer (one allocation per step instead of one `Vec` per subrule; the
    /// inner loop walks a plain slice instead of chasing a `&Vec` pointer).
    /// Use [`Rule2DPlan::lin`] to get subrule `i`'s slice.
    lin_flat: Vec<isize>,
    /// Where each subrule's offsets sit inside `lin_flat`:
    /// subrule `i` owns `lin_flat[spans[i].0 as usize .. spans[i].1 as usize]`.
    spans: Vec<(u32, u32)>,
    /// The widest subrule's Chebyshev radius (max of `|dx|`, `|dy|`). Cells at
    /// least this far from every border are "interior": all their neighbors
    /// are guaranteed in bounds, so they take the fast path with no bounds
    /// checks. Cells closer to a border take the checked "edge" path.
    pub pad: usize,
    /// Whether any subrule uses `randomness`. When false, the stepper skips
    /// RNG construction entirely.
    pub needs_rng: bool,
    /// Rough estimate of work per cell (total neighbor visits across all
    /// subrules). Only used to decide how many parallel chunks a step is worth
    /// splitting into. Deliberately an over-estimate — that direction is safe,
    /// because it never makes a too-small grid look worth parallelizing.
    pub work_per_cell: usize,
    /// `Some` when the rule matches the [`PackedThreshold2D`] shape and the
    /// bit-parallel fast path may be attempted.
    pub packed: Option<PackedThreshold2D>,
}

impl Rule2DPlan {
    /// Build the plan for `rule` on a grid of the given `width`. Called once
    /// at the top of `Grid2D::step()`.
    pub fn new(rule: &Rule2D, width: usize) -> Self {
        Self::with_inactive(rule, width, CellType::inactive())
    }

    pub fn with_inactive(rule: &Rule2D, width: usize, inactive: CellType) -> Self {
        let mut lin_flat = Vec::with_capacity(rule.subrules.iter().map(|s| s.offsets.len()).sum());
        let mut spans = Vec::with_capacity(rule.subrules.len());
        for s in &rule.subrules {
            let start = lin_flat.len() as u32;
            lin_flat.extend(s.offsets.iter().map(|&(dx, dy)| dy as isize * width as isize + dx as isize));
            spans.push((start, lin_flat.len() as u32));
        }
        Self {
            lin_flat,
            spans,
            pad: rule.subrules.iter().map(|s| s.pad).max().unwrap_or(0),
            needs_rng: rule.needs_rng(),
            work_per_cell: rule.subrules.iter().map(|s| s.offsets.len()).sum::<usize>().max(1),
            packed: Self::detect_packed(rule, inactive),
        }
    }

    /// See [`PackedThreshold2D`] for the shape this recognizes: a two-type
    /// world where every subrule counts the same `active` type over one shared
    /// radius-1 neighborhood, deterministically.
    fn detect_packed(rule: &Rule2D, inactive: CellType) -> Option<PackedThreshold2D> {
        let first = rule.subrules.first()?;
        let active = first.criteria_type;
        if active == inactive {
            return None;
        }
        let shape = (first.neighborhood, first.range);
        for s in &rule.subrules {
            let two_type = |t: CellType| t == active || t == inactive;
            if s.pad != 1
                || (s.neighborhood, s.range) != shape
                || s.criteria_type != active
                || s.randomness.is_some()
                || !two_type(s.current_type)
                || !two_type(s.output_type)
            {
                return None;
            }
        }
        // Which of the eight Moore-1 slots this neighborhood counts.
        let moore = neighborhood_offsets(Neighborhood2D::Moore, 1);
        let mut slots = [false; 8];
        for &(dx, dy) in &first.offsets {
            let j = moore.iter().position(|&o| o == (dx, dy))?; // pad == 1 makes misses impossible
            slots[j] = true;
        }
        // Build the (current, count) -> next table by running the subrule
        // chain exactly as the scalar path would.
        let mut table = [[false; 9]; 2];
        for (cur_slot, cur) in [(0usize, inactive), (1usize, active)] {
            for count in 0..=8u32 {
                let next = rule.subrules.iter()
                    .find(|s| s.current_type == cur && s.eval_condition(count))
                    .map_or(inactive, |s| s.output_type);
                table[cur_slot][count as usize] = next == active;
            }
        }
        Some(PackedThreshold2D { active, table, slots })
    }

    /// The linear neighbor offsets for subrule `i` (same index as
    /// `rule.subrules`). An interior cell at flat index `idx` reads neighbor
    /// `k` as `cells[idx.wrapping_add_signed(plan.lin(i)[k])]`.
    #[inline]
    pub fn lin(&self, i: usize) -> &[isize] {
        let (a, b) = self.spans[i];
        &self.lin_flat[a as usize..b as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_wolfram_detection_arms() {
        let x = CellType::from("X");
        let y = CellType::from("Y");
        let inactive = CellType::inactive();
        let sub = |current: CellType, criteria: CellType, code: u128, n: u8,
                   randomness: Option<f64>, output: CellType| Rule1DSubrule {
            current_type: current, criteria_type: criteria, wolfram_code: code, n,
            randomness, output_type: output,
        };
        let good = Rule1D { subrules: vec![
            sub(x, x, 30, 1, None, x),
            sub(inactive, x, 30, 1, None, x),
        ]};
        let detected = Rule1DPlan::new(&good, inactive).packed.expect("canonical shape detects");
        assert_eq!(detected.active, x);
        assert_eq!(detected.code, 30);

        let reject = |rule: Rule1D, why: &str| {
            assert!(Rule1DPlan::new(&rule, inactive).packed.is_none(), "{why}");
        };
        reject(Rule1D { subrules: vec![sub(x, x, 30, 1, None, x)] }, "one subrule");
        reject(Rule1D { subrules: vec![
            sub(x, x, 30, 1, None, x), sub(inactive, x, 30, 1, None, x), sub(inactive, x, 30, 1, None, x),
        ]}, "three subrules");
        reject(Rule1D { subrules: vec![sub(x, x, 30, 2, None, x), sub(inactive, x, 30, 2, None, x)] }, "n != 1");
        reject(Rule1D { subrules: vec![sub(x, x, 30, 1, Some(0.5), x), sub(inactive, x, 30, 1, None, x)] }, "randomness");
        reject(Rule1D { subrules: vec![sub(x, y, 30, 1, None, x), sub(inactive, x, 30, 1, None, x)] }, "criteria mismatch");
        reject(Rule1D { subrules: vec![sub(x, x, 30, 1, None, y), sub(inactive, x, 30, 1, None, x)] }, "output mismatch");
        reject(Rule1D { subrules: vec![sub(x, x, 30, 1, None, x), sub(inactive, x, 110, 1, None, x)] }, "codes differ");
        reject(Rule1D { subrules: vec![sub(x, x, 300, 1, None, x), sub(inactive, x, 300, 1, None, x)] }, "code >= 256");
        reject(Rule1D { subrules: vec![sub(x, x, 30, 1, None, x), sub(y, x, 30, 1, None, x)] }, "second current not inactive");
        reject(Rule1D { subrules: vec![sub(inactive, inactive, 30, 1, None, inactive), sub(inactive, inactive, 30, 1, None, inactive)] }, "active == inactive");
    }

    #[test]
    fn type_counter_add_n_merges_and_skips_zero() {
        let a = CellType::from("A");
        let mut c = TypeCounter::new();
        c.add_n(a, 0);
        assert_eq!(c.iter().count(), 0, "zero adds no entry");
        c.add_n(a, 5);
        c.add_n(a, 2);
        assert_eq!(c.iter().find(|(t, _)| **t == a).map(|(_, n)| *n), Some(7));
    }
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

