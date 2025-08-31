use crate::demos::{ask_steps, read_line_trim};
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

pub fn demo_life() {
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

pub fn demo_from_config() {
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
                        // simple print
                        let mut line = String::new();
                        for _ in 0..g.width { line.push('.'); }
                        println!("(1D) initial:");
                        for _ in 0..steps { g.step(); }
                        println!("Ran {} steps.", steps);
                        // show final
                        let mut line = String::with_capacity(g.width);
                        for i in 0..g.width { if g.cells[i].current == active { line.push('#'); } else { line.push('.'); }}
                        println!("{}", line);
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
