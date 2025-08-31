mod demos;
use crate::demos::*;
use std::io::{self, Write};

fn menu() {
    loop {
        println!("\nCella demos:");
        println!("1) 2D Game of Life (approx)");
        println!("2) 1D Wolfram Rule 30 (n=1)");
        println!("3) 1D Wolfram n=2 demo");
        println!("4) 1D Custom (Wolfram code + n)");
        println!("5) Load from configuration file (JSON)");
        println!("6) Langton's ant (placeholder)");
        println!("0) Exit");
        print!("Select an option: ");
        let _ = io::stdout().flush();
        let choice = demos::read_line_trim();
        match choice.as_str() {
            "1" => demos::demo_life(),
            "2" => demos::demo_1d_rule30(),
            "3" => demos::demo_1d_n2(),
            "4" => demos::demo_1d_custom(),
            "5" => demos::demo_from_config(),
            "6" => println!("Langton's ant not yet implemented in this engine (requires moving agent)."),
            "0" => { println!("Bye!"); break; }
            _ => println!("Unknown option."),
        }
    }
}

fn main() {
    menu();
}
