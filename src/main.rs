use cella_lib::*;
use std::fs;
use std::io::{self, Write};

fn print_grid_2d(g: &Grid2D, active: &CellType) {
    for y in 0..g.height {
        let mut line = String::with_capacity(g.width);
        for x in 0..g.width {
            let i = y * g.width + x;
            if g.cells[i].current == *active { line.push('#'); } else { line.push('.'); }
        }
        println!("{}", line);
    }
}

fn print_grid_1d(g: &Grid1D, active: &CellType) {
    let mut line = String::with_capacity(g.width);
    for i in 0..g.width {
        if g.cells[i].current == *active { line.push('#'); } else { line.push('.'); }
    }
    println!("{}", line);
}

fn read_line_trim() -> String {
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
    input.trim().to_string()
}

fn ask_steps(default_steps: usize) -> usize {
    print!("Enter number of steps to run [{}]: ", default_steps);
    let _ = io::stdout().flush();
    let s = read_line_trim();
    s.parse::<usize>().unwrap_or(default_steps)
}

fn demo_life() {
    let alive = CellType("Alive".into());
    let inactive = CellType::inactive();
    let rule = Rule2D { subrules: vec![
        Rule2DSubrule { current_type: alive.clone(), criteria_type: alive.clone(), threshold: 2, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
        Rule2DSubrule { current_type: inactive.clone(), criteria_type: alive.clone(), threshold: 3, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
    ]};

    let width = 20usize; let height = 10usize; let hist = 5usize;
    let mut init = vec![CellType::inactive(); width*height];
    // Seed a blinker pattern
    let set_alive = |x: usize, y: usize, v: &mut Vec<CellType>| { v[y*width + x] = alive.clone(); };
    set_alive(5, 5, &mut init);
    set_alive(6, 5, &mut init);
    set_alive(7, 5, &mut init);

    let mut grid = Grid2D::new(width, height, hist, init, rule);
    let steps = ask_steps(5);
    println!("Initial state (step {}):", grid.step);
    print_grid_2d(&grid, &alive);
    for _ in 0..steps {
        grid.step();
        println!("\nAfter step {}:", grid.step);
        print_grid_2d(&grid, &alive);
    }
    let json = grid2d_to_json(&grid);
    let _ = fs::write("snapshot.json", json);
}

fn demo_1d_rule30() {
    let x = CellType("X".into());
    let inactive = CellType::inactive();
    let sub_active = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() };
    // Important: allow Inactive cells to become X when Rule 30 criteria match.
    let sub_inactive = Rule1DSubrule { current_type: inactive.clone(), criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() };
    let rule = Rule1D { subrules: vec![sub_active, sub_inactive] };
    let width = 41usize; let hist = 5usize;
    let mut init = vec![inactive.clone(); width];
    init[width/2] = x.clone();
    let mut g = Grid1D::new(width, hist, init, rule);
    let steps = ask_steps(20);
    println!("Initial:");
    print_grid_1d(&g, &x);
    for _ in 0..steps {
        g.step();
        print_grid_1d(&g, &x);
    }
}

fn demo_1d_n2() {
    let x = CellType("X".into());
    // A made-up n=2 code producing interesting patterns (checker-ish)
    let code: u128 = 0xAAAAAAAA; // 32-bit alternating
    let sub = Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: code, n: 2, randomness: None, output_type: x.clone() };
    let rule = Rule1D { subrules: vec![sub] };
    let width = 41usize; let hist = 5usize;
    let mut init = vec![CellType::inactive(); width];
    init[width/2] = x.clone();
    let mut g = Grid1D::new(width, hist, init, rule);
    let steps = ask_steps(20);
    println!("Initial:");
    print_grid_1d(&g, &x);
    for _ in 0..steps { g.step(); print_grid_1d(&g, &x); }
}

fn demo_from_config() {
    use cella_lib::config::CellaConfig;
    print!("Enter path to JSON config: ");
    let _ = io::stdout().flush();
    let path = read_line_trim();
    match CellaConfig::from_file(&path) {
        Ok(cfg) => {
            match cfg {
                CellaConfig::D1(_) => {
                    if let Some(mut g) = cfg.build_grid1d() {
                        let active = g.cells.iter().find(|c| c.current != CellType::inactive()).map(|c| c.current.clone()).unwrap_or(CellType("X".into()));
                        let steps = ask_steps(10);
                        print_grid_1d(&g, &active);
                        for _ in 0..steps { g.step(); print_grid_1d(&g, &active); }
                    } else { println!("Invalid 1D config lengths."); }
                }
                CellaConfig::D2(_) => {
                    if let Some(mut g) = cfg.build_grid2d() {
                        let active = g.cells.iter().find(|c| c.current != CellType::inactive()).map(|c| c.current.clone()).unwrap_or(CellType("Alive".into()));
                        let steps = ask_steps(10);
                        print_grid_2d(&g, &active);
                        for _ in 0..steps { g.step(); println!("\nstep {}:", g.step); print_grid_2d(&g, &active); }
                        let json = grid2d_to_json(&g);
                        let _ = fs::write("snapshot.json", json);
                    } else { println!("Invalid 2D config lengths."); }
                }
            }
        }
        Err(e) => println!("Failed to load config: {}", e),
    }
}

fn menu() {
    loop {
        println!("\nCella demos:");
        println!("1) 2D Game of Life (approx)");
        println!("2) 1D Wolfram Rule 30 (n=1)");
        println!("3) 1D Wolfram n=2 demo");
        println!("4) Load from configuration file (JSON)");
        println!("5) Langton's ant (placeholder)");
        println!("0) Exit");
        print!("Select an option: ");
        let _ = io::stdout().flush();
        let choice = read_line_trim();
        match choice.as_str() {
            "1" => demo_life(),
            "2" => demo_1d_rule30(),
            "3" => demo_1d_n2(),
            "4" => demo_from_config(),
            "5" => println!("Langton's ant not yet implemented in this engine (requires moving agent)."),
            "0" => { println!("Bye!"); break; }
            _ => println!("Unknown option."),
        }
    }
}

fn main() {
    menu();
}
