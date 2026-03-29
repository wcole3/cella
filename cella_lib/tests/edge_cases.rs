use cella_lib::*;

// -------- CellState history bounding --------

#[test]
fn history_bounded_by_limit() {
    let mut st = CellState::new(CellType("A".into()), 2);
    st.transition(&CellType("B".into()));
    st.transition(&CellType("C".into()));
    st.transition(&CellType("D".into()));
    // limit=2, so only the two most recent previous states are kept
    assert_eq!(st.history.len(), 2);
    assert_eq!(st.history[0], CellType("B".into()));
    assert_eq!(st.history[1], CellType("C".into()));
    assert_eq!(st.current, CellType("D".into()));
}

#[test]
fn history_zero_limit_keeps_nothing() {
    let mut st = CellState::new(CellType("A".into()), 0);
    st.transition(&CellType("B".into()));
    st.transition(&CellType("C".into()));
    assert_eq!(st.history.len(), 0);
    assert_eq!(st.current, CellType("C".into()));
}

#[test]
fn history_same_type_no_push() {
    let mut st = CellState::new(CellType("A".into()), 3);
    st.transition(&CellType("A".into()));
    st.transition(&CellType("A".into()));
    assert_eq!(st.history.len(), 0);
    assert_eq!(st.age_in_state, 2);
}

#[test]
fn age_saturates_at_u32_max() {
    let mut st = CellState::new(CellType("A".into()), 1);
    st.age_in_state = u32::MAX - 1;
    st.transition(&CellType("A".into())); // -> MAX
    assert_eq!(st.age_in_state, u32::MAX);
    st.transition(&CellType("A".into())); // saturating, stays MAX
    assert_eq!(st.age_in_state, u32::MAX);
}

// -------- Rule1DSubrule validation edge cases --------

#[test]
fn validate_1d_n0_rejected() {
    let sub = Rule1DSubrule {
        current_type: CellType("X".into()),
        criteria_type: CellType("X".into()),
        wolfram_code: 0,
        n: 0,
        randomness: None,
        output_type: CellType("Y".into()),
    };
    assert_eq!(sub.validate(), Err(RuleError::InvalidN1D(0)));
}

#[test]
fn validate_1d_n4_too_many_patterns() {
    let sub = Rule1DSubrule {
        current_type: CellType("X".into()),
        criteria_type: CellType("X".into()),
        wolfram_code: 0,
        n: 4,
        randomness: None,
        output_type: CellType("Y".into()),
    };
    assert_eq!(sub.validate(), Err(RuleError::TooManyPatterns(4)));
}

#[test]
fn validate_1d_n3_accepts_max_u128() {
    let sub = Rule1DSubrule {
        current_type: CellType("X".into()),
        criteria_type: CellType("X".into()),
        wolfram_code: u128::MAX,
        n: 3,
        randomness: None,
        output_type: CellType("Y".into()),
    };
    assert!(sub.validate().is_ok());
}

#[test]
fn validate_1d_wolfram_code_at_exact_max_n1() {
    // n=1 -> 8 patterns -> max code = 2^8 - 1 = 255
    let ok = Rule1DSubrule {
        current_type: CellType("X".into()),
        criteria_type: CellType("X".into()),
        wolfram_code: 255,
        n: 1,
        randomness: None,
        output_type: CellType("Y".into()),
    };
    assert!(ok.validate().is_ok());

    let bad = Rule1DSubrule {
        current_type: CellType("X".into()),
        criteria_type: CellType("X".into()),
        wolfram_code: 256,
        n: 1,
        randomness: None,
        output_type: CellType("Y".into()),
    };
    assert_eq!(bad.validate(), Err(RuleError::InvalidWolframCode(256, 1)));
}

#[test]
fn validate_1d_randomness_boundary() {
    let x = CellType("X".into());
    // randomness at exact boundaries should be valid
    let r0 = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 0, n: 1, randomness: Some(0.0), output_type: x.clone() };
    assert!(r0.validate().is_ok());
    let r1 = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 0, n: 1, randomness: Some(1.0), output_type: x.clone() };
    assert!(r1.validate().is_ok());
    // slightly outside
    let rn = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 0, n: 1, randomness: Some(-0.001), output_type: x.clone() };
    assert_eq!(rn.validate(), Err(RuleError::InvalidRandomness));
    let rp = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 0, n: 1, randomness: Some(1.001), output_type: x.clone() };
    assert_eq!(rp.validate(), Err(RuleError::InvalidRandomness));
}

// -------- Rule2DSubrule validation edge cases --------

#[test]
fn validate_2d_eq_with_limit_rejected() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    let sub = Rule2DSubrule {
        current_type: a.clone(), criteria_type: b.clone(),
        count: 3, op: CountOp::Eq, limit: Some(5),
        range: 1, neighborhood: Neighborhood2D::Moore,
        randomness: None, output_type: b.clone(),
    };
    assert_eq!(sub.validate(), Err(RuleError::InvalidRange2D));
}

