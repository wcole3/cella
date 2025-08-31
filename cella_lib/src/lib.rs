use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const INERT: &str = "Inert";

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellType(pub String);

impl CellType {
    pub fn inert() -> Self { CellType(INERT.to_string()) }
}

impl Default for CellType {
    fn default() -> Self { CellType::inert() }
}

impl fmt::Display for CellType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CellState {
    pub current: CellType,
    pub age_in_state: u32,
    pub history: Vec<CellType>,
    pub history_limit: usize,
}

impl CellState {
    pub fn new(current: CellType, history_limit: usize) -> Self {
        Self { current, age_in_state: 0, history: Vec::new(), history_limit }
    }

    pub fn transition(&mut self, next: &CellType) {
        if &self.current == next {
            self.age_in_state = self.age_in_state.saturating_add(1);
        } else {
            // push previous state to history
            self.history.push(self.current.clone());
            if self.history.len() > self.history_limit {
                let remove_n = self.history.len() - self.history_limit;
                self.history.drain(0..remove_n);
            }
            self.current = next.clone();
            self.age_in_state = 0;
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Neighborhood2D {
    Moore,
    VonNeumann,
    Langdon,
}

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule1DSubrule {
    pub current_type: CellType,
    pub criteria_type: CellType,
    pub wolfram_code: u128,
    pub n: u8,                    // neighborhood radius (>=1)
    pub randomness: Option<f64>,  // in [0,1]
    pub output_type: CellType,
}

impl Rule1DSubrule {
    pub fn validate(&self) -> Result<(), RuleError> {
        if self.n < 1 { return Err(RuleError::InvalidN1D(self.n)); }
        if let Some(r) = self.randomness { if !(0.0..=1.0).contains(&r) { return Err(RuleError::InvalidRandomness) } }
        let b: u32 = 2u32 * self.n as u32 + 1; // window bits
        let patterns: u32 = 1u32 << b; // number of neighborhood patterns = 2^(2n+1)
        // Rule code is a bitmask over all patterns, valid range is [0, 2^patterns)
        if patterns < 128 {
            let max: u128 = 1u128 << patterns;
            if self.wolfram_code >= max {
                return Err(RuleError::InvalidWolframCode(self.wolfram_code, self.n));
            }
        } else {
            // patterns >= 128 implies max code would exceed u128 shift; accept any u128 value
        }
        Ok(())
    }

    fn pattern_index_1d(window: &[bool]) -> usize {
        // window is length 2n+1 with true meaning matches criteria_type
        // left-to-right as bits, with leftmost as most significant
        let mut idx = 0usize;
        for &b in window {
            idx = (idx << 1) | (b as usize);
        }
        idx
    }

    pub fn applies_and_output(&self, center_current: &CellType, neighborhood: &[CellType]) -> Option<CellType> {
        if center_current != &self.current_type { return None; }
        let crit = &self.criteria_type;
        let window: Vec<bool> = neighborhood.iter().map(|t| t == crit).collect();
        let idx = Self::pattern_index_1d(&window);
        let bit = (self.wolfram_code >> idx) & 1u128;
        if bit == 1u128 {
            // randomness
            if let Some(r) = self.randomness {
                let mut rng = rand::thread_rng();
                let v: f64 = rng.r#gen();
                if v < r { // per spec: true if random >= value; we interpret as pass when v >= r
                    return None;
                }
            }
            return Some(self.output_type.clone());
        }
        None
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule1D { pub subrules: Vec<Rule1DSubrule> }

impl Rule1D {
    pub fn validate(&self) -> Result<(), RuleError> { for s in &self.subrules { s.validate()?; } Ok(()) }

    pub fn n_max(&self) -> u8 { self.subrules.iter().map(|s| s.n).max().unwrap_or(1) }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule2DSubrule {
    pub current_type: CellType,
    pub criteria_type: CellType,
    pub threshold: u32,           // count >= threshold
    pub range: u8,                // n >= 1 defines (2n+1)^2 window
    pub neighborhood: Neighborhood2D,
    pub randomness: Option<f64>,  // in [0,1]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule2D { pub subrules: Vec<Rule2DSubrule> }

impl Rule2D {
    pub fn validate(&self) -> Result<(), RuleError> { for s in &self.subrules { s.validate()?; } Ok(()) }

    pub fn range_max(&self) -> u8 { self.subrules.iter().map(|s| s.range).max().unwrap_or(1) }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grid1D {
    pub width: usize,
    pub history_limit: usize,
    pub cells: Vec<CellState>,
    pub step: u64,
    pub rule: Rule1D,
}

impl Grid1D {
    pub fn new(width: usize, history_limit: usize, initial: Vec<CellType>, rule: Rule1D) -> Self {
        assert_eq!(initial.len(), width, "initial types len must equal width");
        let cells = initial.into_iter().map(|t| CellState::new(t, history_limit)).collect();
        Self { width, history_limit, cells, step: 0, rule }
    }

    fn get_type_or_inert(&self, idx: isize) -> CellType {
        if idx < 0 || idx as usize >= self.width { return CellType::inert(); }
        self.cells[idx as usize].current.clone()
    }

    pub fn step(&mut self) {
        let mut next = self.cells.clone();
        for i in 0..self.width {
            let current_type = self.cells[i].current.clone();
            // Build neighborhood window of length 2n+1 for each subrule individually because n may differ
            let mut decided: Option<CellType> = None;
            'sub: for s in &self.rule.subrules {
                if &current_type != &s.current_type { continue; }
                let n = s.n as isize;
                let len = 2 * n + 1;
                let mut window: Vec<CellType> = Vec::with_capacity(len as usize);
                for d in -n..=n {
                    window.push(self.get_type_or_inert(i as isize + d));
                }
                if let Some(out) = s.applies_and_output(&current_type, &window) {
                    decided = Some(out);
                    break 'sub;
                }
            }
            let new_type = decided.unwrap_or_else(CellType::inert);
            next[i].transition(&new_type);
        }
        self.cells = next;
        self.step = self.step.saturating_add(1);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grid2D {
    pub width: usize,
    pub height: usize,
    pub history_limit: usize,
    pub cells: Vec<CellState>, // row-major length width*height
    pub step: u64,
    pub rule: Rule2D,
}

impl Grid2D {
    pub fn new(width: usize, height: usize, history_limit: usize, initial: Vec<CellType>, rule: Rule2D) -> Self {
        assert_eq!(initial.len(), width * height, "initial types len must equal width*height");
        let cells = initial.into_iter().map(|t| CellState::new(t, history_limit)).collect();
        Self { width, height, history_limit, cells, step: 0, rule }
    }

    fn idx(&self, x: isize, y: isize) -> Option<usize> {
        if x < 0 || y < 0 { return None; }
        let (xu, yu) = (x as usize, y as usize);
        if xu >= self.width || yu >= self.height { return None; }
        Some(yu * self.width + xu)
    }

    fn get_type_or_inert(&self, x: isize, y: isize) -> CellType {
        match self.idx(x, y) { Some(i) => self.cells[i].current.clone(), None => CellType::inert() }
    }

    pub fn step(&mut self) {
        let mut next = self.cells.clone();
        for y in 0..self.height {
            for x in 0..self.width {
                let i = y * self.width + x;
                let current_type = self.cells[i].current.clone();
                let mut decided: Option<CellType> = None;
                'sub: for s in &self.rule.subrules {
                    if &current_type != &s.current_type { continue; }
                    let out = s.applies_and_output(&current_type, |dx, dy| {
                        self.get_type_or_inert(x as isize + dx as isize, y as isize + dy as isize)
                    });
                    if let Some(o) = out { decided = Some(o); break 'sub; }
                }
                let new_type = decided.unwrap_or_else(CellType::inert);
                next[i].transition(&new_type);
            }
        }
        self.cells = next;
        self.step = self.step.saturating_add(1);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GridState {
    D1 { width: usize, history_limit: usize, cells: Vec<CellState>, step: u64, rule: Rule1D },
    D2 { width: usize, height: usize, history_limit: usize, cells: Vec<CellState>, step: u64, rule: Rule2D },
}

impl GridState {
    pub fn from_grid1d(g: &Grid1D) -> Self { Self::D1 { width: g.width, history_limit: g.history_limit, cells: g.cells.clone(), step: g.step, rule: g.rule.clone() } }
    pub fn from_grid2d(g: &Grid2D) -> Self { Self::D2 { width: g.width, height: g.height, history_limit: g.history_limit, cells: g.cells.clone(), step: g.step, rule: g.rule.clone() } }
}

impl Grid1D {
    pub fn from_state(state: &GridState) -> Option<Self> {
        match state {
            GridState::D1 { width, history_limit, cells, step, rule } => Some(Self { width: *width, history_limit: *history_limit, cells: cells.clone(), step: *step, rule: rule.clone() }),
            _ => None,
        }
    }
}

impl Grid2D {
    pub fn from_state(state: &GridState) -> Option<Self> {
        match state {
            GridState::D2 { width, height, history_limit, cells, step, rule } => Some(Self { width: *width, height: *height, history_limit: *history_limit, cells: cells.clone(), step: *step, rule: rule.clone() }),
            _ => None,
        }
    }
}

pub fn grid2d_to_json(g: &Grid2D) -> String {
    let state = GridState::from_grid2d(g);
    serde_json::to_string_pretty(&state).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule1d_validation() {
        let s = Rule1DSubrule { current_type: CellType("A".into()), criteria_type: CellType("A".into()), wolfram_code: 30, n: 1, randomness: Some(0.0), output_type: CellType("B".into()) };
        assert!(s.validate().is_ok());
        let bad = Rule1DSubrule { n: 0, ..s.clone() };
        assert_eq!(bad.validate(), Err(RuleError::InvalidN1D(0)));
    }

    #[test]
    fn rule2d_validation() {
        let s = Rule2DSubrule { current_type: CellType("A".into()), criteria_type: CellType("B".into()), threshold: 2, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: CellType("B".into()) };
        assert!(s.validate().is_ok());
        let bad = Rule2DSubrule { range: 0, ..s.clone() };
        assert_eq!(bad.validate(), Err(RuleError::InvalidRange2D));
    }

    #[test]
    fn grid2d_simple_growth() {
        let a = CellType("A".into());
        let b = CellType("B".into());
        // Any A with at least 1 B neighbor becomes B
        let rule = Rule2D { subrules: vec![Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), threshold: 1, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: b.clone() }] };
        let width = 5; let height = 5; let hist = 3;
        let mut init = vec![a.clone(); width*height];
        // seed one B in center
        init[2*width + 2] = b.clone();
        let mut g = Grid2D::new(width, height, hist, init, rule);
        g.step();
        // Center should stay B, neighbors should become B (at least Moore surrounding cells)
        let mut b_count = 0;
        for c in &g.cells { if c.current == b { b_count += 1; } }
        assert!(b_count > 1);
    }

    #[test]
    fn grid1d_wolfram_smoke() {
        let x = CellType("X".into());
        let y = CellType("Y".into());
        let sub = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 1u128 << 2, n: 1, randomness: None, output_type: y.clone() };
        let rule = Rule1D { subrules: vec![sub] };
        let init = vec![CellType::inert(), x.clone(), CellType::inert()];
        let mut g = Grid1D::new(3, 3, init, rule);
        g.step();
        assert_eq!(g.cells[1].current, y);
    }
}
