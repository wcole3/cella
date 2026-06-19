use cella_lib::*;
use cella_lib::config::{CellaConfig, Config2D};

#[test]
fn serde_rule1d_roundtrip_multistate() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    // n=1, match any window by setting all 8 bits
    let any = 0xFFu128;
    let rule = Rule1D { subrules: vec![
        Rule1DSubrule { current_type: a.clone(), criteria_type: a.clone(), wolfram_code: any, n: 1, randomness: None, output_type: b.clone() },
        Rule1DSubrule { current_type: b.clone(), criteria_type: b.clone(), wolfram_code: any, n: 1, randomness: None, output_type: c.clone() },
        Rule1DSubrule { current_type: c.clone(), criteria_type: c.clone(), wolfram_code: any, n: 1, randomness: None, output_type: a.clone() },
    ]};
    let v1 = serde_json::to_value(&rule).unwrap();
    let rule2: Rule1D = serde_json::from_value(v1.clone()).unwrap();
    let v2 = serde_json::to_value(&rule2).unwrap();
    assert_eq!(v1, v2, "Rule1D JSON value should be stable across roundtrip");
}

#[test]
fn serde_rule2d_roundtrip_multistate() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    // Transition when at least 0 neighbors (always true). This exercises serde, not behavior.
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule::new(a.clone(), b.clone(), 0, CountOp::Gt, 1, Neighborhood2D::Moore, b.clone(), None, None),
        Rule2DSubrule::new(b.clone(), c.clone(), 0, CountOp::Gt, 1, Neighborhood2D::Moore, c.clone(), None, None),
        Rule2DSubrule::new(c.clone(), a.clone(), 0, CountOp::Gt, 1, Neighborhood2D::Moore, a.clone(), None, None),
    ]};
    let v1 = serde_json::to_value(&rule).unwrap();
    let rule2: Rule2D = serde_json::from_value(v1.clone()).unwrap();
    let v2 = serde_json::to_value(&rule2).unwrap();
    assert_eq!(v1, v2, "Rule2D JSON value should be stable across roundtrip");
}

#[test]
fn serde_gridstate_roundtrip_1d_three_state() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    let any = 0xFFu128;
    let rule = Rule1D { subrules: vec![
        Rule1DSubrule { current_type: a.clone(), criteria_type: a.clone(), wolfram_code: any, n: 1, randomness: None, output_type: b.clone() },
        Rule1DSubrule { current_type: b.clone(), criteria_type: b.clone(), wolfram_code: any, n: 1, randomness: None, output_type: c.clone() },
        Rule1DSubrule { current_type: c.clone(), criteria_type: c.clone(), wolfram_code: any, n: 1, randomness: None, output_type: a.clone() },
    ]};
    let width = 9usize; let hist = 3usize;
    let init = (0..width).map(|i| match i % 3 { 0 => a.clone(), 1 => b.clone(), _ => c.clone() }).collect::<Vec<_>>();
    let mut g = Grid1D::new(width, hist, init, rule);
    g.step();
    let st = GridState::from_grid1d(&g);
    let json = st.to_json();
    let st2 = GridState::from_json(&json).unwrap();
    let g2 = Grid1D::from_state(&st2).unwrap();
    assert_eq!(g2.width, g.width);
    assert_eq!(g2.step, g.step);
    for i in 0..width { assert_eq!(g2.cells[i].current, g.cells[i].current); }
}

