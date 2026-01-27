use crate::demos::{ask_steps, read_line_trim};
use cella_lib::*;
use std::collections::BTreeMap;

// ------- Reusable builders (for CLI and GUI) -------
pub fn build_1d_rule30(width: usize, history: usize) -> Grid1D {
    let x = CellType("X".into());
    let inactive = CellType::inactive();
    let sub_active = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() };
    let sub_inactive = Rule1DSubrule { current_type: inactive.clone(), criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() };
    let rule = Rule1D { subrules: vec![sub_active, sub_inactive] };
    let mut init = vec![inactive.clone(); width];
    init[width/2] = x.clone();
    Grid1D::new(width, history, init, rule)
}

pub fn build_1d_code_n(wolfram_code: u128, n: u8, width: usize, history: usize) -> Result<Grid1D, RuleError> {
    let x = CellType("X".into());
    let inactive = CellType::inactive();
    let sub_active = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code, n, randomness: None, output_type: x.clone() };
    let sub_inactive = Rule1DSubrule { current_type: inactive.clone(), criteria_type: x.clone(), wolfram_code, n, randomness: None, output_type: x.clone() };
    let rule = Rule1D { subrules: vec![sub_active, sub_inactive] };
    rule.validate()?;
    let mut init = vec![inactive.clone(); width];
    init[width/2] = x.clone();
    Ok(Grid1D::new(width, history, init, rule))
}

pub fn build_1d_three_state_cycle(width: usize, history: usize) -> Grid1D {
    let a = CellType("A".into());
    let b = CellType("B".into());
    let c = CellType("C".into());
    let any = 0xFFu128; // match any 3-bit window
    let rule = Rule1D { subrules: vec![
        Rule1DSubrule { current_type: a.clone(), criteria_type: a.clone(), wolfram_code: any, n: 1, randomness: None, output_type: b.clone() },
        Rule1DSubrule { current_type: b.clone(), criteria_type: b.clone(), wolfram_code: any, n: 1, randomness: None, output_type: c.clone() },
        Rule1DSubrule { current_type: c.clone(), criteria_type: c.clone(), wolfram_code: any, n: 1, randomness: None, output_type: a.clone() },
    ]};
    let init = (0..width).map(|i| match i % 3 { 0 => a.clone(), 1 => b.clone(), _ => c.clone() }).collect();
    Grid1D::new(width, history, init, rule)
}

fn print_grid_1d(g: &Grid1D) {
    // Build stable mapping for active states (non-Inactive)
    let mut names: Vec<String> = g
        .cells
        .iter()
        .map(|c| c.current.0.clone())
        .filter(|n| n != INACTIVE)
        .collect();
    names.sort();
    names.dedup();
    let symbol_pool: Vec<char> = "!@#$%^&*()".chars().chain('a'..='z').collect();
    let mut map: BTreeMap<String, char> = BTreeMap::new();
    for (i, n) in names.iter().enumerate() {
        let ch = symbol_pool.get(i).copied().unwrap_or('?');
        map.insert(n.clone(), ch);
    }
    let mut line = String::with_capacity(g.width);
    for i in 0..g.width {
        let ty = &g.cells[i].current.0;
        if ty == INACTIVE { line.push('.'); }
        else { line.push(*map.get(ty).unwrap_or(&'?')); }
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
    print_grid_1d(&g);
    for _ in 0..steps { g.step(); print_grid_1d(&g); }
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
    print_grid_1d(&g);
    for _ in 0..steps { g.step(); print_grid_1d(&g); }
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
    print_grid_1d(&g);
    for _ in 0..steps { g.step(); print_grid_1d(&g); }
}

pub fn demo_1d_three_state_cycle() {
    let _a = CellType("A".into());
    let _b = CellType("B".into());
    let _c = CellType("C".into());
    let mut g = build_1d_three_state_cycle(39, 3);
    let steps = ask_steps(15);
    println!("Initial:");
    print_grid_1d(&g);
    for _ in 0..steps { g.step(); print_grid_1d(&g); }
}
