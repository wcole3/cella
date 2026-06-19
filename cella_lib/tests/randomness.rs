use cella_lib::*;

#[test]
fn one_d_randomness_zero_always_applies() {
    // Pattern: [Inactive, X, Inactive] with n=1 has index 2.
    let x = CellType::from("X");
    let y = CellType::from("Y");
    let sub = Rule1DSubrule {
        current_type: x.clone(),
        criteria_type: x.clone(),
        wolfram_code: 1u128 << 2,
        n: 1,
        randomness: Some(0.0), // should always pass when criteria matches
        output_type: y.clone(),
    };
    let rule = Rule1D { subrules: vec![sub] };
    let init = vec![CellType::inactive(), x.clone(), CellType::inactive()];
    let mut g = Grid1D::new(3, 2, init, rule);
    g.step();
    assert_eq!(g.cells[1].current, y);
}

#[test]
fn two_d_randomness_one_never_applies() {
    // Center A has one B neighbor, threshold 1 satisfied, but randomness=1.0 prevents application.
    let a = CellType::from("A");
    let b = CellType::from("B");
    let sub = Rule2DSubrule::new(a.clone(), b.clone(), 1, CountOp::Gt, 1, Neighborhood2D::Moore, b.clone(), Some(1.0), None);
    let rule = Rule2D { subrules: vec![sub] };
    let w = 3usize; let h = 3usize; let hist = 2usize;
    let mut init = vec![a.clone(); w*h];
    // Place a B neighbor at (1,0) relative to center (1,1)
    init[0*w + 1] = b.clone();
    let mut g = Grid2D::new(w, h, hist, init, rule);
    g.step();
    // Since rule didn't apply due to randomness=1.0, center becomes Inactive per engine base rule
    assert_eq!(g.cell_states[1*w + 1].current, CellType::inactive());
}