#[test]
fn serde_gridstate_roundtrip_2d_three_state() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    let rule = Rule2D { subrules: vec![
        // Rotate states based on always-true threshold (count>=0)
        Rule2DSubrule::new(a.clone(), b.clone(), 0, CountOp::Gt, 1, Neighborhood2D::Moore, b.clone(), None, None),
        Rule2DSubrule::new(b.clone(), c.clone(), 0, CountOp::Gt, 1, Neighborhood2D::Moore, c.clone(), None, None),
        Rule2DSubrule::new(c.clone(), a.clone(), 0, CountOp::Gt, 1, Neighborhood2D::Moore, a.clone(), None, None),
    ]};
    let (w,h,hist) = (6usize, 4usize, 2usize);
    let mut init = Vec::with_capacity(w*h);
    for y in 0..h { for x in 0..w { let idx = (x + y) % 3; init.push(match idx { 0 => a.clone(), 1 => b.clone(), _ => c.clone() }); } }
    let mut g = Grid2D::new(w,h,hist,init,rule);
    g.step();
    let st = GridState::from_grid2d(&g);
    let json = st.to_json();
    let st2 = GridState::from_json(&json).unwrap();
    let g2 = Grid2D::from_state(&st2).unwrap();
    assert_eq!(g2.width, g.width);
    assert_eq!(g2.height, g.height);
    assert_eq!(g2.step, g.step);
    for i in 0..(w*h) { assert_eq!(g2.cell_states[i].current, g.cell_states[i].current); }
    // repeat for cells
    for i in 0..(w*h) { assert_eq!(g2.cells[i], g.cells[i]); }
    for i in 0..(w*h) { assert_eq!(g2.next_cells[i], CellType::inactive()); }
}

#[test]
fn serde_config_roundtrip_multistate_2d() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    let (w,h,hist) = (5usize, 4usize, 3usize);
    let init = (0..w*h).map(|i| match i % 3 { 0 => a.as_str().to_string(), 1 => b.as_str().to_string(), _ => c.as_str().to_string() }).collect::<Vec<_>>();
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule::new(a.clone(), b.clone(), 2, CountOp::Gt, 1, Neighborhood2D::VonNeumann, b.clone(), None, None),
        Rule2DSubrule::new(b.clone(), c.clone(), 2, CountOp::Gt, 1, Neighborhood2D::VonNeumann, c.clone(), None, None),
        Rule2DSubrule::new(c.clone(), a.clone(), 2, CountOp::Gt, 1, Neighborhood2D::VonNeumann, a.clone(), None, None),
    ]};
    let cfg = CellaConfig::D2(Config2D { width: w, height: h, history_limit: hist, initial: init, rule: rule.clone() });
    let s = serde_json::to_string(&cfg).unwrap();
    let cfg2: CellaConfig = serde_json::from_str(&s).unwrap();
    match cfg2 {
        CellaConfig::D2(c2) => {
            assert_eq!(c2.width, w);
            assert_eq!(c2.height, h);
            assert_eq!(c2.history_limit, hist);
            assert_eq!(c2.initial.len(), w*h);
            assert_eq!(c2.rule.subrules.len(), rule.subrules.len());
            // check that the rules are eq
            for (i, r) in c2.rule.subrules.iter().enumerate() {
                assert_eq!(*r, *rule.subrules.get(i).expect("rule subrules should deserialize"));
            }
            let built = CellaConfig::D2(c2.clone()).build_grid2d();
            assert!(built.is_some());
        }
        _ => panic!("expected 2d config"),
    }
}

#[test]
fn multistate_1d_rotation_behavior_4_states() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    let d = CellType::from("D");
    let any = 0xFFu128;
    let rule = Rule1D { subrules: vec![
        Rule1DSubrule { current_type: a.clone(), criteria_type: a.clone(), wolfram_code: any, n: 1, randomness: None, output_type: b.clone() },
        Rule1DSubrule { current_type: b.clone(), criteria_type: b.clone(), wolfram_code: any, n: 1, randomness: None, output_type: c.clone() },
        Rule1DSubrule { current_type: c.clone(), criteria_type: c.clone(), wolfram_code: any, n: 1, randomness: None, output_type: d.clone() },
        Rule1DSubrule { current_type: d.clone(), criteria_type: d.clone(), wolfram_code: any, n: 1, randomness: None, output_type: a.clone() },
    ]};
    let width = 12usize; let hist = 2usize;
    let init = (0..width).map(|i| match i % 4 { 0 => a.clone(), 1 => b.clone(), 2 => c.clone(), _ => d.clone() }).collect::<Vec<_>>();
    let mut g = Grid1D::new(width, hist, init, rule);
    g.step();
    for i in 0..width {
        let exp = match i % 4 { 0 => b.clone(), 1 => c.clone(), 2 => d.clone(), _ => a.clone() };
        assert_eq!(g.cells[i].current, exp);
    }
}
