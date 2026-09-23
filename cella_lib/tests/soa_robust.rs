//! Robust tests for SoA layout, history circular buffer, and parallel consistency.

use cella_lib::{
    CellType, CountOp, Grid1D, Grid2D, GridState, Neighborhood2D, Rule1D, Rule1DSubrule, Rule2D,
    Rule2DSubrule,
};

// ─── 1. History circular buffer correctness ───

#[test]
fn history_fifo_order_all_limits() {
    for &limit in &[1usize, 2, 3, 5, 7] {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let c = CellType::from("C");
        let any = 0xFFu128;
        let rule = Rule1D {
            subrules: vec![
                Rule1DSubrule {
                    current_type: a,
                    criteria_type: a,
                    wolfram_code: any,
                    n: 1,
                    randomness: None,
                    output_type: b,
                },
                Rule1DSubrule {
                    current_type: b,
                    criteria_type: b,
                    wolfram_code: any,
                    n: 1,
                    randomness: None,
                    output_type: c,
                },
                Rule1DSubrule {
                    current_type: c,
                    criteria_type: c,
                    wolfram_code: any,
                    n: 1,
                    randomness: None,
                    output_type: a,
                },
            ],
        };

        let width = 3;
        let mut g = Grid1D::new(width, limit, vec![a, a, a], rule);

        let types_ordered = [a, b, c];
        let total_steps = limit.saturating_sub(1) + limit + 3;

        for step in 1..=total_steps {
            g.step();
            let current = g.cell_type(1);
            let hist = g.cell_history(1);

            let mut exp = Vec::new();
            for s in 1..=step {
                let prev_idx = (s - 1) % 3;
                exp.push(types_ordered[prev_idx]);
            }
            if exp.len() > limit {
                exp = exp[exp.len() - limit..].to_vec();
            }
            assert_eq!(
                hist, exp,
                "limit={} step={} current={:?} hist={:?} expected={:?}",
                limit, step, current, hist, exp
            );
        }
    }
}

// ─── 2. Parallel history consistency ───

#[test]
fn parallel_history_1d_boundary() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    let any = 0xFFu128;
    let rule = Rule1D {
        subrules: vec![
            Rule1DSubrule {
                current_type: a,
                criteria_type: a,
                wolfram_code: any,
                n: 1,
                randomness: None,
                output_type: b,
            },
            Rule1DSubrule {
                current_type: b,
                criteria_type: b,
                wolfram_code: any,
                n: 1,
                randomness: None,
                output_type: c,
            },
            Rule1DSubrule {
                current_type: c,
                criteria_type: c,
                wolfram_code: any,
                n: 1,
                randomness: None,
                output_type: a,
            },
        ],
    };

    let width = 10240;
    let hist_limit = 5;
    let init: Vec<CellType> = (0..width).map(|i| if i % 2 == 0 { a } else { b }).collect();
    let mut g = Grid1D::new(width, hist_limit, init, rule.clone());

    let mut g_ref = Grid1D::new(7, hist_limit, vec![a, b, a, b, a, b, a], rule);

    for step_n in 0..10 {
        g.step();
        g_ref.step();

        for i in 0..7 {
            assert_eq!(
                g.cell_type(i),
                g_ref.cell_type(i),
                "step {} cell {} type mismatch",
                step_n + 1,
                i
            );
        }

        let check_indices: Vec<usize> = vec![0, 1, 8191, 8192, 8193, width - 2, width - 1];

        for idx in &check_indices {
            let hist = g.cell_history(*idx);
            let age = g.cell_age(*idx);
            assert!(
                hist.len() <= hist_limit,
                "step {} idx {} hist len {} > limit {}",
                step_n + 1,
                idx,
                hist.len(),
                hist_limit
            );
            for h in &hist {
                assert!(
                    h == &a || h == &b || h == &c,
                    "step {} idx {} invalid history type {:?}",
                    step_n + 1,
                    idx,
                    h
                );
            }
            assert_eq!(age, 0, "step {} idx {} age should be 0", step_n + 1, idx);
        }
    }
}

