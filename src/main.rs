//! Cella binary crate — interactive CLI menu and egui-based GUI for cellular
//! automata simulation.
//!
//! # Usage
//!
//! ```bash
//! # Launch the CLI demo menu
//! cargo run --release
//!
//! # Launch the GUI
//! cargo run --release -- --gui
//!
//! # GUI with custom window size
//! cargo run --release -- --gui --size=1280x720
//! ```
//!
//! Without `--gui` the binary presents a numbered demo menu in the terminal.
//! With `--gui` it opens an egui window for visual simulation, rule editing,
//! and GIF export.

use std::path::PathBuf;
mod demos;
mod gui;
use std::io::{self, Write};

/// Display the interactive CLI demo menu and dispatch user selections.
fn menu() {
    loop {
        println!("\nCella demos (CLI):");
        println!("1) 2D Game of Life (approx)");
        println!("2) 1D Wolfram Rule 30 (n=1)");
        println!("3) 1D Wolfram n=2 demo");
        println!("4) 1D Custom (Wolfram code + n)");
        println!("5) Load from configuration file (JSON)");
        println!("6) Langton's ant (placeholder)");
        println!("7) 1D three-state cycle demo");
        println!("8) 2D three-state cycle demo");
        println!("9) 2D StraightLine neighborhood demo");
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
            "6" => println!(
                "Langton's ant not yet implemented in this engine (requires moving agent)."
            ),
            "7" => demos::demo_1d_three_state_cycle(),
            "8" => demos::demo_2d_three_state_cycle(),
            "9" => demos::demo_2d_straightline(),
            "0" => {
                println!("Bye!");
                break;
            }
            _ => println!("Unknown option."),
        }
    }
}

/// Entry point: dispatches to GUI (`--gui`) or CLI menu.
fn main() {
    // Route library log lines (e.g. `rfd` explaining why no file dialog could
    // open) to stderr. `RUST_LOG=debug` shows more; the default shows warnings
    // and errors only.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    // Choose GUI or CLI via args: pass --gui to launch GUI
    let args: Vec<String> = std::env::args().collect();
    if args
        .iter()
        .any(|a| a == "--gui" || a.eq_ignore_ascii_case("gui"))
    {
        // Parse optional GUI size arguments
        let size = parse_gui_size(&args);
        let config = parse_gui_config(&args);
        if let Err(e) = gui::run_gui(size, config) {
            eprintln!("GUI error: {}", e);
        }
        return;
    }
    menu();
}

/// Parse an optional `--config=PATH` / `--config PATH`: a config file to open
/// at startup instead of the Life demo. Useful where no file dialog can open.
fn parse_gui_config(args: &[String]) -> Option<PathBuf> {
    let mut i = 0usize;
    while i < args.len() {
        if let Some(rest) = args[i].strip_prefix("--config=") {
            if !rest.is_empty() {
                return Some(PathBuf::from(rest));
            }
        } else if args[i] == "--config" {
            return args.get(i + 1).map(PathBuf::from);
        }
        i += 1;
    }
    None
}

/// Parse optional GUI window size from command-line arguments.
///
/// Supports `--size=WIDTHxHEIGHT`, `--width=W`, `--height=H`, or
/// combinations thereof. When only one dimension is given the other
/// is inferred with a 16∶9 aspect ratio.
fn parse_gui_size(args: &[String]) -> Option<(f32, f32)> {
    // Supports:
    //   --size=WIDTHxHEIGHT or --size WIDTHxHEIGHT
    //   --width=WIDTH [--height=HEIGHT]
    //   --height=HEIGHT [--width=WIDTH]
    let mut width: Option<f32> = None;
    let mut height: Option<f32> = None;

    let mut i = 0usize;
    while i < args.len() {
        let arg = &args[i];
        let next = args.get(i + 1);
        if let Some(rest) = arg.strip_prefix("--size=") {
            if let Some((w, h)) = parse_wh(rest) {
                width = Some(w);
                height = Some(h);
            }
        } else if arg == "--size" {
            if let Some((w, h)) = next.and_then(|s| parse_wh(s)) {
                width = Some(w);
                height = Some(h);
                i += 1;
            }
        } else if let Some(rest) = arg.strip_prefix("--width=") {
            if let Ok(w) = rest.parse::<f32>() {
                width = Some(w);
            }
        } else if arg == "--width" {
            if let Some(Ok(w)) = next.map(|s| s.parse::<f32>()) {
                width = Some(w);
                i += 1;
            }
        } else if let Some(rest) = arg.strip_prefix("--height=") {
            if let Ok(h) = rest.parse::<f32>() {
                height = Some(h);
            }
        } else if arg == "--height"
            && let Some(Ok(h)) = next.map(|s| s.parse::<f32>())
        {
            height = Some(h);
            i += 1;
        }
        i += 1;
    }

    match (width, height) {
        (Some(w), Some(h)) if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 => Some((w, h)),
        (Some(w), None) if w.is_finite() && w > 0.0 => {
            let h = w * (9.0 / 16.0);
            Some((w, h))
        }
        (None, Some(h)) if h.is_finite() && h > 0.0 => {
            let w = h * (16.0 / 9.0);
            Some((w, h))
        }
        _ => None,
    }
}

/// Parse a `"WIDTHxHEIGHT"` or `"WIDTH,HEIGHT"` string into a float pair.
fn parse_wh(s: &str) -> Option<(f32, f32)> {
    // Accept formats: 1280x720, 1280X720, 1280,720
    let s = s.trim();
    let s = s.replace('X', "x");
    let parts: Vec<&str> = if s.contains('x') {
        s.split('x').collect()
    } else if s.contains(',') {
        s.split(',').collect()
    } else {
        return None;
    };
    if parts.len() != 2 {
        return None;
    }
    let w = parts[0].trim().parse::<f32>().ok()?;
    let h = parts[1].trim().parse::<f32>().ok()?;
    if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 {
        Some((w, h))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_gui_config_reads_the_equals_form() {
        let got = parse_gui_config(&args(&["cella", "--gui", "--config=configs/life.json"]));
        assert_eq!(got, Some(PathBuf::from("configs/life.json")));
    }

    #[test]
    fn parse_gui_config_reads_the_space_form() {
        let got = parse_gui_config(&args(&["cella", "--config", "configs/life.json", "--gui"]));
        assert_eq!(got, Some(PathBuf::from("configs/life.json")));
    }

    #[test]
    fn parse_gui_config_is_none_when_absent_or_dangling() {
        assert_eq!(parse_gui_config(&args(&["cella", "--gui"])), None);
        assert_eq!(
            parse_gui_config(&args(&["cella", "--gui", "--config"])),
            None
        );
    }
}
