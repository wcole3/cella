//! Rule definitions for 1D and 2D cellular automata.
use rand::Rng;
use serde::{Deserialize, Serialize};
use crate::types::CellType;

/// Neighborhood types for 2D rules.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Neighborhood2D {
    Moore,
    VonNeumann,
    Langdon,
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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule1DSubrule {
    pub current_type: CellType,
    pub criteria_type: CellType,
    pub wolfram_code: u128,
    /// Neighborhood radius (>=1): window size is 2n+1.
    pub n: u8,
    /// Optional randomness in [0,1]; pass only if random >= value.
    pub randomness: Option<f64>,
    pub output_type: CellType,
}

impl Rule1DSubrule {
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

    fn pattern_index_1d(window: &[bool]) -> usize {
        let mut idx = 0usize;
        for &b in window { idx = (idx << 1) | (b as usize); }
        idx
    }

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
    pub fn validate(&self) -> Result<(), RuleError> { for s in &self.subrules { s.validate()?; } Ok(()) }
    pub fn n_max(&self) -> u8 { self.subrules.iter().map(|s| s.n).max().unwrap_or(1) }
}

/// One subrule for a 2D automaton using threshold counts in a neighborhood.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule2DSubrule {
    pub current_type: CellType,
    pub criteria_type: CellType,
    /// Count of criteria cells required to trigger (>= threshold).
    pub threshold: u32,
    /// Range n >= 1 defines (2n+1)^2 window.
    pub range: u8,
    pub neighborhood: Neighborhood2D,
    /// Optional randomness in [0,1]; pass only if random >= value.
    pub randomness: Option<f64>,
    pub output_type: CellType,
}

impl Rule2DSubrule {
    pub fn validate(&self) -> Result<(), RuleError> {
        if self.range < 1 { return Err(RuleError::InvalidRange2D); }
        if let Some(r) = self.randomness { if !(0.0..=1.0).contains(&r) { return Err(RuleError::InvalidRandomness); } }
        Ok(())
    }

    fn within_neighborhood(dx: i32, dy: i32, n: i32, kind: Neighborhood2D) -> bool {
        if dx == 0 && dy == 0 { return false; }
        match kind {
            Neighborhood2D::Moore => dx.abs() <= n && dy.abs() <= n,
            Neighborhood2D::VonNeumann => dx.abs() + dy.abs() <= n,
            Neighborhood2D::Langdon => dx.abs() == dy.abs() && dx.abs() <= n,
        }
    }

    pub fn applies_and_output<F>(&self, center_current: &CellType, mut get_neighbor: F) -> Option<CellType>
    where F: FnMut(i32, i32) -> CellType {
        if center_current != &self.current_type { return None; }
        let n = self.range as i32;
        let mut count = 0u32;
        for dy in -n..=n {
            for dx in -n..=n {
                if !Self::within_neighborhood(dx, dy, n, self.neighborhood) { continue; }
                let t = get_neighbor(dx, dy);
                if t == self.criteria_type { count += 1; }
            }
        }
        if count >= self.threshold {
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
    pub fn validate(&self) -> Result<(), RuleError> { for s in &self.subrules { s.validate()?; } Ok(()) }
    pub fn range_max(&self) -> u8 { self.subrules.iter().map(|s| s.range).max().unwrap_or(1) }
}