#[test]
fn parallel_history_2d_boundary() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    let rule = Rule2D {
        subrules: vec![
            Rule2DSubrule::new(
                a,
                b,
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                b,
                None,
                None,
            ),
            Rule2DSubrule::new(
                b,
                c,
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                c,
                None,
                None,
            ),
            Rule2DSubrule::new(
                c,
                a,
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                a,
                None,
                None,
            ),
        ],
    };

    let (w, h) = (80usize, 80usize);
    let hist_limit = 3;
    let init: Vec<CellType> = (0..(w * h))
        .map(|i| if i % 2 == 0 { a } else { b })
        .collect();
    let mut g = Grid2D::new(w, h, hist_limit, init, rule);

    for step_n in 0..5 {
        g.step();
        let total = w * h;
        let check_indices: Vec<usize> = vec![
            0,
            1,
            w - 1,
            w,
            total - 2,
            total - 1,
            total / 2,
            total / 2 + 1,
        ];

        for idx in &check_indices {
            if *idx >= total {
                continue;
            }
            let ct = g.cell_type(*idx);
            assert!(
                ct == a || ct == b || ct == c,
                "step {} idx {} invalid type {:?}",
                step_n + 1,
                idx,
                ct
            );
            let hist = g.cell_history(*idx);
            assert!(
                hist.len() <= hist_limit,
                "step {} idx {} hist too long",
                step_n + 1,
                idx
            );
            let age = g.cell_age(*idx);
            assert_eq!(age, 0, "step {} idx {} age should be 0", step_n + 1, idx);
        }
    }
}

// ─── 3. SoA serialization roundtrip ───

#[test]
fn soa_serialization_roundtrip_1d() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    let any = 0xFFu128;
    let rule = Rule1D {
        subrules: vec![
            Rule1DSubrule {
                current_type: a,
                criteria_type: a,
                wolfram_code: any,
                n: 1,
                randomness: None,
                output_type: b,
            },
            Rule1DSubrule {
                current_type: b,
                criteria_type: b,
                wolfram_code: any,
                n: 1,
                randomness: None,
                output_type: c,
            },
            Rule1DSubrule {
                current_type: c,
                criteria_type: c,
                wolfram_code: any,
                n: 1,
                randomness: None,
                output_type: a,
            },
        ],
    };

    let width = 64;
    let hist = 5;
    let init: Vec<CellType> = (0..width)
        .map(|i| match i % 3 {
            0 => a,
            1 => b,
            _ => c,
        })
        .collect();
    let mut g = Grid1D::new(width, hist, init, rule);
    let initial = GridState::from_grid1d(&g);

    for _ in 0..(hist + 3) {
        g.step();
    }

    let step_count = g.step;
    let cell_types: Vec<_> = (0..width).map(|i| g.cell_type(i)).collect();
    let cell_ages: Vec<_> = (0..width).map(|i| g.cell_age(i)).collect();
    let cell_hists: Vec<_> = (0..width).map(|i| g.cell_history(i)).collect();

    // Round-trip through a saved config (the new format), not raw GridState
    // JSON, which no longer exists.
    let cfg = cella_lib::config::CellaConfig::save_1d(&initial, &g, Default::default());
    let json = serde_json::to_string(&cfg).unwrap();
    let cfg2: cella_lib::config::CellaConfig = serde_json::from_str(&json).unwrap();

    let mut g2 = cfg2.build_grid1d_resumed().expect("roundtrip should succeed");
    assert_eq!(g2.width, width);
    assert_eq!(g2.step, step_count);
    assert_eq!(g2.history_limit, hist);

    for i in 0..width {
        assert_eq!(
            g2.cell_type(i),
            cell_types[i],
            "cell_type mismatch at {}",
            i
        );
        assert_eq!(g2.cell_age(i), cell_ages[i], "cell_age mismatch at {}", i);
        assert_eq!(
            g2.cell_history(i),
            cell_hists[i],
            "cell_history mismatch at {}",
            i
        );
    }

    g.step();
    g2.step();
    for i in 0..width {
        assert_eq!(
            g2.cell_type(i),
            g.cell_type(i),
            "post-roundtrip step type mismatch at {}",
            i
        );
    }
}

