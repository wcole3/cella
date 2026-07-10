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
use rand::Rng;
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
        for (t, c) in &other.entries {
            for e in &mut self.entries {
                if e.0 == *t {
                    e.1 += *c;
                    return;
                }
            }
            self.entries.push((*t, *c));
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&CellType, &u64)> {
        self.entries.iter().map(|(t, c)| (t, c))
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
        Neighborhood2D::Moore | Neighborhood2D::VonNeumann | Neighborhood2D::Langton => {
            for dx in -n..=n {
                for dy in -n..=n {
                    if dx == 0 && dy == 0 { continue; }
                    match neighborhood {
                        Neighborhood2D::Moore => {set.insert((dx, dy));}
                        Neighborhood2D::VonNeumann => {if dx.abs() + dy.abs() <= n { set.insert((dx, dy)); }}
                        Neighborhood2D::Langton => {if dx.abs() == dy.abs() && dx.abs() <= n { set.insert((dx, dy)); }}
                        _ => {}
                    }
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
    /// Less than: `neighbor_count < target_count`.
    #[serde(rename = "lt")] Lt,
    /// Greater than: `neighbor_count > target_count`.
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
        // make the struct
        Self { current_type, criteria_type, count, op, limit, range,
            neighborhood, randomness, output_type, offsets }
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
}
