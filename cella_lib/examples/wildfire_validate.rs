//! Validation harness: run the wildfire model against an observed fire and
//! score it honestly. See validation/TEST_PLAN.md for the methodology and
//! validation/FORMATS.md for the canonical scenario layout this reads.
//!
//! For each ensemble seed the grid is rebuilt from `config.json`, the seed
//! and the scenario's wind schedule are applied, and the simulation advances
//! between the truth's observation times. Scores per observation time:
//!
//! - IoU (Jaccard) and Sørensen of the burned sets — the field standard.
//! - Arrival-time MAE over cells burned in both (plus miss / false rates),
//!   quantized to observation times exactly like the truth is.
//!
//! Every run also scores two null baselines on the same truth, because a
//! model is only interesting where it beats them:
//!
//! - **persistence**: the ignition set never grows. Any model worse than
//!   this is actively harmful.
//! - **area-matched radial**: a disc grown from the ignition cells (chamfer
//!   distance) whose AREA matches the observed area at every observation
//!   time. This null has perfect area calibration by construction, so any
//!   IoU the model gains over it is genuine *spatial* skill, not area
//!   tuning.
//!
//! Usage (run from cella_lib/, its own build root):
//!   cargo run --release --example wildfire_validate -- \
//!       ../validation/data/scenarios/Bear_2020 [seeds] [out.json] [fields.json]
//!
//! The optional 4th argument writes a second JSON with the full per-cell
//! arrival grids (seed-0 simulation + the radial null). The figure script
//! (validation/scripts/make_figures.py) reads that file to draw the
//! model-vs-observed maps; it is heavy (one number per cell), so it is only
//! written when asked for.

use std::path::{Path, PathBuf};

use cella_lib::config::CellaConfig;
use cella_lib::{CellType, Grid2D, WildfireModel};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Scenario {
    format_version: u32,
    id: String,
    grid: GridMeta,
    wind: Vec<WindEntry>,
    steps_per_hour: f64,
}

#[derive(Deserialize)]
struct GridMeta {
    width: usize,
    height: usize,
}

#[derive(Deserialize)]
struct WindEntry {
    hours: f64,
    speed_ms: f64,
    /// Bearing the wind comes from, degrees clockwise from north.
    from_deg: f64,
}

#[derive(Deserialize)]
struct Truth {
    format_version: u32,
    observed_at: Vec<f64>,
    arrival_hours: Vec<f64>,
    spatial_accuracy_m: f64,
    accuracy_note: String,
}

#[derive(Serialize)]
struct TimeScore {
    hours: f64,
    iou: f64,
    sorensen: f64,
    sim_burned: f64,
    obs_burned: u64,
    /// Fraction of observed-burned cells the simulation missed.
    miss_rate: f64,
    /// Fraction of simulated-burned cells not observed burned.
    false_rate: f64,
}

/// Per-cell arrival grids for figure drawing (written only when the 4th CLI
/// argument asks for them). Hours since t0; -1.0 = never burned. The truth's
/// own arrival grid already lives in truth.json, so it is not repeated here.
#[derive(Serialize)]
struct Fields {
    scenario: String,
    width: usize,
    height: usize,
    /// Seed-0 ensemble member (deterministic, so reproducible).
    sim_arrival_seed0: Vec<f64>,
    /// The area-matched radial null as an arrival grid: the observation time
    /// at which the growing disc first covers each cell.
    radial_arrival: Vec<f64>,
}

#[derive(Serialize)]
struct Report {
    scenario: String,
    seeds: u64,
    truth_spatial_accuracy_m: f64,
    truth_accuracy_note: String,
    /// Ensemble means per observation time.
    model: Vec<TimeScore>,
    /// Arrival-time MAE (hours) over cells burned in both model and truth,
    /// ensemble mean; quantization = observation cadence.
    model_arrival_mae_hours: f64,
    /// Null baselines on identical truth.
    persistence: Vec<TimeScore>,
    radial: Vec<TimeScore>,
    final_iou_model: f64,
    final_iou_persistence: f64,
    final_iou_radial: f64,
}

fn load<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

/// Burned mask at time `t` from an arrival field (-1 = never).
fn mask_at(arrival: &[f64], t: f64) -> Vec<bool> {
    arrival.iter().map(|&a| a >= 0.0 && a <= t).collect()
}

fn overlap_scores(sim: &[bool], obs: &[bool]) -> TimeScore {
    let (mut inter, mut a, mut b) = (0u64, 0u64, 0u64);
    for (&s, &o) in sim.iter().zip(obs) {
        a += s as u64;
        b += o as u64;
        inter += (s && o) as u64;
    }
    let union = a + b - inter;
    TimeScore {
        hours: 0.0,
        iou: if union == 0 {
            1.0
        } else {
            inter as f64 / union as f64
        },
        sorensen: if a + b == 0 {
            1.0
        } else {
            2.0 * inter as f64 / (a + b) as f64
        },
        sim_burned: a as f64,
        obs_burned: b,
        miss_rate: if b == 0 {
            0.0
        } else {
            (b - inter) as f64 / b as f64
        },
        false_rate: if a == 0 {
            0.0
        } else {
            (a - inter) as f64 / a as f64
        },
    }
}