#[test]
fn soa_serialization_roundtrip_2d() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    let rule = Rule2D {
        subrules: vec![
            Rule2DSubrule::new(
                a,
                b,
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                b,
                None,
                None,
            ),
            Rule2DSubrule::new(
                b,
                c,
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                c,
                None,
                None,
            ),
            Rule2DSubrule::new(
                c,
                a,
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                a,
                None,
                None,
            ),
        ],
    };

    let (w, h) = (16usize, 12usize);
    let hist = 4;
    let init: Vec<CellType> = (0..(w * h))
        .map(|i| match i % 3 {
            0 => a,
            1 => b,
            _ => c,
        })
        .collect();
    let mut g = Grid2D::new(w, h, hist, init, rule);
    let initial = GridState::from_grid2d(&g);

    for _ in 0..(hist + 3) {
        g.step();
    }

    let step_count = g.step;
    let total = w * h;
    let cell_types: Vec<_> = (0..total).map(|i| g.cell_type(i)).collect();
    let cell_ages: Vec<_> = (0..total).map(|i| g.cell_age(i)).collect();
    let cell_hists: Vec<_> = (0..total).map(|i| g.cell_history(i)).collect();

    // Round-trip through a saved config (the new format), not raw GridState
    // JSON, which no longer exists.
    let cfg = cella_lib::config::CellaConfig::save_2d(&initial, &g, Default::default());
    let json = serde_json::to_string(&cfg).unwrap();
    let cfg2: cella_lib::config::CellaConfig = serde_json::from_str(&json).unwrap();
    let mut g2 = cfg2.build_grid2d_resumed().expect("roundtrip should succeed");

    assert_eq!(g2.width, w);
    assert_eq!(g2.height, h);
    assert_eq!(g2.step, step_count);
    assert_eq!(g2.history_limit, hist);

    for i in 0..total {
        assert_eq!(g2.cell_type(i), cell_types[i], "type mismatch at {}", i);
        assert_eq!(g2.cell_age(i), cell_ages[i], "age mismatch at {}", i);
        assert_eq!(
            g2.cell_history(i),
            cell_hists[i],
            "history mismatch at {}",
            i
        );
    }

    g.step();
    g2.step();
    for i in 0..total {
        assert_eq!(
            g2.cell_type(i),
            g.cell_type(i),
            "post-roundtrip type mismatch at {}",
            i
        );
    }
}

// ─── 4. history_limit = 0 parallel ───

#[test]
fn zero_history_1d_parallel() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let any = 0xFFu128;
    let rule = Rule1D {
        subrules: vec![
            Rule1DSubrule {
                current_type: a,
                criteria_type: a,
                wolfram_code: any,
                n: 1,
                randomness: None,
                output_type: b,
            },
            Rule1DSubrule {
                current_type: b,
                criteria_type: b,
                wolfram_code: any,
                n: 1,
                randomness: None,
                output_type: a,
            },
        ],
    };

    let width = 10240;
    let init: Vec<CellType> = vec![a; width];
    let mut g = Grid1D::new(width, 0, init, rule);

    for _ in 0..10 {
        g.step();
        for i in [0usize, width / 2, width - 1] {
            let expected = if g.step % 2 == 0 { a } else { b };
            assert_eq!(
                g.cell_type(i),
                expected,
                "cell {} wrong type at step {}",
                i,
                g.step
            );
            assert!(
                g.cell_history(i).is_empty(),
                "cell {} history should be empty",
                i
            );
        }
    }
}

#[test]
fn zero_history_2d_parallel() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let rule = Rule2D {
        subrules: vec![
            Rule2DSubrule::new(
                a,
                b,
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                b,
                None,
                None,
            ),
            Rule2DSubrule::new(
                b,
                a,
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                a,
                None,
                None,
            ),
        ],
    };

    let (w, h) = (80usize, 80usize);
    let total = w * h;
    let init = vec![a; total];
    let mut g = Grid2D::new(w, h, 0, init, rule);

    for _ in 0..10 {
        g.step();
        let expected = if g.step % 2 == 0 { a } else { b };
        for i in [0usize, total / 2, total - 1] {
            assert_eq!(
                g.cell_type(i),
                expected,
                "cell {} wrong at step {}",
                i,
                g.step
            );
            assert!(g.cell_history(i).is_empty());
        }
    }
}

