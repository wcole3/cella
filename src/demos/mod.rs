pub mod one_d;
pub mod two_d;

use std::io::{self, Write};

pub fn read_line_trim() -> String {
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
    input.trim().to_string()
}

pub fn ask_steps(default_steps: usize) -> usize {
    print!("Enter number of steps to run [{}]: ", default_steps);
    let _ = io::stdout().flush();
    let s = read_line_trim();
    s.parse::<usize>().unwrap_or(default_steps)
}

pub use one_d::{demo_1d_rule30, demo_1d_n2, demo_1d_custom};
pub use two_d::demo_life;
pub use two_d::demo_from_config;

// Builders for reuse (CLI + GUI)
pub use one_d::{build_1d_rule30, build_1d_code_n};
pub use two_d::build_2d_life;