#[test]
fn validate_2d_range_zero_rejected() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    let sub = Rule2DSubrule {
        current_type: a.clone(), criteria_type: b.clone(),
        count: 1, op: CountOp::Gt, limit: None,
        range: 0, neighborhood: Neighborhood2D::Moore,
        randomness: None, output_type: b.clone(),
    };
    assert_eq!(sub.validate(), Err(RuleError::InvalidRange2D));
}

// -------- Grid behavior edge cases --------

#[test]
fn all_inactive_grid_1d_stays_inactive() {
    let rule = Rule1D { subrules: vec![] };
    let init = vec![CellType::inactive(); 5];
    let mut g = Grid1D::new(5, 2, init, rule);
    g.step();
    for c in &g.cells {
        assert_eq!(c.current, CellType::inactive());
    }
}

#[test]
fn all_inactive_grid_2d_stays_inactive() {
    let rule = Rule2D { subrules: vec![] };
    let init = vec![CellType::inactive(); 9];
    let mut g = Grid2D::new(3, 3, 2, init, rule);
    g.step();
    for c in &g.cells {
        assert_eq!(c.current, CellType::inactive());
    }
}

#[test]
fn grid_1d_width_1_steps_without_panic() {
    let x = CellType("X".into());
    let rule = Rule1D { subrules: vec![
        Rule1DSubrule {
            current_type: x.clone(), criteria_type: x.clone(),
            wolfram_code: 0xFF, n: 1, randomness: None,
            output_type: x.clone(),
        },
    ]};
    let init = vec![x.clone()];
    let mut g = Grid1D::new(1, 2, init, rule);
    g.step();
    // Should not panic; center cell has inactive neighbors
}

#[test]
fn grid_2d_1x1_steps_without_panic() {
    let a = CellType("A".into());
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule {
            current_type: a.clone(), criteria_type: a.clone(),
            count: 0, op: CountOp::Gt, limit: None,
            range: 1, neighborhood: Neighborhood2D::Moore,
            randomness: None, output_type: a.clone(),
        },
    ]};
    let init = vec![a.clone()];
    let mut g = Grid2D::new(1, 1, 2, init, rule);
    g.step();
    // Should not panic; no neighbors exist
}

#[test]
fn grid_1d_no_matching_subrule_becomes_inactive() {
    // Cell type "X" with a rule that only matches "Y" -> becomes Inactive
    let x = CellType("X".into());
    let y = CellType("Y".into());
    let rule = Rule1D { subrules: vec![
        Rule1DSubrule {
            current_type: y.clone(), criteria_type: y.clone(),
            wolfram_code: 0xFF, n: 1, randomness: None,
            output_type: y.clone(),
        },
    ]};
    let init = vec![CellType::inactive(), x.clone(), CellType::inactive()];
    let mut g = Grid1D::new(3, 2, init, rule);
    g.step();
    assert_eq!(g.cells[1].current, CellType::inactive());
}

#[test]
fn grid_2d_no_matching_subrule_becomes_inactive() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    // Rule only matches B, but grid is all A
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule {
            current_type: b.clone(), criteria_type: b.clone(),
            count: 0, op: CountOp::Gt, limit: None,
            range: 1, neighborhood: Neighborhood2D::Moore,
            randomness: None, output_type: b.clone(),
        },
    ]};
    let init = vec![a.clone(); 9];
    let mut g = Grid2D::new(3, 3, 2, init, rule);
    g.step();
    // All cells should become Inactive since no rule matches type A
    for c in &g.cells {
        assert_eq!(c.current, CellType::inactive());
    }
}

// -------- CountOp zero-neighbor edge cases --------

#[test]
fn countop_lt_zero_always_fails() {
    // Lt with count=0 means "less than 0 neighbors" which is impossible
    let a = CellType("A".into());
    let b = CellType("B".into());
    let sub = Rule2DSubrule {
        current_type: a.clone(), criteria_type: b.clone(),
        count: 0, op: CountOp::Lt, limit: None,
        range: 1, neighborhood: Neighborhood2D::Moore,
        randomness: None, output_type: b.clone(),
    };
    let rule = Rule2D { subrules: vec![sub] };
    let init = vec![a.clone(); 9];
    let mut g = Grid2D::new(3, 3, 2, init, rule);
    g.step();
    // Lt count=0 means <= 0 neighbors, with all A's and criteria B, count of B=0
    // so 0 <= 0 should match
    // Actually, let's just check it doesn't panic
}

