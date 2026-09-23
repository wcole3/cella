//! 2D demo builders and interactive CLI runners.
//!
//! *Builders* (`build_*`) create a [`Grid2D`] ready for stepping.
//! *Demo* functions (`demo_*`) run an interactive CLI session.

use crate::demos::{ask_steps, read_line_trim};
use cella_lib::*;
use std::collections::BTreeMap;
use std::io::{self, Write};

/// Build a 2D Game-of-Life grid with a blinker seed in the centre.
pub fn build_2d_life(width: usize, height: usize, history: usize) -> Grid2D {
    let alive = CellType::from("Alive");
    let inactive = CellType::inactive();
    let rule = Rule2D {
        subrules: vec![
            // Overpopulation: Alive with 4+ Alive neighbors becomes Inactive
            Rule2DSubrule::new(
                alive,
                alive,
                4,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                inactive,
                None,
                None,
            ),
            // Survival: Alive stays Alive (>=2 Alive neighbors), after overpopulation check
            Rule2DSubrule::new(
                alive,
                alive,
                2,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                alive,
                None,
                None,
            ),
            // Prevent birth unless exactly 3 Alive neighbors
            Rule2DSubrule::new(
                inactive,
                alive,
                4,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                inactive,
                None,
                None,
            ),
            // Birth (exactly 3)
            Rule2DSubrule::new(
                inactive,
                alive,
                3,
                CountOp::Eq,
                1,
                Neighborhood2D::Moore,
                alive,
                None,
                None,
            ),
        ],
    };
    let mut init = vec![CellType::inactive(); width * height];
    // seed a blinker in the middle-ish
    if width >= 3 && height >= 1 {
        let y = height / 2;
        let x = width / 2;
        let set = |x: usize, y: usize, v: &mut Vec<CellType>| v[y * width + x] = alive;
        if x > 0 {
            set(x - 1, y, &mut init);
        }
        set(x, y, &mut init);
        if x + 1 < width {
            set(x + 1, y, &mut init);
        }
    }
    Grid2D::new(width, height, history, init, rule)
}

/// Build a 2D three-state cycling automaton (A→B→C→A).
pub fn build_2d_three_state_cycle(width: usize, height: usize, history: usize) -> Grid2D {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let c = CellType::from("C");
    // Rotate when condition always passes (count >= 0). Order matters: transition before survival.
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
    let mut init = vec![CellType::inactive(); width * height];
    for y in 0..height {
        for x in 0..width {
            let idx = (x + y) % 3;
            init[y * width + x] = match idx {
                0 => a,
                1 => b,
                _ => c,
            };
        }
    }
    Grid2D::new(width, height, history, init, rule)
}

/// Print a 2D grid to stdout using symbol mapping.
fn print_grid_2d(g: &Grid2D) {
    // Build stable mapping for active states (non-Inactive)
    let mut names: Vec<String> = (0..g.width * g.height)
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
    for y in 0..g.height {
        let mut line = String::with_capacity(g.width);
        for x in 0..g.width {
            let i = y * g.width + x;
            let ty = &g.cell_type(i).as_str().to_string();
            if ty == INACTIVE {
                line.push('.');
            } else {
                line.push(*map.get(ty).unwrap_or(&'?'));
            }
        }
        println!("{}", line);
    }
}

/// Interactive CLI demo: approximate Conway’s Game of Life.
pub fn demo_life() {
    let alive = CellType::from("Alive");
    let inactive = CellType::inactive();
    let rule = Rule2D {
        subrules: vec![
            // Overpopulation
            Rule2DSubrule::new(
                alive,
                alive,
                4,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                inactive,
                None,
                None,
            ),
            // Survival
            Rule2DSubrule::new(
                alive,
                alive,
                2,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                alive,
                None,
                None,
            ),
            // Birth (exactly 3)
            Rule2DSubrule::new(
                inactive,
                alive,
                3,
                CountOp::Eq,
                1,
                Neighborhood2D::Moore,
                alive,
                None,
                None,
            ),
        ],
    };

    let width = 20usize;
    let height = 10usize;
    let hist = 5usize;
    let mut init = vec![CellType::inactive(); width * height];
    // Seed a blinker pattern
    let set_alive = |x: usize, y: usize, v: &mut Vec<CellType>| {
        v[y * width + x] = alive;
    };
    set_alive(5, 5, &mut init);
    set_alive(6, 5, &mut init);
    set_alive(7, 5, &mut init);

    let mut grid = Grid2D::new(width, height, hist, init, rule);
    let initial_state = GridState::from_grid2d(&grid);
    let steps = ask_steps(5);
    println!("Initial state (step {}):", grid.step);
    print_grid_2d(&grid);
    for _ in 0..steps {
        grid.step();
        println!("\nAfter step {}:", grid.step);
        print_grid_2d(&grid);
    }
    use cella_lib::config::CellaConfig;
    let cfg = CellaConfig::save_2d(&initial_state, &grid, BTreeMap::new());
    let _ = cfg.to_file_pretty("snapshot.json");
}

