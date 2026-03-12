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
use rand::Rng;
use serde::{Deserialize, Serialize};
use crate::types::CellType;

mod serde_u128 {
    use serde::{Deserializer, Serializer};
    use serde::de::{self, Visitor};
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
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Neighborhood2D {
    Moore,
    VonNeumann,
    Langdon,
    StraightLine,
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
/// let sub = Rule1DSubrule { current_type: CellType("X".into()), criteria_type: CellType("X".into()), wolfram_code: 1u128<<2, n: 1, randomness: None, output_type: CellType("Y".into()) };
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
        let patterns: u32 = 1u32 << b; // number of neighborhood patterns = 2^(2n+1)
        if patterns < 128 {
            let max: u128 = 1u128 << patterns;
            if self.wolfram_code >= max {
                return Err(RuleError::InvalidWolframCode(self.wolfram_code, self.n));
            }
        }
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
    ///
    /// fn
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
    /// let x = CellType("X".into());
    /// let y = CellType("Y".into());
    /// let sub = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 1u128<<2, n: 1, randomness: None, output_type: y.clone() };
    /// let window = vec![CellType::inactive(), x.clone(), CellType::inactive()];
    /// let out = sub.applies_and_output(&x, &window);
    /// assert_eq!(out, Some(y));
    /// ```
    pub fn applies_and_output(&self, center_current: &CellType, neighborhood: &[CellType]) -> Option<CellType> {
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
            return Some(self.output_type.clone());
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
/// let a = CellType("A".into());
/// let b = CellType("B".into());
/// let s = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 1, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: b.clone() };
/// assert!(s.validate().is_ok());
/// ```
///
/// StraightLine neighborhood example
/// ```rust
/// use cella_lib::{CellType, Rule2DSubrule, Neighborhood2D, CountOp};
/// let a = CellType("A".into());
/// let b = CellType("B".into());
/// let sub = Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::StraightLine, randomness: None, output_type: b.clone() };
/// // Place two B's in cardinal directions: up (0,-1) and right (+1,0)
/// let out = sub.applies_and_output(&a, |dx, dy| {
///     if (dx, dy) == (0, -1) || (dx, dy) == (1, 0) { b.clone() } else { CellType::inactive() }
/// });
/// assert_eq!(out, Some(b));
/// ```
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum CountOp {
    #[serde(rename = "lt")] Lt,
    #[serde(rename = "gt")] Gt,
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
        if dx == 0 && dy == 0 { return false; }
        match kind {
            Neighborhood2D::Moore => dx.abs() <= n && dy.abs() <= n,
            Neighborhood2D::VonNeumann => dx.abs() + dy.abs() <= n,
            Neighborhood2D::Langdon => dx.abs() == dy.abs() && dx.abs() <= n,
            Neighborhood2D::StraightLine => (dx == 0 && dy.abs() <= n) || (dy == 0 && dx.abs() <= n),
        }
    }

    /// Evaluate this subrule by counting matching neighbors and applying op/limit.
    pub fn applies_and_output<F>(&self, center_current: &CellType, mut get_neighbor: F) -> Option<CellType>
    where F: FnMut(i32, i32) -> CellType {
        if center_current != &self.current_type { return None; }
        let n = self.range as i32;
        let mut neighbors = 0u32;
        for dy in -n..=n {
            for dx in -n..=n {
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
            return Some(self.output_type.clone());
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