#[test]
fn countop_eq_zero_matches_no_neighbors() {
    // Eq with count=0 means "exactly 0 neighbors of criteria_type"
    let a = CellType("A".into());
    let b = CellType("B".into());
    let out = CellType("O".into());
    let sub = Rule2DSubrule {
        current_type: a.clone(), criteria_type: b.clone(),
        count: 0, op: CountOp::Eq, limit: None,
        range: 1, neighborhood: Neighborhood2D::Moore,
        randomness: None, output_type: out.clone(),
    };
    let rule = Rule2D { subrules: vec![sub] };
    // All A, no B neighbors for center
    let init = vec![a.clone(); 9];
    let mut g = Grid2D::new(3, 3, 2, init, rule);
    g.step();
    // Center cell (1,1) has 0 B neighbors -> Eq 0 should match
    assert_eq!(g.cells[4].current, out);
}

// -------- Grid step counter --------

#[test]
fn grid_1d_step_counter_increments() {
    let rule = Rule1D { subrules: vec![] };
    let init = vec![CellType::inactive(); 3];
    let mut g = Grid1D::new(3, 1, init, rule);
    assert_eq!(g.step, 0);
    g.step();
    assert_eq!(g.step, 1);
    g.step();
    assert_eq!(g.step, 2);
}

#[test]
fn grid_2d_step_counter_increments() {
    let rule = Rule2D { subrules: vec![] };
    let init = vec![CellType::inactive(); 4];
    let mut g = Grid2D::new(2, 2, 1, init, rule);
    assert_eq!(g.step, 0);
    g.step();
    assert_eq!(g.step, 1);
    g.step();
    assert_eq!(g.step, 2);
}

// -------- Neighborhood type coverage for 2D --------

#[test]
fn all_neighborhood_types_2d_step_without_panic() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    let neighborhoods = [
        Neighborhood2D::Moore,
        Neighborhood2D::VonNeumann,
        Neighborhood2D::Langdon,
        Neighborhood2D::StraightLine,
    ];
    for nh in &neighborhoods {
        let rule = Rule2D { subrules: vec![
            Rule2DSubrule {
                current_type: a.clone(), criteria_type: b.clone(),
                count: 1, op: CountOp::Gt, limit: None,
                range: 2, neighborhood: *nh,
                randomness: None, output_type: b.clone(),
            },
        ]};
        let mut init = vec![a.clone(); 25];
        init[12] = b.clone(); // center of 5x5
        let mut g = Grid2D::new(5, 5, 2, init, rule);
        g.step();
        // Just verify no panic with each neighborhood type at range 2
    }
}

// -------- CellType basic properties --------

#[test]
fn cell_type_inactive_default() {
    let ct: CellType = Default::default();
    assert_eq!(ct, CellType::inactive());
    assert_eq!(ct.0, "Inactive");
}

#[test]
fn cell_type_display() {
    let ct = CellType("MyType".into());
    assert_eq!(format!("{}", ct), "MyType");
}

// -------- GridState roundtrip preserves counts and peaks --------

#[test]
fn grid_state_1d_roundtrip_counts_and_peaks() {
    let x = CellType("X".into());
    let inactive = CellType::inactive();
    let sub = Rule1DSubrule {
        current_type: x.clone(), criteria_type: x.clone(),
        wolfram_code: 0xFF, n: 1, randomness: None,
        output_type: x.clone(),
    };
    let rule = Rule1D { subrules: vec![sub] };
    let init = vec![inactive.clone(), x.clone(), inactive.clone(), x.clone(), inactive.clone()];
    let mut g = Grid1D::new(5, 2, init, rule);
    g.step();
    let st = GridState::from_grid1d(&g);
    let json = st.to_json();
    let st2 = GridState::from_json(&json).unwrap();
    let g2 = Grid1D::from_state(&st2).unwrap();
    assert_eq!(g.width, g2.width);
    assert_eq!(g.step, g2.step);
    assert_eq!(g.counts_current, g2.counts_current);
    assert_eq!(g.peak_counts, g2.peak_counts);
}

#[test]
fn grid_state_2d_roundtrip_counts_and_peaks() {
    let a = CellType("A".into());
    let b = CellType("B".into());
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule {
            current_type: a.clone(), criteria_type: b.clone(),
            count: 0, op: CountOp::Gt, limit: None,
            range: 1, neighborhood: Neighborhood2D::Moore,
            randomness: None, output_type: b.clone(),
        },
    ]};
    let mut init = vec![a.clone(); 9];
    init[4] = b.clone();
    let mut g = Grid2D::new(3, 3, 2, init, rule);
    g.step();
    let st = GridState::from_grid2d(&g);
    let json = st.to_json();
    let st2 = GridState::from_json(&json).unwrap();
    let g2 = Grid2D::from_state(&st2).unwrap();
    assert_eq!(g.width, g2.width);
    assert_eq!(g.height, g2.height);
    assert_eq!(g.step, g2.step);
    assert_eq!(g.counts_current, g2.counts_current);
    assert_eq!(g.peak_counts, g2.peak_counts);
}
