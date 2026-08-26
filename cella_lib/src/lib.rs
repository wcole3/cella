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

mod chunking;
pub mod config;
pub mod external;
pub mod grid1d;
pub mod grid2d;
pub mod rules;
pub mod state;
pub mod threads;
pub mod types;
pub mod wildfire;

pub use external::{ChunkCtx, ExternalModel, GridView, ModelError, ModelEvent};
pub use grid1d::Grid1D;
pub use grid2d::Grid2D;
pub use rules::{
    CountOp, Neighborhood2D, Rule1D, Rule1DSubrule, Rule2D, Rule2DSubrule, RuleError,
    neighborhood_contains,
};
pub use state::{GridState, grid2d_to_json};
// Re-exports for ergonomic public API
pub use types::{CellState, CellType, INACTIVE};
pub use wildfire::{FuelClass, SpottingParams, WildfireEnv, WildfireModel, WildfireParams};

#[cfg(test)]
mod tests {
    use super::*;

    /// The dominant type is the one *skipped* during per-cell counting and
    /// back-filled by subtraction, so it has to be re-elected whenever another
    /// type takes the majority — otherwise the skip stops saving anything and
    /// every cell of the true majority is counted one by one.
    ///
    /// A crate-internal test because `dominant_type` is not public: population
    /// counts stay correct either way, so this can only be observed from inside.
    #[test]
    fn dominant_type_is_re_elected_when_majority_flips() {
        let alive = CellType::from("Alive");
        let inactive = CellType::inactive();
        let rule = Rule2D {
            subrules: vec![
                Rule2DSubrule::new(
                    alive,
                    alive,
                    0,
                    CountOp::Gt,
                    1,
                    Neighborhood2D::Moore,
                    alive,
                    None,
                    None,
                ),
                Rule2DSubrule::new(
                    inactive,
                    alive,
                    1,
                    CountOp::Gt,
                    1,
                    Neighborhood2D::Moore,
                    alive,
                    None,
                    None,
                ),
            ],
        };
        let (w, h) = (33usize, 33usize);
        let mut init = vec![inactive; w * h];
        init[(h / 2) * w + w / 2] = alive;
        let mut g = Grid2D::new(w, h, 2, init, rule);
        assert_eq!(g.dominant_type, inactive, "Inactive starts as the majority");

        for _ in 0..20 {
            g.step();
        }
        assert_eq!(
            g.counts_current.get(&alive.0).copied().unwrap_or(0),
            (w * h) as u64,
            "the flood should have filled the grid"
        );
        assert_eq!(
            g.dominant_type, alive,
            "dominant_type must follow the majority, else the counting skip is wasted"
        );
    }

    /// Same requirement for the 1D stepper.
    #[test]
    fn dominant_type_is_re_elected_when_majority_flips_1d() {
        let x = CellType::from("X");
        let inactive = CellType::inactive();
        let rule = Rule1D {
            subrules: vec![
                Rule1DSubrule {
                    current_type: x,
                    criteria_type: x,
                    wolfram_code: u128::MAX,
                    n: 1,
                    randomness: None,
                    output_type: x,
                },
                Rule1DSubrule {
                    current_type: inactive,
                    criteria_type: x,
                    wolfram_code: u128::MAX,
                    n: 1,
                    randomness: None,
                    output_type: x,
                },
            ],
        };
        let width = 65usize;
        let mut init = vec![inactive; width];
        init[width / 2] = x;
        let mut g = Grid1D::new(width, 2, init, rule);
        assert_eq!(g.dominant_type, inactive, "Inactive starts as the majority");

        for _ in 0..width {
            g.step();
        }
        assert_eq!(
            g.counts_current.get(&x.0).copied().unwrap_or(0),
            width as u64
        );
        assert_eq!(
            g.dominant_type, x,
            "dominant_type must follow the majority, else the counting skip is wasted"
        );
    }

    #[test]
    fn rule1d_validation() {
        let s = Rule1DSubrule {
            current_type: CellType::from("A"),
            criteria_type: CellType::from("A"),
            wolfram_code: 30,
            n: 1,
            randomness: Some(0.0),
            output_type: CellType::from("B"),
        };
        assert!(s.validate().is_ok());
        let bad = Rule1DSubrule { n: 0, ..s.clone() };
        assert_eq!(bad.validate(), Err(RuleError::InvalidN1D(0)));
    }