/// One ensemble member: returns the simulated arrival field, quantized to the
/// observation times exactly as the truth is.
fn run_seed(cfg: &CellaConfig, sc: &Scenario, seed: u64) -> Vec<f64> {
    let mut grid: Grid2D = cfg.build_grid2d().expect("config builds");
    let burning = CellType::new("Burning");
    let burned = CellType::new("BurnedOut");
    let total = sc.grid.width * sc.grid.height;
    let mut arrival = vec![-1.0f64; total];

    {
        let m = grid
            .model_mut()
            .unwrap()
            .as_any_mut()
            .downcast_mut::<WildfireModel>()
            .unwrap();
        m.params.seed = seed;
    }
    let record = |grid: &Grid2D, arrival: &mut [f64], t: f64| {
        for (i, slot) in arrival.iter_mut().enumerate() {
            if *slot < 0.0 {
                let ty = grid.cell_type(i);
                if ty == burning || ty == burned {
                    *slot = t;
                }
            }
        }
    };
    record(&grid, &mut arrival, 0.0);

    let mut steps_done = 0u64;
    for win in sc.wind.windows(2) {
        let (cur, next) = (&win[0], &win[1]);
        {
            let m = grid
                .model_mut()
                .unwrap()
                .as_any_mut()
                .downcast_mut::<WildfireModel>()
                .unwrap();
            m.params.wind_speed = cur.speed_ms;
            m.params.wind_from_deg = cur.from_deg;
        }
        // Integer step counts drift from real time; track cumulatively so the
        // total stays aligned with the schedule.
        let now_hours = next.hours;
        let target_steps = (now_hours * sc.steps_per_hour).round() as u64;
        for _ in steps_done..target_steps {
            grid.step();
        }
        steps_done = target_steps;
        record(&grid, &mut arrival, now_hours);
    }
    arrival
}

/// Chamfer distance (3-4 mask, two passes) from the ignition set — the basis
/// of the area-matched radial null.
fn chamfer_from(seed_mask: &[bool], w: usize, h: usize) -> Vec<u32> {
    const FAR: u32 = u32::MAX / 2;
    let mut d: Vec<u32> = seed_mask.iter().map(|&s| if s { 0 } else { FAR }).collect();
    let idx = |x: usize, y: usize| y * w + x;
    for y in 0..h {
        for x in 0..w {
            let mut best = d[idx(x, y)];
            if x > 0 {
                best = best.min(d[idx(x - 1, y)] + 3);
            }
            if y > 0 {
                best = best.min(d[idx(x, y - 1)] + 3);
            }
            if x > 0 && y > 0 {
                best = best.min(d[idx(x - 1, y - 1)] + 4);
            }
            if x + 1 < w && y > 0 {
                best = best.min(d[idx(x + 1, y - 1)] + 4);
            }
            d[idx(x, y)] = best;
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let mut best = d[idx(x, y)];
            if x + 1 < w {
                best = best.min(d[idx(x + 1, y)] + 3);
            }
            if y + 1 < h {
                best = best.min(d[idx(x, y + 1)] + 3);
            }
            if x + 1 < w && y + 1 < h {
                best = best.min(d[idx(x + 1, y + 1)] + 4);
            }
            if x > 0 && y + 1 < h {
                best = best.min(d[idx(x - 1, y + 1)] + 4);
            }
            d[idx(x, y)] = best;
        }
    }
    d
}

/// The area-matched radial null: grow a disc from the ignition set to match
/// the observed burned AREA at time `t`. Cells are taken in chamfer-distance
/// order (nearest first).
fn radial_mask(order: &[usize], area: usize, total: usize) -> Vec<bool> {
    let mut m = vec![false; total];
    for &i in order.iter().take(area) {
        m[i] = true;
    }
    m
}

