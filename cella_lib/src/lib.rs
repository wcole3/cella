//! cella: a minimal cellular automata library supporting 1D and 2D grids.
//!
//! This crate exposes composable rules and serializable grid state. Stepping
//! is double-buffered and can run in parallel depending on a simple property
//! file in your repository root:
//!
//!   cella.properties
//!     threads=4
//!
//! The number after `threads=` controls how many worker threads are used to
//! compute each step. If the file or key is missing, the engine defaults to
//! `std::thread::available_parallelism()` (or 1 on error). See [`threads`] for
//! details.
//!
//! Quick start:
//! - Define rules (1D Wolfram-style or 2D threshold neighborhoods)
//! - Create a Grid1D or Grid2D with initial CellType values
//! - Call step() repeatedly; serialize via GridState.

pub mod types;
pub mod rules;
pub mod grid1d;
pub mod grid2d;
pub mod state;
pub mod config;
pub mod threads;

// Re-exports for ergonomic public API
pub use types::{INACTIVE, CellType, CellState};
pub use rules::{Neighborhood2D, RuleError, Rule1D, Rule1DSubrule, Rule2D, Rule2DSubrule};
pub use grid1d::Grid1D;
pub use grid2d::Grid2D;
pub use state::{GridState, grid2d_to_json};

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
        let init = vec![CellType::inactive(), x.clone(), CellType::inactive()];
        let mut g = Grid1D::new(3, 3, init, rule);
        g.step();
        assert_eq!(g.cells[1].current, y);
    }

    #[test]
    fn two_d_von_neumann_neighbors() {
        let a = CellType("A".into());
        let b = CellType("B".into());
        let rule = Rule2D { subrules: vec![Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), threshold: 2, range: 1, neighborhood: Neighborhood2D::VonNeumann, randomness: None, output_type: b.clone() }] };
        let w=3; let h=3; let hist=2;
        let mut init = vec![a.clone(); w*h];
        // place B at (1,0) and (0,1) around center (1,1) -> two cardinal neighbors
        init[0*w + 1] = b.clone();
        init[1*w + 0] = b.clone();
        let mut g = Grid2D::new(w,h,hist,init,rule);
        g.step();
        assert_eq!(g.cells[1*w + 1].current, b);
    }

    #[test]
    fn two_d_langdon_diagonals() {
        let a = CellType("A".into());
        let b = CellType("B".into());
        let rule = Rule2D { subrules: vec![Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), threshold: 2, range: 1, neighborhood: Neighborhood2D::Langdon, randomness: None, output_type: b.clone() }] };
        let w=3; let h=3; let hist=2;
        let mut init = vec![a.clone(); w*h];
        // diagonal neighbors at (0,0) and (2,2) relative to center (1,1)
        init[0*w + 0] = b.clone();
        init[2*w + 2] = b.clone();
        let mut g = Grid2D::new(w,h,hist,init,rule);
        g.step();
        assert_eq!(g.cells[1*w + 1].current, b);
    }

    #[test]
    fn config_roundtrip_build() {
        use crate::config::{CellaConfig, Config2D};
        let alive = CellType("Alive".into());
        let inactive = CellType::inactive();
        let rule = Rule2D { subrules: vec![
            Rule2DSubrule { current_type: alive.clone(), criteria_type: alive.clone(), threshold: 2, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
            Rule2DSubrule { current_type: inactive.clone(), criteria_type: alive.clone(), threshold: 3, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
        ]};
        let w=4; let h=4; let hist=3;
        let mut initial = vec![inactive.0.clone(); w*h];
        initial[1*w + 1] = alive.0.clone();
        initial[1*w + 2] = alive.0.clone();
        initial[1*w + 3.min(w-1)] = alive.0.clone();
        let cfg = CellaConfig::D2(Config2D { width:w, height:h, history_limit:hist, initial, rule });
        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: CellaConfig = serde_json::from_str(&json).unwrap();
        let mut g = cfg2.build_grid2d().unwrap();
        g.step();
        assert!(g.step >= 1);
    }
}


#[cfg(test)]
mod more_tests {
    use super::*;

    #[test]
    fn grid1d_n2_pattern() {
        let x = CellType("X".into());
        let y = CellType("Y".into());
        // For n=2, window len=5. Pattern [0,0,1,0,0] -> idx = 4
        let code: u128 = 1u128 << 4;
        let sub = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: code, n: 2, randomness: None, output_type: y.clone() };
        let rule = Rule1D { subrules: vec![sub] };
        let init = vec![CellType::inactive(), CellType::inactive(), x.clone(), CellType::inactive(), CellType::inactive()];
        let mut g = Grid1D::new(5, 3, init, rule);
        g.step();
        assert_eq!(g.cells[2].current, y);
    }

    #[test]
    fn randomness_bounds() {
        let x = CellType("X".into());
        // 1D invalid randomness
        let bad1 = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 1, n: 1, randomness: Some(1.5), output_type: x.clone() };
        assert_eq!(bad1.validate(), Err(RuleError::InvalidRandomness));
        // 2D invalid randomness
        let bad2 = Rule2DSubrule { current_type: x.clone(), criteria_type: x.clone(), threshold: 1, range: 1, neighborhood: Neighborhood2D::Moore, randomness: Some(-0.1), output_type: x.clone() };
        assert_eq!(bad2.validate(), Err(RuleError::InvalidRandomness));
    }
}