    #[test]
    fn rule2d_validation() {
        let s = Rule2DSubrule::new(
            CellType::from("A"),
            CellType::from("B"),
            2,
            CountOp::Gt,
            1,
            Neighborhood2D::Moore,
            CellType::from("B"),
            None,
            None,
        );
        assert!(s.validate().is_ok());
        let bad = Rule2DSubrule {
            range: 0,
            ..s.clone()
        };
        assert_eq!(bad.validate(), Err(RuleError::InvalidRange2D));
    }

    #[test]
    fn grid2d_simple_growth() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        // Any A with at least 1 B neighbor becomes B
        let rule = Rule2D {
            subrules: vec![Rule2DSubrule::new(
                a.clone(),
                b.clone(),
                1,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                b.clone(),
                None,
                None,
            )],
        };
        let width = 5;
        let height = 5;
        let hist = 3;
        let mut init = vec![a.clone(); width * height];
        // seed one B in center
        init[2 * width + 2] = b.clone();
        let mut g = Grid2D::new(width, height, hist, init, rule);
        g.step();
        // Center should stay B, neighbors should become B (at least Moore surrounding cells)
        let mut b_count = 0;
        for c in &g.cells {
            if *c == b {
                b_count += 1;
            }
        }
        assert!(b_count > 1);
    }

    #[test]
    fn grid1d_wolfram_smoke() {
        let x = CellType::from("X");
        let y = CellType::from("Y");
        let sub = Rule1DSubrule {
            current_type: x.clone(),
            criteria_type: x.clone(),
            wolfram_code: 1u128 << 2,
            n: 1,
            randomness: None,
            output_type: y.clone(),
        };
        let rule = Rule1D {
            subrules: vec![sub],
        };
        let init = vec![CellType::inactive(), x.clone(), CellType::inactive()];
        let mut g = Grid1D::new(3, 3, init, rule);
        g.step();
        assert_eq!(g.cell_type(1), y);
    }

    #[test]
    fn two_d_von_neumann_neighbors() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let rule = Rule2D {
            subrules: vec![Rule2DSubrule::new(
                a.clone(),
                b.clone(),
                2,
                CountOp::Gt,
                1,
                Neighborhood2D::VonNeumann,
                b.clone(),
                None,
                None,
            )],
        };
        let w = 3;
        let h = 3;
        let hist = 2;
        let mut init = vec![a.clone(); w * h];
        // place B at (1,0) and (0,1) around center (1,1) -> two cardinal neighbors
        init[0 * w + 1] = b.clone();
        init[1 * w + 0] = b.clone();
        let mut g = Grid2D::new(w, h, hist, init, rule);
        g.step();
        assert_eq!(g.cell_type(1 * w + 1), b);
    }

    #[test]
    fn two_d_langton_diagonals() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let rule = Rule2D {
            subrules: vec![Rule2DSubrule::new(
                a.clone(),
                b.clone(),
                2,
                CountOp::Gt,
                1,
                Neighborhood2D::Langton,
                b.clone(),
                None,
                None,
            )],
        };
        let w = 3;
        let h = 3;
        let hist = 2;
        let mut init = vec![a.clone(); w * h];
        // diagonal neighbors at (0,0) and (2,2) relative to center (1,1)
        init[0 * w + 0] = b.clone();
        init[2 * w + 2] = b.clone();
        let mut g = Grid2D::new(w, h, hist, init, rule);
        g.step();
        assert_eq!(g.cell_type(1 * w + 1), b);
    }

    #[test]
    fn two_d_straightline_cardinals() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        // Need two straight (cardinal) neighbors to trigger
        let rule = Rule2D {
            subrules: vec![Rule2DSubrule::new(
                a.clone(),
                b.clone(),
                2,
                CountOp::Gt,
                1,
                Neighborhood2D::StraightLine,
                b.clone(),
                None,
                None,
            )],
        };
        let w = 3;
        let h = 3;
        let hist = 2;
        let mut init = vec![a.clone(); w * h];
        // cardinal neighbors at (1,0) and (2,1) relative to center (1,1)
        init[0 * w + 1] = b.clone(); // up
        init[1 * w + 2] = b.clone(); // right
        let mut g = Grid2D::new(w, h, hist, init, rule);
        g.step();
        assert_eq!(g.cell_type(1 * w + 1), b);
    }

    #[test]
    fn config_roundtrip_build() {
        use crate::config::{CellaConfig, Config2D};
        let alive = CellType::from("Alive");
        let inactive = CellType::inactive();
        let rule = Rule2D {
            subrules: vec![
                // Overpopulation: Alive with >=4 Alive neighbors becomes Inactive
                Rule2DSubrule::new(
                    alive.clone(),
                    alive.clone(),
                    4,
                    CountOp::Gt,
                    1,
                    Neighborhood2D::Moore,
                    inactive.clone(),
                    None,
                    None,
                ),
                // Survival: Alive stays Alive with >=2 Alive neighbors (checked after overpop)
                Rule2DSubrule::new(
                    alive.clone(),
                    alive.clone(),
                    2,
                    CountOp::Gt,
                    1,
                    Neighborhood2D::Moore,
                    alive.clone(),
                    None,
                    None,
                ),
                // Birth: Inactive becomes Alive with ==3 Alive neighbors
                Rule2DSubrule::new(
                    inactive.clone(),
                    alive.clone(),
                    3,
                    CountOp::Eq,
                    1,
                    Neighborhood2D::Moore,
                    alive.clone(),
                    None,
                    None,
                ),
            ],
        };
        let w = 4;
        let h = 4;
        let hist = 3;
        // TODO consider changing config to Spur Vec
        let mut initial = vec![inactive.as_str().to_string(); w * h];
        initial[1 * w + 1] = alive.as_str().to_string();
        initial[1 * w + 2] = alive.as_str().to_string();
        initial[1 * w + 3.min(w - 1)] = alive.as_str().to_string();
        let cfg = CellaConfig::D2(Config2D {
            width: w,
            height: h,
            history_limit: hist,
            initial,
            rule,
            model: None,
        });
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
    use crate::config::{CellaConfig, Config2D};

    #[test]
    fn grid1d_n2_pattern() {
        let x = CellType::from("X");
        let y = CellType::from("Y");
        // For n=2, window len=5. Pattern [0,0,1,0,0] -> idx = 4
        let code: u128 = 1u128 << 4;
        let sub = Rule1DSubrule {
            current_type: x.clone(),
            criteria_type: x.clone(),
            wolfram_code: code,
            n: 2,
            randomness: None,
            output_type: y.clone(),
        };
        let rule = Rule1D {
            subrules: vec![sub],
        };
        let init = vec![
            CellType::inactive(),
            CellType::inactive(),
            x.clone(),
            CellType::inactive(),
            CellType::inactive(),
        ];
        let mut g = Grid1D::new(5, 3, init, rule);
        g.step();
        assert_eq!(g.cell_type(2), y);
    }

    #[test]
    fn knight_range1_classic_squares() {
        // All 8 classic knight squares must be reachable at range=1
        let squares = [
            (1, 2),
            (1, -2),
            (-1, 2),
            (-1, -2),
            (2, 1),
            (2, -1),
            (-2, 1),
            (-2, -1),
        ];
        for (dx, dy) in squares {
            assert!(
                neighborhood_contains(dx, dy, 1, Neighborhood2D::Knight),
                "({},{}) should be a knight neighbor at range=1",
                dx,
                dy
            );
        }
        // Adjacent cells are NOT knight neighbors
        for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1), (1, 1)] {
            assert!(
                !neighborhood_contains(dx, dy, 1, Neighborhood2D::Knight),
                "({},{}) should NOT be a knight neighbor at range=1",
                dx,
                dy
            );
        }
    }

    #[test]
    fn knight_origin_always_excluded() {
        for range in [1i32, 2, 3] {
            assert!(
                !neighborhood_contains(0, 0, range, Neighborhood2D::Knight),
                "(0,0) must be excluded at range={}",
                range
            );
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
        assert!(
            neighborhood_contains(0, 4, 2, Neighborhood2D::Knight),
            "(0,4) should be reachable in 2 knight hops"
        );
        assert!(
            neighborhood_contains(4, 0, 2, Neighborhood2D::Knight),
            "(4,0) should be reachable in 2 knight hops"
        );
        // range=1 cells still included at range=2
        assert!(
            neighborhood_contains(1, 2, 2, Neighborhood2D::Knight),
            "(1,2) should be reachable at range=2"
        );
    }

    #[test]
    fn knight_serde_roundtrip() {
        let sub = Rule2DSubrule::new(
            CellType::from("A"),
            CellType::from("B"),
            2,
            CountOp::Eq,
            1,
            Neighborhood2D::Knight,
            CellType::from("A"),
            None,
            None,
        );
        let json = serde_json::to_string(&sub).unwrap();
        assert!(
            json.contains("\"Knight\""),
            "serialised JSON must contain \"Knight\""
        );
        let sub2: Rule2DSubrule = serde_json::from_str(&json).unwrap();
        assert_eq!(sub2.neighborhood, Neighborhood2D::Knight);
    }

    #[test]
    fn knight_grid_step() {
        // 7x7 grid; center (3,3) is A; all 8 knight squares are B.
        // Rule: A with >=1 B knight-neighbor becomes B.
        let a = CellType::from("A");
        let b = CellType::from("B");
        let rule = Rule2D {
            subrules: vec![Rule2DSubrule::new(
                a.clone(),
                b.clone(),
                1,
                CountOp::Gt,
                1,
                Neighborhood2D::Knight,
                b.clone(),
                None,
                None,
            )],
        };
        let w = 7usize;
        let h = 7usize;
        let mut init = vec![a.clone(); w * h];
        // Place B at all 8 classic knight squares around center (3,3)
        for (dx, dy) in [
            (1i32, 2),
            (1, -2),
            (-1, 2),
            (-1, -2),
            (2, 1),
            (2, -1),
            (-2, 1),
            (-2, -1),
        ] {
            let cx = (3 + dx) as usize;
            let cy = (3 + dy) as usize;
            init[cy * w + cx] = b.clone();
        }
        let mut g = Grid2D::new(w, h, 2, init, rule);
        g.step();
        // Center (3,3) had 8 B knight-neighbors, so it should become B
        assert_eq!(
            g.cell_type(3 * w + 3),
            b,
            "center should become B after step"
        );
    }

    #[test]
    fn knight_stress() {
        // --- 1. Symmetry: neighborhood_contains must be symmetric in all 8 reflections ---
        for range in 1i32..=3 {
            for dy in -(range * 2)..=(range * 2) {
                for dx in -(range * 2)..=(range * 2) {
                    let v = neighborhood_contains(dx, dy, range, Neighborhood2D::Knight);
                    // All 8 reflections must agree
                    for (sx, sy) in [(-1i32, 1), (-1, -1), (1, -1)] {
                        assert_eq!(
                            neighborhood_contains(dx * sx, dy * sy, range, Neighborhood2D::Knight),
                            v
                        );
                    }
                    // Transpose symmetry: (dx,dy) reachable iff (dy,dx) reachable
                    assert_eq!(
                        neighborhood_contains(dy, dx, range, Neighborhood2D::Knight),
                        v,
                        "transpose symmetry failure at ({},{}) range={}",
                        dx,
                        dy,
                        range
                    );
                }
            }
        }

        // --- 2. Exact neighbor counts for range=1 and range=2 ---
        // range=1: exactly 8 cells
        let count_r1 = (-4..=4i32)
            .flat_map(|dy| (-4..=4i32).map(move |dx| (dx, dy)))
            .filter(|&(dx, dy)| neighborhood_contains(dx, dy, 1, Neighborhood2D::Knight))
            .count();
        assert_eq!(
            count_r1, 8,
            "range=1 must have exactly 8 knight neighbors, got {}",
            count_r1
        );

        // range=2 unions all cells reachable in 1 or 2 hops; must be strictly more than 8.
        let count_r2 = (-8..=8i32)
            .flat_map(|dy| (-8..=8i32).map(move |dx| (dx, dy)))
            .filter(|&(dx, dy)| neighborhood_contains(dx, dy, 2, Neighborhood2D::Knight))
            .count();
        assert!(
            count_r2 > 8,
            "range=2 must include more than 8 cells, got {}",
            count_r2
        );
        // Spot-check known 2-hop cells:
        // (0,4): (0,0)->(1,2)->(0,4) ✓   (4,0): (0,0)->(2,1)->(4,0) ✓
        // (3,3): (0,0)->(1,2)->(3,3) ✓   (3,-3): (0,0)->(1,-2)->(3,-3) ✓
        for (dx, dy) in [
            (0i32, 4),
            (4, 0),
            (0, -4),
            (-4, 0),
            (3, 3),
            (3, -3),
            (-3, 3),
            (-3, -3),
        ] {
            assert!(
                neighborhood_contains(dx, dy, 2, Neighborhood2D::Knight),
                "({},{}) must be reachable in 2 knight hops",
                dx,
                dy
            );
        }
        // Spot-check cells NOT reachable in 2 hops:
        // (1,0): two knight moves cannot sum to (1,0) — verified by exhaustion.
        // (5,5): minimum hops from origin is 4, so unreachable in <=2.
        for (dx, dy) in [(1i32, 0), (0, 1), (-1, 0), (0, -1), (5, 5)] {
            assert!(
                !neighborhood_contains(dx, dy, 2, Neighborhood2D::Knight),
                "({},{}) must NOT be reachable in 2 knight hops",
                dx,
                dy
            );
        }

        // --- 3. Multi-step grid stress: 20x20 grid, 10 steps, Knight rule, no panic ---
        let a = CellType::from("A");
        let b = CellType::from("B");
        let rule = Rule2D {
            subrules: vec![
                // Survival: A with 2..=4 B knight-neighbors stays A
                Rule2DSubrule::new(
                    a.clone(),
                    b.clone(),
                    2,
                    CountOp::Gt,
                    1,
                    Neighborhood2D::Knight,
                    a.clone(),
                    None,
                    Some(4),
                ),
                // Birth: B with exactly 3 A knight-neighbors becomes A
                Rule2DSubrule::new(
                    b.clone(),
                    a.clone(),
                    3,
                    CountOp::Eq,
                    1,
                    Neighborhood2D::Knight,
                    a.clone(),
                    None,
                    None,
                ),
            ],
        };
        let w = 20usize;
        let h = 20usize;
        // Seed a glider-like pattern near center
        let mut init = vec![b.clone(); w * h];
        for (cx, cy) in [(10usize, 10), (11, 12), (9, 12), (10, 8), (12, 9)] {
            init[cy * w + cx] = a.clone();
        }
        let mut g = Grid2D::new(w, h, 3, init, rule);
        for _ in 0..10 {
            g.step();
        }
        assert_eq!(g.step, 10, "grid must have advanced exactly 10 steps");
        // Grid must still have valid dimensions
        assert_eq!(g.cells.len(), w * h);

        // --- 4. range=3 reachability: (0,0) always excluded, known 3-hop cells included ---
        assert!(!neighborhood_contains(0, 0, 3, Neighborhood2D::Knight));
        // (3,3) is reachable in 3 hops: (0,0)->(1,2)->(2,4)->(3,3)? (2,4)+(1,-1) not knight.
        // (0,0)->(2,1)->(1,3)->(3,4)? No. (0,0)->(1,2)->(3,3)? (1,2)+(2,1)=(3,3). Yes! 2 hops.
        assert!(
            neighborhood_contains(3, 3, 2, Neighborhood2D::Knight),
            "(3,3) should be reachable in 2 hops via (0,0)->(1,2)->(3,3)"
        );
        // (0,6) reachable in 3 hops: (0,0)->(1,2)->(0,4)->(1,6)? No. (0,0)->(1,2)->(2,4)->(0,5)? No.
        // (0,0)->(2,1)->(0,2)->(1,4)? No. (0,0)->(1,2)->(0,4)->(2,5)? No.
        // (0,0)->(2,1)->(1,3)->(0,5)? No. (0,0)->(1,2)->(2,4)->(1,6)? (2,4)+(−1,2)=(1,6)≠(0,6).
        // (0,0)->(2,1)->(0,2)->(2,3)? No. (0,0)->(1,2)->(0,4)->(−1,6)? No.
        // (0,6): (0,0)->(2,1)->(1,3)->(2,5)? No. (0,0)->(1,2)->(2,4)->(0,5)? No.
        // (0,6): (0,0)->(2,1)->(0,2)->(1,4)? No. Let's just verify range=3 count > range=2 count.
        let count_r3 = (-12..=12i32)
            .flat_map(|dy| (-12..=12i32).map(move |dx| (dx, dy)))
            .filter(|&(dx, dy)| neighborhood_contains(dx, dy, 3, Neighborhood2D::Knight))
            .count();
        assert!(
            count_r3 > count_r2,
            "range=3 must cover more cells than range=2: got r3={} r2={}",
            count_r3,
            count_r2
        );
    }

    #[test]
    fn randomness_bounds() {
        let x = CellType::from("X");
        // 1D invalid randomness
        let bad1 = Rule1DSubrule {
            current_type: x.clone(),
            criteria_type: x.clone(),
            wolfram_code: 1,
            n: 1,
            randomness: Some(1.5),
            output_type: x.clone(),
        };
        assert_eq!(bad1.validate(), Err(RuleError::InvalidRandomness));
        // 2D invalid randomness
        let bad2 = Rule2DSubrule::new(
            x.clone(),
            x.clone(),
            1,
            CountOp::Gt,
            1,
            Neighborhood2D::Moore,
            x.clone(),
            Some(-0.1),
            None,
        );
        assert_eq!(bad2.validate(), Err(RuleError::InvalidRandomness));
    }

    /// trivial test to print out all our struct memory packing
    #[test]
    #[ignore = "just for reference, not a test"]
    fn print_struct_sizes() {
        // CellType
        println!("CellType: {}", size_of::<CellType>());
        // CellState
        println!("CellState: {}", size_of::<CellState>());
        // Rule1D
        println!("Rule1D: {}", size_of::<Rule1D>());
        // SubRule1D
        println!("Rule1dSubRule: {}", size_of::<Rule1DSubrule>());
        // Rule2D
        println!("Rule2D: {}", size_of::<Rule2D>());
        // Rule2DSubRule
        println!("Rule2dSubRule: {}", size_of::<Rule2DSubrule>());
    }

    #[test]
    fn serde_rule1d_roundtrip_multistate() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let c = CellType::from("C");
        // n=1, match any window by setting all 8 bits
        let any = 0xFFu128;
        let rule = Rule1D {
            subrules: vec![
                Rule1DSubrule {
                    current_type: a.clone(),
                    criteria_type: a.clone(),
                    wolfram_code: any,
                    n: 1,
                    randomness: None,
                    output_type: b.clone(),
                },
                Rule1DSubrule {
                    current_type: b.clone(),
                    criteria_type: b.clone(),
                    wolfram_code: any,
                    n: 1,
                    randomness: None,
                    output_type: c.clone(),
                },
                Rule1DSubrule {
                    current_type: c.clone(),
                    criteria_type: c.clone(),
                    wolfram_code: any,
                    n: 1,
                    randomness: None,
                    output_type: a.clone(),
                },
            ],
        };
        let v1 = serde_json::to_value(&rule).unwrap();
        let rule2: Rule1D = serde_json::from_value(v1.clone()).unwrap();
        let v2 = serde_json::to_value(&rule2).unwrap();
        assert_eq!(
            v1, v2,
            "Rule1D JSON value should be stable across roundtrip"
        );
    }

    #[test]
    fn serde_rule2d_roundtrip_multistate() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let c = CellType::from("C");
        // Transition when at least 0 neighbors (always true). This exercises serde, not behavior.
        let rule = Rule2D {
            subrules: vec![
                Rule2DSubrule::new(
                    a.clone(),
                    b.clone(),
                    0,
                    CountOp::Gt,
                    1,
                    Neighborhood2D::Moore,
                    b.clone(),
                    None,
                    None,
                ),
                Rule2DSubrule::new(
                    b.clone(),
                    c.clone(),
                    0,
                    CountOp::Gt,
                    1,
                    Neighborhood2D::Moore,
                    c.clone(),
                    None,
                    None,
                ),
                Rule2DSubrule::new(
                    c.clone(),
                    a.clone(),
                    0,
                    CountOp::Gt,
                    1,
                    Neighborhood2D::Moore,
                    a.clone(),
                    None,
                    None,
                ),
            ],
        };
        let v1 = serde_json::to_value(&rule).unwrap();
        let rule2: Rule2D = serde_json::from_value(v1.clone()).unwrap();
        let v2 = serde_json::to_value(&rule2).unwrap();
        assert_eq!(
            v1, v2,
            "Rule2D JSON value should be stable across roundtrip"
        );
    }

    #[test]
    fn serde_gridstate_roundtrip_1d_three_state() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let c = CellType::from("C");
        let any = 0xFFu128;
        let rule = Rule1D {
            subrules: vec![
                Rule1DSubrule {
                    current_type: a.clone(),
                    criteria_type: a.clone(),
                    wolfram_code: any,
                    n: 1,
                    randomness: None,
                    output_type: b.clone(),
                },
                Rule1DSubrule {
                    current_type: b.clone(),
                    criteria_type: b.clone(),
                    wolfram_code: any,
                    n: 1,
                    randomness: None,
                    output_type: c.clone(),
                },
                Rule1DSubrule {
                    current_type: c.clone(),
                    criteria_type: c.clone(),
                    wolfram_code: any,
                    n: 1,
                    randomness: None,
                    output_type: a.clone(),
                },
            ],
        };
        let width = 9usize;
        let hist = 3usize;
        let init = (0..width)
            .map(|i| match i % 3 {
                0 => a.clone(),
                1 => b.clone(),
                _ => c.clone(),
            })
            .collect::<Vec<_>>();
        let mut g = Grid1D::new(width, hist, init, rule);
        g.step();
        let st = GridState::from_grid1d(&g);
        let json = st.to_json();
        let st2 = GridState::from_json(&json).unwrap();
        let g2 = Grid1D::from_state(&st2).unwrap();
        assert_eq!(g2.width, g.width);
        assert_eq!(g2.step, g.step);
        for i in 0..width {
            assert_eq!(g2.cell_type(i), g.cell_type(i));
        }
        // test cells and next_cells after import
        for i in 0..width {
            assert_eq!(g2.cells[i], g.cells[i]);
        }
        for i in 0..width {
            assert_eq!(g2.next_cells[i], CellType::inactive());
        }
    }

    #[test]
    fn serde_gridstate_roundtrip_2d_three_state() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let c = CellType::from("C");
        let rule = Rule2D {
            subrules: vec![
                // Rotate states based on always-true threshold (count>=0)
                Rule2DSubrule::new(
                    a.clone(),
                    b.clone(),
                    0,
                    CountOp::Gt,
                    1,
                    Neighborhood2D::Moore,
                    b.clone(),
                    None,
                    None,
                ),
                Rule2DSubrule::new(
                    b.clone(),
                    c.clone(),
                    0,
                    CountOp::Gt,
                    1,
                    Neighborhood2D::Moore,
                    c.clone(),
                    None,
                    None,
                ),
                Rule2DSubrule::new(
                    c.clone(),
                    a.clone(),
                    0,
                    CountOp::Gt,
                    1,
                    Neighborhood2D::Moore,
                    a.clone(),
                    None,
                    None,
                ),
            ],
        };
        let (w, h, hist) = (6usize, 4usize, 2usize);
        let mut init = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let idx = (x + y) % 3;
                init.push(match idx {
                    0 => a.clone(),
                    1 => b.clone(),
                    _ => c.clone(),
                });
            }
        }
        let mut g = Grid2D::new(w, h, hist, init, rule);
        g.step();
        let st = GridState::from_grid2d(&g);
        let json = st.to_json();
        let st2 = GridState::from_json(&json).unwrap();
        let g2 = Grid2D::from_state(&st2).unwrap();
        assert_eq!(g2.width, g.width);
        assert_eq!(g2.height, g.height);
        assert_eq!(g2.step, g.step);
        for i in 0..(w * h) {
            assert_eq!(g2.cell_type(i), g.cell_type(i));
        }
        // repeat for cells
        for i in 0..(w * h) {
            assert_eq!(g2.cells[i], g.cells[i]);
        }
        for i in 0..(w * h) {
            assert_eq!(g2.next_cells[i], CellType::inactive());
        }
    }

    #[test]
    fn serde_config_roundtrip_multistate_2d() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let c = CellType::from("C");
        let (w, h, hist) = (5usize, 4usize, 3usize);
        let init = (0..w * h)
            .map(|i| match i % 3 {
                0 => a.as_str().to_string(),
                1 => b.as_str().to_string(),
                _ => c.as_str().to_string(),
            })
            .collect::<Vec<_>>();
        let rule = Rule2D {
            subrules: vec![
                Rule2DSubrule::new(
                    a.clone(),
                    b.clone(),
                    2,
                    CountOp::Gt,
                    1,
                    Neighborhood2D::VonNeumann,
                    b.clone(),
                    None,
                    None,
                ),
                Rule2DSubrule::new(
                    b.clone(),
                    c.clone(),
                    2,
                    CountOp::Gt,
                    1,
                    Neighborhood2D::VonNeumann,
                    c.clone(),
                    None,
                    None,
                ),
                Rule2DSubrule::new(
                    c.clone(),
                    a.clone(),
                    2,
                    CountOp::Gt,
                    1,
                    Neighborhood2D::VonNeumann,
                    a.clone(),
                    None,
                    None,
                ),
            ],
        };
        let cfg = CellaConfig::D2(Config2D {
            width: w,
            height: h,
            history_limit: hist,
            initial: init,
            rule: rule.clone(),
            model: None,
        });
        let s = serde_json::to_string(&cfg).unwrap();
        let c2: Config2D = serde_json::from_str(&s).unwrap();
        assert_eq!(c2.width, w);
        assert_eq!(c2.height, h);
        assert_eq!(c2.history_limit, hist);
        assert_eq!(c2.initial.len(), w * h);
        assert_eq!(c2.rule.subrules.len(), rule.subrules.len());
        // check that the rules are eq
        for (i, r) in c2.rule.subrules.iter().enumerate() {
            assert_eq!(
                *r,
                *rule
                    .subrules
                    .get(i)
                    .expect("rule subrules should deserialize")
            );
        }
        let built = CellaConfig::D2(c2.clone()).build_grid2d();
        assert!(built.is_some());
    }

    #[test]
    fn multistate_1d_rotation_behavior_4_states() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let c = CellType::from("C");
        let d = CellType::from("D");
        let any = 0xFFu128;
        let rule = Rule1D {
            subrules: vec![
                Rule1DSubrule {
                    current_type: a.clone(),
                    criteria_type: a.clone(),
                    wolfram_code: any,
                    n: 1,
                    randomness: None,
                    output_type: b.clone(),
                },
                Rule1DSubrule {
                    current_type: b.clone(),
                    criteria_type: b.clone(),
                    wolfram_code: any,
                    n: 1,
                    randomness: None,
                    output_type: c.clone(),
                },
                Rule1DSubrule {
                    current_type: c.clone(),
                    criteria_type: c.clone(),
                    wolfram_code: any,
                    n: 1,
                    randomness: None,
                    output_type: d.clone(),
                },
                Rule1DSubrule {
                    current_type: d.clone(),
                    criteria_type: d.clone(),
                    wolfram_code: any,
                    n: 1,
                    randomness: None,
                    output_type: a.clone(),
                },
            ],
        };
        let width = 12usize;
        let hist = 2usize;
        let init = (0..width)
            .map(|i| match i % 4 {
                0 => a.clone(),
                1 => b.clone(),
                2 => c.clone(),
                _ => d.clone(),
            })
            .collect::<Vec<_>>();
        let mut g = Grid1D::new(width, hist, init, rule);
        g.step();
        for i in 0..width {
            let exp = match i % 4 {
                0 => b.clone(),
                1 => c.clone(),
                2 => d.clone(),
                _ => a.clone(),
            };
            assert_eq!(g.cell_type(i), exp);
            assert_eq!(g.cells[i], exp);
        }
    }
}