// ─── 5. cell_type() bounds checking ───

#[test]
fn cell_type_bounds_1d() {
    let a = CellType::from("A");
    let any = 0xFFu128;
    let rule = Rule1D {
        subrules: vec![Rule1DSubrule {
            current_type: a,
            criteria_type: a,
            wolfram_code: any,
            n: 1,
            randomness: None,
            output_type: a,
        }],
    };
    let width = 5;
    let inactive = CellType::inactive();
    let g = Grid1D::new(width, 3, vec![a; width], rule);

    for i in 0..width {
        assert_eq!(g.cell_type(i), a);
    }
    assert_eq!(
        g.cell_type(width),
        inactive,
        "idx == width should be inactive"
    );
    assert_eq!(
        g.cell_type(width + 100),
        inactive,
        "idx >> width should be inactive"
    );
    assert_eq!(
        g.cell_type(usize::MAX),
        inactive,
        "usize::MAX should be inactive"
    );

    assert_eq!(g.cell_age(width), 0);
    assert_eq!(g.cell_age(usize::MAX), 0);
}

#[test]
fn cell_type_bounds_2d() {
    let a = CellType::from("A");
    let rule = Rule2D {
        subrules: vec![Rule2DSubrule::new(
            a,
            a,
            0,
            CountOp::Gt,
            1,
            Neighborhood2D::Moore,
            a,
            None,
            None,
        )],
    };
    let (w, h) = (5usize, 4usize);
    let total = w * h;
    let inactive = CellType::inactive();
    let g = Grid2D::new(w, h, 3, vec![a; total], rule);

    for i in 0..total {
        assert_eq!(g.cell_type(i), a);
    }
    assert_eq!(g.cell_type(total), inactive);
    assert_eq!(g.cell_type(usize::MAX), inactive);
    assert_eq!(g.cell_age(total), 0);
}

// ─── 6. Multi-subrule 1D with mixed n values ───

