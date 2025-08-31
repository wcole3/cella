use cella_lib::{CellType, Grid2D, Neighborhood2D, Rule2D, Rule2DSubrule, grid2d_to_json};
use std::fs;

fn print_grid(g: &Grid2D, alive: &CellType) {
    for y in 0..g.height {
        let mut line = String::with_capacity(g.width);
        for x in 0..g.width {
            let i = y * g.width + x;
            if g.cells[i].current == *alive { line.push('#'); } else { line.push('.'); }
        }
        println!("{}", line);
    }
}

fn main() {
    // Simple demonstration: Life-like rule using threshold semantics
    let alive = CellType("Alive".into());
    let inert = CellType::inert();

    let rule = Rule2D { subrules: vec![
        // Survival: Alive stays Alive if at least 2 Alive neighbors (approximation of Life)
        Rule2DSubrule { current_type: alive.clone(), criteria_type: alive.clone(), threshold: 2, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
        // Birth: Inert becomes Alive if at least 3 Alive neighbors (approximation)
        Rule2DSubrule { current_type: inert.clone(), criteria_type: alive.clone(), threshold: 3, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
    ]};

    let width = 10usize; let height = 10usize; let hist = 5usize;
    let mut init = vec![CellType::inert(); width*height];
    // Seed a simple blinker pattern at (4,4), (4,5), (4,6)
    let set_alive = |x: usize, y: usize, v: &mut Vec<CellType>| { v[y*width + x] = alive.clone(); };
    set_alive(4, 4, &mut init);
    set_alive(5, 4, &mut init);
    set_alive(6, 4, &mut init);

    let mut grid = Grid2D::new(width, height, hist, init, rule);

    println!("Initial state (step {}):", grid.step);
    print_grid(&grid, &alive);

    for _ in 0..5 {
        grid.step();
        println!("\nAfter step {}:", grid.step);
        print_grid(&grid, &alive);
    }

    // Save snapshot
    let json = grid2d_to_json(&grid);
    let _ = fs::write("snapshot.json", json);
}
