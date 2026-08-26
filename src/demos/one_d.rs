//! 1D demo builders and interactive CLI runners.
//!
//! *Builders* (`build_*`) create a [`Grid1D`] ready for stepping.
//! *Demo* functions (`demo_*`) run an interactive CLI session.

use crate::demos::{ask_steps, read_line_trim};
use cella_lib::*;
use std::collections::BTreeMap;

/// Build a 1D grid pre-seeded for Wolfram Rule 30 (n=1).
///
/// A single `X` cell is placed in the centre; the rest are `Inactive`.
pub fn build_1d_rule30(width: usize, history: usize) -> Grid1D {
    let x = CellType::from("X");
    let inactive = CellType::inactive();
    let sub_active = Rule1DSubrule {
        current_type: x,
        criteria_type: x,
        wolfram_code: 30,
        n: 1,
        randomness: None,
        output_type: x,
    };
    let sub_inactive = Rule1DSubrule {
        current_type: inactive,
        criteria_type: x,
        wolfram_code: 30,
        n: 1,
        randomness: None,
        output_type: x,
    };
    let rule = Rule1D {
        subrules: vec![sub_active, sub_inactive],
    };
    let mut init = vec![inactive; width];
    init[width / 2] = x;
    Grid1D::new(width, history, init, rule)
}

/// Build a 1D grid for an arbitrary Wolfram code and neighbourhood radius.
///
/// Returns `Err` if the code/n combination fails validation.
pub fn build_1d_code_n(
    wolfram_code: u128,
    n: u8,
    width: usize,
    history: usize,
) -> Result<Grid1D, RuleError> {
    let x = CellType::from("X");
    let inactive = CellType::inactive();
    let sub_active = Rule1DSubrule {
        current_type: x,
        criteria_type: x,
        wolfram_code,
        n,
        randomness: None,
        output_type: x,
    };
    let sub_inactive = Rule1DSubrule {
        current_type: inactive,
        criteria_type: x,
        wolfram_code,
        n,
        randomness: None,
        output_type: x,
    };
    let rule = Rule1D {
        subrules: vec![sub_active, sub_inactive],
    };
    rule.validate()?;
    let mut init = vec![inactive; width];
    init[width / 2] = x;
    Ok(Grid1D::new(width, history, init, rule))
}

/// Build a 1D three-state (A→B→C→A) cycling automaton.
pub fn build_1d_three_state_cycle(width: usize, history: usize) -> Grid1D {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    let any = 0xFFu128; // match any 3-bit window
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
    let init = (0..width)
        .map(|i| match i % 3 {
            0 => a,
            1 => b,
            _ => c,
        })
        .collect();
    Grid1D::new(width, history, init, rule)
}

/// Print a 1D grid row to stdout using symbol mapping.
fn print_grid_1d(g: &Grid1D) {
    // Build stable mapping for active states (non-Inactive)
    let mut names: Vec<String> = (0..g.width)
        .map(|i| g.cell_type(i).as_str().to_string())
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
        let ty = &g.cell_type(i).as_str().to_string();
        if ty == INACTIVE {
            line.push('.');
        } else {
            line.push(*map.get(ty).unwrap_or(&'?'));
        }
    }
    println!("{}", line);
}

/// Interactive CLI demo: Wolfram Rule 30 (n=1, width 41).
pub fn demo_1d_rule30() {
    let x = CellType::from("X");
    let inactive = CellType::inactive();
    let sub_active = Rule1DSubrule {
        current_type: x,
        criteria_type: x,
        wolfram_code: 30,
        n: 1,
        randomness: None,
        output_type: x,
    };
    let sub_inactive = Rule1DSubrule {
        current_type: inactive,
        criteria_type: x,
        wolfram_code: 30,
        n: 1,
        randomness: None,
        output_type: x,
    };
    let rule = Rule1D {
        subrules: vec![sub_active, sub_inactive],
    };
    let width = 41usize;
    let hist = 5usize;
    let mut init = vec![inactive; width];
    init[width / 2] = x;
    let mut g = Grid1D::new(width, hist, init, rule);
    let steps = ask_steps(20);
    println!("Initial:");
    print_grid_1d(&g);
    for _ in 0..steps {
        g.step();
        print_grid_1d(&g);
    }
}

/// Interactive CLI demo: Wolfram n=2 (alternating code, width 41).
pub fn demo_1d_n2() {
    let x = CellType::from("X");
    let inactive = CellType::inactive();
    let code: u128 = 0xAAAAAAAA; // alternating
    let sub_active = Rule1DSubrule {
        current_type: x,
        criteria_type: x,
        wolfram_code: code,
        n: 2,
        randomness: None,
        output_type: x,
    };
    let sub_inactive = Rule1DSubrule {
        current_type: inactive,
        criteria_type: x,
        wolfram_code: code,
        n: 2,
        randomness: None,
        output_type: x,
    };
    let rule = Rule1D {
        subrules: vec![sub_active, sub_inactive],
    };
    let width = 41usize;
    let hist = 5usize;
    let mut init = vec![inactive; width];
    init[width / 2] = x;
    let mut g = Grid1D::new(width, hist, init, rule);
    let steps = ask_steps(20);
    println!("Initial:");
    print_grid_1d(&g);
    for _ in 0..steps {
        g.step();
        print_grid_1d(&g);
    }
}

/// Interactive CLI demo: user-supplied Wolfram code and neighbourhood radius.
pub fn demo_1d_custom() {
    let x = CellType::from("X");
    let inactive = CellType::inactive();
    println!("Enter Wolfram code (as integer, supports up to u128): ");
    let code_input = read_line_trim();
    let wolfram_code: u128 = code_input.parse().unwrap_or(30);
    println!("Enter neighborhood radius n (>=1): ");
    let n_input = read_line_trim();
    let n: u8 = n_input.parse().unwrap_or(1);
    let sub_active = Rule1DSubrule {
        current_type: x,
        criteria_type: x,
        wolfram_code,
        n,
        randomness: None,
        output_type: x,
    };
    let sub_inactive = Rule1DSubrule {
        current_type: inactive,
        criteria_type: x,
        wolfram_code,
        n,
        randomness: None,
        output_type: x,
    };
    let rule = Rule1D {
        subrules: vec![sub_active, sub_inactive],
    };
    if let Err(e) = rule.validate() {
        println!("Invalid rule: {}", e);
        return;
    }
    let width = 79usize;
    let hist = 5usize;
    let mut init = vec![inactive; width];
    init[width / 2] = x;
    let mut g = Grid1D::new(width, hist, init, rule);
    let steps = ask_steps(20);
    println!("Initial:");
    print_grid_1d(&g);
    for _ in 0..steps {
        g.step();
        print_grid_1d(&g);
    }
}

/// Interactive CLI demo: three-state cycle (A→B→C→A).
pub fn demo_1d_three_state_cycle() {
    let _a = CellType::from("A");
    let _b = CellType::from("B");
    let _c = CellType::from("C");
    let mut g = build_1d_three_state_cycle(39, 3);
    let steps = ask_steps(15);
    println!("Initial:");
    print_grid_1d(&g);
    for _ in 0..steps {
        g.step();
        print_grid_1d(&g);
    }
}