#[test]
fn multi_subrule_mixed_n_1d() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    let d = CellType::from("D");
    let inactive = CellType::inactive();

    // n=1: only matches [A,A,A] (wolfram idx=7=0b111) -> B
    let sr_n1 = Rule1DSubrule {
        current_type: a,
        criteria_type: a,
        wolfram_code: 1u128 << 7,
        n: 1,
        randomness: None,
        output_type: b,
    };

    // n=2: only matches [I,I,A,I,I] (wolfram idx=4=0b00100) -> C
    // n=1 sub-window is [I,A,I]=idx=2, NOT in sr_n1's wolfram (only idx=7)
    // So n=1 skips, n=2 fires for isolated A in 5-cell window.
    let sr_n2 = Rule1DSubrule {
        current_type: a,
        criteria_type: a,
        wolfram_code: 1u128 << 4,
        n: 2,
        randomness: None,
        output_type: c,
    };

    // n=3: only matches [I,I,I,A,I,I,I] (wolfram idx=8=0b001000) -> D
    // n=1 sub-window [I,A,I]=idx=2, NOT in sr_n1 -> skip
    // n=2 sub-window [I,I,A,I,I]=idx=4, NOT in sr_n2 (only idx=4... wait it IS idx=4)
    // Need sr_n2 to NOT match. sr_n2 has wolfram=1<<4 which matches [I,I,A,I,I].
    // For n=3 to fire, n=2 must also fail. Change sr_n2 to wolfram idx=0 instead.
    // Actually: let's just test n=3 independently.
    let sr_n3 = Rule1DSubrule {
        current_type: a,
        criteria_type: a,
        wolfram_code: 1u128 << 8,
        n: 3,
        randomness: None,
        output_type: d,
    };

    // Test A: n=1 fires for [A,A,A]
    {
        let rule = Rule1D {
            subrules: vec![sr_n1.clone(), sr_n2.clone(), sr_n3.clone()],
        };
        let init = vec![a, a, a];
        let mut g = Grid1D::new(3, 0, init, rule);
        g.step();
        assert_eq!(g.cell_type(1), b, "n=1 [A,A,A] -> B");
    }

    // Test B: n=2 fires for isolated A in 5-cell grid
    // [I,I,A,I,I]: n=1 [I,A,I] idx=2 not set in sr_n1 (only 7) -> skip. n=2 [I,I,A,I,I] idx=4 -> C
    {
        let rule = Rule1D {
            subrules: vec![sr_n1.clone(), sr_n2.clone(), sr_n3.clone()],
        };
        let init = vec![inactive, inactive, a, inactive, inactive];
        let mut g = Grid1D::new(5, 0, init, rule);
        g.step();
        assert_eq!(g.cell_type(2), c, "n=2 [I,I,A,I,I] -> C (n=1 skipped)");
    }

    // Test C: n=3 fires for isolated A in 7-cell grid
    // [I,I,I,A,I,I,I]: n=1 [I,A,I] idx=2 -> skip. n=2 [I,I,A,I,I] idx=4 -> C (sr_n2 matches!)
    // sr_n2 wolfram=1<<4 matches [I,I,A,I,I]. So n=2 fires before n=3 gets checked.
    // To let n=3 fire, make sr_n2 not match this pattern.
    // sr_n2 only has idx=4. For 7-cell grid center: n=2 window [I,I,A,I,I] is idx=4. Matches.
    // Let's make sr_n2 have a different wolfram so it doesn't match isolated-A patterns.
    let sr_n2_v2 = Rule1DSubrule {
        current_type: a,
        criteria_type: a,
        wolfram_code: 1u128 << 15, // [A,A,A,A,A] only
        n: 2,
        randomness: None,
        output_type: c,
    };

    {
        let rule = Rule1D {
            subrules: vec![sr_n1.clone(), sr_n2_v2.clone(), sr_n3.clone()],
        };
        let init = vec![
            inactive, inactive, inactive, a, inactive, inactive, inactive,
        ];
        let mut g = Grid1D::new(7, 0, init, rule);
        g.step();
        // n=1: [I,A,I] idx=2 not in wolfram(7) -> skip
        // n=2: [I,I,A,I,I] idx=4 not in wolfram(15) -> skip
        // n=3: [I,I,I,A,I,I,I] idx=8 in wolfram(8) -> D
        assert_eq!(
            g.cell_type(3),
            d,
            "n=3 [I,I,I,A,I,I,I] -> D (n=1,n=2 skipped)"
        );
    }

    // Test D: all 3 n-values coexist — [A,A,A,A,A,A,A,A,A] (9 cells)
    // Center cell: n=1 [A,A,A] idx=7 -> B (n=1 fires first)
    {
        let rule = Rule1D {
            subrules: vec![sr_n1.clone(), sr_n2_v2.clone(), sr_n3.clone()],
        };
        let init = vec![a; 9];
        let mut g = Grid1D::new(9, 0, init, rule);
        g.step();
        // idx 1..7: n=1 [A,A,A] -> B
        // idx 0: n=1 [I,A,A] idx=6 not set -> skip, n=2 [I,I,A,A,A] idx=7 not set(15) -> skip, n=3 [I,I,I,A,A,A,A] idx=56 not set(8) -> skip -> inactive
        // idx 8: same as idx 0 -> inactive
        assert_eq!(g.cell_type(4), b, "center n=1 fires first");
        assert_eq!(g.cell_type(1), b, "idx 1 n=1 fires");
        assert_eq!(
            g.cell_type(0),
            inactive,
            "edge n=1,n=2,n=3 all skip -> inactive"
        );
    }
}

// ─── 5. Population counting: parallel vs serial, and the total invariant ───

/// `set_thread_override` is process-global, so tests that drive it must not run
/// concurrently with each other or they would clobber each other's setting.
static THREAD_OVERRIDE_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// `counts_current` must always account for every cell exactly once.
///
/// The dominant type's population is back-filled by subtraction rather than
/// counted, so any error in the counted types shows up here as a total that no
/// longer matches the grid size.
fn assert_counts_total_1d(g: &Grid1D, ctx: &str) {
    let sum: u64 = g.counts_current.values().sum();
    assert_eq!(
        sum, g.width as u64,
        "{ctx}: counts sum {sum} != {} cells",
        g.width
    );
}

