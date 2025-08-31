use crate::demos::{ask_steps, read_line_trim};
use cella_lib::*;

fn print_grid_1d(g: &Grid1D, active: &CellType) {
    let mut line = String::with_capacity(g.width);
    for i in 0..g.width {
        if g.cells[i].current == *active { line.push('#'); } else { line.push('.'); }
    }
    println!("{}", line);
}

pub fn demo_1d_rule30() {
    let x = CellType("X".into());
    let inactive = CellType::inactive();
    let sub_active = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() };
    let sub_inactive = Rule1DSubrule { current_type: inactive.clone(), criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() };
    let rule = Rule1D { subrules: vec![sub_active, sub_inactive] };
    let width = 41usize; let hist = 5usize;
    let mut init = vec![inactive.clone(); width];
    init[width/2] = x.clone();
    let mut g = Grid1D::new(width, hist, init, rule);
    let steps = ask_steps(20);
    println!("Initial:");
    print_grid_1d(&g, &x);
    for _ in 0..steps { g.step(); print_grid_1d(&g, &x); }
}

pub fn demo_1d_n2() {
    let x = CellType("X".into());
    let inactive = CellType::inactive();
    let code: u128 = 0xAAAAAAAA; // alternating
    let sub_active = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: code, n: 2, randomness: None, output_type: x.clone() };
    let sub_inactive = Rule1DSubrule { current_type: inactive.clone(), criteria_type: x.clone(), wolfram_code: code, n: 2, randomness: None, output_type: x.clone() };
    let rule = Rule1D { subrules: vec![sub_active, sub_inactive] };
    let width = 41usize; let hist = 5usize;
    let mut init = vec![inactive.clone(); width];
    init[width/2] = x.clone();
    let mut g = Grid1D::new(width, hist, init, rule);
    let steps = ask_steps(20);
    println!("Initial:");
    print_grid_1d(&g, &x);
    for _ in 0..steps { g.step(); print_grid_1d(&g, &x); }
}

pub fn demo_1d_custom() {
    let x = CellType("X".into());
    let inactive = CellType::inactive();
    println!("Enter Wolfram code (as integer, supports up to u128): ");
    let code_input = read_line_trim();
    let wolfram_code: u128 = code_input.parse().unwrap_or(30);
    println!("Enter neighborhood radius n (>=1): ");
    let n_input = read_line_trim();
    let n: u8 = n_input.parse().unwrap_or(1);
    let sub_active = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code, n, randomness: None, output_type: x.clone() };
    let sub_inactive = Rule1DSubrule { current_type: inactive.clone(), criteria_type: x.clone(), wolfram_code, n, randomness: None, output_type: x.clone() };
    let rule = Rule1D { subrules: vec![sub_active, sub_inactive] };
    if let Err(e) = rule.validate() { println!("Invalid rule: {}", e); return; }
    let width = 79usize; let hist = 5usize;
    let mut init = vec![inactive.clone(); width];
    init[width/2] = x.clone();
    let mut g = Grid1D::new(width, hist, init, rule);
    let steps = ask_steps(20);
    println!("Initial:");
    print_grid_1d(&g, &x);
    for _ in 0..steps { g.step(); print_grid_1d(&g, &x); }
}
