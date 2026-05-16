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
pub use rules::{Neighborhood2D, RuleError, Rule1D, Rule1DSubrule, Rule2D, Rule2DSubrule, CountOp, neighborhood_contains};
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
        let s = Rule2DSubrule { current_type: CellType("A".into()), criteria_type: CellType("B".into()), count: 2, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: CellType("B".into()) };
        assert!(s.validate().is_ok());
        let bad = Rule2DSubrule { range: 0, ..s.clone() };
        assert_eq!(bad.validate(), Err(RuleError::InvalidRange2D));
    }

    #[test]
    fn grid2d_simple_growth() {
        let a = CellType("A".into());
        let b = CellType("B".into());
        // Any A with at least 1 B neighbor becomes B
        let rule = Rule2D { subrules: vec![Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 1, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: b.clone() }] };
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
        let rule = Rule2D { subrules: vec![Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::VonNeumann, randomness: None, output_type: b.clone() }] };
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
        let rule = Rule2D { subrules: vec![Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Langdon, randomness: None, output_type: b.clone() }] };
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
    fn two_d_straightline_cardinals() {
        let a = CellType("A".into());
        let b = CellType("B".into());
        // Need two straight (cardinal) neighbors to trigger
        let rule = Rule2D { subrules: vec![Rule2DSubrule { current_type: a.clone(), criteria_type: b.clone(), count: 2, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::StraightLine, randomness: None, output_type: b.clone() }] };
        let w=3; let h=3; let hist=2;
        let mut init = vec![a.clone(); w*h];
        // cardinal neighbors at (1,0) and (2,1) relative to center (1,1)
        init[0*w + 1] = b.clone(); // up
        init[1*w + 2] = b.clone(); // right
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
            // Overpopulation: Alive with >=4 Alive neighbors becomes Inactive
            Rule2DSubrule { current_type: alive.clone(), criteria_type: alive.clone(), count: 4, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: inactive.clone() },
            // Survival: Alive stays Alive with >=2 Alive neighbors (checked after overpop)
            Rule2DSubrule { current_type: alive.clone(), criteria_type: alive.clone(), count: 2, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
            // Birth: Inactive becomes Alive with ==3 Alive neighbors
            Rule2DSubrule { current_type: inactive.clone(), criteria_type: alive.clone(), count: 3, op: CountOp::Eq, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
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
    fn knight_range1_classic_squares() {
        // All 8 classic knight squares must be reachable at range=1
        let squares = [(1,2),(1,-2),(-1,2),(-1,-2),(2,1),(2,-1),(-2,1),(-2,-1)];
        for (dx, dy) in squares {
            assert!(neighborhood_contains(dx, dy, 1, Neighborhood2D::Knight),
                "({},{}) should be a knight neighbor at range=1", dx, dy);
        }
        // Adjacent cells are NOT knight neighbors
        for (dx, dy) in [(1,0),(0,1),(-1,0),(0,-1),(1,1)] {
            assert!(!neighborhood_contains(dx, dy, 1, Neighborhood2D::Knight),
                "({},{}) should NOT be a knight neighbor at range=1", dx, dy);
        }
    }

    #[test]
    fn knight_origin_always_excluded() {
        for range in [1i32, 2, 3] {
            assert!(!neighborhood_contains(0, 0, range, Neighborhood2D::Knight),
                "(0,0) must be excluded at range={}", range);
        }
    }

    #[test]
    fn knight_range2_reachable_cells() {
        // (2,2) is reachable in 2 hops: (0,0)->(1,2)->(2,0)? No. (0,0)->(2,1)->(0,2)? No.
        // (0,0)->(1,2)->(2,4)? No. Let's verify: (2,2): hop1=(1,2), hop2=(1,2)+(1,0)? Not a knight move.
        // Actually (0,0)->(2,1)->(1,3)? No. (0,0)->(1,2)->(2,4)? No.
        // (2,2): reachable via (0,0)->(1,2)->(2,0)? (2,0)!=(2,2). Try (0,0)->(2,1)->(0,2)? No.
        // Correct path: (0,0)->(1,2) then (1,2)+(1,0) not knight. (0,0)->(2,1)->(1,3)? (1,3)!=(2,2).
        // (0,0)->(1,2)->(3,1)? (3,1)!=(2,2). (0,0)->(2,1)->(4,2)? No. (0,0)->(1,2)->(2,4)? No.
        // Actually (2,2) needs: from (1,2) add (1,0) - not knight. From (2,1) add (0,1) - not knight.
        // (2,2) is NOT reachable in 2 hops. (0,4) is: (0,0)->(1,2)->(0,4). Yes!
        assert!(neighborhood_contains(0, 4, 2, Neighborhood2D::Knight),
            "(0,4) should be reachable in 2 knight hops");
        assert!(neighborhood_contains(4, 0, 2, Neighborhood2D::Knight),
            "(4,0) should be reachable in 2 knight hops");
        // range=1 cells still included at range=2
        assert!(neighborhood_contains(1, 2, 2, Neighborhood2D::Knight),
            "(1,2) should be reachable at range=2");
    }

    #[test]
    fn knight_serde_roundtrip() {
        let sub = Rule2DSubrule {
            current_type: CellType("A".into()), criteria_type: CellType("B".into()),
            count: 2, op: CountOp::Eq, limit: None, range: 1,
            neighborhood: Neighborhood2D::Knight, randomness: None,
            output_type: CellType("A".into()),
        };
        let json = serde_json::to_string(&sub).unwrap();
        assert!(json.contains("\"Knight\""), "serialised JSON must contain \"Knight\"");
        let sub2: Rule2DSubrule = serde_json::from_str(&json).unwrap();
        assert_eq!(sub2.neighborhood, Neighborhood2D::Knight);
    }

    #[test]
    fn knight_grid_step() {
        // 7x7 grid; center (3,3) is A; all 8 knight squares are B.
        // Rule: A with >=1 B knight-neighbor becomes B.
        let a = CellType("A".into());
        let b = CellType("B".into());
        let rule = Rule2D { subrules: vec![Rule2DSubrule {
            current_type: a.clone(), criteria_type: b.clone(),
            count: 1, op: CountOp::Gt, limit: None, range: 1,
            neighborhood: Neighborhood2D::Knight, randomness: None,
            output_type: b.clone(),
        }]};
        let w = 7usize; let h = 7usize;
        let mut init = vec![a.clone(); w * h];
        // Place B at all 8 classic knight squares around center (3,3)
        for (dx, dy) in [(1i32,2),(1,-2),(-1,2),(-1,-2),(2,1),(2,-1),(-2,1),(-2,-1)] {
            let cx = (3 + dx) as usize;
            let cy = (3 + dy) as usize;
            init[cy * w + cx] = b.clone();
        }
        let mut g = Grid2D::new(w, h, 2, init, rule);
        g.step();
        // Center (3,3) had 8 B knight-neighbors, so it should become B
        assert_eq!(g.cells[3 * w + 3].current, b, "center should become B after step");
    }

    #[test]
    fn randomness_bounds() {
        let x = CellType("X".into());
        // 1D invalid randomness
        let bad1 = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 1, n: 1, randomness: Some(1.5), output_type: x.clone() };
        assert_eq!(bad1.validate(), Err(RuleError::InvalidRandomness));
        // 2D invalid randomness
        let bad2 = Rule2DSubrule { current_type: x.clone(), criteria_type: x.clone(), count: 1, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: Some(-0.1), output_type: x.clone() };
        assert_eq!(bad2.validate(), Err(RuleError::InvalidRandomness));
    }
}