fn assert_counts_total_2d(g: &Grid2D, ctx: &str) {
    let sum: u64 = g.counts_current.values().sum();
    let total = (g.width * g.height) as u64;
    assert_eq!(sum, total, "{ctx}: counts sum {sum} != {total} cells");
}

/// Three-state cycle: every cell changes type every step, so all three types are
/// live in every chunk — exactly the case where merging per-chunk counters has to
/// fold every entry, not just the first one it recognizes.
fn three_state_rule_1d() -> Rule1D {
    let (a, b, c) = (
        CellType::from("A"),
        CellType::from("B"),
        CellType::from("C"),
    );
    let any = 0xFFu128;
    Rule1D {
        subrules: vec![
            Rule1DSubrule {
                current_type: a,
                criteria_type: a,
                wolfram_code: any,
                n: 1,
                randomness: None,
                output_type: b,
            },
            Rule1DSubrule {
                current_type: b,
                criteria_type: b,
                wolfram_code: any,
                n: 1,
                randomness: None,
                output_type: c,
            },
            Rule1DSubrule {
                current_type: c,
                criteria_type: c,
                wolfram_code: any,
                n: 1,
                randomness: None,
                output_type: a,
            },
        ],
    }
}

fn three_state_rule_2d() -> Rule2D {
    let (a, b, c) = (
        CellType::from("A"),
        CellType::from("B"),
        CellType::from("C"),
    );
    Rule2D {
        subrules: vec![
            Rule2DSubrule::new(
                a,
                b,
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                b,
                None,
                None,
            ),
            Rule2DSubrule::new(
                b,
                c,
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                c,
                None,
                None,
            ),
            Rule2DSubrule::new(
                c,
                a,
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                a,
                None,
                None,
            ),
        ],
    }
}

fn init_cycle(n: usize) -> Vec<CellType> {
    let (a, b, c) = (
        CellType::from("A"),
        CellType::from("B"),
        CellType::from("C"),
    );
    (0..n)
        .map(|i| match i % 3 {
            0 => a,
            1 => b,
            _ => c,
        })
        .collect()
}

/// Multi-threaded stepping must produce the same populations as single-threaded.
///
/// Widths are deliberately not multiples of the thread count so the final chunk
/// is short, and `history_limit` values are chosen so `chunk * history_limit`
/// alignment differs between them.
#[test]
fn parallel_counts_match_serial_1d() {
    let _guard = THREAD_OVERRIDE_GUARD.lock().unwrap();
    // Force the work heuristic to split even these small grids, so the
    // multi-chunk merge path is actually exercised.
    cella_lib::threads::set_min_work_per_chunk_override(1);
    for &width in &[10_241usize, 65_537] {
        for &hist in &[0usize, 3, 7] {
            cella_lib::threads::set_thread_override(1);
            let mut serial = Grid1D::new(width, hist, init_cycle(width), three_state_rule_1d());
            for _ in 0..5 {
                serial.step();
                assert_counts_total_1d(&serial, &format!("serial w={width} hl={hist}"));
            }

            for &threads in &[4usize, 8] {
                cella_lib::threads::set_thread_override(threads);
                let mut par = Grid1D::new(width, hist, init_cycle(width), three_state_rule_1d());
                for _ in 0..5 {
                    par.step();
                    assert_counts_total_1d(&par, &format!("par t={threads} w={width} hl={hist}"));
                }
                assert_eq!(
                    par.counts_current, serial.counts_current,
                    "counts_current differs at t={threads}, width={width}, hl={hist}"
                );
                assert_eq!(
                    par.peak_counts, serial.peak_counts,
                    "peak_counts differs at t={threads}, width={width}, hl={hist}"
                );
            }
        }
    }
    cella_lib::threads::clear_min_work_per_chunk_override();
    cella_lib::threads::clear_thread_override();
}