/// Interactive CLI demo: 2D three-state cycle.
pub fn demo_2d_three_state_cycle() {
    let _a = CellType::from("A");
    let mut g = build_2d_three_state_cycle(24, 12, 3);
    let steps = ask_steps(8);
    println!("Initial:");
    print_grid_2d(&g);
    for _ in 0..steps {
        g.step();
        println!("\nstep {}:", g.step);
        print_grid_2d(&g);
    }
}

/// Interactive CLI demo: load and run a JSON configuration file.
///
/// A file saved mid-run (its `snapshot.step > 0`) offers to resume there
/// instead of starting over from `initial`.
pub fn demo_from_config() {
    use cella_lib::config::CellaConfig;
    print!("Enter path to JSON config: ");
    let _ = io::stdout().flush();
    let path = read_line_trim();
    match CellaConfig::from_file(&path) {
        Ok(cfg) => {
            let resume = match cfg.snapshot() {
                Some(snap) if snap.step > 0 => {
                    print!("Resume at step {}? [Y/n] ", snap.step);
                    let _ = io::stdout().flush();
                    !read_line_trim().eq_ignore_ascii_case("n")
                }
                _ => false,
            };
            match cfg {
                CellaConfig::D1(_) => {
                    let built = resume
                        .then(|| cfg.build_grid1d_resumed())
                        .flatten()
                        .or_else(|| cfg.build_grid1d());
                    if let Some(mut g) = built {
                        let steps = ask_steps(10);
                        for _ in 0..steps {
                            g.step();
                        }
                        // Build symbol map for final state
                        let mut names: Vec<String> = (0..g.width)
                            .map(|i| g.cell_type(i).as_str().to_string())
                            .filter(|n| n != INACTIVE)
                            .collect();
                        names.sort();
                        names.dedup();
                        let symbol_pool: Vec<char> =
                            "!@#$%^&*()".chars().chain('a'..='z').collect();
                        let mut map: BTreeMap<String, char> = BTreeMap::new();
                        for (i, n) in names.iter().enumerate() {
                            map.insert(n.clone(), symbol_pool.get(i).copied().unwrap_or('?'));
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
                        println!("(1D) final after {} steps:", steps);
                        println!("{}", line);
                    } else {
                        println!("Invalid 1D config lengths.");
                    }
                }
                CellaConfig::D2(_) => {
                    let built = resume
                        .then(|| cfg.build_grid2d_resumed())
                        .flatten()
                        .or_else(|| cfg.build_grid2d());
                    if let Some(mut g) = built {
                        let _active = (0..g.width * g.height)
                            .map(|i| g.cell_type(i))
                            .find(|t| *t != CellType::inactive())
                            .unwrap_or(CellType::from("Alive"));
                        // The Reset target, regardless of where this run started:
                        // the config's own `initial`, not wherever `g` is now.
                        let initial_state = cfg.build_grid2d().map(|ig| GridState::from_grid2d(&ig));
                        let steps = ask_steps(10);
                        print_grid_2d(&g);
                        for _ in 0..steps {
                            g.step();
                            println!("\nstep {}:", g.step);
                            print_grid_2d(&g);
                        }
                        if let Some(initial_state) = initial_state {
                            let out = CellaConfig::save_2d(&initial_state, &g, cfg.colors().clone());
                            let _ = out.to_file_pretty("snapshot.json");
                        }
                    } else {
                        println!("Invalid 2D config lengths.");
                    }
                }
            }
        }
        Err(e) => println!("Failed to load config: {}", e),
    }
}

/// Build a 2D StraightLine-neighbourhood demo grid with a cross seed.
pub fn build_2d_straightline(width: usize, height: usize, history: usize) -> Grid2D {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let rule = Rule2D {
        subrules: vec![
            // Any A with at least 1 B in straight cardinal directions within range 2 becomes B
            Rule2DSubrule::new(
                a,
                b,
                1,
                CountOp::Gt,
                2,
                Neighborhood2D::StraightLine,
                b,
                None,
                None,
            ),
            // Persistence: B stays B with at least 1 B straight neighbor (range 1)
            Rule2DSubrule::new(
                b,
                b,
                1,
                CountOp::Gt,
                1,
                Neighborhood2D::StraightLine,
                b,
                None,
                None,
            ),
        ],
    };
    let mut init = vec![a; width * height];
    // Seed a small cross of B near the center
    if width > 2 && height > 2 {
        let cx = width / 2;
        let cy = height / 2;
        let mut set = |x: usize, y: usize| init[y * width + x] = b;
        set(cx, cy);
        if cx > 0 {
            set(cx - 1, cy);
        }
        if cx + 1 < width {
            set(cx + 1, cy);
        }
        if cy > 0 {
            set(cx, cy - 1);
        }
        if cy + 1 < height {
            set(cx, cy + 1);
        }
    }
    Grid2D::new(width, height, history, init, rule)
}

/// Interactive CLI demo: StraightLine neighbourhood growth.
pub fn demo_2d_straightline() {
    let mut g = build_2d_straightline(24, 12, 3);
    let steps = ask_steps(8);
    println!("Initial:");
    print_grid_2d(&g);
    for _ in 0..steps {
        g.step();
        println!("\nstep {}:", g.step);
        print_grid_2d(&g);
    }
}
