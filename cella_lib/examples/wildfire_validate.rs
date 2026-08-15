//! Validation harness: run the wildfire model against an observed fire and
//! score the simulated burned area per day.
//!
//! Inputs come from `validation/scripts/convert_pytorchfire.py`, which turns
//! the PyTorchFire six-fire HDF5 pack into per-fire directories:
//!
//! - `config.json` — a normal cella config (grid + wildfire model), ignition
//!   cells prefilled from the first observed day.
//! - `truth.json`  — observed cumulative burned masks, one per day, as rows
//!   of '0'/'1' characters.
//! - `meta.json`   — per-day uniform wind (speed + direction) and the number
//!   of simulation steps that make up one day.
//!
//! For each ensemble seed the grid is rebuilt, the seed and each day's wind
//! are set through the model, the model advances `steps_per_day` ticks per
//! day, and the simulated burned set (Burning + BurnedOut cells) is scored
//! against that day's observed mask with the two standard burned-area
//! overlap metrics:
//!
//! - IoU (Jaccard):   |A n B| / |A u B|
//! - Sørensen (dice): 2|A n B| / (|A| + |B|)
//!
//! Usage:
//!   cargo run --release -p cella_lib --example wildfire_validate -- \
//!       validation/data/converted/Bear_2020 [seeds] [out.json]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use cella_lib::config::CellaConfig;
use cella_lib::{CellType, Grid2D, WildfireModel};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Meta {
    fire: String,
    width: usize,
    height: usize,
    steps_per_day: usize,
    wind: Vec<WindDay>,
}

#[derive(Deserialize)]
struct WindDay {
    /// Present in meta.json for humans; the harness aligns by index.
    #[allow(dead_code)]
    date: String,
    speed: f64,
    dir_deg: f64,
}

#[derive(Deserialize)]
struct Truth {
    dates: Vec<String>,
    masks: HashMap<String, Vec<String>>,
}

#[derive(Serialize)]
struct DayScore {
    date: String,
    mean_iou: f64,
    mean_sorensen: f64,
    mean_sim_burned: f64,
    obs_burned: u64,
}

#[derive(Serialize)]
struct Report {
    fire: String,
    seeds: u64,
    steps_per_day: usize,
    days: Vec<DayScore>,
    final_mean_iou: f64,
}

fn load<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

/// Observed mask rows -> flat bool vec.
fn mask_bits(rows: &[String], width: usize) -> Vec<bool> {
    let mut out = Vec::with_capacity(rows.len() * width);
    for row in rows {
        assert_eq!(row.len(), width, "mask row width mismatch");
        out.extend(row.bytes().map(|b| b == b'1'));
    }
    out
}

fn scores(sim: &[bool], obs: &[bool]) -> (f64, f64, u64) {
    let (mut inter, mut a, mut b) = (0u64, 0u64, 0u64);
    for (&s, &o) in sim.iter().zip(obs) {
        a += s as u64;
        b += o as u64;
        inter += (s && o) as u64;
    }
    let union = a + b - inter;
    let iou = if union == 0 { 1.0 } else { inter as f64 / union as f64 };
    let sorensen = if a + b == 0 { 1.0 } else { 2.0 * inter as f64 / (a + b) as f64 };
    (iou, sorensen, a)
}

fn run_seed(cfg: &CellaConfig, meta: &Meta, seed: u64) -> Vec<Vec<bool>> {
    let mut grid: Grid2D = cfg.build_grid2d().expect("config builds");
    let burning = CellType::new("Burning");
    let burned = CellType::new("BurnedOut");
    let total = meta.width * meta.height;
    let mut daily = Vec::with_capacity(meta.wind.len());

    // Day 0 is the ignition state itself: score it before any stepping so the
    // alignment between simulated and observed days is explicit.
    let mask = |g: &Grid2D| -> Vec<bool> {
        (0..total).map(|i| { let t = g.cell_type(i); t == burning || t == burned }).collect()
    };
    {
        let m = grid.model_mut().unwrap().as_any_mut().downcast_mut::<WildfireModel>().unwrap();
        m.params.seed = seed;
    }
    daily.push(mask(&grid));

    for day in meta.wind.iter().skip(1) {
        {
            let m = grid.model_mut().unwrap().as_any_mut().downcast_mut::<WildfireModel>().unwrap();
            m.params.wind_speed = day.speed;
            m.params.wind_dir_deg = day.dir_deg;
        }
        for _ in 0..meta.steps_per_day {
            grid.step();
        }
        daily.push(mask(&grid));
    }
    daily
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(args.get(1).map(String::as_str).unwrap_or("validation/data/converted/Bear_2020"));
    let seeds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
    let out_path = args.get(3).map(PathBuf::from);

    let cfg: CellaConfig = load(&dir.join("config.json"));
    let meta: Meta = load(&dir.join("meta.json"));
    let truth: Truth = load(&dir.join("truth.json"));
    assert_eq!(truth.dates.len(), meta.wind.len(), "truth days must match wind days");

    let obs: Vec<Vec<bool>> = truth.dates.iter()
        .map(|d| mask_bits(&truth.masks[d], meta.width))
        .collect();

    eprintln!("{}: {}x{}, {} days, {} steps/day, {seeds} seeds",
        meta.fire, meta.width, meta.height, truth.dates.len(), meta.steps_per_day);

    // ensemble[seed][day] -> (iou, sorensen, burned)
    let mut per_day = vec![(0.0f64, 0.0f64, 0.0f64); truth.dates.len()];
    for seed in 0..seeds {
        let daily = run_seed(&cfg, &meta, seed);
        for (day, sim) in daily.iter().enumerate() {
            let (iou, sor, burned) = scores(sim, &obs[day]);
            per_day[day].0 += iou;
            per_day[day].1 += sor;
            per_day[day].2 += burned as f64;
        }
        eprintln!("  seed {seed} done");
    }

    let n = seeds as f64;
    let days: Vec<DayScore> = truth.dates.iter().enumerate()
        .map(|(i, date)| DayScore {
            date: date.clone(),
            mean_iou: per_day[i].0 / n,
            mean_sorensen: per_day[i].1 / n,
            mean_sim_burned: per_day[i].2 / n,
            obs_burned: obs[i].iter().map(|&b| b as u64).sum(),
        })
        .collect();

    println!("{:<12} {:>8} {:>10} {:>12} {:>12}", "date", "IoU", "Sorensen", "sim burned", "obs burned");
    for d in &days {
        println!("{:<12} {:>8.3} {:>10.3} {:>12.0} {:>12}", d.date, d.mean_iou, d.mean_sorensen, d.mean_sim_burned, d.obs_burned);
    }
    let final_iou = days.last().map(|d| d.mean_iou).unwrap_or(0.0);
    println!("final mean IoU: {final_iou:.3}");

    let report = Report {
        fire: meta.fire.clone(),
        seeds,
        steps_per_day: meta.steps_per_day,
        days,
        final_mean_iou: final_iou,
    };
    let out = out_path.unwrap_or_else(|| {
        PathBuf::from("validation/results").join(format!("{}.json", meta.fire))
    });
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    eprintln!("report written to {}", out.display());
}