#[test]
fn parallel_counts_match_serial_2d() {
    let _guard = THREAD_OVERRIDE_GUARD.lock().unwrap();
    cella_lib::threads::set_min_work_per_chunk_override(1);
    for &(w, h) in &[(63usize, 63usize), (65, 65), (257, 129)] {
        for &hist in &[0usize, 3, 7] {
            cella_lib::threads::set_thread_override(1);
            let mut serial = Grid2D::new(w, h, hist, init_cycle(w * h), three_state_rule_2d());
            for _ in 0..5 {
                serial.step();
                assert_counts_total_2d(&serial, &format!("serial {w}x{h} hl={hist}"));
            }

            for &threads in &[4usize, 8] {
                cella_lib::threads::set_thread_override(threads);
                let mut par = Grid2D::new(w, h, hist, init_cycle(w * h), three_state_rule_2d());
                for _ in 0..5 {
                    par.step();
                    assert_counts_total_2d(&par, &format!("par t={threads} {w}x{h} hl={hist}"));
                }
                assert_eq!(
                    par.counts_current, serial.counts_current,
                    "counts_current differs at t={threads}, {w}x{h}, hl={hist}"
                );
                assert_eq!(
                    par.peak_counts, serial.peak_counts,
                    "peak_counts differs at t={threads}, {w}x{h}, hl={hist}"
                );
            }
        }
    }
    cella_lib::threads::clear_min_work_per_chunk_override();
    cella_lib::threads::clear_thread_override();
}

/// The dominant type is skipped during counting and back-filled by subtraction,
/// so it has to be re-elected when a growing type overtakes it. Here `Inactive`
/// starts as the vast majority and `Alive` floods the grid.
#[test]
fn dominant_type_switches_when_majority_flips_2d() {
    let alive = CellType::from("Alive");
    let inactive = CellType::inactive();
    // Any cell with at least one Alive neighbor becomes Alive, and Alive stays Alive.
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

    let mut saw_alive_majority = false;
    for step in 1..=20 {
        g.step();
        assert_counts_total_2d(&g, &format!("flood step {step}"));
        let n_alive = *g.counts_current.get(&alive.0).unwrap_or(&0);
        if n_alive * 2 > (w * h) as u64 {
            saw_alive_majority = true;
        }
    }
    assert!(
        saw_alive_majority,
        "Alive never became the majority; test setup is wrong"
    );
    // Once flooded, every cell is Alive and Inactive must report zero, not a
    // leftover back-filled remainder.
    assert_eq!(
        *g.counts_current.get(&alive.0).unwrap_or(&0),
        (w * h) as u64
    );
    assert_eq!(*g.counts_current.get(&inactive.0).unwrap_or(&0), 0);
}

// ─── 6. Painting entry point bounds ───

#[test]
fn transition_state_and_buffer_rejects_out_of_bounds_1d() {
    let x = CellType::from("X");
    let rule = Rule1D {
        subrules: vec![Rule1DSubrule {
            current_type: x,
            criteria_type: x,
            wolfram_code: 30,
            n: 1,
            randomness: None,
            output_type: x,
        }],
    };
    let width = 8usize;
    let mut g = Grid1D::new(width, 2, vec![CellType::inactive(); width], rule);
    assert!(
        g.transition_state_and_buffer(width - 1, &x).is_none(),
        "last valid index must succeed"
    );
    // `width` is one past the end: it used to slip past a `>` guard and panic.
    assert!(
        g.transition_state_and_buffer(width, &x).is_some(),
        "idx == width must be rejected"
    );
    assert!(
        g.transition_state_and_buffer(width + 5, &x).is_some(),
        "idx > width must be rejected"
    );
}

#[test]
fn transition_state_and_buffer_rejects_out_of_bounds_2d() {
    let alive = CellType::from("Alive");
    let rule = Rule2D {
        subrules: vec![Rule2DSubrule::new(
            alive,
            alive,
            2,
            CountOp::Gt,
            1,
            Neighborhood2D::Moore,
            alive,
            None,
            None,
        )],
    };
    let (w, h) = (4usize, 3usize);
    let total = w * h;
    let mut g = Grid2D::new(w, h, 2, vec![CellType::inactive(); total], rule);
    assert!(
        g.transition_state_and_buffer(total - 1, &alive).is_none(),
        "last valid index must succeed"
    );
    assert!(
        g.transition_state_and_buffer(total, &alive).is_some(),
        "idx == width*height must be rejected"
    );
    assert!(
        g.transition_state_and_buffer(total + 7, &alive).is_some(),
        "idx > width*height must be rejected"
    );
}
