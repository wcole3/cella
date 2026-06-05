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
use crate::types::CellType;
use rand::Rng;
use serde::{Deserialize, Serialize};

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
/// - `Langdon`: diagonal cells where |dx|==|dy|<=n.
/// - `StraightLine`: cells in straight cardinal lines (up/down/left/right) up to range n.
/// - `Knight`: cells reachable from the origin in at most `range` chess-knight hops
///   (each hop is an L-shaped move: ±1/±2 or ±2/±1). `range=1` gives exactly the
///   8 classic knight squares; `range=N` unions all cells reachable in 1..=N hops.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Neighborhood2D {
    Moore,
    VonNeumann,
    Langdon,
    StraightLine,
    Knight,
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
    pub current_type: CellType,
    pub criteria_type: CellType,
    #[serde(with = "serde_u128")]
    pub wolfram_code: u128,
    /// Neighborhood radius (>=1): window size is 2n+1.
    pub n: u8,
    /// Optional randomness in (0-1); pass only if random >= value.
    pub randomness: Option<f64>,
    pub output_type: CellType,
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

    ///
    /// Converts a slice of booleans into a single integer value representing the binary pattern.
    ///
    /// # Parameters
    /// - `window`: A slice of boolean values, where each `true` represents a binary `1`
    ///   and each `false` represents a binary `0`.
    ///
    /// # Returns
    /// A `usize` value that represents the binary pattern of the boolean slice.
    /// The first element in the slice corresponds to the most significant bit, and
    /// the last element to the least significant bit.
    ///
    /// # Notes
    /// - The function assumes the input slice is not empty. An empty slice would result
    ///   in the `idx` remaining `0`.
    /// - The order of the input slice is significant in forming the binary pattern.
    ///
    /// # Complexity
    /// The function runs in `O(n)` time, where `n` is the length of the `window`, as it iterates
    /// over the slice once.
    fn pattern_index_1d(window: &[bool]) -> usize {
        let mut idx = 0usize;
        for &b in window { idx = (idx << 1) | (b as usize); }
        idx
    }

    /// Evaluate this subrule against the provided neighborhood window.
    ///
    /// `center_current` is the current type of the center cell; `neighborhood`
    /// is a contiguous window of length 2n+1 centered at the cell.
    /// Returns `Some(output_type)` if the subrule triggers.
    ///
    /// ```rust
    /// use cella_lib::{CellType, Rule1DSubrule};
    /// let x = CellType::from("X");
    /// let y = CellType::from("Y");
    /// let sub = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 1u128<<2, n: 1, randomness: None, output_type: y.clone() };
    /// let window = vec![CellType::inactive(), x.clone(), CellType::inactive()];
    /// let out = sub.applies_and_output(&x, &window);
    /// assert_eq!(out, Some(&y));
    /// ```
    pub fn applies_and_output(&self, center_current: &CellType, neighborhood: &Vec<CellType>) -> Option<&CellType> {
        if center_current != &self.current_type { return None; }
        let crit = &self.criteria_type;
        let window: Vec<bool> = neighborhood.iter().map(|t| t == crit).collect();
        let idx = Self::pattern_index_1d(&window);
        let bit = (self.wolfram_code >> idx) & 1u128;
        if bit == 1u128 {
            if let Some(r) = self.randomness {
                let mut rng = rand::thread_rng();
                let v: f64 = rng.r#gen();
                if v < r { return None; }
            }
            return Some(&self.output_type);
        }
        None
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
/// let s = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 1, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: b.clone() };
/// assert!(s.validate().is_ok());
/// ```
///
/// StraightLine neighborhood example
/// ```rust
/// use cella_lib::{CellType, Rule2DSubrule, Neighborhood2D, CountOp};
/// let a = CellType::from("A");
/// let b = CellType::from("B");
/// let sub = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::StraightLine, randomness: None, output_type: b.clone() };
/// // Place two B's in cardinal directions: up (0,-1) and right (+1,0)
/// let out = sub.applies_and_output(&a, |dx, dy| {
///     if (dx, dy) == (0, -1) || (dx, dy) == (1, 0) { b.clone() } else { CellType::inactive() }
/// });
/// assert_eq!(out, Some(&b));
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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule2DSubrule {
    pub current_type: CellType,
    pub criteria_type: CellType,
    /// Comparison baseline value.
    pub count: u32,
    /// Comparison operator: lt/gt/eq. When accompanied by `limit`, creates a
    /// between-range inclusive clause (see `validate`).
    pub op: CountOp,
    /// Optional bound for "between":
    /// - If op=Gt, `limit` is an inclusive upper bound (count..=limit).
    /// - If op=Lt, `limit` is an inclusive lower bound (limit..=count).
    /// - If op=Eq, `limit` must be None.
    pub limit: Option<u32>,
    /// Range n >= 1 defines (2n+1)^2 window.
    pub range: u8,
    pub neighborhood: Neighborhood2D,
    /// Optional randomness in (0-1); pass only if random >= value.
    pub randomness: Option<f64>,
    pub output_type: CellType,
}

/// Returns `true` if `(dx, dy)` is reachable from `(0, 0)` in at most `max_moves` knight hops.
/// Each hop is an L-shaped chess-knight move: (±1, ±2) or (±2, ±1).
fn knight_reachable(dx: i32, dy: i32, max_moves: u8) -> bool {
    use std::collections::VecDeque;
    if dx == 0 && dy == 0 { return false; }
    let mut visited = std::collections::HashSet::new();
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

/// Public helper: returns `true` when `(dx, dy)` belongs to the neighborhood of radius `n`
/// for the given `kind`. `(0, 0)` always returns `false`.
pub fn neighborhood_contains(dx: i32, dy: i32, n: i32, kind: Neighborhood2D) -> bool {
    if dx == 0 && dy == 0 { return false; }
    match kind {
        Neighborhood2D::Moore => dx.abs() <= n && dy.abs() <= n,
        Neighborhood2D::VonNeumann => dx.abs() + dy.abs() <= n,
        Neighborhood2D::Langdon => dx.abs() == dy.abs() && dx.abs() <= n,
        Neighborhood2D::StraightLine => (dx == 0 && dy.abs() <= n) || (dy == 0 && dx.abs() <= n),
        Neighborhood2D::Knight => knight_reachable(dx, dy, n as u8),
    }
}

impl Rule2DSubrule {
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

    fn within_neighborhood(dx: i32, dy: i32, n: i32, kind: Neighborhood2D) -> bool {
        neighborhood_contains(dx, dy, n, kind)
    }

    /// Evaluate this subrule by counting matching neighbors and applying op/limit.
    pub fn applies_and_output<F>(&self, center_current: &CellType, get_neighbor: F) -> Option<&CellType>
    where F: Fn(i32, i32) -> CellType {
        if center_current != &self.current_type { return None; }
        let n = self.range as i32;
        // Knight moves can reach up to 2*range steps per axis (N hops × max 2 per hop),
        // so we widen the iteration window to 2*n for Knight neighborhoods.
        let half = if self.neighborhood == Neighborhood2D::Knight { n * 2 } else { n };
        let mut neighbors = 0u32;
        for dy in -half..=half {
            for dx in -half..=half {
                if !Self::within_neighborhood(dx, dy, n, self.neighborhood) { continue; }
                let t = get_neighbor(dx, dy);
                if t == self.criteria_type { neighbors += 1; }
            }
        }
        let pass = match (self.op, self.limit) {
            (CountOp::Eq, None) => neighbors == self.count,
            (CountOp::Gt, None) => neighbors >= self.count,
            (CountOp::Lt, None) => neighbors <= self.count,
            (CountOp::Gt, Some(hi)) => neighbors >= self.count && neighbors <= hi,
            (CountOp::Lt, Some(lo)) => neighbors <= self.count && neighbors >= lo,
            (CountOp::Eq, Some(_)) => false,
        };
        if pass {
            if let Some(r) = self.randomness {
                let mut rng = rand::thread_rng();
                let v: f64 = rng.r#gen();
                if v < r { return None; }
            }
            return Some(&self.output_type);
        }
        None
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