fn arrival_mae(sim: &[f64], obs: &[f64]) -> f64 {
    let (mut sum, mut n) = (0.0f64, 0u64);
    for (&s, &o) in sim.iter().zip(obs) {
        if s >= 0.0 && o >= 0.0 {
            sum += (s - o).abs();
            n += 1;
        }
    }
    if n == 0 { f64::NAN } else { sum / n as f64 }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(
        args.get(1)
            .map(String::as_str)
            .unwrap_or("../validation/data/scenarios/Bear_2020"),
    );
    let seeds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
    let out_path = args.get(3).map(PathBuf::from);
    let fields_path = args.get(4).map(PathBuf::from);

    let sc: Scenario = load(&dir.join("scenario.json"));
    let truth: Truth = load(&dir.join("truth.json"));
    let cfg: CellaConfig = load(&dir.join("config.json"));
    assert_eq!(sc.format_version, 2, "unknown scenario format (v2 = wind from_deg)");
    assert_eq!(truth.format_version, 2, "unknown truth format");
    let total = sc.grid.width * sc.grid.height;
    assert_eq!(truth.arrival_hours.len(), total, "truth grid mismatch");

    eprintln!(
        "{}: {}x{}, {} observations over {:.0}h, {} seeds (truth accuracy ~{} m)",
        sc.id,
        sc.grid.width,
        sc.grid.height,
        truth.observed_at.len(),
        truth.observed_at.last().unwrap(),
        seeds,
        truth.spatial_accuracy_m
    );

    // Model ensemble.
    let mut acc: Vec<TimeScore> = truth
        .observed_at
        .iter()
        .map(|&t| TimeScore {
            hours: t,
            iou: 0.0,
            sorensen: 0.0,
            sim_burned: 0.0,
            obs_burned: 0,
            miss_rate: 0.0,
            false_rate: 0.0,
        })
        .collect();
    let mut mae_sum = 0.0f64;
    let mut sim_arrival_seed0: Vec<f64> = Vec::new();
    for seed in 0..seeds {
        let sim_arrival = run_seed(&cfg, &sc, seed);
        if seed == 0 {
            sim_arrival_seed0 = sim_arrival.clone();
        }
        for (slot, &t) in acc.iter_mut().zip(&truth.observed_at) {
            let s = overlap_scores(&mask_at(&sim_arrival, t), &mask_at(&truth.arrival_hours, t));
            slot.iou += s.iou;
            slot.sorensen += s.sorensen;
            slot.sim_burned += s.sim_burned;
            slot.obs_burned = s.obs_burned;
            slot.miss_rate += s.miss_rate;
            slot.false_rate += s.false_rate;
        }
        mae_sum += arrival_mae(&sim_arrival, &truth.arrival_hours);
        eprintln!("  seed {seed} done");
    }
    let n = seeds as f64;
    for s in &mut acc {
        s.iou /= n;
        s.sorensen /= n;
        s.sim_burned /= n;
        s.miss_rate /= n;
        s.false_rate /= n;
    }

    // Nulls share the ignition set (arrival == 0 cells in truth).
    let ignition = mask_at(&truth.arrival_hours, 0.0);
    let chamfer = chamfer_from(&ignition, sc.grid.width, sc.grid.height);
    let mut order: Vec<usize> = (0..total).collect();
    order.sort_by_key(|&i| chamfer[i]);
    let (persistence, radial): (Vec<TimeScore>, Vec<TimeScore>) = truth
        .observed_at
        .iter()
        .map(|&t| {
            let obs = mask_at(&truth.arrival_hours, t);
            let obs_area = obs.iter().filter(|&&b| b).count();
            let mut p = overlap_scores(&ignition, &obs);
            p.hours = t;
            let mut r = overlap_scores(&radial_mask(&order, obs_area, total), &obs);
            r.hours = t;
            (p, r)
        })
        .unzip();

    println!(
        "{:<8} {:>7} {:>7} {:>7}   {:>6} {:>6}   {:>10} {:>10}",
        "hours", "model", "persis", "radial", "miss", "false", "sim area", "obs area"
    );
    for (i, s) in acc.iter().enumerate() {
        println!(
            "{:<8.0} {:>7.3} {:>7.3} {:>7.3}   {:>6.2} {:>6.2}   {:>10.0} {:>10}",
            s.hours,
            s.iou,
            persistence[i].iou,
            radial[i].iou,
            s.miss_rate,
            s.false_rate,
            s.sim_burned,
            s.obs_burned
        );
    }
    let mae = mae_sum / n;
    println!("arrival MAE: {mae:.1} h (quantized to observation cadence)");
    println!(
        "final IoU — model {:.3} | persistence {:.3} | area-matched radial {:.3}",
        acc.last().unwrap().iou,
        persistence.last().unwrap().iou,
        radial.last().unwrap().iou
    );

    let report = Report {
        scenario: sc.id.clone(),
        seeds,
        truth_spatial_accuracy_m: truth.spatial_accuracy_m,
        truth_accuracy_note: truth.accuracy_note.clone(),
        final_iou_model: acc.last().unwrap().iou,
        final_iou_persistence: persistence.last().unwrap().iou,
        final_iou_radial: radial.last().unwrap().iou,
        model: acc,
        model_arrival_mae_hours: mae,
        persistence,
        radial,
    };
    let out = out_path
        .unwrap_or_else(|| PathBuf::from("../validation/results").join(format!("{}.json", sc.id)));
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    eprintln!("report written to {}", out.display());

    if let Some(fp) = fields_path {
        // Radial arrival: walk the observation times in order; every cell the
        // disc newly covers gets that time as its arrival.
        let mut radial_arrival = vec![-1.0f64; total];
        for &t in &truth.observed_at {
            let obs_area = mask_at(&truth.arrival_hours, t)
                .iter()
                .filter(|&&b| b)
                .count();
            for &i in order.iter().take(obs_area) {
                if radial_arrival[i] < 0.0 {
                    radial_arrival[i] = t;
                }
            }
        }
        let fields = Fields {
            scenario: sc.id.clone(),
            width: sc.grid.width,
            height: sc.grid.height,
            sim_arrival_seed0,
            radial_arrival,
        };
        if let Some(parent) = fp.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&fp, serde_json::to_string(&fields).unwrap()).unwrap();
        eprintln!("fields written to {}", fp.display());
    }
}
